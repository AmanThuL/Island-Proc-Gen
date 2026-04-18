//! River network extraction (Task 1A.8).
//!
//! Threshold → 8-CC → coast-contact filter → top-N. Writes
//! `derived.river_mask` and backfills `derived.coast_mask.river_mouth_mask`.

use std::collections::VecDeque;

use island_core::field::ScalarField2D;
use island_core::neighborhood::{RIVER_CC_NEIGHBORHOOD, RIVER_COAST_CONTACT};
use island_core::pipeline::SimulationStage;
use island_core::world::WorldState;

use crate::geomorph::neighbour_offsets;

// ─── constants ────────────────────────────────────────────────────────────────

/// Accumulation threshold as a fraction of `land_cell_count` (not total cells),
/// so river density stays stable across varying sea-level / island-radius.
pub(crate) const RIVER_THRESHOLD_FACTOR: f32 = 0.01;

/// Maximum number of river components retained (largest by cell count).
const MAX_RIVERS: usize = 5;

// ─── helpers ──────────────────────────────────────────────────────────────────

/// Returns `true` when any Moore8 neighbour of `(x, y)` is a coast cell.
fn has_coast_neighbour(
    x: usize,
    y: usize,
    w: usize,
    h: usize,
    offsets: &[(i32, i32)],
    is_coast: &ScalarField2D<u8>,
) -> bool {
    for &(dx, dy) in offsets {
        let nx = x as i32 + dx;
        let ny = y as i32 + dy;
        if nx >= 0
            && nx < w as i32
            && ny >= 0
            && ny < h as i32
            && is_coast.get(nx as u32, ny as u32) == 1
        {
            return true;
        }
    }
    false
}

// ─── RiverExtractionStage ─────────────────────────────────────────────────────

/// Extracts the main river network and backfills `coast_mask.river_mouth_mask`.
pub struct RiverExtractionStage;

impl SimulationStage for RiverExtractionStage {
    fn name(&self) -> &'static str {
        "river_extraction"
    }

    fn run(&self, world: &mut WorldState) -> anyhow::Result<()> {
        let river_mask;
        let river_mouth_mask;

        {
            let coast = world
                .derived
                .coast_mask
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("RiverExtractionStage: derived.coast_mask is None (CoastMaskStage must run first)"))?;

            let accum = world
                .derived
                .accumulation
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("RiverExtractionStage: derived.accumulation is None (AccumulationStage must run first)"))?;

            let w = accum.width as usize;
            let h = accum.height as usize;
            let n = w * h;

            let threshold = RIVER_THRESHOLD_FACTOR * coast.land_cell_count as f32;

            // Candidate mask (0/1) — land-only: sea cells can accumulate
            // contributions from diagonal Moore8 land neighbours and thus
            // exceed the threshold, but they are not rivers.
            let mut candidate: Vec<u8> = vec![0; n];
            for y in 0..h {
                for x in 0..w {
                    if coast.is_land.get(x as u32, y as u32) == 1
                        && accum.get(x as u32, y as u32) >= threshold
                    {
                        candidate[y * w + x] = 1;
                    }
                }
            }

            // 8-connected BFS labelling. comp_id 0 = unlabelled; ids start at 1.
            let mut comp_id: Vec<u32> = vec![0; n];
            let mut comp_sizes: Vec<u32> = vec![0]; // slot 0 is a dummy
            let cc_offsets = neighbour_offsets(RIVER_CC_NEIGHBORHOOD);
            let mut next_id: u32 = 1;
            let mut queue: VecDeque<u32> = VecDeque::new();

            for start in 0..n {
                if candidate[start] == 0 || comp_id[start] != 0 {
                    continue;
                }
                let id = next_id;
                next_id += 1;
                comp_id[start] = id;
                comp_sizes.push(0);
                queue.clear();
                queue.push_back(start as u32);

                while let Some(p) = queue.pop_front() {
                    comp_sizes[id as usize] += 1;
                    let px = (p as usize) % w;
                    let py = (p as usize) / w;

                    for &(dx, dy) in cc_offsets {
                        let nx = px as i32 + dx;
                        let ny = py as i32 + dy;
                        if nx < 0 || nx >= w as i32 || ny < 0 || ny >= h as i32 {
                            continue;
                        }
                        let ni = ny as usize * w + nx as usize;
                        if candidate[ni] == 1 && comp_id[ni] == 0 {
                            comp_id[ni] = id;
                            queue.push_back(ni as u32);
                        }
                    }
                }
            }

            let num_comps = (next_id - 1) as usize;

            // Coast-contact filter: keep only components with a Moore8 coast neighbour.
            let coast_offsets = neighbour_offsets(RIVER_COAST_CONTACT);
            let mut comp_has_coast: Vec<u8> = vec![0; num_comps + 1];

            for y in 0..h {
                for x in 0..w {
                    let cid = comp_id[y * w + x];
                    if cid == 0 || comp_has_coast[cid as usize] == 1 {
                        continue;
                    }
                    if has_coast_neighbour(x, y, w, h, coast_offsets, &coast.is_coast) {
                        comp_has_coast[cid as usize] = 1;
                    }
                }
            }

            // Top-N by size among coast-touching components.
            let mut coast_comps: Vec<(u32, u32)> = (1..=num_comps as u32)
                .filter(|&id| comp_has_coast[id as usize] == 1)
                .map(|id| (comp_sizes[id as usize], id))
                .collect();
            coast_comps.sort_unstable_by_key(|c| std::cmp::Reverse(c.0));
            coast_comps.truncate(MAX_RIVERS);

            let mut retained: Vec<u8> = vec![0; num_comps + 1];
            for &(_, id) in &coast_comps {
                retained[id as usize] = 1;
            }

            // Build output fields.
            let mut rm_data = vec![0u8; n];
            let mut rmm_data = vec![0u8; n];

            for y in 0..h {
                for x in 0..w {
                    let idx = y * w + x;
                    let cid = comp_id[idx];
                    if cid == 0 || retained[cid as usize] == 0 {
                        continue;
                    }
                    rm_data[idx] = 1;
                    if has_coast_neighbour(x, y, w, h, coast_offsets, &coast.is_coast) {
                        rmm_data[idx] = 1;
                    }
                }
            }

            let w32 = accum.width;
            let h32 = accum.height;

            river_mask = ScalarField2D::<u8> {
                data: rm_data,
                width: w32,
                height: h32,
            };
            river_mouth_mask = ScalarField2D::<u8> {
                data: rmm_data,
                width: w32,
                height: h32,
            };
        }

        world.derived.river_mask = Some(river_mask);
        world.derived.coast_mask.as_mut().unwrap().river_mouth_mask = Some(river_mouth_mask);

        Ok(())
    }
}

// ─── tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use island_core::field::{MaskField2D, ScalarField2D};
    use island_core::pipeline::SimulationStage;
    use island_core::preset::{IslandAge, IslandArchetypePreset};
    use island_core::seed::Seed;
    use island_core::world::{CoastMask, Resolution, WorldState};

    use super::RiverExtractionStage;
    use crate::geomorph::{CoastMaskStage, PitFillStage, TopographyStage};
    use crate::hydro::{AccumulationStage, BasinsStage, FlowRoutingStage};

    // ── helpers ───────────────────────────────────────────────────────────────

    fn test_preset() -> IslandArchetypePreset {
        IslandArchetypePreset {
            name: "river_test".into(),
            island_radius: 0.45,
            max_relief: 0.8,
            volcanic_center_count: 1,
            island_age: IslandAge::Young,
            prevailing_wind_dir: 0.0,
            marine_moisture_strength: 0.5,
            sea_level: 0.25,
            erosion: Default::default(),
        }
    }

    fn run_full_pipeline(seed: u64, preset: IslandArchetypePreset, res: u32) -> WorldState {
        let mut world = WorldState::new(Seed(seed), preset, Resolution::new(res, res));
        TopographyStage.run(&mut world).expect("TopographyStage");
        CoastMaskStage.run(&mut world).expect("CoastMaskStage");
        PitFillStage.run(&mut world).expect("PitFillStage");
        FlowRoutingStage.run(&mut world).expect("FlowRoutingStage");
        AccumulationStage
            .run(&mut world)
            .expect("AccumulationStage");
        BasinsStage.run(&mut world).expect("BasinsStage");
        RiverExtractionStage
            .run(&mut world)
            .expect("RiverExtractionStage");
        world
    }

    /// Build a minimal CoastMask for synthetic grid tests.
    fn make_coast_mask(
        w: u32,
        h: u32,
        is_land_data: Vec<u8>,
        is_sea_data: Vec<u8>,
        is_coast_data: Vec<u8>,
        land_cell_count: u32,
    ) -> CoastMask {
        let mut is_land = MaskField2D::new(w, h);
        is_land.data = is_land_data;
        let mut is_sea = MaskField2D::new(w, h);
        is_sea.data = is_sea_data;
        let mut is_coast = MaskField2D::new(w, h);
        is_coast.data = is_coast_data;
        CoastMask {
            is_land,
            is_sea,
            is_coast,
            land_cell_count,
            river_mouth_mask: None,
        }
    }

    // ── test 1: empty accumulation produces empty river mask ──────────────────

    #[test]
    fn empty_accumulation_produces_empty_river_mask() {
        let w = 8_u32;
        let h = 8_u32;
        let n = (w * h) as usize;

        // A few coast cells on the border; interior is land.
        let mut is_land_data = vec![1u8; n];
        let mut is_sea_data = vec![0u8; n];
        let mut is_coast_data = vec![0u8; n];
        // Top row: sea
        for x in 0..w as usize {
            is_land_data[x] = 0;
            is_sea_data[x] = 1;
        }
        // Second row: coast (land adjacent to sea)
        for x in 0..w as usize {
            is_coast_data[w as usize + x] = 1;
        }

        let land_cell_count = is_land_data.iter().map(|&v| v as u32).sum::<u32>();
        let coast_mask = make_coast_mask(
            w,
            h,
            is_land_data,
            is_sea_data,
            is_coast_data,
            land_cell_count,
        );

        let mut world = WorldState::new(Seed(0), test_preset(), Resolution::new(w, h));
        world.derived.coast_mask = Some(coast_mask);
        // accumulation all zeros
        world.derived.accumulation = Some(ScalarField2D::<f32>::new(w, h));

        RiverExtractionStage.run(&mut world).expect("stage failed");

        let rm = world.derived.river_mask.as_ref().unwrap();
        assert!(
            rm.data.iter().all(|&v| v == 0),
            "river_mask must be all-zero when accumulation is zero"
        );

        let rmm = world
            .derived
            .coast_mask
            .as_ref()
            .unwrap()
            .river_mouth_mask
            .as_ref()
            .unwrap();
        assert!(
            rmm.data.iter().all(|&v| v == 0),
            "river_mouth_mask must be all-zero when accumulation is zero"
        );
    }

    // ── test 2: at least one river mouth on a volcanic island ─────────────────

    #[test]
    fn at_least_one_river_mouth_from_full_pipeline() {
        let world = run_full_pipeline(42, test_preset(), 64);

        let rm = world.derived.river_mask.as_ref().unwrap();
        assert!(
            rm.data.contains(&1),
            "expected at least one river cell on seed=42 volcanic island"
        );

        let rmm = world
            .derived
            .coast_mask
            .as_ref()
            .unwrap()
            .river_mouth_mask
            .as_ref()
            .unwrap();
        assert!(
            rmm.data.contains(&1),
            "expected at least one river mouth on seed=42 volcanic island"
        );
    }

    // ── test 3: isolated high-A component without coast contact is dropped ────
    //
    // 7x7 grid:
    //   row/col 0 and 6: sea (not coast)
    //   row/col 1 and 5: coast (land adjacent to sea)
    //   row/col 2 and 4: non-coast land
    //   cell (3,3): interior, high accumulation
    //
    // (3,3) Moore8 = (2,2),(3,2),(4,2),(2,3),(4,3),(2,4),(3,4),(4,4) — all
    // non-coast land. So (3,3) has no coast contact → must be dropped.

    #[test]
    fn isolated_high_a_component_without_coast_contact_is_dropped() {
        let w = 7_u32;
        let h = 7_u32;
        let n = (w * h) as usize;

        let idx = |x: u32, y: u32| (y * w + x) as usize;

        let mut is_land_data = vec![0u8; n];
        let mut is_sea_data = vec![1u8; n];
        let mut is_coast_data = vec![0u8; n];

        // Land: rows/cols 1..=5
        for y in 1..=5_u32 {
            for x in 1..=5_u32 {
                is_land_data[idx(x, y)] = 1;
                is_sea_data[idx(x, y)] = 0;
            }
        }
        // Coast: row/col 1 and 5 (land adjacent to sea border)
        for i in 1..=5_u32 {
            is_coast_data[idx(i, 1)] = 1;
            is_coast_data[idx(i, 5)] = 1;
            is_coast_data[idx(1, i)] = 1;
            is_coast_data[idx(5, i)] = 1;
        }

        let land_cell_count = is_land_data.iter().map(|&v| v as u32).sum::<u32>();

        // Accumulation: only (3,3) is above threshold.
        // threshold = 0.01 * 25 = 0.25; set (3,3) = 1.0.
        let mut accum = ScalarField2D::<f32>::new(w, h);
        accum.set(3, 3, 1.0);

        let coast_mask = make_coast_mask(
            w,
            h,
            is_land_data,
            is_sea_data,
            is_coast_data,
            land_cell_count,
        );

        let mut world = WorldState::new(Seed(0), test_preset(), Resolution::new(w, h));
        world.derived.coast_mask = Some(coast_mask);
        world.derived.accumulation = Some(accum);

        RiverExtractionStage.run(&mut world).expect("stage failed");

        let rm = world.derived.river_mask.as_ref().unwrap();
        assert_eq!(
            rm.get(3, 3),
            0,
            "interior (3,3) with no coast contact must be dropped"
        );
    }

    // ── test 4: diagonally-reaching river kept via Moore8 coast contact ────────
    //
    // 5x5 grid. Sea at top-left corner (0,0). Land everywhere else.
    // Coast cell at (1,0) and (0,1) (von4 adjacent to sea).
    // River candidate at (2,2): its Moore8 includes (1,1), which is NOT coast.
    // River candidate at (1,1): its Moore8 includes (0,0) sea-adjacent — but
    // we need to check is_coast, not is_sea. (1,1) has no is_coast neighbour
    // unless we place one carefully.
    //
    // Simpler layout:
    //   (0,0) = sea; (1,0) = coast; (0,1) = coast.
    //   (2,1) = land (non-coast). River candidate = (2,1).
    //   (2,1) Moore8 includes (1,0) which IS coast → retained.

    #[test]
    fn diagonally_reaching_river_kept_via_moore8_contact() {
        let w = 5_u32;
        let h = 5_u32;
        let n = (w * h) as usize;

        let idx = |x: u32, y: u32| (y * w + x) as usize;

        let mut is_land_data = vec![1u8; n];
        let mut is_sea_data = vec![0u8; n];
        let mut is_coast_data = vec![0u8; n];

        // Sea at (0,0)
        is_land_data[idx(0, 0)] = 0;
        is_sea_data[idx(0, 0)] = 1;

        // Coast: cells adjacent (Von4) to (0,0) that are land
        is_coast_data[idx(1, 0)] = 1; // land, has sea neighbour to W
        is_coast_data[idx(0, 1)] = 1; // land, has sea neighbour to N

        let land_cell_count = is_land_data.iter().map(|&v| v as u32).sum::<u32>();

        // threshold = 0.01 * 24 = 0.24; set (2,1) = 1.0 — only river candidate.
        // (2,1) Moore8: (1,0) = coast → must be KEPT.
        let mut accum = ScalarField2D::<f32>::new(w, h);
        accum.set(2, 1, 1.0);

        let coast_mask = make_coast_mask(
            w,
            h,
            is_land_data,
            is_sea_data,
            is_coast_data,
            land_cell_count,
        );

        let mut world = WorldState::new(Seed(0), test_preset(), Resolution::new(w, h));
        world.derived.coast_mask = Some(coast_mask);
        world.derived.accumulation = Some(accum);

        RiverExtractionStage.run(&mut world).expect("stage failed");

        let rm = world.derived.river_mask.as_ref().unwrap();
        assert_eq!(
            rm.get(2, 1),
            1,
            "river at (2,1) has diagonal coast contact via (1,0); must be retained"
        );
    }

    // ── test 5: land-fraction stability across sea levels ─────────────────────
    //
    // Using land_cell_count in the threshold keeps river_cell / land_cell
    // ratio stable as sea_level changes. Verify the ratio doesn't drift > 5x.

    #[test]
    fn land_fraction_stability_across_sea_levels() {
        let mut p_low = test_preset();
        p_low.sea_level = 0.25;

        let mut p_high = test_preset();
        p_high.sea_level = 0.35;

        let w1 = run_full_pipeline(42, p_low, 64);
        let w2 = run_full_pipeline(42, p_high, 64);

        let cm1 = w1.derived.coast_mask.as_ref().unwrap();
        let cm2 = w2.derived.coast_mask.as_ref().unwrap();
        let rm1 = w1.derived.river_mask.as_ref().unwrap();
        let rm2 = w2.derived.river_mask.as_ref().unwrap();

        let land1 = cm1.land_cell_count as f32;
        let land2 = cm2.land_cell_count as f32;
        let river1 = rm1.data.iter().map(|&v| v as f32).sum::<f32>();
        let river2 = rm2.data.iter().map(|&v| v as f32).sum::<f32>();

        // Both should have some land and rivers; avoid division by zero.
        assert!(land1 > 0.0 && land2 > 0.0, "both presets must have land");

        let ratio1 = river1 / land1;
        let ratio2 = river2 / land2;

        // Either could be zero (no rivers extracted), which is a valid outcome;
        // in that case the ratio comparison is moot. If both are nonzero, check
        // the ratios stay within 5x of each other.
        if ratio1 > 0.0 && ratio2 > 0.0 {
            let relative = (ratio1 / ratio2).max(ratio2 / ratio1);
            assert!(
                relative < 5.0,
                "river/land ratio drifted {relative:.2}x between sea_levels (expected < 5x)"
            );
        }
    }

    // ── test 6: determinism bit-exact ─────────────────────────────────────────

    #[test]
    fn determinism_bit_exact() {
        let w1 = run_full_pipeline(42, test_preset(), 64);
        let w2 = run_full_pipeline(42, test_preset(), 64);

        assert_eq!(
            w1.derived.river_mask.as_ref().unwrap().data,
            w2.derived.river_mask.as_ref().unwrap().data,
            "river_mask must be bit-exact across identical runs"
        );
    }

    // ── test 7a: errors when accumulation is missing ──────────────────────────

    #[test]
    fn errors_when_accumulation_missing() {
        let w = 8_u32;
        let h = 8_u32;
        let n = (w * h) as usize;

        let mut world = WorldState::new(Seed(0), test_preset(), Resolution::new(w, h));
        world.derived.coast_mask = Some(make_coast_mask(
            w,
            h,
            vec![1u8; n],
            vec![0u8; n],
            vec![0u8; n],
            n as u32,
        ));
        // accumulation deliberately None

        let result = RiverExtractionStage.run(&mut world);
        assert!(result.is_err(), "expected Err when accumulation is None");
    }

    // ── test 7b: errors when coast_mask is missing ────────────────────────────

    #[test]
    fn errors_when_coast_mask_missing() {
        let w = 8_u32;
        let h = 8_u32;

        let mut world = WorldState::new(Seed(0), test_preset(), Resolution::new(w, h));
        world.derived.accumulation = Some(ScalarField2D::<f32>::new(w, h));
        // coast_mask deliberately None

        let result = RiverExtractionStage.run(&mut world);
        assert!(result.is_err(), "expected Err when coast_mask is None");
    }
}
