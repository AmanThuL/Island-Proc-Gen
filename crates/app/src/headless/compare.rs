//! AD5 three-step compare tool: diff a runtime capture against a golden expected
//! directory entirely from the two `summary.ron` files.
//!
//! No PNG files are read. All decisions are based on hash fields inside the RON
//! documents. This means the compare can run without the overlay PNGs or beauty
//! scene PNGs being present on disk — only `summary.ron` is required in each
//! directory.
//!
//! # Three-step semantics
//!
//! 1. **Shape guards** (Step 1) — structural equality of the two summaries:
//!    schema version, shot-id sets, and per-shot overlay-id sets. Any mismatch
//!    returns [`OverallStatus::InternalError`] immediately.  A diverging
//!    `request_fingerprint` is a warning only; it does not fail the comparison.
//!
//! 2. **Truth diff** (Step 2) — authoritative hash equality:
//!    - `overlay_hashes` mismatch → [`OverallStatus::FailedTruthValidation`]
//!    - `metrics_hash` mismatch → [`OverallStatus::FailedMetricsValidation`]
//!
//! 3. **Beauty** (Step 3) — artifact-only; mismatches become
//!    [`tracing::warn!`] lines and may escalate to
//!    [`OverallStatus::PassedWithBeautySkipped`] but never to a failure status.

use std::collections::BTreeSet;
use std::path::Path;

use anyhow::Result;
use tracing::warn;

use crate::headless::output::{
    BeautyStatus, InternalErrorKind, MetricsMismatch, OverallStatus, RunLayout, RunSummary,
    ShotSummary, TruthMismatch,
};

// ─────────────────────────────────────────────────────────────────────────────
// Private helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Find the shot with `id` in `shots`.
///
/// Panics if not found — callers are only valid after the Step 1C set-equality
/// check has guaranteed the id is present on both sides.
fn find_shot<'a>(shots: &'a [ShotSummary], id: &str) -> &'a ShotSummary {
    shots
        .iter()
        .find(|s| s.id == id)
        .unwrap_or_else(|| panic!("shot id {id:?} guaranteed present by set-equality check"))
}

// ─────────────────────────────────────────────────────────────────────────────
// Public entry point
// ─────────────────────────────────────────────────────────────────────────────

/// Diff `run_dir/summary.ron` against `expected_dir/summary.ron` using the
/// AD5 three-step algorithm.
///
/// Returns `(OverallStatus, Vec<String>)` where the second element carries
/// human-readable warning messages (fingerprint divergence, beauty asymmetry,
/// beauty byte-hash divergence) so callers can surface them without relying
/// solely on `tracing` output.
///
/// The [`OverallStatus::exit_code`] follows the AD9 contract (0 / 2 / 3).
/// Never mutates either directory.
///
/// # Errors
///
/// Returns `Err` only for genuinely unexpected conditions (e.g. an
/// `anyhow` chain that would indicate a programming error); recoverable
/// problems (missing file, RON parse failure, shape mismatch, hash mismatch)
/// are all encoded as `Ok((InternalError { .. }, _))` or
/// `Ok((Failed* { .. }, _))` so the caller's exit-code switch stays a single
/// match arm.
pub fn validate(run_dir: &Path, expected_dir: &Path) -> Result<(OverallStatus, Vec<String>)> {
    let mut warnings: Vec<String> = Vec::new();

    // ── Step 1A: load both summary.ron files ─────────────────────────────────
    let run_summary = match load_summary(run_dir) {
        Ok(s) => s,
        Err(e) => return Ok((e, warnings)),
    };
    let expected_summary = match load_summary(expected_dir) {
        Ok(s) => s,
        Err(e) => return Ok((e, warnings)),
    };

    // ── Step 1B: schema version ───────────────────────────────────────────────
    // Sprint 4.A DD2: the binary always writes schema_version 4 even when
    // processing older v1–v3 requests.  The compare tool therefore allows the
    // run side to carry a *higher* version than the expected (golden) side —
    // this is the normal upgrade direction and must not fail.  Only a *lower*
    // run version (reading a v4 summary under a hypothetical older binary) or an
    // equal version (standard case) triggers the mismatch guard.
    if run_summary.schema_version < expected_summary.schema_version {
        let run_v = run_summary.schema_version;
        let exp_v = expected_summary.schema_version;
        return Ok((
            OverallStatus::InternalError {
                reason: format!(
                    "schema_version mismatch: run={run_v} is older than expected={exp_v}"
                ),
                kind: InternalErrorKind::SchemaVersionMismatch,
            },
            warnings,
        ));
    }

    // ── Step 1C: shot-set equality ────────────────────────────────────────────
    let run_ids: BTreeSet<String> = run_summary.shots.iter().map(|s| s.id.clone()).collect();
    let expected_ids: BTreeSet<String> = expected_summary
        .shots
        .iter()
        .map(|s| s.id.clone())
        .collect();

    let missing: Vec<String> = expected_ids.difference(&run_ids).cloned().collect();
    let extra: Vec<String> = run_ids.difference(&expected_ids).cloned().collect();

    if !missing.is_empty() || !extra.is_empty() {
        return Ok((
            OverallStatus::InternalError {
                reason: format!(
                    "shot set mismatch: missing from run={missing:?}, extra in run={extra:?}"
                ),
                kind: InternalErrorKind::ShotSetMismatch { missing, extra },
            },
            warnings,
        ));
    }

    // ── Step 1D: per-shot overlay-set equality ────────────────────────────────
    // Shots are now known to have the same IDs; align by id.
    for expected_shot in &expected_summary.shots {
        let run_shot = find_shot(&run_summary.shots, &expected_shot.id);

        let run_overlay_ids: BTreeSet<&str> = run_shot
            .truth
            .overlay_hashes
            .keys()
            .map(String::as_str)
            .collect();
        let expected_overlay_ids: BTreeSet<&str> = expected_shot
            .truth
            .overlay_hashes
            .keys()
            .map(String::as_str)
            .collect();

        let missing_overlays: Vec<String> = expected_overlay_ids
            .difference(&run_overlay_ids)
            .map(|s| s.to_string())
            .collect();
        let extra_overlays: Vec<String> = run_overlay_ids
            .difference(&expected_overlay_ids)
            .map(|s| s.to_string())
            .collect();

        if !missing_overlays.is_empty() || !extra_overlays.is_empty() {
            return Ok((
                OverallStatus::InternalError {
                    reason: format!(
                        "overlay set mismatch on shot {:?}: missing={missing_overlays:?} extra={extra_overlays:?}",
                        expected_shot.id
                    ),
                    kind: InternalErrorKind::OverlaySetMismatch {
                        shot_id: expected_shot.id.clone(),
                        missing: missing_overlays,
                        extra: extra_overlays,
                    },
                },
                warnings,
            ));
        }
    }

    // ── Step 1E: request fingerprint divergence (warning only) ───────────────
    if run_summary.request_fingerprint != expected_summary.request_fingerprint {
        let expected_fp = &expected_summary.request_fingerprint;
        let actual_fp = &run_summary.request_fingerprint;
        let w = format!("request_fingerprint divergence: {expected_fp} vs {actual_fp}");
        warn!("{}", w);
        warnings.push(w);
    }

    // ── Step 2A: overlay hash diff ────────────────────────────────────────────
    let mut truth_mismatches: Vec<TruthMismatch> = Vec::new();
    for expected_shot in &expected_summary.shots {
        let run_shot = find_shot(&run_summary.shots, &expected_shot.id);

        for (overlay_id, expected_hash) in &expected_shot.truth.overlay_hashes {
            // The overlay key sets are known equal after Step 1D.
            let actual_hash = run_shot
                .truth
                .overlay_hashes
                .get(overlay_id)
                .expect("overlay id guaranteed present after Step 1D");

            if actual_hash != expected_hash {
                truth_mismatches.push(TruthMismatch {
                    shot_id: expected_shot.id.clone(),
                    overlay_id: overlay_id.clone(),
                    expected_hash: expected_hash.clone(),
                    actual_hash: actual_hash.clone(),
                });
            }
        }
    }

    if !truth_mismatches.is_empty() {
        return Ok((
            OverallStatus::FailedTruthValidation {
                mismatches: truth_mismatches,
            },
            warnings,
        ));
    }

    // ── Step 2B: metrics hash diff ────────────────────────────────────────────
    let metrics_mismatches: Vec<MetricsMismatch> = expected_summary
        .shots
        .iter()
        .filter_map(|expected_shot| {
            let run_shot = find_shot(&run_summary.shots, &expected_shot.id);
            metrics_hash_mismatch(run_shot, expected_shot)
        })
        .collect();

    if !metrics_mismatches.is_empty() {
        return Ok((
            OverallStatus::FailedMetricsValidation {
                mismatches: metrics_mismatches,
            },
            warnings,
        ));
    }

    // ── Step 3: beauty (artifact-only, no FAIL escalation) ───────────────────
    step3_beauty(&run_summary, &expected_summary, warnings)
}

// ─────────────────────────────────────────────────────────────────────────────
// Step 3 helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Evaluate beauty results (Step 3) and return `(Passed, warnings)` or
/// `(PassedWithBeautySkipped, warnings)`. Truth/metrics are guaranteed green
/// before this is called.
///
/// `warnings` is the accumulated warning list from earlier steps; this
/// function appends to it and returns it.
fn step3_beauty(
    run_summary: &RunSummary,
    expected_summary: &RunSummary,
    mut warnings: Vec<String>,
) -> Result<(OverallStatus, Vec<String>)> {
    let mut skipped_shot_ids: Vec<String> = Vec::new();

    for expected_shot in &expected_summary.shots {
        let run_shot = find_shot(&run_summary.shots, &expected_shot.id);
        let run_beauty = run_shot.beauty.as_ref();
        let exp_beauty = expected_shot.beauty.as_ref();

        let shot_id = &expected_shot.id;

        // Detect beauty-spec asymmetry: one side has a BeautySpec, the other
        // does not. This is a warning (not a failure) but must be surfaced.
        match (run_beauty, exp_beauty) {
            (Some(_), None) => {
                let w = format!(
                    "beauty-spec asymmetry on {shot_id}: run has BeautySpec but expected does not"
                );
                warn!("{}", w);
                warnings.push(w);
                continue;
            }
            (None, Some(_)) => {
                let w = format!(
                    "beauty-spec asymmetry on {shot_id}: expected has BeautySpec but run does not"
                );
                warn!("{}", w);
                warnings.push(w);
                continue;
            }
            (None, None) => {
                // No beauty spec on either side: nothing to evaluate.
                continue;
            }
            (Some(_), Some(_)) => {
                // Both sides have beauty — fall through to skip/hash checks.
            }
        }

        let mut this_shot_skipped = false;

        // Warn and mark skipped if either side has BeautyStatus::Skipped.
        let emit_skip_warn = |side: &str, status: &BeautyStatus| -> Option<String> {
            if let BeautyStatus::Skipped { reason } = status {
                Some(format!(
                    "beauty skipped on {shot_id} (side: {side}): {reason}"
                ))
            } else {
                None
            }
        };
        if let Some(rb) = run_beauty {
            if let Some(w) = emit_skip_warn("run", &rb.status) {
                warn!("{}", w);
                warnings.push(w);
                this_shot_skipped = true;
            }
        }
        if let Some(eb) = exp_beauty {
            if let Some(w) = emit_skip_warn("expected", &eb.status) {
                warn!("{}", w);
                warnings.push(w);
                this_shot_skipped = true;
            }
        }

        if this_shot_skipped {
            skipped_shot_ids.push(shot_id.clone());
            continue;
        }

        // Both sides rendered: compare byte_hash (divergence is warning-only).
        if let (Some(rb), Some(eb)) = (run_beauty, exp_beauty) {
            if let (Some(rh), Some(eh)) = (&rb.byte_hash, &eb.byte_hash) {
                if rh != eh {
                    let w = format!(
                        "beauty perceptual divergence on {shot_id}: expected={eh} actual={rh}"
                    );
                    warn!("{}", w);
                    warnings.push(w);
                    // Not a failure — AD5 Step 3 beauty is artifact-only.
                }
            }
        }
    }

    if !skipped_shot_ids.is_empty() {
        Ok((
            OverallStatus::PassedWithBeautySkipped {
                skipped_shot_ids,
                reason: "at least one beauty shot skipped on run or expected side".to_owned(),
            },
            warnings,
        ))
    } else {
        Ok((OverallStatus::Passed, warnings))
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Internal helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Load and parse `<dir>/summary.ron`, returning an `InternalError` status on
/// IO or parse failure rather than propagating `anyhow::Error`.
fn load_summary(dir: &Path) -> std::result::Result<RunSummary, OverallStatus> {
    let path = RunLayout::new(dir).summary_ron();
    let text = std::fs::read_to_string(&path).map_err(|e| OverallStatus::InternalError {
        reason: format!("read {path:?} failed: {e}"),
        kind: InternalErrorKind::Io,
    })?;
    ron::de::from_str::<RunSummary>(&text).map_err(|e| OverallStatus::InternalError {
        reason: format!("parse {path:?} as RunSummary failed: {e}"),
        kind: InternalErrorKind::RonParse,
    })
}

/// Compare the `metrics_hash` fields of two aligned shots.
///
/// Returns `None` when the hashes match (both `Some` equal, or both `None`).
/// Returns `Some(MetricsMismatch)` in all other cases, including the mixed
/// `(Some, None)` and `(None, Some)` cases.
fn metrics_hash_mismatch(
    run_shot: &ShotSummary,
    expected_shot: &ShotSummary,
) -> Option<MetricsMismatch> {
    let r = &run_shot.truth.metrics_hash;
    let e = &expected_shot.truth.metrics_hash;

    match (r, e) {
        (Some(rh), Some(eh)) if rh == eh => None,
        (None, None) => None,
        _ => Some(MetricsMismatch {
            shot_id: expected_shot.id.clone(),
            expected_hash: e.as_deref().unwrap_or("<none>").to_owned(),
            actual_hash: r.as_deref().unwrap_or("<none>").to_owned(),
        }),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::headless::output::{
        BeautySummary, RunLayout, RunSummary, ShotSummary, TruthSummary, write_summary_ron,
    };

    // ── Test helpers ──────────────────────────────────────────────────────────

    /// Write a `RunSummary` as `summary.ron` inside `dir` and return `dir` for
    /// use as the path argument to `validate`.
    fn write_summary(dir: &Path, summary: &RunSummary) {
        let layout = RunLayout::new(dir);
        write_summary_ron(&layout, summary).expect("write_summary_ron must succeed in tests");
    }

    /// Build a minimal `RunSummary` with one shot having a single overlay hash.
    fn make_summary(schema_version: u32, fingerprint: &str, shots: Vec<ShotSummary>) -> RunSummary {
        RunSummary {
            schema_version,
            run_id: "test_run".into(),
            request_fingerprint: fingerprint.to_owned(),
            timestamp_utc: "2026-04-17T12:00:00Z".into(),
            shots,
            overall_status: OverallStatus::Passed,
            warnings: vec![],
        }
    }

    fn make_shot(
        id: &str,
        overlay_hashes: BTreeMap<String, String>,
        metrics_hash: Option<String>,
    ) -> ShotSummary {
        ShotSummary {
            id: id.to_owned(),
            truth: TruthSummary {
                overlay_hashes,
                metrics_hash,
            },
            beauty: None,
            pipeline_ms: 0.0,
            bake_ms: 0.0,
            gpu_render_ms: None,
            stage_timings: BTreeMap::new(),
        }
    }

    fn single_overlay(id: &str, hash: &str) -> BTreeMap<String, String> {
        BTreeMap::from([(id.to_owned(), hash.to_owned())])
    }

    fn hash(n: u8) -> String {
        format!("{:0>64}", n)
    }

    fn make_shot_with_beauty(
        id: &str,
        overlay_hashes: BTreeMap<String, String>,
        beauty_status: BeautyStatus,
        byte_hash: Option<String>,
    ) -> ShotSummary {
        ShotSummary {
            id: id.to_owned(),
            truth: TruthSummary {
                overlay_hashes,
                metrics_hash: None,
            },
            beauty: Some(BeautySummary {
                camera_preset: "hero".to_owned(),
                status: beauty_status,
                byte_hash,
            }),
            pipeline_ms: 0.0,
            bake_ms: 0.0,
            gpu_render_ms: None,
            stage_timings: BTreeMap::new(),
        }
    }

    // ── Test helpers (shared across tests) ───────────────────────────────────

    /// Write `run` and `expected` summaries to two temp dirs and call
    /// `validate`. Returns `(OverallStatus, warnings)`.
    fn validate_via_tempdirs(
        run: &RunSummary,
        expected: &RunSummary,
    ) -> (OverallStatus, Vec<String>) {
        let run_dir = tempfile::tempdir().expect("tempdir");
        let exp_dir = tempfile::tempdir().expect("tempdir");
        write_summary(run_dir.path(), run);
        write_summary(exp_dir.path(), expected);
        validate(run_dir.path(), exp_dir.path()).expect("validate must not Err")
    }

    // ── Tests ─────────────────────────────────────────────────────────────────

    #[test]
    fn validate_both_passed_returns_passed() {
        let shot = make_shot("s1", single_overlay("slope", &hash(1)), Some(hash(2)));
        let summary = make_summary(1, &hash(10), vec![shot]);
        let (status, _warnings) = validate_via_tempdirs(&summary, &summary);
        assert_eq!(
            status,
            OverallStatus::Passed,
            "identical summaries must Passed"
        );
    }

    #[test]
    fn validate_shot_set_mismatch_returns_internal_error() {
        // run has two shots; expected has only one with a different id
        let run_summary = make_summary(
            1,
            &hash(10),
            vec![
                make_shot("shot_a", single_overlay("slope", &hash(1)), None),
                make_shot("shot_b", single_overlay("slope", &hash(1)), None),
            ],
        );
        let exp_summary = make_summary(
            1,
            &hash(10),
            vec![make_shot("shot_c", single_overlay("slope", &hash(1)), None)],
        );

        let (status, _warnings) = validate_via_tempdirs(&run_summary, &exp_summary);
        match status {
            OverallStatus::InternalError {
                kind: InternalErrorKind::ShotSetMismatch { missing, extra },
                ..
            } => {
                // "shot_c" is in expected but not in run → missing
                assert!(
                    missing.contains(&"shot_c".to_owned()),
                    "missing should contain shot_c, got {missing:?}"
                );
                // "shot_a" and "shot_b" are in run but not expected → extra
                assert!(
                    extra.contains(&"shot_a".to_owned()),
                    "extra should contain shot_a, got {extra:?}"
                );
                assert!(
                    extra.contains(&"shot_b".to_owned()),
                    "extra should contain shot_b, got {extra:?}"
                );
            }
            other => panic!("expected InternalError(ShotSetMismatch), got {other:?}"),
        }
    }

    #[test]
    fn validate_overlay_set_mismatch_returns_internal_error() {
        // Both have shot "s1", but different overlay keys.
        let run_overlays = BTreeMap::from([("slope".to_owned(), hash(1))]);
        let exp_overlays = BTreeMap::from([("elevation".to_owned(), hash(1))]);

        let run_summary = make_summary(1, &hash(10), vec![make_shot("s1", run_overlays, None)]);
        let exp_summary = make_summary(1, &hash(10), vec![make_shot("s1", exp_overlays, None)]);

        let (status, _warnings) = validate_via_tempdirs(&run_summary, &exp_summary);
        match status {
            OverallStatus::InternalError {
                kind:
                    InternalErrorKind::OverlaySetMismatch {
                        shot_id,
                        missing,
                        extra,
                    },
                ..
            } => {
                assert_eq!(shot_id, "s1");
                assert!(
                    missing.contains(&"elevation".to_owned()),
                    "missing should contain elevation, got {missing:?}"
                );
                assert!(
                    extra.contains(&"slope".to_owned()),
                    "extra should contain slope, got {extra:?}"
                );
            }
            other => panic!("expected InternalError(OverlaySetMismatch), got {other:?}"),
        }
    }

    #[test]
    fn validate_schema_version_mismatch_returns_internal_error() {
        let shot = make_shot("s1", single_overlay("slope", &hash(1)), None);
        let run_summary = make_summary(1, &hash(10), vec![shot.clone()]);
        let exp_summary = make_summary(2, &hash(10), vec![shot]);

        let (status, _warnings) = validate_via_tempdirs(&run_summary, &exp_summary);
        match status {
            OverallStatus::InternalError {
                kind: InternalErrorKind::SchemaVersionMismatch,
                ..
            } => {}
            other => panic!("expected InternalError(SchemaVersionMismatch), got {other:?}"),
        }
    }

    #[test]
    fn validate_truth_hash_mismatch_returns_failed_truth_validation() {
        // Both have "slope" but with different hashes.
        let run_summary = make_summary(
            1,
            &hash(10),
            vec![make_shot("s1", single_overlay("slope", &hash(1)), None)],
        );
        let exp_summary = make_summary(
            1,
            &hash(10),
            vec![make_shot("s1", single_overlay("slope", &hash(2)), None)],
        );

        let (status, _warnings) = validate_via_tempdirs(&run_summary, &exp_summary);
        match status {
            OverallStatus::FailedTruthValidation { mismatches } => {
                assert_eq!(mismatches.len(), 1);
                assert_eq!(mismatches[0].shot_id, "s1");
                assert_eq!(mismatches[0].overlay_id, "slope");
                assert_eq!(mismatches[0].expected_hash, hash(2));
                assert_eq!(mismatches[0].actual_hash, hash(1));
            }
            other => panic!("expected FailedTruthValidation, got {other:?}"),
        }
    }

    #[test]
    fn validate_metrics_hash_mismatch_returns_failed_metrics_validation() {
        // Overlay hashes match; metrics_hash differs.
        let run_summary = make_summary(
            1,
            &hash(10),
            vec![make_shot(
                "s1",
                single_overlay("slope", &hash(1)),
                Some(hash(5)),
            )],
        );
        let exp_summary = make_summary(
            1,
            &hash(10),
            vec![make_shot(
                "s1",
                single_overlay("slope", &hash(1)),
                Some(hash(6)),
            )],
        );

        let (status, _warnings) = validate_via_tempdirs(&run_summary, &exp_summary);
        match status {
            OverallStatus::FailedMetricsValidation { mismatches } => {
                assert_eq!(mismatches.len(), 1);
                assert_eq!(mismatches[0].shot_id, "s1");
                assert_eq!(mismatches[0].expected_hash, hash(6));
                assert_eq!(mismatches[0].actual_hash, hash(5));
            }
            other => panic!("expected FailedMetricsValidation, got {other:?}"),
        }
    }

    #[test]
    fn validate_request_fingerprint_divergence_is_warning_not_fail() {
        let shot = make_shot("s1", single_overlay("slope", &hash(1)), Some(hash(2)));
        // Only the request_fingerprint differs; everything else matches.
        let run_summary = make_summary(1, &hash(10), vec![shot.clone()]);
        let exp_summary = make_summary(1, &hash(11), vec![shot]);

        let (status, warnings) = validate_via_tempdirs(&run_summary, &exp_summary);
        // Must pass (not fail) despite the fingerprint divergence.
        assert!(
            matches!(
                status,
                OverallStatus::Passed | OverallStatus::PassedWithBeautySkipped { .. }
            ),
            "request_fingerprint divergence must not fail; got {status:?}"
        );
        // The warning must be surfaced to the caller.
        assert!(
            warnings
                .iter()
                .any(|w| w.contains("request_fingerprint divergence")),
            "fingerprint divergence warning must appear in returned warnings; got {warnings:?}"
        );
    }

    #[test]
    fn validate_beauty_skipped_on_one_side_returns_passed_with_beauty_skipped() {
        let overlays = single_overlay("slope", &hash(1));

        // Run side: beauty Rendered; expected side: beauty Skipped.
        let run_shot = make_shot_with_beauty(
            "shot_a",
            overlays.clone(),
            BeautyStatus::Rendered,
            Some(hash(99)),
        );
        let exp_shot = make_shot_with_beauty(
            "shot_a",
            overlays,
            BeautyStatus::Skipped {
                reason: "no GPU on CI".to_owned(),
            },
            None,
        );

        let (status, _warnings) = validate_via_tempdirs(
            &make_summary(1, &hash(10), vec![run_shot]),
            &make_summary(1, &hash(10), vec![exp_shot]),
        );
        match status {
            OverallStatus::PassedWithBeautySkipped {
                skipped_shot_ids,
                reason: _,
            } => {
                assert_eq!(skipped_shot_ids, vec!["shot_a".to_owned()]);
            }
            other => panic!("expected PassedWithBeautySkipped, got {other:?}"),
        }
    }

    #[test]
    fn validate_beauty_byte_hash_differs_is_warning_not_fail() {
        let overlays = single_overlay("slope", &hash(1));
        let run_shot = make_shot_with_beauty(
            "s1",
            overlays.clone(),
            BeautyStatus::Rendered,
            Some(hash(50)),
        );
        let exp_shot =
            make_shot_with_beauty("s1", overlays, BeautyStatus::Rendered, Some(hash(51)));

        let (status, _warnings) = validate_via_tempdirs(
            &make_summary(1, &hash(10), vec![run_shot]),
            &make_summary(1, &hash(10), vec![exp_shot]),
        );
        // Beauty byte_hash divergence must not escalate to a failure status.
        assert_eq!(
            status,
            OverallStatus::Passed,
            "beauty byte_hash divergence must not fail; got {status:?}"
        );
    }

    #[test]
    fn validate_missing_expected_summary_returns_internal_error_io() {
        let run_dir = tempfile::tempdir().expect("tempdir");
        let exp_dir = tempfile::tempdir().expect("tempdir");

        // Write only the run summary; leave expected dir empty.
        let shot = make_shot("s1", single_overlay("slope", &hash(1)), None);
        write_summary(run_dir.path(), &make_summary(1, &hash(10), vec![shot]));
        // Note: no write_summary for exp_dir — file is absent.

        let (status, _warnings) =
            validate(run_dir.path(), exp_dir.path()).expect("validate must not Err");
        match status {
            OverallStatus::InternalError {
                kind: InternalErrorKind::Io,
                ..
            } => {}
            other => panic!("expected InternalError(Io), got {other:?}"),
        }
    }

    #[test]
    fn validate_does_not_require_png_files_to_exist() {
        // Create summary.ron files that reference overlays, but do NOT write any
        // PNG files — validate must succeed purely from the RON.
        let overlays = BTreeMap::from([
            ("slope".to_owned(), hash(1)),
            ("final_elevation".to_owned(), hash(2)),
        ]);
        let shot = make_shot("s1", overlays, Some(hash(3)));
        let summary = make_summary(1, &hash(10), vec![shot]);

        let run_dir = tempfile::tempdir().expect("tempdir");
        let exp_dir = tempfile::tempdir().expect("tempdir");
        write_summary(run_dir.path(), &summary);
        write_summary(exp_dir.path(), &summary);

        // Confirm the expected PNG paths do NOT exist.
        assert!(!run_dir.path().join("shots/s1/overlays/slope.png").exists());
        assert!(!exp_dir.path().join("shots/s1/overlays/slope.png").exists());

        let (status, _warnings) =
            validate(run_dir.path(), exp_dir.path()).expect("validate must not Err");
        assert_eq!(
            status,
            OverallStatus::Passed,
            "validate must not require PNG files to exist; got {status:?}"
        );
    }

    // ── I2: Step-ordering proof ───────────────────────────────────────────────

    #[test]
    fn validate_truth_mismatch_takes_precedence_over_metrics_mismatch() {
        // Both the overlay hash AND the metrics hash differ. Step 2A must
        // early-return FailedTruthValidation; Step 2B (metrics) must not fire.
        // This locks the AD5 step ordering: truth > metrics.
        //
        // If a future refactor reorders Step 2A and Step 2B, this test goes red.
        let run_summary = make_summary(
            1,
            &hash(10),
            vec![make_shot(
                "s1",
                single_overlay("slope", &"run_overlay_hash".repeat(4)),
                Some("run_metrics_hash".repeat(4)),
            )],
        );
        let exp_summary = make_summary(
            1,
            &hash(10),
            vec![make_shot(
                "s1",
                single_overlay("slope", &"exp_overlay_hash".repeat(4)),
                Some("exp_metrics_hash".repeat(4)),
            )],
        );
        let (status, _warnings) = validate_via_tempdirs(&run_summary, &exp_summary);
        match status {
            OverallStatus::FailedTruthValidation { mismatches } => {
                assert_eq!(mismatches.len(), 1);
                assert_eq!(mismatches[0].overlay_id, "slope");
            }
            other => panic!("expected FailedTruthValidation, got {other:?}"),
        }
    }

    // ── I3: Beauty-spec asymmetry ─────────────────────────────────────────────

    #[test]
    fn validate_beauty_spec_asymmetry_is_warning_not_fail() {
        // Run has beauty Some, expected has beauty None. Truth matches.
        // Must return Passed + warnings containing "beauty-spec asymmetry".
        let overlays = single_overlay("slope", &hash(1));

        // run side has a BeautySpec; expected side does not.
        let run_shot = make_shot_with_beauty(
            "s1",
            overlays.clone(),
            BeautyStatus::Rendered,
            Some(hash(77)),
        );
        let exp_shot = make_shot("s1", overlays, None);

        let (status, warnings) = validate_via_tempdirs(
            &make_summary(1, &hash(10), vec![run_shot]),
            &make_summary(1, &hash(10), vec![exp_shot]),
        );
        assert!(
            matches!(
                status,
                OverallStatus::Passed | OverallStatus::PassedWithBeautySkipped { .. }
            ),
            "beauty-spec asymmetry must not fail; got {status:?}"
        );
        assert!(
            warnings.iter().any(|w| w.contains("beauty-spec asymmetry")),
            "asymmetry warning must appear in returned warnings; got {warnings:?}"
        );
    }

    // ── Sprint 4.A AD8 whitelist tests ────────────────────────────────────────

    /// Sprint 4.A: `stage_timings.*` values are timing measurements that vary
    /// per-run and per-machine. The compare tool must NOT fail when two
    /// summaries differ only in `stage_timings` — those fields are inherently
    /// not compared (the compare tool only checks overlay_hashes + metrics_hash).
    ///
    /// This test writes two summaries with identical truth data but different
    /// `stage_timings` and asserts the comparison yields exit-0 (Passed).
    #[test]
    fn ad8_whitelist_includes_stage_timings_fields() {
        use island_core::pipeline::StageTiming;

        let overlays = single_overlay("slope", &hash(1));

        // Shot A: stage_timings with specific cpu_ms values.
        let mut shot_a = make_shot("s1", overlays.clone(), Some(hash(2)));
        shot_a.stage_timings.insert(
            "TopographyStage".to_owned(),
            StageTiming {
                cpu_ms: 5.0,
                gpu_ms: None,
            },
        );
        shot_a.stage_timings.insert(
            "CoastMaskStage".to_owned(),
            StageTiming {
                cpu_ms: 2.0,
                gpu_ms: None,
            },
        );

        // Shot B: same truth, different timing values.
        let mut shot_b = make_shot("s1", overlays, Some(hash(2)));
        shot_b.stage_timings.insert(
            "TopographyStage".to_owned(),
            StageTiming {
                cpu_ms: 99.0,
                gpu_ms: Some(10.0),
            }, // very different
        );
        shot_b.stage_timings.insert(
            "CoastMaskStage".to_owned(),
            StageTiming {
                cpu_ms: 0.1,
                gpu_ms: None,
            },
        );

        let (status, _warnings) = validate_via_tempdirs(
            &make_summary(1, &hash(10), vec![shot_a]),
            &make_summary(1, &hash(10), vec![shot_b]),
        );
        assert_eq!(
            status,
            OverallStatus::Passed,
            "differing stage_timings must not fail compare; got {status:?}"
        );
    }

    /// Sprint 4.A DD2: a run with schema_version=4 against a baseline at
    /// schema_version=3 must succeed (upgrade direction: run >= expected).
    #[test]
    fn schema_version_upgrade_direction_passes() {
        let shot = make_shot("s1", single_overlay("slope", &hash(1)), Some(hash(2)));
        // Run = v4 (fresh binary output), expected = v3 (stored baseline).
        let run_summary = make_summary(4, &hash(10), vec![shot.clone()]);
        let exp_summary = make_summary(3, &hash(10), vec![shot]);

        let (status, _warnings) = validate_via_tempdirs(&run_summary, &exp_summary);
        assert_eq!(
            status,
            OverallStatus::Passed,
            "run(v4) vs expected(v3) must pass — upgrade direction; got {status:?}"
        );
    }

    /// Sprint 4.A DD2: run schema_version OLDER than expected must fail with
    /// SchemaVersionMismatch.
    #[test]
    fn schema_version_downgrade_direction_fails() {
        let shot = make_shot("s1", single_overlay("slope", &hash(1)), Some(hash(2)));
        // Run = v2 (hypothetical old binary), expected = v4 (stored baseline).
        let run_summary = make_summary(2, &hash(10), vec![shot.clone()]);
        let exp_summary = make_summary(4, &hash(10), vec![shot]);

        let (status, _warnings) = validate_via_tempdirs(&run_summary, &exp_summary);
        match status {
            OverallStatus::InternalError {
                kind: InternalErrorKind::SchemaVersionMismatch,
                ..
            } => {}
            other => {
                panic!("run(v2) vs expected(v4) must fail SchemaVersionMismatch; got {other:?}")
            }
        }
    }
}
