//! Headless (windowless) capture pipeline.
//!
//! The entry point is [`request::CaptureRequest`], which is loaded from a RON
//! file and drives batch offline rendering without opening a GPU window.
//!
//! Two CLI sub-flags live on the main `app` binary (see `main.rs`):
//! - `--headless <request.ron>` → [`run`]
//! - `--headless-validate <run_dir> --against <expected_dir>` → [`validate`]
//!
//! Both return an [`output::OverallStatus`] which [`output::OverallStatus::exit_code`]
//! maps to the AD9 exit-code contract (0 / 2 / 3). `main.rs` uses that mapping
//! to set the process exit code without scraping any string.

use std::path::Path;

use anyhow::Result;

pub mod compare;
pub mod executor;
pub mod output;
pub mod request;

use output::OverallStatus;

/// Execute a `CaptureRequest` loaded from `request_path`.
///
/// Thin facade over [`executor::run_request`]; kept as the stable public API
/// that `main.rs` calls so the executor module can evolve without breaking
/// the `app::headless::run` call site.
///
/// Returns `(OverallStatus, Vec<String>)` where the second element is always
/// empty — `run` writes warnings into `summary.ron::warnings` itself rather
/// than surfacing them to the caller.
pub fn run(request_path: &Path) -> Result<(OverallStatus, Vec<String>)> {
    Ok((executor::run_request(request_path)?, Vec::new()))
}

/// Diff a runtime capture directory against a checked-in expected directory.
///
/// Delegates to [`compare::validate`] (AD5 three-step compare semantics).
/// The second element of the returned tuple carries human-readable warning
/// messages (fingerprint divergence, beauty asymmetry, etc.) that callers
/// should print to stderr.
pub fn validate(run_dir: &Path, expected_dir: &Path) -> Result<(OverallStatus, Vec<String>)> {
    compare::validate(run_dir, expected_dir)
}
