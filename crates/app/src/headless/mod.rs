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

pub mod output;
pub mod request;

use output::{InternalErrorKind, OverallStatus};

/// Execute a `CaptureRequest` loaded from `request_path`.
///
/// Implementation lands in Task 1C.6 (executor). Currently a stub that
/// reports an `InternalError` so calls pre-1C.6 fail loudly rather than
/// silently succeeding.
pub fn run(request_path: &Path) -> Result<OverallStatus> {
    Ok(OverallStatus::InternalError {
        reason: format!(
            "headless::run is not yet implemented (Task 1C.6); requested {request_path:?}"
        ),
        kind: InternalErrorKind::Other,
    })
}

/// Diff a runtime capture directory against a checked-in expected directory.
///
/// Implementation lands in Task 1C.8 (compare tool). Currently a stub.
pub fn validate(run_dir: &Path, expected_dir: &Path) -> Result<OverallStatus> {
    Ok(OverallStatus::InternalError {
        reason: format!(
            "headless::validate is not yet implemented (Task 1C.8); run={run_dir:?} expected={expected_dir:?}"
        ),
        kind: InternalErrorKind::Other,
    })
}
