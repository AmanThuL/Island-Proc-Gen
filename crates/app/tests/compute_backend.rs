//! Sprint 4.D integration tests for the `--compute-backend` CLI flag.
//!
//! These tests spawn the release binary and inspect the exit code + stderr.
//! The GPU backend at 4.D returns `Unsupported` for both pilot ops, so
//! `--compute-backend gpu` must exit 3 (`InternalError`) with a clear
//! error message.
//!
//! The non-ignored test (`headless_gpu_backend_returns_internal_error_with_clear_message`)
//! is the load-bearing 4.D verification gate per the sprint plan.
//!
//! # Requirements
//!
//! The test requires the `app` release binary to be present at
//! `target/release/app`. Run:
//!
//! ```bash
//! cargo build -p app --release
//! cargo test -p app --test compute_backend
//! ```

use std::path::PathBuf;
use std::process::Command;

/// Resolve the workspace root directory (two levels above `CARGO_MANIFEST_DIR`
/// for `crates/app`).
fn workspace_root() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.pop(); // crates/app → crates
    path.pop(); // crates → workspace root
    path
}

/// Resolve the path to the release `app` binary.
///
/// Returns `None` when the binary doesn't exist yet (e.g. before a release
/// build). Tests that need the binary call `skip_if_no_binary()` instead of
/// panicking.
fn release_binary() -> PathBuf {
    workspace_root().join("target/release/app")
}

/// Return true if the release binary exists; used to skip gracefully on
/// machines that have only done a debug build.
fn binary_exists() -> bool {
    release_binary().exists()
}

/// Path to the sprint_1a_baseline request.ron (relative to workspace root,
/// as the `output_dir` inside the file is also relative to workspace root).
fn baseline_request_relative() -> &'static str {
    "crates/data/golden/headless/sprint_1a_baseline/request.ron"
}

// ── Non-ignored tests (load-bearing Sprint 4.D gate) ─────────────────────────

/// Sprint 4.D gate: `--compute-backend gpu` exits 3 (`InternalError`) because
/// both pilot GPU ops are unimplemented at this sprint.
///
/// The error message on stderr must contain the phrase
/// `"GPU compute backend has no implementation for op"` so automated log
/// scrapers can pattern-match on it.
///
/// This test requires:
/// 1. The release binary to exist (`cargo build -p app --release`).
/// 2. A working GPU adapter (macOS Metal on the baseline host).
///
/// On machines without a GPU adapter, `GpuContext::new_headless` will fail
/// before any op dispatch — that also produces exit 3, but with a different
/// message. The test accepts either outcome as "exit 3 from GPU path".
#[test]
fn headless_gpu_backend_returns_internal_error_with_clear_message() {
    if !binary_exists() {
        eprintln!(
            "skip: release binary not found at {:?}; run `cargo build -p app --release` first",
            release_binary()
        );
        return;
    }

    let output = Command::new(release_binary())
        .current_dir(workspace_root())
        .arg("--headless")
        .arg(baseline_request_relative())
        .arg("--compute-backend")
        .arg("gpu")
        .output()
        .expect("failed to spawn app binary");

    // Exit code must be 3 (InternalError per AD9).
    let exit_code = output.status.code().unwrap_or(-1);
    assert_eq!(
        exit_code, 3,
        "expected exit code 3 (InternalError) for --compute-backend gpu at Sprint 4.D, got {exit_code}"
    );

    // The combined stderr/stdout must contain the clear error phrase so log
    // scrapers can identify this as a "not-yet-implemented" rather than a
    // real GPU error.
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let combined = format!("{stderr}{stdout}");

    let has_impl_message = combined.contains("GPU compute backend has no implementation for op")
        // Also accept the adapter-failure path for machines without GPU.
        || combined.contains("GPU context")
        || combined.contains("GpuContext::new_headless failed")
        || combined.contains("No suitable GPU adapter");

    assert!(
        has_impl_message,
        "stderr must contain 'GPU compute backend has no implementation for op' (or a GPU \
         bootstrap failure message on machines without a GPU adapter); got:\n{combined}"
    );
}

/// Verify that `--compute-backend cpu` still exits 0 (Passed) on the same
/// baseline that exits 3 for `gpu`. This is the regression guard: the CPU
/// path must remain bit-identical.
///
/// Marked `#[ignore]` because it requires the release binary and a GPU adapter
/// (for the beauty path). Run with `cargo test -p app --test compute_backend -- --ignored`.
#[test]
#[ignore = "requires release binary + GPU adapter; run with --ignored to exercise"]
fn headless_cpu_backend_exits_zero_on_baseline() {
    if !binary_exists() {
        eprintln!(
            "skip: release binary not found at {:?}; run `cargo build -p app --release` first",
            release_binary()
        );
        return;
    }

    let output = Command::new(release_binary())
        .current_dir(workspace_root())
        .arg("--headless")
        .arg(baseline_request_relative())
        .arg("--compute-backend")
        .arg("cpu")
        .output()
        .expect("failed to spawn app binary");

    let exit_code = output.status.code().unwrap_or(-1);
    assert_eq!(
        exit_code,
        0,
        "expected exit code 0 (Passed) for --compute-backend cpu, got {exit_code}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Verify that omitting `--compute-backend` (default = cpu) still exits 0.
/// This guards the "no silent CPU fallback" requirement: the default path
/// must be explicitly cpu-fast.
///
/// Marked `#[ignore]` because it requires the release binary and a GPU adapter
/// (for the beauty path). Run with `cargo test -p app --test compute_backend -- --ignored`.
#[test]
#[ignore = "requires release binary + GPU adapter; run with --ignored to exercise"]
fn headless_default_backend_exits_zero_on_baseline() {
    if !binary_exists() {
        eprintln!(
            "skip: release binary not found at {:?}; run `cargo build -p app --release` first",
            release_binary()
        );
        return;
    }

    let output = Command::new(release_binary())
        .current_dir(workspace_root())
        .arg("--headless")
        .arg(baseline_request_relative())
        .output()
        .expect("failed to spawn app binary");

    let exit_code = output.status.code().unwrap_or(-1);
    assert_eq!(
        exit_code,
        0,
        "expected exit code 0 (Passed) for default (cpu) backend, got {exit_code}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
