//! Stream Power Incision Model — Sprint 2 DD1.
//!
//! Applies one SPIM iteration to `authoritative.height` in place for every
//! land cell.  The incision flux is `Ef = K · A^m · S^n` (Whipple & Tucker
//! 1999; KP17 §3.1). Parameters are read from `world.preset.erosion` at
//! run time so slider changes take effect on the next re-run.
//!
//! This stage is in-place-on-height only.  It does **not** update any
//! `derived.*` field — `ErosionOuterLoop` (Task 2.3) handles cache
//! invalidation and flow-network rebuilds around repeated SPIM calls.

use island_core::pipeline::SimulationStage;
use island_core::world::WorldState;

/// Sprint 2 DD1: Stream Power Incision Model (SPIM).
///
/// `Ef = K · A^m · S^n`, with `(m, n) = (0.35, 1.0)` locked in v1 to
/// avoid the KP17 pathological `m/n = 0.5` regime. Mutates
/// `authoritative.height` in place for every land cell; clamps new height
/// at `sea_level` to prevent negative / below-sea heights that would
/// produce NaN slopes downstream.
///
/// Unit struct — all params read from `world.preset.erosion` at run time
/// so mid-session slider changes take effect on the next rerun.
pub struct StreamPowerIncisionStage;

impl SimulationStage for StreamPowerIncisionStage {
    fn name(&self) -> &'static str {
        "stream_power_incision"
    }

    fn run(&self, world: &mut WorldState) -> anyhow::Result<()> {
        let k = world.preset.erosion.spim_k;
        let m = world.preset.erosion.spim_m;
        let n = world.preset.erosion.spim_n;
        let sea_level = world.preset.sea_level;

        // ── prerequisite checks ───────────────────────────────────────────────
        // Verify derived prerequisites exist before taking &mut on height.
        // The length checks use Option::is_none() so the borrow checker is
        // happy: we don't hold references into `world` across the mutable
        // borrow below.
        if world.derived.accumulation.is_none() {
            anyhow::bail!(
                "StreamPowerIncisionStage prerequisite missing: \
                 derived.accumulation (run FlowRouting+Accumulation first)"
            );
        }
        if world.derived.slope.is_none() {
            anyhow::bail!(
                "StreamPowerIncisionStage prerequisite missing: \
                 derived.slope (run DerivedGeomorph first)"
            );
        }
        if world.derived.coast_mask.is_none() {
            anyhow::bail!(
                "StreamPowerIncisionStage prerequisite missing: \
                 derived.coast_mask (run CoastMask first)"
            );
        }

        // Verify authoritative height exists.
        if world.authoritative.height.is_none() {
            anyhow::bail!(
                "StreamPowerIncisionStage: authoritative.height is None \
                 (TopographyStage must run first)"
            );
        }

        // ── cell loop ─────────────────────────────────────────────────────────
        // Split borrow: `world.derived` and `world.authoritative` are disjoint
        // struct fields so the compiler accepts shared refs into `derived` held
        // simultaneously with the `&mut` into `authoritative.height`.
        let n_cells = world.resolution.sim_width as usize * world.resolution.sim_height as usize;

        let accumulation = &world.derived.accumulation.as_ref().unwrap().data;
        let slope = &world.derived.slope.as_ref().unwrap().data;
        let is_land = &world.derived.coast_mask.as_ref().unwrap().is_land.data;
        let h_field = world.authoritative.height.as_mut().unwrap();

        for i in 0..n_cells {
            // Sea cells: erosion noop.
            if is_land[i] == 0 {
                continue;
            }

            let a = accumulation[i];
            let s = slope[i];

            // `ef` = K * A^m * S^n. Non-finite → 0.0 so extreme accumulation
            // (e.g. f32::MAX) doesn't silently NaN-propagate into the height field.
            let ef = k * a.powf(m) * s.powf(n);
            let ef = if ef.is_finite() { ef } else { 0.0 };

            // In-place incision; clamp at sea_level (dt = 1.0 in v1).
            h_field.data[i] = (h_field.data[i] - ef).max(sea_level);
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

    use super::StreamPowerIncisionStage;

    // ── helpers ───────────────────────────────────────────────────────────────

    fn base_preset(sea_level: f32) -> IslandArchetypePreset {
        IslandArchetypePreset {
            name: "spim_test".into(),
            island_radius: 0.5,
            max_relief: 0.8,
            volcanic_center_count: 1,
            island_age: IslandAge::Young,
            prevailing_wind_dir: 0.0,
            marine_moisture_strength: 0.5,
            sea_level,
            erosion: ErosionParams::default(),
        }
    }

    fn preset_with_k(sea_level: f32, spim_k: f32) -> IslandArchetypePreset {
        IslandArchetypePreset {
            erosion: ErosionParams {
                spim_k,
                ..ErosionParams::default()
            },
            ..base_preset(sea_level)
        }
    }

    /// All-land coast mask for a `w × h` grid.
    fn all_land_coast(w: u32, h: u32) -> CoastMask {
        let n = w * h;
        let mut is_land = MaskField2D::new(w, h);
        is_land.data.fill(1);
        CoastMask {
            is_land,
            is_sea: MaskField2D::new(w, h),
            is_coast: MaskField2D::new(w, h),
            land_cell_count: n,
            river_mouth_mask: None,
        }
    }

    /// Mixed coast mask: rows 0..half are sea, rows half..h are land.
    fn half_sea_coast(w: u32, h: u32) -> CoastMask {
        let half = h / 2;
        let sea_cells = (half * w) as usize;
        let land_cells = ((h - half) * w) as usize;

        let mut is_land = MaskField2D::new(w, h);
        is_land.data[sea_cells..].fill(1);

        let mut is_sea = MaskField2D::new(w, h);
        is_sea.data[..sea_cells].fill(1);

        CoastMask {
            is_land,
            is_sea,
            is_coast: MaskField2D::new(w, h),
            land_cell_count: land_cells as u32,
            river_mouth_mask: None,
        }
    }

    /// Build a world with the three required derived fields pre-set.
    fn make_world(
        w: u32,
        h: u32,
        preset: IslandArchetypePreset,
        height_val: f32,
        slope_val: f32,
        accum_val: f32,
        coast: CoastMask,
    ) -> WorldState {
        let mut world = WorldState::new(Seed(0), preset, Resolution::new(w, h));

        let mut height = ScalarField2D::<f32>::new(w, h);
        height.data.fill(height_val);
        world.authoritative.height = Some(height);

        let mut slope = ScalarField2D::<f32>::new(w, h);
        slope.data.fill(slope_val);
        world.derived.slope = Some(slope);

        let mut accum = ScalarField2D::<f32>::new(w, h);
        accum.data.fill(accum_val);
        world.derived.accumulation = Some(accum);

        world.derived.coast_mask = Some(coast);
        world
    }

    // ─── Test 1: SPIM reduces height on uniform land ──────────────────────────
    //
    // 8×8 grid, all land, height=0.5, slope=0.1, accumulation=1.0, default K/m/n.
    // Expected ef ≈ K * 1^m * 0.1^n = 1e-3 * 0.1 = 1e-4.
    // Assert: 0 < h_before - h_after < 5e-4 for every cell.
    #[test]
    fn spim_reduces_height_on_uniform_land() {
        let (w, h) = (8u32, 8u32);
        let preset = base_preset(0.0);
        let mut world = make_world(w, h, preset, 0.5, 0.1, 1.0, all_land_coast(w, h));

        let h_before: Vec<f32> = world.authoritative.height.as_ref().unwrap().data.clone();

        StreamPowerIncisionStage
            .run(&mut world)
            .expect("spim run failed");

        let h_after = world.authoritative.height.as_ref().unwrap();
        for i in 0..(w * h) as usize {
            let delta = h_before[i] - h_after.data[i];
            assert!(
                delta > 0.0,
                "cell {i}: height should decrease, delta={delta}"
            );
            assert!(
                delta < 5e-4,
                "cell {i}: height decrease too large, delta={delta}"
            );
        }
    }

    // ─── Test 2: SPIM is a noop on sea cells ─────────────────────────────────
    //
    // Half-sea grid; sea cell heights must be unchanged after SPIM.
    #[test]
    fn spim_noop_on_sea_cells() {
        let (w, h) = (8u32, 8u32);
        let preset = base_preset(0.0);
        let coast = half_sea_coast(w, h);
        let mut world = make_world(w, h, preset, 0.5, 0.1, 1.0, coast);

        let h_before: Vec<f32> = world.authoritative.height.as_ref().unwrap().data.clone();

        StreamPowerIncisionStage
            .run(&mut world)
            .expect("spim run failed");

        let h_after = world.authoritative.height.as_ref().unwrap();
        let half = (h / 2) as usize;
        for iy in 0..half {
            for ix in 0..w as usize {
                let i = iy * w as usize + ix;
                assert_eq!(
                    h_after.data[i], h_before[i],
                    "sea cell ({ix},{iy}) height must not change"
                );
            }
        }
    }

    // ─── Test 3: SPIM clamps at sea_level ────────────────────────────────────
    //
    // height == sea_level with large K=1.0; ef would be huge but every cell
    // must clamp to sea_level (no underflow, no NaN).
    #[test]
    fn spim_clamps_at_sea_level() {
        let (w, h) = (8u32, 8u32);
        let sea_level = 0.3;
        let preset = preset_with_k(sea_level, 1.0);
        let mut world = make_world(w, h, preset, sea_level, 0.5, 10.0, all_land_coast(w, h));

        StreamPowerIncisionStage
            .run(&mut world)
            .expect("spim run failed");

        let h_field = world.authoritative.height.as_ref().unwrap();
        for i in 0..(w * h) as usize {
            assert!(
                h_field.data[i] >= sea_level,
                "cell {i}: height {:.6} below sea_level {sea_level}",
                h_field.data[i]
            );
            assert!(
                h_field.data[i].is_finite(),
                "cell {i}: height is non-finite"
            );
        }
    }

    // ─── Test 4: SPIM stays finite on extreme accumulation ───────────────────
    //
    // accumulation = f32::MAX with locked `m = 0.35` produces `ef ≈ 3e9`,
    // finite but huge — the sea-level clamp path catches it and heights
    // stay finite (not the non-finite guard, which cannot trigger at
    // `m ≤ 1.0` because `f32::MAX.powf(0.35) ≈ 3e13 < f32::INF`).
    #[test]
    fn spim_finite_on_extreme_accumulation() {
        let (w, h) = (8u32, 8u32);
        let sea_level = 0.0;
        let preset = base_preset(sea_level);
        let mut world = make_world(w, h, preset, 0.5, 0.1, 1.0, all_land_coast(w, h));

        // Blast a few accumulation cells to f32::MAX.
        if let Some(a) = world.derived.accumulation.as_mut() {
            a.data[0] = f32::MAX;
            a.data[5] = f32::MAX;
            a.data[10] = f32::MAX;
        }

        StreamPowerIncisionStage
            .run(&mut world)
            .expect("spim run failed");

        let h_field = world.authoritative.height.as_ref().unwrap();
        for i in 0..(w * h) as usize {
            assert!(
                h_field.data[i].is_finite(),
                "cell {i}: height is non-finite after extreme accumulation"
            );
        }
    }

    // ─── Test 4b: non-finite guard explicitly exercised ──────────────────────
    //
    // The locked `(m, n) = (0.35, 1.0)` can't produce a non-finite `ef`, so
    // the `if ef.is_finite() { ef } else { 0.0 }` guard is defensively
    // present against future parameter experimentation (e.g. a tuner that
    // overrides `m` past 1.0 and combines with f32::MAX accumulation). Force
    // `m = 2.5` + `A = f32::MAX` to push `A^m` past f32::INF, then assert
    // height still drops at most by the sea-level clamp's worth — meaning
    // `ef` was squashed to 0.0 by the guard, NOT applied as Inf.
    #[test]
    fn spim_non_finite_guard_clamps_to_zero_under_parameter_override() {
        let (w, h) = (4u32, 1u32);
        let sea_level = 0.1;
        let preset = IslandArchetypePreset {
            erosion: ErosionParams {
                spim_k: 1.0,
                spim_m: 2.5, // m > 1 — can produce ef > f32::MAX
                spim_n: 1.0,
                ..ErosionParams::default()
            },
            ..base_preset(sea_level)
        };
        let mut world = make_world(w, h, preset, 0.5, 0.1, 1.0, all_land_coast(w, h));
        world.derived.accumulation.as_mut().unwrap().data[1] = f32::MAX;

        StreamPowerIncisionStage
            .run(&mut world)
            .expect("spim run failed");

        let h_field = world.authoritative.height.as_ref().unwrap();
        // If the guard didn't fire, `h - Inf = -Inf`, clamp to sea_level.
        // If the guard DID fire (ef → 0.0), `h - 0.0 = 0.5`.
        // Assert the non-finite branch produced 0.5, not sea_level 0.1.
        assert!(
            h_field.data[1].is_finite(),
            "cell 1 height non-finite — guard failed"
        );
        assert!(
            (h_field.data[1] - 0.5).abs() < 1e-5,
            "cell 1 height {} != 0.5 — non-finite guard did not clamp ef to 0",
            h_field.data[1]
        );
    }

    // ─── Test 5: missing accumulation returns Err ─────────────────────────────
    //
    // Construct a world with derived.accumulation = None; run must return Err.
    #[test]
    fn spim_missing_accumulation_returns_error() {
        let (w, h) = (8u32, 8u32);
        let preset = base_preset(0.0);
        // Build world manually — no accumulation field.
        let mut world = WorldState::new(Seed(0), preset, Resolution::new(w, h));

        let mut height = ScalarField2D::<f32>::new(w, h);
        height.data.fill(0.5);
        world.authoritative.height = Some(height);

        let mut slope = ScalarField2D::<f32>::new(w, h);
        slope.data.fill(0.1);
        world.derived.slope = Some(slope);

        world.derived.coast_mask = Some(all_land_coast(w, h));
        // derived.accumulation intentionally left None.

        let result = StreamPowerIncisionStage.run(&mut world);
        assert!(
            result.is_err(),
            "expected Err when derived.accumulation is None"
        );
    }
}
