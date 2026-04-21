//! Sprint 3 DD4: LFPM-inspired sequential upwind sweep precipitation model.
//!
//! ## Algorithm
//!
//! Replaces the Sprint 1B per-cell raymarch with a stateful water-vapour `q`
//! field advected in topological (upstream-first) order:
//!
//! ```text
//! q(p) = q(p_upwind)
//!         · exp(-CONDENSATION_DT / TAU_C · uplift_factor)   // orographic condensation
//!         · exp(-CONDENSATION_DT / TAU_F)                    // generic fallout / rain shadow
//!         + marine_recharge(p)                               // coast-proximity recharge
//!
//! P(p) = (q_before - q(p)) / CONDENSATION_DT                // mass balance
//! ```
//!
//! where `uplift_factor = max(0, signed_uplift(wind, grad_z)) * UPLIFT_GAIN`.
//!
//! `marine_recharge(p) = MARINE_RECHARGE_STRENGTH · exp(-dist_to_coast / MARINE_RECHARGE_DECAY)`.
//!
//! ## Sweep order
//!
//! Per-cell "wind phase" = `-wind · position` (ascending sort ⇒ upstream first).
//! Cached in `derived.precipitation_sweep_order` across runs at the same wind
//! direction; invalidated by the `Precipitation` arm of `clear_stage_outputs`.
//!
//! ## Preheat
//!
//! Two throwaway sweeps are run before the main integration to eliminate the
//! `q=0 everywhere → P=0` startup transient when wind is near-axis-aligned.
//! "Throwaway" means a temporary `q` scratch buffer is advanced twice; only the
//! main integration writes to `baked.precipitation`.

use glam::Vec2;
use island_core::field::ScalarField2D;

use crate::climate::common::{grad_scalar_at, signed_uplift};

// ── locked constants (DD4) ────────────────────────────────────────────────────

/// Initial water-vapour at the upwind boundary. Preset-exposed via
/// `ClimateParams.q_0`; this constant is the locked default.
pub const Q_0_DEFAULT: f32 = 1.0;

/// Condensation time scale `τ_c`. Smaller → faster condensation on windward
/// slopes. Preset-exposed via `ClimateParams.tau_c`.
pub const TAU_C_DEFAULT: f32 = 0.15;

/// Generic fallout time scale `τ_f` (rain shadow). Smaller → stronger drying
/// leeward. Preset-exposed via `ClimateParams.tau_f`.
pub const TAU_F_DEFAULT: f32 = 0.60;

/// Scales the orographic component: `uplift_factor = max(0, signed_uplift) * UPLIFT_GAIN`.
/// Not preset-exposed (DD4 comment: "仍留常量").
pub const UPLIFT_GAIN: f32 = 3.0;

/// Peak coastal recharge injected per step at the shoreline.
/// Not preset-exposed.
pub const MARINE_RECHARGE_STRENGTH: f32 = 0.2;

/// Exponential decay length for the marine recharge term (in cells).
/// Not preset-exposed.
pub const MARINE_RECHARGE_DECAY: f32 = 0.08;

/// Explicit Euler step size used in both the condensation and fallout exponents.
pub const CONDENSATION_DT: f32 = 1.0;

/// Number of throwaway preheat sweeps run before the main integration.
/// 2 is sufficient to propagate the Q_0 boundary condition through an
/// axis-aligned domain of any width.
const PREHEAT_SWEEPS: usize = 2;

// ── public entry point ────────────────────────────────────────────────────────

/// Run a full V3 LFPM precipitation sweep and return a normalised `[0, 1]`
/// precipitation field.
///
/// `sweep_order` is an `Option<&mut Option<Vec<usize>>>` pointing at
/// `derived.precipitation_sweep_order`. On the first call the inner
/// `Option` is `None` and the sweep order is computed and stored. On
/// subsequent calls with the same wind direction it is reused.
///
/// # Arguments
///
/// * `z` — pit-filled height field.
/// * `dist_to_coast` — Von4-BFS distance-to-coast in cell units.
/// * `wind` — unit vector (direction of origin, meteorological convention).
/// * `q_0` — initial vapour at the upwind boundary.
/// * `tau_c` — condensation time scale.
/// * `tau_f` — fallout time scale.
/// * `sweep_cache` — mutable reference to `derived.precipitation_sweep_order`.
pub fn run_v3_sweep(
    z: &ScalarField2D<f32>,
    dist_to_coast: &ScalarField2D<f32>,
    wind: Vec2,
    q_0: f32,
    tau_c: f32,
    tau_f: f32,
    sweep_cache: &mut Option<Vec<usize>>,
) -> ScalarField2D<f32> {
    let w = z.width;
    let h = z.height;
    let n = (w * h) as usize;

    // ── Step 1: build or reuse sweep order ─────────────────────────────────
    // Wind phase of cell (ix, iy): `-wind · (ix, iy)`.  Ascending sort puts
    // the most-upwind cells first (largest negative dot-product → smallest
    // scalar), which is equivalent to "cells the air parcel will visit first".
    let order: &[usize] = sweep_cache.get_or_insert_with(|| {
        let mut indices: Vec<usize> = (0..n).collect();
        // Stable sort is required for determinism when two cells have
        // identical wind phases (e.g. exactly axis-aligned wind and a grid
        // row/column).  Cost: O(n log n), paid once per wind direction.
        indices.sort_by(|&a, &b| {
            let ax = (a % w as usize) as f32;
            let ay = (a / w as usize) as f32;
            let bx = (b % w as usize) as f32;
            let by = (b / w as usize) as f32;
            let pa = -wind.x * ax - wind.y * ay;
            let pb = -wind.x * bx - wind.y * by;
            pa.partial_cmp(&pb).unwrap_or(std::cmp::Ordering::Equal)
        });
        indices
    });

    // ── Step 2: helper closures ─────────────────────────────────────────────

    // Compute the upstream neighbour index for cell `idx` by stepping one
    // unit along `+wind` (the direction of air origin). Clamped to grid.
    //
    // **Stencil approximation:** this is a nearest-neighbour upwind lookup
    // (the `.round()` at `ux`/`uy` below), not a bilinear sample of the
    // continuous upwind ray. Causality is correct because the sweep order
    // (stable sort by `-wind·p_position` at Step 1) guarantees every cell's
    // upwind neighbour is processed first — regardless of how precise the
    // per-cell upwind lookup is. For wind directions that round to a
    // diagonal stencil (e.g. 30° off-axis → `(+1, +1)`), the `q_upwind`
    // read is a discretised proxy. A bilinear sample would tighten it; the
    // existing V2 raymarch path already uses `sample_nearest_cell` with
    // similar rounding, so the approximation is consistent across variants.
    // Tuning this to bilinear is a Sprint 3.10 calibration decision per
    // reviewer I1, not a 3.4 correctness issue.
    let upwind_idx = |idx: usize| -> Option<usize> {
        let ix = (idx % w as usize) as f32;
        let iy = (idx / w as usize) as f32;
        let ux = (ix + wind.x).round();
        let uy = (iy + wind.y).round();
        if ux < 0.0 || uy < 0.0 || ux >= w as f32 || uy >= h as f32 {
            None // off-grid boundary → use q_0 as boundary condition
        } else {
            Some((uy as usize) * w as usize + ux as usize)
        }
    };

    // Marine recharge at cell `idx`.
    let recharge = |idx: usize| -> f32 {
        let ix = (idx % w as usize) as u32;
        let iy = (idx / w as usize) as u32;
        let d = dist_to_coast.get(ix, iy);
        MARINE_RECHARGE_STRENGTH * (-d * MARINE_RECHARGE_DECAY).exp()
    };

    // Advance `q[idx]` one cell using the previous q state of its upwind
    // neighbour.
    let advance_q = |q_upwind: f32, idx: usize, tau_c: f32, tau_f: f32| -> f32 {
        let ix = (idx % w as usize) as u32;
        let iy = (idx / w as usize) as u32;
        let grad = grad_scalar_at(z, ix, iy);
        let uplift_factor = signed_uplift(wind, grad).max(0.0) * UPLIFT_GAIN;
        let condensation_decay = (-CONDENSATION_DT / tau_c * uplift_factor).exp();
        let fallout_decay = (-CONDENSATION_DT / tau_f).exp();
        q_upwind * condensation_decay * fallout_decay + recharge(idx)
    };

    // ── Step 3: preheat — throwaway sweeps ─────────────────────────────────
    // Two passes are run with a scratch buffer to warm the q field and
    // eliminate the "q=0 at first column" startup transient that occurs when
    // wind is nearly axis-aligned and only one boundary row feeds the domain.
    let mut q_scratch = vec![0.0_f32; n];

    for _pass in 0..PREHEAT_SWEEPS {
        // Reset boundary cells to q_0.
        for &idx in order.iter() {
            let q_up = match upwind_idx(idx) {
                None => q_0,
                Some(up_idx) => q_scratch[up_idx],
            };
            q_scratch[idx] = advance_q(q_up, idx, tau_c, tau_f);
        }
    }

    // ── Step 4: main integration ────────────────────────────────────────────
    // For each cell in sweep order, compute P(p) = (q_before - q(p)) / dt.
    let mut q_main = q_scratch; // reuse preheat final state as initial condition
    let mut raw = ScalarField2D::<f32>::new(w, h);

    for &idx in order.iter() {
        let q_before = match upwind_idx(idx) {
            None => q_0,
            Some(up_idx) => q_main[up_idx],
        };
        let q_after = advance_q(q_before, idx, tau_c, tau_f);
        q_main[idx] = q_after;
        let precip = (q_before - q_after) / CONDENSATION_DT;
        let ix = (idx % w as usize) as u32;
        let iy = (idx / w as usize) as u32;
        // Clamp: near-coast cells where `marine_recharge(idx)` injects
        // fresh vapour can make `q_after > q_before`, producing a
        // negative `P`. Silently floor at 0 — the `[0, 1]` normalised
        // output contract demands non-negative precipitation, and the
        // recharge injection is a source term for the Qs system not a
        // reverse-advection of "already-fallen rain". Task 3.9's
        // precipitation_mass_balance invariant tests the aggregate
        // budget, not per-cell sign.
        raw.set(ix, iy, precip.max(0.0));
    }

    // ── Step 5: normalise to [0, 1] ─────────────────────────────────────────
    let max_val = raw.data.iter().cloned().fold(f32::EPSILON, f32::max);

    for v in raw.data.iter_mut() {
        *v = (*v / max_val).clamp(0.0, 1.0);
    }

    raw
}

// ── tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use island_core::field::{MaskField2D, ScalarField2D};
    use island_core::preset::{
        ClimateParams, IslandAge, IslandArchetypePreset, PrecipitationVariant,
    };
    use island_core::seed::Seed;
    use island_core::world::{CoastMask, Resolution, WorldState};

    use crate::StageId;
    use crate::climate::common::{compute_distance_to_mask, wind_unit};
    use crate::climate::precipitation::PrecipitationStage;
    use crate::invalidation::invalidate_from;
    use island_core::pipeline::SimulationStage;

    // ── shared test helpers ───────────────────────────────────────────────────

    fn preset_with_variant(wind_dir: f32, variant: PrecipitationVariant) -> IslandArchetypePreset {
        IslandArchetypePreset {
            name: "precip_test".into(),
            island_radius: 0.5,
            max_relief: 1.0,
            volcanic_center_count: 1,
            island_age: IslandAge::Young,
            prevailing_wind_dir: wind_dir,
            marine_moisture_strength: 1.0,
            sea_level: 0.0,
            erosion: Default::default(),
            climate: ClimateParams {
                precipitation_variant: variant,
                ..ClimateParams::default()
            },
        }
    }

    fn preset_v3(wind_dir: f32) -> IslandArchetypePreset {
        preset_with_variant(wind_dir, PrecipitationVariant::V3Lfpm)
    }

    fn preset_v2(wind_dir: f32) -> IslandArchetypePreset {
        preset_with_variant(wind_dir, PrecipitationVariant::V2Raymarch)
    }

    /// Build a world with a N-S ridge at `cx`. Wind from east (angle 0.0).
    fn world_with_ns_ridge(w: u32, h: u32, cx: u32, preset: IslandArchetypePreset) -> WorldState {
        let mut world = WorldState::new(Seed(0), preset, Resolution::new(w, h));

        let mut z = ScalarField2D::<f32>::new(w, h);
        for iy in 0..h {
            for ix in 0..w {
                let dx = (ix as i32 - cx as i32).unsigned_abs();
                let height = (8_u32.saturating_sub(dx)) as f32 * 0.1;
                z.set(ix, iy, height);
            }
        }
        world.derived.z_filled = Some(z);

        let mut is_land = MaskField2D::new(w, h);
        is_land.data.fill(1);
        let mut is_coast = MaskField2D::new(w, h);
        for iy in 0..h {
            is_coast.set(0, iy, 1);
            is_coast.set(w - 1, iy, 1);
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

    // ── Test 1: windward/leeward ratio ────────────────────────────────────────

    /// V3 LFPM must produce windward mean > leeward mean * 1.2 on a synthetic
    /// N-S ridge (wind from east, ridge at mid-domain). §10 acceptance requires
    /// > 1.2 across all 5 archetypes; this synthetic test covers the physics.
    #[test]
    fn v3_lfpm_preserves_windward_leeward_ratio() {
        let (w, h) = (64_u32, 32_u32);
        let ridge_x = 32_u32;
        let mut world = world_with_ns_ridge(w, h, ridge_x, preset_v3(0.0));

        PrecipitationStage.run(&mut world).expect("stage failed");
        let p = world.baked.precipitation.as_ref().unwrap();

        let mut windward = 0.0_f32;
        let mut leeward = 0.0_f32;
        // Sample 4 cells on each flank, mid-height row.
        for ix in 0..4 {
            windward += p.get(ridge_x + 2 + ix, h / 2);
            leeward += p.get(ridge_x - 5 - ix, h / 2);
        }
        windward /= 4.0;
        leeward /= 4.0;

        assert!(
            windward > leeward * 1.2,
            "V3 windward {windward} must exceed leeward {leeward} by 20%"
        );
    }

    // ── Test 2: all-finite output ─────────────────────────────────────────────

    /// V3 must never emit NaN or Inf — catches exp(-dt/tau) overflow at extreme slopes.
    #[test]
    fn v3_lfpm_mass_is_finite_no_nan() {
        let (w, h) = (32_u32, 32_u32);
        let mut world = world_with_ns_ridge(w, h, 16, preset_v3(0.0));
        PrecipitationStage.run(&mut world).expect("stage failed");
        let p = world.baked.precipitation.as_ref().unwrap();
        for &v in &p.data {
            assert!(v.is_finite(), "V3 produced non-finite precipitation: {v}");
        }
    }

    // ── Test 3: sweep order is cached after first run ─────────────────────────

    /// After the first V3 run, `derived.precipitation_sweep_order` must be `Some`.
    #[test]
    fn v3_lfpm_sweep_order_is_cached() {
        let (w, h) = (32_u32, 32_u32);
        let mut world = world_with_ns_ridge(w, h, 16, preset_v3(0.0));

        // First run — cache is None.
        assert!(
            world.derived.precipitation_sweep_order.is_none(),
            "cache must start as None"
        );
        PrecipitationStage.run(&mut world).expect("first run");
        assert!(
            world.derived.precipitation_sweep_order.is_some(),
            "cache must be Some after first V3 run"
        );

        // Capture the order.
        let order_first = world
            .derived
            .precipitation_sweep_order
            .as_ref()
            .unwrap()
            .clone();

        // Second run at the same wind direction — order contents unchanged.
        PrecipitationStage.run(&mut world).expect("second run");
        let order_second = world.derived.precipitation_sweep_order.as_ref().unwrap();
        assert_eq!(
            &order_first, order_second,
            "sweep order contents must be identical across runs at same wind direction"
        );
    }

    // ── Test 4: sweep order invalidates on wind change ────────────────────────

    /// After a V3 run, changing `prevailing_wind_dir` + `invalidate_from(Precipitation)`
    /// must produce a different sweep order on rerun.
    #[test]
    fn v3_lfpm_sweep_order_invalidates_on_wind_change() {
        let (w, h) = (32_u32, 32_u32);
        let mut world = world_with_ns_ridge(w, h, 16, preset_v3(0.0));

        // First run — wind from east.
        PrecipitationStage.run(&mut world).expect("first run");
        let order_east = world
            .derived
            .precipitation_sweep_order
            .as_ref()
            .unwrap()
            .clone();

        // Change wind to north (PI/2) and invalidate.
        world.preset.prevailing_wind_dir = std::f32::consts::FRAC_PI_2;
        invalidate_from(&mut world, StageId::Precipitation);
        assert!(
            world.derived.precipitation_sweep_order.is_none(),
            "sweep order must be None after invalidate_from(Precipitation)"
        );

        // Rerun — new order must differ from the east-wind order.
        PrecipitationStage
            .run(&mut world)
            .expect("second run after wind change");
        let order_north = world.derived.precipitation_sweep_order.as_ref().unwrap();
        assert_ne!(
            &order_east, order_north,
            "sweep order must change when wind direction changes"
        );
    }

    // ── Test 5: Precipitation invalidation arm clears sweep order ─────────────

    /// Unit-tests the `Precipitation` arm of `clear_stage_outputs` directly.
    #[test]
    fn precipitation_sweep_order_is_cleared_by_precipitation_invalidation_arm() {
        let (w, h) = (16_u32, 16_u32);
        let mut world = world_with_ns_ridge(w, h, 8, preset_v3(0.0));

        PrecipitationStage.run(&mut world).expect("run");
        assert!(world.derived.precipitation_sweep_order.is_some());

        invalidate_from(&mut world, StageId::Precipitation);

        assert!(
            world.derived.precipitation_sweep_order.is_none(),
            "precipitation_sweep_order must be None after invalidate_from(Precipitation)"
        );
    }

    // ── Test 6: V2 raymarch is deterministic across repeated runs ────────────

    /// Construct a world, run PrecipitationStage with V2Raymarch twice, and
    /// verify the resulting `baked.precipitation` data is identical.
    /// This is a determinism lock, not a cross-version comparison.
    #[test]
    fn v2_raymarch_is_deterministic_across_repeated_runs() {
        let (w, h) = (32_u32, 32_u32);
        let mut w1 = world_with_ns_ridge(w, h, 16, preset_v2(0.0));
        let mut w2 = world_with_ns_ridge(w, h, 16, preset_v2(0.0));

        PrecipitationStage.run(&mut w1).expect("run1");
        PrecipitationStage.run(&mut w2).expect("run2");

        assert_eq!(
            w1.baked.precipitation.as_ref().unwrap().data,
            w2.baked.precipitation.as_ref().unwrap().data,
            "V2Raymarch must be bit-exact across two identical runs"
        );
    }

    // ── Test 7: preheat prevents zero windward transient ─────────────────────

    /// Wind from east (angle 0): upwind boundary is ix=w-1.  Two preheat
    /// sweeps warm the q field before the main integration.  The purpose is
    /// to ensure that cells near the upwind boundary (ix near w-1) start
    /// with a non-zero q estimate so the first main pass does not see a cold
    /// q=0 state on any interior cell.
    ///
    /// Observable consequence: the windward face of the N-S ridge (ix just
    /// east of ridge_x) receives non-trivial precipitation.
    ///
    /// **Honest scope note (reviewer I2):** this test asserts
    /// `windward_sum > 0`, which already passes even with `PREHEAT_SWEEPS =
    /// 0` because windward cells near the upwind boundary see `q_upwind =
    /// q_0` directly via the off-grid boundary path. Strengthening to a
    /// cold-vs-warm start comparison would require a test-only
    /// `PREHEAT_SWEEPS_OVERRIDE` knob on `run_v3_sweep`. Renamed to
    /// reflect what this test actually proves (ridge geometry produces
    /// nonzero precipitation) rather than what it claims (preheat is
    /// necessary). Cold-start regression coverage remains a Sprint 3.10
    /// calibration-suite concern.
    #[test]
    fn v3_lfpm_windward_ridge_produces_nonzero_precipitation() {
        let (w, h) = (32_u32, 32_u32);
        let ridge_x = 16_u32;
        let mut world = world_with_ns_ridge(w, h, ridge_x, preset_v3(0.0));

        PrecipitationStage.run(&mut world).expect("stage");
        let p = world.baked.precipitation.as_ref().unwrap();

        // With east wind, the windward face is east of the ridge (ix > ridge_x).
        // Sum a 4-cell wide window east of the ridge at mid-height.
        let windward_sum: f32 = (0..4).map(|dx| p.get(ridge_x + 1 + dx, h / 2)).sum::<f32>();
        // The windward face must have accumulated some precipitation; without
        // preheat (cold q=0 start) the sweep would process these high-ix cells
        // first with q_upwind=q_0, giving correct values regardless — so
        // PREHEAT_SWEEPS=2 is a defense in depth for near-axis-aligned winds
        // where the boundary might wrap across rows. The invariant here is that
        // orographic uplift on the windward ridge face registers as non-zero
        // precipitation (the actual functionality test, not just preheat).
        assert!(
            windward_sum > 0.0,
            "windward ridge cells have zero precipitation (preheat or physics failure, sum={windward_sum})"
        );
    }

    // ── Test 8: V3 output normalised ─────────────────────────────────────────

    /// Output is in [0, 1] and max > 0 (domain has some precipitation).
    #[test]
    fn v3_lfpm_output_is_normalized() {
        let (w, h) = (32_u32, 32_u32);
        let mut world = world_with_ns_ridge(w, h, 16, preset_v3(0.0));
        PrecipitationStage.run(&mut world).expect("stage");
        let p = world.baked.precipitation.as_ref().unwrap();
        let stats = p.stats().expect("non-empty");
        assert!(stats.min >= 0.0, "min must be >= 0");
        assert!(stats.max <= 1.0 + 1e-6, "max must be <= 1");
        assert!(stats.max > 0.1, "max must be significantly above 0");
    }

    // ── Test 9: direct V3 helper — run_v3_sweep smoke test ───────────────────

    /// Calls `run_v3_sweep` directly to verify the function signature and
    /// that the cache is populated.
    #[test]
    fn run_v3_sweep_smoke_test() {
        let (w, h) = (16_u32, 16_u32);
        let mut z = ScalarField2D::<f32>::new(w, h);
        // Simple ramp: z = x / w.
        for iy in 0..h {
            for ix in 0..w {
                z.set(ix, iy, ix as f32 / w as f32);
            }
        }
        let mut coast = MaskField2D::new(w, h);
        for iy in 0..h {
            coast.set(0, iy, 1);
        }
        let dist = compute_distance_to_mask(&coast, w, h);
        let wind = wind_unit(0.0); // from east

        let mut cache: Option<Vec<usize>> = None;

        let result = run_v3_sweep(
            &z,
            &dist,
            wind,
            Q_0_DEFAULT,
            TAU_C_DEFAULT,
            TAU_F_DEFAULT,
            &mut cache,
        );

        // Cache populated after first call.
        assert!(
            cache.is_some(),
            "sweep cache must be populated after run_v3_sweep"
        );
        assert_eq!(result.width, w);
        assert_eq!(result.height, h);
        for &v in &result.data {
            assert!(v.is_finite(), "run_v3_sweep produced non-finite: {v}");
        }

        // Second call reuses cache (same result).
        let result2 = run_v3_sweep(
            &z,
            &dist,
            wind,
            Q_0_DEFAULT,
            TAU_C_DEFAULT,
            TAU_F_DEFAULT,
            &mut cache,
        );
        assert_eq!(
            result.data, result2.data,
            "result must be deterministic across two calls"
        );
    }
}
