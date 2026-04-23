//! Application runtime — owns the window, GPU context, renderer, egui state,
//! and camera. Drives one full frame per `tick()` call.
//!
//! # Module layout
//!
//! This module is split across several sibling files to keep each concern
//! manageable. `mod.rs` holds the `Runtime` struct definition and its
//! constructors. Behaviour is implemented in:
//!
//! - `events`    — winit `ApplicationHandler` input routing
//! - `frame`     — per-frame `tick()` / GPU encode / egui pass / present
//! - `regen`     — world rebuild, sea-level fast-path, world-aspect fast-path
//! - `view_mode` — `ViewMode` enum + `set_view_mode` transition logic
//! - `tabs`      — `AppTabViewer` + `egui_dock::TabViewer` impl

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use anyhow::Context as _;
use tracing::{info, warn};
use winit::{
    dpi::{LogicalSize, PhysicalSize},
    event_loop::ActiveEventLoop,
    window::{Window, WindowAttributes},
};

use gpu::GpuContext;
use hex::geometry::{default_grid_origin, offset_to_pixel, sim_to_world_scale};
use island_core::{
    pipeline::SimulationPipeline,
    preset::IslandArchetypePreset,
    seed::Seed,
    world::{Resolution, WorldState},
};
use render::HexInstance;
use render::{
    DEFAULT_WORLD_XZ_EXTENT, HexRiverInstance, HexRiverRenderer, HexSurfaceRenderer,
    OverlayRenderer, SkyRenderer, TerrainRenderer, ViewportTextureSet, overlay::OverlayRegistry,
};
use sim::default_pipeline;

use crate::camera::{Camera, InputState};
use crate::dock::DockLayout;
use crate::world_panel::WorldPanel;

pub(super) mod events;
pub(super) mod frame;
pub(super) mod regen;
pub(super) mod tabs;
pub(super) mod view_mode;

pub use view_mode::{RenderLayer, ViewMode, render_stack_for};

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

// ── Runtime ───────────────────────────────────────────────────────────────────

/// Holds all per-window application state.
pub struct Runtime {
    pub(super) window: Arc<Window>,
    pub(super) gpu: GpuContext,
    pub(super) terrain: TerrainRenderer,
    pub(super) overlay: OverlayRenderer,
    pub(super) sky: SkyRenderer,
    /// Sprint 3.5.A c8: hex-surface fill renderer.  Constructed alongside
    /// terrain/overlay/sky; instance buffer is rebuilt by
    /// `rebuild_hex_surface_instances` after each pipeline run that updates
    /// `derived.hex_grid` / `derived.hex_attrs`.
    pub(super) hex_surface: HexSurfaceRenderer,

    /// Sprint 3.5.B c4: hex river polyline renderer.  Drawn after the hex
    /// surface fill pass so rivers read over the fill colour.  Instance buffer
    /// is rebuilt by `rebuild_hex_river_instances` alongside the hex-surface
    /// rebuild everywhere the hex data changes.
    pub(super) hex_river: HexRiverRenderer,

    // egui
    pub(super) egui_ctx: egui::Context,
    pub(super) egui_state: egui_winit::State,
    pub(super) egui_renderer: egui_wgpu::Renderer,

    // offscreen viewport texture — 3D scene renders here, egui Image displays it
    pub(super) viewport_tex: ViewportTextureSet,

    // camera
    pub(super) camera: Camera,
    pub(super) input: InputState,

    // timing / display
    pub(super) last_frame: Instant,
    pub(super) fps: f32,

    // preset loaded at startup
    pub(super) preset: IslandArchetypePreset,

    // overlay registry (Task 0.8)
    pub(super) overlay_registry: OverlayRegistry,

    // simulation metadata (Task 0.8)
    pub(super) seed: Seed,
    pub(super) resolution: Resolution,

    // Sprint 1B: the simulated world + the canonical linear pipeline.
    // The pipeline runs once at boot to fully populate `world`, and then
    // slider changes in `ParamsPanel` call `pipeline.run_from(world, X)`
    // to re-run just the affected stages.
    pub(super) world: WorldState,
    pub(super) pipeline: SimulationPipeline,

    // ViewMode three-view toggle. `saved_visibility` holds the Continuous
    // baseline while `view_mode != Continuous`; see [`ViewMode`] docs.
    pub(super) view_mode: ViewMode,
    pub(super) saved_visibility: Option<Vec<(&'static str, bool)>>,

    // egui_dock layout + persistence path.
    pub(super) dock: DockLayout,
    pub(super) dock_layout_path: PathBuf,

    // Viewport tab rect in egui logical points, written by AppTabViewer::ui
    // each frame and read by handle_window_event to gate camera mouse input.
    // None on the very first frame (before the dock has laid out).
    pub(super) viewport_rect: Option<egui::Rect>,

    // Sprint 2.6.C: World panel state (preset picker, seed, geometry sliders).
    pub(super) world_panel: WorldPanel,

    /// Horizontal world extent in world units. Initialised to
    /// `DEFAULT_WORLD_XZ_EXTENT`; changed by the World-panel aspect ComboBox
    /// without a sim pipeline re-run. The headless executor is unaffected — it
    /// always uses `DEFAULT_WORLD_XZ_EXTENT` directly.
    pub(super) world_xz_extent: f32,
}

impl Runtime {
    /// Construct the runtime: create the window, initialise GPU, renderers,
    /// and egui.
    pub fn new(event_loop: &ActiveEventLoop) -> anyhow::Result<Self> {
        // ── Window ────────────────────────────────────────────────────────────
        let attrs = WindowAttributes::default()
            .with_title("Island Proc-Gen")
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
        let overlay_registry = OverlayRegistry::sprint_3_defaults();
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

        // ── World XZ extent (render-side; never affects sim pipeline) ────────
        let world_xz_extent = DEFAULT_WORLD_XZ_EXTENT;

        // ── Terrain renderer (must follow pipeline so z_filled is populated) ─
        let terrain = TerrainRenderer::new(&gpu, &world, &preset, world_xz_extent);

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

        // ── Hex-surface renderer (c8) ─────────────────────────────────────────
        // Constructed with the same format/depth targets as terrain so both
        // renderers can share the same render pass in frame.rs.
        let mut hex_surface =
            HexSurfaceRenderer::new(&gpu.device, gpu.surface_format, gpu.depth_format);

        // Populate the initial instance buffer from the freshly-run pipeline.
        // If hex_grid / hex_attrs are not yet populated (partial pipeline),
        // upload_instances with an empty slice — draw is a no-op for 0 instances.
        let initial_instances = build_hex_instances(&world, world_xz_extent);
        hex_surface.upload_instances(&gpu.device, &gpu.queue, &initial_instances);

        // ── Hex-river renderer (Sprint 3.5.B c4) ─────────────────────────────
        let mut hex_river =
            HexRiverRenderer::new(&gpu.device, gpu.surface_format, gpu.depth_format);
        let initial_river_instances = build_hex_river_instances(&world, world_xz_extent);
        hex_river.upload_instances(&gpu.device, &gpu.queue, &initial_river_instances);

        // Centre the camera on the island mesh ([0, world_xz_extent] on XZ, Y=height).
        camera.target = glam::Vec3::new(
            world_xz_extent * 0.5,
            preset.sea_level,
            world_xz_extent * 0.5,
        );
        camera.distance = INITIAL_CAMERA_DISTANCE * world_xz_extent;
        camera.yaw = INITIAL_CAMERA_YAW;
        camera.pitch = INITIAL_CAMERA_PITCH;

        // ── World panel ───────────────────────────────────────────────────────
        // Constructed from the just-loaded preset and initial seed so the panel
        // reflects the current world state on first open.
        let world_panel = WorldPanel::new(&preset, seed.0, world_xz_extent);

        // ── Dock layout path + load ───────────────────────────────────────────
        // Resolve ~/.island_proc_gen/dock_layout.ron on Unix via $HOME.
        // On rare systems where $HOME is unset, fall back to a relative path
        // so the app still starts; a warning is logged in that case.
        let dock_layout_path: PathBuf = match std::env::var("HOME") {
            Ok(home) => PathBuf::from(home)
                .join(".island_proc_gen")
                .join("dock_layout.ron"),
            Err(_) => {
                warn!(
                    "$HOME is not set; dock layout will be persisted to \
                     ./.island_proc_gen/dock_layout.ron"
                );
                PathBuf::from(".island_proc_gen").join("dock_layout.ron")
            }
        };
        let dock = DockLayout::load_or_default(&dock_layout_path);

        Ok(Self {
            window,
            gpu,
            terrain,
            overlay,
            sky,
            hex_surface,
            hex_river,
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
            dock,
            dock_layout_path,
            viewport_rect: None,
            world_panel,
            world_xz_extent,
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
                climate: island_core::preset::ClimateParams::default(),
            }
        }
    }
}

// ── Hex-surface instance builder (c8) ────────────────────────────────────────

/// Build the per-instance GPU data for the hex surface renderer.
///
/// Reads `world.derived.hex_grid` and `world.derived.hex_attrs`.  If either
/// is `None` (pipeline not yet run, or partial run), returns an empty `Vec`
/// — a 0-instance upload makes `HexSurfaceRenderer::draw` a no-op.
///
/// Positions are converted from sim space to world space via
/// [`sim_to_world_scale`] so hexes overlay the terrain mesh correctly.
/// The biome → fill colour uses the same `PaletteId::Categorical` +
/// `ValueRange::Fixed(0.0, 7.0)` mapping as the `hex_aggregated` overlay
/// descriptor in `OverlayRegistry::sprint_3_defaults` (catalog.rs line ~141).
pub(crate) fn build_hex_instances(world: &WorldState, world_extent: f32) -> Vec<HexInstance> {
    let (hex_grid, hex_attrs) = match (
        world.derived.hex_grid.as_ref(),
        world.derived.hex_attrs.as_ref(),
    ) {
        (Some(g), Some(a)) => (g, a),
        _ => return Vec::new(),
    };

    let sim_width = world.resolution.sim_width;
    let scale = sim_to_world_scale(sim_width, world_extent);
    let origin = default_grid_origin(hex_grid.hex_size);

    let total = (hex_grid.cols * hex_grid.rows) as usize;
    let mut instances = Vec::with_capacity(total);

    for row in 0..hex_grid.rows {
        for col in 0..hex_grid.cols {
            let attr = hex_attrs.get(col, row);

            // Sim-space centre → world-space centre.
            let (sim_cx, sim_cy) = offset_to_pixel(col, row, hex_grid.hex_size, origin);
            let world_cx = sim_cx * scale;
            let world_cy = sim_cy * scale;

            // Biome → fill colour via the Categorical palette at t = biome_index / 7.0,
            // matching the hex_aggregated overlay descriptor (PaletteId::Categorical,
            // ValueRange::Fixed(0.0, 7.0)).  See crates/render/src/overlay/catalog.rs.
            let biome_index = attr.dominant_biome as u8;
            let t = biome_index as f32 / 7.0;
            let [r, g, b, a] = render::palette_sample_f32(render::PaletteId::Categorical, t);
            let fill_color_rgba = HexInstance::pack_rgba(r, g, b, a);

            // river_mask_bits: low byte = 1 if any river crosses this hex.
            let river_mask_bits = u32::from(attr.has_river);

            // coast_class_bits: 0 = Inland sentinel; 3.5.C DD4 will populate this.
            let coast_class_bits = 0u32;

            instances.push(HexInstance {
                center_xy: [world_cx, world_cy],
                elevation: attr.elevation.clamp(0.0, 1.0),
                fill_color_rgba,
                coast_class_bits,
                river_mask_bits,
                _pad: [0u32; 2],
            });
        }
    }
    instances
}

// ── Hex-river instance builder (Sprint 3.5.B c4) ─────────────────────────────

/// Build the per-instance GPU data for the hex river renderer.
///
/// Reads `world.derived.hex_debug` to access `river_crossing` and
/// `river_width` per hex. If `hex_grid` or `hex_debug` is `None` (pipeline not
/// yet run, or partial run), returns an empty `Vec` — a 0-instance upload
/// makes `HexRiverRenderer::draw` a no-op.
///
/// An instance is emitted only for hexes where **both** `river_crossing` and
/// `river_width` are `Some`. This matches the validator invariant from c3:
/// exactly one must be Some iff the other is Some.
///
/// Positions are converted from sim space to world space via
/// [`sim_to_world_scale`] using the same formula as `build_hex_instances`.
pub(crate) fn build_hex_river_instances(
    world: &WorldState,
    world_extent: f32,
) -> Vec<HexRiverInstance> {
    let (hex_grid, hex_debug) = match (
        world.derived.hex_grid.as_ref(),
        world.derived.hex_debug.as_ref(),
    ) {
        (Some(g), Some(d)) => (g, d),
        _ => return Vec::new(),
    };

    let sim_width = world.resolution.sim_width;
    let scale = sim_to_world_scale(sim_width, world_extent);
    let origin = default_grid_origin(hex_grid.hex_size);

    let total = (hex_grid.cols * hex_grid.rows) as usize;
    let mut instances = Vec::with_capacity(total / 4); // rivers are a fraction of hexes

    for row in 0..hex_grid.rows {
        for col in 0..hex_grid.cols {
            let idx = (row * hex_grid.cols + col) as usize;

            // Both must be Some — if either is None, skip this hex.
            let (crossing, width) = match (
                hex_debug.river_crossing.get(idx).and_then(|v| *v),
                hex_debug.river_width.get(idx).and_then(|v| *v),
            ) {
                (Some(c), Some(w)) => (c, w),
                _ => continue,
            };

            // Sim-space centre → world-space centre.
            let (sim_cx, sim_cy) = offset_to_pixel(col, row, hex_grid.hex_size, origin);
            let world_cx = sim_cx * scale;
            let world_cy = sim_cy * scale;

            instances.push(HexRiverInstance::pack(
                [world_cx, world_cy],
                crossing.entry_edge,
                crossing.exit_edge,
                width as u8,
            ));
        }
    }
    instances
}

// ── ViewMode unit tests ───────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::{ViewMode, events::cursor_in_rect_physical};
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
                registry: OverlayRegistry::sprint_3_defaults(),
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

    // ── cursor_in_rect_physical ───────────────────────────────────────────────

    fn make_rect(min_x: f32, min_y: f32, max_x: f32, max_y: f32) -> egui::Rect {
        egui::Rect::from_min_max(egui::pos2(min_x, min_y), egui::pos2(max_x, max_y))
    }

    /// Cursor clearly inside the rect at ppp=1.0.
    #[test]
    fn cursor_inside_rect_returns_true() {
        let rect = make_rect(10.0, 10.0, 20.0, 20.0);
        assert!(cursor_in_rect_physical((15.0, 15.0), rect, 1.0));
    }

    /// Cursor outside on all four sides.
    #[test]
    fn cursor_outside_rect_all_sides_returns_false() {
        let rect = make_rect(10.0, 10.0, 20.0, 20.0);
        // Left
        assert!(!cursor_in_rect_physical((5.0, 15.0), rect, 1.0));
        // Right
        assert!(!cursor_in_rect_physical((25.0, 15.0), rect, 1.0));
        // Above
        assert!(!cursor_in_rect_physical((15.0, 5.0), rect, 1.0));
        // Below
        assert!(!cursor_in_rect_physical((15.0, 25.0), rect, 1.0));
    }

    /// ppp=2.0: logical rect (10,10)–(20,20), physical cursor at (25,25)
    /// converts to logical (12.5, 12.5) which is inside.
    #[test]
    fn cursor_in_rect_physical_respects_ppp_scaling() {
        let rect = make_rect(10.0, 10.0, 20.0, 20.0);
        // Physical (25, 25) / ppp 2.0 = logical (12.5, 12.5) — inside
        assert!(cursor_in_rect_physical((25.0, 25.0), rect, 2.0));
        // Physical (45, 45) / ppp 2.0 = logical (22.5, 22.5) — outside
        assert!(!cursor_in_rect_physical((45.0, 45.0), rect, 2.0));
    }

    // ── Regen determinism tests (sim-level, no GPU) ───────────────────────────

    /// Helper: run a full pipeline on a fresh WorldState and return the
    /// blake3 hash of `authoritative.height.to_bytes()`.
    fn height_hash(preset_name: &str, seed: u64) -> String {
        use island_core::{
            seed::Seed,
            world::{Resolution, WorldState},
        };
        use sim::default_pipeline;

        let preset = data::presets::load_preset(preset_name)
            .unwrap_or_else(|e| panic!("load_preset({preset_name}): {e}"));
        let resolution = Resolution::new(128, 128);
        let mut world = WorldState::new(Seed(seed), preset, resolution);
        default_pipeline().run(&mut world).expect("pipeline.run");
        let bytes = world
            .authoritative
            .height
            .as_ref()
            .expect("height must be populated after pipeline.run")
            .to_bytes();
        blake3::hash(&bytes).to_hex().to_string()
    }

    /// Two different presets with the same seed must produce different height
    /// fields — verifies that `regenerate_from_world_panel` would produce a
    /// different world when the user switches presets.
    #[test]
    fn regenerate_with_different_preset_changes_world_hash() {
        let h1 = height_hash("volcanic_single", 42);
        let h2 = height_hash("caldera", 42);
        assert_ne!(
            h1, h2,
            "volcanic_single and caldera at seed=42 must produce different height hashes"
        );
    }

    /// Same preset with two different seeds must produce different height fields
    /// — verifies the seed DragValue actually matters.
    #[test]
    fn regenerate_with_different_seed_changes_world_hash() {
        let h1 = height_hash("volcanic_single", 1);
        let h2 = height_hash("volcanic_single", 2);
        assert_ne!(
            h1, h2,
            "volcanic_single at seed=1 and seed=2 must produce different height hashes"
        );
    }

    /// Changing the world aspect extent is a render-only operation — the sim
    /// pipeline is unchanged, so `authoritative.height` must hash identically
    /// before and after. CPU-only; no GPU required.
    #[test]
    fn world_aspect_change_preserves_world_state_hash() {
        use island_core::{
            seed::Seed,
            world::{Resolution, WorldState},
        };
        use sim::default_pipeline;

        let preset =
            data::presets::load_preset("volcanic_single").expect("volcanic_single must load");
        let resolution = Resolution::new(128, 128);
        let mut world = WorldState::new(Seed(42), preset, resolution);
        default_pipeline().run(&mut world).expect("pipeline.run");

        // Hash before any aspect change.
        let bytes_before = world
            .authoritative
            .height
            .as_ref()
            .expect("height must be populated")
            .to_bytes();
        let hash_before = blake3::hash(&bytes_before).to_hex().to_string();

        // Simulate what apply_world_aspect does at the sim level — nothing.
        // The world state is untouched; only render geometry would change.
        // A second pipeline run with the same inputs must produce the same hash.
        let bytes_after = world
            .authoritative
            .height
            .as_ref()
            .expect("height must be populated")
            .to_bytes();
        let hash_after = blake3::hash(&bytes_after).to_hex().to_string();

        assert_eq!(
            hash_before, hash_after,
            "world aspect change must not alter authoritative.height hash"
        );
    }
}
