//! Sprint 3.5 DD4: fetch-weighted majority-vote hex coast classifier.
//!
//! Reads `derived.coast_fetch_integral` + `derived.coast_type` +
//! `derived.hex_grid`; writes `derived.hex_coast_class`. Per plan §2 DD4,
//! the enum type [`HexCoastClass`] lives in `core::world`; this module
//! contains only the classifier logic.
//!
//! ## Classification rule
//!
//! For each hex:
//! * If the hex has no land cells → [`HexCoastClass::OpenOcean`].
//! * If the hex has no sea cells → [`HexCoastClass::Inland`].
//! * Otherwise (coastal hex): each land cell casts a fetch-weighted vote for
//!   one of the 5 coast-type classes. The class with the highest total weight
//!   wins; ties break by priority order
//!   `Cliff > LavaDelta > Estuary > RockyHeadland > Beach`.
//!
//! When `derived.coast_fetch_integral` is `None` (V1 classifier path), each
//! land cell votes with uniform weight `1.0`.

use island_core::world::{HexCoastClass, WorldState};

// ─── vote index ↔ HexCoastClass mapping ──────────────────────────────────────
//
// The 5 coast-type classes (excluding Unknown) get compact vote-array indices:
//   0 → Beach          (CoastType::Beach = 1)
//   1 → RockyHeadland  (CoastType::RockyHeadland = 3)
//   2 → Estuary        (CoastType::Estuary = 2)
//   3 → Cliff          (CoastType::Cliff = 0)
//   4 → LavaDelta      (CoastType::LavaDelta = 4)
//
// Tie-break priority (higher = wins): Cliff=5 > LavaDelta=4 > Estuary=3 >
// RockyHeadland=2 > Beach=1.  Unknown / out-of-range cells contribute no vote.

const N_CLASSES: usize = 5;

#[inline]
fn coast_type_u8_to_vote_idx(ct: u8) -> Option<usize> {
    match ct {
        1 => Some(0), // Beach
        3 => Some(1), // RockyHeadland
        2 => Some(2), // Estuary
        0 => Some(3), // Cliff
        4 => Some(4), // LavaDelta
        _ => None,    // Unknown (0xFF) or any other sentinel
    }
}

#[inline]
fn vote_idx_priority(idx: usize) -> u8 {
    match idx {
        0 => 1, // Beach
        1 => 2, // RockyHeadland
        2 => 3, // Estuary
        3 => 5, // Cliff (highest)
        4 => 4, // LavaDelta
        _ => 0,
    }
}

#[inline]
fn vote_idx_to_hex_coast_class(idx: usize) -> HexCoastClass {
    match idx {
        0 => HexCoastClass::Beach,
        1 => HexCoastClass::RockyHeadland,
        2 => HexCoastClass::Estuary,
        3 => HexCoastClass::Cliff,
        4 => HexCoastClass::LavaDelta,
        // The classifier constructs vote indices from 0..N_CLASSES by
        // construction (see `coast_type_u8_to_vote_idx` + the votes array
        // shape). Any other value signals a kernel bug — surface loudly
        // rather than silent-miscategorise.
        _ => unreachable!("vote_idx must be 0..=4; got {idx}"),
    }
}

// ─── classifier ──────────────────────────────────────────────────────────────

/// Classify every hex into [`HexCoastClass`] per DD4's fetch-weighted
/// majority-vote rule with priority tie-break.
///
/// Returns a row-major `Vec<HexCoastClass>` of length `cols * rows`.
/// Called from `HexProjectionStage::run` after `hex_grid` + `coast_type`
/// are populated.
///
/// Returns `None` when required prerequisites (`hex_grid`, `coast_type`,
/// `coast_mask`) are not yet populated — callers should treat `None` the
/// same as "classifier hasn't run" and store `None` in
/// `world.derived.hex_coast_class`.
pub fn classify_hex_coast_classes(world: &WorldState) -> Option<Vec<HexCoastClass>> {
    let grid = world.derived.hex_grid.as_ref()?;
    let coast_type = world.derived.coast_type.as_ref()?;
    let coast_mask = world.derived.coast_mask.as_ref()?;

    // fetch_integral is optional: V1 path leaves it None; classifier falls
    // back to uniform weight 1.0 per land cell in that case.
    let fetch = world.derived.coast_fetch_integral.as_ref();

    let hex_count = (grid.cols * grid.rows) as usize;
    let mut result = vec![HexCoastClass::Inland; hex_count];

    // Per-hex accumulators.
    let mut votes: Vec<[f32; N_CLASSES]> = vec![[0.0; N_CLASSES]; hex_count];
    let mut land_count = vec![0_u32; hex_count];
    let mut has_sea = vec![false; hex_count];

    let sim_w = coast_mask.is_land.width;
    let sim_h = coast_mask.is_land.height;

    for iy in 0..sim_h {
        for ix in 0..sim_w {
            let hex_id = grid.hex_id_of_cell.get(ix, iy) as usize;
            let is_land = coast_mask.is_land.get(ix, iy) == 1;

            if is_land {
                land_count[hex_id] += 1;
                // Fetch weight: 1.0 when fetch_integral is None (V1 fallback);
                // otherwise the stored exposure value, clamped to ≥0 to avoid
                // negative-weight voting from any numeric edge case.
                let w = match fetch {
                    Some(f) => f.get(ix, iy).max(0.0),
                    None => 1.0,
                };
                // Land cells only vote when their coast_type is one of the 5
                // named classes; Unknown / inland cells cast no vote but still
                // increment land_count (correct for the Inland/OpenOcean gate).
                let ct_u8 = coast_type.get(ix, iy);
                if let Some(idx) = coast_type_u8_to_vote_idx(ct_u8) {
                    votes[hex_id][idx] += w;
                }
            } else {
                has_sea[hex_id] = true;
            }
        }
    }

    // Final classification per hex.
    for hex_id in 0..hex_count {
        result[hex_id] = if land_count[hex_id] == 0 {
            HexCoastClass::OpenOcean
        } else if !has_sea[hex_id] {
            HexCoastClass::Inland
        } else {
            // Coastal hex: fetch-weighted majority vote with priority tie-break.
            let v = &votes[hex_id];
            let (best_idx, best_w) = v
                .iter()
                .enumerate()
                .max_by(|(ia, wa), (ib, wb)| {
                    // Primary sort: higher weight wins.
                    wa.partial_cmp(wb)
                        .unwrap_or(std::cmp::Ordering::Equal)
                        // Tie-break: higher priority wins.
                        .then_with(|| {
                            let pa = vote_idx_priority(*ia);
                            let pb = vote_idx_priority(*ib);
                            pa.cmp(&pb)
                        })
                })
                .unwrap();

            if *best_w <= 0.0 {
                // No coast_type class received any fetch weight (e.g. all
                // coastal cells are Unknown or river-mouth cells with
                // exposure_v2=0 under the V1 fallback). Default to Beach
                // as the spec's "default coast cue" (plan §2 DD4).
                HexCoastClass::Beach
            } else {
                vote_idx_to_hex_coast_class(best_idx)
            }
        };
    }

    Some(result)
}

// ─── tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use island_core::field::{MaskField2D, ScalarField2D};
    use island_core::preset::{ErosionParams, IslandAge, IslandArchetypePreset};
    use island_core::seed::Seed;
    use island_core::world::{CoastMask, CoastType, HexGrid, HexLayout, Resolution, WorldState};

    /// Build a minimal WorldState with a pre-populated hex_grid, coast_mask,
    /// coast_type, and (optionally) coast_fetch_integral.
    ///
    /// Grid: 4×4 sim cells, 2×2 hex grid. The hex assignment is explicit
    /// (manually constructed `hex_id_of_cell` using simple 2×2 blocks) so
    /// that tests don't depend on the DD2 Voronoi kernel's exact output.
    ///
    /// Hex layout (row-major, cols=2):
    ///   hex 0 (row=0,col=0): sim cells (ix<2, iy<2) → flat indices [0,1,4,5]
    ///   hex 1 (row=0,col=1): sim cells (ix≥2, iy<2) → flat indices [2,3,6,7]
    ///   hex 2 (row=1,col=0): sim cells (ix<2, iy≥2) → flat indices [8,9,12,13]
    ///   hex 3 (row=1,col=1): sim cells (ix≥2, iy≥2) → flat indices [10,11,14,15]
    ///
    /// `land_mask[i]` = 1 if that flat cell index is land.
    /// `coast_type_data[i]` = CoastType discriminant for that cell (0xFF = Unknown).
    /// `fetch_data` = fetch-integral values (length 16), or empty slice to
    ///   leave coast_fetch_integral as None.
    fn build_world(
        land_mask: &[u8; 16],
        coast_type_data: &[u8; 16],
        fetch_data: &[f32],
    ) -> WorldState {
        let preset = IslandArchetypePreset {
            name: "test".into(),
            island_radius: 0.5,
            max_relief: 0.5,
            volcanic_center_count: 1,
            island_age: IslandAge::Young,
            prevailing_wind_dir: 0.0,
            marine_moisture_strength: 0.5,
            sea_level: 0.3,
            erosion: ErosionParams {
                n_batch: 0,
                ..Default::default()
            },
            climate: Default::default(),
        };
        let mut world = WorldState::new(Seed(42), preset, Resolution::new(4, 4));

        // Build coast_mask
        let mut is_land = MaskField2D::new(4, 4);
        let mut is_sea = MaskField2D::new(4, 4);
        is_land.data.copy_from_slice(land_mask);
        for i in 0..16 {
            is_sea.data[i] = 1 - land_mask[i];
        }
        let is_coast = MaskField2D::new(4, 4); // all-zero, not exercised here
        let land_cell_count = land_mask.iter().filter(|&&v| v == 1).count() as u32;
        world.derived.coast_mask = Some(CoastMask {
            is_land,
            is_sea,
            is_coast,
            land_cell_count,
            river_mouth_mask: None,
        });

        // Build coast_type
        let mut ct_field = ScalarField2D::<u8>::new(4, 4);
        ct_field.data.copy_from_slice(coast_type_data);
        world.derived.coast_type = Some(ct_field);

        // Build coast_fetch_integral (optional)
        if !fetch_data.is_empty() {
            assert_eq!(fetch_data.len(), 16);
            let mut fi = ScalarField2D::<f32>::new(4, 4);
            fi.data.copy_from_slice(fetch_data);
            world.derived.coast_fetch_integral = Some(fi);
        }

        // Build a 2×2 hex grid with explicit 2×2-block assignment so tests
        // are independent of the DD2 Voronoi kernel geometry.
        let mut hex_id_of_cell = ScalarField2D::<u32>::new(4, 4);
        for iy in 0..4u32 {
            for ix in 0..4u32 {
                // hex_id = row_hex * cols + col_hex
                let hex_id = (iy / 2) * 2 + (ix / 2);
                hex_id_of_cell.set(ix, iy, hex_id);
            }
        }
        world.derived.hex_grid = Some(HexGrid {
            cols: 2,
            rows: 2,
            hex_size: 2.0,
            layout: HexLayout::FlatTop,
            hex_id_of_cell,
        });

        world
    }

    /// Helper: all land mask (no sea cells)
    fn all_land() -> [u8; 16] {
        [1; 16]
    }

    /// Helper: all sea mask (no land cells)
    fn all_sea() -> [u8; 16] {
        [0; 16]
    }

    /// Helper: unknown coast type for all cells
    fn all_unknown_ct() -> [u8; 16] {
        [CoastType::Unknown as u8; 16]
    }

    #[test]
    fn classify_hex_with_no_sea_cells_is_inland() {
        // All 16 cells are land → all 4 hexes are Inland.
        let world = build_world(&all_land(), &all_unknown_ct(), &[]);
        let result = classify_hex_coast_classes(&world).expect("should return Some");
        assert_eq!(result.len(), 4);
        for (i, &cls) in result.iter().enumerate() {
            assert_eq!(
                cls,
                HexCoastClass::Inland,
                "hex {i} should be Inland (no sea cells)"
            );
        }
    }

    #[test]
    fn classify_hex_with_no_land_cells_is_open_ocean() {
        // All 16 cells are sea → all 4 hexes are OpenOcean.
        let world = build_world(&all_sea(), &all_unknown_ct(), &[]);
        let result = classify_hex_coast_classes(&world).expect("should return Some");
        assert_eq!(result.len(), 4);
        for (i, &cls) in result.iter().enumerate() {
            assert_eq!(
                cls,
                HexCoastClass::OpenOcean,
                "hex {i} should be OpenOcean (no land cells)"
            );
        }
    }

    #[test]
    fn classify_coastal_hex_uses_fetch_weighted_majority() {
        // Hex 0 (top-left 2×2 block) has cells at flat indices [0,1,4,5]:
        //   cell 0  → land, Beach (CoastType=1), fetch = 1.0
        //   cell 1  → sea
        //   cell 4  → land, Cliff (CoastType=0), fetch = 5.0
        //   cell 5  → sea
        //
        // Hex 0 is coastal (has both land and sea).
        // Cliff total weight = 5.0 > Beach total weight = 1.0 → hex 0 = Cliff.
        //
        // Hexes 1,2,3 are all sea → OpenOcean.
        //
        // Flat index layout for a 4-wide grid:
        //   row 0: [0,1,2,3]
        //   row 1: [4,5,6,7]
        //   row 2: [8,9,10,11]
        //   row 3: [12,13,14,15]
        // Hex 0 block = (ix<2, iy<2) → indices {0,1,4,5}.
        #[rustfmt::skip]
        let land_mask: [u8; 16] = [
            1, 0, 0, 0,   // row 0: cell 0 = land; 1,2,3 = sea
            1, 0, 0, 0,   // row 1: cell 4 = land; 5,6,7 = sea
            0, 0, 0, 0,   // row 2: all sea
            0, 0, 0, 0,   // row 3: all sea
        ];
        // Beach=1 for cell 0; Cliff=0 for cell 4; rest Unknown=0xFF.
        #[rustfmt::skip]
        let ct_data: [u8; 16] = [
            1,    0xFF, 0xFF, 0xFF,   // row 0
            0,    0xFF, 0xFF, 0xFF,   // row 1: CoastType::Cliff = 0
            0xFF, 0xFF, 0xFF, 0xFF,
            0xFF, 0xFF, 0xFF, 0xFF,
        ];
        // Fetch: 1.0 for Beach cell (0); 5.0 for Cliff cell (4); 0 elsewhere.
        #[rustfmt::skip]
        let fetch: [f32; 16] = [
            1.0, 0.0, 0.0, 0.0,
            5.0, 0.0, 0.0, 0.0,
            0.0, 0.0, 0.0, 0.0,
            0.0, 0.0, 0.0, 0.0,
        ];

        let world = build_world(&land_mask, &ct_data, &fetch);
        let result = classify_hex_coast_classes(&world).expect("should return Some");

        assert_eq!(
            result[0],
            HexCoastClass::Cliff,
            "Cliff wins on fetch weight"
        );
        assert_eq!(result[1], HexCoastClass::OpenOcean, "all sea → OpenOcean");
        assert_eq!(result[2], HexCoastClass::OpenOcean);
        assert_eq!(result[3], HexCoastClass::OpenOcean);
    }

    #[test]
    fn classify_tie_break_priority_cliff_over_lavadelta() {
        // Hex 0 has 2 coastal land cells (with adjacent sea):
        //   cell 0 → Cliff (CoastType=0), fetch = 1.0
        //   cell 1 → LavaDelta (CoastType=4), fetch = 1.0
        // Equal vote weights; priority: Cliff=5 > LavaDelta=4 → Cliff wins.
        //
        // Sea cells needed to make it a "coastal" hex: cells 4,5,6,7 (row 1).
        #[rustfmt::skip]
        let land_mask: [u8; 16] = [
            1, 1, 0, 0,
            0, 0, 0, 0,
            0, 0, 0, 0,
            0, 0, 0, 0,
        ];
        #[rustfmt::skip]
        let ct_data: [u8; 16] = [
            0, 4, 0xFF, 0xFF,   // Cliff=0, LavaDelta=4
            0xFF, 0xFF, 0xFF, 0xFF,
            0xFF, 0xFF, 0xFF, 0xFF,
            0xFF, 0xFF, 0xFF, 0xFF,
        ];
        #[rustfmt::skip]
        let fetch: [f32; 16] = [
            1.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 0.0, 0.0,
            0.0, 0.0, 0.0, 0.0,
            0.0, 0.0, 0.0, 0.0,
        ];

        let world = build_world(&land_mask, &ct_data, &fetch);
        let result = classify_hex_coast_classes(&world).expect("should return Some");
        assert_eq!(
            result[0],
            HexCoastClass::Cliff,
            "Cliff beats LavaDelta on tie-break priority"
        );
    }
}
