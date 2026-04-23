//! Erosion and coast-type invariants — height explosion guard, sea-crossing
//! fraction, sediment bounds, deposition zone fractions, and coast-type
//! well-formedness (both v1 and v2).

use crate::preset::IslandAge;
use crate::world::WorldState;

use super::{
    DEPOSITION_FLAG_THRESHOLD, DEPOSITION_ZONE_FRACTION_HI, DEPOSITION_ZONE_FRACTION_LO,
    EROSION_MAX_GROWTH_FACTOR, EROSION_MAX_SEA_CROSSING_FRACTION, ValidationError,
};

/// Every coast cell's `coast_type` byte must be in `0..=4`; every non-coast
/// cell must carry the sentinel `0xFF` (`CoastType::Unknown`).
///
/// Sprint 3 DD6 widened the legal range from `0..=3` to `0..=4` when
/// [`crate::world::CoastType::LavaDelta`] (discriminant 4) was added. The
/// Sprint 2 v1 classifier never emits discriminant 4; the Sprint 3 v2
/// classifier may emit it on Young presets near volcanic centers.
///
/// Returns `Ok(())` immediately if either `derived.coast_mask` or
/// `derived.coast_type` is `None` (stage hasn't run yet — skip rather than
/// error).
pub fn coast_type_well_formed(world: &WorldState) -> Result<(), ValidationError> {
    let coast_mask = match world.derived.coast_mask.as_ref() {
        Some(m) => m,
        None => return Ok(()),
    };
    let coast_type = match world.derived.coast_type.as_ref() {
        Some(ct) => ct,
        None => return Ok(()),
    };

    for (i, (&is_coast, &ct_value)) in coast_mask
        .is_coast
        .data
        .iter()
        .zip(coast_type.data.iter())
        .enumerate()
    {
        // Sprint 3 DD6: widened from `> 3` to `> 4` to admit LavaDelta.
        // The 0xFF Unknown sentinel on a coast cell still fails (0xFF > 4).
        if is_coast == 1 && ct_value > 4 {
            return Err(ValidationError::CoastTypeOutOfRange {
                cell_index: i,
                value: ct_value,
            });
        } else if is_coast != 1 && ct_value != 0xFF {
            return Err(ValidationError::NonCoastCellNotUnknown {
                cell_index: i,
                value: ct_value,
            });
        }
    }

    Ok(())
}

/// Post-erosion height field must be finite everywhere, and the new maximum
/// must not exceed `baseline.max_height_pre * EROSION_MAX_GROWTH_FACTOR`.
///
/// Returns `Ok(())` immediately if `authoritative.height` or
/// `derived.erosion_baseline` is `None` (skip).
pub fn erosion_no_explosion(world: &WorldState) -> Result<(), ValidationError> {
    let height = match world.authoritative.height.as_ref() {
        Some(h) => h,
        None => return Ok(()),
    };
    let baseline = match world.derived.erosion_baseline.as_ref() {
        Some(b) => b,
        None => return Ok(()),
    };

    let mut max_now = f32::NEG_INFINITY;
    for (i, &v) in height.data.iter().enumerate() {
        if !v.is_finite() {
            return Err(ValidationError::ErosionHeightNonFinite {
                cell_index: i,
                value: v,
            });
        }
        if v > max_now {
            max_now = v;
        }
    }

    let ceiling = baseline.max_height_pre * EROSION_MAX_GROWTH_FACTOR;
    if max_now > ceiling {
        return Err(ValidationError::ErosionExplosion {
            max_pre: baseline.max_height_pre,
            max_post: max_now,
            factor: EROSION_MAX_GROWTH_FACTOR,
        });
    }

    Ok(())
}

/// The fraction of land cells that crossed the sea-level threshold during
/// erosion must not exceed `EROSION_MAX_SEA_CROSSING_FRACTION`.
///
/// Returns `Ok(())` immediately if `derived.coast_mask` or
/// `derived.erosion_baseline` is `None`, or if `baseline.land_cell_count_pre
/// == 0` (all-sea preset; skip).
pub fn erosion_no_excessive_sea_crossing(world: &WorldState) -> Result<(), ValidationError> {
    let coast_mask = match world.derived.coast_mask.as_ref() {
        Some(m) => m,
        None => return Ok(()),
    };
    let baseline = match world.derived.erosion_baseline.as_ref() {
        Some(b) => b,
        None => return Ok(()),
    };

    let pre = baseline.land_cell_count_pre;
    if pre == 0 {
        return Ok(());
    }

    let post = coast_mask.land_cell_count;
    let delta = (pre as i64 - post as i64).unsigned_abs() as f32;
    let fraction = delta / pre as f32;

    if fraction > EROSION_MAX_SEA_CROSSING_FRACTION {
        return Err(ValidationError::ErosionExcessiveSeaCrossing {
            pre_land: pre,
            post_land: post,
            fraction,
        });
    }

    Ok(())
}

/// Every land cell's sediment thickness `hs` must be finite and in `[0, 1]`;
/// every sea cell must have `hs == 0.0`.
///
/// Returns `Err(SedimentFieldMissing)` if `authoritative.sediment` is `None`
/// (the Sprint 3 init hook in `SedimentUpdateStage` must have run).
///
/// # Errors
///
/// * [`ValidationError::SedimentFieldMissing`] — field is absent.
/// * [`ValidationError::SedimentOutOfRange`] — land cell `hs` outside `[0, 1]` or non-finite.
/// * [`ValidationError::SedimentSeaCellNonZero`] — sea cell `hs != 0.0`.
pub fn sediment_bounded(world: &WorldState) -> Result<(), ValidationError> {
    let sediment = world
        .authoritative
        .sediment
        .as_ref()
        .ok_or(ValidationError::SedimentFieldMissing)?;

    let coast_mask =
        world
            .derived
            .coast_mask
            .as_ref()
            .ok_or(ValidationError::MissingPrecondition {
                field: "derived.coast_mask",
            })?;

    for (i, (&hs, &is_sea)) in sediment
        .data
        .iter()
        .zip(coast_mask.is_sea.data.iter())
        .enumerate()
    {
        if is_sea == 1 {
            // Sea cells must be exactly zero.
            if hs != 0.0 {
                return Err(ValidationError::SedimentSeaCellNonZero {
                    cell_index: i,
                    value: hs,
                });
            }
        } else {
            // Land cells (including coast): finite and in [0, 1].
            if !hs.is_finite() || !(0.0..=1.0).contains(&hs) {
                return Err(ValidationError::SedimentOutOfRange {
                    cell_index: i,
                    value: hs,
                });
            }
        }
    }

    Ok(())
}

/// The fraction of land cells with `hs > `[`DEPOSITION_FLAG_THRESHOLD`] must
/// lie within `[`[`DEPOSITION_ZONE_FRACTION_LO`]`, `[`DEPOSITION_ZONE_FRACTION_HI`]`]`.
///
/// In v1 `DEPOSITION_ZONE_FRACTION_LO = 0.0`, so only the upper bound is
/// actively enforced. At current SPACE-lite parameter calibration, transport
/// capacity generally exceeds incoming Qs on 64²–128² grids, producing
/// near-zero deposition fractions on stock presets. The lower bound will be
/// tightened in Task 3.10 once 256² hero-shot calibration is complete.
///
/// Returns `Ok(())` immediately if `land_cell_count == 0` (all-sea preset;
/// no deposition zones to fraction-count).
///
/// # Errors
///
/// * [`ValidationError::SedimentFieldMissing`] — `authoritative.sediment` absent.
/// * [`ValidationError::MissingPrecondition`] — `derived.coast_mask` absent.
/// * [`ValidationError::DepositionZoneFractionOutOfRange`] — fraction outside `[LO, HI]`.
pub fn deposition_zone_fraction_realistic(world: &WorldState) -> Result<(), ValidationError> {
    let sediment = world
        .authoritative
        .sediment
        .as_ref()
        .ok_or(ValidationError::SedimentFieldMissing)?;

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

    let deposition_count = sediment
        .data
        .iter()
        .zip(coast_mask.is_land.data.iter())
        .filter(|&(&hs, &is_land)| is_land == 1 && hs > DEPOSITION_FLAG_THRESHOLD)
        .count() as u32;

    let fraction = deposition_count as f32 / land_total as f32;

    if !(DEPOSITION_ZONE_FRACTION_LO..=DEPOSITION_ZONE_FRACTION_HI).contains(&fraction) {
        return Err(ValidationError::DepositionZoneFractionOutOfRange {
            fraction,
            threshold: DEPOSITION_FLAG_THRESHOLD,
            lo: DEPOSITION_ZONE_FRACTION_LO,
            hi: DEPOSITION_ZONE_FRACTION_HI,
        });
    }

    Ok(())
}

/// Sprint 3 Task 3.9 additive coast-type constraint.
///
/// Enforces two sub-invariants on top of [`coast_type_well_formed`]:
/// 1. Every coast cell (`is_coast == 1`) has discriminant in `0..=4`.
/// 2. `LavaDelta` (discriminant 4) may only appear when
///    `preset.island_age == IslandAge::Young`. Mature and Old presets must
///    have zero LavaDelta cells.
///
/// This is a **separate, additive** invariant — [`coast_type_well_formed`]
/// is not replaced or modified.
///
/// Returns `Ok(())` immediately if either `derived.coast_mask` or
/// `derived.coast_type` is `None` (self-skipping; stage hasn't run).
///
/// # Errors
///
/// * [`ValidationError::CoastTypeV2DiscOutOfRange`] — coast cell discriminant > 4.
/// * [`ValidationError::LavaDeltaOnNonYoungPreset`] — LavaDelta cells on Mature / Old island.
pub fn coast_type_v2_well_formed(world: &WorldState) -> Result<(), ValidationError> {
    let coast_mask = match world.derived.coast_mask.as_ref() {
        Some(m) => m,
        None => return Ok(()),
    };
    let coast_type = match world.derived.coast_type.as_ref() {
        Some(ct) => ct,
        None => return Ok(()),
    };

    const LAVA_DELTA_DISC: u8 = 4;
    let mut lava_delta_count: usize = 0;

    for (i, (&is_coast, &ct_value)) in coast_mask
        .is_coast
        .data
        .iter()
        .zip(coast_type.data.iter())
        .enumerate()
    {
        if is_coast == 1 {
            if ct_value > LAVA_DELTA_DISC {
                return Err(ValidationError::CoastTypeV2DiscOutOfRange {
                    cell_index: i,
                    value: ct_value,
                });
            }
            if ct_value == LAVA_DELTA_DISC {
                lava_delta_count += 1;
            }
        }
    }

    // Young-only constraint: non-Young presets must have zero LavaDelta cells.
    if lava_delta_count > 0 && world.preset.island_age != IslandAge::Young {
        return Err(ValidationError::LavaDeltaOnNonYoungPreset {
            count: lava_delta_count,
            island_age: world.preset.island_age,
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
    use crate::seed::Seed;
    use crate::test_support::test_preset;
    use crate::world::{CoastMask, ErosionBaseline, Resolution, WorldState};

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

    // Helper: build a WorldState with coast_mask + coast_type for well-formed checks.
    fn make_coast_type_world(
        w: u32,
        h: u32,
        is_coast_data: Vec<u8>,
        coast_type_data: Vec<u8>,
    ) -> WorldState {
        let n = (w * h) as usize;
        let is_land: Vec<u8> = is_coast_data.to_vec();
        let is_sea = vec![0u8; n];
        let mut world = WorldState::new(Seed(0), test_preset(), Resolution::new(w, h));
        world.derived.coast_mask = Some(make_coast_mask(w, h, is_land, is_sea, is_coast_data));
        let mut ct = ScalarField2D::<u8>::new(w, h);
        ct.data = coast_type_data;
        world.derived.coast_type = Some(ct);
        world
    }

    // ── 11: coast_type_well_formed — happy path ───────────────────────────────
    //
    // 5 coast cells with types 0/1/2/3/4 respectively. All valid after the
    // Sprint 3 DD6 widening from `0..=3` to `0..=4` (LavaDelta = 4).
    #[test]
    fn coast_type_well_formed_passes_when_coast_cells_have_valid_types() {
        let world = make_coast_type_world(5, 1, vec![1, 1, 1, 1, 1], vec![0, 1, 2, 3, 4]);
        assert!(
            coast_type_well_formed(&world).is_ok(),
            "expected Ok for coast types 0..=4 (Sprint 3 DD6 range)"
        );
    }

    // ── 11b: coast_type_well_formed accepts LavaDelta (Sprint 3 DD6) ─────────
    //
    // Regression guard for the 0..=3 → 0..=4 widening: a coast cell carrying
    // discriminant 4 (LavaDelta) must validate.
    #[test]
    fn coast_type_well_formed_accepts_lava_delta() {
        let world = make_coast_type_world(2, 1, vec![1, 1], vec![0, 4]);
        assert!(
            coast_type_well_formed(&world).is_ok(),
            "LavaDelta (disc=4) must be accepted by the Sprint 3 DD6-widened invariant"
        );
    }

    // ── 11c: coast_type_well_formed still rejects disc=5 ──────────────────────
    //
    // The widening is exactly one slot; disc=5 has no CoastType variant and
    // must still be flagged as out-of-range.
    #[test]
    fn coast_type_well_formed_rejects_disc_five() {
        let world = make_coast_type_world(2, 1, vec![1, 1], vec![0, 5]);
        let err = coast_type_well_formed(&world).unwrap_err();
        assert!(
            matches!(
                err,
                ValidationError::CoastTypeOutOfRange {
                    cell_index: 1,
                    value: 5
                }
            ),
            "disc=5 must still be rejected (no CoastType variant), got: {err}"
        );
    }

    // ── 12: coast_type_well_formed — failure: coast cell with 0xFF ────────────
    //
    // Coast cell at index 2 has 0xFF (Unknown sentinel), which is invalid for
    // a coast cell.
    #[test]
    fn coast_type_well_formed_fails_on_coast_cell_with_0xff() {
        let world = make_coast_type_world(4, 1, vec![1, 1, 1, 0], vec![0, 1, 0xFF, 0xFF]);
        let err = coast_type_well_formed(&world).unwrap_err();
        assert!(
            matches!(
                err,
                ValidationError::CoastTypeOutOfRange {
                    cell_index: 2,
                    value: 0xFF
                }
            ),
            "expected CoastTypeOutOfRange at index 2, got: {err}"
        );
    }

    // ── 12b: coast_type_well_formed — failure: non-coast cell with valid variant ─
    //
    // Non-coast cell at index 2 has 0x01 (Beach) instead of the Unknown sentinel.
    // Guards against a classifier that forgets to initialise non-coast cells.
    #[test]
    fn coast_type_well_formed_fails_on_non_coast_cell_with_valid_variant() {
        let world = make_coast_type_world(4, 1, vec![1, 0, 0, 0], vec![0, 0xFF, 0x01, 0xFF]);
        let err = coast_type_well_formed(&world).unwrap_err();
        assert!(
            matches!(
                err,
                ValidationError::NonCoastCellNotUnknown {
                    cell_index: 2,
                    value: 0x01
                }
            ),
            "expected NonCoastCellNotUnknown at index 2, got: {err}"
        );
    }

    // Helper: build a WorldState with height + erosion_baseline.
    fn make_erosion_world(
        w: u32,
        h: u32,
        height_data: Vec<f32>,
        baseline: ErosionBaseline,
    ) -> WorldState {
        let mut world = WorldState::new(Seed(0), test_preset(), Resolution::new(w, h));
        let mut height = ScalarField2D::<f32>::new(w, h);
        height.data = height_data;
        world.authoritative.height = Some(height);
        world.derived.erosion_baseline = Some(baseline);
        world
    }

    // ── 13: erosion_no_explosion — passes when max is within 1.05x ───────────
    #[test]
    fn erosion_no_explosion_passes_at_baseline() {
        // baseline.max_height_pre = 1.0, current max = 0.95: within 1.05x ceiling.
        let world = make_erosion_world(
            2,
            1,
            vec![0.95, 0.8],
            ErosionBaseline {
                max_height_pre: 1.0,
                land_cell_count_pre: 2,
            },
        );
        assert!(
            erosion_no_explosion(&world).is_ok(),
            "expected Ok when max is below 1.05x baseline"
        );
    }

    // ── 14: erosion_no_explosion — fails when max exceeds 1.05x ──────────────
    #[test]
    fn erosion_no_explosion_fails_beyond_factor() {
        // baseline.max_height_pre = 1.0, current max = 1.10: exceeds 1.05x ceiling.
        let world = make_erosion_world(
            2,
            1,
            vec![1.10, 0.5],
            ErosionBaseline {
                max_height_pre: 1.0,
                land_cell_count_pre: 2,
            },
        );
        let err = erosion_no_explosion(&world).unwrap_err();
        assert!(
            matches!(err, ValidationError::ErosionExplosion { .. }),
            "expected ErosionExplosion, got: {err}"
        );
    }

    // Helper: build a WorldState with coast_mask + erosion_baseline for
    // sea-crossing checks (height not needed).
    fn make_sea_crossing_world(pre_land: u32, post_land: u32) -> WorldState {
        let total = pre_land.max(post_land).max(1);
        let w = total;
        let h = 1;
        let n = total as usize;

        let is_land: Vec<u8> = (0..n)
            .map(|i| if i < post_land as usize { 1 } else { 0 })
            .collect();
        let is_sea: Vec<u8> = is_land.iter().map(|&v| 1 - v).collect();
        let is_coast = vec![0u8; n];

        let mut world = WorldState::new(Seed(0), test_preset(), Resolution::new(w, h));
        world.derived.coast_mask = Some(make_coast_mask(w, h, is_land, is_sea, is_coast));
        world.derived.erosion_baseline = Some(ErosionBaseline {
            max_height_pre: 1.0,
            land_cell_count_pre: pre_land,
        });
        world
    }

    // ── 15: erosion_no_excessive_sea_crossing — passes at 3 % ────────────────
    //
    // pre = 1000, post = 970 → 3.0 % delta, below the 5 % limit.
    #[test]
    fn erosion_no_excessive_sea_crossing_passes_at_3_percent() {
        let world = make_sea_crossing_world(1000, 970);
        assert!(
            erosion_no_excessive_sea_crossing(&world).is_ok(),
            "expected Ok for 3% sea crossing"
        );
    }

    // ── 16: erosion_no_excessive_sea_crossing — fails at 7 % ─────────────────
    //
    // pre = 1000, post = 930 → 7.0 % delta, above the 5 % limit.
    #[test]
    fn erosion_no_excessive_sea_crossing_fails_at_7_percent() {
        let world = make_sea_crossing_world(1000, 930);
        let err = erosion_no_excessive_sea_crossing(&world).unwrap_err();
        assert!(
            matches!(
                err,
                ValidationError::ErosionExcessiveSeaCrossing {
                    pre_land: 1000,
                    post_land: 930,
                    ..
                }
            ),
            "expected ErosionExcessiveSeaCrossing, got: {err}"
        );
    }

    // ── bonus: skip when erosion_baseline is None ─────────────────────────────
    #[test]
    fn erosion_no_explosion_skips_when_baseline_missing() {
        // height is present but erosion_baseline is None — should skip (Ok).
        let mut world = WorldState::new(Seed(0), test_preset(), Resolution::new(2, 1));
        let mut h = ScalarField2D::<f32>::new(2, 1);
        h.data = vec![5.0, 5.0]; // would be "explosive" if baseline were 1.0
        world.authoritative.height = Some(h);
        assert!(
            erosion_no_explosion(&world).is_ok(),
            "expected Ok when baseline is missing (ErosionOuterLoop not yet run)"
        );
    }

    // ── bonus: NaN in height triggers ErosionHeightNonFinite ─────────────────
    #[test]
    fn erosion_no_explosion_detects_nan_height() {
        let world = make_erosion_world(
            2,
            1,
            vec![f32::NAN, 0.5],
            ErosionBaseline {
                max_height_pre: 1.0,
                land_cell_count_pre: 2,
            },
        );
        let err = erosion_no_explosion(&world).unwrap_err();
        assert!(
            matches!(
                err,
                ValidationError::ErosionHeightNonFinite { cell_index: 0, .. }
            ),
            "expected ErosionHeightNonFinite at cell 0, got: {err}"
        );
    }

    /// Build a minimal world with `coast_mask` (all-land), `sediment`, and
    /// `baked.precipitation` for Sprint 3 invariant unit tests.
    fn make_sprint3_world(
        w: u32,
        h: u32,
        is_land: Vec<u8>,
        is_sea: Vec<u8>,
        sediment_data: Vec<f32>,
        precip_data: Vec<f32>,
    ) -> WorldState {
        let n = (w * h) as usize;
        let is_coast = vec![0u8; n];
        let mut world = WorldState::new(Seed(0), test_preset(), Resolution::new(w, h));
        world.derived.coast_mask = Some(make_coast_mask(w, h, is_land, is_sea, is_coast));
        let mut sed = ScalarField2D::<f32>::new(w, h);
        sed.data = sediment_data;
        world.authoritative.sediment = Some(sed);
        let mut precip = ScalarField2D::<f32>::new(w, h);
        precip.data = precip_data;
        world.baked.precipitation = Some(precip);
        world
    }

    // ── sediment_bounded: test 1 — all-land world with valid hs ──────────────
    #[test]
    fn sediment_bounded_accepts_valid_land_state() {
        let n = 4usize;
        let world = make_sprint3_world(
            2,
            2,
            vec![1u8; n],
            vec![0u8; n],
            vec![0.0, 0.1, 0.5, 1.0],
            vec![0.3; n],
        );
        assert!(
            sediment_bounded(&world).is_ok(),
            "valid sediment values in [0,1] on land cells must pass"
        );
    }

    // ── sediment_bounded: test 2 — NaN triggers error ────────────────────────
    #[test]
    fn sediment_bounded_rejects_nan() {
        let n = 4usize;
        let world = make_sprint3_world(
            2,
            2,
            vec![1u8; n],
            vec![0u8; n],
            vec![f32::NAN, 0.1, 0.5, 0.9],
            vec![0.3; n],
        );
        let err = sediment_bounded(&world).unwrap_err();
        assert!(
            matches!(
                err,
                ValidationError::SedimentOutOfRange { cell_index: 0, .. }
            ),
            "NaN hs must fire SedimentOutOfRange at cell 0, got: {err}"
        );
    }

    // ── sediment_bounded: test 3 — hs > 1.0 triggers error ──────────────────
    #[test]
    fn sediment_bounded_rejects_above_upper() {
        let n = 4usize;
        let world = make_sprint3_world(
            2,
            2,
            vec![1u8; n],
            vec![0u8; n],
            vec![1.5, 0.1, 0.5, 0.9],
            vec![0.3; n],
        );
        let err = sediment_bounded(&world).unwrap_err();
        assert!(
            matches!(
                err,
                ValidationError::SedimentOutOfRange { cell_index: 0, .. }
            ),
            "hs=1.5 must fire SedimentOutOfRange at cell 0, got: {err}"
        );
    }

    // ── sediment_bounded: test 4 — sea cell with non-zero hs ─────────────────
    //
    // 2×1: cell 0 is sea, cell 1 is land. Sea cell has hs=0.1 → must fire.
    #[test]
    fn sediment_bounded_rejects_sea_cell_nonzero() {
        let world = make_sprint3_world(
            2,
            1,
            vec![0u8, 1u8], // is_land
            vec![1u8, 0u8], // is_sea
            vec![0.1, 0.3], // sediment — sea cell has 0.1 (wrong)
            vec![0.3, 0.3],
        );
        let err = sediment_bounded(&world).unwrap_err();
        assert!(
            matches!(
                err,
                ValidationError::SedimentSeaCellNonZero { cell_index: 0, .. }
            ),
            "sea cell with hs=0.1 must fire SedimentSeaCellNonZero at cell 0, got: {err}"
        );
    }

    // ── sediment_bounded: test 5 — missing sediment field returns error ───────
    #[test]
    fn sediment_bounded_missing_field_returns_error() {
        let n = 4usize;
        let mut world = WorldState::new(Seed(0), test_preset(), Resolution::new(2, 2));
        world.derived.coast_mask = Some(make_coast_mask(
            2,
            2,
            vec![1u8; n],
            vec![0u8; n],
            vec![0u8; n],
        ));
        // Do NOT set authoritative.sediment.
        let err = sediment_bounded(&world).unwrap_err();
        assert!(
            matches!(err, ValidationError::SedimentFieldMissing),
            "missing sediment must fire SedimentFieldMissing, got: {err}"
        );
    }

    // ── deposition_zone_fraction: test 1 — nominal fraction in [LO, HI] ──────
    //
    // 10 land cells: 3 with hs > 0.15 → fraction = 0.30 ∈ [0.05, 0.70].
    #[test]
    fn deposition_zone_fraction_realistic_accepts_nominal() {
        let n = 10usize;
        let mut sed = vec![0.05f32; n]; // all below threshold initially
        sed[0] = 0.20; // above DEPOSITION_FLAG_THRESHOLD = 0.15
        sed[4] = 0.50;
        sed[7] = 0.80;
        let world = make_sprint3_world(10, 1, vec![1u8; n], vec![0u8; n], sed, vec![0.3; n]);
        assert!(
            deposition_zone_fraction_realistic(&world).is_ok(),
            "30% deposition fraction must be accepted (in [0.0, 0.70])"
        );
    }

    // ── deposition_zone_fraction: test 2 — zero deposition fraction is accepted
    //
    // At v1 SPACE-lite calibration, small grids often produce 0% deposition
    // (transport capacity exceeds incoming Qs). LO = 0.0 so this must pass.
    // The lower bound will be tightened in Task 3.10 once the 256² deposition
    // physics is calibrated.
    #[test]
    fn deposition_zone_fraction_realistic_accepts_zero_at_v1_lo_bound() {
        let n = 10usize;
        let world = make_sprint3_world(
            10,
            1,
            vec![1u8; n],
            vec![0u8; n],
            vec![0.05f32; n], // all below threshold → fraction = 0.0
            vec![0.3; n],
        );
        // With LO = 0.0, fraction 0.0 is valid (no lower bound enforced in v1).
        assert!(
            deposition_zone_fraction_realistic(&world).is_ok(),
            "fraction 0.0 must be accepted when DEPOSITION_ZONE_FRACTION_LO = 0.0 (v1 calibration)"
        );
    }

    // ── deposition_zone_fraction: test 3 — saturated (fraction = 1.0) ────────
    //
    // All land cells have hs = 1.0 (above threshold) → fraction = 1.0 > HI.
    #[test]
    fn deposition_zone_fraction_realistic_rejects_saturated() {
        let n = 10usize;
        let world = make_sprint3_world(
            10,
            1,
            vec![1u8; n],
            vec![0u8; n],
            vec![1.0f32; n], // all above threshold
            vec![0.3; n],
        );
        let err = deposition_zone_fraction_realistic(&world).unwrap_err();
        assert!(
            matches!(
                err,
                ValidationError::DepositionZoneFractionOutOfRange { fraction, .. }
                if fraction == 1.0
            ),
            "100% fraction must fire DepositionZoneFractionOutOfRange, got: {err}"
        );
    }

    // ── deposition_zone_fraction: test 4 — all-sea world returns Ok ──────────
    #[test]
    fn deposition_zone_fraction_realistic_accepts_empty_land() {
        let n = 4usize;
        // All-sea world: land_cell_count == 0.
        let world = make_sprint3_world(
            2,
            2,
            vec![0u8; n], // no land
            vec![1u8; n], // all sea
            vec![0.0f32; n],
            vec![0.0f32; n],
        );
        assert!(
            deposition_zone_fraction_realistic(&world).is_ok(),
            "all-sea world (land_count=0) must return Ok immediately"
        );
    }

    // ── coast_type_v2_well_formed: helpers ────────────────────────────────────

    fn make_v2_coast_world(
        w: u32,
        h: u32,
        is_coast_data: Vec<u8>,
        coast_type_data: Vec<u8>,
        island_age: IslandAge,
    ) -> WorldState {
        let n = (w * h) as usize;
        let is_land = is_coast_data.clone();
        let is_sea = vec![0u8; n];
        let mut preset = test_preset();
        preset.island_age = island_age;
        let mut world = WorldState::new(Seed(0), preset, Resolution::new(w, h));
        world.derived.coast_mask = Some(make_coast_mask(w, h, is_land, is_sea, is_coast_data));
        let mut ct = ScalarField2D::<u8>::new(w, h);
        ct.data = coast_type_data;
        world.derived.coast_type = Some(ct);
        world
    }

    // ── coast_type_v2_well_formed: test 1 — Young with LavaDelta ────────────
    #[test]
    fn coast_type_v2_well_formed_accepts_young_with_lava_delta() {
        // 3 coast cells: types Beach(1), RockyHeadland(3), LavaDelta(4).
        // Young preset → LavaDelta is allowed.
        let world = make_v2_coast_world(3, 1, vec![1, 1, 1], vec![1, 3, 4], IslandAge::Young);
        assert!(
            coast_type_v2_well_formed(&world).is_ok(),
            "Young preset with LavaDelta must pass v2 invariant"
        );
    }

    // ── coast_type_v2_well_formed: test 2 — Mature with LavaDelta rejects ────
    #[test]
    fn coast_type_v2_well_formed_rejects_mature_with_lava_delta() {
        let world = make_v2_coast_world(3, 1, vec![1, 1, 1], vec![1, 3, 4], IslandAge::Mature);
        let err = coast_type_v2_well_formed(&world).unwrap_err();
        assert!(
            matches!(
                err,
                ValidationError::LavaDeltaOnNonYoungPreset {
                    count: 1,
                    island_age: IslandAge::Mature,
                }
            ),
            "Mature preset with LavaDelta must fire LavaDeltaOnNonYoungPreset, got: {err}"
        );
    }

    // ── coast_type_v2_well_formed: test 3 — Mature without LavaDelta passes ──
    #[test]
    fn coast_type_v2_well_formed_accepts_mature_without_lava() {
        let world = make_v2_coast_world(3, 1, vec![1, 1, 1], vec![1, 2, 3], IslandAge::Mature);
        assert!(
            coast_type_v2_well_formed(&world).is_ok(),
            "Mature preset without LavaDelta must pass v2 invariant"
        );
    }

    // ── coast_type_v2_well_formed: test 4 — discriminant 5 on coast cell ─────
    #[test]
    fn coast_type_v2_well_formed_rejects_disc_five_on_coast() {
        let world = make_v2_coast_world(2, 1, vec![1, 1], vec![0, 5], IslandAge::Young);
        let err = coast_type_v2_well_formed(&world).unwrap_err();
        assert!(
            matches!(
                err,
                ValidationError::CoastTypeV2DiscOutOfRange {
                    cell_index: 1,
                    value: 5
                }
            ),
            "disc=5 on a coast cell must fire CoastTypeV2DiscOutOfRange, got: {err}"
        );
    }

    // ── coast_type_v2_well_formed: test 5 — Old with LavaDelta rejects ────────
    #[test]
    fn coast_type_v2_well_formed_rejects_old_with_lava_delta() {
        let world = make_v2_coast_world(2, 1, vec![1, 1], vec![1, 4], IslandAge::Old);
        let err = coast_type_v2_well_formed(&world).unwrap_err();
        assert!(
            matches!(
                err,
                ValidationError::LavaDeltaOnNonYoungPreset {
                    island_age: IslandAge::Old,
                    ..
                }
            ),
            "Old preset with LavaDelta must fire LavaDeltaOnNonYoungPreset, got: {err}"
        );
    }
}
