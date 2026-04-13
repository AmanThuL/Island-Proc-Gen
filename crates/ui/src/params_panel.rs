//! Preset parameters panel — read-only view of [`IslandArchetypePreset`].

use island_core::preset::IslandArchetypePreset;

/// egui panel that displays the current preset's fields (read-only).
pub struct ParamsPanel;

impl ParamsPanel {
    /// Draw the "Params" window.
    pub fn show(ctx: &egui::Context, preset: &IslandArchetypePreset) {
        egui::Window::new("Params")
            .default_pos(egui::pos2(16.0, 180.0))
            .show(ctx, |ui| {
                ui.label(format!("name: {}", preset.name));
                ui.label(format!("island_radius: {:.3}", preset.island_radius));
                ui.label(format!("max_relief: {:.3}", preset.max_relief));
                ui.label(format!(
                    "volcanic_center_count: {}",
                    preset.volcanic_center_count
                ));
                ui.label(format!("island_age: {:?}", preset.island_age));
                ui.label(format!(
                    "prevailing_wind_dir: {:.3}",
                    preset.prevailing_wind_dir
                ));
                ui.label(format!(
                    "marine_moisture_strength: {:.3}",
                    preset.marine_moisture_strength
                ));
                ui.label(format!("sea_level: {:.3}", preset.sea_level));
            });
    }
}
