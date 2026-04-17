//! `TemperatureStage` (DD1) — lapse-rate temperature with coastal modifier.
//!
//! For every land cell:
//!
//! ```text
//! T(x, y) = T_SEA_LEVEL
//!         - LAPSE_RATE_C_PER_KM * (z_norm * peak_m / 1000)
//!         + COASTAL_MODIFIER_C * exp(-dist_to_coast / COASTAL_DECAY)
//! ```
//!
//! Sea cells are written as `T_SEA_LEVEL` (not `0.0`) so downstream
//! biome / PET stages don't have to special-case them.
//!
//! `dist_to_coast` is approximated by a cheap BFS from the coast set
//! (Von4 neighbourhood, capped at one full sweep so the cost stays
//! `O(sim_cells)` for any island shape).
//!
//! # Units
//!
//! The `z` field is normalised to `[0, 1]`; we convert to metres via
//! `peak_m = MAX_RELIEF_REF_M * preset.max_relief`. This is the only v1
//! stage that handles a dimensional length.

use anyhow::anyhow;
use island_core::field::ScalarField2D;
use island_core::pipeline::SimulationStage;
use island_core::preset::MAX_RELIEF_REF_M;
use island_core::world::WorldState;

use crate::climate::common::compute_distance_to_mask;

// ── physical + empirical constants (hardcoded v1; Sprint 3 promotes to config)

/// Sea-level mean annual temperature in °C for a "tropical volcanic
/// island" archetype. Matches the Réunion / Hawaii order of magnitude.
pub(crate) const T_SEA_LEVEL_C: f32 = 26.0;

/// Environmental lapse rate in °C per km of elevation gain. `6.5` is
/// the International Standard Atmosphere value and Bruijnzeel 2005
/// reports near-identical numbers for tropical montane conditions.
pub(crate) const LAPSE_RATE_C_PER_KM: f32 = 6.5;

/// Coastal warming bonus in °C applied to cells near the shoreline.
/// Proxy for marine heat capacity buffering the diurnal/seasonal range.
pub(crate) const COASTAL_MODIFIER_C: f32 = 2.0;

/// Decay length (in normalized cell-distance units) over which the
/// coastal modifier falls off. `0.05` ≈ 5 % of domain half-width.
pub(crate) const COASTAL_DECAY: f32 = 0.05;

// ── stage ────────────────────────────────────────────────────────────────────

/// DD1: populate `world.baked.temperature`.
pub struct TemperatureStage;

impl SimulationStage for TemperatureStage {
    fn name(&self) -> &'static str {
        "temperature"
    }

    fn run(&self, world: &mut WorldState) -> anyhow::Result<()> {
        let z = world
            .derived
            .z_filled
            .as_ref()
            .ok_or_else(|| anyhow!("TemperatureStage: z_filled is None"))?;
        let coast = world
            .derived
            .coast_mask
            .as_ref()
            .ok_or_else(|| anyhow!("TemperatureStage: coast_mask is None"))?;

        let w = z.width;
        let h = z.height;

        // One-cell-equals-one-distance BFS is close enough for the
        // coastal modifier's order-of-magnitude role. Normalise by the
        // smaller dimension so square / rectangular domains share the
        // same falloff scale.
        let dist_field = compute_distance_to_mask(&coast.is_coast, w, h);
        let norm = w.min(h) as f32;
        let peak_m = MAX_RELIEF_REF_M * world.preset.max_relief;

        let mut temperature = ScalarField2D::<f32>::new(w, h);
        for iy in 0..h {
            for ix in 0..w {
                if coast.is_sea.get(ix, iy) == 1 {
                    temperature.set(ix, iy, T_SEA_LEVEL_C);
                    continue;
                }

                let z_norm = z.get(ix, iy);
                let elev_m = z_norm * peak_m;
                let lapse = LAPSE_RATE_C_PER_KM * elev_m / 1000.0;

                let d = dist_field.get(ix, iy) / norm;
                let coastal = COASTAL_MODIFIER_C * (-d / COASTAL_DECAY).exp();

                temperature.set(ix, iy, T_SEA_LEVEL_C - lapse + coastal);
            }
        }

        world.baked.temperature = Some(temperature);
        Ok(())
    }
}

// ── tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use island_core::field::{MaskField2D, ScalarField2D};
    use island_core::preset::{IslandAge, IslandArchetypePreset};
    use island_core::seed::Seed;
    use island_core::world::{CoastMask, Resolution, WorldState};

    fn preset(max_relief: f32) -> IslandArchetypePreset {
        IslandArchetypePreset {
            name: "temp_test".into(),
            island_radius: 0.5,
            max_relief,
            volcanic_center_count: 1,
            island_age: IslandAge::Young,
            prevailing_wind_dir: 0.0,
            marine_moisture_strength: 0.5,
            sea_level: 0.0,
        }
    }

    /// Build a minimal `WorldState` with one coastal column on the left.
    /// All other land cells sit at the supplied uniform height.
    fn world_with_uniform_z(w: u32, h: u32, z_value: f32, max_relief: f32) -> WorldState {
        let mut world = WorldState::new(Seed(0), preset(max_relief), Resolution::new(w, h));

        let mut z = ScalarField2D::<f32>::new(w, h);
        z.data.fill(z_value);
        world.derived.z_filled = Some(z);

        let mut is_land = MaskField2D::new(w, h);
        is_land.data.fill(1);
        let mut is_coast = MaskField2D::new(w, h);
        for iy in 0..h {
            is_coast.set(0, iy, 1);
        }
        world.derived.coast_mask = Some(CoastMask {
            is_land,
            is_sea: MaskField2D::new(w, h),
            is_coast,
            land_cell_count: (w * h),
            river_mouth_mask: None,
        });
        world
    }

    // 1. Uniform z with zero max_relief → lapse term is 0; temperature
    //    equals sea-level + coastal modifier. The leftmost column is
    //    exactly on the coast (dist 0) so it gets the full modifier.
    #[test]
    fn zero_relief_gives_sea_level_plus_coastal() {
        let (w, h) = (8_u32, 8_u32);
        let mut world = world_with_uniform_z(w, h, 0.5, 0.0); // max_relief = 0
        TemperatureStage.run(&mut world).expect("stage failed");

        let t = world.baked.temperature.as_ref().unwrap();

        // Leftmost column: d=0, coastal term = COASTAL_MODIFIER_C.
        for iy in 0..h {
            let expected = T_SEA_LEVEL_C + COASTAL_MODIFIER_C;
            assert!(
                (t.get(0, iy) - expected).abs() < 1e-4,
                "left-col ({iy}): expected {expected}, got {}",
                t.get(0, iy)
            );
        }
    }

    // 2. Interior lapse rate: uniform z = 1.0 with max_relief = 1.0
    //    → elev_m = MAX_RELIEF_REF_M, lapse = 6.5 * 2.5 = 16.25 °C. At
    //    (30, 16) in a 32-cell-wide grid the normalized coast distance
    //    is 30/32 = 0.9375, so `coastal = 2 * exp(-18.75) ≈ 1.4e-8` —
    //    effectively zero. The assertion is tight enough to catch a
    //    wrong unit conversion or an off-by-one in the lapse formula.
    #[test]
    fn lapse_rate_math() {
        let (w, h) = (32_u32, 32_u32);
        let mut world = world_with_uniform_z(w, h, 1.0, 1.0);
        TemperatureStage.run(&mut world).expect("stage failed");
        let t = world.baked.temperature.as_ref().unwrap();

        let far = t.get(30, 16);
        let expected_lapse = LAPSE_RATE_C_PER_KM * MAX_RELIEF_REF_M / 1000.0; // 16.25
        let expected = T_SEA_LEVEL_C - expected_lapse; // 9.75
        assert!(
            (far - expected).abs() < 1e-3,
            "far-interior temperature expected {expected}, got {far}"
        );
    }

    // 3. Determinism: two runs on an identical input produce bit-exact
    //    temperature fields.
    #[test]
    fn temperature_determinism() {
        let (w, h) = (16_u32, 16_u32);
        let mut w1 = world_with_uniform_z(w, h, 0.7, 0.8);
        let mut w2 = world_with_uniform_z(w, h, 0.7, 0.8);
        TemperatureStage.run(&mut w1).expect("run1");
        TemperatureStage.run(&mut w2).expect("run2");
        let t1 = &w1.baked.temperature.as_ref().unwrap().data;
        let t2 = &w2.baked.temperature.as_ref().unwrap().data;
        assert_eq!(t1, t2);
    }

    // 4. Missing precondition: returns Err when z_filled is absent.
    #[test]
    fn errors_when_z_filled_missing() {
        let mut world = WorldState::new(Seed(0), preset(1.0), Resolution::new(4, 4));
        let result = TemperatureStage.run(&mut world);
        assert!(result.is_err());
    }

    // 5. Coastal modifier decays with distance: the furthest land cell
    //    from the coast must be at least `COASTAL_MODIFIER_C / 2` colder
    //    than the coast itself (on a uniform-z domain where the lapse
    //    term is identical everywhere).
    #[test]
    fn coastal_modifier_decays() {
        let (w, h) = (32_u32, 32_u32);
        let mut world = world_with_uniform_z(w, h, 0.5, 0.5);
        TemperatureStage.run(&mut world).expect("stage failed");
        let t = world.baked.temperature.as_ref().unwrap();

        let coast_temp = t.get(0, 16);
        let far_temp = t.get(w - 1, 16);
        assert!(
            coast_temp - far_temp > COASTAL_MODIFIER_C * 0.5,
            "coastal bonus should decay: coast={coast_temp}, far={far_temp}"
        );
    }
}
