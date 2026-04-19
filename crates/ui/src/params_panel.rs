//! Preset parameters panel — read-only fields plus a Sprint 1B wind
//! direction slider and Sprint 2.7 erosion sliders that report change
//! events back to the runtime.

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

    /// Any of the 4 erosion sliders (`spim_k` / `hillslope_d` /
    /// `n_batch` / `n_inner`) was touched in a way that warrants a
    /// `run_from(ErosionOuterLoop)`.
    ///
    /// **Tier A** (live, `spim_k` and `hillslope_d`): set on every
    /// `changed()` event so the terrain updates while the user drags.
    ///
    /// **Tier B** (on-release, `n_batch` and `n_inner`): set only on
    /// `drag_stopped()` or a non-drag edit, because re-routing the
    /// flow network on every intermediate integer value would stall the
    /// frame loop. The Tier A / Tier B split lives here in the egui
    /// wiring — the Runtime sees a single flag either way.
    pub erosion_changed: bool,
}

/// egui panel that shows preset fields and exposes a wind-direction
/// slider. Sprint 2+ will add the rest of the DD9 slider list
/// (lapse rate, marine moisture, Budyko ω, cloud base/top).
pub struct ParamsPanel;

impl ParamsPanel {
    /// Draw the "Params" tab body inline into the provided `ui`. Returns flags
    /// for any slider that was touched this frame.
    pub fn show(ui: &mut egui::Ui, preset: &mut IslandArchetypePreset) -> ParamsPanelResult {
        let mut result = ParamsPanelResult::default();

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

        // ── Erosion (Sprint 2) ────────────────────────────────────
        ui.separator();
        ui.label("Erosion (Sprint 2)");

        // Tier A: live update on every drag tick.
        let k_slider = egui::Slider::new(&mut preset.erosion.spim_k, 1e-5..=5e-3)
            .text("spim K")
            .logarithmic(true)
            .fixed_decimals(4);
        if ui.add(k_slider).changed() {
            result.erosion_changed = true;
        }

        let d_slider = egui::Slider::new(&mut preset.erosion.hillslope_d, 0.0..=1e-2)
            .text("hillslope D")
            .fixed_decimals(4);
        if ui.add(d_slider).changed() {
            result.erosion_changed = true;
        }

        // Tier B: fire only on drag_stopped (release) or a direct
        // click-to-edit that isn't a drag — avoids stalling the frame
        // loop with a flow-network rebuild on every intermediate integer.
        let n_batch_resp = ui.add(
            egui::DragValue::new(&mut preset.erosion.n_batch)
                .range(0_u32..=20_u32)
                .prefix("n_batch: "),
        );
        if n_batch_resp.drag_stopped() || (n_batch_resp.changed() && !n_batch_resp.dragged()) {
            result.erosion_changed = true;
        }

        let n_inner_resp = ui.add(
            egui::DragValue::new(&mut preset.erosion.n_inner)
                .range(1_u32..=20_u32)
                .prefix("n_inner: "),
        );
        if n_inner_resp.drag_stopped() || (n_inner_resp.changed() && !n_inner_resp.dragged()) {
            result.erosion_changed = true;
        }

        result
    }
}
