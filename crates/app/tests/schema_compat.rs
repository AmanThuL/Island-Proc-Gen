//! Sprint 4.A forward-compat integration tests.
//!
//! These tests exercise schema compatibility between v3 golden baselines and
//! the v4 binary. They use real on-disk `summary.ron` files from the checked-in
//! baselines to confirm that parsing works under the v4 `ShotSummary` schema
//! (which added the `stage_timings` field with `#[serde(default)]`).

use app::headless::output::{RunSummary, ShotSummary};

/// Parse the real sprint_3_5_hex_surface v3 `summary.ron` under the v4
/// binary's `RunSummary` type.
///
/// Asserts:
/// - `schema_version == 3` (the file keeps its original version)
/// - `stage_timings.is_empty()` for all shots (absent field → default empty map)
/// - No parse error (forward-compat via `#[serde(default)]`)
#[test]
fn v3_summary_parses_under_v4_binary_with_empty_stage_timings() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../data/golden/headless/sprint_3_5_hex_surface/summary.ron"
    );

    let text =
        std::fs::read_to_string(path).unwrap_or_else(|e| panic!("failed to read {path}: {e}"));

    let summary: RunSummary =
        ron::de::from_str(&text).expect("sprint_3_5_hex_surface/summary.ron must parse");

    // The on-disk file is schema_version 3.
    assert_eq!(
        summary.schema_version, 3,
        "on-disk v3 baseline must still report schema_version 3 when parsed"
    );

    // All shots must have empty stage_timings (v3 had no such field).
    for shot in &summary.shots {
        assert!(
            shot.stage_timings.is_empty(),
            "shot '{}': v3 baseline must parse with empty stage_timings under v4 binary; \
             got {:?}",
            shot.id,
            shot.stage_timings
        );
    }

    // The file must have at least one shot (sanity).
    assert!(
        !summary.shots.is_empty(),
        "sprint_3_5_hex_surface baseline must have at least one shot"
    );
}

/// Parse the sprint_1a_baseline v3 summary and confirm forward-compat.
#[test]
fn v3_sprint_1a_summary_parses_with_empty_stage_timings() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../data/golden/headless/sprint_1a_baseline/summary.ron"
    );

    let text =
        std::fs::read_to_string(path).unwrap_or_else(|e| panic!("failed to read {path}: {e}"));

    let summary: RunSummary =
        ron::de::from_str(&text).expect("sprint_1a_baseline/summary.ron must parse");

    for shot in &summary.shots {
        assert!(
            shot.stage_timings.is_empty(),
            "shot '{}': v3 baseline must parse with empty stage_timings",
            shot.id
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
