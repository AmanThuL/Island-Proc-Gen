//! Hex-surface invariants — shape and biome-weight-vector consistency of the
//! hex attribute grid.
//!
//! `hex_attrs_present` is the only member of this family in v1.
//! Sprint 5 S1's real-hex rework and the hex-grammar extensions planned for
//! Sprint 3.5.D will add further invariants here.

use crate::preset::CoastTypeVariant;
use crate::world::{HexCoastClass, WorldState};

use super::ValidationError;

/// `hex_attrs.attrs.len() == cols * rows`, and every entry's
/// `biome_weights` vector length matches the canonical biome count.
pub fn hex_attrs_present(world: &WorldState) -> Result<(), ValidationError> {
    let attrs = world
        .derived
        .hex_attrs
        .as_ref()
        .ok_or(ValidationError::MissingPrecondition {
            field: "derived.hex_attrs",
        })?;

    let expected = (attrs.cols * attrs.rows) as usize;
    if attrs.attrs.len() != expected {
        return Err(ValidationError::HexAttrsShapeMismatch {
            cols: attrs.cols,
            rows: attrs.rows,
            got: attrs.attrs.len(),
        });
    }

    let expected_biome_count = crate::world::BiomeType::COUNT;
    for (i, hex) in attrs.attrs.iter().enumerate() {
        if hex.biome_weights.len() != expected_biome_count {
            let col = (i as u32) % attrs.cols;
            let row = (i as u32) / attrs.cols;
            return Err(ValidationError::HexBiomeWeightsLengthMismatch {
                col,
                row,
                got: hex.biome_weights.len(),
                expected: expected_biome_count,
            });
        }
    }
    Ok(())
}

/// DD3 (Sprint 3.5.B c1): every `HexRiverCrossing` in
/// `world.derived.hex_debug.river_crossing` must have both `entry_edge` and
/// `exit_edge` in the range `0..=5` (the 6-edge hex encoding per DD1).
///
/// Returns `Ok(())` when `hex_debug` is `None` (skip-if-missing pattern —
/// the same as `hex_attrs_present` when its precondition is absent).
pub fn hex_river_crossing_edges_in_range(world: &WorldState) -> Result<(), ValidationError> {
    let Some(dbg) = world.derived.hex_debug.as_ref() else {
        return Ok(());
    };
    for (hex_id, crossing_opt) in dbg.river_crossing.iter().enumerate() {
        if let Some(rc) = crossing_opt {
            if rc.entry_edge > 5 || rc.exit_edge > 5 {
                return Err(ValidationError::HexRiverCrossingEdgeOutOfRange {
                    hex_id,
                    entry_edge: rc.entry_edge,
                    exit_edge: rc.exit_edge,
                });
            }
        }
    }
    Ok(())
}

/// DD3 (Sprint 3.5.B c3): `river_width[i].is_some() == river_crossing[i].is_some()`
/// for every hex.
///
/// Returns `Ok(())` when `hex_debug` is `None` (skip-if-missing). Also skips
/// when `river_width` is empty (pre-c3 pipelines that populated `river_crossing`
/// before this field existed).
pub fn river_width_matches_crossing_presence(world: &WorldState) -> Result<(), ValidationError> {
    let Some(dbg) = world.derived.hex_debug.as_ref() else {
        return Ok(());
    };
    // Skip if river_width was not populated (pre-c3 compatibility).
    if dbg.river_width.is_empty() {
        return Ok(());
    }
    for (hex_id, (crossing_opt, width_opt)) in dbg
        .river_crossing
        .iter()
        .zip(dbg.river_width.iter())
        .enumerate()
    {
        if crossing_opt.is_some() != width_opt.is_some() {
            return Err(ValidationError::HexRiverWidthCrossingMismatch {
                hex_id,
                has_crossing: crossing_opt.is_some(),
                has_width: width_opt.is_some(),
            });
        }
    }
    Ok(())
}

/// Sprint 3.5.C DD4 invariant #3: every `HexCoastClass` discriminant in
/// `derived.hex_coast_class` must be in range `0..=6` (all known variants of
/// [`HexCoastClass`]), and any hex classified as `LavaDelta` must have at
/// least one underlying sim cell with `CoastType == LavaDelta` (discriminant 4).
///
/// Returns `Ok(())` (skip-if-missing) when any of `hex_coast_class`,
/// `coast_type`, `coast_mask`, or `hex_grid` is `None`.
///
/// # Errors
///
/// * [`ValidationError::HexCoastClassDiscriminantOutOfRange`] — discriminant
///   outside `0..=6`.
/// * [`ValidationError::HexCoastClassLavaDeltaWithoutCellSupport`] — `LavaDelta`
///   hex with no backing cell-level `CoastType::LavaDelta`.
pub fn hex_coast_class_well_formed(world: &WorldState) -> Result<(), ValidationError> {
    let Some(classes) = world.derived.hex_coast_class.as_ref() else {
        return Ok(());
    };
    let Some(grid) = world.derived.hex_grid.as_ref() else {
        return Ok(());
    };
    let Some(coast_type) = world.derived.coast_type.as_ref() else {
        return Ok(());
    };
    let Some(coast_mask) = world.derived.coast_mask.as_ref() else {
        return Ok(());
    };

    // Range check: every discriminant must map to a known HexCoastClass variant.
    for (hex_id, cls) in classes.iter().enumerate() {
        let disc = *cls as u8;
        if HexCoastClass::from_u8(disc).is_none() {
            return Err(ValidationError::HexCoastClassDiscriminantOutOfRange {
                hex_id,
                discriminant: disc,
            });
        }
    }

    // LavaDelta consistency: if a hex is classified LavaDelta, at least one land
    // sim cell inside it must carry CoastType == LavaDelta (discriminant 4).
    // Per Sprint 3 DD6, CoastType::LavaDelta discriminant = 4 (not HexCoastClass::LavaDelta
    // which is 6 — these are two different enums at two different levels).
    const COAST_TYPE_LAVADELTA_DISC: u8 = 4;

    let sim_w = coast_mask.is_land.width;
    let sim_h = coast_mask.is_land.height;
    let hex_count = (grid.cols * grid.rows) as usize;

    // Build a per-hex flag: does any sim cell mapped to this hex carry
    // CoastType == LavaDelta?  We only check cells whose coast_type byte is
    // COAST_TYPE_LAVADELTA_DISC; non-coast cells carry 0xFF (Unknown).
    let mut has_lavadelta_cell = vec![false; hex_count];
    for iy in 0..sim_h {
        for ix in 0..sim_w {
            let flat = coast_mask.is_land.index(ix, iy);
            if coast_type.data[flat] == COAST_TYPE_LAVADELTA_DISC {
                let hex_id = grid.hex_id_of_cell.get(ix, iy) as usize;
                if hex_id < hex_count {
                    has_lavadelta_cell[hex_id] = true;
                }
            }
        }
    }

    for (hex_id, cls) in classes.iter().enumerate() {
        if *cls == HexCoastClass::LavaDelta && !has_lavadelta_cell[hex_id] {
            return Err(ValidationError::HexCoastClassLavaDeltaWithoutCellSupport { hex_id });
        }
    }

    Ok(())
}

/// Sprint 3.5.C DD4 invariant #5: when `derived.hex_coast_class` is
/// non-empty AND the active `coast_type_variant` is `V2FetchIntegral`,
/// `derived.coast_fetch_integral` must be `Some`.
///
/// The V2 hex classifier weights its vote by each cell's fetch-integral
/// value; if the field is absent the classification result is undefined.
/// When the variant is `V1Cheap`, the fetch integral is legitimately `None`
/// (the classifier uses uniform weighting), so the requirement is skipped.
///
/// Returns `Ok(())` (skip-if-missing) when `hex_coast_class` is `None`
/// or empty.
///
/// # Errors
///
/// * [`ValidationError::HexCoastClassRequiresFetchIntegral`] — `hex_coast_class`
///   is non-empty, variant is `V2FetchIntegral`, but `coast_fetch_integral`
///   is `None`.
pub fn hex_coast_class_requires_fetch_integral(world: &WorldState) -> Result<(), ValidationError> {
    let Some(classes) = world.derived.hex_coast_class.as_ref() else {
        return Ok(());
    };
    if classes.is_empty() {
        return Ok(());
    }

    // V1Cheap does not produce a coast_fetch_integral — skip the requirement.
    if world.preset.erosion.coast_type_variant == CoastTypeVariant::V1Cheap {
        return Ok(());
    }

    // V2FetchIntegral: fetch field must be present.
    if world.derived.coast_fetch_integral.is_none() {
        return Err(ValidationError::HexCoastClassRequiresFetchIntegral);
    }

    Ok(())
}

// ─── tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::field::{MaskField2D, ScalarField2D};
    use crate::preset::CoastTypeVariant;
    use crate::seed::Seed;
    use crate::test_support::test_preset;
    use crate::world::{
        BakedSnapshot, CoastMask, HexAttributeField, HexAttributes, HexDebugAttributes, HexGrid,
        HexLayout, HexRiverCrossing, Resolution, RiverWidth, WorldState,
    };

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

    #[test]
    fn hex_attrs_present_happy_path() {
        let mut world = minimal_world_for_1b(4, 4);
        let n_hex = 16;
        let attrs: Vec<HexAttributes> = (0..n_hex)
            .map(|_| HexAttributes {
                elevation: 0.0,
                slope: 0.0,
                rainfall: 0.0,
                temperature: 0.0,
                moisture: 0.0,
                biome_weights: vec![0.0; crate::world::BiomeType::COUNT],
                dominant_biome: crate::world::BiomeType::CoastalScrub,
                has_river: false,
            })
            .collect();
        world.derived.hex_attrs = Some(HexAttributeField {
            attrs,
            cols: 4,
            rows: 4,
        });
        assert!(hex_attrs_present(&world).is_ok());
    }

    #[test]
    fn hex_attrs_present_detects_biome_row_length_mismatch() {
        let mut world = minimal_world_for_1b(4, 4);
        let attrs = (0..16)
            .map(|i| HexAttributes {
                elevation: 0.0,
                slope: 0.0,
                rainfall: 0.0,
                temperature: 0.0,
                moisture: 0.0,
                biome_weights: if i == 5 {
                    vec![0.0; 3] // wrong length on one hex
                } else {
                    vec![0.0; crate::world::BiomeType::COUNT]
                },
                dominant_biome: crate::world::BiomeType::CoastalScrub,
                has_river: false,
            })
            .collect();
        world.derived.hex_attrs = Some(HexAttributeField {
            attrs,
            cols: 4,
            rows: 4,
        });
        let err = hex_attrs_present(&world).unwrap_err();
        assert!(matches!(
            err,
            ValidationError::HexBiomeWeightsLengthMismatch { col: 1, row: 1, .. }
        ));
    }

    // ── hex_river_crossing_edges_in_range tests (DD3, Sprint 3.5.B c1) ────────

    /// Fixture: a minimal WorldState with `hex_debug` populated with a given
    /// `river_crossing` vector. 4×4 hex grid / 4×4 sim domain.
    fn world_with_river_crossings(crossings: Vec<Option<HexRiverCrossing>>) -> WorldState {
        let mut world = minimal_world_for_1b(4, 4);
        let n = crossings.len();
        world.derived.hex_debug = Some(HexDebugAttributes {
            slope_variance: vec![0.0; n],
            accessibility_cost: vec![1.0; n],
            river_crossing: crossings,
            river_width: vec![None; n],
        });
        world
    }

    /// `entry_edge = 6` is out of the 6-edge range and must be rejected.
    #[test]
    fn hex_river_crossing_edges_in_range_rejects_out_of_range_edge() {
        let crossings = vec![
            None,
            Some(HexRiverCrossing {
                entry_edge: 6, // invalid — only 0..=5 are valid hex edges
                exit_edge: 3,
            }),
            None,
        ];
        let world = world_with_river_crossings(crossings);
        let err = hex_river_crossing_edges_in_range(&world).unwrap_err();
        assert!(
            matches!(
                err,
                ValidationError::HexRiverCrossingEdgeOutOfRange {
                    hex_id: 1,
                    entry_edge: 6,
                    exit_edge: 3,
                }
            ),
            "expected HexRiverCrossingEdgeOutOfRange at hex_id 1, got {err:?}"
        );
    }

    /// All 6 valid hex edge values (0..=5 per DD1) must pass.
    #[test]
    fn hex_river_crossing_edges_in_range_accepts_all_6_valid_edges() {
        // Build one crossing per valid edge pair (entry = exit = e as u8).
        let crossings: Vec<Option<HexRiverCrossing>> = (0_u8..=5)
            .map(|e| {
                Some(HexRiverCrossing {
                    entry_edge: e,
                    exit_edge: (e + 1) % 6, // next edge, also valid
                })
            })
            .collect();
        let world = world_with_river_crossings(crossings);
        assert!(
            hex_river_crossing_edges_in_range(&world).is_ok(),
            "all 6 valid hex edges (0..=5) must pass the range validator"
        );
    }

    /// When `hex_debug` is `None`, the validator must return `Ok(())` — skip
    /// if missing, matching the pattern used by `hex_attrs_present`.
    #[test]
    fn hex_river_crossing_edges_in_range_skip_if_missing() {
        let world = minimal_world_for_1b(4, 4);
        // hex_debug is None — validator must be a no-op.
        assert!(world.derived.hex_debug.is_none());
        assert!(hex_river_crossing_edges_in_range(&world).is_ok());
    }

    // ── river_width_matches_crossing_presence tests (DD3, Sprint 3.5.B c3) ──────

    /// Build a world where `river_crossing[i]` and `river_width[i]` are both
    /// provided or both absent for every hex — must pass.
    #[test]
    fn river_width_matches_crossing_presence_happy_path() {
        let crossings = vec![
            None,
            Some(HexRiverCrossing {
                entry_edge: 0,
                exit_edge: 3,
            }),
            None,
        ];
        let n = crossings.len();
        let mut world = minimal_world_for_1b(4, 4);
        world.derived.hex_debug = Some(HexDebugAttributes {
            slope_variance: vec![0.0; n],
            accessibility_cost: vec![1.0; n],
            river_crossing: crossings,
            river_width: vec![None, Some(RiverWidth::Small), None],
        });
        assert!(river_width_matches_crossing_presence(&world).is_ok());
    }

    /// When `river_crossing[i]` is `Some` but `river_width[i]` is `None`,
    /// the validator must return an error.
    #[test]
    fn river_width_matches_crossing_presence_detects_crossing_without_width() {
        let crossings = vec![
            None,
            Some(HexRiverCrossing {
                entry_edge: 1,
                exit_edge: 4,
            }),
        ];
        let n = crossings.len();
        let mut world = minimal_world_for_1b(4, 4);
        world.derived.hex_debug = Some(HexDebugAttributes {
            slope_variance: vec![0.0; n],
            accessibility_cost: vec![1.0; n],
            river_crossing: crossings,
            // hex_id 1 has crossing but no width — mismatch.
            river_width: vec![None, None],
        });
        let err = river_width_matches_crossing_presence(&world).unwrap_err();
        assert!(
            matches!(
                err,
                ValidationError::HexRiverWidthCrossingMismatch {
                    hex_id: 1,
                    has_crossing: true,
                    has_width: false,
                }
            ),
            "expected HexRiverWidthCrossingMismatch at hex_id 1, got {err:?}"
        );
    }

    /// When `hex_debug` is `None`, the validator must return `Ok(())`.
    #[test]
    fn river_width_matches_crossing_presence_skip_if_missing() {
        let world = minimal_world_for_1b(4, 4);
        assert!(world.derived.hex_debug.is_none());
        assert!(river_width_matches_crossing_presence(&world).is_ok());
    }

    // ── hex_coast_class_well_formed tests (DD4, Sprint 3.5.C c4) ─────────────

    /// Build a minimal HexGrid (2×2 hexes covering a 4×4 sim domain, each
    /// 2×2 block of sim cells mapping to one hex).
    fn make_hex_grid_2x2_on_4x4() -> HexGrid {
        // hex_id layout: hex(0,0)=0 (cols 0..2, rows 0..2)
        //                hex(1,0)=1 (cols 2..4, rows 0..2)
        //                hex(0,1)=2 (cols 0..2, rows 2..4)
        //                hex(1,1)=3 (cols 2..4, rows 2..4)
        let mut mapping = ScalarField2D::<u32>::new(4, 4);
        for y in 0u32..4 {
            for x in 0u32..4 {
                let hx = x / 2;
                let hy = y / 2;
                mapping.set(x, y, hy * 2 + hx);
            }
        }
        HexGrid {
            cols: 2,
            rows: 2,
            hex_size: 1.0,
            layout: HexLayout::FlatTop,
            hex_id_of_cell: mapping,
        }
    }

    /// Build a 4×4 coast_type ScalarField2D.  All cells default to 0xFF
    /// (Unknown / non-coast).  Caller sets specific cells to a CoastType disc.
    fn make_coast_type_field_4x4(overrides: &[(u32, u32, u8)]) -> ScalarField2D<u8> {
        let mut ct = ScalarField2D::<u8>::new(4, 4);
        // Fill all with 0xFF (Unknown sentinel for non-coast cells).
        for v in ct.data.iter_mut() {
            *v = 0xFF;
        }
        for &(x, y, val) in overrides {
            ct.set(x, y, val);
        }
        ct
    }

    /// A world where hex 0 is classified Beach with all cells having CoastType
    /// Beach (discriminant 1) — must pass both range and LavaDelta checks.
    #[test]
    fn well_formed_accepts_valid_beach_classification() {
        let mut world = minimal_world_for_1b(4, 4);
        world.derived.hex_grid = Some(make_hex_grid_2x2_on_4x4());
        // coast_type: cells (0,0) and (1,0) are Beach (disc=1), rest Unknown.
        world.derived.coast_type = Some(make_coast_type_field_4x4(&[(0, 0, 1), (1, 0, 1)]));
        // hex_coast_class: hex 0 = Beach (disc=2), hexes 1-3 = Inland (disc=0).
        world.derived.hex_coast_class = Some(vec![
            HexCoastClass::Beach,
            HexCoastClass::Inland,
            HexCoastClass::Inland,
            HexCoastClass::Inland,
        ]);
        assert!(
            hex_coast_class_well_formed(&world).is_ok(),
            "Beach classification with matching cell-level coast_type must pass"
        );
    }

    /// `HexCoastClass::LavaDelta` on hex 0 without any cell-level
    /// `CoastType::LavaDelta` (disc 4) in hex 0's cells must fail.
    #[test]
    fn rejects_lavadelta_without_cell_support() {
        let mut world = minimal_world_for_1b(4, 4);
        world.derived.hex_grid = Some(make_hex_grid_2x2_on_4x4());
        // coast_type: cells in hex 0 are Beach (disc=1), NOT LavaDelta (disc=4).
        world.derived.coast_type = Some(make_coast_type_field_4x4(&[
            (0, 0, 1), // Beach, not LavaDelta
            (1, 0, 1),
        ]));
        // hex_coast_class: hex 0 = LavaDelta — but no cell has CoastType LavaDelta.
        world.derived.hex_coast_class = Some(vec![
            HexCoastClass::LavaDelta,
            HexCoastClass::Inland,
            HexCoastClass::Inland,
            HexCoastClass::Inland,
        ]);
        let err = hex_coast_class_well_formed(&world).unwrap_err();
        assert!(
            matches!(
                err,
                ValidationError::HexCoastClassLavaDeltaWithoutCellSupport { hex_id: 0 }
            ),
            "expected HexCoastClassLavaDeltaWithoutCellSupport at hex_id 0, got {err:?}"
        );
    }

    /// `HexCoastClass::LavaDelta` on hex 0 IS valid when at least one cell
    /// inside hex 0 has `CoastType == LavaDelta` (discriminant 4).
    #[test]
    fn accepts_lavadelta_with_cell_support() {
        let mut world = minimal_world_for_1b(4, 4);
        world.derived.hex_grid = Some(make_hex_grid_2x2_on_4x4());
        // Cell (0,0) in hex 0 has CoastType::LavaDelta (disc=4).
        world.derived.coast_type = Some(make_coast_type_field_4x4(&[
            (0, 0, 4), // LavaDelta discriminant = 4 (CoastType level)
        ]));
        world.derived.hex_coast_class = Some(vec![
            HexCoastClass::LavaDelta,
            HexCoastClass::Inland,
            HexCoastClass::Inland,
            HexCoastClass::Inland,
        ]);
        assert!(
            hex_coast_class_well_formed(&world).is_ok(),
            "LavaDelta hex with a backing LavaDelta cell must pass"
        );
    }

    /// `hex_coast_class = None` → `Ok(())`.
    #[test]
    fn hex_coast_class_well_formed_skip_if_missing() {
        let world = minimal_world_for_1b(4, 4);
        assert!(world.derived.hex_coast_class.is_none());
        assert!(hex_coast_class_well_formed(&world).is_ok());
    }

    // Note: out-of-range discriminant test — HexCoastClass is a closed #[repr(u8)]
    // enum; constructing a discriminant outside 0..=6 requires `unsafe` transmute.
    // The runtime range check exists to catch ABI/memory-corruption issues not
    // reachable in safe Rust.  The enum closure itself is the compile-time
    // enforcement mechanism; no safe-Rust test can reach the error path.

    // ── hex_coast_class_requires_fetch_integral tests (DD4, Sprint 3.5.C c4) ──

    /// Both `hex_coast_class` and `coast_fetch_integral` are `Some` → `Ok`.
    #[test]
    fn accepts_when_fetch_integral_populated() {
        let mut world = minimal_world_for_1b(4, 4);
        world.derived.hex_coast_class = Some(vec![HexCoastClass::Beach]);
        world.derived.coast_fetch_integral = Some(ScalarField2D::<f32>::new(4, 4));
        // Default variant is V2FetchIntegral.
        assert_eq!(
            world.preset.erosion.coast_type_variant,
            CoastTypeVariant::V2FetchIntegral
        );
        assert!(hex_coast_class_requires_fetch_integral(&world).is_ok());
    }

    /// `hex_coast_class` is non-empty, variant is `V2FetchIntegral`, but
    /// `coast_fetch_integral` is `None` → `Err(HexCoastClassRequiresFetchIntegral)`.
    #[test]
    fn rejects_when_classes_populated_but_fetch_missing() {
        let mut world = minimal_world_for_1b(4, 4);
        world.derived.hex_coast_class = Some(vec![HexCoastClass::Beach]);
        // coast_fetch_integral left as None (default).
        assert!(world.derived.coast_fetch_integral.is_none());
        assert_eq!(
            world.preset.erosion.coast_type_variant,
            CoastTypeVariant::V2FetchIntegral
        );
        let err = hex_coast_class_requires_fetch_integral(&world).unwrap_err();
        assert!(
            matches!(err, ValidationError::HexCoastClassRequiresFetchIntegral),
            "expected HexCoastClassRequiresFetchIntegral, got {err:?}"
        );
    }

    /// When the variant is `V1Cheap`, `coast_fetch_integral` is legitimately
    /// absent and the validator must return `Ok(())`.
    #[test]
    fn accepts_when_v1_cheap_variant_active() {
        let mut world = minimal_world_for_1b(4, 4);
        world.preset.erosion.coast_type_variant = CoastTypeVariant::V1Cheap;
        world.derived.hex_coast_class = Some(vec![HexCoastClass::Beach]);
        // coast_fetch_integral is None — legitimate for V1Cheap.
        assert!(world.derived.coast_fetch_integral.is_none());
        assert!(
            hex_coast_class_requires_fetch_integral(&world).is_ok(),
            "V1Cheap variant must not require coast_fetch_integral"
        );
    }

    /// `hex_coast_class = None` → `Ok(())` regardless of variant.
    #[test]
    fn hex_coast_class_requires_fetch_integral_skip_if_missing() {
        let world = minimal_world_for_1b(4, 4);
        assert!(world.derived.hex_coast_class.is_none());
        assert!(hex_coast_class_requires_fetch_integral(&world).is_ok());
    }
}
