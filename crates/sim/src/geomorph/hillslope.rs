//! Hillslope diffusion stage — Sprint 2 DD2.
//!
//! Applies `n_diff_substep` sub-steps of explicit-Euler diffusion
//! `∂z/∂t = D · ∇²z` to `authoritative.height` each time the stage runs.
//!
//! The outer `ErosionOuterLoop` (Task 2.3) calls this stage `n_inner × n_batch`
//! times; per call this stage performs `n_diff_substep` internal iterations.
//!
//! **Boundary treatment (DD2):**
//! - Sea cells (`is_sea == 1`): never written.
//! - Coast cells (`is_coast == 1`): skipped — coast lives near sea-level,
//!   diffusion would underflow.
//! - Sim-grid outer ring (`ix == 0 || ix == w-1 || iy == 0 || iy == h-1`):
//!   skipped — no full 4-neighbour stencil available.

use island_core::pipeline::SimulationStage;
use island_core::world::WorldState;

/// Sprint 2 DD2: hillslope creep via explicit-Euler diffusion.
///
/// `∂z/∂t = D · ∇²z`, solved with `n_diff_substep` sub-steps each call at
/// `dt_sub = 1.0 / n_diff_substep`. Parameters are read from
/// `world.preset.erosion` at run time so slider changes take effect on the
/// next rerun.
///
/// # Example (doc-test disabled due to workspace shadowing — see lib.rs)
pub struct HillslopeDiffusionStage;

impl SimulationStage for HillslopeDiffusionStage {
    fn name(&self) -> &'static str {
        "hillslope_diffusion"
    }

    fn run(&self, world: &mut WorldState) -> anyhow::Result<()> {
        let d = world.preset.erosion.hillslope_d;
        let n_sub = world.preset.erosion.n_diff_substep as usize;
        let width = world.resolution.sim_width as usize;
        let height = world.resolution.sim_height as usize;

        // ── prerequisite checks ───────────────────────────────────────────────
        if world.authoritative.height.is_none() {
            anyhow::bail!(
                "HillslopeDiffusionStage prerequisite missing: \
                 authoritative.height (TopographyStage must run first)"
            );
        }
        if world.derived.coast_mask.is_none() {
            anyhow::bail!(
                "HillslopeDiffusionStage prerequisite missing: \
                 derived.coast_mask (CoastMaskStage must run first)"
            );
        }

        // ── set up double buffer ──────────────────────────────────────────────
        // We need one reusable scratch buffer. Allocate once outside the
        // substep loop and swap in-place to avoid per-substep allocation.
        let dt_sub = 1.0_f32 / n_sub as f32;

        // Split borrow: `world.derived` and `world.authoritative` are disjoint
        // struct fields, so shared refs into `coast_mask` coexist with the
        // `&mut` into `authoritative.height`.
        let coast_mask = world.derived.coast_mask.as_ref().unwrap();
        let is_sea = &coast_mask.is_sea.data;
        let is_coast = &coast_mask.is_coast.data;
        let h_field = world.authoritative.height.as_mut().unwrap();

        // Scratch buffer reused across all substeps.
        let mut z_new: Vec<f32> = h_field.data.clone();

        for _ in 0..n_sub {
            // z_new starts as a copy of the current state; we overwrite only
            // interior land cells and then swap.
            z_new.copy_from_slice(&h_field.data);

            for iy in 1..(height - 1) {
                for ix in 1..(width - 1) {
                    let i = iy * width + ix;

                    if is_sea[i] == 1 {
                        continue;
                    }
                    if is_coast[i] == 1 {
                        continue;
                    }

                    let z_here = h_field.data[i];
                    let z_n = h_field.data[(iy - 1) * width + ix];
                    let z_s = h_field.data[(iy + 1) * width + ix];
                    let z_w = h_field.data[iy * width + (ix - 1)];
                    let z_e = h_field.data[iy * width + (ix + 1)];

                    let lap = z_n + z_s + z_e + z_w - 4.0 * z_here;
                    z_new[i] = z_here + d * lap * dt_sub;
                }
            }

            // Domain-boundary ring (ix == 0, ix == w-1, iy == 0, iy == h-1)
            // was already preserved by `copy_from_slice` — no write needed.

            // Swap: h_field.data becomes z_new for the next substep.
            std::mem::swap(&mut h_field.data, &mut z_new);
        }

        Ok(())
    }
}

// ─── tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use island_core::field::{MaskField2D, ScalarField2D};
    use island_core::pipeline::SimulationStage;
    use island_core::preset::{ErosionParams, IslandAge, IslandArchetypePreset};
    use island_core::seed::Seed;
    use island_core::world::{CoastMask, Resolution, WorldState};

    use super::HillslopeDiffusionStage;

    // ── helpers ───────────────────────────────────────────────────────────────

    fn base_preset() -> IslandArchetypePreset {
        IslandArchetypePreset {
            name: "hillslope_test".into(),
            island_radius: 0.5,
            max_relief: 0.8,
            volcanic_center_count: 1,
            island_age: IslandAge::Young,
            prevailing_wind_dir: 0.0,
            marine_moisture_strength: 0.5,
            sea_level: 0.0,
            erosion: ErosionParams::default(),
        }
    }

    fn all_land_coast(w: u32, h: u32) -> CoastMask {
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

    fn make_world(w: u32, h: u32, preset: IslandArchetypePreset) -> WorldState {
        let mut world = WorldState::new(Seed(0), preset, Resolution::new(w, h));
        let mut height = ScalarField2D::<f32>::new(w, h);
        height.data.fill(0.0);
        world.authoritative.height = Some(height);
        world.derived.coast_mask = Some(all_land_coast(w, h));
        world
    }

    // ─── Test 1: uniform flat field → Laplacian == 0 everywhere ─────────────
    //
    // Every cell = 0.5, all land. After run(), every cell must be unchanged.
    #[test]
    fn hillslope_preserves_uniform_flat_field() {
        let (w, h) = (8u32, 8u32);
        let mut world = make_world(w, h, base_preset());
        world.authoritative.height.as_mut().unwrap().data.fill(0.5);

        HillslopeDiffusionStage.run(&mut world).expect("run failed");

        let h_field = world.authoritative.height.as_ref().unwrap();
        for i in 0..(w * h) as usize {
            assert!(
                (h_field.data[i] - 0.5).abs() < 1e-6,
                "cell {i}: flat field should be unchanged, got {}",
                h_field.data[i]
            );
        }
    }

    // ─── Test 2: tent field smooths toward mean ───────────────────────────────
    //
    // 16×16 all-land; center cell = 1.0, others = 0.0. After run with default
    // D=1e-3, n_diff_substep=4, the center strictly decreases and each of its
    // 4-neighbors strictly increases.
    #[test]
    fn hillslope_smooths_tent_toward_mean() {
        let (w, h) = (16u32, 16u32);
        let mut world = make_world(w, h, base_preset());
        let (cx, cy) = (8usize, 8usize);
        {
            let f = world.authoritative.height.as_mut().unwrap();
            f.data.fill(0.0);
            f.data[cy * w as usize + cx] = 1.0;
        }

        let center_before = 1.0_f32;
        let neighbor_before = 0.0_f32;

        HillslopeDiffusionStage.run(&mut world).expect("run failed");

        let f = world.authoritative.height.as_ref().unwrap();
        let wi = w as usize;
        let center_after = f.data[cy * wi + cx];
        let n_after = f.data[(cy - 1) * wi + cx];
        let s_after = f.data[(cy + 1) * wi + cx];
        let e_after = f.data[cy * wi + (cx + 1)];
        let ww_after = f.data[cy * wi + (cx - 1)];

        assert!(
            center_after < center_before,
            "center cell should strictly decrease: before={center_before}, after={center_after}"
        );
        assert!(
            n_after > neighbor_before,
            "north neighbor should strictly increase: before={neighbor_before}, after={n_after}"
        );
        assert!(
            s_after > neighbor_before,
            "south neighbor should strictly increase: before={neighbor_before}, after={s_after}"
        );
        assert!(
            e_after > neighbor_before,
            "east neighbor should strictly increase: before={neighbor_before}, after={e_after}"
        );
        assert!(
            ww_after > neighbor_before,
            "west neighbor should strictly increase: before={neighbor_before}, after={ww_after}"
        );
    }

    // ─── Test 3: coast and sea cells are untouched ────────────────────────────
    //
    // 8×8: row 0 = all sea, row 1 = all coast, rows 2..8 = interior land.
    // Initial heights: 0.0 on sea, 0.5 on land/coast, cell (4,4) = 1.0.
    // After run(), row 0 and row 1 must be unchanged.
    #[test]
    fn hillslope_leaves_coast_and_sea_unchanged() {
        let (w, h) = (8u32, 8u32);
        let n = (w * h) as usize;

        let mut is_land = MaskField2D::new(w, h);
        let mut is_sea = MaskField2D::new(w, h);
        let mut is_coast = MaskField2D::new(w, h);

        for ix in 0..w {
            // row 0: sea
            is_sea.data[(0 * w + ix) as usize] = 1;
            // row 1: coast
            is_coast.data[(1 * w + ix) as usize] = 1;
            // rows 2..8: land
            for iy in 2..h {
                is_land.data[(iy * w + ix) as usize] = 1;
            }
        }
        let coast = CoastMask {
            is_land,
            is_sea,
            is_coast,
            land_cell_count: (w * (h - 2)) as u32,
            river_mouth_mask: None,
        };

        let mut world = WorldState::new(Seed(0), base_preset(), Resolution::new(w, h));
        let mut height = ScalarField2D::<f32>::new(w, h);
        height.data.fill(0.5);
        // Row 0 (sea) gets 0.0
        for ix in 0..w as usize {
            height.data[ix] = 0.0;
        }
        // Set one interior spike
        height.data[4 * w as usize + 4] = 1.0;
        world.authoritative.height = Some(height);
        world.derived.coast_mask = Some(coast);

        // Snapshot sea (row 0) and coast (row 1) before run.
        let sea_before: Vec<f32> = (0..w as usize)
            .map(|ix| world.authoritative.height.as_ref().unwrap().data[ix])
            .collect();
        let coast_before: Vec<f32> = (0..w as usize)
            .map(|ix| world.authoritative.height.as_ref().unwrap().data[w as usize + ix])
            .collect();

        HillslopeDiffusionStage.run(&mut world).expect("run failed");

        let f = world.authoritative.height.as_ref().unwrap();
        for ix in 0..w as usize {
            assert_eq!(
                f.data[ix], sea_before[ix],
                "sea cell ({ix},0) must be unchanged"
            );
            assert_eq!(
                f.data[w as usize + ix],
                coast_before[ix],
                "coast cell ({ix},1) must be unchanged"
            );
        }

        // Avoid unused variable warning.
        let _ = n;
    }

    // ─── Test 4: grid boundary cells are never written ────────────────────────
    //
    // 6×6 all-land; set height[0][0] = 1.0, others 0.0. After run(), the
    // corner cell must still be 1.0 (it's on the domain boundary ring).
    #[test]
    fn hillslope_leaves_grid_boundary_unchanged() {
        let (w, h) = (6u32, 6u32);
        let mut world = make_world(w, h, base_preset());
        {
            let f = world.authoritative.height.as_mut().unwrap();
            f.data.fill(0.0);
            f.data[0] = 1.0;
        }

        HillslopeDiffusionStage.run(&mut world).expect("run failed");

        let f = world.authoritative.height.as_ref().unwrap();
        assert_eq!(
            f.data[0], 1.0,
            "corner cell (0,0) is on the domain boundary and must not be written"
        );
    }

    // ─── Test 5: missing height returns Err ──────────────────────────────────
    #[test]
    fn hillslope_missing_height_returns_error() {
        let (w, h) = (8u32, 8u32);
        let mut world = WorldState::new(Seed(0), base_preset(), Resolution::new(w, h));
        // authoritative.height intentionally None
        world.derived.coast_mask = Some(all_land_coast(w, h));

        let result = HillslopeDiffusionStage.run(&mut world);
        assert!(
            result.is_err(),
            "expected Err when authoritative.height is None"
        );
    }

    // ─── Test 6: missing coast_mask returns Err ───────────────────────────────
    #[test]
    fn hillslope_missing_coast_mask_returns_error() {
        let (w, h) = (8u32, 8u32);
        let mut world = WorldState::new(Seed(0), base_preset(), Resolution::new(w, h));
        let mut height = ScalarField2D::<f32>::new(w, h);
        height.data.fill(0.5);
        world.authoritative.height = Some(height);
        // derived.coast_mask intentionally None

        let result = HillslopeDiffusionStage.run(&mut world);
        assert!(
            result.is_err(),
            "expected Err when derived.coast_mask is None"
        );
    }

    // ─── Test 7: CFL stability — 10 sequential runs stay finite ──────────────
    //
    // 8×8 all-land; linear ramp along x from 0.0 to 1.0. Run 10 times in
    // sequence (simulating many outer-loop iterations). Every cell must remain
    // finite and within [-0.5, 1.5] (generous bounds; diffusion is
    // monotone-decreasing on extrema so values actually stay in [0, 1]).
    #[test]
    fn hillslope_cfl_stable_at_default_d() {
        let (w, h) = (8u32, 8u32);
        let mut world = make_world(w, h, base_preset());
        {
            let f = world.authoritative.height.as_mut().unwrap();
            for iy in 0..h as usize {
                for ix in 0..w as usize {
                    f.data[iy * w as usize + ix] = ix as f32 / (w - 1) as f32;
                }
            }
        }

        for _ in 0..10 {
            HillslopeDiffusionStage.run(&mut world).expect("run failed");
        }

        let f = world.authoritative.height.as_ref().unwrap();
        for i in 0..(w * h) as usize {
            let v = f.data[i];
            assert!(
                v.is_finite(),
                "cell {i}: height is non-finite after 10 runs: {v}"
            );
            assert!(
                v >= -0.5 && v <= 1.5,
                "cell {i}: height {v} escaped bounds [-0.5, 1.5]"
            );
        }
    }
}
