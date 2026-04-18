//! `PetStage` (DD3) + `WaterBalanceStage` (DD4) — Hamon PET and Budyko
//! Fu-equation water balance.
//!
//! ## DD3 — Hamon PET
//!
//! ```text
//! PET(x, y) = PET_COEFFICIENT * max(0, T(x, y) - PET_T_BASE_C)
//! ```
//!
//! `T` is the temperature field in °C, `PET_T_BASE_C = 0` so only
//! positive temperatures contribute, and `PET_COEFFICIENT = 0.04`
//! keeps the proxy on roughly the same order of magnitude as the
//! `[0, 1]` normalized precipitation field. v1 lives in sim constants;
//! Sprint 3 promotes to a preset-level hydroclimate block.
//!
//! ## DD4 — Budyko Fu-equation ET/R split
//!
//! ```text
//! ET / P = 1 + (PET / P) - (1 + (PET / P)^ω)^(1/ω)
//! R     = P - ET
//! ```
//!
//! `ω = 2.2` (mid-range for tropical forest islands per Chen 2023),
//! `PET / P` is clipped to `[0.01, 10.0]` before evaluation to avoid
//! numerical blow-ups at the two dry / wet asymptotes. With this
//! clamp and `ω ≥ 1`, the Fu curve is monotone in `PET/P` and always
//! satisfies `ET ≤ P`, so `R = P - ET ≥ 0` — enforced by clamping
//! the final `R` output to `[0, ∞)` as a belt-and-suspenders guard
//! against f32 rounding.
//!
//! Sea cells get `PET = ET = R = 0` — there is no land water balance
//! to run over them.

use anyhow::anyhow;
use island_core::field::ScalarField2D;
use island_core::pipeline::SimulationStage;
use island_core::world::WorldState;

/// PET proxy coefficient. Converts `max(0, T)` (°C) into a normalized
/// `[0, ~1.2]` value that roughly aligns with the `[0, 1]` precipitation
/// proxy.
pub(crate) const PET_COEFFICIENT: f32 = 0.04;

/// Only cells warmer than this contribute to PET.
pub(crate) const PET_T_BASE_C: f32 = 0.0;

/// Budyko Fu-equation parameter. Mid-range for tropical forested
/// islands; Sprint 3 promotes to `preset.hydroclimate_omega`.
pub(crate) const BUDYKO_OMEGA: f32 = 2.2;

/// Lower clamp on `PET / P` before Fu evaluation (guards the wet
/// asymptote).
pub(crate) const PET_OVER_P_MIN: f32 = 0.01;

/// Upper clamp on `PET / P` (guards the dry asymptote).
pub(crate) const PET_OVER_P_MAX: f32 = 10.0;

// ── PetStage ─────────────────────────────────────────────────────────────────

/// DD3: populate `world.derived.pet` from `world.baked.temperature`.
pub struct PetStage;

impl SimulationStage for PetStage {
    fn name(&self) -> &'static str {
        "pet"
    }

    fn run(&self, world: &mut WorldState) -> anyhow::Result<()> {
        let temperature = world
            .baked
            .temperature
            .as_ref()
            .ok_or_else(|| anyhow!("PetStage: baked.temperature is None"))?;
        let coast = world
            .derived
            .coast_mask
            .as_ref()
            .ok_or_else(|| anyhow!("PetStage: coast_mask is None"))?;

        let w = temperature.width;
        let h = temperature.height;
        let mut pet = ScalarField2D::<f32>::new(w, h);

        for iy in 0..h {
            for ix in 0..w {
                if coast.is_sea.get(ix, iy) == 1 {
                    continue;
                }
                let t_c = temperature.get(ix, iy);
                let value = PET_COEFFICIENT * (t_c - PET_T_BASE_C).max(0.0);
                pet.set(ix, iy, value);
            }
        }

        world.derived.pet = Some(pet);
        Ok(())
    }
}

// ── WaterBalanceStage ────────────────────────────────────────────────────────

/// DD4: populate `world.derived.{et, runoff}` from `baked.precipitation`
/// and `derived.pet`.
pub struct WaterBalanceStage;

impl SimulationStage for WaterBalanceStage {
    fn name(&self) -> &'static str {
        "water_balance"
    }

    fn run(&self, world: &mut WorldState) -> anyhow::Result<()> {
        let precip = world
            .baked
            .precipitation
            .as_ref()
            .ok_or_else(|| anyhow!("WaterBalanceStage: baked.precipitation is None"))?;
        let pet = world
            .derived
            .pet
            .as_ref()
            .ok_or_else(|| anyhow!("WaterBalanceStage: derived.pet is None"))?;
        let coast = world
            .derived
            .coast_mask
            .as_ref()
            .ok_or_else(|| anyhow!("WaterBalanceStage: coast_mask is None"))?;

        let w = precip.width;
        let h = precip.height;
        let mut et = ScalarField2D::<f32>::new(w, h);
        let mut runoff = ScalarField2D::<f32>::new(w, h);

        for iy in 0..h {
            for ix in 0..w {
                if coast.is_sea.get(ix, iy) == 1 {
                    continue;
                }

                let p = precip.get(ix, iy);
                if p <= 0.0 {
                    // No precipitation → no ET / R on land either.
                    continue;
                }
                let pet_val = pet.get(ix, iy);
                let et_over_p = fu_et_over_p(pet_val / p);
                let et_value = (et_over_p * p).clamp(0.0, p);
                let runoff_value = (p - et_value).max(0.0);

                et.set(ix, iy, et_value);
                runoff.set(ix, iy, runoff_value);
            }
        }

        world.derived.et = Some(et);
        world.derived.runoff = Some(runoff);
        Ok(())
    }
}

/// Fu-equation `ET / P` from a dryness index, clipped for numerical
/// safety. Pure function so tests can hit it without building a world.
fn fu_et_over_p(pet_over_p: f32) -> f32 {
    let phi = pet_over_p.clamp(PET_OVER_P_MIN, PET_OVER_P_MAX);
    1.0 + phi - (1.0 + phi.powf(BUDYKO_OMEGA)).powf(1.0 / BUDYKO_OMEGA)
}

// ── tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use island_core::field::{MaskField2D, ScalarField2D};
    use island_core::preset::{IslandAge, IslandArchetypePreset};
    use island_core::seed::Seed;
    use island_core::world::{CoastMask, Resolution, WorldState};

    fn preset() -> IslandArchetypePreset {
        IslandArchetypePreset {
            name: "wb_test".into(),
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
        world
    }

    fn set_uniform_temp(world: &mut WorldState, t_c: f32) {
        let w = world.resolution.sim_width;
        let h = world.resolution.sim_height;
        let mut t = ScalarField2D::<f32>::new(w, h);
        t.data.fill(t_c);
        world.baked.temperature = Some(t);
    }

    fn set_uniform_precip(world: &mut WorldState, p: f32) {
        let w = world.resolution.sim_width;
        let h = world.resolution.sim_height;
        let mut precip = ScalarField2D::<f32>::new(w, h);
        precip.data.fill(p);
        world.baked.precipitation = Some(precip);
    }

    // ── DD3 PetStage tests ──────────────────────────────────────────────

    #[test]
    fn pet_is_zero_below_base_temperature() {
        let mut world = land_world(4, 4);
        set_uniform_temp(&mut world, -5.0);
        PetStage.run(&mut world).expect("pet stage");
        let pet = world.derived.pet.as_ref().unwrap();
        assert_eq!(pet.stats().unwrap().max, 0.0);
    }

    #[test]
    fn pet_is_linear_in_positive_temperature() {
        let mut world = land_world(4, 4);
        set_uniform_temp(&mut world, 20.0);
        PetStage.run(&mut world).expect("pet stage");
        let pet = world.derived.pet.as_ref().unwrap();
        for v in pet.data.iter() {
            assert!((v - 20.0 * PET_COEFFICIENT).abs() < 1e-6);
        }
    }

    // ── DD4 WaterBalanceStage tests ─────────────────────────────────────

    #[test]
    fn fu_asymptotes() {
        // Very wet (low PET/P) → ET/P → 0 (at the lower clamp, not 0
        // exactly, because we clamp to PET_OVER_P_MIN = 0.01).
        let wet = fu_et_over_p(0.001);
        assert!(wet < 0.02, "wet extreme ET/P = {wet}, expected ~0");

        // Dry extreme (high PET/P) → ET/P → 1 (all precipitation evaporates).
        let dry = fu_et_over_p(100.0);
        assert!(
            (dry - 1.0).abs() < 0.05,
            "dry extreme ET/P = {dry}, expected ~1"
        );

        // ET/P is monotone increasing in PET/P.
        let mut prev = -1.0_f32;
        for i in 1..=100 {
            let phi = i as f32 * 0.1;
            let v = fu_et_over_p(phi);
            assert!(v >= prev, "Fu not monotone at phi={phi}: {prev} → {v}");
            prev = v;
        }
    }

    #[test]
    fn water_balance_enforces_p_minus_et_equals_runoff() {
        let mut world = land_world(8, 8);
        set_uniform_temp(&mut world, 20.0);
        set_uniform_precip(&mut world, 0.8);
        PetStage.run(&mut world).expect("pet");
        WaterBalanceStage.run(&mut world).expect("water balance");

        let p_field = world.baked.precipitation.as_ref().unwrap();
        let et_field = world.derived.et.as_ref().unwrap();
        let r_field = world.derived.runoff.as_ref().unwrap();
        for iy in 0..8 {
            for ix in 0..8 {
                let p = p_field.get(ix, iy);
                let et = et_field.get(ix, iy);
                let r = r_field.get(ix, iy);
                assert!(
                    (p - et - r).abs() < 1e-5,
                    "balance broken at ({ix},{iy}): p={p} et={et} r={r}"
                );
                assert!(r >= 0.0);
                assert!(et <= p + 1e-6);
            }
        }
    }

    #[test]
    fn water_balance_zero_precipitation_yields_zero_et_and_runoff() {
        let mut world = land_world(4, 4);
        set_uniform_temp(&mut world, 30.0);
        set_uniform_precip(&mut world, 0.0);
        PetStage.run(&mut world).expect("pet");
        WaterBalanceStage.run(&mut world).expect("wb");
        assert_eq!(world.derived.et.as_ref().unwrap().stats().unwrap().max, 0.0);
        assert_eq!(
            world.derived.runoff.as_ref().unwrap().stats().unwrap().max,
            0.0
        );
    }

    #[test]
    fn water_balance_determinism() {
        let mut a = land_world(8, 8);
        set_uniform_temp(&mut a, 25.0);
        set_uniform_precip(&mut a, 0.6);
        let mut b = land_world(8, 8);
        set_uniform_temp(&mut b, 25.0);
        set_uniform_precip(&mut b, 0.6);
        PetStage.run(&mut a).expect("pet");
        WaterBalanceStage.run(&mut a).expect("wb");
        PetStage.run(&mut b).expect("pet");
        WaterBalanceStage.run(&mut b).expect("wb");
        assert_eq!(
            &a.derived.et.as_ref().unwrap().data,
            &b.derived.et.as_ref().unwrap().data
        );
        assert_eq!(
            &a.derived.runoff.as_ref().unwrap().data,
            &b.derived.runoff.as_ref().unwrap().data
        );
    }

    #[test]
    fn pet_errors_when_temperature_missing() {
        let mut world = land_world(4, 4);
        assert!(PetStage.run(&mut world).is_err());
    }

    #[test]
    fn water_balance_errors_when_pet_missing() {
        let mut world = land_world(4, 4);
        set_uniform_precip(&mut world, 0.5);
        assert!(WaterBalanceStage.run(&mut world).is_err());
    }
}
