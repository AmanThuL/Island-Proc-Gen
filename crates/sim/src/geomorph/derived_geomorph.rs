//! Derived geomorph stage — Task 1A.4 (slope) + Task 1B.0b (curvature).
//!
//! Reads `world.derived.z_filled` (output of `PitFillStage`) and writes
//! both `world.derived.slope = |grad z_filled|` and
//! `world.derived.curvature = laplacian(z_filled)` in a single pass.
//!
//! Sea cells are forced to `0.0` for both fields to suppress shoreline
//! discretization artefacts. Land cells use central finite differences
//! with a one-sided fallback at domain boundaries. `dx = dy = 1.0` (cell
//! units) so the slope is in "rise per cell" and the laplacian is in
//! `z_units / cell²`.

use island_core::field::ScalarField2D;
use island_core::pipeline::SimulationStage;
use island_core::world::WorldState;

/// Task 1A.4 + Task 1B.0b: populate `world.derived.{slope, curvature}`.
pub struct DerivedGeomorphStage;

impl SimulationStage for DerivedGeomorphStage {
    fn name(&self) -> &'static str {
        "derived_geomorph"
    }

    fn run(&self, world: &mut WorldState) -> anyhow::Result<()> {
        let z = world.derived.z_filled.as_ref().ok_or_else(|| {
            anyhow::anyhow!("DerivedGeomorphStage: z_filled is None (PitFillStage must run first)")
        })?;

        let coast = world.derived.coast_mask.as_ref().ok_or_else(|| {
            anyhow::anyhow!(
                "DerivedGeomorphStage: coast_mask is None (CoastMaskStage must run first)"
            )
        })?;

        let w = z.width;
        let h = z.height;
        let mut slope = ScalarField2D::<f32>::new(w, h);
        let mut curvature = ScalarField2D::<f32>::new(w, h);

        for iy in 0..h {
            for ix in 0..w {
                if coast.is_sea.get(ix, iy) == 1 {
                    // sea cells written as exactly 0.0 — no shoreline artefacts
                    continue;
                }

                let z_here = z.get(ix, iy);

                // ── central-diff gradient with one-sided boundary ──────
                let (z_xm, z_xp) = if ix == 0 {
                    (z_here, z.get(1, iy))
                } else if ix == w - 1 {
                    (z.get(w - 2, iy), z_here)
                } else {
                    (z.get(ix - 1, iy), z.get(ix + 1, iy))
                };
                let (z_ym, z_yp) = if iy == 0 {
                    (z_here, z.get(ix, 1))
                } else if iy == h - 1 {
                    (z.get(ix, h - 2), z_here)
                } else {
                    (z.get(ix, iy - 1), z.get(ix, iy + 1))
                };

                let gx = if ix == 0 || ix == w - 1 {
                    z_xp - z_xm
                } else {
                    (z_xp - z_xm) * 0.5
                };
                let gy = if iy == 0 || iy == h - 1 {
                    z_yp - z_ym
                } else {
                    (z_yp - z_ym) * 0.5
                };
                slope.set(ix, iy, (gx * gx + gy * gy).sqrt());

                // ── 5-point stencil laplacian ──────────────────────────
                // `laplacian(z) = z_xp + z_xm + z_yp + z_ym - 4*z_here`.
                // At a domain boundary the missing neighbour is replaced
                // by `z_here` (Neumann / reflecting ghost), which biases
                // the estimate: e.g. for `z = x² + y²` the interior value
                // is exactly 4 but boundary cells land on 3 (one ghost)
                // or 2 (two ghosts, at corners). Downstream consumers
                // treat boundary curvature as indicative, not analytical.
                let lap = z_xp + z_xm + z_yp + z_ym - 4.0 * z_here;
                curvature.set(ix, iy, lap);
            }
        }

        world.derived.slope = Some(slope);
        world.derived.curvature = Some(curvature);
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
            erosion: Default::default(),
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

    /// Left half is sea, right half is land. Used by both sea-zero tests
    /// (slope and curvature) to verify their respective zero policies.
    fn left_half_sea_mask(w: u32, h: u32) -> CoastMask {
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
        CoastMask {
            is_land,
            is_sea,
            is_coast,
            land_cell_count: land_count,
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
        let (a, b) = (0.1_f32, 0.2_f32);
        let mut z = ScalarField2D::<f32>::new(w, h);
        for iy in 0..h {
            for ix in 0..w {
                z.set(ix, iy, a * ix as f32 + b * iy as f32 + 0.1);
            }
        }

        let mut world = make_world_with_fields(z, left_half_sea_mask(w, h));
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

    // 4. Two runs on the same input produce bit-exact slope + curvature.
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

        let s1 = &world1.derived.slope.as_ref().unwrap().data;
        let s2 = &world2.derived.slope.as_ref().unwrap().data;
        assert_eq!(s1, s2, "slope must be bit-exact across two fresh runs");

        let c1 = &world1.derived.curvature.as_ref().unwrap().data;
        let c2 = &world2.derived.curvature.as_ref().unwrap().data;
        assert_eq!(c1, c2, "curvature must be bit-exact across two fresh runs");
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

    // 7. Analytical laplacian: z = x² + y² → curvature = 4 at every interior
    //    cell. Boundary cells use one-sided Neumann fallback and produce
    //    offset values (dependent on how many sides fall back), so the
    //    assertion is restricted to the strict interior.
    #[test]
    fn quadratic_bowl_interior_curvature_is_four() {
        let (w, h) = (8_u32, 8_u32);
        let mut z = ScalarField2D::<f32>::new(w, h);
        for iy in 0..h {
            for ix in 0..w {
                let x = ix as f32;
                let y = iy as f32;
                z.set(ix, iy, x * x + y * y);
            }
        }

        let mut world = make_world_with_fields(z, all_land_mask(w, h));
        DerivedGeomorphStage.run(&mut world).expect("stage failed");
        let c = world.derived.curvature.as_ref().unwrap();

        for iy in 1..(h - 1) {
            for ix in 1..(w - 1) {
                let v = c.get(ix, iy);
                assert!(
                    (v - 4.0).abs() < 1e-4,
                    "interior ({ix},{iy}): expected 4.0, got {v}"
                );
            }
        }
    }

    // 8. Neumann-ghost boundary bias pin. At `(0, 4)` with `z = x²+y²`:
    //    z_here = 16, z_xp = 17, z_ym = 9, z_yp = 25, z_xm ← z_here (Neumann).
    //    lap = 17 + 16 + 25 + 9 - 4*16 = 3. Locks the fallback semantics
    //    so a future refactor can't silently switch to a true one-sided
    //    stencil and change downstream overlay output.
    #[test]
    fn quadratic_bowl_boundary_is_neumann_biased() {
        let (w, h) = (8_u32, 8_u32);
        let mut z = ScalarField2D::<f32>::new(w, h);
        for iy in 0..h {
            for ix in 0..w {
                let x = ix as f32;
                let y = iy as f32;
                z.set(ix, iy, x * x + y * y);
            }
        }

        let mut world = make_world_with_fields(z, all_land_mask(w, h));
        DerivedGeomorphStage.run(&mut world).expect("stage failed");
        let c = world.derived.curvature.as_ref().unwrap();

        // Left edge, interior y: one ghost → 3.
        let left_interior = c.get(0, 4);
        assert!(
            (left_interior - 3.0).abs() < 1e-4,
            "left-edge interior should land on 3.0, got {left_interior}"
        );

        // Top-left corner: two ghosts → 2.
        let corner = c.get(0, 0);
        assert!(
            (corner - 2.0).abs() < 1e-4,
            "top-left corner should land on 2.0 (two ghosts), got {corner}"
        );
    }

    // 9. Cone apex has strongly negative curvature (concave down at the
    //    summit), while the smooth flank interior is near zero.
    #[test]
    fn cone_apex_curvature_is_negative() {
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
        let c = world.derived.curvature.as_ref().unwrap();

        let apex = c.get(4, 4);
        assert!(
            apex < -0.1,
            "cone apex should have clearly negative curvature, got {apex}"
        );
    }

    // 9. Sea cells curvature = 0 bit-exact (same policy as slope).
    #[test]
    fn sea_cells_have_zero_curvature() {
        let (w, h) = (8_u32, 8_u32);
        let mut z = ScalarField2D::<f32>::new(w, h);
        for iy in 0..h {
            for ix in 0..w {
                let x = ix as f32;
                let y = iy as f32;
                z.set(ix, iy, x * x + y * y);
            }
        }

        let mut world = make_world_with_fields(z, left_half_sea_mask(w, h));
        DerivedGeomorphStage.run(&mut world).expect("stage failed");

        let c = world.derived.curvature.as_ref().unwrap();
        for iy in 0..h {
            for ix in 0..(w / 2) {
                assert_eq!(
                    c.get(ix, iy),
                    0.0,
                    "sea cell ({ix},{iy}) must have curvature == 0.0"
                );
            }
        }
    }

    // 10. Err when coast_mask is None.
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
