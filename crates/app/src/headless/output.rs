//! Output types, writers, and helpers for the headless capture pipeline.
//!
//! This module owns:
//! - [`RunSummary`] and friends (the §4 data structures written to `summary.ron`)
//! - [`compute_run_id`] / [`compute_request_fingerprint`] (AD5 canonical fingerprint)
//! - [`write_rgba8_png`] (deterministic RGBA-8 PNG encoder)
//! - [`RunLayout`] (directory-layout helper)
//! - [`write_request_ron`] / [`write_summary_ron`] (atomic RON writers)

use std::collections::BTreeMap;
use std::io::Write as IoWrite;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use data::golden::SummaryMetrics;

use crate::headless::request::CaptureRequest;

// ─────────────────────────────────────────────────────────────────────────────
// §4 Public data structures
// ─────────────────────────────────────────────────────────────────────────────

/// Top-level summary written to `<run_dir>/summary.ron` after a headless run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunSummary {
    /// Always `1` for Sprint 1C.
    pub schema_version: u32,
    /// Stable identifier for this run (first 16 hex chars of the request
    /// fingerprint when not explicitly supplied).
    pub run_id: String,
    /// Full 64-hex blake3 fingerprint of the canonical request bytes.
    pub request_fingerprint: String,
    /// ISO 8601 UTC timestamp of when the run started, e.g. `2026-04-17T12:34:56Z`.
    pub timestamp_utc: String,
    /// One entry per shot in the request.
    pub shots: Vec<ShotSummary>,
    /// Aggregate pass/fail status.
    pub overall_status: OverallStatus,
    /// Non-fatal warnings accumulated during the run.
    #[serde(default)]
    pub warnings: Vec<String>,
}

/// Per-shot summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShotSummary {
    /// Matches [`CaptureShot::id`](crate::headless::request::CaptureShot::id).
    pub id: String,
    /// CPU truth path results.
    pub truth: TruthSummary,
    /// GPU beauty results; `None` when [`BeautySpec`](crate::headless::request::BeautySpec) was absent.
    #[serde(default)]
    pub beauty: Option<BeautySummary>,
    /// Wall time spent running the simulation pipeline for this shot (ms).
    pub pipeline_ms: f64,
    /// Wall time spent baking/exporting overlay PNGs (ms).
    pub bake_ms: f64,
    /// Wall time spent on the GPU offscreen render (ms); `None` when skipped.
    #[serde(default)]
    pub gpu_render_ms: Option<f64>,
}

/// CPU truth results for a single shot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TruthSummary {
    /// Map of overlay ID → 64-hex blake3 hash of the exported PNG bytes.
    pub overlay_hashes: BTreeMap<String, String>,
    /// 64-hex blake3 hash of the serialised `metrics.ron` bytes.
    /// `None` when [`TruthSpec::include_metrics`](crate::headless::request::TruthSpec::include_metrics)
    /// is `false`; avoids a false-positive equality match between two shots that
    /// both disabled metrics (they would otherwise share the hash-of-empty-bytes sentinel).
    #[serde(default)]
    pub metrics_hash: Option<String>,
}

/// GPU beauty results for a single shot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeautySummary {
    /// Camera preset used (e.g. `"hero"`, `"top_debug"`, `"low_oblique"`).
    pub camera_preset: String,
    /// Whether the render succeeded, was skipped, etc.
    pub status: BeautyStatus,
    /// 64-hex blake3 hash of the written `beauty/scene.png` bytes; `None` when not rendered.
    #[serde(default)]
    pub byte_hash: Option<String>,
}

/// Outcome of a beauty (GPU) render attempt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BeautyStatus {
    /// GPU render completed and the PNG was written.
    Rendered,
    /// GPU render was skipped (headless environment without a suitable adapter, etc.).
    Skipped { reason: String },
}

/// Aggregate outcome of the full headless run.
///
/// # AD9 Exit-code contract
///
/// Shell scripts `case $?` this directly; use [`OverallStatus::exit_code`] to
/// obtain the code.
///
/// | Code | Meaning |
/// |------|---------|
/// | 0    | Truth green (all hashes match, or beauty was skipped non-fatally) |
/// | 2    | Validation failure (hash mismatch) |
/// | 3    | Tool-level / internal error |
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OverallStatus {
    /// All truth hashes matched; all beauty renders succeeded.
    Passed,
    /// Truth green but one or more beauty renders were skipped (non-fatal).
    PassedWithBeautySkipped {
        skipped_shot_ids: Vec<String>,
        reason: String,
    },
    /// One or more overlay PNG hashes did not match the golden.
    FailedTruthValidation { mismatches: Vec<TruthMismatch> },
    /// One or more metrics hashes did not match the golden.
    FailedMetricsValidation { mismatches: Vec<MetricsMismatch> },
    /// A tool-level error prevented the run from completing normally.
    InternalError {
        reason: String,
        kind: InternalErrorKind,
    },
}

impl OverallStatus {
    /// AD9 exit-code contract: 0 for truth-green, 2 for validation failure,
    /// 3 for tool-level error. This is a public contract — downstream shell
    /// scripts `case $?` this directly.
    ///
    /// # Examples
    ///
    /// ```
    /// use app::headless::output::OverallStatus;
    ///
    /// assert_eq!(OverallStatus::Passed.exit_code(), 0);
    /// assert_eq!(
    ///     OverallStatus::PassedWithBeautySkipped {
    ///         skipped_shot_ids: vec!["s1".into()],
    ///         reason: "no GPU".into(),
    ///     }
    ///     .exit_code(),
    ///     0
    /// );
    /// assert_eq!(
    ///     OverallStatus::FailedTruthValidation { mismatches: vec![] }.exit_code(),
    ///     2
    /// );
    /// assert_eq!(
    ///     OverallStatus::InternalError {
    ///         reason: "oops".into(),
    ///         kind: app::headless::output::InternalErrorKind::Io,
    ///     }
    ///     .exit_code(),
    ///     3
    /// );
    /// ```
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::Passed | Self::PassedWithBeautySkipped { .. } => 0,
            Self::FailedTruthValidation { .. } | Self::FailedMetricsValidation { .. } => 2,
            Self::InternalError { .. } => 3,
        }
    }
}

/// Discriminant for [`OverallStatus::InternalError`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum InternalErrorKind {
    Io,
    RonParse,
    SchemaVersionMismatch,
    PresetNotFound,
    PipelineError,
    GpuRuntimeError,
    ShotSetMismatch {
        missing: Vec<String>,
        extra: Vec<String>,
    },
    OverlaySetMismatch {
        shot_id: String,
        missing: Vec<String>,
        extra: Vec<String>,
    },
    /// Fallback for unit variants introduced in a later schema version.
    ///
    /// `#[serde(other)]` is AD9's forward-compat gate: a Sprint 1C binary
    /// reading a Sprint 4 summary that names an unknown variant parses it as
    /// `Other` rather than hard-failing the RON load.
    #[serde(other)]
    Other,
}

/// A single overlay hash mismatch from truth validation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TruthMismatch {
    /// Shot ID that produced the mismatch.
    pub shot_id: String,
    /// Overlay whose PNG hash differed.
    pub overlay_id: String,
    /// Expected (golden) hash.
    pub expected_hash: String,
    /// Actual (produced) hash.
    pub actual_hash: String,
}

/// A single metrics hash mismatch from metrics validation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MetricsMismatch {
    /// Shot ID that produced the mismatch.
    pub shot_id: String,
    /// Expected (golden) hash.
    pub expected_hash: String,
    /// Actual (produced) hash.
    pub actual_hash: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// AD5: Canonical-bytes fingerprint
// ─────────────────────────────────────────────────────────────────────────────

/// Produce the deterministic byte stream used for both `run_id` and
/// `request_fingerprint` (AD5, Step 1).
///
/// The following fields are stripped before serialisation so the fingerprint
/// reflects *what* is being captured, not *where* or *when*:
/// - `run_id`    — the name of the run
/// - `output_dir` — the destination on disk
///
/// Shot and overlay ordering is also normalised so that logically equivalent
/// requests with different list orderings produce the same fingerprint.
pub fn canonical_bytes(req: &CaptureRequest) -> Vec<u8> {
    let mut canonical = req.clone();

    // Strip "where/how" fields.
    canonical.run_id = None;
    canonical.output_dir = None;

    // Sort shots by id, ASCII ascending.
    canonical.shots.sort_by(|a, b| a.id.cmp(&b.id));

    // Within each shot, sort overlay lists.
    for shot in &mut canonical.shots {
        shot.truth.overlays.sort();
        if let Some(beauty) = &mut shot.beauty {
            beauty.overlay_stack.sort();
        }
    }

    ron::ser::to_string_pretty(&canonical, ron::ser::PrettyConfig::default())
        .expect("CaptureRequest is always serialisable to RON")
        .into_bytes()
}

/// Returns the first 16 hex characters of the blake3 hash of
/// [`canonical_bytes`].
///
/// This is used as the default `run_id` when the caller does not supply one.
pub fn compute_run_id(req: &CaptureRequest) -> String {
    let hex = blake3::hash(&canonical_bytes(req)).to_hex().to_string();
    hex[..16].to_owned()
}

/// Returns the full 64-hex blake3 hash of [`canonical_bytes`].
///
/// Stored in [`RunSummary::request_fingerprint`] for later provenance checks.
pub fn compute_request_fingerprint(req: &CaptureRequest) -> String {
    blake3::hash(&canonical_bytes(req)).to_hex().to_string()
}

// ─────────────────────────────────────────────────────────────────────────────
// PNG encoder
// ─────────────────────────────────────────────────────────────────────────────

/// Write an RGBA-8 PNG to `path`.
///
/// Parent directories are created if they do not exist.  No ancillary metadata
/// chunks are written so that the output is byte-deterministic across runs.
///
/// # Errors
///
/// Returns an [`std::io::Error`] on any I/O failure.
pub fn write_rgba8_png(path: &Path, rgba: &[u8], width: u32, height: u32) -> std::io::Result<()> {
    assert_eq!(
        rgba.len(),
        (width * height * 4) as usize,
        "rgba slice length must equal width * height * 4"
    );

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let file = std::fs::File::create(path)?;
    let mut encoder = png::Encoder::new(file, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    // Disable all ancillary chunks for deterministic output.
    encoder.set_compression(png::Compression::Default);
    encoder.set_filter(png::FilterType::Sub);
    let mut writer = encoder.write_header().map_err(png_to_io)?;
    writer.write_image_data(rgba).map_err(png_to_io)?;
    Ok(())
}

fn png_to_io(e: png::EncodingError) -> std::io::Error {
    std::io::Error::other(e.to_string())
}

// ─────────────────────────────────────────────────────────────────────────────
// Directory layout
// ─────────────────────────────────────────────────────────────────────────────

/// Helper that centralises all path construction for a headless run's output
/// directory tree, exactly matching AD4's layout:
///
/// ```text
/// <root>/
/// ├── request.ron
/// ├── summary.ron
/// └── shots/<shot_id>/
///     ├── metrics.ron
///     ├── overlays/<overlay_id>.png
///     └── beauty/scene.png
/// ```
///
/// ```
/// # use std::path::Path;
/// # use app::headless::output::RunLayout;
/// let layout = RunLayout::new("/tmp/my_run");
/// assert_eq!(layout.request_ron(), Path::new("/tmp/my_run/request.ron"));
/// assert_eq!(layout.summary_ron(),  Path::new("/tmp/my_run/summary.ron"));
/// assert_eq!(
///     layout.shot_dir("hero_42"),
///     Path::new("/tmp/my_run/shots/hero_42")
/// );
/// assert_eq!(
///     layout.overlay_png("hero_42", "slope"),
///     Path::new("/tmp/my_run/shots/hero_42/overlays/slope.png")
/// );
/// assert_eq!(
///     layout.metrics_ron("hero_42"),
///     Path::new("/tmp/my_run/shots/hero_42/metrics.ron")
/// );
/// assert_eq!(
///     layout.beauty_png("hero_42"),
///     Path::new("/tmp/my_run/shots/hero_42/beauty/scene.png")
/// );
/// ```
pub struct RunLayout {
    pub root: PathBuf,
}

impl RunLayout {
    /// Create a new layout rooted at `root`.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// `<root>/request.ron`
    pub fn request_ron(&self) -> PathBuf {
        self.root.join("request.ron")
    }

    /// `<root>/summary.ron`
    pub fn summary_ron(&self) -> PathBuf {
        self.root.join("summary.ron")
    }

    /// `<root>/shots/<shot_id>/`
    pub fn shot_dir(&self, shot_id: &str) -> PathBuf {
        self.root.join("shots").join(shot_id)
    }

    /// `<root>/shots/<shot_id>/overlays/`
    pub fn overlays_dir(&self, shot_id: &str) -> PathBuf {
        self.shot_dir(shot_id).join("overlays")
    }

    /// `<root>/shots/<shot_id>/overlays/<overlay_id>.png`
    pub fn overlay_png(&self, shot_id: &str, overlay_id: &str) -> PathBuf {
        self.overlays_dir(shot_id).join(format!("{overlay_id}.png"))
    }

    /// `<root>/shots/<shot_id>/metrics.ron`
    pub fn metrics_ron(&self, shot_id: &str) -> PathBuf {
        self.shot_dir(shot_id).join("metrics.ron")
    }

    /// `<root>/shots/<shot_id>/beauty/`
    pub fn beauty_dir(&self, shot_id: &str) -> PathBuf {
        self.shot_dir(shot_id).join("beauty")
    }

    /// `<root>/shots/<shot_id>/beauty/scene.png`
    pub fn beauty_png(&self, shot_id: &str) -> PathBuf {
        self.beauty_dir(shot_id).join("scene.png")
    }

    /// Create `<root>/shots/<shot_id>/{overlays,beauty}/` (and all parents) on
    /// disk. Call this before writing any per-shot files.
    pub fn create_shot_dirs(&self, shot_id: &str) -> std::io::Result<()> {
        std::fs::create_dir_all(self.overlays_dir(shot_id))?;
        std::fs::create_dir_all(self.beauty_dir(shot_id))?;
        Ok(())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// RON writers
// ─────────────────────────────────────────────────────────────────────────────

/// Write the capture request as pretty RON to `<root>/request.ron`.
///
/// Uses a write-to-temp-then-rename strategy to avoid partial writes.
pub fn write_request_ron(layout: &RunLayout, req: &CaptureRequest) -> std::io::Result<()> {
    let content = ron::ser::to_string_pretty(req, ron::ser::PrettyConfig::default())
        .map_err(|e| std::io::Error::other(e.to_string()))?;
    write_atomic(layout.request_ron(), content.as_bytes())
}

/// Write the run summary as pretty RON to `<root>/summary.ron`.
///
/// Uses a write-to-temp-then-rename strategy to avoid partial writes.
pub fn write_summary_ron(layout: &RunLayout, summary: &RunSummary) -> std::io::Result<()> {
    let content = ron::ser::to_string_pretty(summary, ron::ser::PrettyConfig::default())
        .map_err(|e| std::io::Error::other(e.to_string()))?;
    write_atomic(layout.summary_ron(), content.as_bytes())
}

/// Serialise a [`SummaryMetrics`] snapshot to its canonical RON byte form.
///
/// Returned bytes are *exactly* what [`write_metrics_ron`] writes to disk and
/// what the headless executor feeds into blake3 to compute
/// [`TruthSummary::metrics_hash`]. Keeping these two in one helper guarantees
/// they never drift.
pub fn metrics_ron_bytes(metrics: &SummaryMetrics) -> std::io::Result<Vec<u8>> {
    let s = ron::ser::to_string_pretty(metrics, ron::ser::PrettyConfig::default())
        .map_err(|e| std::io::Error::other(e.to_string()))?;
    Ok(s.into_bytes())
}

/// Write a [`SummaryMetrics`] snapshot as pretty RON to
/// `<root>/shots/<shot_id>/metrics.ron`.
///
/// Returns the canonical bytes that were written (same bytes as
/// [`metrics_ron_bytes`]) so the caller can compute `blake3(...)` without
/// serialising a second time. Uses a write-to-temp-then-rename strategy to
/// avoid partial writes.
pub fn write_metrics_ron(
    layout: &RunLayout,
    shot_id: &str,
    metrics: &SummaryMetrics,
) -> std::io::Result<Vec<u8>> {
    let bytes = metrics_ron_bytes(metrics)?;
    write_atomic(layout.metrics_ron(shot_id), &bytes)?;
    Ok(bytes)
}

/// Write `bytes` to `dest` atomically via a sibling `.tmp` file + rename.
///
/// Calls `sync_all` on the temp file before rename so a mid-write power loss
/// cannot leave a zero-length `summary.ron` that looks valid to consumers.
fn write_atomic(dest: PathBuf, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = dest.with_extension("tmp");
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, &dest)
}

// ─────────────────────────────────────────────────────────────────────────────
// Timestamp helper
// ─────────────────────────────────────────────────────────────────────────────

/// Return the current UTC time as an ISO 8601 string, e.g. `"2026-04-17T12:34:56Z"`.
///
/// Uses only `std::time::SystemTime`; no external crates required.
pub fn now_utc_iso8601() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // Decompose Unix timestamp into calendar fields (Gregorian, proleptic).
    let s = secs % 60;
    let m = (secs / 60) % 60;
    let h = (secs / 3600) % 24;

    // Days since epoch.
    let days = secs / 86400;

    // Shift to a 400-year Gregorian cycle anchor (2000-03-01 = day 10957 in Unix days,
    // but we use the classic algorithm starting from 1970-01-01).
    // We use the "days from civil" inverse mapping (Howard Hinnant's algorithm).
    let z = days as i64 + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097; // day of era [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365; // year of era [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // day of year [0, 365]
    let mp = (5 * doy + 2) / 153; // internal month [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // day [1, 31]
    let mo = if mp < 10 { mp + 3 } else { mp - 9 }; // month [1, 12]
    let y = if mo <= 2 { y + 1 } else { y };

    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{m:02}:{s:02}Z")
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::headless::request::{BeautySpec, CaptureRequest, CaptureShot, TruthSpec};

    // ── helpers ──────────────────────────────────────────────────────────────

    fn make_req(run_id: Option<&str>, shots: Vec<CaptureShot>) -> CaptureRequest {
        CaptureRequest {
            schema_version: 1,
            run_id: run_id.map(str::to_owned),
            output_dir: None,
            shots,
        }
    }

    fn shot(id: &str, overlays: Vec<&str>) -> CaptureShot {
        CaptureShot {
            id: id.to_owned(),
            seed: 42,
            preset: "volcanic_single".to_owned(),
            sim_resolution: 128,
            truth: TruthSpec {
                overlays: overlays.iter().map(|s| s.to_string()).collect(),
                include_metrics: true,
            },
            beauty: None,
        }
    }

    fn shot_with_beauty(id: &str, overlays: Vec<&str>, beauty_overlays: Vec<&str>) -> CaptureShot {
        CaptureShot {
            id: id.to_owned(),
            seed: 42,
            preset: "volcanic_single".to_owned(),
            sim_resolution: 128,
            truth: TruthSpec {
                overlays: overlays.iter().map(|s| s.to_string()).collect(),
                include_metrics: true,
            },
            beauty: Some(BeautySpec {
                camera_preset: "hero".to_owned(),
                overlay_stack: beauty_overlays.iter().map(|s| s.to_string()).collect(),
                resolution: (1280, 800),
            }),
        }
    }

    // ── AD9 exit-code tests ───────────────────────────────────────────────────

    #[test]
    fn exit_code_passed_is_zero() {
        assert_eq!(OverallStatus::Passed.exit_code(), 0);
    }

    #[test]
    fn exit_code_passed_with_beauty_skipped_is_zero() {
        let status = OverallStatus::PassedWithBeautySkipped {
            skipped_shot_ids: vec!["s1".into()],
            reason: "no GPU".into(),
        };
        assert_eq!(status.exit_code(), 0);
    }

    #[test]
    fn exit_code_failed_truth_validation_is_two() {
        let status = OverallStatus::FailedTruthValidation { mismatches: vec![] };
        assert_eq!(status.exit_code(), 2);
    }

    #[test]
    fn exit_code_failed_metrics_validation_is_two() {
        let status = OverallStatus::FailedMetricsValidation { mismatches: vec![] };
        assert_eq!(status.exit_code(), 2);
    }

    #[test]
    fn exit_code_internal_error_is_three() {
        let status = OverallStatus::InternalError {
            reason: "disk full".into(),
            kind: InternalErrorKind::Io,
        };
        assert_eq!(status.exit_code(), 3);
    }

    #[test]
    fn exit_code_internal_error_all_kinds_are_three() {
        let kinds = [
            InternalErrorKind::Io,
            InternalErrorKind::RonParse,
            InternalErrorKind::SchemaVersionMismatch,
            InternalErrorKind::PresetNotFound,
            InternalErrorKind::PipelineError,
            InternalErrorKind::GpuRuntimeError,
            InternalErrorKind::ShotSetMismatch {
                missing: vec![],
                extra: vec![],
            },
            InternalErrorKind::OverlaySetMismatch {
                shot_id: "s".into(),
                missing: vec![],
                extra: vec![],
            },
            InternalErrorKind::Other,
        ];
        for kind in kinds {
            let status = OverallStatus::InternalError {
                reason: "x".into(),
                kind,
            };
            assert_eq!(
                status.exit_code(),
                3,
                "InternalError variant should map to 3"
            );
        }
    }

    #[test]
    fn internal_error_kind_unknown_variant_falls_back_to_other() {
        // AD9 forward-compat: a Sprint 4 summary.ron naming a variant this binary
        // doesn't know about must parse as `Other`, not error out.
        let ron_str = r#"InternalError(
            reason: "from the future",
            kind: FutureVariantWeDoNotKnow,
        )"#;
        let status: OverallStatus =
            ron::de::from_str(ron_str).expect("unknown InternalErrorKind variant must parse");
        match status {
            OverallStatus::InternalError {
                kind: InternalErrorKind::Other,
                ..
            } => {}
            other => panic!("expected InternalError with Other kind, got {other:?}"),
        }
    }

    // ── AD5 canonical bytes / fingerprint tests ───────────────────────────────

    #[test]
    fn canonical_bytes_ignores_run_id() {
        let req_a = make_req(Some("run_alpha"), vec![shot("s", vec!["slope"])]);
        let req_b = make_req(Some("run_beta"), vec![shot("s", vec!["slope"])]);
        assert_eq!(
            canonical_bytes(&req_a),
            canonical_bytes(&req_b),
            "canonical_bytes must be identical when only run_id differs"
        );
    }

    #[test]
    fn canonical_bytes_ignores_output_dir() {
        let mut req_a = make_req(None, vec![shot("s", vec!["slope"])]);
        let mut req_b = make_req(None, vec![shot("s", vec!["slope"])]);
        req_a.output_dir = Some(PathBuf::from("/tmp/a"));
        req_b.output_dir = Some(PathBuf::from("/tmp/b"));
        assert_eq!(
            canonical_bytes(&req_a),
            canonical_bytes(&req_b),
            "canonical_bytes must be identical when only output_dir differs"
        );
    }

    #[test]
    fn canonical_bytes_ignores_shot_order() {
        let req_a = make_req(
            None,
            vec![shot("alpha", vec!["slope"]), shot("beta", vec!["slope"])],
        );
        let req_b = make_req(
            None,
            vec![shot("beta", vec!["slope"]), shot("alpha", vec!["slope"])],
        );
        assert_eq!(
            canonical_bytes(&req_a),
            canonical_bytes(&req_b),
            "canonical_bytes must be identical when shots are in different orders"
        );
    }

    #[test]
    fn canonical_bytes_ignores_overlay_order() {
        let req_a = make_req(None, vec![shot("s", vec!["slope", "final_elevation"])]);
        let req_b = make_req(None, vec![shot("s", vec!["final_elevation", "slope"])]);
        assert_eq!(
            canonical_bytes(&req_a),
            canonical_bytes(&req_b),
            "canonical_bytes must be identical when truth overlays are in different orders"
        );
    }

    #[test]
    fn canonical_bytes_ignores_beauty_overlay_order() {
        let req_a = make_req(
            None,
            vec![shot_with_beauty(
                "s",
                vec!["slope"],
                vec!["river_network", "slope"],
            )],
        );
        let req_b = make_req(
            None,
            vec![shot_with_beauty(
                "s",
                vec!["slope"],
                vec!["slope", "river_network"],
            )],
        );
        assert_eq!(
            canonical_bytes(&req_a),
            canonical_bytes(&req_b),
            "canonical_bytes must be identical when beauty overlay_stack order differs"
        );
    }

    #[test]
    fn run_id_is_prefix_of_fingerprint() {
        let req = make_req(None, vec![shot("s", vec!["slope"])]);
        let run_id = compute_run_id(&req);
        let fingerprint = compute_request_fingerprint(&req);
        assert_eq!(run_id.len(), 16, "run_id must be exactly 16 hex chars");
        assert_eq!(
            fingerprint.len(),
            64,
            "fingerprint must be exactly 64 hex chars"
        );
        assert_eq!(
            &fingerprint[..16],
            run_id,
            "run_id must equal the first 16 chars of request_fingerprint"
        );
    }

    #[test]
    fn fingerprint_is_deterministic() {
        let req = make_req(None, vec![shot("s", vec!["slope"])]);
        assert_eq!(
            compute_request_fingerprint(&req),
            compute_request_fingerprint(&req),
            "fingerprint must be the same on repeated calls"
        );
    }

    // ── PNG encoder test ──────────────────────────────────────────────────────

    #[test]
    fn write_rgba8_png_round_trip() {
        let dir = tempfile::tempdir().expect("tempdir must succeed");
        let path = dir.path().join("test.png");

        // 2×1 image: red pixel then blue pixel.
        let rgba: Vec<u8> = vec![
            255, 0, 0, 255, // red
            0, 0, 255, 255, // blue
        ];
        write_rgba8_png(&path, &rgba, 2, 1).expect("write_rgba8_png must succeed");

        // Decode and verify.
        let file = std::fs::File::open(&path).expect("file must exist after write");
        let decoder = png::Decoder::new(file);
        let mut reader = decoder.read_info().expect("PNG header must be valid");
        let mut buf = vec![0u8; reader.output_buffer_size()];
        let info = reader.next_frame(&mut buf).expect("frame must decode");

        assert_eq!(info.width, 2);
        assert_eq!(info.height, 1);
        assert_eq!(info.color_type, png::ColorType::Rgba);
        assert_eq!(info.bit_depth, png::BitDepth::Eight);
        assert_eq!(&buf[..info.buffer_size()], rgba.as_slice());
    }

    #[test]
    fn write_rgba8_png_creates_parent_dirs() {
        let dir = tempfile::tempdir().expect("tempdir must succeed");
        let path = dir.path().join("a").join("b").join("test.png");
        let rgba = vec![0u8, 0, 0, 255]; // 1×1 black
        write_rgba8_png(&path, &rgba, 1, 1).expect("write must create parent dirs");
        assert!(path.exists());
    }

    // ── RunLayout tests ───────────────────────────────────────────────────────

    #[test]
    fn run_layout_paths() {
        let layout = RunLayout::new("/tmp/my_run");
        assert_eq!(
            layout.request_ron(),
            PathBuf::from("/tmp/my_run/request.ron")
        );
        assert_eq!(
            layout.summary_ron(),
            PathBuf::from("/tmp/my_run/summary.ron")
        );
        assert_eq!(
            layout.shot_dir("hero_42"),
            PathBuf::from("/tmp/my_run/shots/hero_42")
        );
        assert_eq!(
            layout.overlays_dir("hero_42"),
            PathBuf::from("/tmp/my_run/shots/hero_42/overlays")
        );
        assert_eq!(
            layout.overlay_png("hero_42", "slope"),
            PathBuf::from("/tmp/my_run/shots/hero_42/overlays/slope.png")
        );
        assert_eq!(
            layout.metrics_ron("hero_42"),
            PathBuf::from("/tmp/my_run/shots/hero_42/metrics.ron")
        );
        assert_eq!(
            layout.beauty_dir("hero_42"),
            PathBuf::from("/tmp/my_run/shots/hero_42/beauty")
        );
        assert_eq!(
            layout.beauty_png("hero_42"),
            PathBuf::from("/tmp/my_run/shots/hero_42/beauty/scene.png")
        );
    }

    #[test]
    fn create_shot_dirs_creates_directory() {
        let dir = tempfile::tempdir().expect("tempdir must succeed");
        let layout = RunLayout::new(dir.path());
        layout
            .create_shot_dirs("my_shot")
            .expect("create_shot_dirs must succeed");
        assert!(layout.shot_dir("my_shot").is_dir());
        assert!(layout.overlays_dir("my_shot").is_dir());
        assert!(layout.beauty_dir("my_shot").is_dir());
    }

    // ── RON writer tests ──────────────────────────────────────────────────────

    #[test]
    fn write_summary_ron_round_trip() {
        let dir = tempfile::tempdir().expect("tempdir must succeed");
        let layout = RunLayout::new(dir.path());

        let summary = RunSummary {
            schema_version: 1,
            run_id: "abc123".into(),
            request_fingerprint: "a".repeat(64),
            timestamp_utc: "2026-04-17T12:00:00Z".into(),
            shots: vec![ShotSummary {
                id: "s1".into(),
                truth: TruthSummary {
                    overlay_hashes: {
                        let mut m = BTreeMap::new();
                        m.insert("slope".into(), "b".repeat(64));
                        m
                    },
                    metrics_hash: Some("c".repeat(64)),
                },
                beauty: Some(BeautySummary {
                    camera_preset: "hero".into(),
                    status: BeautyStatus::Rendered,
                    byte_hash: Some("d".repeat(64)),
                }),
                pipeline_ms: 1234.5,
                bake_ms: 56.7,
                gpu_render_ms: Some(89.0),
            }],
            overall_status: OverallStatus::Passed,
            warnings: vec!["minor warning".into()],
        };

        write_summary_ron(&layout, &summary).expect("write_summary_ron must succeed");

        let raw = std::fs::read_to_string(layout.summary_ron())
            .expect("summary.ron must exist after write");
        let recovered: RunSummary =
            ron::de::from_str(&raw).expect("summary.ron must deserialise back to RunSummary");

        // Compare fields individually since f64 doesn't derive PartialEq cleanly
        // with NaN, but our values are normal floats.
        assert_eq!(recovered.schema_version, summary.schema_version);
        assert_eq!(recovered.run_id, summary.run_id);
        assert_eq!(recovered.request_fingerprint, summary.request_fingerprint);
        assert_eq!(recovered.timestamp_utc, summary.timestamp_utc);
        assert_eq!(recovered.overall_status, summary.overall_status);
        assert_eq!(recovered.warnings, summary.warnings);
        assert_eq!(recovered.shots.len(), 1);
        let rs = &recovered.shots[0];
        assert_eq!(rs.id, "s1");
        assert_eq!(
            rs.truth.overlay_hashes,
            summary.shots[0].truth.overlay_hashes
        );
        assert_eq!(rs.truth.metrics_hash, summary.shots[0].truth.metrics_hash);
        assert!((rs.pipeline_ms - 1234.5).abs() < 1e-9);
        assert!((rs.bake_ms - 56.7).abs() < 1e-9);
        assert_eq!(rs.gpu_render_ms.map(|v| (v * 10.0).round()), Some(890.0));
        let beauty = rs.beauty.as_ref().unwrap();
        assert_eq!(beauty.camera_preset, "hero");
        assert_eq!(beauty.status, BeautyStatus::Rendered);
        assert_eq!(beauty.byte_hash, Some("d".repeat(64)));
    }

    #[test]
    fn write_request_ron_round_trip() {
        let dir = tempfile::tempdir().expect("tempdir must succeed");
        let layout = RunLayout::new(dir.path());

        let req = make_req(Some("my_run"), vec![shot("s1", vec!["slope", "elevation"])]);
        write_request_ron(&layout, &req).expect("write_request_ron must succeed");

        let raw = std::fs::read_to_string(layout.request_ron())
            .expect("request.ron must exist after write");
        let recovered: CaptureRequest =
            ron::de::from_str(&raw).expect("request.ron must deserialise back to CaptureRequest");
        assert_eq!(recovered, req);
    }

    // ── now_utc_iso8601 test ──────────────────────────────────────────────────

    #[test]
    fn now_utc_iso8601_format() {
        let ts = now_utc_iso8601();
        // Should match YYYY-MM-DDTHH:MM:SSZ
        assert_eq!(ts.len(), 20, "timestamp must be exactly 20 chars: {ts}");
        assert!(ts.ends_with('Z'), "timestamp must end with Z: {ts}");
        assert_eq!(&ts[4..5], "-");
        assert_eq!(&ts[7..8], "-");
        assert_eq!(&ts[10..11], "T");
        assert_eq!(&ts[13..14], ":");
        assert_eq!(&ts[16..17], ":");
        // All digit fields should parse as numbers.
        ts[..4].parse::<u32>().expect("year must be numeric");
        ts[5..7].parse::<u32>().expect("month must be numeric");
        ts[8..10].parse::<u32>().expect("day must be numeric");
        ts[11..13].parse::<u32>().expect("hour must be numeric");
        ts[14..16].parse::<u32>().expect("minute must be numeric");
        ts[17..19].parse::<u32>().expect("second must be numeric");
    }
}
