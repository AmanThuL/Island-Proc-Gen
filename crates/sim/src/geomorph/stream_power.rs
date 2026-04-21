//! Stream Power Incision Model — Sprint 2 DD1 + Sprint 3 DD2.
//!
//! Applies one SPIM iteration to `authoritative.height` in place for every
//! land cell.  Two variants are selected via
//! [`island_core::preset::SpimVariant`]:
//!
//! * `Plain` (Sprint 2) — `E_f = K · A^m · S^n` (Whipple & Tucker 1999;
//!   KP17 §3.1). Mutates height only; does not touch `authoritative.sediment`.
//! * `SpaceLite` (Sprint 3, default) — dual equation:
//!   - `E_bed = K_bed · A^m · S^n · exp(-hs / H*)` mutates height;
//!   - `E_sed = K_sed · A^m · S^n · min(hs, HS_ENTRAIN_MAX)` controls
//!     sediment-layer entrainment.
//!
//!   `hs` is updated in-place: `hs_new = clamp(hs + E_bed·dt − E_sed·dt,
//!   0, 1)`. Task 3.3 adds the deposition term `+ Qs_in/A_cell · dt`.
//!
//! Parameters are read from `world.preset.erosion` at run time so slider
//! changes take effect on the next re-run.
//!
//! This stage is in-place-on-height (and, under `SpaceLite`, also
//! in-place-on-sediment). It does **not** update any `derived.*` field —
//! `ErosionOuterLoop` (Task 2.3) handles cache invalidation and
//! flow-network rebuilds around repeated SPIM calls.

use island_core::pipeline::SimulationStage;
use island_core::preset::SpimVariant;
use island_core::world::WorldState;

use crate::geomorph::sediment::{H_STAR, HS_ENTRAIN_MAX};

/// Sprint 2 DD1 + Sprint 3 DD2: Stream Power Incision Model (SPIM).
///
/// Dispatches to one of two inner routines based on
/// `world.preset.erosion.spim_variant`:
///
/// * [`SpimVariant::Plain`] — Sprint 2 single-equation fallback:
///   `E_f = K · A^m · S^n`, with `(m, n) = (0.35, 1.0)` locked in v1 to
///   avoid the KP17 pathological `m/n = 0.5` regime.
/// * [`SpimVariant::SpaceLite`] — Sprint 3 dual equation (default), see
///   [`run_space_lite`].
///
/// Both branches mutate `authoritative.height` in place for every land
/// cell and clamp new height at `sea_level` to prevent negative /
/// below-sea heights that would produce NaN slopes downstream. The
/// `SpaceLite` branch additionally mutates `authoritative.sediment` in
/// place (clamped to `[0, 1]`).
///
/// Unit struct — all params read from `world.preset.erosion` at run time
/// so mid-session slider changes take effect on the next rerun.
pub struct StreamPowerIncisionStage;

impl SimulationStage for StreamPowerIncisionStage {
    fn name(&self) -> &'static str {
        "stream_power_incision"
    }

    fn run(&self, world: &mut WorldState) -> anyhow::Result<()> {
        // ── prerequisite checks ───────────────────────────────────────────────
        // Shared across both branches. Kept here (not in per-branch helpers)
        // so the bail-out message cites the stage name consistently.
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
        if world.authoritative.height.is_none() {
            anyhow::bail!(
                "StreamPowerIncisionStage: authoritative.height is None \
                 (TopographyStage must run first)"
            );
        }

        match world.preset.erosion.spim_variant {
            SpimVariant::Plain => run_plain(world),
            SpimVariant::SpaceLite => run_space_lite(world),
        }
    }
}

/// Raw stream-power kernel `K · A^m · S^n`, with the non-finite guard that
/// protects against `f32::MAX`-magnitude accumulations combined with
/// `m > 1` parameter overrides. Returns `0.0` whenever the product is
/// non-finite — same defensive behaviour as Sprint 2.
#[inline]
fn stream_power_kernel(k: f32, a: f32, s: f32, m: f32, n: f32) -> f32 {
    let ef = k * a.powf(m) * s.powf(n);
    if ef.is_finite() { ef } else { 0.0 }
}

/// Sprint 2 DD1 — single-equation SPIM (`SpimVariant::Plain`).
///
/// Preserved verbatim for baseline regeneration (Task 3.10's `pre_*`
/// shots rely on `preset_override.erosion.spim_variant = Some(Plain)`)
/// and for Sprint 3 ablations against the old physics.
fn run_plain(world: &mut WorldState) -> anyhow::Result<()> {
    let k = world.preset.erosion.spim_k;
    let m = world.preset.erosion.spim_m;
    let n = world.preset.erosion.spim_n;
    let sea_level = world.preset.sea_level;

    // Split borrow: `world.derived` and `world.authoritative` are disjoint
    // struct fields so the compiler accepts shared refs into `derived` held
    // simultaneously with the `&mut` into `authoritative.height`.
    let n_cells = world.resolution.sim_width as usize * world.resolution.sim_height as usize;

    let accumulation = &world.derived.accumulation.as_ref().unwrap().data;
    let slope = &world.derived.slope.as_ref().unwrap().data;
    let is_land = &world.derived.coast_mask.as_ref().unwrap().is_land.data;
    let h_field = world.authoritative.height.as_mut().unwrap();

    for i in 0..n_cells {
        if is_land[i] == 0 {
            continue;
        }
        let ef = stream_power_kernel(k, accumulation[i], slope[i], m, n);
        // In-place incision; clamp at sea_level (dt = 1.0 in v1).
        h_field.data[i] = (h_field.data[i] - ef).max(sea_level);
    }

    Ok(())
}

/// Sprint 3 DD2 — SPACE-lite dual-equation SPIM (`SpimVariant::SpaceLite`).
///
/// For each land cell `p`:
///
/// ```text
/// A  = derived.accumulation[p]
/// S  = derived.slope[p]
/// hs = authoritative.sediment[p]
///
/// E_bed  = K_bed · A^m · S^n · exp(-hs / H*)
/// hs_eff = min(hs, HS_ENTRAIN_MAX)
/// E_sed  = K_sed · A^m · S^n · hs_eff
///
/// z[p]  -= E_bed · dt;  z[p] = max(z[p], sea_level)
/// hs[p]  = clamp(hs + E_bed·dt − E_sed·dt, 0, 1)
/// ```
///
/// `dt = 1.0` in v1 (matches Sprint 2). Task 3.3 will add a deposition
/// term `+ Qs_in/A_cell · dt` into the `hs` update, routed from
/// `SedimentUpdateStage`.
///
/// Non-finite guard semantics match `run_plain`: if any intermediate
/// product overflows (e.g. `A = f32::MAX` combined with a parameter
/// override `m > 1`), the corresponding flux is clamped to `0.0` and
/// neither `z` nor `hs` is corrupted.
///
/// # Prerequisite: `authoritative.sediment` must be `Some`.
///
/// Task 3.1 sets `hs_init(p) = 0.1 · is_land(p)` at the end of
/// `CoastMaskStage`, so by the time `ErosionOuterLoop` runs SPIM the
/// sediment field is always populated. A missing sediment field here is a
/// bug (Coastal stage didn't run, or `invalidate_from(Coastal)` was
/// called without a follow-up `run_from(Coastal)`); bail with a clear
/// message rather than silently skip.
fn run_space_lite(world: &mut WorldState) -> anyhow::Result<()> {
    if world.authoritative.sediment.is_none() {
        anyhow::bail!(
            "StreamPowerIncisionStage(SpaceLite): authoritative.sediment is None \
             (CoastMaskStage must run first — Task 3.1 sets hs_init)"
        );
    }

    let k_bed = world.preset.erosion.space_k_bed;
    let k_sed = world.preset.erosion.space_k_sed;
    let h_star = world.preset.erosion.h_star;
    let m = world.preset.erosion.spim_m;
    let n = world.preset.erosion.spim_n;
    let sea_level = world.preset.sea_level;

    // Guard against a pathological h_star ≤ 0 from a misconfigured preset.
    // exp(-hs / 0) is undefined; fall back to the locked default rather
    // than propagating NaN through every land cell.
    let h_star = if h_star > 0.0 { h_star } else { H_STAR };

    let n_cells = world.resolution.sim_width as usize * world.resolution.sim_height as usize;

    // Triple split borrow: derived is shared (&), authoritative.height and
    // authoritative.sediment are both &mut, but since they're distinct
    // struct fields the compiler accepts the split via destructuring.
    let accumulation = &world.derived.accumulation.as_ref().unwrap().data;
    let slope = &world.derived.slope.as_ref().unwrap().data;
    let is_land = &world.derived.coast_mask.as_ref().unwrap().is_land.data;

    // Split the authoritative fields into two disjoint &muts.
    let auth = &mut world.authoritative;
    let h_field = auth.height.as_mut().unwrap();
    let hs_field = auth.sediment.as_mut().unwrap();

    debug_assert_eq!(
        h_field.data.len(),
        hs_field.data.len(),
        "authoritative.height and authoritative.sediment must share resolution"
    );

    for i in 0..n_cells {
        if is_land[i] == 0 {
            continue;
        }

        let a = accumulation[i];
        let s = slope[i];
        let hs = hs_field.data[i];

        // Shielded bedrock incision: exp(-hs / H*) ∈ (0, 1] for hs ≥ 0.
        let shield = (-hs / h_star).exp();
        let e_bed = stream_power_kernel(k_bed, a, s, m, n) * shield;
        let e_bed = if e_bed.is_finite() { e_bed } else { 0.0 };

        // Sediment entrainment, capped on hs_eff so thick piles don't
        // produce unbounded stripping flux.
        let hs_eff = hs.min(HS_ENTRAIN_MAX);
        let e_sed = stream_power_kernel(k_sed, a, s, m, n) * hs_eff;
        let e_sed = if e_sed.is_finite() { e_sed } else { 0.0 };

        // dt = 1.0 in v1.
        h_field.data[i] = (h_field.data[i] - e_bed).max(sea_level);
        // Sediment mass balance: bedrock erosion sheds into hs, entrainment
        // strips hs. Task 3.3 adds + Qs_in/A_cell for deposition.
        hs_field.data[i] = (hs + e_bed - e_sed).clamp(0.0, 1.0);
    }

    Ok(())
}

// ─── tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use island_core::field::{MaskField2D, ScalarField2D};
    use island_core::pipeline::SimulationStage;
    use island_core::preset::{ErosionParams, IslandAge, IslandArchetypePreset, SpimVariant};
    use island_core::seed::Seed;
    use island_core::world::{CoastMask, Resolution, WorldState};

    use super::StreamPowerIncisionStage;
    use super::{H_STAR, HS_ENTRAIN_MAX};

    // ── helpers ───────────────────────────────────────────────────────────────

    /// Shared test preset: pick the SPIM variant, everything else is a fixed
    /// neutral fixture. Sprint 2 tests pass `Plain` to keep their Sprint 2
    /// numerical invariants (which rely on `spim_k` / `spim_m` / `spim_n`)
    /// unaffected by Sprint 3's SPACE-lite default; Sprint 3 tests pass
    /// `SpaceLite`.
    fn preset_for(sea_level: f32, variant: SpimVariant) -> IslandArchetypePreset {
        IslandArchetypePreset {
            name: "spim_test".into(),
            island_radius: 0.5,
            max_relief: 0.8,
            volcanic_center_count: 1,
            island_age: IslandAge::Young,
            prevailing_wind_dir: 0.0,
            marine_moisture_strength: 0.5,
            sea_level,
            erosion: ErosionParams {
                spim_variant: variant,
                ..ErosionParams::default()
            },
            climate: Default::default(),
        }
    }

    fn base_preset(sea_level: f32) -> IslandArchetypePreset {
        preset_for(sea_level, SpimVariant::Plain)
    }

    fn space_lite_preset(sea_level: f32) -> IslandArchetypePreset {
        preset_for(sea_level, SpimVariant::SpaceLite)
    }

    fn preset_with_k(sea_level: f32, spim_k: f32) -> IslandArchetypePreset {
        IslandArchetypePreset {
            erosion: ErosionParams {
                spim_k,
                spim_variant: SpimVariant::Plain,
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

    /// Build a world with the three required derived fields pre-set. Also
    /// seeds `authoritative.sediment = 0.0` everywhere so SPACE-lite tests
    /// can override specific cells before the run; Plain tests ignore it.
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

        // Sediment field starts at 0.0 — Sprint 2 Plain tests don't care;
        // Sprint 3 SPACE-lite tests can overwrite specific cells.
        world.authoritative.sediment = Some(ScalarField2D::<f32>::new(w, h));

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

    // ─── Sprint 3 DD2: SPACE-lite dual-equation tests ────────────────────────

    /// Helper: run a full Plain world + a full SPACE-lite world with `hs`
    /// identically zero on all land, identical everything else, one SPIM
    /// step, compare resulting heights.
    ///
    /// With `hs = 0`:
    /// * `exp(-0/H*) = 1`, so SPACE-lite's bedrock incision reduces to
    ///   `K_bed · A^m · S^n`.
    /// * Plain's incision is `K · A^m · S^n`.
    ///
    /// When `K_bed == K`, the two height updates are bit-identical. We
    /// pin `Plain.spim_k` to `space_k_bed` so the forms collapse to the
    /// same numeric path, isolating the physical reduction from the
    /// constant difference.
    #[test]
    fn space_lite_reduces_to_plain_when_hs_is_zero() {
        let (w, h) = (8u32, 8u32);
        let sea_level = 0.0;
        // Build a SPACE-lite preset with default SPACE-lite constants …
        let space_preset = space_lite_preset(sea_level);
        let k_bed = space_preset.erosion.space_k_bed;

        // … and a Plain preset whose `spim_k == space_k_bed` so the kernel
        // `K · A^m · S^n` is numerically identical.
        let plain_preset = IslandArchetypePreset {
            erosion: ErosionParams {
                spim_variant: SpimVariant::Plain,
                spim_k: k_bed,
                ..ErosionParams::default()
            },
            ..space_lite_preset(sea_level)
        };

        let mut space_world = make_world(w, h, space_preset, 0.5, 0.1, 1.0, all_land_coast(w, h));
        // `hs = 0` everywhere (make_world already does this).
        let mut plain_world = make_world(w, h, plain_preset, 0.5, 0.1, 1.0, all_land_coast(w, h));

        StreamPowerIncisionStage
            .run(&mut space_world)
            .expect("space-lite run");
        StreamPowerIncisionStage
            .run(&mut plain_world)
            .expect("plain run");

        let h_space = &space_world.authoritative.height.as_ref().unwrap().data;
        let h_plain = &plain_world.authoritative.height.as_ref().unwrap().data;

        // Both branches compute `K · A^m · S^n` identically when hs = 0,
        // so heights should be bit-exact. `f32::EPSILON · max(K_bed, K)`
        // is the theoretical bound per the task brief; give a tiny safety
        // margin for compiler reordering.
        let tol = f32::EPSILON * k_bed.max(plain_world.preset.erosion.spim_k) * 8.0;
        for i in 0..(w * h) as usize {
            let d = (h_space[i] - h_plain[i]).abs();
            assert!(
                d <= tol,
                "cell {i}: space-lite height {} != plain height {}, \
                 delta={d}, tol={tol}",
                h_space[i],
                h_plain[i]
            );
        }
    }

    /// SPACE-lite must keep `hs ∈ [0, 1]` after one inner step, with no
    /// NaN / Inf on any land cell. Uses a mid-range hs to exercise both
    /// the `e_bed` deposition into hs and the `e_sed` entrainment out of
    /// hs.
    #[test]
    fn space_lite_respects_hs_bounds() {
        let (w, h) = (8u32, 8u32);
        let preset = space_lite_preset(0.0);
        let mut world = make_world(w, h, preset, 0.5, 0.2, 2.0, all_land_coast(w, h));

        // Seed hs with a pattern crossing the HS_ENTRAIN_MAX boundary.
        {
            let hs = world.authoritative.sediment.as_mut().unwrap();
            for (i, v) in hs.data.iter_mut().enumerate() {
                // 0.0, 0.1, 0.25, 0.5, 0.75, … wrapping modulo 8
                *v = (i as f32 * 0.125) % 1.0;
            }
        }

        StreamPowerIncisionStage
            .run(&mut world)
            .expect("space-lite run");

        let hs_after = world.authoritative.sediment.as_ref().unwrap();
        for (i, &v) in hs_after.data.iter().enumerate() {
            assert!(
                v.is_finite(),
                "cell {i}: hs non-finite after SPACE-lite: {v}"
            );
            assert!(
                (0.0..=1.0).contains(&v),
                "cell {i}: hs out of [0,1] after SPACE-lite: {v}"
            );
        }
    }

    /// With `K_sed = 0.0` the entrainment term vanishes, so the `hs`
    /// update reduces to `hs += E_bed · dt`. Bedrock incision always
    /// yields `E_bed ≥ 0` on land, so `hs` monotonically increases
    /// (until the upper clamp at 1.0 bites).
    ///
    /// Uses a small `hs_init` well below the clamp, a single step, and
    /// checks every land cell strictly increased by roughly `E_bed` (the
    /// exact `E_bed` is `K_bed · 1 · 0.1 · exp(-0.1/H*)`).
    #[test]
    fn space_lite_thickens_sediment_on_bedrock_incision_when_entrainment_is_weak() {
        let (w, h) = (8u32, 8u32);
        let mut preset = space_lite_preset(0.0);
        preset.erosion.space_k_sed = 0.0; // disable entrainment
        let mut world = make_world(w, h, preset, 0.5, 0.1, 1.0, all_land_coast(w, h));

        // Uniform small hs so the e_bed shield `exp(-0.1/0.05)` is strong
        // but the cells are well below the 1.0 upper clamp.
        world
            .authoritative
            .sediment
            .as_mut()
            .unwrap()
            .data
            .fill(0.1);

        let hs_before: Vec<f32> = world.authoritative.sediment.as_ref().unwrap().data.clone();

        StreamPowerIncisionStage
            .run(&mut world)
            .expect("space-lite run");

        let hs_after = world.authoritative.sediment.as_ref().unwrap();
        for i in 0..(w * h) as usize {
            assert!(
                hs_after.data[i] > hs_before[i],
                "cell {i}: hs must strictly increase (no entrainment): \
                 before={}, after={}",
                hs_before[i],
                hs_after.data[i]
            );
            assert!(
                hs_after.data[i] <= 1.0,
                "cell {i}: hs exceeded 1.0 clamp: {}",
                hs_after.data[i]
            );
        }
    }

    /// Determinism lock: running Plain twice on a fresh identical world
    /// must produce a byte-identical height field. This is the
    /// Sprint-2-bit-exact property that Task 3.10's baseline
    /// regeneration relies on.
    ///
    /// If Sprint 2 snapshot infrastructure were in-crate we'd compare a
    /// stored blake3 against the current output; in its absence the
    /// "run twice, same bytes" form is the strongest local check.
    #[test]
    fn plain_branch_is_deterministic_across_repeated_runs() {
        let (w, h) = (8u32, 8u32);
        let preset = base_preset(0.0); // Plain variant

        let mut world_a = make_world(w, h, preset.clone(), 0.5, 0.1, 1.0, all_land_coast(w, h));
        let mut world_b = make_world(w, h, preset, 0.5, 0.1, 1.0, all_land_coast(w, h));

        StreamPowerIncisionStage.run(&mut world_a).expect("run A");
        StreamPowerIncisionStage.run(&mut world_b).expect("run B");

        let h_a = &world_a.authoritative.height.as_ref().unwrap().data;
        let h_b = &world_b.authoritative.height.as_ref().unwrap().data;

        // Byte-for-byte identical, not just ε-close.
        for i in 0..(w * h) as usize {
            assert_eq!(
                h_a[i].to_bits(),
                h_b[i].to_bits(),
                "cell {i}: plain SPIM not deterministic ({} vs {})",
                h_a[i],
                h_b[i]
            );
        }
    }

    /// SPACE-lite without sediment must bail with a clear error — the
    /// safety rail against Coastal being invalidated without a rerun.
    #[test]
    fn space_lite_missing_sediment_returns_error() {
        let (w, h) = (4u32, 4u32);
        let preset = space_lite_preset(0.0);
        let mut world = make_world(w, h, preset, 0.5, 0.1, 1.0, all_land_coast(w, h));
        world.authoritative.sediment = None;

        let result = StreamPowerIncisionStage.run(&mut world);
        assert!(
            result.is_err(),
            "SPACE-lite must error when authoritative.sediment is None"
        );
    }

    /// SPACE-lite's `e_bed` shield `exp(-hs / H*)` decays with thicker
    /// sediment. Two synthetic worlds, identical in every way except `hs`
    /// (one with `hs = 0.0`, the other with `hs = 0.5 = H_STAR · 10`),
    /// should show the thick-sediment world incising strictly less.
    /// Verifies the cover-effect physics is wired correctly (not just
    /// that `hs` is being read).
    #[test]
    fn space_lite_bedrock_shield_reduces_incision_with_thicker_sediment() {
        let (w, h) = (4u32, 4u32);
        let sea_level = 0.0;
        let preset = space_lite_preset(sea_level);

        let mut thin = make_world(w, h, preset.clone(), 0.5, 0.1, 1.0, all_land_coast(w, h));
        let mut thick = make_world(w, h, preset, 0.5, 0.1, 1.0, all_land_coast(w, h));
        // thin keeps hs = 0.0 (make_world default); thick overrides to 0.5.
        thick
            .authoritative
            .sediment
            .as_mut()
            .unwrap()
            .data
            .fill(0.5);

        StreamPowerIncisionStage.run(&mut thin).expect("thin run");
        StreamPowerIncisionStage.run(&mut thick).expect("thick run");

        let h_thin = &thin.authoritative.height.as_ref().unwrap().data;
        let h_thick = &thick.authoritative.height.as_ref().unwrap().data;
        // Thick sediment shields bedrock → thick height drops less → is
        // higher than thin height.
        for i in 0..(w * h) as usize {
            assert!(
                h_thick[i] > h_thin[i],
                "cell {i}: shielding failed — thick hs should leave higher z; \
                 h_thick={}, h_thin={}",
                h_thick[i],
                h_thin[i]
            );
        }

        // And the HS_ENTRAIN_MAX / H_STAR sanity constants are reachable
        // from this module.
        assert!(HS_ENTRAIN_MAX > 0.0);
        assert!(H_STAR > 0.0);
    }
}
