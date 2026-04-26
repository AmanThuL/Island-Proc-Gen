use std::time::Instant;

use island_core::{seed::Seed, world::WorldState};
use sim::{StageId, invalidate_from};

use super::Runtime;

impl Runtime {
    /// Rebuild the hex-surface instance buffer from `world.derived.hex_grid`
    /// and `world.derived.hex_attrs`.
    ///
    /// Called after any stage run that changes those fields — full pipeline
    /// runs (`regenerate_from_world_panel`), sea-level fast paths
    /// (`apply_sea_level_fast_path`), and slider re-runs in `frame.rs`.
    ///
    /// Positions/sizes are converted from sim space to world space via
    /// [`hex::geometry::sim_to_world_scale`] so hexes overlay the terrain
    /// mesh correctly. If either `hex_grid` or `hex_attrs` is `None`
    /// (partial pipeline run), an empty instance buffer is uploaded —
    /// the draw call becomes a no-op.
    pub(super) fn rebuild_hex_surface_instances(&mut self) {
        let instances = super::build_hex_instances(&self.world, self.world_xz_extent);
        self.hex_surface
            .upload_instances(&self.gpu.device, &self.gpu.queue, &instances);
    }

    /// Rebuild the hex-river instance buffer from `world.derived.hex_debug`.
    ///
    /// Called alongside `rebuild_hex_surface_instances` everywhere the hex
    /// data changes so river threads stay in sync with the surface fill.
    ///
    /// If `hex_grid` or `hex_debug` is `None` (partial pipeline run), an
    /// empty instance buffer is uploaded — the draw call becomes a no-op.
    pub(super) fn rebuild_hex_river_instances(&mut self) {
        let instances = super::build_hex_river_instances(&self.world, self.world_xz_extent);
        self.hex_river
            .upload_instances(&self.gpu.device, &self.gpu.queue, &instances);
    }

    /// Full world rebuild triggered by the `Regenerate` button.
    ///
    /// Reads the current `world_panel` state (preset name, seed, three slider
    /// overrides), rebuilds the `WorldState`, runs the full pipeline, and
    /// re-initialises both renderers so the GPU buffers reflect the new world.
    pub(super) fn regenerate_from_world_panel(&mut self) -> anyhow::Result<()> {
        // 1. Build new preset: load the stock preset, then apply slider overrides.
        let mut new_preset = data::presets::load_preset(&self.world_panel.preset_name)?;
        new_preset.island_radius = self.world_panel.island_radius;
        new_preset.max_relief = self.world_panel.max_relief;
        new_preset.sea_level = self.world_panel.sea_level;

        // 2. New WorldState.
        let new_seed = Seed(self.world_panel.seed);
        self.preset = new_preset.clone();
        self.world = WorldState::new(new_seed, new_preset, self.resolution);
        self.seed = new_seed;

        // 3. Full pipeline run. Reset profiler state before running so
        //    cumulative_timings reflects only the new regen, not the old session.
        self.cumulative_timings.clear();
        self.dirty_frontier = None;
        let regen_start = Instant::now();
        self.pipeline.run(&mut self.world)?;
        self.last_regen_ms = regen_start.elapsed().as_secs_f64() * 1_000.0;
        self.accumulate_tick_timings();

        // 4. Rebuild terrain renderer — picks up new sea_vbo height + new
        //    light uniform sea_level from the preset.
        self.terrain = render::TerrainRenderer::new(
            &self.gpu,
            &self.world,
            &self.preset,
            self.world_xz_extent,
        );

        // 5. Rebuild overlay renderer — shares the new terrain VBO/IBO/view_buf.
        self.overlay = render::OverlayRenderer::new(
            &self.gpu,
            &self.world,
            &self.overlay_registry,
            self.terrain.view_buf(),
            self.terrain.terrain_vbo(),
            self.terrain.terrain_ibo(),
            self.terrain.terrain_index_count(),
        );

        // 6. Recentre camera target at the new water line (preserve
        //    yaw/pitch/distance).
        self.camera.target = glam::Vec3::new(
            self.world_xz_extent * 0.5,
            self.preset.sea_level,
            self.world_xz_extent * 0.5,
        );

        // 7. Reset panel slider state to the just-applied preset values so
        //    the next edit starts from a correct baseline.
        self.world_panel.island_radius = self.preset.island_radius;
        self.world_panel.max_relief = self.preset.max_relief;
        self.world_panel.sea_level = self.preset.sea_level;

        // 8. Rebuild hex-surface + hex-river instance buffers from the new derived caches.
        self.rebuild_hex_surface_instances();
        self.rebuild_hex_river_instances();

        Ok(())
    }

    /// Sea-level fast path — re-runs only from `Coastal` instead of rebuilding
    /// the whole world.  Called when the `sea_level` slider is released.
    pub(super) fn apply_sea_level_fast_path(&mut self) -> anyhow::Result<()> {
        // 1. Sync the new sea_level into both preset mirrors.
        let new_sea_level = self.world_panel.sea_level;
        self.preset.sea_level = new_sea_level;
        self.world.preset.sea_level = new_sea_level;

        // 2. Invalidate + re-run from Coastal. Reset profiler cumulative so the
        //    Profiler tab reflects only stages that actually re-ran.
        self.cumulative_timings.clear();
        self.dirty_frontier = Some(StageId::Coastal);
        invalidate_from(&mut self.world, StageId::Coastal);
        self.pipeline
            .run_from(&mut self.world, StageId::Coastal as usize)?;
        self.accumulate_tick_timings();

        // 3. Update terrain renderer sea quad vertices + light uniform.
        self.terrain.update_sea_level(&self.gpu, new_sea_level);

        // 4. Refresh overlay textures (coast_mask and derived overlays changed).
        self.overlay
            .refresh(&self.gpu, &self.world, &self.overlay_registry);

        // 5. Move camera target Y to the new water line.
        self.camera.target.y = new_sea_level;

        // 6. Rebuild hex-surface + hex-river instances — sea level change re-runs
        //    Coastal + downstream, which updates hex_attrs.elevation and hex_debug.
        self.rebuild_hex_surface_instances();
        self.rebuild_hex_river_instances();

        Ok(())
    }

    /// World-aspect fast path — rebuilds the terrain mesh and sea quad at the
    /// new horizontal extent, then rebuilds `OverlayRenderer` against the fresh
    /// VBO/IBO handles. No sim pipeline re-run; all `authoritative.*` fields are
    /// unchanged.
    ///
    /// Called when the World-panel aspect ComboBox fires an
    /// [`WorldPanelEvent::aspect_extent_changed`] event.
    pub(super) fn apply_world_aspect(&mut self, new_extent: f32) {
        let old_extent = self.world_xz_extent;
        self.world_xz_extent = new_extent;

        // Rebuild terrain mesh at the new extent.
        self.terrain =
            render::TerrainRenderer::new(&self.gpu, &self.world, &self.preset, new_extent);

        // Rebuild overlay against the fresh buffer handles.
        self.overlay = render::OverlayRenderer::new(
            &self.gpu,
            &self.world,
            &self.overlay_registry,
            self.terrain.view_buf(),
            self.terrain.terrain_vbo(),
            self.terrain.terrain_ibo(),
            self.terrain.terrain_index_count(),
        );

        // Recentre camera target; scale distance to preserve relative framing.
        self.camera.target =
            glam::Vec3::new(new_extent * 0.5, self.preset.sea_level, new_extent * 0.5);
        let scale = new_extent / old_extent.max(f32::EPSILON);
        self.camera.distance *= scale;

        // Rebuild hex instances at the new world extent — the sim_to_world_scale
        // factor changes with extent even though hex_attrs / hex_debug are unchanged.
        self.rebuild_hex_surface_instances();
        self.rebuild_hex_river_instances();
    }
}
