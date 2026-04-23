//! Hex-surface invariants — shape and biome-weight-vector consistency of the
//! hex attribute grid.
//!
//! `hex_attrs_present` is the only member of this family in v1.
//! Sprint 5 S1's real-hex rework and the hex-grammar extensions planned for
//! Sprint 3.5.D will add further invariants here.

use crate::world::WorldState;

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

// ─── tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::field::MaskField2D;
    use crate::seed::Seed;
    use crate::test_support::test_preset;
    use crate::world::{
        BakedSnapshot, CoastMask, HexAttributeField, HexAttributes, HexDebugAttributes,
        HexRiverCrossing, Resolution, RiverWidth, WorldState,
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
}
