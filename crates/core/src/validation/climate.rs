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

/// Von4-distance at which the Sprint 3.5.D DD6 coastal-margin SM floor
/// stops applying. Mirrors `COASTAL_MARGIN_MAX_DIST` in
/// `crates/sim/src/hydro/soil_moisture.rs`. Kept as a private const here
/// to avoid a `core → sim` dep edge (core is the sink).
const COASTAL_MARGIN_MAX_DIST: u32 = 3;

/// Floor value the DD6 change lifts every Von4 ≤ `COASTAL_MARGIN_MAX_DIST`
/// land cell to. Mirrors `COASTAL_MARGIN_SM_FLOOR` in
/// `crates/sim/src/hydro/soil_moisture.rs`. Absolute tolerance for the
/// validator check is `COASTAL_MARGIN_SM_FLOOR - EPSILON` to absorb f32
/// round-trip noise.
const COASTAL_MARGIN_SM_FLOOR: f32 = 0.25;
const COASTAL_MARGIN_FLOOR_EPSILON: f32 = 1e-6;

/// Sprint 3.5.D §4 invariant #6: after `SoilMoistureStage::run`, every
/// land cell whose Von4-distance to nearest sea cell is ≤ 3 must have
/// `baked.soil_moisture >= 0.25 - EPSILON`. Protects DD6 Change 1 from
/// silent regression (e.g. if a future edit inadvertently reorders the
/// floor branch before LFPM + fog coupling, or drops the floor entirely).
///
/// Skip-if-missing: `baked.soil_moisture = None` OR `derived.coast_mask
/// = None` → Ok, matching the other Sprint 3 climate / hydro validators.
///
/// Re-uses the same multi-source Von4 BFS pattern from
/// `SoilMoistureStage::run`'s DD6 branch so the validator and the stage
/// can't drift on distance semantics.
///
/// # Errors
///
/// * [`ValidationError::CoastalMarginSmFloorMissed`] — at least one land
///   cell within Von4-distance ≤ 3 has soil_moisture below the floor.
pub fn coastal_margin_sm_floor_applied(world: &WorldState) -> Result<(), ValidationError> {
    let Some(soil_moisture) = world.baked.soil_moisture.as_ref() else {
        return Ok(());
    };
    let Some(coast_mask) = world.derived.coast_mask.as_ref() else {
        return Ok(());
    };

    let w = coast_mask.is_land.width;
    let h = coast_mask.is_land.height;
    if w == 0 || h == 0 {
        return Ok(());
    }

    // Multi-source Von4 BFS from sea cells. distance[cell] = 0 on sea,
    // then 1..=MAX for land cells within the coastal margin, and u32::MAX
    // for land cells further inland (never reached by the bounded BFS).
    let mut distance = vec![u32::MAX; (w * h) as usize];
    let mut frontier: Vec<(u32, u32)> = Vec::new();
    for iy in 0..h {
        for ix in 0..w {
            if coast_mask.is_land.get(ix, iy) == 0 {
                distance[(iy * w + ix) as usize] = 0;
                frontier.push((ix, iy));
            }
        }
    }

    const VON4: [(i32, i32); 4] = [(-1, 0), (1, 0), (0, -1), (0, 1)];
    let mut next: Vec<(u32, u32)> = Vec::new();
    for dist in 1..=COASTAL_MARGIN_MAX_DIST {
        for &(cx, cy) in &frontier {
            for (dx, dy) in VON4 {
                let nx = cx as i32 + dx;
                let ny = cy as i32 + dy;
                if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                    continue;
                }
                let (nxu, nyu) = (nx as u32, ny as u32);
                let nidx = (nyu * w + nxu) as usize;
                if distance[nidx] != u32::MAX {
                    continue;
                }
                if coast_mask.is_land.get(nxu, nyu) != 1 {
                    continue;
                }
                distance[nidx] = dist;
                next.push((nxu, nyu));
            }
        }
        frontier.clear();
        std::mem::swap(&mut frontier, &mut next);
    }

    // Check every Von4 ≤ MAX_DIST land cell against the floor.
    for iy in 0..h {
        for ix in 0..w {
            let idx = (iy * w + ix) as usize;
            let d = distance[idx];
            if d == 0 || d == u32::MAX {
                continue; // sea OR inland (beyond floor range)
            }
            let sm = soil_moisture.get(ix, iy);
            if sm < COASTAL_MARGIN_SM_FLOOR - COASTAL_MARGIN_FLOOR_EPSILON {
                return Err(ValidationError::CoastalMarginSmFloorMissed {
                    ix,
                    iy,
                    dist: d,
                    soil_moisture: sm,
                });
            }
        }
    }

    Ok(())
}

// ─── tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::field::{MaskField2D, ScalarField2D};
    use crate::preset::PrecipitationVariant;
    use crate::seed::Seed;
    use crate::test_support::test_preset;
    use crate::world::{BakedSnapshot, CoastMask, Resolution, WorldState};

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

    // ── coastal_margin_sm_floor_applied (Sprint 3.5.D DD6) ────────────────────

    /// Build a 5×5 world with a single sea column on the left (x=0) and
    /// land everywhere else. Von4-distances to sea: x=0 → 0, x=1 → 1,
    /// x=2 → 2, x=3 → 3, x=4 → 4 (outside floor range).
    fn make_coastal_world(w: u32, h: u32, soil_moisture: Vec<f32>) -> WorldState {
        let n = (w * h) as usize;
        let mut is_land = vec![1u8; n];
        let mut is_sea = vec![0u8; n];
        for iy in 0..h {
            let idx = (iy * w) as usize;
            is_land[idx] = 0; // x=0 column is sea
            is_sea[idx] = 1;
        }
        let is_coast = vec![0u8; n];
        let mut world = WorldState::new(Seed(0), test_preset(), Resolution::new(w, h));
        world.derived.coast_mask = Some(make_coast_mask(w, h, is_land, is_sea, is_coast));
        let mut sm_field = ScalarField2D::<f32>::new(w, h);
        sm_field.data = soil_moisture;
        world.baked.soil_moisture = Some(sm_field);
        world
    }

    #[test]
    fn coastal_margin_sm_floor_applied_accepts_valid_world() {
        // 5×5: sea at x=0; land x=1..=3 at SM=0.30 (floor met); x=4 at SM=0.10 (inland, OK).
        let w = 5_u32;
        let h = 5_u32;
        let n = (w * h) as usize;
        let mut sm = vec![0.0_f32; n];
        for iy in 0..h {
            for ix in 0..w {
                let idx = (iy * w + ix) as usize;
                sm[idx] = match ix {
                    0 => 0.0,      // sea — ignored
                    1..=3 => 0.30, // Von4 ≤ 3 — must be ≥ 0.25
                    _ => 0.10,     // inland — floor doesn't apply
                };
            }
        }
        let world = make_coastal_world(w, h, sm);
        assert!(coastal_margin_sm_floor_applied(&world).is_ok());
    }

    #[test]
    fn coastal_margin_sm_floor_applied_rejects_missing_floor() {
        // Land cell at x=2 (Von4-dist=2 from sea) has SM=0.10 — violates floor.
        let w = 5_u32;
        let h = 5_u32;
        let n = (w * h) as usize;
        let mut sm = vec![0.30_f32; n];
        // Zero out x=0 column (sea); break the floor on x=2 row 0.
        for iy in 0..h {
            sm[(iy * w) as usize] = 0.0;
        }
        sm[2] = 0.10; // (ix=2, iy=0): Von4-dist=2 from sea, SM below floor
        let world = make_coastal_world(w, h, sm);
        let err = coastal_margin_sm_floor_applied(&world).unwrap_err();
        match err {
            ValidationError::CoastalMarginSmFloorMissed {
                ix,
                iy,
                dist,
                soil_moisture,
            } => {
                assert_eq!(ix, 2);
                assert_eq!(iy, 0);
                assert_eq!(dist, 2);
                assert!((soil_moisture - 0.10).abs() < 1e-6);
            }
            other => panic!("expected CoastalMarginSmFloorMissed, got: {other:?}"),
        }
    }

    #[test]
    fn coastal_margin_sm_floor_applied_skip_if_missing() {
        // baked.soil_moisture = None → Ok
        let world = minimal_world_for_1b(4, 4);
        assert!(coastal_margin_sm_floor_applied(&world).is_ok());
    }

    #[test]
    fn coastal_margin_sm_floor_applied_ignores_interior_cells() {
        // 6×3: sea at x=0; x=4 is Von4-dist=4 (beyond floor range) at SM=0.10.
        // Validator must accept (floor doesn't apply).
        let w = 6_u32;
        let h = 3_u32;
        let n = (w * h) as usize;
        let mut sm = vec![0.30_f32; n];
        for iy in 0..h {
            sm[(iy * w) as usize] = 0.0; // sea column
            sm[(iy * w + 4) as usize] = 0.10; // Von4-dist=4 — inland, floor skipped
            sm[(iy * w + 5) as usize] = 0.10; // Von4-dist=5 — inland, floor skipped
        }
        let world = make_coastal_world(w, h, sm);
        assert!(coastal_margin_sm_floor_applied(&world).is_ok());
    }
}
