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

use output::{OverallStatus, RunLayout, RunSummary};

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

/// Read the `summary.ron` written by a `--headless <request_path>` run.
///
/// Resolves the output directory the same way [`executor::run_request`] does:
/// uses `CaptureRequest.output_dir` when present, otherwise falls back to
/// `captures/headless/<run_id>`. Returns the parsed [`RunSummary`] on
/// success; returns `Err` when the request or summary cannot be read/parsed.
///
/// Used by `main.rs` for `--print-breakdown`: we re-read the summary that the
/// executor wrote to disk so we don't have to plumb the full summary through
/// the `run()` return type (which only returns `(OverallStatus, Vec<String>)`
/// to keep the public API stable across sprints).
pub fn read_last_summary(request_path: &Path) -> Result<RunSummary> {
    let text = std::fs::read_to_string(request_path)?;
    let req: request::CaptureRequest = ron::de::from_str(&text)?;

    let run_id = req
        .run_id
        .clone()
        .unwrap_or_else(|| output::compute_run_id(&req));
    let output_dir = req
        .output_dir
        .clone()
        .unwrap_or_else(|| std::path::PathBuf::from("captures/headless").join(&run_id));

    let layout = RunLayout::new(output_dir);
    let summary_text = std::fs::read_to_string(layout.summary_ron())?;
    let summary: RunSummary = ron::de::from_str(&summary_text)?;
    Ok(summary)
}
