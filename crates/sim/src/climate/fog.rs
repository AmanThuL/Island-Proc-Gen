//! `FogLikelihoodStage` (DD7 v2, Sprint 3 DD5) — trade-wind inversion-layer
//! model.
//!
//! For each land cell:
//!
//! ```text
//! inversion_z      = 0.65 · preset.max_relief   (proxy for cloud-base altitude)
//! band_thickness   = 0.15 · preset.max_relief
//! elev_band(p)     = exp(-((z[p] - inversion_z) / band_thickness)^2)
//! uplift_factor(p) = smoothstep(0, UPLIFT_SATURATION, max(0, signed_uplift))
//! fog_likelihood[p]= elev_band(p) · (0.5 + 0.5 · uplift_factor(p))
//! ```
//!
//! The formula weights elevation-band presence 50 % and orographic uplift 50 %
//! (Sprint 1B had 0 % / 100 % — the band was a multiplicative gate, not an
//! additive contribution). Sea cells are forced to `0.0`. The inversion height
//! and band thickness are derived from `preset.max_relief` so the fog layer
//! self-scales with island relief rather than sitting at a fixed z_norm.
//!
//! The sign convention for `uplift_factor` comes from
//! `climate::common::signed_uplift` (shared with `PrecipitationStage`).
//! Consumed by `BiomeWeightsStage::CloudForest` in DD6 (tuned in Sprint 3 DD5)
//! and by `SoilMoistureStage` via `fog_water_input` (Sprint 3 DD5 Change 3).

use anyhow::anyhow;
use island_core::field::ScalarField2D;
use island_core::pipeline::SimulationStage;
use island_core::world::WorldState;

use crate::climate::common::{grad_scalar_at, signed_uplift, smoothstep, wind_unit};

/// Fraction of `max_relief` that places the inversion-layer centre.
/// `inversion_z = INVERSION_Z_FRACTION · preset.max_relief`.
pub(crate) const INVERSION_Z_FRACTION: f32 = 0.65;

/// Fraction of `max_relief` that sets the Gaussian half-width of the fog band.
/// `band_thickness = BAND_THICKNESS_FRACTION · preset.max_relief`.
pub(crate) const BAND_THICKNESS_FRACTION: f32 = 0.15;

/// Upper edge of the uplift saturation window. Any positive uplift at
/// or above this value maps to `uplift_factor = 1`.
pub(crate) const UPLIFT_SATURATION: f32 = 0.3;

/// Minimum fog likelihood contribution from the elevation band alone
/// (when `uplift_factor = 0`). Equals `0.5` because the formula is
/// `elev_band · (0.5 + 0.5 · uplift_factor)`.
pub(crate) const FOG_BAND_WEIGHT: f32 = 0.5;

/// DD7 v2: populate `world.derived.fog_likelihood` using the Sprint 3 DD5
/// trade-wind inversion-layer model.
pub struct FogLikelihoodStage;

impl SimulationStage for FogLikelihoodStage {
    fn name(&self) -> &'static str {
        "fog_likelihood"
    }

    fn run(&self, world: &mut WorldState) -> anyhow::Result<()> {
        let z = world
            .derived
            .z_filled
            .as_ref()
            .ok_or_else(|| anyhow!("FogLikelihoodStage: z_filled is None"))?;
        let coast = world
            .derived
            .coast_mask
            .as_ref()
            .ok_or_else(|| anyhow!("FogLikelihoodStage: coast_mask is None"))?;

        let w = z.width;
        let h = z.height;
        let wind = wind_unit(world.preset.prevailing_wind_dir);

        // Inversion-layer parameters derived from relief.
        let max_relief = world.preset.max_relief;
        let inversion_z = INVERSION_Z_FRACTION * max_relief;
        let band_thickness = BAND_THICKNESS_FRACTION * max_relief;
        // Guard: if max_relief is zero (degenerate preset), all fog stays 0.
        let band_thickness_safe = band_thickness.max(f32::EPSILON);

        let mut fog = ScalarField2D::<f32>::new(w, h);
        for iy in 0..h {
            for ix in 0..w {
                if coast.is_sea.get(ix, iy) == 1 {
                    continue; // sea cells stay at 0.0
                }

                let z_norm = z.get(ix, iy);

                // Gaussian bell centred on the inversion layer.
                let dz = (z_norm - inversion_z) / band_thickness_safe;
                let elev_band = (-(dz * dz)).exp();

                // Orographic uplift factor (50 % weight).
                let grad = grad_scalar_at(z, ix, iy);
                let uplift = signed_uplift(wind, grad).max(0.0);
                let uplift_factor = smoothstep(0.0, UPLIFT_SATURATION, uplift);

                fog.set(
                    ix,
                    iy,
                    elev_band * (FOG_BAND_WEIGHT + FOG_BAND_WEIGHT * uplift_factor),
                );
            }
        }

        world.derived.fog_likelihood = Some(fog);
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

    fn preset_with_relief(wind_dir: f32, max_relief: f32) -> IslandArchetypePreset {
        IslandArchetypePreset {
            name: "fog_test".into(),
            island_radius: 0.5,
            max_relief,
            volcanic_center_count: 1,
            island_age: IslandAge::Young,
            prevailing_wind_dir: wind_dir,
            marine_moisture_strength: 1.0,
            sea_level: 0.0,
            erosion: Default::default(),
            climate: Default::default(),
        }
    }

    fn preset(wind_dir: f32) -> IslandArchetypePreset {
        preset_with_relief(wind_dir, 1.0)
    }

    fn all_land_coast(w: u32, h: u32) -> CoastMask {
        let mut is_land = MaskField2D::new(w, h);
        is_land.data.fill(1);
        CoastMask {
            is_land,
            is_sea: MaskField2D::new(w, h),
            is_coast: MaskField2D::new(w, h),
            land_cell_count: w * h,
            river_mouth_mask: None,
        }
    }

    // 1. Sprint 3 DD5: fog likelihood peaks near the inversion layer.
    //    With max_relief = 1.0:
    //      inversion_z = 0.65 * 1.0 = 0.65
    //      band_thickness = 0.15 * 1.0 = 0.15
    //    A flat domain (no uplift) at z = inversion_z should maximise
    //    elev_band = 1.0, giving fog = 1.0 * 0.5 = 0.5.
    //    A cell at z = inversion_z + band_thickness should have elev_band
    //    = exp(-1) ≈ 0.368, fog ≈ 0.5 * 0.368 ≈ 0.184.
    //    A cell far from the inversion layer (z = 0.0) should have
    //    fog ≈ 0 (elev_band ≈ exp(-(0.65/0.15)^2) ≈ exp(-18.8) ≈ 0).
    #[test]
    fn fog_likelihood_peaks_near_inversion_layer() {
        let (w, h) = (16_u32, 16_u32);
        let max_relief = 1.0_f32;
        let inversion_z = INVERSION_Z_FRACTION * max_relief; // 0.65
        let band_thickness = BAND_THICKNESS_FRACTION * max_relief; // 0.15

        let mut world = WorldState::new(Seed(0), preset(0.0), Resolution::new(w, h));

        // Three rows: at inversion, 1 band_thickness above, far below (z=0).
        let mut z = ScalarField2D::<f32>::new(w, h);
        for ix in 0..w {
            z.set(ix, 0, inversion_z); // row 0: at peak
            z.set(ix, 1, inversion_z + band_thickness); // row 1: one sigma above
            z.set(ix, 2, 0.0); // row 2: far below
        }
        world.derived.z_filled = Some(z);
        world.derived.coast_mask = Some(all_land_coast(w, h));

        FogLikelihoodStage.run(&mut world).expect("stage failed");
        let fog = world.derived.fog_likelihood.as_ref().unwrap();

        // At inversion_z (no uplift): elev_band = 1, fog = 0.5.
        let at_peak = fog.get(1, 0); // interior cell, no boundary effects on grad
        assert!(
            (at_peak - 0.5).abs() < 0.01,
            "fog at inversion_z should be ~0.5, got {at_peak}"
        );

        // One sigma above: elev_band = exp(-1) ≈ 0.368, fog ≈ 0.184.
        let one_sigma = fog.get(1, 1);
        assert!(
            (one_sigma - 0.5 * std::f32::consts::E.recip()).abs() < 0.02,
            "fog at inversion_z + band_thickness should be ~0.184, got {one_sigma}"
        );

        // Far below: nearly zero.
        let far_below = fog.get(1, 2);
        assert!(
            far_below < 1e-3,
            "fog far from inversion_z should be near zero, got {far_below}"
        );

        // Peak must be greater than one-sigma, one-sigma must be greater than far-below.
        assert!(
            at_peak > one_sigma,
            "fog must decrease away from inversion layer"
        );
        assert!(
            one_sigma > far_below,
            "one-sigma fog must exceed far-below fog"
        );
    }

    // 2. Uplift doubles the fog: a steep windward slope at inversion_z
    //    has uplift_factor = 1, giving fog = elev_band * (0.5 + 0.5) = elev_band.
    //    A flat cell at the same elevation has fog = elev_band * 0.5.
    #[test]
    fn steep_windward_at_inversion_has_stronger_fog_than_flat() {
        let (w, h) = (8_u32, 8_u32);
        let mut world = WorldState::new(Seed(0), preset(0.0), Resolution::new(w, h));

        let inversion_z = INVERSION_Z_FRACTION; // max_relief = 1.0

        // z gradient at (4,4): neighbours z(3,4)=inversion_z+0.3 and z(5,4)=inversion_z-0.3.
        // grad_z.x = (inversion_z-0.3 - (inversion_z+0.3)) / 2 = -0.3.
        // Wind from east (dir=0): signed_uplift = -wind.dot(grad) = -1*(-0.3) = 0.3.
        // uplift_factor = smoothstep(0, 0.3, 0.3) = 1.
        let mut z = ScalarField2D::<f32>::new(w, h);
        z.data.fill(inversion_z);
        z.set(3, 4, inversion_z + 0.3);
        z.set(5, 4, inversion_z - 0.3);
        world.derived.z_filled = Some(z);
        world.derived.coast_mask = Some(all_land_coast(w, h));

        FogLikelihoodStage.run(&mut world).expect("stage failed");
        let fog = world.derived.fog_likelihood.as_ref().unwrap();

        // Steep cell at inversion: elev_band=1, uplift_factor=1, fog=1*1=1.0.
        let steep = fog.get(4, 4);
        assert!(
            (steep - 1.0).abs() < 0.01,
            "steep windward cell at inversion should have fog ≈ 1.0, got {steep}"
        );

        // Flat cell at inversion: elev_band=1, uplift_factor=0, fog=0.5.
        let flat = fog.get(1, 1);
        assert!(
            (flat - 0.5).abs() < 0.01,
            "flat cell at inversion should have fog ≈ 0.5, got {flat}"
        );

        assert!(steep > flat, "steep windward fog must exceed flat fog");
    }

    // 3. Sea level (z ≈ 0) is far from the inversion layer → fog ≈ 0.
    //    This is the spec's "fog_likelihood_zero_outside_band" check.
    #[test]
    fn fog_likelihood_zero_outside_band() {
        let (w, h) = (16_u32, 16_u32);
        let max_relief = 1.0_f32;
        // inversion_z = 0.65, band_thickness = 0.15.
        // At z = 0: dz = (0 - 0.65)/0.15 = -4.33, exp(-18.8) ≈ 6e-9.
        let mut world = WorldState::new(
            Seed(0),
            preset_with_relief(0.0, max_relief),
            Resolution::new(w, h),
        );
        let mut z = ScalarField2D::<f32>::new(w, h);
        z.data.fill(0.0); // sea level height
        world.derived.z_filled = Some(z);
        world.derived.coast_mask = Some(all_land_coast(w, h));

        FogLikelihoodStage.run(&mut world).expect("stage failed");
        let fog = world.derived.fog_likelihood.as_ref().unwrap();
        for v in fog.data.iter() {
            assert!(
                *v < 1e-4,
                "z=0 cell should have near-zero fog (far from inversion layer), got {v}"
            );
        }
    }

    // 4. Determinism: two runs bit-exact.
    #[test]
    fn fog_determinism() {
        let (w, h) = (16_u32, 16_u32);
        let make_world = || {
            let mut world = WorldState::new(Seed(0), preset(0.0), Resolution::new(w, h));
            let mut z = ScalarField2D::<f32>::new(w, h);
            for iy in 0..h {
                for ix in 0..w {
                    z.set(ix, iy, 0.3 + 0.025 * ix as f32);
                }
            }
            world.derived.z_filled = Some(z);
            world.derived.coast_mask = Some(all_land_coast(w, h));
            world
        };
        let mut w1 = make_world();
        let mut w2 = make_world();
        FogLikelihoodStage.run(&mut w1).expect("run1");
        FogLikelihoodStage.run(&mut w2).expect("run2");
        assert_eq!(
            &w1.derived.fog_likelihood.as_ref().unwrap().data,
            &w2.derived.fog_likelihood.as_ref().unwrap().data
        );
    }

    // 5. Sea cells are forced to 0 even when their elevation would produce
    //    strong fog. Guards the `is_sea` early-continue in the inner loop.
    #[test]
    fn sea_cells_have_zero_fog() {
        let (w, h) = (8_u32, 8_u32);
        let mut world = WorldState::new(Seed(0), preset(0.0), Resolution::new(w, h));

        // Put (4,4) at exactly inversion_z so it would have strong fog if land.
        let inversion_z = INVERSION_Z_FRACTION; // max_relief = 1.0
        let mut z = ScalarField2D::<f32>::new(w, h);
        z.data.fill(inversion_z);
        world.derived.z_filled = Some(z);

        let mut is_land = MaskField2D::new(w, h);
        is_land.data.fill(1);
        is_land.set(4, 4, 0);
        let mut is_sea = MaskField2D::new(w, h);
        is_sea.set(4, 4, 1);
        world.derived.coast_mask = Some(CoastMask {
            is_land,
            is_sea,
            is_coast: MaskField2D::new(w, h),
            land_cell_count: w * h - 1,
            river_mouth_mask: None,
        });

        FogLikelihoodStage.run(&mut world).expect("stage failed");
        let fog = world.derived.fog_likelihood.as_ref().unwrap();
        assert_eq!(
            fog.get(4, 4),
            0.0,
            "sea cell must have fog == 0 regardless of terrain"
        );
    }

    // 6. Missing precondition errors cleanly.
    #[test]
    fn errors_when_z_filled_missing() {
        let mut world = WorldState::new(Seed(0), preset(0.0), Resolution::new(4, 4));
        let result = FogLikelihoodStage.run(&mut world);
        assert!(result.is_err());
    }
}
