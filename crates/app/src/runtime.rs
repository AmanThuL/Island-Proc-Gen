//! Application runtime — owns the window, GPU context, renderer, egui state,
//! and camera. Drives one full frame per `tick()` call.

use std::sync::Arc;
use std::time::Instant;

use anyhow::Context as _;
use tracing::{debug, info, warn};
use winit::{
    dpi::{LogicalSize, PhysicalSize},
    event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::ActiveEventLoop,
    window::{Window, WindowAttributes},
};

use gpu::GpuContext;
use island_core::{
    pipeline::SimulationPipeline,
    preset::IslandArchetypePreset,
    seed::Seed,
    world::{Resolution, WorldState},
};
use render::{
    OverlayRenderer, SkyRenderer, TerrainRenderer, ViewportTextureSet, WORLD_XZ_EXTENT,
    overlay::OverlayRegistry,
};
use sim::{StageId, default_pipeline, invalidate_from};

use crate::camera::{Camera, InputState};
use crate::dock::{DockLayout, TabKind};

// ── ViewMode ──────────────────────────────────────────────────────────────────

/// Controls which overlays are shown each frame.
///
/// Leaving `Continuous` (in either direction) snapshots the user's current
/// per-overlay visibility as the "baseline". `HexOverlay` renders the
/// baseline + `hex_aggregated` forced on; `HexOnly` renders only
/// `hex_aggregated` (every baseline entry hidden). Returning to
/// `Continuous` restores the baseline and clears the snapshot, so
/// `HexOverlay → HexOnly → Continuous` lands back on the original state
/// regardless of intermediate hops.
///
/// Invariant: `saved_visibility` is `Some` iff `view_mode != Continuous`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    /// User controls overlay visibility freely. Default state.
    Continuous,
    /// All user-enabled overlays are shown AND `hex_aggregated` is forced on.
    HexOverlay,
    /// Only `hex_aggregated` is shown; all other overlays are hidden.
    /// Prior visibility is saved and restored on exit.
    HexOnly,
}

impl ViewMode {
    /// Human-readable label for the egui ComboBox.
    pub fn label(self) -> &'static str {
        match self {
            ViewMode::Continuous => "Continuous",
            ViewMode::HexOverlay => "Hex overlay",
            ViewMode::HexOnly => "Hex only",
        }
    }
}

// ── Startup defaults (edit here to change initial window / camera state) ──────

/// Initial logical window dimensions on first open. The OS may shrink this to
/// fit the display.
pub(crate) const INITIAL_WINDOW_WIDTH: u32 = 1280;
pub(crate) const INITIAL_WINDOW_HEIGHT: u32 = 800;

/// Orbit camera defaults used at first open AND by the camera panel "Reset"
/// button. Target Y is overridden at runtime to `preset.sea_level`. The
/// angles/distance below capture the user-approved preview view: a ~13° yaw
/// / ~13° pitch / 1.44-distance orbit that frames the island with the
/// volcano peak prominent, the coastline visible, and the camera safely
/// above `sea_level` (negative pitch would put the eye below the water).
pub(crate) const INITIAL_CAMERA_DISTANCE: f32 = 1.44;
pub(crate) const INITIAL_CAMERA_YAW: f32 = 0.23;
pub(crate) const INITIAL_CAMERA_PITCH: f32 = 0.22;

// ── AppTabViewer ──────────────────────────────────────────────────────────────

/// `egui_dock::TabViewer` implementation for the main window.
///
/// Holds short-lived borrows to all state the tab bodies need for one frame.
/// Constructed inside `tick()` immediately before `DockArea::show_inside` and
/// dropped right after.
struct AppTabViewer<'a> {
    viewport_tex_id: egui::TextureId,
    overlay_registry: &'a mut OverlayRegistry,
    camera: &'a mut Camera,
    island_radius: f32,
    view_mode: ViewMode,
    new_view_mode: &'a mut Option<ViewMode>,
    preset: &'a mut IslandArchetypePreset,
    params_result: &'a mut ui::ParamsPanelResult,
    stats_data: &'a ui::StatsPanelData,
}

impl egui_dock::TabViewer for AppTabViewer<'_> {
    type Tab = TabKind;

    fn title(&mut self, tab: &mut TabKind) -> egui::WidgetText {
        tab.title().into()
    }

    fn is_closeable(&self, tab: &TabKind) -> bool {
        tab.closeable()
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut TabKind) {
        match tab {
            TabKind::Viewport => {
                let avail = ui.available_size();
                ui.add(egui::Image::new(egui::load::SizedTexture::new(
                    self.viewport_tex_id,
                    avail,
                )));
            }
            TabKind::Overlays => {
                ui::OverlayPanel::show(ui, self.overlay_registry);
            }
            TabKind::World => {
                ui.label("World panel — coming in 2.6.C");
            }
            TabKind::Camera => {
                if let Some(mode) = crate::camera_panel::CameraPanel::show(
                    ui,
                    self.camera,
                    self.island_radius,
                    self.view_mode,
                ) {
                    *self.new_view_mode = Some(mode);
                }
            }
            TabKind::Params => {
                *self.params_result = ui::ParamsPanel::show(ui, self.preset);
            }
            TabKind::Stats => {
                ui::StatsPanel::show(ui, self.stats_data);
            }
        }
    }
}

// ── Runtime ───────────────────────────────────────────────────────────────────

/// Holds all per-window application state.
pub struct Runtime {
    window: Arc<Window>,
    gpu: GpuContext,
    terrain: TerrainRenderer,
    overlay: OverlayRenderer,
    sky: SkyRenderer,

    // egui
    egui_ctx: egui::Context,
    egui_state: egui_winit::State,
    egui_renderer: egui_wgpu::Renderer,

    // offscreen viewport texture — 3D scene renders here, egui Image displays it
    viewport_tex: ViewportTextureSet,

    // camera
    camera: Camera,
    input: InputState,

    // timing / display
    last_frame: Instant,
    fps: f32,

    // preset loaded at startup
    preset: IslandArchetypePreset,

    // overlay registry (Task 0.8)
    overlay_registry: OverlayRegistry,

    // simulation metadata (Task 0.8)
    seed: Seed,
    resolution: Resolution,

    // Sprint 1B: the simulated world + the canonical linear pipeline.
    // The pipeline runs once at boot to fully populate `world`, and then
    // slider changes in `ParamsPanel` call `pipeline.run_from(world, X)`
    // to re-run just the affected stages.
    world: WorldState,
    pipeline: SimulationPipeline,

    // ViewMode three-view toggle. `saved_visibility` holds the Continuous
    // baseline while `view_mode != Continuous`; see [`ViewMode`] docs.
    view_mode: ViewMode,
    saved_visibility: Option<Vec<(&'static str, bool)>>,

    // egui_dock layout (B.2: in-memory; B.3 adds persistence).
    dock: DockLayout,
}

impl Runtime {
    /// Construct the runtime: create the window, initialise GPU, renderers,
    /// and egui.
    pub fn new(event_loop: &ActiveEventLoop) -> anyhow::Result<Self> {
        // ── Window ────────────────────────────────────────────────────────────
        let attrs = WindowAttributes::default()
            .with_title("Island Proc-Gen — Sprint 2")
            .with_inner_size(LogicalSize::new(
                INITIAL_WINDOW_WIDTH,
                INITIAL_WINDOW_HEIGHT,
            ));
        let window = Arc::new(event_loop.create_window(attrs).context("create_window")?);

        // ── GPU ───────────────────────────────────────────────────────────────
        let gpu = GpuContext::new(window.clone()).context("GpuContext::new")?;

        // ── egui ──────────────────────────────────────────────────────────────
        let egui_ctx = egui::Context::default();

        let egui_state = egui_winit::State::new(
            egui_ctx.clone(),
            egui::ViewportId::ROOT,
            &*window, // &dyn HasDisplayHandle
            Some(window.scale_factor() as f32),
            None, // theme
            Some(gpu.device.limits().max_texture_dimension_2d as usize),
        );

        let mut egui_renderer = egui_wgpu::Renderer::new(
            &gpu.device,
            gpu.surface_format,
            egui_wgpu::RendererOptions::default(),
        );

        // ── Viewport texture — offscreen render target for the 3D scene ──────
        // B.1: starts at window size. B.2 onwards: sized to the dock tab rect.
        let PhysicalSize {
            width: win_w,
            height: win_h,
        } = gpu.size;
        let viewport_tex = ViewportTextureSet::new(&gpu, (win_w, win_h), &mut egui_renderer);

        // ── Camera ────────────────────────────────────────────────────────────
        let PhysicalSize { width, height } = gpu.size;
        let aspect = width as f32 / height.max(1) as f32;
        let mut camera = Camera::new(aspect);

        // ── Preset ───────────────────────────────────────────────────────────
        let preset = load_preset();

        // ── Overlay / sim metadata ───────────────────────────────────────────
        let overlay_registry = OverlayRegistry::sprint_2_5_defaults();
        let seed = Seed(42);
        let resolution = Resolution::new(256, 256);

        // ── Canonical 19-stage pipeline (built once, reused for slider re-runs) ─
        // Uses `sim::default_pipeline()` — the single source of truth for stage
        // ordering. Sliders call `run_from(StageId::X as usize)` and rely on
        // the pipeline's push-order matching the enum discriminants. A local
        // copy of the builder here would silently drift when StageId changes.
        let pipeline = default_pipeline();
        let mut world = WorldState::new(seed, preset.clone(), resolution);
        pipeline.run(&mut world).context("initial pipeline run")?;
        let land_cells = world
            .derived
            .coast_mask
            .as_ref()
            .map(|c| c.land_cell_count)
            .unwrap_or(0);
        info!(
            stages = pipeline.len(),
            land_cells, "pipeline completed (all 11 invariants passed)"
        );

        // ── Terrain renderer (must follow pipeline so z_filled is populated) ─
        let terrain = TerrainRenderer::new(&gpu, &world, &preset);

        // ── Overlay renderer — shares terrain VBO/IBO/view_buf ────────────────
        let overlay = OverlayRenderer::new(
            &gpu,
            &world,
            &overlay_registry,
            terrain.view_buf(),
            terrain.terrain_vbo(),
            terrain.terrain_ibo(),
            terrain.terrain_index_count(),
        );

        // ── Sky renderer (depends only on gpu) ───────────────────────────────
        let sky = SkyRenderer::new(&gpu);

        // Centre the camera on the island mesh ([0, WORLD_XZ_EXTENT] on XZ, Y=height).
        camera.target = glam::Vec3::new(
            WORLD_XZ_EXTENT * 0.5,
            preset.sea_level,
            WORLD_XZ_EXTENT * 0.5,
        );
        camera.distance = INITIAL_CAMERA_DISTANCE * WORLD_XZ_EXTENT;
        camera.yaw = INITIAL_CAMERA_YAW;
        camera.pitch = INITIAL_CAMERA_PITCH;

        Ok(Self {
            window,
            gpu,
            terrain,
            overlay,
            sky,
            egui_ctx,
            egui_state,
            egui_renderer,
            viewport_tex,
            camera,
            input: InputState::default(),
            last_frame: Instant::now(),
            fps: 0.0,
            preset,
            overlay_registry,
            seed,
            resolution,
            world,
            pipeline,
            view_mode: ViewMode::Continuous,
            saved_visibility: None,
            dock: DockLayout::default_layout(),
        })
    }

    /// Request a repaint from the OS (called from `about_to_wait`).
    pub fn request_redraw(&self) {
        self.window.request_redraw();
    }

    /// Borrow the simulated world. Sprint 1B+ overlay bindings will consume
    /// this to look up `derived.*` fields by name.
    pub fn world(&self) -> &WorldState {
        &self.world
    }

    /// Transition to a new [`ViewMode`], applying visibility changes to
    /// `overlay_registry`. Idempotent on same-mode calls.
    ///
    /// `saved_visibility` holds the Continuous-mode baseline — the user's
    /// original per-overlay visibility. It is populated when leaving
    /// Continuous for the first time and cleared when returning to
    /// Continuous, so `HexOverlay ↔ HexOnly` transitions never lose the
    /// baseline. This matters when a user entered `HexOverlay` (which
    /// forces `hex_aggregated` on), then `HexOnly`, then back to
    /// `Continuous`: they should land on their original state, not on
    /// a state carrying `HexOverlay`'s side-effects.
    pub fn set_view_mode(&mut self, new_mode: ViewMode) {
        if new_mode == self.view_mode {
            return;
        }
        let was_continuous = self.view_mode == ViewMode::Continuous;
        let becoming_continuous = new_mode == ViewMode::Continuous;

        if was_continuous && !becoming_continuous {
            self.saved_visibility = Some(
                self.overlay_registry
                    .all()
                    .iter()
                    .map(|d| (d.id, d.visible))
                    .collect(),
            );
        }

        match new_mode {
            ViewMode::Continuous => {
                if let Some(saved) = self.saved_visibility.take() {
                    for (id, visible) in saved {
                        self.overlay_registry.set_visibility(id, visible);
                    }
                }
            }
            ViewMode::HexOverlay => {
                if let Some(saved) = self.saved_visibility.as_ref() {
                    for &(id, visible) in saved {
                        self.overlay_registry.set_visibility(id, visible);
                    }
                }
                self.overlay_registry.set_visibility("hex_aggregated", true);
            }
            ViewMode::HexOnly => {
                if let Some(saved) = self.saved_visibility.as_ref() {
                    for &(id, _) in saved {
                        self.overlay_registry.set_visibility(id, false);
                    }
                }
                self.overlay_registry.set_visibility("hex_aggregated", true);
            }
        }
        self.view_mode = new_mode;
    }

    /// Expose the current [`ViewMode`] for the UI.
    pub fn view_mode(&self) -> ViewMode {
        self.view_mode
    }

    /// Handle a `WindowEvent` from winit.
    pub fn handle_window_event(&mut self, event_loop: &ActiveEventLoop, event: WindowEvent) {
        // Forward to egui first; it will consume keyboard/pointer events it owns.
        let response = self.egui_state.on_window_event(&self.window, &event);

        if response.consumed {
            return; // egui handled it
        }

        match event {
            WindowEvent::CloseRequested => {
                debug!("Window close requested");
                event_loop.exit();
            }

            WindowEvent::Resized(new_size) => {
                self.gpu.resize(new_size);
                let aspect = new_size.width as f32 / new_size.height.max(1) as f32;
                self.camera.set_aspect(aspect);
                // B.1: viewport fills the window. In B.3 the aspect source will
                // move to the viewport tab rect, but for now they are equal.
                self.viewport_tex.resize(
                    &self.gpu,
                    (new_size.width, new_size.height),
                    &mut self.egui_renderer,
                );
            }

            WindowEvent::RedrawRequested => {
                self.tick();
            }

            // ── Camera input ──────────────────────────────────────────────────
            WindowEvent::MouseInput { state, button, .. } => match button {
                MouseButton::Left => {
                    self.input.left_pressed = state == ElementState::Pressed;
                    if state == ElementState::Released {
                        self.input.last_cursor = None;
                    }
                }
                MouseButton::Right => {
                    self.input.right_pressed = state == ElementState::Pressed;
                    if state == ElementState::Released {
                        self.input.last_cursor = None;
                    }
                }
                _ => {}
            },

            WindowEvent::CursorMoved { position, .. } => {
                let (cx, cy) = (position.x, position.y);
                if let Some((lx, ly)) = self.input.last_cursor {
                    let PhysicalSize { width, height } = self.gpu.size;
                    let dx = (cx - lx) as f32 / width as f32;
                    let dy = (cy - ly) as f32 / height as f32;

                    if self.input.right_pressed
                        || (self.input.left_pressed && self.input.shift_held)
                    {
                        self.camera.pan(dx, dy);
                    } else if self.input.left_pressed {
                        self.camera.orbit(dx, dy);
                    }
                }
                self.input.last_cursor = Some((cx, cy));
            }

            WindowEvent::MouseWheel { delta, .. } => {
                let scroll = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(pos) => pos.y as f32 * 0.01,
                };
                self.camera.zoom(scroll);
            }

            WindowEvent::ModifiersChanged(mods) => {
                self.input.shift_held = mods.state().shift_key();
            }

            _ => {}
        }
    }

    // ── Frame ─────────────────────────────────────────────────────────────────

    fn tick(&mut self) {
        // FPS (exponential moving average)
        let now = Instant::now();
        let dt = now.duration_since(self.last_frame).as_secs_f32();
        self.last_frame = now;
        self.fps = if self.fps == 0.0 {
            1.0 / dt.max(f32::EPSILON)
        } else {
            self.fps * 0.95 + (1.0 / dt.max(f32::EPSILON)) * 0.05
        };

        // ── Sim step (Sprint 0: no-op) ────────────────────────────────────────

        // ── Upload camera ─────────────────────────────────────────────────────
        let vp = self.camera.view_projection();
        let eye = self.camera.eye();
        self.terrain.update_view(&self.gpu.queue, vp, eye);

        // ── Acquire surface ───────────────────────────────────────────────────
        //
        // `surface_expect` panics with a descriptive message if we ever end
        // up here with a headless (surface-less) `GpuContext`. Runtime is
        // the interactive path by construction, so this assumption holds;
        // the panic would flag a programming error (e.g. a future refactor
        // that accidentally plumbs a headless ctx through the window event
        // loop) rather than a runtime concern.
        let surface = self.gpu.surface_expect();
        let frame = match surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f) => f,
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => return,
            wgpu::CurrentSurfaceTexture::Outdated
            | wgpu::CurrentSurfaceTexture::Suboptimal(_)
            | wgpu::CurrentSurfaceTexture::Lost => {
                surface.configure(&self.gpu.device, &self.gpu.config);
                return;
            }
            wgpu::CurrentSurfaceTexture::Validation => return,
        };

        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame_encoder"),
            });

        // ── Terrain pass — renders into the offscreen viewport texture ────────
        // B.1: colour target is viewport_tex.color_view() (not the window
        // surface). egui will composite the result via egui::Image in the
        // CentralPanel below. The clear colour is the fallback background
        // visible under the sky renderer when sky doesn't cover the full quad.
        {
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("terrain_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: self.viewport_tex.color_view(),
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.08,
                            g: 0.08,
                            b: 0.12,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: self.viewport_tex.depth_view(),
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            self.sky.draw(&mut rpass);
            self.terrain.draw(&mut rpass);
            self.overlay
                .draw(&mut rpass, &self.overlay_registry, &self.gpu.queue);
        }

        // ── egui pass ─────────────────────────────────────────────────────────
        // Extract values before the mutable borrows that follow.
        let fps = self.fps;
        let resolution = self.resolution;
        let seed = self.seed;
        let island_radius = self.preset.island_radius;
        let registry = &mut self.overlay_registry;
        let camera = &mut self.camera;
        let preset = &mut self.preset;
        let view_mode = self.view_mode;

        let raw_input = self.egui_state.take_egui_input(&self.window);

        // Use begin_pass / end_pass (the non-deprecated path in egui 0.34).
        self.egui_ctx.begin_pass(raw_input);

        // ── DockArea — six-tab dock layout replacing the old CentralPanel +
        //    four floating windows. TabViewer dispatches each tab variant to
        //    its panel body; the Viewport tab renders the offscreen texture.
        //
        // Extract local temporaries that the TabViewer needs but that would
        // otherwise conflict with the `&mut self.dock.state` borrow below.
        let viewport_tex_id = self.viewport_tex.egui_texture_id();
        let mut new_view_mode: Option<ViewMode> = None;
        let mut params_result = ui::ParamsPanelResult::default();
        let stats_data = ui::StatsPanelData {
            fps,
            resolution,
            seed,
        };

        // Reborrow self.dock.state as a local so the closure below doesn't
        // need to borrow self while self.egui_ctx is also borrowed by show().
        let dock_state = &mut self.dock.state;
        {
            #[allow(deprecated)]
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE)
                .show(&self.egui_ctx, |ui| {
                    let mut viewer = AppTabViewer {
                        viewport_tex_id,
                        overlay_registry: registry,
                        camera,
                        island_radius,
                        view_mode,
                        new_view_mode: &mut new_view_mode,
                        preset,
                        params_result: &mut params_result,
                        stats_data: &stats_data,
                    };
                    egui_dock::DockArea::new(dock_state).show_inside(ui, &mut viewer);
                });
        }

        let full_output = self.egui_ctx.end_pass();

        // Apply ViewMode transition if the user changed it in the panel.
        if let Some(mode) = new_view_mode {
            self.set_view_mode(mode);
        }

        // Slider re-run: when a Sprint 1B climate slider is touched,
        // update the world's preset copy and re-run the affected stage
        // plus its downstream neighbours via `run_from`, then re-bake
        // the overlay textures so visible overlays reflect the new
        // fields on the very next draw.
        if params_result.wind_dir_changed {
            self.world.preset = self.preset.clone();
            if let Err(err) = self
                .pipeline
                .run_from(&mut self.world, StageId::Precipitation as usize)
            {
                warn!("slider re-run failed: {err}");
            } else {
                self.overlay
                    .refresh(&self.gpu, &self.world, &self.overlay_registry);
            }
        }

        // Sprint 2.7: erosion slider re-run protocol (CLAUDE.md Gotchas):
        //   1. Sync world.preset from self.preset so stages read the new values.
        //   2. Invalidate caches from the ErosionOuterLoop frontier.
        //   3. Re-run from ErosionOuterLoop (includes CoastType + all 1B stages).
        if params_result.erosion_changed {
            self.world.preset = self.preset.clone();
            invalidate_from(&mut self.world, StageId::ErosionOuterLoop);
            if let Err(err) = self
                .pipeline
                .run_from(&mut self.world, StageId::ErosionOuterLoop as usize)
            {
                warn!("erosion slider re-run failed: {err}");
            } else {
                self.overlay
                    .refresh(&self.gpu, &self.world, &self.overlay_registry);
            }
        }

        // Handle egui platform output (cursor changes, clipboard, etc.)
        self.egui_state
            .handle_platform_output(&self.window, full_output.platform_output);

        let PhysicalSize { width, height } = self.gpu.size;
        let screen_descriptor = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [width, height],
            pixels_per_point: egui_winit::pixels_per_point(&self.egui_ctx, &self.window),
        };

        let paint_jobs = self
            .egui_ctx
            .tessellate(full_output.shapes, full_output.pixels_per_point);

        // Upload textures (fonts etc.) and update vertex/index buffers
        for (id, delta) in &full_output.textures_delta.set {
            self.egui_renderer
                .update_texture(&self.gpu.device, &self.gpu.queue, *id, delta);
        }
        let _extra_cmd_bufs = self.egui_renderer.update_buffers(
            &self.gpu.device,
            &self.gpu.queue,
            &mut encoder,
            &paint_jobs,
            &screen_descriptor,
        );

        {
            let mut rpass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("egui_pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations {
                            // B.1: terrain now renders into the offscreen
                            // viewport texture; the egui pass composites that
                            // image onto the window surface via the CentralPanel
                            // Image widget. Clear to dark slate so stale pixels
                            // are never visible at window edges or between frames.
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: 0.08,
                                g: 0.08,
                                b: 0.12,
                                a: 1.0,
                            }),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                })
                .forget_lifetime(); // egui_wgpu::Renderer::render takes &mut RenderPass<'static>

            self.egui_renderer
                .render(&mut rpass, &paint_jobs, &screen_descriptor);
        }

        // Free textures scheduled for removal
        for id in &full_output.textures_delta.free {
            self.egui_renderer.free_texture(id);
        }

        // ── Submit ────────────────────────────────────────────────────────────
        self.gpu.queue.submit(std::iter::once(encoder.finish()));
        self.window.pre_present_notify();
        frame.present();
    }
}

// ── Preset loading helper ─────────────────────────────────────────────────────

fn load_preset() -> IslandArchetypePreset {
    match data::presets::load_preset("volcanic_single") {
        Ok(p) => p,
        Err(e) => {
            warn!("Could not load preset: {e} — using inline fallback");
            island_core::preset::IslandArchetypePreset {
                name: "volcanic_single".to_string(),
                island_radius: 0.6,
                max_relief: 0.8,
                volcanic_center_count: 1,
                island_age: island_core::preset::IslandAge::Young,
                prevailing_wind_dir: 0.0,
                marine_moisture_strength: 0.7,
                sea_level: 0.3,
                erosion: island_core::preset::ErosionParams::default(),
            }
        }
    }
}

// ── ViewMode unit tests ───────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::ViewMode;
    use render::overlay::OverlayRegistry;

    /// Minimal stand-in for the runtime's view_mode state machine, using only
    /// `OverlayRegistry` + the two `ViewMode` fields. Mirrors `set_view_mode`
    /// without requiring a live GPU / window.
    struct FakeRuntime {
        registry: OverlayRegistry,
        view_mode: ViewMode,
        saved_visibility: Option<Vec<(&'static str, bool)>>,
    }

    impl FakeRuntime {
        fn new() -> Self {
            Self {
                registry: OverlayRegistry::sprint_2_5_defaults(),
                view_mode: ViewMode::Continuous,
                saved_visibility: None,
            }
        }

        /// Mirror of `Runtime::set_view_mode` — identical logic, no GPU deps.
        fn set_view_mode(&mut self, new_mode: ViewMode) {
            if new_mode == self.view_mode {
                return;
            }
            let was_continuous = self.view_mode == ViewMode::Continuous;
            let becoming_continuous = new_mode == ViewMode::Continuous;

            if was_continuous && !becoming_continuous {
                self.saved_visibility = Some(
                    self.registry
                        .all()
                        .iter()
                        .map(|d| (d.id, d.visible))
                        .collect(),
                );
            }

            match new_mode {
                ViewMode::Continuous => {
                    if let Some(saved) = self.saved_visibility.take() {
                        for (id, visible) in saved {
                            self.registry.set_visibility(id, visible);
                        }
                    }
                }
                ViewMode::HexOverlay => {
                    if let Some(saved) = self.saved_visibility.as_ref() {
                        for &(id, visible) in saved {
                            self.registry.set_visibility(id, visible);
                        }
                    }
                    self.registry.set_visibility("hex_aggregated", true);
                }
                ViewMode::HexOnly => {
                    if let Some(saved) = self.saved_visibility.as_ref() {
                        for &(id, _) in saved {
                            self.registry.set_visibility(id, false);
                        }
                    }
                    self.registry.set_visibility("hex_aggregated", true);
                }
            }
            self.view_mode = new_mode;
        }

        fn visibility_snapshot(&self) -> Vec<(&'static str, bool)> {
            self.registry
                .all()
                .iter()
                .map(|d| (d.id, d.visible))
                .collect()
        }
    }

    /// Any round-trip that ends at `Continuous` must leave the overlay
    /// visibility state bit-exact equal to the initial state. The snapshot
    /// is the Continuous baseline (the user's original visibility), so
    /// HexOverlay's side-effect of forcing `hex_aggregated` on is undone on
    /// return, regardless of how many HexOverlay/HexOnly hops occurred
    /// between.
    #[test]
    fn view_mode_toggle_sequence_is_idempotent() {
        // Pick an initial state where hex_aggregated is OFF so HexOverlay's
        // side-effect is observable if restoration fails.
        let mut rt = FakeRuntime::new();
        rt.registry.set_visibility("slope", true);
        rt.registry.set_visibility("hex_aggregated", false);
        let initial = rt.visibility_snapshot();

        // Case A: Continuous → HexOnly → Continuous
        rt.set_view_mode(ViewMode::HexOnly);
        rt.set_view_mode(ViewMode::Continuous);
        assert_eq!(
            initial,
            rt.visibility_snapshot(),
            "Continuous → HexOnly → Continuous must restore initial visibility"
        );

        // Case B: Continuous → HexOverlay → HexOnly → Continuous
        rt.set_view_mode(ViewMode::HexOverlay);
        rt.set_view_mode(ViewMode::HexOnly);
        rt.set_view_mode(ViewMode::Continuous);
        assert_eq!(
            initial,
            rt.visibility_snapshot(),
            "HexOverlay → HexOnly → Continuous must restore the pre-ViewMode baseline (hex_aggregated off)"
        );
    }

    /// Entering HexOnly must hide everything except `hex_aggregated`.
    #[test]
    fn hex_only_shows_only_hex_aggregated() {
        let mut rt = FakeRuntime::new();
        // Enable a few overlays so there is something to hide.
        rt.registry.set_visibility("slope", true);
        rt.registry.set_visibility("river_network", true);

        rt.set_view_mode(ViewMode::HexOnly);

        let visible_ids: Vec<&str> = rt
            .registry
            .all()
            .iter()
            .filter(|d| d.visible)
            .map(|d| d.id)
            .collect();
        assert_eq!(
            visible_ids,
            vec!["hex_aggregated"],
            "HexOnly must show only hex_aggregated, got: {visible_ids:?}"
        );
    }
}
