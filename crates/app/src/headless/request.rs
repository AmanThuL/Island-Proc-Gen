use std::path::PathBuf;

use island_core::preset::{ClimateParams, ErosionParams, IslandAge, IslandArchetypePreset};

/// Sprint 2 DD5: selective overrides folded on top of the loaded preset.
///
/// Applied by `headless::executor::run_shot` *after*
/// `data::presets::load_preset(&shot.preset)` returns and *before* the
/// simulation pipeline runs. Every field is `Option`; only `Some`
/// variants override the loaded preset. `None` leaves the preset
/// field unchanged (forward-compat with Sprint 1C v1 request files).
///
/// Primary use-case for Sprint 2 is the before/after erosion compare
/// in `crates/data/golden/headless/sprint_2_erosion/`: `pre_*` shots
/// set `erosion: Some(ErosionParams { n_batch: 0, ..default })` to
/// turn the ErosionOuterLoop into a noop, while `post_*` shots leave
/// `preset_override = None` and get the locked defaults.
#[derive(Debug, Clone, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct PresetOverride {
    #[serde(default)]
    pub island_radius: Option<f32>,
    #[serde(default)]
    pub max_relief: Option<f32>,
    #[serde(default)]
    pub volcanic_center_count: Option<u32>,
    #[serde(default)]
    pub island_age: Option<IslandAge>,
    #[serde(default)]
    pub prevailing_wind_dir: Option<f32>,
    #[serde(default)]
    pub marine_moisture_strength: Option<f32>,
    #[serde(default)]
    pub sea_level: Option<f32>,
    #[serde(default)]
    pub erosion: Option<ErosionParams>,
    /// Sprint 3 DD8: optional override of the climate sub-struct.
    /// `None` (the default) leaves `preset.climate` untouched. Required by
    /// the `sprint_3_sediment_climate` baseline's `pre_*` shots so they can
    /// force `climate.precipitation_variant = V2Raymarch` on top of the
    /// erosion v1-variant override.
    #[serde(default)]
    pub climate: Option<ClimateParams>,
}

impl PresetOverride {
    /// Fold `self` on top of `preset`, assigning only the `Some` fields.
    ///
    /// `None` fields are a no-op — the existing preset value is left intact.
    /// This is the canonical application point; call it from the executor
    /// immediately after `data::presets::load_preset` returns.
    pub fn apply_to(&self, preset: &mut IslandArchetypePreset) {
        if let Some(v) = self.island_radius {
            preset.island_radius = v;
        }
        if let Some(v) = self.max_relief {
            preset.max_relief = v;
        }
        if let Some(v) = self.volcanic_center_count {
            preset.volcanic_center_count = v;
        }
        if let Some(v) = self.island_age {
            preset.island_age = v;
        }
        if let Some(v) = self.prevailing_wind_dir {
            preset.prevailing_wind_dir = v;
        }
        if let Some(v) = self.marine_moisture_strength {
            preset.marine_moisture_strength = v;
        }
        if let Some(v) = self.sea_level {
            preset.sea_level = v;
        }
        if let Some(v) = &self.erosion {
            preset.erosion = v.clone();
        }
        if let Some(v) = &self.climate {
            preset.climate = v.clone();
        }
    }
}

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
#[derive(Debug, Clone, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct CaptureRequest {
    /// Schema version. Sprint 1C shipped v1; Sprint 2 bumped to v2 to add
    /// `CaptureShot.preset_override` (DD5); Sprint 3.5 bumps to v3 to add
    /// `CaptureShot.view_mode` (DD8). v1 and v2 request files still parse
    /// under a v3 binary because each extension is `#[serde(default)]`.
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
#[derive(Debug, Clone, PartialEq, serde::Deserialize, serde::Serialize)]
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

    /// Sprint 2 DD5: optional overrides applied on top of the loaded preset.
    /// `None` (v1 default) leaves the loaded preset untouched so v1 request
    /// files produce bit-exact identical results under v2 binaries.
    #[serde(default)]
    pub preset_override: Option<PresetOverride>,

    /// Sprint 3.5 DD8: view mode this shot renders in. `None` = `Continuous`
    /// (legacy default), keeping v1 and v2 request files bit-compatible.
    /// Only `schema_version: 3` requests meaningfully use `Some(HexOverlay)`
    /// / `Some(HexOnly)`; older schemas still parse.
    #[serde(default)]
    pub view_mode: Option<crate::runtime::ViewMode>,
}

/// Specification for the deterministic CPU truth path.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct TruthSpec {
    /// Overlay IDs to export; each must exist in the Sprint 2 13-descriptor registry.
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
                    preset_override: None,
                    view_mode: None,
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
                    preset_override: None,
                    view_mode: None,
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
                preset_override: None,
                view_mode: None,
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

    // ── Sprint 2 DD5: PresetOverride tests ──────────────────────────────────

    #[test]
    fn preset_override_none_is_v1_forward_compat() {
        // A v1 RON document (no preset_override field) parses to
        // preset_override == None, preserving bit-exact backward compat.
        let ron_str = r#"(
            schema_version: 1,
            shots: [
                (
                    id: "v1_shot",
                    seed: 7,
                    preset: "volcanic_single",
                    sim_resolution: 64,
                    truth: (overlays: []),
                ),
            ],
        )"#;
        let req: CaptureRequest =
            ron::de::from_str(ron_str).expect("v1 document must parse under v2 schema");
        assert_eq!(
            req.shots[0].preset_override, None,
            "preset_override must default to None when the field is absent (v1 forward-compat)"
        );
    }

    #[test]
    fn preset_override_some_full_round_trips() {
        // A CaptureShot with every PresetOverride field set must survive
        // a RON round-trip via PartialEq (no Eq required — f32 fields).
        let override_spec = PresetOverride {
            island_radius: Some(0.42),
            max_relief: Some(0.42),
            volcanic_center_count: Some(3),
            island_age: Some(IslandAge::Mature),
            prevailing_wind_dir: Some(0.42),
            marine_moisture_strength: Some(0.42),
            sea_level: Some(0.42),
            erosion: Some(ErosionParams {
                n_batch: 0,
                ..ErosionParams::default()
            }),
            climate: Some(ClimateParams::default()),
        };
        let shot = CaptureShot {
            id: "override_shot".to_owned(),
            seed: 1,
            preset: "volcanic_single".to_owned(),
            sim_resolution: 64,
            truth: TruthSpec {
                overlays: vec![],
                include_metrics: false,
            },
            beauty: None,
            preset_override: Some(override_spec),
            view_mode: None,
        };
        let ron_str = ron::ser::to_string_pretty(&shot, ron::ser::PrettyConfig::default())
            .expect("serialization must succeed");
        let recovered: CaptureShot =
            ron::de::from_str(&ron_str).expect("deserialization must succeed");
        assert_eq!(
            shot, recovered,
            "full PresetOverride must survive RON round-trip"
        );
    }

    #[test]
    fn preset_override_partial_applies_only_some_fields() {
        // Only marine_moisture_strength is Some; every other field stays at
        // its pre-call value when apply_to is invoked.
        let override_spec = PresetOverride {
            island_radius: None,
            max_relief: None,
            volcanic_center_count: None,
            island_age: None,
            prevailing_wind_dir: None,
            marine_moisture_strength: Some(0.1),
            sea_level: None,
            erosion: None,
            climate: None,
        };
        let mut preset = data::presets::load_preset("volcanic_single")
            .expect("volcanic_single must be a known preset");
        let original_island_radius = preset.island_radius;
        let original_max_relief = preset.max_relief;
        let original_volcanic_center_count = preset.volcanic_center_count;
        let original_island_age = preset.island_age;
        let original_prevailing_wind_dir = preset.prevailing_wind_dir;
        let original_sea_level = preset.sea_level;
        let original_erosion = preset.erosion.clone();

        override_spec.apply_to(&mut preset);

        assert_eq!(
            preset.marine_moisture_strength, 0.1,
            "marine_moisture_strength must be overridden to 0.1"
        );
        assert_eq!(
            preset.island_radius, original_island_radius,
            "island_radius must be unchanged"
        );
        assert_eq!(
            preset.max_relief, original_max_relief,
            "max_relief must be unchanged"
        );
        assert_eq!(
            preset.volcanic_center_count, original_volcanic_center_count,
            "volcanic_center_count must be unchanged"
        );
        assert_eq!(
            preset.island_age, original_island_age,
            "island_age must be unchanged"
        );
        assert_eq!(
            preset.prevailing_wind_dir, original_prevailing_wind_dir,
            "prevailing_wind_dir must be unchanged"
        );
        assert_eq!(
            preset.sea_level, original_sea_level,
            "sea_level must be unchanged"
        );
        assert_eq!(
            preset.erosion, original_erosion,
            "erosion must be unchanged"
        );
    }

    #[test]
    fn preset_override_erosion_folds_into_preset() {
        // apply_to with only the erosion field set overrides n_batch and n_inner
        // while leaving the other ErosionParams fields at their defaults.
        let override_spec = PresetOverride {
            island_radius: None,
            max_relief: None,
            volcanic_center_count: None,
            island_age: None,
            prevailing_wind_dir: None,
            marine_moisture_strength: None,
            sea_level: None,
            erosion: Some(ErosionParams {
                n_batch: 0,
                n_inner: 5,
                ..ErosionParams::default()
            }),
            climate: None,
        };
        let mut preset = data::presets::load_preset("volcanic_single")
            .expect("volcanic_single must be a known preset");
        override_spec.apply_to(&mut preset);

        assert_eq!(preset.erosion.n_batch, 0, "n_batch must be overridden to 0");
        assert_eq!(preset.erosion.n_inner, 5, "n_inner must be overridden to 5");
        // The remaining erosion fields must be the ErosionParams::default() values.
        let defaults = ErosionParams::default();
        assert_eq!(
            preset.erosion.spim_k, defaults.spim_k,
            "spim_k must remain at default"
        );
        assert_eq!(
            preset.erosion.spim_m, defaults.spim_m,
            "spim_m must remain at default"
        );
        assert_eq!(
            preset.erosion.spim_n, defaults.spim_n,
            "spim_n must remain at default"
        );
        assert_eq!(
            preset.erosion.hillslope_d, defaults.hillslope_d,
            "hillslope_d must remain at default"
        );
        assert_eq!(
            preset.erosion.n_diff_substep, defaults.n_diff_substep,
            "n_diff_substep must remain at default"
        );
    }

    #[test]
    fn schema_v2_parses_preset_override_from_ron() {
        // A hand-written v2 RON string with a nested preset_override must parse
        // to the exact PresetOverride struct, including the erosion sub-struct.
        let ron_str = r#"(
            schema_version: 2,
            shots: [
                (
                    id: "erosion_pre",
                    seed: 42,
                    preset: "volcanic_single",
                    sim_resolution: 64,
                    truth: (overlays: []),
                    preset_override: Some((
                        erosion: Some((
                            n_batch: 0,
                            n_inner: 5,
                        )),
                    )),
                ),
            ],
        )"#;
        let req: CaptureRequest =
            ron::de::from_str(ron_str).expect("v2 document with preset_override must parse");
        assert_eq!(req.schema_version, 2);
        let override_spec = req.shots[0]
            .preset_override
            .as_ref()
            .expect("preset_override must be Some");
        let erosion = override_spec
            .erosion
            .as_ref()
            .expect("erosion field must be Some");
        assert_eq!(erosion.n_batch, 0, "n_batch must be 0");
        assert_eq!(erosion.n_inner, 5, "n_inner must be 5");
        // All other PresetOverride fields must be None (not present in the RON).
        assert_eq!(override_spec.island_radius, None);
        assert_eq!(override_spec.max_relief, None);
        assert_eq!(override_spec.volcanic_center_count, None);
        assert_eq!(override_spec.island_age, None);
        assert_eq!(override_spec.prevailing_wind_dir, None);
        assert_eq!(override_spec.marine_moisture_strength, None);
        assert_eq!(override_spec.sea_level, None);
    }

    // ── Sprint 3.5 DD8: schema backward-compat gate ─────────────────────────

    #[test]
    fn schema_v1_and_v2_still_parse_under_v3_binary() {
        // v1 fixture — no preset_override, no view_mode.
        let v1_ron = r#"(
            schema_version: 1,
            run_id: Some("v1_fixture"),
            shots: [(
                id: "s",
                seed: 42,
                preset: "volcanic_single",
                sim_resolution: 128,
                truth: (overlays: [], include_metrics: true),
            )],
        )"#;
        let v1: CaptureRequest =
            ron::de::from_str(v1_ron).expect("v1 fixture must parse under v3 binary");
        assert_eq!(v1.schema_version, 1);
        assert_eq!(v1.shots.len(), 1);
        assert!(v1.shots[0].preset_override.is_none());
        assert!(v1.shots[0].view_mode.is_none());

        // v2 fixture — has preset_override, no view_mode.
        let v2_ron = r#"(
            schema_version: 2,
            run_id: Some("v2_fixture"),
            shots: [(
                id: "s",
                seed: 42,
                preset: "volcanic_single",
                sim_resolution: 128,
                truth: (overlays: [], include_metrics: true),
                preset_override: Some((
                    erosion: Some((
                        n_batch: 0,
                    )),
                )),
            )],
        )"#;
        let v2: CaptureRequest =
            ron::de::from_str(v2_ron).expect("v2 fixture must parse under v3 binary");
        assert_eq!(v2.schema_version, 2);
        assert!(v2.shots[0].preset_override.is_some());
        assert!(v2.shots[0].view_mode.is_none());

        // v3 fixture — has explicit view_mode.
        let v3_ron = r#"(
            schema_version: 3,
            run_id: Some("v3_fixture"),
            shots: [(
                id: "s",
                seed: 42,
                preset: "volcanic_single",
                sim_resolution: 128,
                truth: (overlays: [], include_metrics: true),
                view_mode: Some(HexOnly),
            )],
        )"#;
        let v3: CaptureRequest =
            ron::de::from_str(v3_ron).expect("v3 fixture must parse under v3 binary");
        assert_eq!(v3.schema_version, 3);
        assert_eq!(
            v3.shots[0].view_mode,
            Some(crate::runtime::ViewMode::HexOnly)
        );

        // Downgrade invariant: stripping view_mode from v3 still parses (additive).
        let v3_stripped = r#"(
            schema_version: 3,
            run_id: Some("v3_stripped"),
            shots: [(
                id: "s",
                seed: 42,
                preset: "volcanic_single",
                sim_resolution: 128,
                truth: (overlays: [], include_metrics: true),
            )],
        )"#;
        let v3s: CaptureRequest = ron::de::from_str(v3_stripped)
            .expect("v3 without view_mode still parses (additive extension)");
        assert!(v3s.shots[0].view_mode.is_none());
    }
}
