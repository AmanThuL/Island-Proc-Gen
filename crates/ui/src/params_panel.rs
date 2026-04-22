//! Preset parameters panel — read-only fields plus a Sprint 1B wind
//! direction slider, Sprint 2.7 erosion sliders, and Sprint 3.8 SPACE-lite
//! and LFPM climate sliders that report change events back to the runtime.

use island_core::preset::IslandArchetypePreset;
use std::f32::consts::TAU;

// ── Sprint 3.8 slider range constants ────────────────────────────────────────
// Declared as pub(crate) constants so the unit tests in this module can verify
// them without pulling in a live egui context.

/// `space_k_bed` slider range: `1e-4 ..= 6e-3`. Upper bound tightened in
/// Sprint 3.1 post-close from the spec's `2e-2` — the calibration probe
/// found every K above ~5.5e-3 trips `erosion_no_excessive_sea_crossing`
/// on at least one grid size (5 % land-cell-loss invariant), and the
/// default `5.0e-3` sits near the practical maximum. The old `2e-2` cap
/// let interactive slider drags flood the WARN log as soon as the value
/// crossed ~7e-3; 6e-3 gives a small experimental margin without
/// guaranteed tripping.
pub(crate) const SPACE_K_BED_RANGE: std::ops::RangeInclusive<f32> = 1e-4_f32..=6e-3_f32;
/// `space_k_sed` slider range: `1e-4 ..= 1.8e-2`. Upper bound tracks
/// `SPACE_K_BED_RANGE.end * 3` per the DD2 3:1 `K_sed / K_bed` ratio
/// (both sliders mutate independently, but the range cap preserves the
/// ratio envelope).
pub(crate) const SPACE_K_SED_RANGE: std::ops::RangeInclusive<f32> = 1e-4_f32..=1.8e-2_f32;
/// `h_star` slider range: `0.01 ..= 0.30`.
pub(crate) const H_STAR_RANGE: std::ops::RangeInclusive<f32> = 0.01_f32..=0.30_f32;
/// `q_0` slider range: `0.5 ..= 2.0`.
pub(crate) const Q_0_RANGE: std::ops::RangeInclusive<f32> = 0.5_f32..=2.0_f32;
/// `tau_c` slider range: `0.05 ..= 0.50`.
pub(crate) const TAU_C_RANGE: std::ops::RangeInclusive<f32> = 0.05_f32..=0.50_f32;
/// `tau_f` slider range: `0.20 ..= 10.00`. Upper bound raised from 2.00 in
/// Sprint 3.1 Task 3.1.C.0 to accommodate the new 5.0 default (was 0.60).
pub(crate) const TAU_F_RANGE: std::ops::RangeInclusive<f32> = 0.20_f32..=10.00_f32;

// ── Slider → StageId frontier mapping (test infrastructure only) ─────────────
// These items are used exclusively by the unit tests in this file and need not
// be visible to the rest of the crate outside a test build.

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Sprint3SliderId {
    SpaceKBed,
    SpaceKSed,
    HStar,
    Q0,
    TauC,
    TauF,
}

/// Frontier StageId discriminants (usize).
/// `ErosionOuterLoop = 8`, `Precipitation = 11` per `sim::StageId`.
#[cfg(test)]
pub(crate) const EROSION_OUTER_LOOP_IDX: usize = 8;
#[cfg(test)]
pub(crate) const PRECIPITATION_IDX: usize = 11;

#[cfg(test)]
pub(crate) fn sprint3_slider_frontier(id: Sprint3SliderId) -> usize {
    match id {
        Sprint3SliderId::SpaceKBed | Sprint3SliderId::SpaceKSed | Sprint3SliderId::HStar => {
            EROSION_OUTER_LOOP_IDX
        }
        Sprint3SliderId::Q0 | Sprint3SliderId::TauC | Sprint3SliderId::TauF => PRECIPITATION_IDX,
    }
}

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

    /// Any of the Sprint 3.8 SPACE-lite erosion sliders (`space_k_bed` /
    /// `space_k_sed` / `h_star`) was touched. The caller should
    /// `invalidate_from(ErosionOuterLoop)` + `run_from(ErosionOuterLoop)`.
    ///
    /// **Tier A**: live update on every drag tick (same as `spim_k`).
    pub space_changed: bool,

    /// Any of the Sprint 3.8 LFPM climate sliders (`q_0` / `tau_c` /
    /// `tau_f`) was touched. The caller should `run_from(Precipitation)`.
    ///
    /// **Tier A**: live update on every drag tick.
    pub climate_changed: bool,
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

        // ── SPACE-lite erosion (Sprint 3.8) ──────────────────────────
        // Tier A: live update on drag. Frontier: ErosionOuterLoop (idx 8).
        ui.separator();
        ui.label("Erosion SPACE-lite (Sprint 3)");

        if ui
            .add(
                egui::Slider::new(&mut preset.erosion.space_k_bed, SPACE_K_BED_RANGE)
                    .text("K_bed")
                    .logarithmic(true)
                    .fixed_decimals(5),
            )
            .changed()
        {
            result.space_changed = true;
        }

        if ui
            .add(
                egui::Slider::new(&mut preset.erosion.space_k_sed, SPACE_K_SED_RANGE)
                    .text("K_sed")
                    .logarithmic(true)
                    .fixed_decimals(5),
            )
            .changed()
        {
            result.space_changed = true;
        }

        if ui
            .add(
                egui::Slider::new(&mut preset.erosion.h_star, H_STAR_RANGE)
                    .text("H* (cover)")
                    .logarithmic(true)
                    .fixed_decimals(4),
            )
            .changed()
        {
            result.space_changed = true;
        }

        // ── LFPM climate (Sprint 3.8) ─────────────────────────────────
        // Tier A: live update on drag. Frontier: Precipitation (idx 11).
        ui.separator();
        ui.label("Climate LFPM (Sprint 3)");

        if ui
            .add(
                egui::Slider::new(&mut preset.climate.q_0, Q_0_RANGE)
                    .text("q\u{2080} (init. vapour)")
                    .fixed_decimals(3),
            )
            .changed()
        {
            result.climate_changed = true;
        }

        if ui
            .add(
                egui::Slider::new(&mut preset.climate.tau_c, TAU_C_RANGE)
                    .text("\u{03C4}_c (condensation)")
                    .fixed_decimals(3),
            )
            .changed()
        {
            result.climate_changed = true;
        }

        if ui
            .add(
                egui::Slider::new(&mut preset.climate.tau_f, TAU_F_RANGE)
                    .text("\u{03C4}_f (fallout)")
                    .fixed_decimals(3),
            )
            .changed()
        {
            result.climate_changed = true;
        }

        result
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify the six slider range constants match the spec (§5 of
    /// sprint_3_sediment_advanced_climate.md).
    #[test]
    fn sprint_3_slider_ranges_match_spec() {
        // SPACE-lite erosion — logarithmic K values
        assert_eq!(*SPACE_K_BED_RANGE.start(), 1e-4_f32, "SPACE_K_BED start");
        assert_eq!(*SPACE_K_BED_RANGE.end(), 6e-3_f32, "SPACE_K_BED end");

        assert_eq!(*SPACE_K_SED_RANGE.start(), 1e-4_f32, "SPACE_K_SED start");
        assert_eq!(*SPACE_K_SED_RANGE.end(), 1.8e-2_f32, "SPACE_K_SED end");

        assert_eq!(*H_STAR_RANGE.start(), 0.01_f32, "H_STAR start");
        assert_eq!(*H_STAR_RANGE.end(), 0.30_f32, "H_STAR end");

        // LFPM climate — linear
        assert_eq!(*Q_0_RANGE.start(), 0.5_f32, "Q_0 start");
        assert_eq!(*Q_0_RANGE.end(), 2.0_f32, "Q_0 end");

        assert_eq!(*TAU_C_RANGE.start(), 0.05_f32, "TAU_C start");
        assert_eq!(*TAU_C_RANGE.end(), 0.50_f32, "TAU_C end");

        assert_eq!(*TAU_F_RANGE.start(), 0.20_f32, "TAU_F start");
        assert_eq!(*TAU_F_RANGE.end(), 10.00_f32, "TAU_F end");
    }

    /// Verify that each Sprint 3.8 slider maps to the correct `run_from`
    /// frontier index.  ErosionOuterLoop = 8; Precipitation = 11 per
    /// `sim::StageId`.
    #[test]
    fn sprint_3_slider_rerun_frontier_mapping() {
        // SPACE-lite sliders → ErosionOuterLoop (8)
        assert_eq!(
            sprint3_slider_frontier(Sprint3SliderId::SpaceKBed),
            EROSION_OUTER_LOOP_IDX,
            "space_k_bed must map to ErosionOuterLoop"
        );
        assert_eq!(
            sprint3_slider_frontier(Sprint3SliderId::SpaceKSed),
            EROSION_OUTER_LOOP_IDX,
            "space_k_sed must map to ErosionOuterLoop"
        );
        assert_eq!(
            sprint3_slider_frontier(Sprint3SliderId::HStar),
            EROSION_OUTER_LOOP_IDX,
            "h_star must map to ErosionOuterLoop"
        );

        // LFPM climate sliders → Precipitation (11)
        assert_eq!(
            sprint3_slider_frontier(Sprint3SliderId::Q0),
            PRECIPITATION_IDX,
            "q_0 must map to Precipitation"
        );
        assert_eq!(
            sprint3_slider_frontier(Sprint3SliderId::TauC),
            PRECIPITATION_IDX,
            "tau_c must map to Precipitation"
        );
        assert_eq!(
            sprint3_slider_frontier(Sprint3SliderId::TauF),
            PRECIPITATION_IDX,
            "tau_f must map to Precipitation"
        );

        // Cross-check the index constants against StageId discriminants.
        assert_eq!(EROSION_OUTER_LOOP_IDX, 8, "ErosionOuterLoop discriminant");
        assert_eq!(PRECIPITATION_IDX, 11, "Precipitation discriminant");
    }

    /// Default preset values must lie within the slider ranges, so the
    /// sliders are not initialised outside their own bounds.
    #[test]
    fn sprint_3_default_values_lie_within_ranges() {
        use island_core::preset::{ClimateParams, ErosionParams};
        let ep = ErosionParams::default();
        let cp = ClimateParams::default();

        assert!(
            SPACE_K_BED_RANGE.contains(&ep.space_k_bed),
            "default space_k_bed={} not in range {:?}",
            ep.space_k_bed,
            SPACE_K_BED_RANGE,
        );
        assert!(
            SPACE_K_SED_RANGE.contains(&ep.space_k_sed),
            "default space_k_sed={} not in range {:?}",
            ep.space_k_sed,
            SPACE_K_SED_RANGE,
        );
        assert!(
            H_STAR_RANGE.contains(&ep.h_star),
            "default h_star={} not in range {:?}",
            ep.h_star,
            H_STAR_RANGE,
        );
        assert!(
            Q_0_RANGE.contains(&cp.q_0),
            "default q_0={} not in range {:?}",
            cp.q_0,
            Q_0_RANGE,
        );
        assert!(
            TAU_C_RANGE.contains(&cp.tau_c),
            "default tau_c={} not in range {:?}",
            cp.tau_c,
            TAU_C_RANGE,
        );
        assert!(
            TAU_F_RANGE.contains(&cp.tau_f),
            "default tau_f={} not in range {:?}",
            cp.tau_f,
            TAU_F_RANGE,
        );
    }
}
