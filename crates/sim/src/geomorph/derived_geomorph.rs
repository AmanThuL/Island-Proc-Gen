//! Derived geomorph stage — Task 1A.4.
//!
//! Reads `world.derived.z_filled` (output of `PitFillStage`) and writes
//! `world.derived.slope = |grad z_filled|`.
//!
//! Sea cells are forced to `slope = 0` to suppress shoreline discretization
//! artefacts; land cells use a central finite difference with one-sided
//! fallback at domain boundaries.  `dx = dy = 1.0` (cell units) for Sprint 1A.

use island_core::field::ScalarField2D;
use island_core::pipeline::SimulationStage;
use island_core::world::WorldState;

/// Sprint 1A Task 1A.4: cache `|grad z_filled|` as `world.derived.slope`.
pub struct DerivedGeomorphStage;

impl SimulationStage for DerivedGeomorphStage {
    fn name(&self) -> &'static str {
        "derived_geomorph"
    }

    fn run(&self, world: &mut WorldState) -> anyhow::Result<()> {
        let z = world
            .derived
            .z_filled
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("DerivedGeomorphStage: z_filled is None (PitFillStage must run first)"))?;

        let coast = world
            .derived
            .coast_mask
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("DerivedGeomorphStage: coast_mask is None (CoastMaskStage must run first)"))?;

        let w = z.width;
        let h = z.height;
        let mut slope = ScalarField2D::<f32>::new(w, h);

        for iy in 0..h {
            for ix in 0..w {
                if coast.is_sea.get(ix, iy) == 1 {
                    // sea cells written as exactly 0.0 — no shoreline artefacts
                    continue;
                }

                let gx = if ix == 0 {
                    z.get(1, iy) - z.get(0, iy)
                } else if ix == w - 1 {
                    z.get(w - 1, iy) - z.get(w - 2, iy)
                } else {
                    (z.get(ix + 1, iy) - z.get(ix - 1, iy)) * 0.5
                };

                let gy = if iy == 0 {
                    z.get(ix, 1) - z.get(ix, 0)
                } else if iy == h - 1 {
                    z.get(ix, h - 1) - z.get(ix, h - 2)
                } else {
                    (z.get(ix, iy + 1) - z.get(ix, iy - 1)) * 0.5
                };

                slope.set(ix, iy, (gx * gx + gy * gy).sqrt());
            }
        }

        world.derived.slope = Some(slope);
        Ok(())
    }
}

// ─── tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use island_core::field::{MaskField2D, ScalarField2D};
    use island_core::pipeline::SimulationStage;
    use island_core::preset::{IslandAge, IslandArchetypePreset};
    use island_core::seed::Seed;
    use island_core::world::{CoastMask, Resolution, WorldState};

    use super::DerivedGeomorphStage;

    fn base_preset() -> IslandArchetypePreset {
        IslandArchetypePreset {
            name: "slope_test".into(),
            island_radius: 0.5,
            max_relief: 0.8,
            volcanic_center_count: 1,
            island_age: IslandAge::Young,
            prevailing_wind_dir: 0.0,
            marine_moisture_strength: 0.5,
            sea_level: -1.0,
        }
    }

    fn all_land_mask(w: u32, h: u32) -> CoastMask {
        let n = (w * h) as usize;
        let mut is_land = MaskField2D::new(w, h);
        for i in 0..n {
            is_land.data[i] = 1;
        }
        let is_sea = MaskField2D::new(w, h);
        let is_coast = MaskField2D::new(w, h);
        CoastMask {
            is_land,
            is_sea,
            is_coast,
            land_cell_count: n as u32,
            river_mouth_mask: None,
        }
    }

    fn make_world_with_fields(z_filled: ScalarField2D<f32>, coast: CoastMask) -> WorldState {
        let w = z_filled.width;
        let h = z_filled.height;
        let mut world = WorldState::new(Seed(0), base_preset(), Resolution::new(w, h));
        world.derived.z_filled = Some(z_filled);
        world.derived.coast_mask = Some(coast);
        world
    }

    // 1. Linear plane z = a*x + b*y — interior cells must have slope == sqrt(a^2 + b^2).
    #[test]
    fn linear_plane_has_uniform_slope() {
        let (a, b) = (0.1_f32, 0.2_f32);
        let expected = (a * a + b * b).sqrt();
        let (w, h) = (8_u32, 8_u32);

        let mut z = ScalarField2D::<f32>::new(w, h);
        for iy in 0..h {
            for ix in 0..w {
                z.set(ix, iy, a * ix as f32 + b * iy as f32);
            }
        }

        let mut world = make_world_with_fields(z, all_land_mask(w, h));
        DerivedGeomorphStage.run(&mut world).expect("stage failed");

        let slope = world.derived.slope.as_ref().unwrap();
        for iy in 1..=6_u32 {
            for ix in 1..=6_u32 {
                let s = slope.get(ix, iy);
                assert!(
                    (s - expected).abs() < 1e-5,
                    "interior ({ix},{iy}): expected {expected}, got {s}"
                );
            }
        }
    }

    // 2. Cone z = max(0, 1 - r/r0): centre slope ≈ 0, ring at dist 2 slope ≈ 1/r0.
    #[test]
    fn cone_slope_is_radial() {
        let (w, h) = (9_u32, 9_u32);
        let (cx, cy) = (4.0_f32, 4.0_f32);
        let r0 = 4.0_f32;

        let mut z = ScalarField2D::<f32>::new(w, h);
        for iy in 0..h {
            for ix in 0..w {
                let r = ((ix as f32 - cx).powi(2) + (iy as f32 - cy).powi(2)).sqrt();
                z.set(ix, iy, (1.0 - r / r0).max(0.0));
            }
        }

        let mut world = make_world_with_fields(z, all_land_mask(w, h));
        DerivedGeomorphStage.run(&mut world).expect("stage failed");

        let slope = world.derived.slope.as_ref().unwrap();

        // centre: gradient of a cone at its apex is ~0
        let centre_slope = slope.get(4, 4);
        assert!(
            centre_slope < 0.1,
            "centre slope should be near 0, got {centre_slope}"
        );

        // cell (2, 4) is distance 2 from centre on the x-axis: slope ≈ 1/r0 = 0.25
        let ring_slope = slope.get(2, 4);
        assert!(
            (ring_slope - 1.0 / r0).abs() < 0.05,
            "ring slope at (2,4) should be ~{}, got {ring_slope}",
            1.0 / r0
        );
    }

    // 3. Sea cells must have slope == 0.0 (bit-exact).
    #[test]
    fn sea_cells_have_zero_slope() {
        let (w, h) = (8_u32, 8_u32);
        let a = 0.1_f32;
        let b = 0.2_f32;

        let mut z = ScalarField2D::<f32>::new(w, h);
        for iy in 0..h {
            for ix in 0..w {
                z.set(ix, iy, a * ix as f32 + b * iy as f32 + 0.1);
            }
        }

        // mark the left half sea, right half land
        let mut is_land = MaskField2D::new(w, h);
        let mut is_sea = MaskField2D::new(w, h);
        let is_coast = MaskField2D::new(w, h);
        let mut land_count = 0_u32;
        for iy in 0..h {
            for ix in 0..w {
                let idx = (iy * w + ix) as usize;
                if ix >= w / 2 {
                    is_land.data[idx] = 1;
                    land_count += 1;
                } else {
                    is_sea.data[idx] = 1;
                }
            }
        }

        let coast = CoastMask {
            is_land,
            is_sea,
            is_coast,
            land_cell_count: land_count,
            river_mouth_mask: None,
        };
        let mut world = make_world_with_fields(z, coast);
        DerivedGeomorphStage.run(&mut world).expect("stage failed");

        let slope = world.derived.slope.as_ref().unwrap();
        for iy in 0..h {
            for ix in 0..(w / 2) {
                assert_eq!(
                    slope.get(ix, iy),
                    0.0,
                    "sea cell ({ix},{iy}) must have slope == 0.0"
                );
            }
        }
    }

    // 4. Two runs on the same input produce bit-exact slope.data.
    #[test]
    fn bit_exact_determinism() {
        let (a, b) = (0.1_f32, 0.2_f32);
        let (w, h) = (8_u32, 8_u32);

        let make_z = || {
            let mut z = ScalarField2D::<f32>::new(w, h);
            for iy in 0..h {
                for ix in 0..w {
                    z.set(ix, iy, a * ix as f32 + b * iy as f32);
                }
            }
            z
        };

        let mut world1 = make_world_with_fields(make_z(), all_land_mask(w, h));
        let mut world2 = make_world_with_fields(make_z(), all_land_mask(w, h));
        DerivedGeomorphStage.run(&mut world1).expect("run 1 failed");
        DerivedGeomorphStage.run(&mut world2).expect("run 2 failed");

        let d1 = &world1.derived.slope.as_ref().unwrap().data;
        let d2 = &world2.derived.slope.as_ref().unwrap().data;
        assert_eq!(d1, d2, "slope must be bit-exact across two fresh runs");
    }

    // 5. Slope is non-negative everywhere.
    #[test]
    fn slope_min_is_non_negative() {
        let (w, h) = (8_u32, 8_u32);
        let (cx, cy) = (4.0_f32, 4.0_f32);
        let r0 = 3.0_f32;

        let mut z = ScalarField2D::<f32>::new(w, h);
        for iy in 0..h {
            for ix in 0..w {
                let r = ((ix as f32 - cx).powi(2) + (iy as f32 - cy).powi(2)).sqrt();
                z.set(ix, iy, 0.05 * ix as f32 + (1.0 - r / r0).max(0.0));
            }
        }

        let mut world = make_world_with_fields(z, all_land_mask(w, h));
        DerivedGeomorphStage.run(&mut world).expect("stage failed");

        let slope = world.derived.slope.as_ref().unwrap();
        let stats = slope.stats().expect("non-empty field");
        assert!(
            stats.min >= 0.0,
            "slope.min must be >= 0.0, got {}",
            stats.min
        );
    }

    // 6. Err when z_filled is None.
    #[test]
    fn errors_when_z_filled_missing() {
        let mut world = WorldState::new(Seed(0), base_preset(), Resolution::new(8, 8));
        // z_filled intentionally left None
        let result = DerivedGeomorphStage.run(&mut world);
        assert!(result.is_err(), "expected Err when z_filled is None");
    }

    // 7. Err when coast_mask is None.
    #[test]
    fn errors_when_coast_mask_missing() {
        let (w, h) = (8_u32, 8_u32);
        let mut z = ScalarField2D::<f32>::new(w, h);
        for i in 0..(w * h) as usize {
            z.data[i] = i as f32 * 0.01;
        }
        let mut world = WorldState::new(Seed(0), base_preset(), Resolution::new(w, h));
        world.derived.z_filled = Some(z);
        // coast_mask intentionally left None
        let result = DerivedGeomorphStage.run(&mut world);
        assert!(result.is_err(), "expected Err when coast_mask is None");
    }
}
