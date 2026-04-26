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

use std::collections::BTreeMap;
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
    Headless {
        request: PathBuf,
        /// Sprint 4.A: when `true`, print a per-stage timing breakdown to
        /// stdout after the run completes. GPU columns show `—` in 4.A;
        /// populated by Sprint 4.D+ `GpuBackend` implementations.
        print_breakdown: bool,
    },
    HeadlessValidate {
        run: PathBuf,
        expected: PathBuf,
    },
}

fn parse_cli(args: &[String]) -> Result<CliMode> {
    if let Some(i) = args.iter().position(|a| a == "--headless") {
        let request = args
            .get(i + 1)
            .ok_or_else(|| anyhow!("--headless requires a <request.ron> path"))?;
        let print_breakdown = args.iter().any(|a| a == "--print-breakdown");
        return Ok(CliMode::Headless {
            request: PathBuf::from(request),
            print_breakdown,
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
        CliMode::Headless {
            request,
            print_breakdown,
        } => {
            let result = headless::run(&request);
            // Print breakdown before exit if requested.
            if print_breakdown {
                if let Ok((ref status, _)) = result {
                    if let Ok(summary) = headless::read_last_summary(&request) {
                        print_stage_breakdown(&summary);
                    } else {
                        // Summary may not be written on InternalError; best effort only.
                        eprintln!(
                            "warn: --print-breakdown requested but summary.ron could not be read \
                             (status: {status:?})"
                        );
                    }
                }
            }
            run_headless_exit(result)
        }
        CliMode::HeadlessValidate { run, expected } => {
            run_headless_exit(headless::validate(&run, &expected))
        }
    }
}

/// Slack percentage above which a warning is emitted for a single shot's
/// timing breakdown. Represents overhead outside the per-stage timing loop
/// (e.g. ValidationStage not captured, OS scheduling jitter).
const SLACK_WARNING_PCT: f64 = 10.0;

/// Sprint 4.A DD3 surface A: print per-shot stage timing tables after a
/// `--headless --print-breakdown` run.
///
/// Format per sprint doc DD3:
/// - One table per shot, plus a cross-shot summary table.
/// - GPU columns show `—` in Sprint 4.A (always `None`); populated by 4.D+.
/// - Slack = `pipeline_ms − Σcpu_ms`. Normal < 5%; > 10% emits a warning.
fn print_stage_breakdown(summary: &app::headless::output::RunSummary) {
    use app::headless::output::ShotSummary;

    /// Format an optional GPU timing as a right-aligned 8-char field,
    /// substituting `—` when no GPU time was recorded.
    fn format_gpu_ms(gpu_ms: Option<f64>) -> String {
        gpu_ms
            .map(|g| format!("{g:>8.3}"))
            .unwrap_or_else(|| "       —".to_owned())
    }

    /// Compute a percentage of `value / total * 100`, returning 0.0 when
    /// `total` is zero to avoid division-by-zero in degenerate runs.
    fn pct_of(value: f64, total: f64) -> f64 {
        if total > 0.0 {
            value / total * 100.0
        } else {
            0.0
        }
    }

    fn print_shot_table(shot: &ShotSummary) {
        let pipeline_ms = shot.pipeline_ms;
        println!("\nShot: {}  (backend: cpu)", shot.id);
        println!(
            "  {:<28} {:>8}   {:>8}   {:>10}",
            "Stage", "CPU ms", "GPU ms", "% of pipeline"
        );
        println!("  {}", "─".repeat(62));

        let mut cpu_sum = 0.0_f64;
        for (name, timing) in &shot.stage_timings {
            let pct = pct_of(timing.cpu_ms, pipeline_ms);
            println!(
                "  {:<28} {:>8.3}   {}   {:>9.1}%",
                name,
                timing.cpu_ms,
                format_gpu_ms(timing.gpu_ms),
                pct
            );
            cpu_sum += timing.cpu_ms;
        }

        println!("  {}", "─".repeat(62));
        println!(
            "  {:<28} {:>8.3}             {:>9.1}%",
            "TOTAL (stage sum)",
            cpu_sum,
            pct_of(cpu_sum, pipeline_ms)
        );
        let slack_pct = pct_of(pipeline_ms - cpu_sum, pipeline_ms);
        println!(
            "  {:<28} {:>8.3}             {:>+9.1}% slack",
            "pipeline_ms (lump)", pipeline_ms, slack_pct
        );
        if slack_pct > SLACK_WARNING_PCT {
            eprintln!(
                "warn: shot '{}' slack {slack_pct:.1}% exceeds {SLACK_WARNING_PCT:.0}% — \
                 overhead outside stage loop is high",
                shot.id
            );
        }
    }

    for shot in &summary.shots {
        print_shot_table(shot);
    }

    // Cross-shot summary
    if summary.shots.len() > 1 {
        println!("\n{}", "═".repeat(66));
        println!("Cross-shot summary ({} shots):", summary.shots.len());
        println!(
            "  {:<28} {:>8}   {:>8}   {:>10}",
            "Stage", "CPU ms", "GPU ms", "% of total pipeline"
        );
        println!("  {}", "─".repeat(62));

        // Collect all stage names in consistent (alphabetical) BTreeMap order.
        let mut totals: BTreeMap<&str, (f64, Option<f64>)> = BTreeMap::new();
        let mut pipeline_total = 0.0_f64;

        for shot in &summary.shots {
            pipeline_total += shot.pipeline_ms;
            for (name, timing) in &shot.stage_timings {
                let entry = totals.entry(name.as_str()).or_insert((0.0, None));
                entry.0 += timing.cpu_ms;
                if let Some(g) = timing.gpu_ms {
                    *entry.1.get_or_insert(0.0) += g;
                }
            }
        }

        let mut cpu_sum = 0.0_f64;
        for (name, (cpu, gpu)) in &totals {
            let pct = pct_of(*cpu, pipeline_total);
            println!(
                "  {:<28} {:>8.3}   {}   {:>9.1}%",
                name,
                cpu,
                format_gpu_ms(*gpu),
                pct
            );
            cpu_sum += cpu;
        }
        println!("  {}", "─".repeat(62));
        let slack_pct = pct_of(pipeline_total - cpu_sum, pipeline_total);
        println!(
            "  {:<28} {:>8.3}             {:>+9.1}% slack",
            "pipeline_ms (total)", pipeline_total, slack_pct
        );
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
/// * `Ok((status, _))` → `status.exit_code() as u8` (0 / 2 / 3 by AD9 contract)
/// * `Err(_)`          → `3` (tool-level error — e.g. RON parse or IO outside
///   the `OverallStatus` reporting pipeline)
///
/// Split from `run_headless_exit` so the routing can be unit-tested — the
/// stdlib `ExitCode` type does not implement `PartialEq` as of Rust 1.95.
fn headless_exit_byte_from_result(
    result: &Result<(app::headless::output::OverallStatus, Vec<String>)>,
) -> u8 {
    match result {
        Ok((status, _)) => status.exit_code() as u8,
        Err(_) => 3,
    }
}

/// Map a `headless::{run,validate}` result to the AD9 process exit code and
/// emit a stderr breadcrumb for warnings, tool-level errors, and internal
/// errors.
fn run_headless_exit(
    result: Result<(app::headless::output::OverallStatus, Vec<String>)>,
) -> ExitCode {
    if let Ok((_, warnings)) = &result {
        for w in warnings {
            eprintln!("warn: {w}");
        }
    }
    match &result {
        Ok((app::headless::output::OverallStatus::InternalError { reason, .. }, _)) => {
            eprintln!("headless internal error: {reason}");
        }
        Err(e) => {
            eprintln!("headless harness error: {e:#}");
        }
        _ => {}
    }
    ExitCode::from(headless_exit_byte_from_result(&result))
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
        let CliMode::Headless {
            request,
            print_breakdown,
        } = parse_cli(&args).unwrap()
        else {
            panic!("expected CliMode::Headless");
        };
        assert_eq!(request, PathBuf::from("/tmp/req.ron"));
        assert!(
            !print_breakdown,
            "--print-breakdown should default to false"
        );
    }

    #[test]
    fn headless_print_breakdown_flag_is_parsed() {
        let args = s(&["app", "--headless", "/tmp/req.ron", "--print-breakdown"]);
        let CliMode::Headless {
            request,
            print_breakdown,
        } = parse_cli(&args).unwrap()
        else {
            panic!("expected CliMode::Headless");
        };
        assert_eq!(request, PathBuf::from("/tmp/req.ron"));
        assert!(
            print_breakdown,
            "--print-breakdown should be true when present"
        );
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

        let passed = Ok((OverallStatus::Passed, vec![]));
        assert_eq!(headless_exit_byte_from_result(&passed), 0);

        let passed_beauty_skipped = Ok((
            OverallStatus::PassedWithBeautySkipped {
                skipped_shot_ids: vec!["s1".into()],
                reason: "no GPU".into(),
            },
            vec![],
        ));
        assert_eq!(headless_exit_byte_from_result(&passed_beauty_skipped), 0);

        let failed_truth = Ok((
            OverallStatus::FailedTruthValidation { mismatches: vec![] },
            vec![],
        ));
        assert_eq!(headless_exit_byte_from_result(&failed_truth), 2);

        let failed_metrics = Ok((
            OverallStatus::FailedMetricsValidation { mismatches: vec![] },
            vec![],
        ));
        assert_eq!(headless_exit_byte_from_result(&failed_metrics), 2);

        let internal = Ok((
            OverallStatus::InternalError {
                reason: "x".into(),
                kind: InternalErrorKind::Other,
            },
            vec![],
        ));
        assert_eq!(headless_exit_byte_from_result(&internal), 3);

        // Any raw `Err` — IO failure / RON parse that couldn't even produce
        // an `OverallStatus` — must also surface as the tool-level `3` byte.
        let raw_err: Result<(OverallStatus, Vec<String>)> = Err(anyhow!("outer io error"));
        assert_eq!(headless_exit_byte_from_result(&raw_err), 3);
    }
}
