//! Loading and enumeration of built-in island archetype presets.
//!
//! ## Path resolution strategy
//!
//! `load_preset` tries two locations in order:
//!
//! 1. **Runtime-relative** — `./crates/data/presets/<name>.ron` relative to
//!    the current working directory.  This is the path that works when the
//!    workspace is run with `cargo run` from the repository root, and it is
//!    the path that **Sprint 4 CLI** should use by shipping the `presets/`
//!    directory alongside the compiled binary (or by setting the CWD to the
//!    binary's parent directory at startup).
//!
//! 2. **Manifest-relative** — `$CARGO_MANIFEST_DIR/presets/<name>.ron`.
//!    `CARGO_MANIFEST_DIR` is baked in at compile time and always points to
//!    `crates/data/`, so this path works unconditionally in `cargo test`.
//!
//! Sprint 4 note: if the CLI ships with `presets/` next to the binary, change
//! strategy 1 to use `std::env::current_exe()?.parent()?` as the base.

// The workspace crate named `core` is aliased to `island_core` in Cargo.toml
// so that it does not shadow the Rust std `core` crate (which thiserror's
// derive macro needs as `::core::fmt`).
use std::path::{Path, PathBuf};

use island_core::preset::IslandArchetypePreset;

// ─── error ────────────────────────────────────────────────────────────────────

/// Error returned when a preset cannot be loaded.
#[derive(Debug, thiserror::Error)]
pub enum PresetLoadError {
    #[error("preset '{name}' not found at {path}")]
    NotFound { name: String, path: String },

    #[error("io error reading preset '{name}': {source}")]
    Io {
        name: String,
        #[source]
        source: std::io::Error,
    },

    #[error("parse error in preset '{name}': {source}")]
    Parse {
        name: String,
        #[source]
        source: Box<ron::error::SpannedError>,
    },
}

// ─── built-in manifest ────────────────────────────────────────────────────────

/// Names of all built-in presets shipped with this crate.
///
/// Sprint 4 CLI can call this to populate a `--preset` argument's choices.
pub fn list_builtin() -> Vec<&'static str> {
    vec![
        "volcanic_single",
        "volcanic_twin",
        "caldera",
        "volcanic_caldera_young",
        "volcanic_twin_old",
        "volcanic_eroded_ridge",
    ]
}

// ─── public API ───────────────────────────────────────────────────────────────

/// Load a named preset from disk.
///
/// `name` should be one of the values returned by [`list_builtin`], e.g.
/// `"volcanic_single"`.
///
/// See module-level docs for the path resolution strategy.
pub fn load_preset(name: &str) -> Result<IslandArchetypePreset, PresetLoadError> {
    let candidate_paths = candidate_paths(name);

    for path in &candidate_paths {
        if path.exists() {
            return load_from_path(name, path);
        }
    }

    // None of the candidates existed
    Err(PresetLoadError::NotFound {
        name: name.to_string(),
        path: candidate_paths
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(", "),
    })
}

// ─── internals ────────────────────────────────────────────────────────────────

/// Build the ordered list of paths to try for a given preset name.
fn candidate_paths(name: &str) -> Vec<PathBuf> {
    let file_name = format!("{name}.ron");
    vec![
        // 1. Runtime-relative (for `cargo run` from repo root / CLI binary)
        PathBuf::from("crates/data/presets").join(&file_name),
        // 2. Manifest-relative (always resolves correctly in `cargo test`)
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("presets")
            .join(&file_name),
    ]
}

/// Read and parse a preset from an explicit filesystem path.
///
/// Exposed as `pub(crate)` so integration tests can inject temporary paths
/// without going through the full path-resolution logic.
pub(crate) fn load_from_path(
    name: &str,
    path: &Path,
) -> Result<IslandArchetypePreset, PresetLoadError> {
    let text = std::fs::read_to_string(path).map_err(|source| {
        if source.kind() == std::io::ErrorKind::NotFound {
            PresetLoadError::NotFound {
                name: name.to_string(),
                path: path.display().to_string(),
            }
        } else {
            PresetLoadError::Io {
                name: name.to_string(),
                source,
            }
        }
    })?;

    ron::from_str::<IslandArchetypePreset>(&text).map_err(|source| PresetLoadError::Parse {
        name: name.to_string(),
        source: Box::new(source),
    })
}

// ─── tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use island_core::preset::{IslandAge, IslandArchetypePreset};
    use std::io::Write as _;

    // 3. volcanic_single loads and matches expected values
    #[test]
    fn load_volcanic_single_matches_expected() {
        let p = load_preset("volcanic_single").expect("should load volcanic_single");
        let expected = IslandArchetypePreset {
            name: "volcanic_single".to_string(),
            island_radius: 0.55,
            max_relief: 0.85,
            volcanic_center_count: 1,
            island_age: IslandAge::Young,
            prevailing_wind_dir: 1.5708,
            marine_moisture_strength: 0.75,
            sea_level: 0.30,
            erosion: Default::default(),
            climate: Default::default(),
        };
        assert_eq!(p, expected);
    }

    // 4. volcanic_twin loads and matches expected values
    #[test]
    fn load_volcanic_twin_matches_expected() {
        let p = load_preset("volcanic_twin").expect("should load volcanic_twin");
        let expected = IslandArchetypePreset {
            name: "volcanic_twin".to_string(),
            island_radius: 0.65,
            max_relief: 0.75,
            volcanic_center_count: 2,
            island_age: IslandAge::Young,
            prevailing_wind_dir: 1.5708,
            marine_moisture_strength: 0.70,
            sea_level: 0.32,
            erosion: Default::default(),
            climate: Default::default(),
        };
        assert_eq!(p, expected);
    }

    // 5. caldera loads and matches expected values
    #[test]
    fn load_caldera_matches_expected() {
        let p = load_preset("caldera").expect("should load caldera");
        let expected = IslandArchetypePreset {
            name: "caldera".to_string(),
            island_radius: 0.50,
            max_relief: 0.55,
            volcanic_center_count: 1,
            island_age: IslandAge::Mature,
            prevailing_wind_dir: 1.5708,
            marine_moisture_strength: 0.80,
            sea_level: 0.28,
            erosion: Default::default(),
            climate: Default::default(),
        };
        assert_eq!(p, expected);
    }

    // 6. missing preset returns NotFound, not a panic
    #[test]
    fn load_missing_returns_err() {
        let result = load_preset("does_not_exist");
        assert!(
            matches!(result, Err(PresetLoadError::NotFound { .. })),
            "expected NotFound, got {result:?}"
        );
    }

    // 7. malformed RON returns Parse error, not a panic
    #[test]
    fn load_malformed_returns_err() {
        // Write bad RON to a temp file and use load_from_path directly
        let mut tmp = tempfile::NamedTempFile::new().expect("temp file");
        tmp.write_all(b"this is { not valid ron at all !!!")
            .unwrap();
        let result = load_from_path("malformed_test", tmp.path());
        assert!(
            matches!(result, Err(PresetLoadError::Parse { .. })),
            "expected Parse error, got {result:?}"
        );
    }

    // 8. volcanic_caldera_young loads and matches expected values
    #[test]
    fn load_volcanic_caldera_young_matches_expected() {
        let p = load_preset("volcanic_caldera_young").expect("should load volcanic_caldera_young");
        let expected = IslandArchetypePreset {
            name: "volcanic_caldera_young".to_string(),
            island_radius: 0.50,
            max_relief: 0.70,
            volcanic_center_count: 1,
            island_age: IslandAge::Young,
            prevailing_wind_dir: 1.5708,
            marine_moisture_strength: 0.75,
            sea_level: 0.30,
            erosion: Default::default(),
            climate: Default::default(),
        };
        assert_eq!(p, expected);
    }

    // 9. volcanic_twin_old loads and matches expected values
    #[test]
    fn load_volcanic_twin_old_matches_expected() {
        let p = load_preset("volcanic_twin_old").expect("should load volcanic_twin_old");
        let expected = IslandArchetypePreset {
            name: "volcanic_twin_old".to_string(),
            island_radius: 0.65,
            max_relief: 0.40,
            volcanic_center_count: 2,
            island_age: IslandAge::Old,
            prevailing_wind_dir: 1.5708,
            marine_moisture_strength: 0.70,
            sea_level: 0.32,
            erosion: Default::default(),
            climate: Default::default(),
        };
        assert_eq!(p, expected);
    }

    // 10. volcanic_eroded_ridge loads and matches expected values
    #[test]
    fn load_volcanic_eroded_ridge_matches_expected() {
        let p = load_preset("volcanic_eroded_ridge").expect("should load volcanic_eroded_ridge");
        let expected = IslandArchetypePreset {
            name: "volcanic_eroded_ridge".to_string(),
            island_radius: 0.60,
            max_relief: 0.55,
            volcanic_center_count: 1,
            island_age: IslandAge::Mature,
            prevailing_wind_dir: 1.5708,
            marine_moisture_strength: 0.72,
            sea_level: 0.30,
            erosion: island_core::preset::ErosionParams {
                n_batch: 12,
                ..Default::default()
            },
            climate: Default::default(),
        };
        assert_eq!(p, expected);
    }
}
