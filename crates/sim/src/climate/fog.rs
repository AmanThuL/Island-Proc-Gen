//! `FogLikelihoodStage` (DD7) — elevation-band × orographic uplift proxy.
//!
//! For each land cell:
//!
//! ```text
//! elevation_factor = smoothstep(CLOUD_BASE_Z - CLOUD_EDGE_SOFTNESS, CLOUD_BASE_Z, z_norm)
//!                  * (1 - smoothstep(CLOUD_TOP_Z, CLOUD_TOP_Z + CLOUD_EDGE_SOFTNESS, z_norm))
//! uplift_factor    = smoothstep(0, UPLIFT_SATURATION, max(0, signed_uplift))
//! fog_likelihood   = elevation_factor * uplift_factor
//! ```
//!
//! Sea cells are forced to `0.0`. The sign convention comes from
//! `climate::common::signed_uplift` (shared with `PrecipitationStage`).
//! Consumed by `BiomeWeightsStage::CloudForest` in DD6.

use anyhow::anyhow;
use island_core::field::ScalarField2D;
use island_core::pipeline::SimulationStage;
use island_core::world::WorldState;

use crate::climate::common::{grad_scalar_at, signed_uplift, smoothstep, wind_unit};

/// Lower normalized-elevation bound of the cloud belt.
pub(crate) const CLOUD_BASE_Z: f32 = 0.4;

/// Upper normalized-elevation bound of the cloud belt.
pub(crate) const CLOUD_TOP_Z: f32 = 0.75;

/// Half-width of the smoothstep transition at each band edge.
pub(crate) const CLOUD_EDGE_SOFTNESS: f32 = 0.05;

/// Upper edge of the uplift saturation window. Any positive uplift at
/// or above this value maps to `uplift_factor = 1`.
pub(crate) const UPLIFT_SATURATION: f32 = 0.3;

/// DD7: populate `world.derived.fog_likelihood`.
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

        let mut fog = ScalarField2D::<f32>::new(w, h);
        for iy in 0..h {
            for ix in 0..w {
                if coast.is_sea.get(ix, iy) == 1 {
                    continue; // sea cells stay at 0.0
                }

                let z_norm = z.get(ix, iy);
                let elevation_factor =
                    smoothstep(CLOUD_BASE_Z - CLOUD_EDGE_SOFTNESS, CLOUD_BASE_Z, z_norm)
                        * (1.0
                            - smoothstep(CLOUD_TOP_Z, CLOUD_TOP_Z + CLOUD_EDGE_SOFTNESS, z_norm));

                let grad = grad_scalar_at(z, ix, iy);
                let uplift = signed_uplift(wind, grad).max(0.0);
                let uplift_factor = smoothstep(0.0, UPLIFT_SATURATION, uplift);

                fog.set(ix, iy, elevation_factor * uplift_factor);
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

    fn preset(wind_dir: f32) -> IslandArchetypePreset {
        IslandArchetypePreset {
            name: "fog_test".into(),
            island_radius: 0.5,
            max_relief: 1.0,
            volcanic_center_count: 1,
            island_age: IslandAge::Young,
            prevailing_wind_dir: wind_dir,
            marine_moisture_strength: 1.0,
            sea_level: 0.0,
            erosion: Default::default(),
        }
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

    // 1. A flat domain at the cloud-belt elevation has zero fog because
    //    there's no orographic uplift anywhere (uplift_factor = 0).
    #[test]
    fn flat_cloud_belt_has_no_fog() {
        let (w, h) = (16_u32, 16_u32);
        let mut world = WorldState::new(Seed(0), preset(0.0), Resolution::new(w, h));
        let mut z = ScalarField2D::<f32>::new(w, h);
        z.data.fill(0.6); // inside [CLOUD_BASE_Z, CLOUD_TOP_Z]
        world.derived.z_filled = Some(z);
        world.derived.coast_mask = Some(all_land_coast(w, h));

        FogLikelihoodStage.run(&mut world).expect("stage failed");
        let fog = world.derived.fog_likelihood.as_ref().unwrap();
        for v in fog.data.iter() {
            assert!(*v < 1e-6, "flat belt cell produced fog {v}");
        }
    }

    // 2. A steep windward slope inside the cloud belt produces strong
    //    fog (elevation_factor ≈ 1 × uplift_factor ≈ 1 ≈ 1). A gentle
    //    slope produces ~0 uplift_factor even inside the belt. This
    //    locks both factors as real contributors.
    #[test]
    fn steep_windward_belt_has_strong_fog_gentle_slope_has_none() {
        let (w, h) = (8_u32, 8_u32);
        let mut world = WorldState::new(Seed(0), preset(0.0), Resolution::new(w, h));

        // Steep windward ramp along row 4: z(3,4) = 0.9, z(4,4) = 0.6,
        // z(5,4) = 0.3. The west side is high, the east side is low,
        // so wind from the east climbs the ramp → positive uplift.
        // grad_z.x at (4,4) = (0.3 − 0.9) / 2 = −0.3, signed = 0.3,
        // uplift_factor = smoothstep(0, 0.3, 0.3) = 1.
        let mut z = ScalarField2D::<f32>::new(w, h);
        z.data.fill(0.6); // whole domain sits inside the belt by default
        z.set(3, 4, 0.9);
        z.set(5, 4, 0.3);
        world.derived.z_filled = Some(z);
        world.derived.coast_mask = Some(all_land_coast(w, h));

        FogLikelihoodStage.run(&mut world).expect("stage failed");
        let fog = world.derived.fog_likelihood.as_ref().unwrap();

        let steep = fog.get(4, 4);
        // Math: grad_z.x = (0.3 − 0.9)/2 = −0.3, signed = +0.3,
        // uplift_factor = smoothstep(0, 0.3, 0.3) = 1, elevation_factor
        // at z = 0.6 is 1 (inside the fully-saturated belt), so the
        // expected product is 1.0. Tight bound catches any regression
        // that silently halves either factor.
        assert!(
            steep > 0.99,
            "expected saturated fog at steep cell, got {steep}"
        );

        // A flat cell elsewhere in the belt has uplift_factor = 0.
        let flat = fog.get(1, 1);
        assert!(
            flat < 1e-6,
            "expected zero fog on flat belt cell, got {flat}"
        );
    }

    // 3. Below cloud base: zero fog everywhere.
    #[test]
    fn below_cloud_base_has_no_fog() {
        let (w, h) = (16_u32, 16_u32);
        let mut world = WorldState::new(Seed(0), preset(0.0), Resolution::new(w, h));
        let mut z = ScalarField2D::<f32>::new(w, h);
        // z = 0.2 for all — well below CLOUD_BASE_Z - EDGE_SOFTNESS = 0.35.
        z.data.fill(0.2);
        world.derived.z_filled = Some(z);
        world.derived.coast_mask = Some(all_land_coast(w, h));

        FogLikelihoodStage.run(&mut world).expect("stage failed");
        let fog = world.derived.fog_likelihood.as_ref().unwrap();
        assert_eq!(fog.stats().unwrap().max, 0.0);
    }

    // 4. Above cloud top: zero fog everywhere.
    #[test]
    fn above_cloud_top_has_no_fog() {
        let (w, h) = (16_u32, 16_u32);
        let mut world = WorldState::new(Seed(0), preset(0.0), Resolution::new(w, h));
        let mut z = ScalarField2D::<f32>::new(w, h);
        z.data.fill(0.9); // above CLOUD_TOP_Z + EDGE_SOFTNESS = 0.80
        world.derived.z_filled = Some(z);
        world.derived.coast_mask = Some(all_land_coast(w, h));

        FogLikelihoodStage.run(&mut world).expect("stage failed");
        let fog = world.derived.fog_likelihood.as_ref().unwrap();
        assert_eq!(fog.stats().unwrap().max, 0.0);
    }

    // 5. Determinism: two runs bit-exact.
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

    // 6. Sea cells are forced to 0 even when their elevation + uplift
    //    would otherwise produce strong fog. Guards the `is_sea` early-
    //    continue in the inner loop.
    #[test]
    fn sea_cells_have_zero_fog() {
        let (w, h) = (8_u32, 8_u32);
        let mut world = WorldState::new(Seed(0), preset(0.0), Resolution::new(w, h));

        // Same steep ramp as test 2, but mark (4, 4) as sea.
        let mut z = ScalarField2D::<f32>::new(w, h);
        z.data.fill(0.6);
        z.set(3, 4, 0.9);
        z.set(5, 4, 0.3);
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

    // 7. Missing precondition errors cleanly.
    #[test]
    fn errors_when_z_filled_missing() {
        let mut world = WorldState::new(Seed(0), preset(0.0), Resolution::new(4, 4));
        let result = FogLikelihoodStage.run(&mut world);
        assert!(result.is_err());
    }
}
