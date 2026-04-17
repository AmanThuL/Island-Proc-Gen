//! Island Proc-Gen — desktop shell + headless capture entry point.
//!
//! Boots a winit 0.30 `ApplicationHandler` event loop, constructs the
//! `Runtime` (window + wgpu + egui + camera) on the first `resumed` event,
//! and delegates all window events to `Runtime::handle_window_event`.
//!
//! When invoked with `--headless <request.ron>` or
//! `--headless-validate <run> --against <expected>` the interactive path
//! is skipped entirely — no winit event loop, no window, no egui — and the
//! process exits with the AD9 code (0 / 2 / 3) mapped from
//! [`headless::output::OverallStatus::exit_code`].

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Result, anyhow};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    window::WindowId,
};

use app::headless;
use app::runtime::Runtime;

// ── CLI argument parsing ──────────────────────────────────────────────────────

/// Top-level CLI mode selected by argv.
///
/// Flags are parsed by-name rather than by position so the order and the
/// presence of other argv entries (e.g. cargo's own test harness) doesn't
/// break the routing.
enum CliMode {
    Interactive,
    Headless { request: PathBuf },
    HeadlessValidate { run: PathBuf, expected: PathBuf },
}

fn parse_cli(args: &[String]) -> Result<CliMode> {
    if let Some(i) = args.iter().position(|a| a == "--headless") {
        let request = args
            .get(i + 1)
            .ok_or_else(|| anyhow!("--headless requires a <request.ron> path"))?;
        return Ok(CliMode::Headless {
            request: PathBuf::from(request),
        });
    }

    if let Some(i) = args.iter().position(|a| a == "--headless-validate") {
        let run = args
            .get(i + 1)
            .ok_or_else(|| anyhow!("--headless-validate requires a <run_dir> path"))?;
        // Only accept `--against` that appears after `--headless-validate`;
        // a stray `--against` earlier in argv would otherwise silently steal
        // the expected-path slot.
        let against_i = args
            .iter()
            .enumerate()
            .skip(i + 1)
            .find_map(|(idx, a)| (a == "--against").then_some(idx))
            .ok_or_else(|| anyhow!("--headless-validate requires --against <expected_dir>"))?;
        let expected = args
            .get(against_i + 1)
            .ok_or_else(|| anyhow!("--against requires an <expected_dir> path"))?;
        return Ok(CliMode::HeadlessValidate {
            run: PathBuf::from(run),
            expected: PathBuf::from(expected),
        });
    }

    Ok(CliMode::Interactive)
}

// ── AppHandler ────────────────────────────────────────────────────────────────

struct AppHandler {
    runtime: Option<Runtime>,
}

impl ApplicationHandler for AppHandler {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.runtime.is_none() {
            match Runtime::new(event_loop) {
                Ok(rt) => {
                    self.runtime = Some(rt);
                }
                Err(e) => {
                    tracing::error!("Runtime::new failed: {e:#}");
                    event_loop.exit();
                }
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        if let Some(rt) = self.runtime.as_mut() {
            rt.handle_window_event(event_loop, event);
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(rt) = self.runtime.as_ref() {
            rt.request_redraw();
        }
    }
}

// ── Entry point ───────────────────────────────────────────────────────────────

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("app=debug".parse().unwrap())
                .add_directive("gpu=info".parse().unwrap())
                .add_directive("render=info".parse().unwrap()),
        )
        .init();

    let args: Vec<String> = std::env::args().collect();
    let mode = match parse_cli(&args) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("cli parse error: {e:#}");
            // CLI parsing failure is a tool-level error per AD9 (exit code 3).
            return ExitCode::from(3);
        }
    };

    match mode {
        CliMode::Interactive => match run_interactive() {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                tracing::error!("interactive loop failed: {e:#}");
                ExitCode::FAILURE
            }
        },
        CliMode::Headless { request } => run_headless_exit(headless::run(&request)),
        CliMode::HeadlessValidate { run, expected } => {
            run_headless_exit(headless::validate(&run, &expected))
        }
    }
}

fn run_interactive() -> Result<()> {
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);

    let mut handler = AppHandler { runtime: None };
    event_loop.run_app(&mut handler)?;
    Ok(())
}

/// Collapse a `headless::{run,validate}` result to the AD9 exit code byte.
///
/// * `Ok(status)` → `status.exit_code() as u8` (0 / 2 / 3 by AD9 contract)
/// * `Err(_)`     → `3` (tool-level error — e.g. RON parse or IO outside the
///   `OverallStatus` reporting pipeline)
///
/// Split from `run_headless_exit` so the routing can be unit-tested — the
/// stdlib `ExitCode` type does not implement `PartialEq` as of Rust 1.95.
fn headless_exit_byte(result: &Result<app::headless::output::OverallStatus>) -> u8 {
    match result {
        Ok(status) => status.exit_code() as u8,
        Err(_) => 3,
    }
}

/// Map a `headless::{run,validate}` result to the AD9 process exit code and
/// emit a stderr breadcrumb for the tool-level error / internal-error paths.
fn run_headless_exit(result: Result<app::headless::output::OverallStatus>) -> ExitCode {
    match &result {
        Ok(app::headless::output::OverallStatus::InternalError { reason, .. }) => {
            eprintln!("headless internal error: {reason}");
        }
        Err(e) => {
            eprintln!("headless harness error: {e:#}");
        }
        _ => {}
    }
    ExitCode::from(headless_exit_byte(&result))
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| (*x).to_owned()).collect()
    }

    #[test]
    fn no_flags_defaults_to_interactive() {
        let args = s(&["app"]);
        assert!(matches!(parse_cli(&args).unwrap(), CliMode::Interactive));
    }

    #[test]
    fn headless_flag_takes_request_path() {
        let args = s(&["app", "--headless", "/tmp/req.ron"]);
        let CliMode::Headless { request } = parse_cli(&args).unwrap() else {
            panic!("expected CliMode::Headless");
        };
        assert_eq!(request, PathBuf::from("/tmp/req.ron"));
    }

    #[test]
    fn headless_flag_missing_path_errors() {
        let args = s(&["app", "--headless"]);
        assert!(parse_cli(&args).is_err());
    }

    #[test]
    fn validate_flag_requires_both_paths() {
        let args = s(&[
            "app",
            "--headless-validate",
            "/tmp/run",
            "--against",
            "/tmp/exp",
        ]);
        let CliMode::HeadlessValidate { run, expected } = parse_cli(&args).unwrap() else {
            panic!("expected CliMode::HeadlessValidate");
        };
        assert_eq!(run, PathBuf::from("/tmp/run"));
        assert_eq!(expected, PathBuf::from("/tmp/exp"));
    }

    #[test]
    fn validate_flag_missing_against_errors() {
        let args = s(&["app", "--headless-validate", "/tmp/run"]);
        assert!(parse_cli(&args).is_err());
    }

    #[test]
    fn validate_flag_missing_expected_path_errors() {
        let args = s(&["app", "--headless-validate", "/tmp/run", "--against"]);
        assert!(parse_cli(&args).is_err());
    }

    #[test]
    fn validate_flag_ignores_against_appearing_before_headless_validate() {
        // A stray `--against` earlier in argv must not be scooped up in place
        // of one that follows `--headless-validate`.
        let args = s(&[
            "app",
            "--against",
            "/early/stray",
            "--headless-validate",
            "/tmp/run",
        ]);
        assert!(parse_cli(&args).is_err());
    }

    // §6 AD9 routing contract — locked here in main.rs to prevent drift
    // between `OverallStatus::exit_code()` and the process exit byte.
    #[test]
    fn headless_exit_byte_maps_overall_status_to_ad9_code() {
        use app::headless::output::{InternalErrorKind, OverallStatus};

        let passed = Ok(OverallStatus::Passed);
        assert_eq!(headless_exit_byte(&passed), 0);

        let passed_beauty_skipped = Ok(OverallStatus::PassedWithBeautySkipped {
            skipped_shot_ids: vec!["s1".into()],
            reason: "no GPU".into(),
        });
        assert_eq!(headless_exit_byte(&passed_beauty_skipped), 0);

        let failed_truth = Ok(OverallStatus::FailedTruthValidation { mismatches: vec![] });
        assert_eq!(headless_exit_byte(&failed_truth), 2);

        let failed_metrics = Ok(OverallStatus::FailedMetricsValidation { mismatches: vec![] });
        assert_eq!(headless_exit_byte(&failed_metrics), 2);

        let internal = Ok(OverallStatus::InternalError {
            reason: "x".into(),
            kind: InternalErrorKind::Other,
        });
        assert_eq!(headless_exit_byte(&internal), 3);

        // Any raw `Err` — IO failure / RON parse that couldn't even produce
        // an `OverallStatus` — must also surface as the tool-level `3` byte.
        let raw_err: Result<OverallStatus> = Err(anyhow!("outer io error"));
        assert_eq!(headless_exit_byte(&raw_err), 3);
    }
}
