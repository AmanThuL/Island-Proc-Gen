//! Stats panel — FPS, resolution, and seed hash.

use island_core::{seed::Seed, world::Resolution};

/// egui panel that shows runtime statistics.
pub struct StatsPanel;

/// Data snapshot passed to [`StatsPanel::show`] each frame.
pub struct StatsPanelData {
    /// Exponential-moving-average frames per second.
    pub fps: f32,
    /// Simulation-grid resolution.
    pub resolution: Resolution,
    /// Deterministic seed for this session.
    pub seed: Seed,
}

impl StatsPanel {
    /// Draw the "Stats" tab body inline into the provided `ui`.
    pub fn show(ui: &mut egui::Ui, data: &StatsPanelData) {
        ui.label(format!("FPS: {:.1}", data.fps));
        ui.label(format!(
            "resolution: {}x{}",
            data.resolution.sim_width, data.resolution.sim_height
        ));
        ui.label(format!("seed: 0x{:016x}", data.seed.0));
    }
}
