//! Sprint 4.E + 4.F parity tests — opt-in via `IPG_RUN_GPU_PARITY=1`.
//!
//! These tests verify that the GPU compute kernels produce results within the
//! per-kernel tolerances defined in DD8 of the Sprint 4 compute productization
//! spec. They require a GPU adapter that supports compute (Metal on macOS).
//!
//! # Opt-in mechanism
//!
//! All tests in this file gate on `IPG_RUN_GPU_PARITY=1` at the top. If the
//! env var is absent the test returns immediately. Additionally, all tests are
//! `#[ignore]` so `cargo test` doesn't pick them up by accident.
//!
//! Run with:
//! ```bash
//! IPG_RUN_GPU_PARITY=1 cargo test -p app --test compute_backend_parity -- --ignored
//! ```
//!
//! # DD8 hillslope parity contract (Sprint 4.E)
//!
//! For a 128² `volcanic_single` seed 42 world, after one `hillslope_diffusion`
//! call (params taken from the default preset, n_diff_substep=4):
//!
//! - `max(|h_cpu - h_gpu|) ≤ 1e-5` absolute (interior cells)
//! - `max(|h_cpu - h_gpu| / max(|h_cpu|, ε)) ≤ 1e-4` relative (interior)
//! - boundary ring cells (i/j == 0 or W-1/H-1) checked separately with the
//!   same thresholds
//! - mean height drift over 100 repeated calls ≤ 1e-6 per call (mass
//!   conservation sanity)
//!
//! DO NOT use a blanket `1e-4 * max(reference)` rule — these are explicit
//! per-kernel absolute + relative tolerances from DD8.

use std::sync::Arc;

use island_core::{
    field::ScalarField2D,
    pipeline::{ComputeBackend, ComputeOp, HillslopeParams, StreamPowerParams},
    world::{Resolution, WorldState},
};

/// Env-var guard — placed at the top of every test.
fn gpu_parity_enabled() -> bool {
    std::env::var("IPG_RUN_GPU_PARITY").as_deref() == Ok("1")
}

// ── Pipeline setup helper ─────────────────────────────────────────────────────

/// Build a `WorldState` that has been run through the canonical pipeline up to
/// (but not including) `ErosionOuterLoop` (index 8). Returns the world ready
/// to have a single hillslope diffusion dispatch applied.
///
/// Uses `volcanic_single` preset, seed 42, 128² resolution — the DD8
/// hillslope parity reference configuration.
fn build_pre_erosion_world() -> WorldState {
    let preset =
        data::presets::load_preset("volcanic_single").expect("volcanic_single preset must exist");

    let resolution = Resolution::new(128, 128);
    let mut world = WorldState::new(island_core::seed::Seed(42), preset, resolution);

    // Run stages 0 through 7 (Topography → RiverExtraction), stopping before
    // ErosionOuterLoop (index 8). This populates all prerequisites for
    // hillslope: authoritative.height, derived.coast_mask, etc.
    sim::default_pipeline()
        .run_from(&mut world, 0)
        .expect("pre-erosion pipeline run must succeed");

    // Reset height back to its post-topography state (before any erosion):
    // we want to test hillslope in isolation on the raw topography.
    // Actually, run_from(0) runs all 19 stages including erosion. We need to
    // run only stages 0..ErosionOuterLoop (0..8).
    //
    // Re-build and run_from(0) up to but not including index 8.
    let preset2 =
        data::presets::load_preset("volcanic_single").expect("volcanic_single preset must exist");
    let mut world2 = WorldState::new(island_core::seed::Seed(42), preset2, resolution);

    // Build a CPU-only pipeline that stops before ErosionOuterLoop.
    // Use StageId::ErosionOuterLoop index = 8, so we run [0..8) = stages 0-7.
    // SimulationPipeline::run runs all stages; we can use run_from(0) but stop
    // by running only the first 8 stages explicitly. Since we can't easily
    // slice the pipeline, we run the full pipeline but with a preset that has
    // n_batch=0 so ErosionOuterLoop is a no-op, then snapshot the height before
    // erosion would have mutated it.
    //
    // Easiest approach: override n_batch=0 so ErosionOuterLoop skips all work.
    world2.preset.erosion.n_batch = 0;

    sim::default_pipeline()
        .run(&mut world2)
        .expect("full pipeline with n_batch=0 must succeed");

    world2
}

// ── DD8 hillslope parity test ─────────────────────────────────────────────────

/// Sprint 4.E DD8 gate: GPU hillslope diffusion output must match the CPU
/// reference within the per-kernel tolerances specified in DD8.
///
/// # What is checked
///
/// - Interior cells: `max |h_cpu - h_gpu| ≤ 1e-5` absolute
/// - Interior cells: `max |h_cpu - h_gpu| / max(|h_cpu|, 1e-9) ≤ 1e-4` relative
/// - Boundary ring (i,j ∈ {0, W-1, H-1}): same absolute + relative thresholds
/// - 100-iteration mass conservation: mean(|Σ h|) drift ≤ 1e-6 per iteration
///
/// # Opt-in
///
/// Set `IPG_RUN_GPU_PARITY=1` and run with `--ignored`.
#[test]
#[ignore = "requires GPU adapter; set IPG_RUN_GPU_PARITY=1 and run with --ignored"]
fn compute_backend_parity_hillslope_within_tolerance() {
    if !gpu_parity_enabled() {
        eprintln!("skipped — set IPG_RUN_GPU_PARITY=1 to run GPU parity tests");
        return;
    }

    // ── Build the shared pre-erosion world ────────────────────────────────────
    let world_template = build_pre_erosion_world();
    let w = world_template.resolution.sim_width as usize;
    let h = world_template.resolution.sim_height as usize;

    let params = HillslopeParams {
        hillslope_d: world_template.preset.erosion.hillslope_d,
        n_diff_substep: world_template.preset.erosion.n_diff_substep,
    };

    // ── CPU reference ─────────────────────────────────────────────────────────
    let mut world_cpu = clone_world(&world_template);
    let cpu_backend = Arc::new(sim::compute::CpuBackend);
    cpu_backend
        .run_hillslope_diffusion(&mut world_cpu, &params)
        .expect("CPU hillslope must succeed");
    let h_cpu = world_cpu
        .authoritative
        .height
        .as_ref()
        .unwrap()
        .data
        .clone();

    // ── GPU run ───────────────────────────────────────────────────────────────
    let ctx = Arc::new(
        gpu::GpuContext::new_headless((w as u32, h as u32))
            .expect("GPU headless context required for parity test"),
    );

    // If TIMESTAMP_QUERY is not supported, the timer will be None — that's OK,
    // we still get the GPU result; we just won't have gpu_ms populated.
    let gpu_backend = gpu::GpuBackend::new(Arc::clone(&ctx));
    assert!(
        gpu_backend.supports(ComputeOp::HillslopeDiffusion),
        "GpuBackend must support HillslopeDiffusion at Sprint 4.E"
    );

    let mut world_gpu = clone_world(&world_template);
    let timing = gpu_backend
        .run_hillslope_diffusion(&mut world_gpu, &params)
        .expect("GPU hillslope must succeed");
    let h_gpu = world_gpu
        .authoritative
        .height
        .as_ref()
        .unwrap()
        .data
        .clone();

    eprintln!(
        "hillslope parity: cpu_ms={:.3}, gpu_ms={:?}",
        timing.cpu_ms, timing.gpu_ms
    );

    // ── DD8 tolerance checks ──────────────────────────────────────────────────
    // Interior cells: exclude the boundary ring (i == 0 || i == w-1 || j == 0 || j == h-1).
    const ABS_TOL: f32 = 1e-5;
    const REL_TOL: f32 = 1e-4;
    const EPS: f32 = 1e-9;

    let mut max_abs_interior: f32 = 0.0;
    let mut max_rel_interior: f32 = 0.0;
    let mut max_abs_boundary: f32 = 0.0;
    let mut max_rel_boundary: f32 = 0.0;

    for iy in 0..h {
        for ix in 0..w {
            let i = iy * w + ix;
            let diff = (h_cpu[i] - h_gpu[i]).abs();
            let rel = diff / h_cpu[i].abs().max(EPS);
            let on_boundary = ix == 0 || ix == w - 1 || iy == 0 || iy == h - 1;
            if on_boundary {
                max_abs_boundary = max_abs_boundary.max(diff);
                max_rel_boundary = max_rel_boundary.max(rel);
            } else {
                max_abs_interior = max_abs_interior.max(diff);
                max_rel_interior = max_rel_interior.max(rel);
            }
        }
    }

    eprintln!(
        "hillslope parity: max_abs_interior={:.3e}, max_rel_interior={:.3e}",
        max_abs_interior, max_rel_interior
    );
    eprintln!(
        "hillslope parity: max_abs_boundary={:.3e}, max_rel_boundary={:.3e}",
        max_abs_boundary, max_rel_boundary
    );

    assert!(
        max_abs_interior <= ABS_TOL,
        "DD8 interior absolute tolerance exceeded: max_abs={:.3e} > {:.3e}",
        max_abs_interior,
        ABS_TOL
    );
    assert!(
        max_rel_interior <= REL_TOL,
        "DD8 interior relative tolerance exceeded: max_rel={:.3e} > {:.3e}",
        max_rel_interior,
        REL_TOL
    );
    assert!(
        max_abs_boundary <= ABS_TOL,
        "DD8 boundary absolute tolerance exceeded: max_abs={:.3e} > {:.3e}",
        max_abs_boundary,
        ABS_TOL
    );
    assert!(
        max_rel_boundary <= REL_TOL,
        "DD8 boundary relative tolerance exceeded: max_rel={:.3e} > {:.3e}",
        max_rel_boundary,
        REL_TOL
    );

    // ── Mass conservation: 100-iteration drift ────────────────────────────────
    // Run 100 GPU dispatches on a fresh world and check that the per-iteration
    // mean height drift stays ≤ 1e-6.
    let mut world_mass = clone_world(&world_template);
    let h0_mean: f64 = mean_height(&world_mass);

    for _ in 0..100 {
        gpu_backend
            .run_hillslope_diffusion(&mut world_mass, &params)
            .expect("GPU hillslope must succeed on mass conservation run");
    }
    let h100_mean: f64 = mean_height(&world_mass);
    let drift_per_iter = (h100_mean - h0_mean).abs() / 100.0;

    eprintln!(
        "hillslope mass conservation: h0_mean={:.6}, h100_mean={:.6}, drift_per_iter={:.3e}",
        h0_mean, h100_mean, drift_per_iter
    );

    assert!(
        drift_per_iter <= 1e-6,
        "DD8 mass conservation: mean drift per iteration {:.3e} exceeds 1e-6",
        drift_per_iter
    );
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Clone a `WorldState` by cloning only the fields needed for hillslope
/// dispatch: `resolution`, `preset`, `authoritative.height`, `derived.coast_mask`.
fn clone_world(src: &WorldState) -> WorldState {
    use island_core::world::CoastMask;

    let mut dst = WorldState::new(src.seed, src.preset.clone(), src.resolution);

    // Clone height.
    if let Some(ref h) = src.authoritative.height {
        let mut h2 = ScalarField2D::<f32>::new(h.width, h.height);
        h2.data.clone_from(&h.data);
        dst.authoritative.height = Some(h2);
    }

    // Clone coast_mask (need is_sea, is_coast — is_land and river_mouth_mask
    // are not used by the hillslope kernel directly).
    if let Some(ref cm) = src.derived.coast_mask {
        use island_core::field::MaskField2D;
        let w = cm.is_sea.width;
        let h = cm.is_sea.height;
        let mut is_land = MaskField2D::new(w, h);
        is_land.data.clone_from(&cm.is_land.data);
        let mut is_sea = MaskField2D::new(w, h);
        is_sea.data.clone_from(&cm.is_sea.data);
        let mut is_coast = MaskField2D::new(w, h);
        is_coast.data.clone_from(&cm.is_coast.data);
        dst.derived.coast_mask = Some(CoastMask {
            is_land,
            is_sea,
            is_coast,
            land_cell_count: cm.land_cell_count,
            river_mouth_mask: None,
        });
    }

    dst
}

/// Compute the mean height of all cells in `world.authoritative.height`.
fn mean_height(world: &WorldState) -> f64 {
    let h = world.authoritative.height.as_ref().unwrap();
    let sum: f64 = h.data.iter().map(|&v| v as f64).sum();
    sum / h.data.len() as f64
}

// ── Stream power parity helpers ───────────────────────────────────────────────

/// Build a `WorldState` that has been run through the pipeline up to (but not
/// including) the first SPIM dispatch inside `ErosionOuterLoop`.
///
/// Approach: run the full pipeline with `n_batch=0` (no erosion) so all the
/// prerequisite derived fields (coast_mask, accumulation, slope, sediment) are
/// populated.  Then manually set `n_batch` back to 1 so the caller can invoke
/// a single SPIM step.
///
/// Uses `volcanic_single` preset, seed 42, 128² resolution — the same
/// configuration as the hillslope parity test.
fn build_pre_spim_world() -> WorldState {
    let mut preset =
        data::presets::load_preset("volcanic_single").expect("volcanic_single preset must exist");
    let resolution = Resolution::new(128, 128);

    // Run the full pipeline with n_batch=0 to populate all derived fields
    // (coast_mask, accumulation, slope, sediment init from CoastMaskStage)
    // without running any erosion iterations.
    preset.erosion.n_batch = 0;
    let mut world = WorldState::new(island_core::seed::Seed(42), preset, resolution);
    sim::default_pipeline()
        .run(&mut world)
        .expect("pre-spim pipeline run must succeed");

    world
}

/// Clone a `WorldState` with all fields needed for stream power dispatch:
/// - `authoritative.height`
/// - `authoritative.sediment`
/// - `derived.coast_mask` (is_land, is_sea, is_coast)
/// - `derived.accumulation`
/// - `derived.slope`
fn clone_world_for_stream_power(src: &WorldState) -> WorldState {
    use island_core::{field::MaskField2D, world::CoastMask};

    let mut dst = WorldState::new(src.seed, src.preset.clone(), src.resolution);

    // Clone height.
    if let Some(ref h) = src.authoritative.height {
        let mut h2 = ScalarField2D::<f32>::new(h.width, h.height);
        h2.data.clone_from(&h.data);
        dst.authoritative.height = Some(h2);
    }

    // Clone sediment.
    if let Some(ref hs) = src.authoritative.sediment {
        let mut hs2 = ScalarField2D::<f32>::new(hs.width, hs.height);
        hs2.data.clone_from(&hs.data);
        dst.authoritative.sediment = Some(hs2);
    }

    // Clone coast_mask.
    if let Some(ref cm) = src.derived.coast_mask {
        let w = cm.is_land.width;
        let h = cm.is_land.height;
        let mut is_land = MaskField2D::new(w, h);
        is_land.data.clone_from(&cm.is_land.data);
        let mut is_sea = MaskField2D::new(w, h);
        is_sea.data.clone_from(&cm.is_sea.data);
        let mut is_coast = MaskField2D::new(w, h);
        is_coast.data.clone_from(&cm.is_coast.data);
        dst.derived.coast_mask = Some(CoastMask {
            is_land,
            is_sea,
            is_coast,
            land_cell_count: cm.land_cell_count,
            river_mouth_mask: None,
        });
    }

    // Clone accumulation.
    if let Some(ref acc) = src.derived.accumulation {
        let mut acc2 = ScalarField2D::<f32>::new(acc.width, acc.height);
        acc2.data.clone_from(&acc.data);
        dst.derived.accumulation = Some(acc2);
    }

    // Clone slope.
    if let Some(ref sl) = src.derived.slope {
        let mut sl2 = ScalarField2D::<f32>::new(sl.width, sl.height);
        sl2.data.clone_from(&sl.data);
        dst.derived.slope = Some(sl2);
    }

    dst
}

/// Extract `StreamPowerParams` from a world (matching how `ErosionOuterLoop`
/// builds them, including `hs_entrain_max` from the locked constant).
fn stream_power_params(world: &WorldState) -> StreamPowerParams {
    StreamPowerParams {
        spim_k: world.preset.erosion.spim_k,
        spim_m: world.preset.erosion.spim_m,
        spim_n: world.preset.erosion.spim_n,
        space_k_bed: world.preset.erosion.space_k_bed,
        space_k_sed: world.preset.erosion.space_k_sed,
        h_star: world.preset.erosion.h_star,
        // HS_ENTRAIN_MAX is a locked constant in sim::geomorph::sediment; use the
        // canonical value 0.5 here — matches what ErosionOuterLoop passes through.
        hs_entrain_max: 0.5,
        sea_level: world.preset.sea_level,
        spim_variant: world.preset.erosion.spim_variant,
    }
}

// ── DD8 stream-power per-iter parity test ─────────────────────────────────────

/// Sprint 4.F DD8 gate: GPU stream power incision output must match the CPU
/// reference within DD8's per-kernel tolerances.
///
/// # Region-based tolerance (DD8)
///
/// - Sea cells (`is_land == 0`): exactly `0.0` delta — both impls' is_land
///   gate must agree byte-exact.
/// - Low-slope interior cells (`slope < 1e-6`): `max |delta| ≤ 1e-7`.
/// - Normal land cells with `accumulation > 1e-4`:
///   `max |delta_cpu - delta_gpu| ≤ 1e-4` on the *incision delta*
///   (not absolute height).
/// - Hard invariants: no NaN/Inf in `h_gpu`; all sediment ∈ [0, 1].
///
/// # Opt-in
///
/// Set `IPG_RUN_GPU_PARITY=1` and run with `--ignored`.
#[test]
#[ignore = "requires GPU adapter; set IPG_RUN_GPU_PARITY=1 and run with --ignored"]
fn compute_backend_parity_stream_power_within_tolerance() {
    if !gpu_parity_enabled() {
        eprintln!("skipped — set IPG_RUN_GPU_PARITY=1 to run GPU parity tests");
        return;
    }

    let world_template = build_pre_spim_world();
    let w = world_template.resolution.sim_width as usize;
    let h_grid = world_template.resolution.sim_height as usize;
    let params = stream_power_params(&world_template);

    // ── CPU reference ─────────────────────────────────────────────────────────
    let mut world_cpu = clone_world_for_stream_power(&world_template);
    let cpu_backend = Arc::new(sim::compute::CpuBackend);
    let cpu_timing = cpu_backend
        .run_stream_power_incision(&mut world_cpu, &params)
        .expect("CPU stream power must succeed");

    let h_cpu = world_cpu
        .authoritative
        .height
        .as_ref()
        .unwrap()
        .data
        .clone();
    let h_input = world_template
        .authoritative
        .height
        .as_ref()
        .unwrap()
        .data
        .clone();
    let is_land = world_template
        .derived
        .coast_mask
        .as_ref()
        .unwrap()
        .is_land
        .data
        .clone();
    let accumulation = world_template
        .derived
        .accumulation
        .as_ref()
        .unwrap()
        .data
        .clone();
    let slope_data = world_template.derived.slope.as_ref().unwrap().data.clone();

    // ── GPU run ───────────────────────────────────────────────────────────────
    let ctx = Arc::new(
        gpu::GpuContext::new_headless((w as u32, h_grid as u32))
            .expect("GPU headless context required for parity test"),
    );
    let gpu_backend = gpu::GpuBackend::new(Arc::clone(&ctx));
    assert!(
        gpu_backend.supports(ComputeOp::StreamPowerIncision),
        "GpuBackend must support StreamPowerIncision at Sprint 4.F"
    );

    let mut world_gpu = clone_world_for_stream_power(&world_template);
    let gpu_timing = gpu_backend
        .run_stream_power_incision(&mut world_gpu, &params)
        .expect("GPU stream power must succeed");

    let h_gpu = world_gpu
        .authoritative
        .height
        .as_ref()
        .unwrap()
        .data
        .clone();
    let hs_gpu = world_gpu
        .authoritative
        .sediment
        .as_ref()
        .unwrap()
        .data
        .clone();

    eprintln!(
        "stream_power parity: cpu_ms={:.3}, gpu_ms={:?}",
        gpu_timing.cpu_ms, gpu_timing.gpu_ms
    );
    eprintln!(
        "stream_power parity [cpu reference]: cpu_ms={:.3}",
        cpu_timing.cpu_ms,
    );

    // ── DD8 tolerance checks ──────────────────────────────────────────────────
    // Hard invariants: no NaN/Inf in h_gpu; sediment ∈ [0, 1].
    for (i, &v) in h_gpu.iter().enumerate() {
        assert!(v.is_finite(), "cell {i}: h_gpu non-finite: {v}");
    }
    for (i, &v) in hs_gpu.iter().enumerate() {
        assert!(
            v.is_finite() && (0.0..=1.0).contains(&v),
            "cell {i}: sediment_gpu out of [0,1] or non-finite: {v}"
        );
    }

    // Per-region tolerance checks.
    let n_cells = w * h_grid;
    let mut max_abs_sea: f32 = 0.0;
    let mut max_abs_low_slope: f32 = 0.0;
    let mut max_delta_diff_normal: f32 = 0.0;
    let mut max_abs_normal: f32 = 0.0;

    for i in 0..n_cells {
        let diff_h = (h_cpu[i] - h_gpu[i]).abs();

        if is_land[i] == 0 {
            // Sea cell: must be exact 0.0 delta.
            max_abs_sea = max_abs_sea.max(diff_h);
        } else if slope_data[i] < 1e-6 {
            // Near-zero slope: very tight bound.
            max_abs_low_slope = max_abs_low_slope.max(diff_h);
        } else if accumulation[i] > 1e-4 {
            // Normal land cell: compare incision deltas (avoids absolute height fp noise).
            let delta_cpu = h_input[i] - h_cpu[i]; // erosion applied by CPU
            let delta_gpu = h_input[i] - h_gpu[i]; // erosion applied by GPU
            let delta_diff = (delta_cpu - delta_gpu).abs();
            max_delta_diff_normal = max_delta_diff_normal.max(delta_diff);
            max_abs_normal = max_abs_normal.max(diff_h);
        }
    }

    eprintln!(
        "stream_power parity: max_abs_sea={:.3e}, max_abs_low_slope={:.3e}, \
         max_delta_diff_normal={:.3e}, max_abs_normal={:.3e}",
        max_abs_sea, max_abs_low_slope, max_delta_diff_normal, max_abs_normal
    );

    // DD8: sea cells must be bit-exact (is_land gate agreement).
    assert!(
        max_abs_sea == 0.0,
        "DD8: sea cells must have 0.0 delta (is_land gate mismatch); max={:.3e}",
        max_abs_sea
    );

    // DD8: low-slope cells ≤ 1e-7.
    assert!(
        max_abs_low_slope <= 1e-7,
        "DD8: low-slope cells tolerance exceeded: max={:.3e} > 1e-7",
        max_abs_low_slope
    );

    // DD8: normal land cells with A > 1e-4: delta-diff ≤ 1e-4.
    assert!(
        max_delta_diff_normal <= 1e-4,
        "DD8: normal land cell incision-delta tolerance exceeded: max={:.3e} > 1e-4",
        max_delta_diff_normal
    );
}

// ── DD8 accumulated 100-iteration parity test ─────────────────────────────────

/// Sprint 4.F DD8 gate: after 100 stream power iterations (ErosionOuterLoop
/// with 10 batches × 10 inner steps), GPU and CPU height fields agree within
/// `1e-3 × max(|h_cpu|)` accumulated drift.
///
/// Also verifies the hard invariants: no NaN/Inf, sediment ∈ [0, 1], and the
/// `erosion_no_excessive_sea_crossing` property (< 5 % of land cells crossing
/// sea_level).
///
/// # Opt-in
///
/// Set `IPG_RUN_GPU_PARITY=1` and run with `--ignored`.
#[test]
#[ignore = "requires GPU adapter; set IPG_RUN_GPU_PARITY=1 and run with --ignored"]
fn compute_backend_parity_full_erosion_outer_loop_accumulated_within_tolerance() {
    if !gpu_parity_enabled() {
        eprintln!("skipped — set IPG_RUN_GPU_PARITY=1 to run GPU parity tests");
        return;
    }

    // ── Build two parallel worlds from the same seed+preset ──────────────────
    // Use a smaller grid (64²) for the accumulated test so it completes quickly.
    let mut preset =
        data::presets::load_preset("volcanic_single").expect("volcanic_single preset must exist");
    let resolution = Resolution::new(64, 64);

    // 10 batches × 10 inner = 100 stream-power dispatches per backend.
    preset.erosion.n_batch = 10;
    preset.erosion.n_inner = 10;

    let mut world_cpu = WorldState::new(island_core::seed::Seed(42), preset.clone(), resolution);
    let mut world_gpu_state =
        WorldState::new(island_core::seed::Seed(42), preset.clone(), resolution);

    // ── CPU run (default CpuBackend pipeline) ─────────────────────────────────
    let cpu_start = std::time::Instant::now();
    sim::default_pipeline()
        .run(&mut world_cpu)
        .expect("CPU pipeline run must succeed");
    let cpu_total_ms = cpu_start.elapsed().as_secs_f64() * 1000.0;

    // ── GPU run (GpuBackend pipeline) ─────────────────────────────────────────
    let w = resolution.sim_width;
    let h_grid = resolution.sim_height;
    let ctx = Arc::new(
        gpu::GpuContext::new_headless((w, h_grid))
            .expect("GPU headless context required for parity test"),
    );
    let gpu_backend_arc: Arc<dyn island_core::pipeline::ComputeBackend> =
        Arc::new(gpu::GpuBackend::new(Arc::clone(&ctx)));

    let gpu_start = std::time::Instant::now();
    sim::default_pipeline_with_backend(gpu_backend_arc)
        .run(&mut world_gpu_state)
        .expect("GPU pipeline run must succeed");
    let gpu_total_ms = gpu_start.elapsed().as_secs_f64() * 1000.0;

    // Drain any accumulated GPU ms from the world side-channel for reporting.
    let gpu_kernel_ms = world_gpu_state.derived.last_stage_gpu_ms.unwrap_or(0.0);

    let h_cpu = world_cpu
        .authoritative
        .height
        .as_ref()
        .unwrap()
        .data
        .clone();
    let h_gpu = world_gpu_state
        .authoritative
        .height
        .as_ref()
        .unwrap()
        .data
        .clone();
    let hs_gpu = world_gpu_state
        .authoritative
        .sediment
        .as_ref()
        .unwrap()
        .data
        .clone();
    let is_land = world_cpu
        .derived
        .coast_mask
        .as_ref()
        .unwrap()
        .is_land
        .data
        .clone();

    // ── Hard invariants ───────────────────────────────────────────────────────
    for (i, &v) in h_gpu.iter().enumerate() {
        assert!(
            v.is_finite(),
            "cell {i}: h_gpu non-finite after 100 iters: {v}"
        );
    }
    for (i, &v) in hs_gpu.iter().enumerate() {
        assert!(
            v.is_finite() && (0.0..=1.0).contains(&v),
            "cell {i}: sediment_gpu out of [0,1] after 100 iters: {v}"
        );
    }

    // erosion_no_excessive_sea_crossing: < 5 % of original land cells cross sea_level.
    let sea_level = preset.sea_level;
    let n_land_pre = is_land.iter().filter(|&&v| v == 1).count();
    let n_crossed_gpu = h_gpu
        .iter()
        .zip(is_land.iter())
        .filter(|(h, land)| **land == 1 && **h < sea_level)
        .count();
    let sea_crossing_pct = if n_land_pre > 0 {
        n_crossed_gpu as f64 / n_land_pre as f64 * 100.0
    } else {
        0.0
    };
    assert!(
        sea_crossing_pct < 5.0,
        "erosion_no_excessive_sea_crossing: {sea_crossing_pct:.2}% of land cells \
         crossed sea_level (limit 5 %)"
    );

    // ── Accumulated tolerance check ───────────────────────────────────────────
    let n_cells = (w * h_grid) as usize;
    let max_cpu_h: f32 = h_cpu.iter().fold(0.0_f32, |acc, &v| acc.max(v.abs()));
    let accumulated_tol = 1e-3 * max_cpu_h;

    let mut max_abs_err: f32 = 0.0;
    for i in 0..n_cells {
        let diff = (h_cpu[i] - h_gpu[i]).abs();
        max_abs_err = max_abs_err.max(diff);
    }

    eprintln!(
        "accumulated_100_iter: cpu_ms={:.1}, gpu_total_ms={:.1}, gpu_kernel_ms={:.1}, \
         max_abs_err={:.3e}, tol={:.3e} (= 1e-3 * {:.4}), sea_crossing_gpu={:.2}%",
        cpu_total_ms,
        gpu_total_ms,
        gpu_kernel_ms,
        max_abs_err,
        accumulated_tol,
        max_cpu_h,
        sea_crossing_pct
    );

    assert!(
        max_abs_err <= accumulated_tol,
        "DD8 accumulated 100-iter tolerance exceeded: max_abs_err={:.3e} > tol={:.3e}",
        max_abs_err,
        accumulated_tol
    );
}
