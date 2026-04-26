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
    pipeline::{ComputeBackend, ComputeOp, HillslopeParams},
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
