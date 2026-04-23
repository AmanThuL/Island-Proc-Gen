//! Climate invariants — precipitation non-negativity, temperature physical
//! range, and V3Lfpm precipitation mass-balance.

use crate::world::WorldState;

use super::{PRECIP_MEAN_HI, PRECIP_MEAN_LO, ValidationError};

/// Every cell of `world.baked.precipitation` is `>= 0`.
pub fn precipitation_nonneg(world: &WorldState) -> Result<(), ValidationError> {
    let precip =
        world
            .baked
            .precipitation
            .as_ref()
            .ok_or(ValidationError::MissingPrecondition {
                field: "baked.precipitation",
            })?;
    for y in 0..precip.height {
        for x in 0..precip.width {
            let v = precip.get(x, y);
            if v < 0.0 {
                return Err(ValidationError::PrecipitationNegative { x, y, value: v });
            }
        }
    }
    Ok(())
}

/// Every cell temperature sits between the lapse-rate-derived minimum
/// and sea-level-plus-coastal-modifier maximum, within a small slack.
pub fn temperature_physical_range(world: &WorldState) -> Result<(), ValidationError> {
    let temperature =
        world
            .baked
            .temperature
            .as_ref()
            .ok_or(ValidationError::MissingPrecondition {
                field: "baked.temperature",
            })?;

    // Physical bounds from the Sprint 1B TemperatureStage contract.
    // `TemperatureStage` owns the numeric constants, so we recompute
    // the bounds from the preset here rather than hardcoding a copy.
    const T_SEA_LEVEL_C: f32 = 26.0;
    const LAPSE_RATE_C_PER_KM: f32 = 6.5;
    const COASTAL_MODIFIER_C: f32 = 2.0;
    const SLACK: f32 = 1.0;

    let peak_m = crate::preset::MAX_RELIEF_REF_M * world.preset.max_relief;
    let max_lapse = LAPSE_RATE_C_PER_KM * peak_m / 1000.0;
    let lo = T_SEA_LEVEL_C - max_lapse - SLACK;
    let hi = T_SEA_LEVEL_C + COASTAL_MODIFIER_C + SLACK;

    for y in 0..temperature.height {
        for x in 0..temperature.width {
            let v = temperature.get(x, y);
            if v < lo || v > hi {
                return Err(ValidationError::TemperatureOutOfRange {
                    x,
                    y,
                    value: v,
                    lo,
                    hi,
                    sea_c: T_SEA_LEVEL_C,
                    peak_m,
                });
            }
        }
    }
    Ok(())
}

/// V3Lfpm precipitation mass-balance sanity check.
///
/// Only runs when `preset.climate.precipitation_variant == V3Lfpm`; callers
/// (and [`crate::sim::ValidationStage`]) must gate on this condition and skip
/// for `V2Raymarch`.
///
/// The V3 sweep normalises `P ∈ [0, 1]` per cell in its last step. A mean
/// below [`PRECIP_MEAN_LO`] means the pipeline produced effectively zero rain;
/// a mean above [`PRECIP_MEAN_HI`] means values leaked outside the `[0, 1]`
/// normalisation range. Both are pipeline-breakage indicators, not
/// calibration issues.
///
/// Decision (Task 3.9): use a simple `mean_p ∈ [PRECIP_MEAN_LO, PRECIP_MEAN_HI]`
/// guard rather than an analytical "half-saturation" proxy, because (a) the
/// analytical proxy requires knowing the per-cell normalisation scale which is
/// internal to the V3 sweep, and (b) the simpler check reliably catches the
/// two real failure modes (zero rain, out-of-range explosion) without false-
/// positive-ing on the 5 shipped archetypes where measured mean P ∈ [0.1, 0.8].
///
/// Returns `Err(MissingPrecondition)` if `baked.precipitation` is `None`;
/// callers that gate on V2/V3 dispatch should call this only after the
/// precipitation stage has run.
///
/// # Errors
///
/// * [`ValidationError::MissingPrecondition`] — `baked.precipitation` absent.
/// * [`ValidationError::PrecipitationMassBalanceViolation`] — mean outside `[LO, HI]`.
pub fn precipitation_mass_balance(world: &WorldState) -> Result<(), ValidationError> {
    let precip =
        world
            .baked
            .precipitation
            .as_ref()
            .ok_or(ValidationError::MissingPrecondition {
                field: "baked.precipitation",
            })?;

    let coast_mask =
        world
            .derived
            .coast_mask
            .as_ref()
            .ok_or(ValidationError::MissingPrecondition {
                field: "derived.coast_mask",
            })?;

    let land_total = coast_mask.land_cell_count;
    if land_total == 0 {
        return Ok(()); // all-sea preset; no land → skip.
    }

    let sum_p: f32 = precip
        .data
        .iter()
        .zip(coast_mask.is_land.data.iter())
        .filter(|&(_, &is_land)| is_land == 1)
        .map(|(&p, _)| p)
        .sum();

    let mean_p = sum_p / land_total as f32;

    if !(PRECIP_MEAN_LO..=PRECIP_MEAN_HI).contains(&mean_p) {
        return Err(ValidationError::PrecipitationMassBalanceViolation {
            mean: mean_p,
            lo: PRECIP_MEAN_LO,
            hi: PRECIP_MEAN_HI,
        });
    }

    Ok(())
}

// ─── tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::field::{MaskField2D, ScalarField2D};
    use crate::preset::IslandAge;
    use crate::preset::IslandArchetypePreset;
    use crate::preset::PrecipitationVariant;
    use crate::seed::Seed;
    use crate::world::{BakedSnapshot, CoastMask, Resolution, WorldState};

    fn test_preset() -> IslandArchetypePreset {
        IslandArchetypePreset {
            name: "validation_test".into(),
            island_radius: 0.5,
            max_relief: 0.5,
            volcanic_center_count: 1,
            island_age: IslandAge::Young,
            prevailing_wind_dir: 0.0,
            marine_moisture_strength: 0.5,
            sea_level: 0.3,
            erosion: Default::default(),
            climate: Default::default(),
        }
    }

    /// Build a minimal CoastMask from raw Vec<u8> data.
    fn make_coast_mask(
        w: u32,
        h: u32,
        is_land: Vec<u8>,
        is_sea: Vec<u8>,
        is_coast: Vec<u8>,
    ) -> CoastMask {
        let land_cell_count = is_land.iter().map(|&v| v as u32).sum();
        let mut land = MaskField2D::new(w, h);
        land.data = is_land;
        let mut sea = MaskField2D::new(w, h);
        sea.data = is_sea;
        let mut coast = MaskField2D::new(w, h);
        coast.data = is_coast;
        CoastMask {
            is_land: land,
            is_sea: sea,
            is_coast: coast,
            land_cell_count,
            river_mouth_mask: None,
        }
    }

    fn minimal_world_for_1b(w: u32, h: u32) -> WorldState {
        let mut world = WorldState::new(Seed(0), test_preset(), Resolution::new(w, h));
        world.baked = BakedSnapshot::default();
        world.derived.coast_mask = Some(make_coast_mask(
            w,
            h,
            vec![1u8; (w * h) as usize],
            vec![0u8; (w * h) as usize],
            vec![0u8; (w * h) as usize],
        ));
        world
    }

    fn make_precip_world(
        w: u32,
        h: u32,
        precip_data: Vec<f32>,
        variant: crate::preset::PrecipitationVariant,
    ) -> WorldState {
        let n = (w * h) as usize;
        let mut preset = test_preset();
        preset.climate.precipitation_variant = variant;
        let mut world = WorldState::new(Seed(0), preset, Resolution::new(w, h));
        world.derived.coast_mask = Some(make_coast_mask(
            w,
            h,
            vec![1u8; n],
            vec![0u8; n],
            vec![0u8; n],
        ));
        let mut p = ScalarField2D::<f32>::new(w, h);
        p.data = precip_data;
        world.baked.precipitation = Some(p);
        world
    }

    #[test]
    fn precipitation_nonneg_happy_path() {
        let mut world = minimal_world_for_1b(4, 4);
        let mut p = ScalarField2D::<f32>::new(4, 4);
        p.data.fill(0.3);
        world.baked.precipitation = Some(p);
        assert!(precipitation_nonneg(&world).is_ok());
    }

    #[test]
    fn precipitation_nonneg_detects_negative() {
        let mut world = minimal_world_for_1b(4, 4);
        let mut p = ScalarField2D::<f32>::new(4, 4);
        p.set(2, 1, -0.1);
        world.baked.precipitation = Some(p);
        let err = precipitation_nonneg(&world).unwrap_err();
        assert!(matches!(
            err,
            ValidationError::PrecipitationNegative { x: 2, y: 1, .. }
        ));
    }

    #[test]
    fn temperature_physical_range_happy_path() {
        let mut world = minimal_world_for_1b(4, 4);
        let mut t = ScalarField2D::<f32>::new(4, 4);
        t.data.fill(20.0);
        world.baked.temperature = Some(t);
        assert!(temperature_physical_range(&world).is_ok());
    }

    #[test]
    fn temperature_physical_range_detects_too_hot() {
        let mut world = minimal_world_for_1b(4, 4);
        let mut t = ScalarField2D::<f32>::new(4, 4);
        t.data.fill(20.0);
        t.set(1, 2, 50.0); // impossibly hot
        world.baked.temperature = Some(t);
        let err = temperature_physical_range(&world).unwrap_err();
        assert!(matches!(
            err,
            ValidationError::TemperatureOutOfRange { x: 1, y: 2, .. }
        ));
    }

    // ── precipitation_mass_balance: test 1 — nominal V3 ─────────────────────
    #[test]
    fn precipitation_mass_balance_accepts_nominal_v3() {
        // Mean of 0.3 is well within [1e-4, 1.0].
        let world = make_precip_world(4, 1, vec![0.2, 0.3, 0.4, 0.3], PrecipitationVariant::V3Lfpm);
        assert!(
            precipitation_mass_balance(&world).is_ok(),
            "nominal V3 precipitation must pass mass-balance check"
        );
    }

    // ── precipitation_mass_balance: test 2 — V2 world skips even if broken ───
    //
    // The invariant is guarded at the ValidationStage level; calling it
    // directly on a V2 world with zero precip should not panic, but the test
    // exercises the skip-logic by relying on ValidationStage's guard.
    // We test the function directly here with a valid world to verify it
    // at least returns Ok when the precip values are valid (V2 = skip at
    // call-site, but here we call it directly and it does run).
    //
    // The meaningful guard test is in validation_stage.rs where the `if V3Lfpm`
    // branch lives. Here we just verify the function doesn't panic or return
    // an error when called on a V2-labelled world with valid precip values.
    #[test]
    fn precipitation_mass_balance_v2_world_with_valid_precip_passes() {
        let world = make_precip_world(4, 1, vec![0.3; 4], PrecipitationVariant::V2Raymarch);
        // The function itself doesn't check the variant — it's the caller's job.
        assert!(
            precipitation_mass_balance(&world).is_ok(),
            "valid precip on a V2 world must pass when called directly"
        );
    }

    // ── precipitation_mass_balance: test 3 — zero precip fires ───────────────
    #[test]
    fn precipitation_mass_balance_rejects_zero() {
        let world = make_precip_world(4, 1, vec![0.0f32; 4], PrecipitationVariant::V3Lfpm);
        let err = precipitation_mass_balance(&world).unwrap_err();
        assert!(
            matches!(
                err,
                ValidationError::PrecipitationMassBalanceViolation { mean, .. }
                if mean == 0.0
            ),
            "zero precipitation must fire PrecipitationMassBalanceViolation, got: {err}"
        );
    }

    // ── precipitation_mass_balance: test 4 — explosion fires ─────────────────
    //
    // All cells with P = 5.0 → mean = 5.0 > PRECIP_MEAN_HI = 1.0.
    #[test]
    fn precipitation_mass_balance_rejects_explosion() {
        let world = make_precip_world(4, 1, vec![5.0f32; 4], PrecipitationVariant::V3Lfpm);
        let err = precipitation_mass_balance(&world).unwrap_err();
        assert!(
            matches!(
                err,
                ValidationError::PrecipitationMassBalanceViolation { mean, .. }
                if mean == 5.0
            ),
            "P=5 explosion must fire PrecipitationMassBalanceViolation, got: {err}"
        );
    }
}
