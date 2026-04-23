use island_core::{seed::Seed, world::WorldState};
use sim::{StageId, invalidate_from};

use super::Runtime;

impl Runtime {
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

        // 3. Full pipeline run.
        self.pipeline.run(&mut self.world)?;

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

        Ok(())
    }

    /// Sea-level fast path — re-runs only from `Coastal` instead of rebuilding
    /// the whole world.  Called when the `sea_level` slider is released.
    pub(super) fn apply_sea_level_fast_path(&mut self) -> anyhow::Result<()> {
        // 1. Sync the new sea_level into both preset mirrors.
        let new_sea_level = self.world_panel.sea_level;
        self.preset.sea_level = new_sea_level;
        self.world.preset.sea_level = new_sea_level;

        // 2. Invalidate + re-run from Coastal.
        invalidate_from(&mut self.world, StageId::Coastal);
        self.pipeline
            .run_from(&mut self.world, StageId::Coastal as usize)?;

        // 3. Update terrain renderer sea quad vertices + light uniform.
        self.terrain.update_sea_level(&self.gpu, new_sea_level);

        // 4. Refresh overlay textures (coast_mask and derived overlays changed).
        self.overlay
            .refresh(&self.gpu, &self.world, &self.overlay_registry);

        // 5. Move camera target Y to the new water line.
        self.camera.target.y = new_sea_level;

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
    }
}
