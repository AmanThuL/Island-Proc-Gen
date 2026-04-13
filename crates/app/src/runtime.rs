//! Application runtime — owns the window, GPU context, renderer, egui state,
//! and camera. Drives one full frame per `tick()` call.

use std::sync::Arc;
use std::time::Instant;

use anyhow::Context as _;
use tracing::{debug, warn};
use winit::{
    dpi::PhysicalSize,
    event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::ActiveEventLoop,
    window::{Window, WindowAttributes},
};

use gpu::GpuContext;
use island_core::{preset::IslandArchetypePreset, seed::Seed, world::Resolution};
use render::{overlay::OverlayRegistry, TerrainRenderer};

use crate::camera::{Camera, InputState};

// ── Runtime ───────────────────────────────────────────────────────────────────

/// Holds all per-window application state.
pub struct Runtime {
    window: Arc<Window>,
    gpu: GpuContext,
    terrain: TerrainRenderer,

    // egui
    egui_ctx: egui::Context,
    egui_state: egui_winit::State,
    egui_renderer: egui_wgpu::Renderer,

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
}

impl Runtime {
    /// Construct the runtime: create the window, initialise GPU, renderers,
    /// and egui.
    pub fn new(event_loop: &ActiveEventLoop) -> anyhow::Result<Self> {
        // ── Window ────────────────────────────────────────────────────────────
        let attrs = WindowAttributes::default()
            .with_title("Island Proc-Gen — Sprint 0");
        let window = Arc::new(
            event_loop.create_window(attrs).context("create_window")?,
        );

        // ── GPU ───────────────────────────────────────────────────────────────
        let gpu = GpuContext::new(window.clone()).context("GpuContext::new")?;

        // ── Terrain renderer ──────────────────────────────────────────────────
        let terrain = TerrainRenderer::new(&gpu);

        // ── egui ──────────────────────────────────────────────────────────────
        let egui_ctx = egui::Context::default();

        let egui_state = egui_winit::State::new(
            egui_ctx.clone(),
            egui::ViewportId::ROOT,
            &*window,                        // &dyn HasDisplayHandle
            Some(window.scale_factor() as f32),
            None,                            // theme
            Some(gpu.device.limits().max_texture_dimension_2d as usize),
        );

        let egui_renderer = egui_wgpu::Renderer::new(
            &gpu.device,
            gpu.surface_format,
            egui_wgpu::RendererOptions::default(),
        );

        // ── Camera ────────────────────────────────────────────────────────────
        let PhysicalSize { width, height } = gpu.size;
        let aspect = width as f32 / height.max(1) as f32;
        let camera = Camera::new(aspect);

        // ── Preset ───────────────────────────────────────────────────────────
        let preset = load_preset();

        // ── Overlay / sim metadata ───────────────────────────────────────────
        let overlay_registry = OverlayRegistry::sprint_0_defaults();
        let seed = Seed(42);
        let resolution = Resolution::new(256, 256);

        Ok(Self {
            window,
            gpu,
            terrain,
            egui_ctx,
            egui_state,
            egui_renderer,
            camera,
            input: InputState::default(),
            last_frame: Instant::now(),
            fps: 0.0,
            preset,
            overlay_registry,
            seed,
            resolution,
        })
    }

    /// Request a repaint from the OS (called from `about_to_wait`).
    pub fn request_redraw(&self) {
        self.window.request_redraw();
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
            }

            WindowEvent::RedrawRequested => {
                self.tick();
            }

            // ── Camera input ──────────────────────────────────────────────────
            WindowEvent::MouseInput { state, button, .. } => {
                match button {
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
                }
            }

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
        self.terrain.update_view_proj(&self.gpu.queue, vp);

        // ── Acquire surface ───────────────────────────────────────────────────
        let frame = match self.gpu.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f) => f,
            wgpu::CurrentSurfaceTexture::Timeout
            | wgpu::CurrentSurfaceTexture::Occluded => return,
            wgpu::CurrentSurfaceTexture::Outdated
            | wgpu::CurrentSurfaceTexture::Suboptimal(_) => {
                self.gpu.surface.configure(&self.gpu.device, &self.gpu.config);
                return;
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                self.gpu.surface.configure(&self.gpu.device, &self.gpu.config);
                return;
            }
            wgpu::CurrentSurfaceTexture::Validation => return,
        };

        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder =
            self.gpu
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("frame_encoder"),
                });

        // ── Terrain pass ─────────────────────────────────────────────────────
        {
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("terrain_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
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
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            self.terrain.draw(&mut rpass);
        }

        // ── egui pass ─────────────────────────────────────────────────────────
        // Extract values before the mutable borrow of overlay_registry.
        let fps = self.fps;
        let resolution = self.resolution;
        let seed = self.seed;
        let preset = &self.preset;
        let registry = &mut self.overlay_registry;

        let raw_input = self.egui_state.take_egui_input(&self.window);

        // Use begin_pass / end_pass (the non-deprecated path in egui 0.34).
        self.egui_ctx.begin_pass(raw_input);

        // Three panels replacing the Task 0.7 FPS-only "Info" window.
        ui::OverlayPanel::show(&self.egui_ctx, registry);
        ui::ParamsPanel::show(&self.egui_ctx, preset);
        ui::StatsPanel::show(
            &self.egui_ctx,
            &ui::StatsPanelData {
                fps,
                resolution,
                seed,
            },
        );

        let full_output = self.egui_ctx.end_pass();

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
                            load: wgpu::LoadOp::Load, // keep terrain underneath
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
            }
        }
    }
}
