//! `BiomeWeightsStage` (DD6) — per-cell biome suitability, partition-
//! of-unity normalisation, and per-basin edge-aware smoothing.
//!
//! Inputs (all from Sprint 1B stages upstream):
//! - `baked.temperature`   (°C)
//! - `baked.soil_moisture` ([0, 1])
//! - `derived.z_filled`    ([0, 1])
//! - `derived.slope`       (cell units)
//! - `derived.fog_likelihood`
//! - `derived.river_mask`  (for river_proximity)
//! - `derived.coast_mask`  (for land / coast_proximity)
//! - `derived.basin_id`    (for the edge-aware smoothing pass)
//!
//! Output: `baked.biome_weights` — `BiomeWeights` with
//! `sum_i(Bi[x,y]) ≈ 1` on every land cell (exactly 0 on sea).
//!
//! The basin-level smoothing makes good on the Sprint 1A hand-off
//! contract that built `basin_id` specifically so 1B biome stages can
//! avoid smoothing across drainage divides.

use std::collections::BTreeMap;

use anyhow::anyhow;
use island_core::field::ScalarField2D;
use island_core::pipeline::SimulationStage;
use island_core::world::{BiomeType, BiomeWeights, WorldState};

use crate::climate::common::compute_distance_to_mask;
use crate::ecology::suitability::{
    EnvSample, bare_rock_lava, cloud_forest, coastal_scrub, dry_shrub, grassland, lowland_forest,
    montane_wet_forest, riparian_vegetation,
};

/// Decay length for river-proximity falloff. Intentionally broader
/// than `SoilMoistureStage::RIVER_DECAY` (0.02): riparian vegetation
/// influence naturally extends further from the channel than the
/// wetted-bank moisture effect does, so biome scoring should not
/// share the same decay length as the soil-moisture smoothing.
const RIVER_PROXIMITY_DECAY: f32 = 0.03;

/// Decay length for coast-proximity falloff in `[0, 1]` domain units.
const COAST_PROXIMITY_DECAY: f32 = 0.08;

/// Epsilon for the suitability-sum normalization.
const NORMALIZE_EPS: f32 = 1e-6;

/// Weight pulled toward the per-basin mean during the DD6 smoothing
/// pass. `0.3` is the "visibly-smoother-but-not-flat" value from the
/// spec (§304); Task 1B.9 can surface it.
pub(crate) const BASIN_SMOOTH_ALPHA: f32 = 0.3;

/// DD6: populate `world.baked.biome_weights`.
pub struct BiomeWeightsStage;

impl SimulationStage for BiomeWeightsStage {
    fn name(&self) -> &'static str {
        "biome_weights"
    }

    fn run(&self, world: &mut WorldState) -> anyhow::Result<()> {
        let temperature = world
            .baked
            .temperature
            .as_ref()
            .ok_or_else(|| anyhow!("BiomeWeightsStage: baked.temperature is None"))?;
        let soil_moisture = world
            .baked
            .soil_moisture
            .as_ref()
            .ok_or_else(|| anyhow!("BiomeWeightsStage: baked.soil_moisture is None"))?;
        let z = world
            .derived
            .z_filled
            .as_ref()
            .ok_or_else(|| anyhow!("BiomeWeightsStage: derived.z_filled is None"))?;
        let slope = world
            .derived
            .slope
            .as_ref()
            .ok_or_else(|| anyhow!("BiomeWeightsStage: derived.slope is None"))?;
        let fog = world
            .derived
            .fog_likelihood
            .as_ref()
            .ok_or_else(|| anyhow!("BiomeWeightsStage: derived.fog_likelihood is None"))?;
        let river_mask = world
            .derived
            .river_mask
            .as_ref()
            .ok_or_else(|| anyhow!("BiomeWeightsStage: derived.river_mask is None"))?;
        let basin_id = world
            .derived
            .basin_id
            .as_ref()
            .ok_or_else(|| anyhow!("BiomeWeightsStage: derived.basin_id is None"))?;
        let coast = world
            .derived
            .coast_mask
            .as_ref()
            .ok_or_else(|| anyhow!("BiomeWeightsStage: coast_mask is None"))?;

        let w = temperature.width;
        let h = temperature.height;
        let dist_norm = w.min(h) as f32;

        let dist_to_river = compute_distance_to_mask(river_mask, w, h);
        let dist_to_coast = compute_distance_to_mask(&coast.is_coast, w, h);

        let mut weights = BiomeWeights::new(w, h);

        // Per-cell raw → normalized pass.
        for iy in 0..h {
            for ix in 0..w {
                if coast.is_land.get(ix, iy) != 1 {
                    continue;
                }
                let env = EnvSample {
                    temperature_c: temperature.get(ix, iy),
                    soil_moisture: soil_moisture.get(ix, iy),
                    z_norm: z.get(ix, iy),
                    slope: slope.get(ix, iy),
                    fog_likelihood: fog.get(ix, iy),
                    river_proximity: (-dist_to_river.get(ix, iy)
                        / dist_norm
                        / RIVER_PROXIMITY_DECAY)
                        .exp(),
                    coast_proximity: (-dist_to_coast.get(ix, iy)
                        / dist_norm
                        / COAST_PROXIMITY_DECAY)
                        .exp(),
                };
                let raw = raw_suitabilities(env);
                let sum: f32 = raw.iter().sum();
                let denom = sum.max(NORMALIZE_EPS);
                let idx = weights.index(ix, iy);
                for (i, value) in raw.iter().enumerate() {
                    weights.weights[i][idx] = value / denom;
                }
            }
        }

        // Per-basin mean + α-blended smoothing (DD6 §283-302).
        apply_basin_smoothing(&mut weights, basin_id, coast);

        // Derived sidecar: per-cell argmax as u32 so the overlay path
        // can render it through the same ScalarDerived resolver that
        // basin_id uses.
        let mut dominant = ScalarField2D::<u32>::new(w, h);
        for iy in 0..h {
            for ix in 0..w {
                if coast.is_land.get(ix, iy) != 1 {
                    continue;
                }
                let biome = weights.dominant_biome_at(ix, iy);
                dominant.set(ix, iy, biome as u32);
            }
        }

        world.baked.biome_weights = Some(weights);
        world.derived.dominant_biome_per_cell = Some(dominant);
        Ok(())
    }
}

/// Evaluate all 8 suitability functions at a given `EnvSample` in the
/// canonical `BiomeType::ALL` order.
fn raw_suitabilities(env: EnvSample) -> [f32; BiomeType::COUNT] {
    [
        coastal_scrub(env),
        lowland_forest(env),
        montane_wet_forest(env),
        cloud_forest(env),
        dry_shrub(env),
        grassland(env),
        bare_rock_lava(env),
        riparian_vegetation(env),
    ]
}

/// Apply the DD6 per-basin blend. Two-pass: first compute each basin's
/// mean suitability across its land cells; then blend every land cell
/// toward its own basin's mean with weight `BASIN_SMOOTH_ALPHA` and
/// renormalise to sum-1. Writes in-place into `weights`.
///
/// `BTreeMap` is load-bearing for determinism: `HashMap` would be
/// safe in practice (we never iterate during writes, only point-
/// lookup by basin id) but a future refactor could easily break
/// that assumption. Using `BTreeMap` makes the determinism a
/// structural guarantee.
fn apply_basin_smoothing(
    weights: &mut BiomeWeights,
    basin_id: &ScalarField2D<u32>,
    coast: &island_core::world::CoastMask,
) {
    let w = weights.width;
    let h = weights.height;

    // Accumulators: basin → (sum_per_biome, cell_count).
    let mut sums: BTreeMap<u32, ([f64; BiomeType::COUNT], u32)> = BTreeMap::new();
    for iy in 0..h {
        for ix in 0..w {
            if coast.is_land.get(ix, iy) != 1 {
                continue;
            }
            let id = basin_id.get(ix, iy);
            let idx = weights.index(ix, iy);
            let entry = sums.entry(id).or_insert(([0.0; BiomeType::COUNT], 0));
            for (i, row) in weights.weights.iter().enumerate() {
                entry.0[i] += row[idx] as f64;
            }
            entry.1 += 1;
        }
    }

    let means: BTreeMap<u32, [f32; BiomeType::COUNT]> = sums
        .into_iter()
        .map(|(id, (acc, count))| {
            let inv = if count > 0 { 1.0 / count as f64 } else { 0.0 };
            let mut m = [0.0_f32; BiomeType::COUNT];
            for i in 0..BiomeType::COUNT {
                m[i] = (acc[i] * inv) as f32;
            }
            (id, m)
        })
        .collect();

    let alpha = BASIN_SMOOTH_ALPHA;
    let self_w = 1.0 - alpha;
    for iy in 0..h {
        for ix in 0..w {
            if coast.is_land.get(ix, iy) != 1 {
                continue;
            }
            let id = basin_id.get(ix, iy);
            let Some(mean) = means.get(&id) else { continue };
            let idx = weights.index(ix, iy);

            let mut sum = 0.0_f32;
            for (i, row) in weights.weights.iter_mut().enumerate() {
                let blended = self_w * row[idx] + alpha * mean[i];
                row[idx] = blended;
                sum += blended;
            }

            // Renormalise to preserve the sum-1 invariant the downstream
            // overlays / metrics depend on.
            let denom = sum.max(NORMALIZE_EPS);
            for row in weights.weights.iter_mut() {
                row[idx] /= denom;
            }
        }
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
            name: "biome_test".into(),
            island_radius: 0.5,
            max_relief: 1.0,
            volcanic_center_count: 1,
            island_age: IslandAge::Young,
            prevailing_wind_dir: 0.0,
            marine_moisture_strength: 1.0,
            sea_level: 0.0,
        }
    }

    /// Build a wholly-synthetic world with all the prerequisite fields
    /// pre-populated. Cells are parametrised by functional position via
    /// the supplied closure, keeping the test body focused on biome
    /// outcomes.
    fn synthetic_world<F>(w: u32, h: u32, make: F) -> WorldState
    where
        F: Fn(u32, u32) -> (f32, f32, f32, f32, f32),
    {
        let mut world = WorldState::new(Seed(0), preset(), Resolution::new(w, h));

        let mut z = ScalarField2D::<f32>::new(w, h);
        let mut slope = ScalarField2D::<f32>::new(w, h);
        let mut temperature = ScalarField2D::<f32>::new(w, h);
        let mut soil_moisture = ScalarField2D::<f32>::new(w, h);
        let mut fog = ScalarField2D::<f32>::new(w, h);
        for iy in 0..h {
            for ix in 0..w {
                let (zv, sv, tv, mv, fv) = make(ix, iy);
                z.set(ix, iy, zv);
                slope.set(ix, iy, sv);
                temperature.set(ix, iy, tv);
                soil_moisture.set(ix, iy, mv);
                fog.set(ix, iy, fv);
            }
        }
        world.derived.z_filled = Some(z);
        world.derived.slope = Some(slope);
        world.baked.temperature = Some(temperature);
        world.baked.soil_moisture = Some(soil_moisture);
        world.derived.fog_likelihood = Some(fog);
        world.derived.river_mask = Some(MaskField2D::new(w, h));

        // One basin for the whole domain (id = 1).
        let mut basin = ScalarField2D::<u32>::new(w, h);
        basin.data.fill(1);
        world.derived.basin_id = Some(basin);

        let mut is_land = MaskField2D::new(w, h);
        is_land.data.fill(1);
        let mut is_coast = MaskField2D::new(w, h);
        for iy in 0..h {
            is_coast.set(0, iy, 1);
            is_coast.set(w - 1, iy, 1);
        }
        for ix in 0..w {
            is_coast.set(ix, 0, 1);
            is_coast.set(ix, h - 1, 1);
        }
        world.derived.coast_mask = Some(CoastMask {
            is_land,
            is_sea: MaskField2D::new(w, h),
            is_coast,
            land_cell_count: w * h,
            river_mouth_mask: None,
        });
        world
    }

    #[test]
    fn weights_sum_to_one_on_land() {
        let mut world = synthetic_world(8, 8, |_, _| (0.3, 0.1, 22.0, 0.6, 0.2));
        BiomeWeightsStage.run(&mut world).expect("stage");
        let bw = world.baked.biome_weights.as_ref().unwrap();
        for iy in 0..bw.height {
            for ix in 0..bw.width {
                let idx = bw.index(ix, iy);
                let sum: f32 = bw.weights.iter().map(|row| row[idx]).sum();
                // Spec §285 asks for sum ≈ 1 within 1e-4; tolerance is
                // tighter than that because the final renormalise in
                // `apply_basin_smoothing` restores the partition every
                // call, so a regression here would fire immediately.
                assert!((sum - 1.0).abs() < 1e-5, "sum at ({ix},{iy}) = {sum}");
            }
        }
    }

    #[test]
    fn warm_wet_lowland_prefers_lowland_forest() {
        let mut world = synthetic_world(4, 4, |_, _| (0.1, 0.05, 24.0, 0.65, 0.0));
        BiomeWeightsStage.run(&mut world).expect("stage");
        let bw = world.baked.biome_weights.as_ref().unwrap();
        let dominant = bw.dominant_biome_at(2, 2);
        assert_eq!(dominant, BiomeType::LowlandForest);
    }

    #[test]
    fn high_dry_peak_prefers_bare_rock() {
        let mut world = synthetic_world(4, 4, |_, _| (0.9, 0.2, 5.0, 0.1, 0.0));
        BiomeWeightsStage.run(&mut world).expect("stage");
        let bw = world.baked.biome_weights.as_ref().unwrap();
        let dominant = bw.dominant_biome_at(2, 2);
        assert_eq!(dominant, BiomeType::BareRockLava);
    }

    #[test]
    fn foggy_cool_midelevation_prefers_cloud_forest() {
        let mut world = synthetic_world(4, 4, |_, _| (0.55, 0.1, 15.0, 0.85, 0.9));
        BiomeWeightsStage.run(&mut world).expect("stage");
        let bw = world.baked.biome_weights.as_ref().unwrap();
        let dominant = bw.dominant_biome_at(2, 2);
        assert_eq!(dominant, BiomeType::CloudForest);
    }

    #[test]
    fn biome_determinism() {
        let make = || {
            synthetic_world(6, 6, |ix, iy| {
                (ix as f32 * 0.1, 0.1, 20.0 - iy as f32 * 0.5, 0.5, 0.0)
            })
        };
        let mut a = make();
        let mut b = make();
        BiomeWeightsStage.run(&mut a).expect("a");
        BiomeWeightsStage.run(&mut b).expect("b");
        let aw = a.baked.biome_weights.as_ref().unwrap();
        let bw = b.baked.biome_weights.as_ref().unwrap();
        for i in 0..BiomeType::COUNT {
            assert_eq!(
                aw.weights[i], bw.weights[i],
                "biome row {i} not deterministic"
            );
        }
    }

    #[test]
    fn errors_when_prerequisite_missing() {
        let mut world = WorldState::new(Seed(0), preset(), Resolution::new(4, 4));
        assert!(BiomeWeightsStage.run(&mut world).is_err());
    }

    #[test]
    fn three_biomes_appear_across_a_varied_domain() {
        // Vary z from 0 to 0.95 across the domain so multiple biome
        // preferences activate. Ideally catches a regression where the
        // normalization collapses everything onto one biome.
        let mut world = synthetic_world(16, 1, |ix, _| {
            let z = ix as f32 / 15.0 * 0.95;
            let t = 26.0 - z * 16.0;
            let theta = 0.4 + z * 0.4;
            (z, z * 0.5, t, theta, 0.5)
        });
        BiomeWeightsStage.run(&mut world).expect("stage");
        let bw = world.baked.biome_weights.as_ref().unwrap();
        let mut seen = std::collections::HashSet::new();
        for ix in 0..bw.width {
            seen.insert(bw.dominant_biome_at(ix, 0));
        }
        assert!(
            seen.len() >= 3,
            "expected at least 3 distinct dominant biomes, got {:?}",
            seen
        );
    }
}
