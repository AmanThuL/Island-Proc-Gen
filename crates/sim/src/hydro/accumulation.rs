//! D8 topological-sort flow accumulation — Task 1A.6.
//!
//! Reads `world.derived.flow_dir` (written by `FlowRoutingStage`) and
//! produces `world.derived.accumulation`: the upstream cell count A(x, y)
//! for every cell, including itself (minimum value is 1.0).
//!
//! `AccumulationStage` does NOT need `coast_mask` — coast and sea semantics
//! are already baked into `FLOW_DIR_SINK` by `FlowRoutingStage`.

use std::collections::VecDeque;

use island_core::field::ScalarField2D;
use island_core::pipeline::SimulationStage;
use island_core::world::WorldState;

use super::{D8_OFFSETS, FLOW_DIR_SINK};

// ─── AccumulationStage ────────────────────────────────────────────────────────

/// Sprint 1A Task 1A.6: upstream cell count via topological-sort on the D8 DAG.
///
/// Each cell starts at `A = 1.0` (itself). The stage propagates contributions
/// downstream in topological order. Sinks (coast, sea, 0xFF) accumulate all
/// upstream flow but do not propagate further.
pub struct AccumulationStage;

impl SimulationStage for AccumulationStage {
    fn name(&self) -> &'static str {
        "accumulation"
    }

    fn run(&self, world: &mut WorldState) -> anyhow::Result<()> {
        let flow_dir = world.derived.flow_dir.as_ref().ok_or_else(|| {
            anyhow::anyhow!(
                "AccumulationStage: derived.flow_dir is None (FlowRoutingStage must run first)"
            )
        })?;

        let w = flow_dir.width as usize;
        let h = flow_dir.height as usize;
        let n = w * h;

        // ── in-degree pass ────────────────────────────────────────────────────
        // Count how many upstream cells point to each cell.
        let mut indeg: Vec<u32> = vec![0; n];

        for y in 0..h {
            for x in 0..w {
                let dir = flow_dir.get(x as u32, y as u32);
                if let Some((qx, qy)) = downstream_cell(x as i32, y as i32, dir, w as i32, h as i32)
                {
                    indeg[qy as usize * w + qx as usize] += 1;
                }
            }
        }

        // ── topo-sort accumulation ────────────────────────────────────────────
        let mut accum: Vec<f32> = vec![1.0; n];

        // Seed queue with all cells that have no upstream sources.
        let mut queue: VecDeque<u32> = (0..n as u32).filter(|&p| indeg[p as usize] == 0).collect();

        while let Some(p) = queue.pop_front() {
            let x = p as usize % w;
            let y = p as usize / w;
            let dir = flow_dir.get(x as u32, y as u32);

            let Some((qx, qy)) = downstream_cell(x as i32, y as i32, dir, w as i32, h as i32)
            else {
                // Sink — no downstream to propagate to.
                continue;
            };

            let q = qy as usize * w + qx as usize;
            accum[q] += accum[p as usize];
            indeg[q] -= 1;
            if indeg[q] == 0 {
                queue.push_back(q as u32);
            }
        }

        let mut out = ScalarField2D::<f32>::new(flow_dir.width, flow_dir.height);
        out.data = accum;

        world.derived.accumulation = Some(out);
        Ok(())
    }
}

// ─── helpers ──────────────────────────────────────────────────────────────────

/// Resolve the downstream cell index given a D8 direction code.
///
/// Returns `None` for sinks (0xFF) or if the computed neighbour falls outside
/// the grid (pathological — D8 routing never picks OOB neighbours).
#[inline]
fn downstream_cell(x: i32, y: i32, dir: u8, w: i32, h: i32) -> Option<(u32, u32)> {
    if dir == FLOW_DIR_SINK {
        return None;
    }
    debug_assert!(
        (dir as usize) < D8_OFFSETS.len(),
        "flow_dir contains invalid direction {dir}; FlowRoutingStage contract violated"
    );
    let (dx, dy) = D8_OFFSETS[dir as usize];
    let qx = x + dx;
    let qy = y + dy;
    if qx < 0 || qx >= w || qy < 0 || qy >= h {
        return None;
    }
    Some((qx as u32, qy as u32))
}

// ─── tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use island_core::field::ScalarField2D;
    use island_core::pipeline::SimulationStage;
    use island_core::preset::{IslandAge, IslandArchetypePreset};
    use island_core::seed::Seed;
    use island_core::world::{Resolution, WorldState};

    use super::AccumulationStage;
    use crate::geomorph::{CoastMaskStage, PitFillStage, TopographyStage};
    use crate::hydro::{FLOW_DIR_SINK, FlowRoutingStage};

    // ── helpers ───────────────────────────────────────────────────────────────

    fn test_preset() -> IslandArchetypePreset {
        IslandArchetypePreset {
            name: "accum_test".into(),
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

    /// Run the full pipeline through AccumulationStage on a 64×64 world.
    fn run_full_pipeline(seed: u64) -> WorldState {
        let mut world = WorldState::new(Seed(seed), test_preset(), Resolution::new(64, 64));
        TopographyStage.run(&mut world).expect("TopographyStage");
        CoastMaskStage.run(&mut world).expect("CoastMaskStage");
        PitFillStage.run(&mut world).expect("PitFillStage");
        FlowRoutingStage.run(&mut world).expect("FlowRoutingStage");
        AccumulationStage
            .run(&mut world)
            .expect("AccumulationStage");
        world
    }

    // ── test 1: hand-computed 3×3 accumulation ───────────────────────────────
    //
    // Grid (x=col, y=row), flow directions set explicitly:
    //   (0,0)=SE(7)  (1,0)=S(6)   (2,0)=S(6)
    //   (0,1)=SE(7)  (1,1)=SE(7)  (2,1)=S(6)
    //   (0,2)=E(0)   (1,2)=E(0)   (2,2)=SINK
    //
    // D8_OFFSETS: SE(7) = (+1,+1), S(6) = (0,+1), E(0) = (+1,0)
    //
    // Flow graph (x,y → downstream x,y):
    //   (0,0) SE→ (1,1)
    //   (1,0) S → (1,1)
    //   (2,0) S → (2,1)
    //   (0,1) SE→ (1,2)   ← NOT (1,1); SE from row 1 goes to row 2
    //   (1,1) SE→ (2,2)
    //   (2,1) S → (2,2)
    //   (0,2) E → (1,2)
    //   (1,2) E → (2,2)
    //
    // Accumulation (resolve in upstream-first order):
    //   A[0][0]=1, A[1][0]=1, A[2][0]=1, A[0][1]=1, A[0][2]=1
    //   A[1][1] = 1 + A[0][0] + A[1][0]         = 3
    //   A[2][1] = 1 + A[2][0]                    = 2
    //   A[1][2] = 1 + A[0][1] + A[0][2]          = 3
    //   A[2][2] = 1 + A[1][1] + A[2][1] + A[1][2] = 1+3+2+3 = 9  ✓ (9 total cells)
    #[test]
    fn hand_computed_3x3_accumulation() {
        let w = 3_u32;
        let h = 3_u32;

        // Build flow_dir manually with explicit direction codes.
        let dirs: [[u8; 3]; 3] = [
            [7, 6, 6],             // y=0: SE, S, S
            [7, 7, 6],             // y=1: SE, SE, S
            [0, 0, FLOW_DIR_SINK], // y=2: E, E, SINK
        ];
        let mut flow_dir = ScalarField2D::<u8>::new(w, h);
        for y in 0..h {
            for x in 0..w {
                flow_dir.set(x, y, dirs[y as usize][x as usize]);
            }
        }

        let mut world = WorldState::new(Seed(0), test_preset(), Resolution::new(w, h));
        world.derived.flow_dir = Some(flow_dir);

        AccumulationStage
            .run(&mut world)
            .expect("AccumulationStage failed");

        let acc = world.derived.accumulation.as_ref().unwrap();

        assert_eq!(acc.get(0, 0), 1.0, "A[0][0] must be 1 (no upstream)");
        assert_eq!(acc.get(1, 1), 3.0, "A[1][1] must be 3");
        assert_eq!(acc.get(2, 1), 2.0, "A[2][1] must be 2");
        assert_eq!(acc.get(1, 2), 3.0, "A[1][2] must be 3");
        assert_eq!(acc.get(2, 2), 9.0, "A[2][2] must be 9 (all 9 cells)");
    }

    // ── test 2: monotonicity — A[downstream] >= A[upstream] ──────────────────

    #[test]
    fn monotonicity_a_ge_upstream() {
        let world = run_full_pipeline(42);
        let fd = world.derived.flow_dir.as_ref().unwrap();
        let acc = world.derived.accumulation.as_ref().unwrap();
        let w = fd.width;
        let h = fd.height;

        for y in 0..h {
            for x in 0..w {
                let dir = fd.get(x, y);
                if dir == FLOW_DIR_SINK {
                    continue;
                }
                let (dx, dy) = crate::hydro::D8_OFFSETS[dir as usize];
                let qx = x as i32 + dx;
                let qy = y as i32 + dy;
                if qx < 0 || qx >= w as i32 || qy < 0 || qy >= h as i32 {
                    continue;
                }
                let a_p = acc.get(x, y);
                let a_q = acc.get(qx as u32, qy as u32);
                assert!(
                    a_q >= a_p,
                    "monotonicity violated: A[{x},{y}]={a_p} > A[{qx},{qy}]={a_q}"
                );
            }
        }
    }

    // ── test 3: sum of sink accumulations == total cell count ─────────────────
    //
    // Every cell contributes 1.0 to exactly one sink. So summing A over all
    // sink cells must equal the total cell count.

    #[test]
    fn total_sink_sum_equals_cell_count() {
        let world = run_full_pipeline(42);
        let fd = world.derived.flow_dir.as_ref().unwrap();
        let acc = world.derived.accumulation.as_ref().unwrap();
        let w = fd.width;
        let h = fd.height;
        let n = (w * h) as f32;

        let sink_sum: f32 = (0..h)
            .flat_map(|y| (0..w).map(move |x| (x, y)))
            .filter(|&(x, y)| fd.get(x, y) == FLOW_DIR_SINK)
            .map(|(x, y)| acc.get(x, y))
            .sum();

        assert!(
            (sink_sum - n).abs() < 0.5,
            "sum of sink A values {sink_sum} must equal cell count {n}"
        );
    }

    // ── test 4: bit-exact determinism ─────────────────────────────────────────

    #[test]
    fn bit_exact_determinism() {
        let w1 = run_full_pipeline(42);
        let w2 = run_full_pipeline(42);
        assert_eq!(
            w1.derived.accumulation.as_ref().unwrap().data,
            w2.derived.accumulation.as_ref().unwrap().data,
            "accumulation must be bit-exact across two identical runs"
        );
    }

    // ── test 5: error when flow_dir is missing ────────────────────────────────

    #[test]
    fn errors_when_flow_dir_missing() {
        let mut world = WorldState::new(Seed(0), test_preset(), Resolution::new(8, 8));
        // flow_dir deliberately None
        let result = AccumulationStage.run(&mut world);
        assert!(result.is_err(), "expected Err when flow_dir is None");
    }
}
