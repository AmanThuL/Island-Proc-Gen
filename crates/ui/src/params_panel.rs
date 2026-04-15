//! Preset parameters panel — read-only fields plus a Sprint 1B wind
//! direction slider that reports a change event back to the runtime.

use island_core::preset::IslandArchetypePreset;
use std::f32::consts::TAU;

/// Result of one `ParamsPanel::show` call. The `*_changed` flags let
/// the caller decide which pipeline stages to re-run via `run_from`.
#[derive(Debug, Default, Clone, Copy)]
pub struct ParamsPanelResult {
    /// `preset.prevailing_wind_dir` was touched in this frame. The
    /// caller should `run_from(StageId::Precipitation)` to rebuild
    /// precipitation + fog + downstream (water balance, soil moisture,
    /// biomes, hex projection).
    pub wind_dir_changed: bool,
}

/// egui panel that shows preset fields and exposes a wind-direction
/// slider. Sprint 2+ will add the rest of the DD9 slider list
/// (lapse rate, marine moisture, Budyko ω, cloud base/top).
pub struct ParamsPanel;

impl ParamsPanel {
    /// Draw the "Params" window. Returns flags for any slider that
    /// was touched this frame.
    pub fn show(ctx: &egui::Context, preset: &mut IslandArchetypePreset) -> ParamsPanelResult {
        let mut result = ParamsPanelResult::default();
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

                ui.separator();
                ui.label("Climate");
                let wind_slider = egui::Slider::new(&mut preset.prevailing_wind_dir, 0.0..=TAU)
                    .text("wind dir (rad)")
                    .fixed_decimals(3);
                let wind_response = ui.add(wind_slider);
                if wind_response.changed() {
                    result.wind_dir_changed = true;
                }

                ui.label(format!(
                    "marine_moisture_strength: {:.3}",
                    preset.marine_moisture_strength
                ));
                ui.label(format!("sea_level: {:.3}", preset.sea_level));
            });
        result
    }
}
