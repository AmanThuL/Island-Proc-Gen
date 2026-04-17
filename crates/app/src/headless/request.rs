use std::path::PathBuf;

/// A headless capture request, described in RON format and passed to
/// `cargo run -p app -- --headless <request.ron>`.
///
/// # Example RON document
///
/// ```ron
/// (
///     schema_version: 1,
///     run_id: Some("my_run"),
///     output_dir: None,
///     shots: [
///         (
///             id: "hero_seed_42",
///             seed: 42,
///             preset: "volcanic_single",
///             sim_resolution: 128,
///             truth: (
///                 overlays: ["final_elevation", "river_network"],
///                 include_metrics: true,
///             ),
///             beauty: None,
///         ),
///     ],
/// )
/// ```
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct CaptureRequest {
    /// Schema version. Always `1` for Sprint 1C. Bump only on breaking changes.
    pub schema_version: u32,

    /// Stable identifier for this run.
    ///
    /// When `None` the executor will assign
    /// `blake3(canonical_bytes(request))[0..16]` (hex, no timestamp component),
    /// making the same request deterministically produce the same `run_id` and
    /// therefore the same output directory.  Supply an explicit value when you
    /// want two semantically different captures to coexist side-by-side.
    #[serde(default)]
    pub run_id: Option<String>,

    /// Root directory for runtime outputs.
    ///
    /// Defaults to `captures/headless/<run_id>/` when `None`.  The same
    /// `run_id` always clobbers the previous contents by design; use an
    /// explicit `run_id` override if you need multiple runs to persist.
    #[serde(default)]
    pub output_dir: Option<PathBuf>,

    /// The ordered list of captures to execute.
    ///
    /// Intentionally required (no `#[serde(default)]`): a missing `shots` field
    /// is almost always a malformed request, and an empty list would be a
    /// silent no-op.  Callers who truly want "run nothing" must write `shots: []`.
    pub shots: Vec<CaptureShot>,
}

/// A single capture unit: one `(seed, preset, resolution)` combination.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct CaptureShot {
    /// Unique identifier within the request; becomes the shot sub-directory name.
    pub id: String,

    /// RNG seed for the simulation.
    pub seed: u64,

    /// Preset name, resolved via `data::presets::load_preset`.
    pub preset: String,

    /// Simulation grid side length (e.g. 64, 128, 256, 512).
    pub sim_resolution: u32,

    /// CPU-side deterministic overlay exports.
    pub truth: TruthSpec,

    /// GPU offscreen beauty capture.  `None` skips the GPU render for this shot.
    #[serde(default)]
    pub beauty: Option<BeautySpec>,
}

/// Specification for the deterministic CPU truth path.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct TruthSpec {
    /// Overlay IDs to export; each must exist in the 12-descriptor registry.
    pub overlays: Vec<String>,

    /// When `true` (default), write a `metrics.ron` alongside the overlay PNGs.
    #[serde(default = "default_include_metrics")]
    pub include_metrics: bool,
}

fn default_include_metrics() -> bool {
    true
}

/// Specification for the GPU offscreen beauty capture.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct BeautySpec {
    /// Camera preset: `"hero"`, `"top_debug"`, or `"low_oblique"`.
    pub camera_preset: String,

    /// Overlay IDs to composite on the beauty shot; may be empty.
    pub overlay_stack: Vec<String>,

    /// Output image dimensions in pixels.  Defaults to `(1280, 800)`.
    #[serde(default = "default_beauty_resolution")]
    pub resolution: (u32, u32),
}

fn default_beauty_resolution() -> (u32, u32) {
    (1280, 800)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn full_request() -> CaptureRequest {
        CaptureRequest {
            schema_version: 1,
            run_id: Some("test_run".to_owned()),
            output_dir: Some(PathBuf::from("captures/headless/test_run")),
            shots: vec![
                CaptureShot {
                    id: "shot_with_beauty".to_owned(),
                    seed: 42,
                    preset: "volcanic_single".to_owned(),
                    sim_resolution: 128,
                    truth: TruthSpec {
                        overlays: vec!["final_elevation".to_owned(), "river_network".to_owned()],
                        include_metrics: true,
                    },
                    beauty: Some(BeautySpec {
                        camera_preset: "hero".to_owned(),
                        overlay_stack: vec!["river_network".to_owned()],
                        resolution: (1280, 800),
                    }),
                },
                CaptureShot {
                    id: "shot_truth_only".to_owned(),
                    seed: 99,
                    preset: "caldera".to_owned(),
                    sim_resolution: 256,
                    truth: TruthSpec {
                        overlays: vec!["slope".to_owned()],
                        include_metrics: false,
                    },
                    beauty: None,
                },
            ],
        }
    }

    #[test]
    fn serialize_and_deserialize_round_trip() {
        let original = full_request();
        let ron_str = ron::ser::to_string_pretty(&original, ron::ser::PrettyConfig::default())
            .expect("serialization must succeed");
        let recovered: CaptureRequest =
            ron::de::from_str(&ron_str).expect("deserialization must succeed");
        assert_eq!(original, recovered);
    }

    #[test]
    fn defaults_for_optional_fields() {
        // Minimal RON: omit run_id, output_dir, beauty, and include_metrics.
        let ron_str = r#"(
            schema_version: 1,
            shots: [
                (
                    id: "minimal",
                    seed: 1,
                    preset: "volcanic_single",
                    sim_resolution: 64,
                    truth: (
                        overlays: ["final_elevation"],
                    ),
                ),
            ],
        )"#;

        let req: CaptureRequest =
            ron::de::from_str(ron_str).expect("deserialization of minimal doc must succeed");

        assert_eq!(req.run_id, None, "run_id should default to None");
        assert_eq!(req.output_dir, None, "output_dir should default to None");

        let shot = &req.shots[0];
        assert_eq!(shot.beauty, None, "beauty should default to None");
        assert!(
            shot.truth.include_metrics,
            "include_metrics should default to true"
        );
    }

    #[test]
    fn schema_version_survives_round_trip() {
        // Canonical Sprint 1C requests carry schema_version = 1.
        let req = full_request();
        assert_eq!(req.schema_version, 1);
        let ron_str = ron::ser::to_string_pretty(&req, ron::ser::PrettyConfig::default())
            .expect("serialization must succeed");
        let recovered: CaptureRequest =
            ron::de::from_str(&ron_str).expect("deserialization must succeed");
        assert_eq!(recovered.schema_version, 1);

        // A future `schema_version: 99` document still parses cleanly — version
        // gating is the executor's responsibility, not the serde layer's.
        // Locks the contract: adding a v2 field must not trip on an older parser
        // that happens to see a v99 value.
        let future = r#"(
            schema_version: 99,
            shots: [
                (
                    id: "from_the_future",
                    seed: 0,
                    preset: "volcanic_single",
                    sim_resolution: 64,
                    truth: (overlays: []),
                ),
            ],
        )"#;
        let parsed: CaptureRequest = ron::de::from_str(future)
            .expect("future-version documents must still parse at the serde layer");
        assert_eq!(parsed.schema_version, 99);
    }

    fn request_with_camera(camera_preset: &str) -> CaptureRequest {
        CaptureRequest {
            schema_version: 1,
            run_id: None,
            output_dir: None,
            shots: vec![CaptureShot {
                id: "cam_test".to_owned(),
                seed: 0,
                preset: "volcanic_single".to_owned(),
                sim_resolution: 64,
                truth: TruthSpec {
                    overlays: vec![],
                    include_metrics: true,
                },
                beauty: Some(BeautySpec {
                    camera_preset: camera_preset.to_owned(),
                    overlay_stack: vec![],
                    resolution: (1280, 800),
                }),
            }],
        }
    }

    #[test]
    fn camera_preset_name_lexicon() {
        // The spec defines three valid camera preset strings.  Verify each
        // survives a RON round-trip unchanged (the field is plain String, not
        // an enum, for forward compatibility).
        for name in ["hero", "top_debug", "low_oblique"] {
            let ron_str = ron::ser::to_string_pretty(
                &request_with_camera(name),
                ron::ser::PrettyConfig::default(),
            )
            .expect("serialization must succeed");
            let recovered: CaptureRequest =
                ron::de::from_str(&ron_str).expect("deserialization must succeed");
            let beauty = recovered.shots[0].beauty.as_ref().unwrap();
            assert_eq!(
                beauty.camera_preset, name,
                "camera preset name '{name}' must survive RON round-trip"
            );
        }
    }
}
