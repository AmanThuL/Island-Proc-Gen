//! D8 flow-direction stage — Task 1A.5.
//!
//! Reads `world.derived.z_filled` (pit-filled terrain) and
//! `world.derived.coast_mask`, computes a per-cell D8 downstream direction,
//! and writes `world.derived.flow_dir`.
//!
//! Tiebreak noise is applied to a stage-local scratch buffer (`z_flow`) that
//! is never written back to `WorldState`.

use rand::Rng;

use island_core::field::ScalarField2D;
use island_core::pipeline::SimulationStage;
use island_core::world::WorldState;

use super::{D8_OFFSETS, FLOW_DIR_SINK};

// ─── constants ────────────────────────────────────────────────────────────────

/// RNG stream for the per-cell tiebreak jitter scratch.
///
/// Jitter is a routing helper only — it never enters `WorldState`.
pub(crate) const FLOW_ROUTING_TIEBREAK_STREAM: u64 = 0x0001_0A05;

// ─── FlowRoutingStage ─────────────────────────────────────────────────────────

/// Sprint 1A Task 1A.5: D8 downstream direction from pit-filled terrain.
///
/// Each interior land cell is assigned a direction index 0–7 (see
/// [`D8_OFFSETS`]) pointing to its steepest downhill Moore8 neighbour.
/// Coast cells and sea cells receive [`FLOW_DIR_SINK`] (0xFF).
pub struct FlowRoutingStage;

impl SimulationStage for FlowRoutingStage {
    fn name(&self) -> &'static str {
        "flow_routing"
    }

    fn run(&self, world: &mut WorldState) -> anyhow::Result<()> {
        let z_filled = world.derived.z_filled.as_ref().ok_or_else(|| {
            anyhow::anyhow!(
                "FlowRoutingStage: derived.z_filled is None (PitFillStage must run first)"
            )
        })?;

        let coast_mask = world.derived.coast_mask.as_ref().ok_or_else(|| {
            anyhow::anyhow!(
                "FlowRoutingStage: derived.coast_mask is None (CoastMaskStage must run first)"
            )
        })?;

        let w = z_filled.width as usize;
        let h = z_filled.height as usize;

        // z_flow is stage-local: z_filled + tiny per-cell jitter.
        // It never enters WorldState — jitter is a routing helper only.
        let mut rng = world.seed.fork(FLOW_ROUTING_TIEBREAK_STREAM).to_rng();
        let z_flow: Vec<f32> = z_filled
            .data
            .iter()
            .map(|&z| z + rng.random_range(-1.0..1.0) * 1e-6)
            .collect();

        let mut flow_dir = ScalarField2D::<u8>::new(z_filled.width, z_filled.height);

        for y in 0..h {
            for x in 0..w {
                let xu = x as u32;
                let yu = y as u32;

                // Coast and sea are outlets — they drain flow, not route it.
                if coast_mask.is_coast.get(xu, yu) == 1 || coast_mask.is_sea.get(xu, yu) == 1 {
                    flow_dir.set(xu, yu, FLOW_DIR_SINK);
                    continue;
                }

                let p = y * w + x;
                let mut best_slope = 0.0_f32;
                let mut best_dir: u8 = FLOW_DIR_SINK;

                for (i, &(dx, dy)) in D8_OFFSETS.iter().enumerate() {
                    let nx = x as i32 + dx;
                    let ny = y as i32 + dy;
                    if nx < 0 || nx >= w as i32 || ny < 0 || ny >= h as i32 {
                        continue;
                    }
                    let q = ny as usize * w + nx as usize;
                    let cell_dist = if dx.abs() + dy.abs() == 2 {
                        std::f32::consts::SQRT_2
                    } else {
                        1.0_f32
                    };
                    let slope = (z_flow[p] - z_flow[q]) / cell_dist;
                    if slope > best_slope {
                        best_slope = slope;
                        best_dir = i as u8;
                    }
                }

                flow_dir.set(xu, yu, best_dir);
            }
        }

        world.derived.flow_dir = Some(flow_dir);
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

    use super::FlowRoutingStage;
    use crate::geomorph::{CoastMaskStage, PitFillStage, TopographyStage};
    use crate::hydro::FLOW_DIR_SINK;

    // ── helpers ───────────────────────────────────────────────────────────────

    fn test_preset() -> IslandArchetypePreset {
        IslandArchetypePreset {
            name: "flow_routing_test".into(),
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

    /// Run the full pipeline through FlowRoutingStage on a 64×64 world.
    fn run_full_pipeline(seed: u64) -> WorldState {
        let mut world = WorldState::new(Seed(seed), test_preset(), Resolution::new(64, 64));
        TopographyStage.run(&mut world).expect("TopographyStage");
        CoastMaskStage.run(&mut world).expect("CoastMaskStage");
        PitFillStage.run(&mut world).expect("PitFillStage");
        FlowRoutingStage.run(&mut world).expect("FlowRoutingStage");
        world
    }

    // ── test 1: hand-computed 3×3 flow directions ─────────────────────────────
    //
    // Grid (x=col, y=row):
    //   (0,0)=9  (1,0)=7  (2,0)=5
    //   (0,1)=6  (1,1)=4  (2,1)=2
    //   (0,2)=3  (1,2)=1  (2,2)=0  ← coast (outlet)
    //
    // All cells are land; (2,2) is marked as coast (global minimum).
    // No jitter stream is seeded from Seed(0) so results are deterministic.
    //
    // Hand-verified directions (D8_OFFSETS order: E=0,NE=1,N=2,NW=3,W=4,SW=5,S=6,SE=7):
    //   (2,2) → SINK (coast)
    //   (1,1) z=4: slope_S  = (4-1)/1.0 = 3.0 > slope_SE = (4-0)/√2 ≈ 2.83 → S (6)
    //   (0,0) z=9: slope_SE = (9-4)/√2 ≈ 3.54 > slope_S = (9-6)/1 = 3.0  → SE (7)
    //   (2,0) z=5: only downhill neighbour is S=(2,1)=2, slope=3.0           → S  (6)
    //   (2,1) z=2: slope_S  = (2-0)/1.0 = 2.0 > slope_SW = (2-1)/√2 ≈ 0.71 → S  (6)
    #[test]
    fn hand_computed_3x3_flow_directions() {
        let w = 3_u32;
        let h = 3_u32;

        // Build z_filled manually — bypass TopographyStage.
        let z_vals: [[f32; 3]; 3] = [
            [9.0, 7.0, 5.0], // y=0
            [6.0, 4.0, 2.0], // y=1
            [3.0, 1.0, 0.0], // y=2
        ];
        let mut z_filled = ScalarField2D::<f32>::new(w, h);
        for y in 0..h {
            for x in 0..w {
                z_filled.set(x, y, z_vals[y as usize][x as usize]);
            }
        }

        // All cells are land; only (2,2) is coast.
        let n = (w * h) as usize;
        let mut is_land = MaskField2D::new(w, h);
        let is_sea = MaskField2D::new(w, h);
        let mut is_coast = MaskField2D::new(w, h);
        for i in 0..n {
            is_land.data[i] = 1;
        }
        is_coast.set(2, 2, 1);

        let coast_mask = CoastMask {
            is_land,
            is_sea,
            is_coast,
            land_cell_count: n as u32,
            river_mouth_mask: None,
        };

        let mut world = WorldState::new(Seed(0), test_preset(), Resolution::new(w, h));
        world.derived.z_filled = Some(z_filled);
        world.derived.coast_mask = Some(coast_mask);

        FlowRoutingStage
            .run(&mut world)
            .expect("FlowRoutingStage failed");

        let fd = world.derived.flow_dir.as_ref().unwrap();

        // (2,2) is coast → SINK
        assert_eq!(fd.get(2, 2), FLOW_DIR_SINK, "(2,2) coast must be SINK");

        // (1,1): slope_S = 3.0 beats slope_SE ≈ 2.83 → direction 6 (S)
        assert_eq!(fd.get(1, 1), 6, "(1,1) must flow S (index 6)");

        // (0,0): slope_SE ≈ 3.54 beats slope_S = 3.0 → direction 7 (SE)
        assert_eq!(fd.get(0, 0), 7, "(0,0) must flow SE (index 7)");

        // (2,0): only downhill is S=(2,1)=2, slope=3.0 → direction 6 (S)
        assert_eq!(fd.get(2, 0), 6, "(2,0) must flow S (index 6)");

        // (2,1): slope_S=2.0 beats slope_SW≈0.71 → direction 6 (S)
        assert_eq!(fd.get(2, 1), 6, "(2,1) must flow S (index 6)");
    }

    // ── test 2: no sink on non-coast land after pit fill ──────────────────────

    #[test]
    fn no_sink_on_non_coast_land_after_pit_fill() {
        let world = run_full_pipeline(42);
        let fd = world.derived.flow_dir.as_ref().unwrap();
        let cm = world.derived.coast_mask.as_ref().unwrap();
        let w = fd.width;
        let h = fd.height;

        for y in 0..h {
            for x in 0..w {
                if cm.is_land.get(x, y) == 1 && cm.is_coast.get(x, y) == 0 {
                    assert_ne!(
                        fd.get(x, y),
                        FLOW_DIR_SINK,
                        "interior land cell ({x},{y}) must not be a sink after pit fill"
                    );
                }
            }
        }
    }

    // ── test 3: coast cells are sinks ─────────────────────────────────────────

    #[test]
    fn coast_cells_are_sinks() {
        let world = run_full_pipeline(42);
        let fd = world.derived.flow_dir.as_ref().unwrap();
        let cm = world.derived.coast_mask.as_ref().unwrap();
        let w = fd.width;
        let h = fd.height;

        for y in 0..h {
            for x in 0..w {
                if cm.is_coast.get(x, y) == 1 {
                    assert_eq!(
                        fd.get(x, y),
                        FLOW_DIR_SINK,
                        "coast cell ({x},{y}) must be SINK"
                    );
                }
            }
        }
    }

    // ── test 4: sea cells are sinks ───────────────────────────────────────────

    #[test]
    fn sea_cells_are_sinks() {
        let world = run_full_pipeline(42);
        let fd = world.derived.flow_dir.as_ref().unwrap();
        let cm = world.derived.coast_mask.as_ref().unwrap();
        let w = fd.width;
        let h = fd.height;

        for y in 0..h {
            for x in 0..w {
                if cm.is_sea.get(x, y) == 1 {
                    assert_eq!(
                        fd.get(x, y),
                        FLOW_DIR_SINK,
                        "sea cell ({x},{y}) must be SINK"
                    );
                }
            }
        }
    }

    // ── test 5: determinism — bit-exact across two runs ───────────────────────

    #[test]
    fn determinism_bit_exact() {
        let w1 = run_full_pipeline(42);
        let w2 = run_full_pipeline(42);
        assert_eq!(
            w1.derived.flow_dir.as_ref().unwrap().data,
            w2.derived.flow_dir.as_ref().unwrap().data,
            "flow_dir must be bit-exact across two identical runs"
        );
    }

    // ── test 6: jitter does not leak into z_filled ────────────────────────────

    #[test]
    fn jitter_does_not_leak_into_world_state() {
        let preset = test_preset();
        let mut world = WorldState::new(Seed(42), preset, Resolution::new(64, 64));
        TopographyStage.run(&mut world).expect("TopographyStage");
        CoastMaskStage.run(&mut world).expect("CoastMaskStage");
        PitFillStage.run(&mut world).expect("PitFillStage");

        let z_before = world.derived.z_filled.as_ref().unwrap().data.clone();
        FlowRoutingStage.run(&mut world).expect("FlowRoutingStage");
        let z_after = world.derived.z_filled.as_ref().unwrap().data.clone();

        assert_eq!(
            z_before, z_after,
            "z_filled must be byte-identical before and after FlowRoutingStage"
        );
    }

    // ── test 7: jitter does not leak into coast_mask ──────────────────────────

    #[test]
    fn jitter_does_not_leak_to_coast_mask() {
        let preset = test_preset();
        let mut world = WorldState::new(Seed(42), preset, Resolution::new(64, 64));
        TopographyStage.run(&mut world).expect("TopographyStage");
        CoastMaskStage.run(&mut world).expect("CoastMaskStage");
        PitFillStage.run(&mut world).expect("PitFillStage");

        let is_land_before = world
            .derived
            .coast_mask
            .as_ref()
            .unwrap()
            .is_land
            .data
            .clone();
        let is_sea_before = world
            .derived
            .coast_mask
            .as_ref()
            .unwrap()
            .is_sea
            .data
            .clone();
        let is_coast_before = world
            .derived
            .coast_mask
            .as_ref()
            .unwrap()
            .is_coast
            .data
            .clone();

        FlowRoutingStage.run(&mut world).expect("FlowRoutingStage");

        let cm = world.derived.coast_mask.as_ref().unwrap();
        assert_eq!(is_land_before, cm.is_land.data, "is_land must be unchanged");
        assert_eq!(is_sea_before, cm.is_sea.data, "is_sea must be unchanged");
        assert_eq!(
            is_coast_before, cm.is_coast.data,
            "is_coast must be unchanged"
        );
    }

    // ── test 8a: errors when z_filled is missing ──────────────────────────────

    #[test]
    fn errors_when_z_filled_missing() {
        let mut world = WorldState::new(Seed(0), test_preset(), Resolution::new(8, 8));
        // coast_mask present but z_filled absent
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
        // z_filled deliberately None
        let result = FlowRoutingStage.run(&mut world);
        assert!(result.is_err(), "expected Err when z_filled is None");
    }

    // ── test 8b: errors when coast_mask is missing ────────────────────────────

    #[test]
    fn errors_when_coast_mask_missing() {
        let mut world = WorldState::new(Seed(0), test_preset(), Resolution::new(8, 8));
        // z_filled present but coast_mask absent
        let mut z_filled = ScalarField2D::<f32>::new(8, 8);
        for i in 0..64 {
            z_filled.data[i] = 0.5;
        }
        world.derived.z_filled = Some(z_filled);
        // coast_mask deliberately None
        let result = FlowRoutingStage.run(&mut world);
        assert!(result.is_err(), "expected Err when coast_mask is None");
    }
}
