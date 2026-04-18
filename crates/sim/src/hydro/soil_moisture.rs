//! `SoilMoistureStage` (DD5) — composite `[0, 1]` soil-moisture proxy.
//!
//! `theta` is a weighted sum of three cheap indicators plus a single
//! downstream smoothing pass along `flow_dir`:
//!
//! ```text
//! theta_raw(x, y) = W_ET    * clamp(ET / PET, 0, 1)
//!                 + W_ACC   * log(A + 1) / log(A_max + 1)
//!                 + W_RIVER * exp(-dist_to_river / RIVER_DECAY)
//!
//! theta(x, y) = 0.75 * theta_raw(x, y) + 0.25 * theta_raw(down(x, y))
//!     if flow_dir[(x, y)] != FLOW_DIR_SINK and is_land(down)
//! theta(x, y) = theta_raw(x, y)
//!     otherwise
//! ```
//!
//! Weights `W_ET=0.5, W_ACC=0.3, W_RIVER=0.2` sum to `1.0` so the raw
//! field is a convex combination of three `[0, 1]`-bounded indicators
//! and is already bounded `[0, 1]` without any outer `normalize()`
//! wrapper. The downstream smoothing pass is itself a convex
//! combination of two `[0, 1]` values and preserves the bound.
//!
//! The sprint doc formula prefaces the weighted sum with a
//! `normalize(...)` call; we intentionally skip that outer step
//! because the weights already partition unity and a min-max stretch
//! would destroy the physical interpretation of `theta` as an
//! "absolute fraction of saturation". Stripping the stretch keeps
//! DD6 biome suitability thresholds comparable across runs.
//!
//! `RIVER_DECAY = 0.02` is the spec value with `dist_to_river`
//! normalised by `min(sim_width, sim_height)`. On square domains
//! (the common case) the effective decay length is exactly the same
//! as the spec; on rectangular domains the decay is slightly
//! anisotropic toward the shorter axis. Task 1B.9 / Sprint 2 may
//! revisit if non-square previews start to matter.
//!
//! The `flow_dir` consumption makes good on the Sprint 1A hand-off
//! contract (the basin-id / flow_dir fields were built in 1A
//! specifically so 1B stages can walk water information along the
//! hydro graph, not just read scalar accumulation).

use anyhow::anyhow;
use island_core::field::ScalarField2D;
use island_core::pipeline::SimulationStage;
use island_core::world::{D8_OFFSETS, FLOW_DIR_SINK, WorldState};

use crate::climate::common::compute_distance_to_mask;

pub(crate) const W_ET: f32 = 0.5;
pub(crate) const W_ACC: f32 = 0.3;
pub(crate) const W_RIVER: f32 = 0.2;

/// Normalized decay length for river-proximity falloff (fraction of
/// the smaller domain dimension).
pub(crate) const RIVER_DECAY: f32 = 0.02;

/// Weight on the current cell's own raw value in the downstream
/// smoothing pass. The remaining weight goes to the D8 downstream
/// neighbour.
pub(crate) const SMOOTH_SELF_WEIGHT: f32 = 0.75;

/// DD5: populate `world.baked.soil_moisture`.
pub struct SoilMoistureStage;

impl SimulationStage for SoilMoistureStage {
    fn name(&self) -> &'static str {
        "soil_moisture"
    }

    fn run(&self, world: &mut WorldState) -> anyhow::Result<()> {
        let et = world
            .derived
            .et
            .as_ref()
            .ok_or_else(|| anyhow!("SoilMoistureStage: derived.et is None"))?;
        let pet = world
            .derived
            .pet
            .as_ref()
            .ok_or_else(|| anyhow!("SoilMoistureStage: derived.pet is None"))?;
        let accum = world
            .derived
            .accumulation
            .as_ref()
            .ok_or_else(|| anyhow!("SoilMoistureStage: derived.accumulation is None"))?;
        let river_mask = world
            .derived
            .river_mask
            .as_ref()
            .ok_or_else(|| anyhow!("SoilMoistureStage: derived.river_mask is None"))?;
        let flow_dir = world
            .derived
            .flow_dir
            .as_ref()
            .ok_or_else(|| anyhow!("SoilMoistureStage: derived.flow_dir is None"))?;
        let coast = world
            .derived
            .coast_mask
            .as_ref()
            .ok_or_else(|| anyhow!("SoilMoistureStage: coast_mask is None"))?;

        let w = et.width;
        let h = et.height;

        // Precompute distance to any river cell. River-free domains get
        // f32::MAX everywhere, making `exp(-dist/decay)` round to 0 —
        // the stage still produces a valid field.
        let dist_to_river = compute_distance_to_mask(river_mask, w, h);
        let dist_norm = w.min(h) as f32;

        // Maximum accumulation across land cells for log-compression.
        let mut a_max = 0.0_f32;
        for iy in 0..h {
            for ix in 0..w {
                if coast.is_land.get(ix, iy) == 1 {
                    a_max = a_max.max(accum.get(ix, iy));
                }
            }
        }
        let log_denom = (a_max + 1.0).ln().max(f32::EPSILON);

        // Raw composite field.
        let mut raw = ScalarField2D::<f32>::new(w, h);
        for iy in 0..h {
            for ix in 0..w {
                if coast.is_land.get(ix, iy) != 1 {
                    continue;
                }

                let et_val = et.get(ix, iy);
                let pet_val = pet.get(ix, iy);
                let et_over_pet = if pet_val > 0.0 {
                    (et_val / pet_val).clamp(0.0, 1.0)
                } else {
                    0.0
                };

                let acc_term = (accum.get(ix, iy) + 1.0).ln() / log_denom;
                let river_term = (-dist_to_river.get(ix, iy) / dist_norm / RIVER_DECAY).exp();

                let value = W_ET * et_over_pet + W_ACC * acc_term + W_RIVER * river_term;
                raw.set(ix, iy, value);
            }
        }

        // 1-pass downstream smoothing along flow_dir.
        let mut smoothed = ScalarField2D::<f32>::new(w, h);
        let other_weight = 1.0 - SMOOTH_SELF_WEIGHT;
        for iy in 0..h {
            for ix in 0..w {
                if coast.is_land.get(ix, iy) != 1 {
                    continue;
                }

                let self_val = raw.get(ix, iy);
                let dir = flow_dir.get(ix, iy);
                if dir == FLOW_DIR_SINK {
                    smoothed.set(ix, iy, self_val);
                    continue;
                }
                let (dx, dy) = D8_OFFSETS[dir as usize];
                let qx = ix as i32 + dx;
                let qy = iy as i32 + dy;
                if qx < 0 || qy < 0 || qx >= w as i32 || qy >= h as i32 {
                    smoothed.set(ix, iy, self_val);
                    continue;
                }
                let qxu = qx as u32;
                let qyu = qy as u32;
                if coast.is_land.get(qxu, qyu) != 1 {
                    smoothed.set(ix, iy, self_val);
                    continue;
                }

                let neighbour_val = raw.get(qxu, qyu);
                smoothed.set(
                    ix,
                    iy,
                    (SMOOTH_SELF_WEIGHT * self_val + other_weight * neighbour_val).clamp(0.0, 1.0),
                );
            }
        }

        world.baked.soil_moisture = Some(smoothed);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use island_core::field::{MaskField2D, ScalarField2D};
    use island_core::preset::{IslandAge, IslandArchetypePreset};
    use island_core::seed::Seed;
    use island_core::world::{CoastMask, Resolution, WorldState};

    fn preset() -> IslandArchetypePreset {
        IslandArchetypePreset {
            name: "sm_test".into(),
            island_radius: 0.5,
            max_relief: 1.0,
            volcanic_center_count: 1,
            island_age: IslandAge::Young,
            prevailing_wind_dir: 0.0,
            marine_moisture_strength: 1.0,
            sea_level: 0.0,
            erosion: Default::default(),
        }
    }

    /// Minimal all-land world. All hydro inputs default to empty fields,
    /// so individual tests overwrite just the ones they care about.
    fn land_world(w: u32, h: u32) -> WorldState {
        let mut world = WorldState::new(Seed(0), preset(), Resolution::new(w, h));

        let mut is_land = MaskField2D::new(w, h);
        is_land.data.fill(1);
        world.derived.coast_mask = Some(CoastMask {
            is_land,
            is_sea: MaskField2D::new(w, h),
            is_coast: MaskField2D::new(w, h),
            land_cell_count: w * h,
            river_mouth_mask: None,
        });
        world.derived.et = Some(ScalarField2D::<f32>::new(w, h));
        world.derived.pet = Some(ScalarField2D::<f32>::new(w, h));
        world.derived.accumulation = Some(ScalarField2D::<f32>::new(w, h));
        world.derived.river_mask = Some(MaskField2D::new(w, h));
        let mut flow_dir = ScalarField2D::<u8>::new(w, h);
        flow_dir.data.fill(FLOW_DIR_SINK);
        world.derived.flow_dir = Some(flow_dir);
        world
    }

    #[test]
    fn zero_inputs_yield_zero_field() {
        let mut world = land_world(8, 8);
        SoilMoistureStage.run(&mut world).expect("stage");
        let theta = world.baked.soil_moisture.as_ref().unwrap();
        assert_eq!(theta.stats().unwrap().max, 0.0);
    }

    #[test]
    fn et_over_pet_contribution() {
        let (w, h) = (4, 4);
        let mut world = land_world(w, h);
        // ET = PET → et_over_pet = 1.0. No accumulation, no river. Expected
        // theta = W_ET * 1 = 0.5 everywhere (before smoothing; since every
        // cell has no downstream, smoothing is the identity).
        let mut et = ScalarField2D::<f32>::new(w, h);
        et.data.fill(0.3);
        let mut pet = ScalarField2D::<f32>::new(w, h);
        pet.data.fill(0.3);
        world.derived.et = Some(et);
        world.derived.pet = Some(pet);

        SoilMoistureStage.run(&mut world).expect("stage");
        let theta = world.baked.soil_moisture.as_ref().unwrap();
        for v in theta.data.iter() {
            assert!((v - W_ET).abs() < 1e-6, "expected {W_ET}, got {v}");
        }
    }

    #[test]
    fn river_proximity_contribution() {
        let (w, h) = (8, 8);
        let mut world = land_world(w, h);

        // Mark a single river cell at (3, 3). Cells on top of it get
        // exp(0) = 1 contribution * W_RIVER = 0.2.
        let mut river_mask = MaskField2D::new(w, h);
        river_mask.set(3, 3, 1);
        world.derived.river_mask = Some(river_mask);

        SoilMoistureStage.run(&mut world).expect("stage");
        let theta = world.baked.soil_moisture.as_ref().unwrap();
        assert!((theta.get(3, 3) - W_RIVER).abs() < 1e-4);

        // A cell far away should be near 0 (exp(-large) ≈ 0).
        let far = theta.get(7, 0);
        assert!(far < 0.01);
    }

    #[test]
    fn downstream_smoothing_mixes_with_neighbour() {
        let (w, h) = (4, 4);
        let mut world = land_world(w, h);

        // Cell (1,2) has ET/PET = 1 → raw = 0.5.
        // Cell (2,2) (its downstream via flow_dir=E=0) has ET/PET = 0 → raw = 0.
        // Smoothed value at (1,2) = 0.75 * 0.5 + 0.25 * 0 = 0.375.
        let mut et = ScalarField2D::<f32>::new(w, h);
        let mut pet = ScalarField2D::<f32>::new(w, h);
        et.set(1, 2, 0.4);
        pet.set(1, 2, 0.4);
        world.derived.et = Some(et);
        world.derived.pet = Some(pet);

        let mut flow_dir = ScalarField2D::<u8>::new(w, h);
        flow_dir.data.fill(FLOW_DIR_SINK);
        flow_dir.set(1, 2, 0); // point (1,2) east to (2,2)
        world.derived.flow_dir = Some(flow_dir);

        SoilMoistureStage.run(&mut world).expect("stage");
        let theta = world.baked.soil_moisture.as_ref().unwrap();

        let expected = SMOOTH_SELF_WEIGHT * W_ET + (1.0 - SMOOTH_SELF_WEIGHT) * 0.0;
        assert!(
            (theta.get(1, 2) - expected).abs() < 1e-5,
            "smoothed cell should mix: expected {expected}, got {}",
            theta.get(1, 2)
        );
    }

    #[test]
    fn soil_moisture_determinism() {
        let build = || {
            let (w, h) = (8, 8);
            let mut world = land_world(w, h);
            let mut et = ScalarField2D::<f32>::new(w, h);
            et.data.fill(0.3);
            let mut pet = ScalarField2D::<f32>::new(w, h);
            pet.data.fill(0.4);
            world.derived.et = Some(et);
            world.derived.pet = Some(pet);
            let mut accum = ScalarField2D::<f32>::new(w, h);
            for iy in 0..h {
                for ix in 0..w {
                    accum.set(ix, iy, (ix * iy) as f32);
                }
            }
            world.derived.accumulation = Some(accum);
            world
        };
        let mut a = build();
        let mut b = build();
        SoilMoistureStage.run(&mut a).expect("a");
        SoilMoistureStage.run(&mut b).expect("b");
        assert_eq!(
            &a.baked.soil_moisture.as_ref().unwrap().data,
            &b.baked.soil_moisture.as_ref().unwrap().data
        );
    }

    #[test]
    fn output_is_bounded_0_1() {
        // Stress test: fill every indicator to its maximum. theta_raw =
        // 0.5 + 0.3 + 0.2 = 1.0 by construction, smoothing is a convex
        // combination, so theta is still in [0, 1].
        let (w, h) = (8, 8);
        let mut world = land_world(w, h);
        let mut et = ScalarField2D::<f32>::new(w, h);
        et.data.fill(0.5);
        let mut pet = ScalarField2D::<f32>::new(w, h);
        pet.data.fill(0.5);
        world.derived.et = Some(et);
        world.derived.pet = Some(pet);
        let mut accum = ScalarField2D::<f32>::new(w, h);
        accum.data.fill(100.0);
        world.derived.accumulation = Some(accum);
        let mut river = MaskField2D::new(w, h);
        river.data.fill(1);
        world.derived.river_mask = Some(river);

        SoilMoistureStage.run(&mut world).expect("stage");
        let theta = world.baked.soil_moisture.as_ref().unwrap();
        let s = theta.stats().unwrap();
        assert!(s.min >= 0.0);
        assert!(s.max <= 1.0 + 1e-5);
        assert!(
            (s.max - 1.0).abs() < 1e-4,
            "saturated theta should be ~1, got {}",
            s.max
        );
    }

    #[test]
    fn errors_when_prerequisite_missing() {
        let mut world = WorldState::new(Seed(0), preset(), Resolution::new(4, 4));
        assert!(SoilMoistureStage.run(&mut world).is_err());
    }
}
