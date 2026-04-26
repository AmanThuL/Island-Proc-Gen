//! Sprint 4.A forward-compat integration tests.
//!
//! These tests exercise schema compatibility between v3 baseline RON
//! files (which lack the `stage_timings` field) and the v4 binary's
//! `RunSummary` / `ShotSummary` types. The fixtures are **statically
//! embedded** RON strings, NOT live baseline files: Sprint 4.B's
//! cascade regen lifts the on-disk baselines to v4, after which a
//! test reading from `crates/data/golden/headless/<x>/summary.ron`
//! would see v4 RON with populated `stage_timings`. The point of the
//! forward-compat test is the parse contract, which is independent of
//! the live baselines' current schema.
//!
//! If `RunSummary` / `ShotSummary` / `TruthSummary` shape changes in a
//! way that breaks v3 parse, these tests fire — that is the contract
//! the `#[serde(default)]` annotation on `stage_timings` is meant to
//! preserve forever.

use app::headless::output::{RunSummary, ShotSummary};

/// A minimal v3-style `summary.ron` fixture — schema_version 3, two
/// shots, no `stage_timings` field anywhere. Mirrors the shape that
/// Sprint 1C / 2 / 3.5 binaries wrote.
const V3_FIXTURE: &str = r#"(
    schema_version: 3,
    run_id: "v3_fixture",
    request_fingerprint: "0000000000000000000000000000000000000000000000000000000000000000",
    timestamp_utc: "2026-04-20T00:00:00Z",
    shots: [
        (
            id: "shot_a",
            truth: (
                overlay_hashes: {
                    "final_elevation": "deadbeef",
                },
                metrics_hash: Some("cafebabe"),
            ),
            beauty: None,
            pipeline_ms: 12.34,
            bake_ms: 5.67,
            gpu_render_ms: Some(8.90),
        ),
        (
            id: "shot_b",
            truth: (
                overlay_hashes: {},
                metrics_hash: None,
            ),
            beauty: None,
            pipeline_ms: 10.0,
            bake_ms: 5.0,
            gpu_render_ms: None,
        ),
    ],
    overall_status: Passed,
    warnings: [],
)
"#;

/// Parse a static v3-style fixture under the v4 binary's `RunSummary` type.
///
/// This is the single load-bearing forward-compat assertion. If
/// `ShotSummary.stage_timings` ever loses its `#[serde(default)]` (or if
/// any other field gains a non-default-able mandatory addition), this test
/// fires.
#[test]
fn v3_summary_parses_under_v4_binary_with_empty_stage_timings() {
    let summary: RunSummary =
        ron::de::from_str(V3_FIXTURE).expect("static v3 fixture must parse under v4 binary");

    assert_eq!(
        summary.schema_version, 3,
        "static v3 fixture parses as schema_version 3"
    );
    assert_eq!(summary.shots.len(), 2);

    for shot in &summary.shots {
        assert!(
            shot.stage_timings.is_empty(),
            "shot '{}': v3 fixture must yield empty stage_timings via #[serde(default)]; got {:?}",
            shot.id,
            shot.stage_timings
        );
    }
}

/// `ShotSummary` has a stable field set: verify the struct compiles with all
/// required fields and `stage_timings` has the correct type.
///
/// This is a compile-time anchor test — the body is trivial but the struct
/// literal will fail to compile if any field is added/removed/renamed.
#[test]
fn shot_summary_field_set_includes_stage_timings() {
    use app::headless::output::{BeautySummary, TruthSummary};
    use island_core::pipeline::StageTiming;
    use std::collections::BTreeMap;

    let s = ShotSummary {
        id: "test".into(),
        truth: TruthSummary {
            overlay_hashes: BTreeMap::new(),
            metrics_hash: None,
        },
        beauty: None::<BeautySummary>,
        pipeline_ms: 0.0,
        bake_ms: 0.0,
        gpu_render_ms: None,
        stage_timings: {
            let mut m = BTreeMap::new();
            m.insert(
                "stage".to_owned(),
                StageTiming {
                    cpu_ms: 1.0,
                    gpu_ms: None,
                },
            );
            m
        },
    };
    assert_eq!(s.stage_timings.len(), 1);
}
