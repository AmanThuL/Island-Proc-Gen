//! Drainage basin labeling via reverse-BFS from coast sinks — Task 1A.7.
//!
//! Reads `world.derived.flow_dir` and `world.derived.coast_mask`, assigns each
//! land cell a drainage basin id, and writes `world.derived.basin_id`.
//!
//! Convention: 0 = sea/unlabeled; 1, 2, … N = basin id in row-major sink order.
//!
//! Sprint 2.5 Task 2.5.G adds a post-process connected-component pass that
//! promotes internal lakes (land cells with `FLOW_DIR_SINK` not reachable by
//! the coastal reverse-BFS) to fresh basin ids when their CC is large enough.

use std::collections::VecDeque;

use island_core::field::ScalarField2D;
use island_core::pipeline::SimulationStage;
use island_core::world::WorldState;

use super::{D8_OFFSETS, FLOW_DIR_SINK};

// ─── constants ────────────────────────────────────────────────────────────────

/// Minimum connected-component size (in cells) for an internal lake sink group
/// to be promoted to a fresh basin id.
///
/// Sink CCs smaller than this threshold remain at `basin_id = 0` (unlabeled)
/// rather than fragmenting the basin partition with noise-level lake pixels.
pub const MIN_INTERNAL_LAKE_CELLS: u32 = 8;

// ─── BasinsStage ──────────────────────────────────────────────────────────────

/// Sprint 1A Task 1A.7: drainage basin partition via reverse-BFS from sinks.
///
/// Every land cell is labeled with the id of the coast-sink it drains into.
/// Id assignment is purely geometric (row-major order of sinks), so identical
/// geometry always produces identical ids regardless of seed.
pub struct BasinsStage;

impl SimulationStage for BasinsStage {
    fn name(&self) -> &'static str {
        "basins"
    }

    fn run(&self, world: &mut WorldState) -> anyhow::Result<()> {
        let flow_dir = world.derived.flow_dir.as_ref().ok_or_else(|| {
            anyhow::anyhow!(
                "BasinsStage: derived.flow_dir is None (FlowRoutingStage must run first)"
            )
        })?;

        let coast_mask = world.derived.coast_mask.as_ref().ok_or_else(|| {
            anyhow::anyhow!(
                "BasinsStage: derived.coast_mask is None (CoastMaskStage must run first)"
            )
        })?;

        let w = flow_dir.width as usize;
        let h = flow_dir.height as usize;
        let n = w * h;

        // ── collect sinks ─────────────────────────────────────────────────────
        // A coastal sink is a land cell that terminates flow INTO the ocean:
        //   (a) FLOW_DIR_SINK and marked as a coast cell (is_coast == 1), or
        //   (b) a land cell whose D8 downstream is sea or out-of-bounds.
        //
        // Case (b) handles the rare situation where a land cell flows directly
        // into a sea cell rather than through a formally-marked coast cell
        // (diagonal Moore8 edge — see CLAUDE.md gotcha).
        //
        // Inland FLOW_DIR_SINK cells (is_coast == 0 and no sea downstream)
        // are NOT coastal sinks. They represent post-erosion internal
        // depressions and are handled by the post-process CC pass below,
        // which groups them into lake basins when large enough.
        //
        // Sort by row-major index for deterministic id assignment.
        let mut sinks: Vec<u32> = (0..n as u32)
            .filter(|&p| {
                let x = p as usize % w;
                let y = p as usize / w;
                if coast_mask.is_land.get(x as u32, y as u32) == 0 {
                    return false;
                }
                let dir = flow_dir.get(x as u32, y as u32);
                if dir == FLOW_DIR_SINK {
                    // Coastal FLOW_DIR_SINK: only if the cell is a coast cell.
                    // Inland FLOW_DIR_SINK cells are deferred to the CC pass.
                    return coast_mask.is_coast.get(x as u32, y as u32) == 1;
                }
                debug_assert!(
                    (dir as usize) < D8_OFFSETS.len(),
                    "flow_dir contains invalid direction {dir}; FlowRoutingStage contract violated"
                );
                let (dx, dy) = D8_OFFSETS[dir as usize];
                let qx = x as i32 + dx;
                let qy = y as i32 + dy;
                if qx < 0 || qx >= w as i32 || qy < 0 || qy >= h as i32 {
                    return true; // OOB → implicit sink
                }
                coast_mask.is_sea.get(qx as u32, qy as u32) == 1
            })
            .collect();
        sinks.sort_unstable();

        // ── build reverse adjacency (O(n)) ────────────────────────────────────
        let mut reverse_adj: Vec<Vec<u32>> = vec![Vec::new(); n];

        for y in 0..h {
            for x in 0..w {
                let p = (y * w + x) as u32;
                let dir = flow_dir.get(x as u32, y as u32);
                if dir == FLOW_DIR_SINK {
                    continue;
                }
                debug_assert!(
                    (dir as usize) < D8_OFFSETS.len(),
                    "flow_dir contains invalid direction {dir}"
                );
                let (dx, dy) = D8_OFFSETS[dir as usize];
                let qx = x as i32 + dx;
                let qy = y as i32 + dy;
                if qx >= 0 && qx < w as i32 && qy >= 0 && qy < h as i32 {
                    let q = (qy as usize * w + qx as usize) as u32;
                    reverse_adj[q as usize].push(p);
                }
            }
        }

        // ── reverse-BFS from each sink ────────────────────────────────────────
        let mut basin_id: Vec<u32> = vec![0u32; n];
        // Explicit visited mask guards against cycles from future routing bugs.
        let mut visited: Vec<u8> = vec![0u8; n];

        let mut queue: VecDeque<u32> = VecDeque::new();

        for (idx, &sink) in sinks.iter().enumerate() {
            let id = (idx + 1) as u32;
            if visited[sink as usize] == 1 {
                continue;
            }
            basin_id[sink as usize] = id;
            visited[sink as usize] = 1;
            queue.push_back(sink);

            while let Some(p) = queue.pop_front() {
                for &up in &reverse_adj[p as usize] {
                    if visited[up as usize] == 1 {
                        continue;
                    }
                    let ux = up as usize % w;
                    let uy = up as usize / w;
                    // Only traverse to land cells; sea cells stay at 0.
                    if coast_mask.is_land.get(ux as u32, uy as u32) == 0 {
                        continue;
                    }
                    basin_id[up as usize] = id;
                    visited[up as usize] = 1;
                    queue.push_back(up);
                }
            }
        }

        // ── post-process: promote large internal-lake CCs ─────────────────
        // Land cells still at basin_id == 0 after the coastal reverse-BFS are
        // internal depressions not reachable from any coast sink. Label each
        // Von4-connected component of such cells; CCs >= MIN_INTERNAL_LAKE_CELLS
        // get a fresh basin id. Von4 (not Moore8) is intentional: it matches
        // CoastMaskStage's adjacency and keeps two diagonally-touching depressions
        // as separate lakes.
        let mut next_id: u32 = sinks.len() as u32 + 1;

        // Von4 offsets: E, W, N, S.
        const VON4: [(i32, i32); 4] = [(1, 0), (-1, 0), (0, -1), (0, 1)];

        for start in 0..n {
            let sx = start % w;
            let sy = start / w;
            if visited[start] == 1 {
                continue;
            }
            if coast_mask.is_land.get(sx as u32, sy as u32) == 0 {
                continue;
            }
            if flow_dir.get(sx as u32, sy as u32) != FLOW_DIR_SINK {
                continue;
            }

            // BFS to collect the full Von4 CC of unvisited sink land cells.
            let mut cc: Vec<usize> = Vec::new();
            queue.push_back(start as u32);
            visited[start] = 1;

            while let Some(p) = queue.pop_front() {
                cc.push(p as usize);
                let px = p as usize % w;
                let py = p as usize / w;
                for &(dx, dy) in &VON4 {
                    let qx = px as i32 + dx;
                    let qy = py as i32 + dy;
                    if qx < 0 || qx >= w as i32 || qy < 0 || qy >= h as i32 {
                        continue;
                    }
                    let q = qy as usize * w + qx as usize;
                    if visited[q] == 1 {
                        continue;
                    }
                    if coast_mask.is_land.get(qx as u32, qy as u32) == 0 {
                        continue;
                    }
                    if flow_dir.get(qx as u32, qy as u32) != FLOW_DIR_SINK {
                        continue;
                    }
                    visited[q] = 1;
                    queue.push_back(q as u32);
                }
            }

            if cc.len() as u32 >= MIN_INTERNAL_LAKE_CELLS {
                let id = next_id;
                next_id += 1;
                for &cell in &cc {
                    basin_id[cell] = id;
                }
            }
        }

        world.derived.basin_id = Some(ScalarField2D {
            data: basin_id,
            width: flow_dir.width,
            height: flow_dir.height,
        });
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

    use super::BasinsStage;
    use crate::geomorph::{CoastMaskStage, PitFillStage, TopographyStage};
    use crate::hydro::{FLOW_DIR_SINK, FlowRoutingStage};

    // ── helpers ───────────────────────────────────────────────────────────────

    fn test_preset() -> IslandArchetypePreset {
        IslandArchetypePreset {
            name: "basins_test".into(),
            island_radius: 0.45,
            max_relief: 0.8,
            volcanic_center_count: 1,
            island_age: IslandAge::Young,
            prevailing_wind_dir: 0.0,
            marine_moisture_strength: 0.5,
            sea_level: 0.25,
            erosion: Default::default(),
            climate: Default::default(),
        }
    }

    fn run_full_pipeline(seed: u64) -> WorldState {
        let mut world = WorldState::new(Seed(seed), test_preset(), Resolution::new(64, 64));
        TopographyStage.run(&mut world).expect("TopographyStage");
        CoastMaskStage.run(&mut world).expect("CoastMaskStage");
        PitFillStage.run(&mut world).expect("PitFillStage");
        FlowRoutingStage.run(&mut world).expect("FlowRoutingStage");
        BasinsStage.run(&mut world).expect("BasinsStage");
        world
    }

    // ── test 1: two separated volcanoes produce two basins ────────────────────
    //
    // 6x6 grid. Left cluster (x in 0..3, all land) has coast-sink at (0,0).
    // Right cluster (x in 3..6, all land) has coast-sink at (5,5).
    // Row-major: (0,0) = 0, (5,5) = 35. So left sink → id 1, right → id 2.
    //
    // Left cluster flow: all non-sink cells route toward (0,0) via W or N.
    // Right cluster flow: all non-sink cells route toward (5,5) via E or S.
    //
    // Both sink cells are marked as coast cells (is_coast == 1) so the
    // revised sink-collection filter includes them as coastal sinks, not
    // deferred to the internal-lake CC pass.
    #[test]
    fn two_separated_volcanoes_produce_two_basins() {
        let w = 6_u32;
        let h = 6_u32;
        let n = (w * h) as usize;

        let mut is_land = MaskField2D::new(w, h);
        let is_sea = MaskField2D::new(w, h);
        let mut is_coast = MaskField2D::new(w, h);

        // All cells are land.
        for i in 0..n {
            is_land.data[i] = 1;
        }
        // Mark both sinks as coast cells so they qualify as coastal sinks.
        is_coast.set(0, 0, 1);
        is_coast.set(5, 5, 1);

        let mut flow_dir = ScalarField2D::<u8>::new(w, h);

        // Left cluster: all cells drain toward (0,0).
        // (0,0) is the sink.
        // Other left cells: route N (dir=2, dy=-1) if y>0, else W (dir=4, dx=-1) if x>0.
        // For (0,0): FLOW_DIR_SINK.
        for y in 0..h {
            for x in 0..3_u32 {
                if x == 0 && y == 0 {
                    flow_dir.set(x, y, FLOW_DIR_SINK);
                } else if y > 0 {
                    flow_dir.set(x, y, 2); // N
                } else {
                    flow_dir.set(x, y, 4); // W
                }
            }
        }

        // Right cluster: all cells drain toward (5,5).
        // (5,5) is the sink.
        // Other right cells: route S (dir=6, dy=+1) if y<5, else E (dir=0, dx=+1) if x<5.
        for y in 0..h {
            for x in 3..6_u32 {
                if x == 5 && y == 5 {
                    flow_dir.set(x, y, FLOW_DIR_SINK);
                } else if y < 5 {
                    flow_dir.set(x, y, 6); // S
                } else {
                    flow_dir.set(x, y, 0); // E
                }
            }
        }

        let coast_mask = CoastMask {
            is_land,
            is_sea,
            is_coast,
            land_cell_count: n as u32,
            river_mouth_mask: None,
        };

        let mut world = WorldState::new(Seed(0), test_preset(), Resolution::new(w, h));
        world.derived.flow_dir = Some(flow_dir);
        world.derived.coast_mask = Some(coast_mask);

        BasinsStage.run(&mut world).expect("BasinsStage failed");

        let bi = world.derived.basin_id.as_ref().unwrap();

        // Left cluster (x in 0..3): all must be basin 1 (sink (0,0) < (5,5) in row-major).
        for y in 0..h {
            for x in 0..3_u32 {
                assert_eq!(
                    bi.get(x, y),
                    1,
                    "left cluster cell ({x},{y}) must have basin_id == 1"
                );
            }
        }

        // Right cluster (x in 3..6): all must be basin 2.
        for y in 0..h {
            for x in 3..6_u32 {
                assert_eq!(
                    bi.get(x, y),
                    2,
                    "right cluster cell ({x},{y}) must have basin_id == 2"
                );
            }
        }
    }

    // ── test 2: every land cell has nonzero basin id ──────────────────────────

    #[test]
    fn every_land_cell_has_nonzero_basin_id() {
        let world = run_full_pipeline(42);
        let bi = world.derived.basin_id.as_ref().unwrap();
        let cm = world.derived.coast_mask.as_ref().unwrap();
        let w = bi.width;
        let h = bi.height;

        for y in 0..h {
            for x in 0..w {
                if cm.is_land.get(x, y) == 1 {
                    assert_ne!(
                        bi.get(x, y),
                        0,
                        "land cell ({x},{y}) must have nonzero basin_id"
                    );
                }
            }
        }
    }

    // ── test 3: every sea cell has zero basin id ──────────────────────────────

    #[test]
    fn every_sea_cell_has_zero_basin_id() {
        let world = run_full_pipeline(42);
        let bi = world.derived.basin_id.as_ref().unwrap();
        let cm = world.derived.coast_mask.as_ref().unwrap();
        let w = bi.width;
        let h = bi.height;

        for y in 0..h {
            for x in 0..w {
                if cm.is_sea.get(x, y) == 1 {
                    assert_eq!(
                        bi.get(x, y),
                        0,
                        "sea cell ({x},{y}) must have basin_id == 0"
                    );
                }
            }
        }
    }

    // ── test 4: basin consistency along flow ──────────────────────────────────
    //
    // For every non-sink land cell p, if its downstream q is also a land cell,
    // they must share the same basin id (monotone-along-flow invariant).
    // q may be a sea cell when a land cell flows directly into the ocean;
    // in that case basin_id[q] == 0 and the invariant doesn't apply to q.

    #[test]
    fn basin_consistency_along_flow() {
        let world = run_full_pipeline(42);
        let bi = world.derived.basin_id.as_ref().unwrap();
        let fd = world.derived.flow_dir.as_ref().unwrap();
        let cm = world.derived.coast_mask.as_ref().unwrap();
        let w = fd.width as i32;
        let h = fd.height as i32;

        for y in 0..fd.height {
            for x in 0..fd.width {
                let dir = fd.get(x, y);
                if dir == FLOW_DIR_SINK || cm.is_land.get(x, y) == 0 {
                    continue;
                }
                let (dx, dy) = crate::hydro::D8_OFFSETS[dir as usize];
                let qx = x as i32 + dx;
                let qy = y as i32 + dy;
                if qx < 0 || qx >= w || qy < 0 || qy >= h {
                    continue;
                }
                // Only assert consistency within the land domain.
                if cm.is_land.get(qx as u32, qy as u32) == 0 {
                    continue;
                }
                let id_p = bi.get(x, y);
                let id_q = bi.get(qx as u32, qy as u32);
                assert_eq!(
                    id_p, id_q,
                    "basin_id mismatch along flow: cell ({x},{y}) id={id_p} → ({qx},{qy}) id={id_q}"
                );
            }
        }
    }

    // ── test 5: deterministic ids across runs ─────────────────────────────────

    #[test]
    fn deterministic_ids_across_runs() {
        let w1 = run_full_pipeline(42);
        let w2 = run_full_pipeline(42);
        assert_eq!(
            w1.derived.basin_id.as_ref().unwrap().data,
            w2.derived.basin_id.as_ref().unwrap().data,
            "basin_id must be bit-exact across two identical runs"
        );
    }

    // ── test 6: sink reaches itself ───────────────────────────────────────────

    #[test]
    fn sink_reaches_itself() {
        let world = run_full_pipeline(42);
        let bi = world.derived.basin_id.as_ref().unwrap();
        let fd = world.derived.flow_dir.as_ref().unwrap();
        let cm = world.derived.coast_mask.as_ref().unwrap();
        let w = fd.width;
        let h = fd.height;

        for y in 0..h {
            for x in 0..w {
                if cm.is_land.get(x, y) == 1 && fd.get(x, y) == FLOW_DIR_SINK {
                    assert_ne!(
                        bi.get(x, y),
                        0,
                        "sink cell ({x},{y}) must have nonzero basin_id"
                    );
                }
            }
        }
    }

    // ── test: internal lake CC above threshold gets promoted ─────────────────
    //
    // An inland lake depression (cells with FLOW_DIR_SINK, is_coast=0) is not
    // included in the coastal-sink set and therefore starts at basin_id=0 after
    // the reverse-BFS. The CC pass must detect it and promote it when large
    // enough (>= MIN_INTERNAL_LAKE_CELLS = 8).
    //
    // Grid (7 × 3):
    //   col 0:   sea (is_sea=1)
    //   col 1:   coast sink (is_land=1, is_coast=1, FLOW_DIR_SINK) — 3 cells
    //   col 2:   land (is_land=1, is_coast=0, flow_dir=W) — drains to col 1
    //   cols 3-6: inland lake (is_land=1, is_coast=0, FLOW_DIR_SINK) — 4×3=12 cells
    //
    // The 12 inland-lake cells form a single Von4-connected CC.
    // 12 ≥ 8 → promoted to one fresh basin id.
    #[test]
    fn basin_promoted_for_internal_lake_above_threshold() {
        let w = 7_u32;
        let h = 3_u32;

        let mut is_land = MaskField2D::new(w, h);
        let mut is_sea = MaskField2D::new(w, h);
        let mut is_coast = MaskField2D::new(w, h);
        let mut flow_dir = ScalarField2D::<u8>::new(w, h);

        for y in 0..h {
            // col 0: sea
            is_sea.set(0, y, 1);
            flow_dir.set(0, y, FLOW_DIR_SINK);

            // col 1: coast sink (is_coast qualifies it as a coastal sink)
            is_land.set(1, y, 1);
            is_coast.set(1, y, 1);
            flow_dir.set(1, y, FLOW_DIR_SINK);

            // col 2: land, drains W into coast
            is_land.set(2, y, 1);
            flow_dir.set(2, y, 4); // W

            // cols 3..6: inland lake — FLOW_DIR_SINK but NOT coast
            for x in 3..w {
                is_land.set(x, y, 1);
                flow_dir.set(x, y, FLOW_DIR_SINK);
            }
        }

        let land_cell_count: u32 = is_land.data.iter().map(|&v| v as u32).sum();
        let coast_mask = CoastMask {
            is_land,
            is_sea,
            is_coast,
            land_cell_count,
            river_mouth_mask: None,
        };

        let mut world = WorldState::new(Seed(0), test_preset(), Resolution::new(w, h));
        world.derived.flow_dir = Some(flow_dir);
        world.derived.coast_mask = Some(coast_mask);

        BasinsStage.run(&mut world).expect("BasinsStage failed");

        let bi = world.derived.basin_id.as_ref().unwrap();

        // Coast cells (col 1) are coastal sinks → each gets a unique nonzero id.
        for y in 0..h {
            assert_ne!(bi.get(1, y), 0, "coast-sink (1,{y}) must have nonzero id");
        }

        // Col 2 drains W into col 1 → reverse-BFS reaches it and assigns coast id.
        for y in 0..h {
            assert_eq!(
                bi.get(2, y),
                bi.get(1, y),
                "cell (2,{y}) drains into coast (1,{y}) and must share its basin id"
            );
        }

        // Inland lake (cols 3..6): 4×3=12 cells, ≥ 8 → all promoted to one id.
        let lake_id = bi.get(3, 0);
        assert_ne!(
            lake_id, 0,
            "inland lake cells must be promoted to a nonzero id"
        );
        for y in 0..h {
            for x in 3..w {
                assert_eq!(
                    bi.get(x, y),
                    lake_id,
                    "inland lake cell ({x},{y}) must share promoted lake id {lake_id}"
                );
            }
        }
    }

    // ── test: internal lake CC below threshold stays at 0 ────────────────────
    //
    // Same structure but the inland lake is only 2 cells (< threshold=8) and
    // must remain at basin_id=0.
    //
    // Grid (5 × 1):
    //   col 0: sea
    //   col 1: coast sink (is_coast=1, FLOW_DIR_SINK)
    //   col 2: land, flow_dir=W
    //   cols 3-4: inland lake — 2 cells, FLOW_DIR_SINK, is_coast=0
    //
    // 2 < 8 → stays at 0.
    #[test]
    fn basin_lumped_for_internal_lake_below_threshold() {
        let w = 5_u32;
        let h = 1_u32;

        let mut is_land = MaskField2D::new(w, h);
        let mut is_sea = MaskField2D::new(w, h);
        let mut is_coast = MaskField2D::new(w, h);
        let mut flow_dir = ScalarField2D::<u8>::new(w, h);

        // col 0: sea
        is_sea.set(0, 0, 1);
        flow_dir.set(0, 0, FLOW_DIR_SINK);

        // col 1: coast sink
        is_land.set(1, 0, 1);
        is_coast.set(1, 0, 1);
        flow_dir.set(1, 0, FLOW_DIR_SINK);

        // col 2: land drains W
        is_land.set(2, 0, 1);
        flow_dir.set(2, 0, 4); // W

        // cols 3-4: inland lake, 2 cells only
        for x in 3..w {
            is_land.set(x, 0, 1);
            flow_dir.set(x, 0, FLOW_DIR_SINK);
        }

        let land_cell_count: u32 = is_land.data.iter().map(|&v| v as u32).sum();
        let coast_mask = CoastMask {
            is_land,
            is_sea,
            is_coast,
            land_cell_count,
            river_mouth_mask: None,
        };

        let mut world = WorldState::new(Seed(0), test_preset(), Resolution::new(w, h));
        world.derived.flow_dir = Some(flow_dir);
        world.derived.coast_mask = Some(coast_mask);

        BasinsStage.run(&mut world).expect("BasinsStage failed");

        let bi = world.derived.basin_id.as_ref().unwrap();

        // Inland lake (2 cells, below threshold=8) must stay at basin_id=0.
        for x in 3..w {
            assert_eq!(
                bi.get(x, 0),
                0,
                "inland lake cell ({x},0) with 2-cell CC must stay at 0 (below threshold)"
            );
        }

        // Coast sink and its upstream cell still have nonzero ids.
        assert_ne!(bi.get(1, 0), 0, "coast sink (1,0) must have nonzero id");
        assert_ne!(bi.get(2, 0), 0, "upstream cell (2,0) must have nonzero id");
    }

    // ── test 7a: errors when flow_dir is missing ──────────────────────────────

    #[test]
    fn errors_when_flow_dir_missing() {
        let mut world = WorldState::new(Seed(0), test_preset(), Resolution::new(8, 8));
        let mut is_sea = MaskField2D::new(8, 8);
        for v in is_sea.data.iter_mut() {
            *v = 1;
        }
        world.derived.coast_mask = Some(CoastMask {
            is_land: MaskField2D::new(8, 8),
            is_sea,
            is_coast: MaskField2D::new(8, 8),
            land_cell_count: 0,
            river_mouth_mask: None,
        });
        let result = BasinsStage.run(&mut world);
        assert!(result.is_err(), "expected Err when flow_dir is None");
    }

    // ── test 7b: errors when coast_mask is missing ────────────────────────────

    #[test]
    fn errors_when_coast_mask_missing() {
        let mut world = WorldState::new(Seed(0), test_preset(), Resolution::new(8, 8));
        world.derived.flow_dir = Some(ScalarField2D::<u8>::new(8, 8));
        let result = BasinsStage.run(&mut world);
        assert!(result.is_err(), "expected Err when coast_mask is None");
    }
}
