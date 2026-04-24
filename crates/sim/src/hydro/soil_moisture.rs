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
//!
//! ## Sprint 3.5.D DD6: `coastal_margin` SM floor
//!
//! After the LFPM v3 precipitation + fog coupling pass, a spatial-
//! proximity floor is applied to land cells within Von4-distance ≤ 3
//! of the nearest sea cell:
//!
//! ```text
//! for every land cell p where Von4-dist-to-sea(p) ≤ COASTAL_MARGIN_MAX_DIST:
//!     soil_moisture[p] = max(soil_moisture[p], COASTAL_MARGIN_SM_FLOOR)
//! ```
//!
//! Rationale: CoastalScrub's `f_coast * f_dry` gates require the cell
//! to be near the coast; LFPM v3 post-3.1.C delivers `θ = 0.05–0.20`
//! near coasts, which is drier than typical coastal soil. A 0.25 floor
//! on Von4 ≤ 3 land cells lifts θ into the CoastalScrub bell's active
//! range without touching the bell structure or any other climate path.
//! Sea cells are explicitly excluded from the floor application.

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

/// Sprint 3 DD5: fog likelihood → water input conversion factor.
/// `fog_water_input[p] = FOG_WATER_GAIN * fog_likelihood[p]`.
/// Dimensionless proxy for fog-drip contribution relative to precipitation.
pub const FOG_WATER_GAIN: f32 = 0.30;

/// Sprint 3 DD5: fraction of `fog_water_input` that enters the soil-moisture
/// store. Less than 1.0 because fog drip has significant surface runoff on
/// steep volcanic terrain.
pub const FOG_TO_SM_COUPLING: f32 = 0.60;

/// Sprint 3.5.D DD6: Von4 distance threshold (in cells) from the nearest
/// sea cell below which the coastal-margin SM floor is applied.
/// Value-locked by `coastal_margin_sm_floor_applied` (added in c3).
pub const COASTAL_MARGIN_MAX_DIST: u32 = 3;

/// Sprint 3.5.D DD6: minimum soil-moisture floor imposed on land cells
/// within `COASTAL_MARGIN_MAX_DIST` cells of the sea (Von4 distance).
/// `0.25` is chosen so CoastalScrub's `f_dry = smoothstep(0.10, 0.50,
/// 1.0 - θ)` still evaluates to `smoothstep(0.10, 0.50, 0.75) = 1.0`
/// (the floor is not so high as to suppress the "dryish coast" signal).
/// Value-locked by `coastal_margin_sm_floor_applied` (added in c3).
pub const COASTAL_MARGIN_SM_FLOOR: f32 = 0.25;

/// DD5: populate `world.baked.soil_moisture` and `world.derived.fog_water_input`.
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

        // Sprint 3 DD5: add fog-drip to soil moisture and populate the
        // `fog_water_input` derived cache for the overlay (Task 3.7).
        //
        // `fog_likelihood` may be None if this stage is called in isolation
        // (e.g. a test that only wires up the raw hydro inputs). When it is
        // None we skip the fog coupling pass — soil_moisture is still valid,
        // just missing the fog-drip contribution.
        let fog_likelihood_snap = world.derived.fog_likelihood.clone();
        if let Some(fog) = &fog_likelihood_snap {
            // Allocate or reuse fog_water_input with the same reuse discipline
            // used for deposition_flux / precipitation_sweep_order.
            let needs_alloc = world
                .derived
                .fog_water_input
                .as_ref()
                .map(|f| f.width != w || f.height != h)
                .unwrap_or(true);
            if needs_alloc {
                world.derived.fog_water_input = Some(ScalarField2D::<f32>::new(w, h));
            } else {
                // Zero out sea cells from a previous run.
                world
                    .derived
                    .fog_water_input
                    .as_mut()
                    .unwrap()
                    .data
                    .fill(0.0);
            }

            let fwi = world.derived.fog_water_input.as_mut().unwrap();
            for iy in 0..h {
                for ix in 0..w {
                    if coast.is_land.get(ix, iy) != 1 {
                        continue; // sea cells keep 0.0 in fog_water_input
                    }
                    let fog_water = FOG_WATER_GAIN * fog.get(ix, iy);
                    fwi.set(ix, iy, fog_water);
                    let new_sm =
                        (smoothed.get(ix, iy) + fog_water * FOG_TO_SM_COUPLING).clamp(0.0, 1.0);
                    smoothed.set(ix, iy, new_sm);
                }
            }
        }

        // Sprint 3.5.D DD6: coastal-margin SM floor.
        //
        // Multi-source Von4 BFS from all sea cells. For every land cell
        // whose Von4 distance to the nearest sea cell is ≤
        // COASTAL_MARGIN_MAX_DIST, apply `max(θ, COASTAL_MARGIN_SM_FLOOR)`.
        //
        // The BFS uses a layer-by-layer frontier so it early-terminates
        // once the current layer depth exceeds COASTAL_MARGIN_MAX_DIST,
        // bounding work to O(coast_len × COASTAL_MARGIN_MAX_DIST) rather
        // than O(domain).
        //
        // Sea cells are skipped (the floor only applies to land cells).
        {
            // Seed frontier with all sea cells (dist = 0).
            let mut frontier: Vec<(u32, u32)> = Vec::new();
            for iy in 0..h {
                for ix in 0..w {
                    if coast.is_land.get(ix, iy) != 1 {
                        frontier.push((ix, iy));
                    }
                }
            }

            // Track visited cells to avoid re-queuing.
            let total = (w * h) as usize;
            let mut visited: Vec<bool> = vec![false; total];
            for &(ix, iy) in &frontier {
                visited[(iy * w + ix) as usize] = true;
            }

            // BFS up to depth COASTAL_MARGIN_MAX_DIST, collecting land cells.
            let mut next: Vec<(u32, u32)> = Vec::new();
            let von4: [(i32, i32); 4] = [(-1, 0), (1, 0), (0, -1), (0, 1)];
            for _dist in 1..=COASTAL_MARGIN_MAX_DIST {
                for &(x, y) in &frontier {
                    for (dx, dy) in von4 {
                        let nx = x as i32 + dx;
                        let ny = y as i32 + dy;
                        if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                            continue;
                        }
                        let (nxu, nyu) = (nx as u32, ny as u32);
                        let idx = (nyu * w + nxu) as usize;
                        if visited[idx] {
                            continue;
                        }
                        visited[idx] = true;
                        next.push((nxu, nyu));
                        // Apply floor only to land cells (sea neighbours
                        // are already in the visited set from the seed pass).
                        if coast.is_land.get(nxu, nyu) == 1 {
                            let old = smoothed.get(nxu, nyu);
                            smoothed.set(nxu, nyu, old.max(COASTAL_MARGIN_SM_FLOOR));
                        }
                    }
                }
                frontier.clear();
                std::mem::swap(&mut frontier, &mut next);
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
            climate: Default::default(),
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

    // ── Sprint 3 DD5 tests ────────────────────────────────────────────────────

    /// Minimal preset for full-pipeline invalidation tests.
    /// `n_batch = 0` keeps `ErosionOuterLoop` as a no-op so the small 32×32
    /// grid does not trip the sea-crossing invariant.
    fn pipeline_preset() -> IslandArchetypePreset {
        use island_core::preset::ErosionParams;
        IslandArchetypePreset {
            name: "sm_pipeline_test".into(),
            island_radius: 0.5,
            max_relief: 1.0,
            volcanic_center_count: 1,
            island_age: IslandAge::Young,
            prevailing_wind_dir: 0.0,
            marine_moisture_strength: 1.0,
            sea_level: 0.0,
            erosion: ErosionParams {
                n_batch: 0,
                ..Default::default()
            },
            climate: Default::default(),
        }
    }

    /// `fog_water_input` is populated after `SoilMoistureStage` when
    /// `fog_likelihood` is present. Values must be non-negative and equal
    /// `FOG_WATER_GAIN * fog_likelihood[p]`.
    #[test]
    fn fog_water_input_populated_after_soil_moisture_run() {
        let (w, h) = (4, 4);
        let mut world = land_world(w, h);

        // Fill fog_likelihood with a known pattern.
        let mut fog = ScalarField2D::<f32>::new(w, h);
        for iy in 0..h {
            for ix in 0..w {
                fog.set(ix, iy, 0.1 * ix as f32 + 0.05 * iy as f32);
            }
        }
        world.derived.fog_likelihood = Some(fog.clone());

        SoilMoistureStage.run(&mut world).expect("stage");

        let fwi = world
            .derived
            .fog_water_input
            .as_ref()
            .expect("fog_water_input should be Some");
        for iy in 0..h {
            for ix in 0..w {
                let expected = FOG_WATER_GAIN * fog.get(ix, iy);
                let actual = fwi.get(ix, iy);
                assert!(
                    actual >= 0.0,
                    "fog_water_input must be non-negative at ({ix},{iy}): {actual}"
                );
                assert!(
                    (actual - expected).abs() < 1e-6,
                    "fog_water_input[({ix},{iy})] = {actual}, expected {expected}"
                );
            }
        }
    }

    /// Cells with `fog_likelihood = 1.0` gain more soil moisture than cells
    /// with `fog_likelihood = 0.0`, holding all other inputs equal.
    #[test]
    fn soil_moisture_gains_fog_water_contribution() {
        let (w, h) = (4, 4);

        // Build two identical worlds, differing only in fog_likelihood.
        let build_with_fog = |fog_val: f32| {
            let mut world = land_world(w, h);
            let mut fog = ScalarField2D::<f32>::new(w, h);
            fog.data.fill(fog_val);
            world.derived.fog_likelihood = Some(fog);
            world
        };

        let mut world_fog = build_with_fog(1.0);
        let mut world_no_fog = build_with_fog(0.0);

        SoilMoistureStage.run(&mut world_fog).expect("fog run");
        SoilMoistureStage
            .run(&mut world_no_fog)
            .expect("no-fog run");

        let sm_fog = world_fog.baked.soil_moisture.as_ref().unwrap();
        let sm_no_fog = world_no_fog.baked.soil_moisture.as_ref().unwrap();

        // fog_contribution = FOG_WATER_GAIN * FOG_TO_SM_COUPLING = 0.30 * 0.60 = 0.18.
        let expected_delta = FOG_WATER_GAIN * FOG_TO_SM_COUPLING;
        for iy in 0..h {
            for ix in 0..w {
                let delta = sm_fog.get(ix, iy) - sm_no_fog.get(ix, iy);
                assert!(
                    (delta - expected_delta).abs() < 1e-5,
                    "fog contribution mismatch at ({ix},{iy}): got delta {delta}, expected {expected_delta}"
                );
            }
        }
    }

    /// Sea cells must have `fog_water_input = 0.0` after `SoilMoistureStage`.
    ///
    /// The fog coupling loop skips sea cells via the `is_land != 1` guard,
    /// so the zero-initialised field value persists for every sea cell even
    /// when the neighbouring land cells have high fog likelihood.
    #[test]
    fn fog_water_input_is_zero_on_sea_cells() {
        let (w, h) = (4_u32, 4_u32);

        // Build a world where (2, 2) is a sea cell surrounded by land.
        let mut world = WorldState::new(Seed(0), preset(), Resolution::new(w, h));
        let mut is_land = MaskField2D::new(w, h);
        is_land.data.fill(1);
        is_land.set(2, 2, 0);
        let mut is_sea = MaskField2D::new(w, h);
        is_sea.set(2, 2, 1);
        world.derived.coast_mask = Some(CoastMask {
            is_land,
            is_sea,
            is_coast: MaskField2D::new(w, h),
            land_cell_count: w * h - 1,
            river_mouth_mask: None,
        });
        world.derived.et = Some(ScalarField2D::<f32>::new(w, h));
        world.derived.pet = Some(ScalarField2D::<f32>::new(w, h));
        world.derived.accumulation = Some(ScalarField2D::<f32>::new(w, h));
        world.derived.river_mask = Some(MaskField2D::new(w, h));
        let mut flow_dir = ScalarField2D::<u8>::new(w, h);
        flow_dir.data.fill(FLOW_DIR_SINK);
        world.derived.flow_dir = Some(flow_dir);

        // High fog likelihood everywhere, including the sea cell.
        let mut fog = ScalarField2D::<f32>::new(w, h);
        fog.data.fill(1.0);
        world.derived.fog_likelihood = Some(fog);

        SoilMoistureStage.run(&mut world).expect("stage");

        let fwi = world
            .derived
            .fog_water_input
            .as_ref()
            .expect("fog_water_input should be Some");
        assert_eq!(
            fwi.get(2, 2),
            0.0,
            "sea cell (2,2) must have fog_water_input = 0.0, got {}",
            fwi.get(2, 2)
        );
        // Sanity: adjacent land cell has non-zero fog_water_input.
        assert!(
            fwi.get(1, 2) > 0.0,
            "land cell (1,2) should have fog_water_input > 0.0"
        );
    }

    /// `invalidate_from(FogLikelihood)` cascades through the SoilMoisture
    /// arm and must clear `derived.fog_water_input`.
    ///
    /// FogLikelihood (12) < SoilMoisture (15) in the StageId ordering, so
    /// `invalidate_from(FogLikelihood)` iterates through every arm from 12
    /// to 17 inclusive, hitting the SoilMoisture arm which sets
    /// `fog_water_input = None`.
    #[test]
    fn invalidate_from_fog_likelihood_cascades_to_fog_water_input() {
        use crate::{StageId, default_pipeline, invalidate_from};
        use island_core::world::Resolution;

        let mut world = WorldState::new(Seed(2), pipeline_preset(), Resolution::new(32, 32));
        default_pipeline().run(&mut world).expect("full pipeline");

        assert!(
            world.derived.fog_water_input.is_some(),
            "fog_water_input must be Some after full pipeline"
        );

        // Invalidating at FogLikelihood (12) cascades through SoilMoisture (15).
        invalidate_from(&mut world, StageId::FogLikelihood);

        assert!(
            world.derived.fog_likelihood.is_none(),
            "fog_likelihood must be None after invalidate_from(FogLikelihood)"
        );
        assert!(
            world.derived.fog_water_input.is_none(),
            "fog_water_input must be None after invalidate_from(FogLikelihood) cascade"
        );
    }

    /// After `invalidate_from(SoilMoisture)`, `derived.fog_water_input` must
    /// be `None`.
    #[test]
    fn invalidate_from_soil_moisture_clears_fog_water_input() {
        use crate::{StageId, default_pipeline, invalidate_from};
        use island_core::world::Resolution;

        let mut world = WorldState::new(Seed(1), pipeline_preset(), Resolution::new(32, 32));
        default_pipeline().run(&mut world).expect("full pipeline");

        assert!(
            world.derived.fog_water_input.is_some(),
            "fog_water_input must be Some after full pipeline"
        );

        invalidate_from(&mut world, StageId::SoilMoisture);

        assert!(
            world.derived.fog_water_input.is_none(),
            "fog_water_input must be None after invalidate_from(SoilMoisture)"
        );
    }
}
