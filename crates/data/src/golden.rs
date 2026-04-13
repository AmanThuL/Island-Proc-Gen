//! Loading and enumeration of golden seed entries for deterministic regression testing.
//!
//! ## Path resolution strategy
//!
//! `load_golden_seeds` tries two locations in order:
//!
//! 1. **Runtime-relative** — `./crates/data/golden/seeds.ron` relative to
//!    the current working directory. This is the path that works when the
//!    workspace is run with `cargo run` from the repository root.
//!
//! 2. **Manifest-relative** — `$CARGO_MANIFEST_DIR/golden/seeds.ron`.
//!    `CARGO_MANIFEST_DIR` is baked in at compile time and always points to
//!    `crates/data/`, so this path works unconditionally in `cargo test`.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

// ─── error ────────────────────────────────────────────────────────────────────

/// Error returned when golden seeds cannot be loaded.
#[derive(Debug, thiserror::Error)]
pub enum GoldenLoadError {
    #[error("golden seeds file not found at {path}")]
    NotFound { path: String },

    #[error("io error reading golden seeds: {source}")]
    Io {
        #[source]
        source: std::io::Error,
    },

    #[error("parse error in golden seeds file: {source}")]
    Parse {
        #[source]
        source: Box<ron::error::SpannedError>,
    },
}

// ─── types ────────────────────────────────────────────────────────────────────

/// A single golden seed entry: (seed, preset_name) pair.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GoldenSeedEntry {
    pub seed: u64,
    pub preset_name: String,
}

/// Collection of golden seed entries for regression testing.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GoldenSeeds {
    pub entries: Vec<GoldenSeedEntry>,
}

// ─── public API ───────────────────────────────────────────────────────────────

/// Load golden seeds from the canonical file location.
///
/// See module-level docs for the path resolution strategy.
pub fn load_golden_seeds() -> Result<GoldenSeeds, GoldenLoadError> {
    let candidate_paths = candidate_paths();

    for path in &candidate_paths {
        if path.exists() {
            return load_from_path(path);
        }
    }

    // None of the candidates existed
    Err(GoldenLoadError::NotFound {
        path: candidate_paths
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(", "),
    })
}

// ─── internals ────────────────────────────────────────────────────────────────

/// Build the ordered list of paths to try for golden seeds.
fn candidate_paths() -> Vec<PathBuf> {
    vec![
        // 1. Runtime-relative (for `cargo run` from repo root)
        PathBuf::from("crates/data/golden/seeds.ron"),
        // 2. Manifest-relative (always resolves correctly in `cargo test`)
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("golden")
            .join("seeds.ron"),
    ]
}

/// Read and parse golden seeds from an explicit filesystem path.
///
/// Exposed as `pub(crate)` so integration tests can inject temporary paths
/// without going through the full path-resolution logic.
pub(crate) fn load_from_path(path: &Path) -> Result<GoldenSeeds, GoldenLoadError> {
    let text = std::fs::read_to_string(path).map_err(|source| {
        if source.kind() == std::io::ErrorKind::NotFound {
            GoldenLoadError::NotFound {
                path: path.display().to_string(),
            }
        } else {
            GoldenLoadError::Io { source }
        }
    })?;

    ron::from_str::<GoldenSeeds>(&text).map_err(|source| GoldenLoadError::Parse {
        source: Box::new(source),
    })
}

// ─── tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_golden_seeds_returns_three_entries() {
        let seeds = load_golden_seeds().expect("should load golden seeds");
        assert_eq!(seeds.entries.len(), 3, "expected exactly 3 entries");

        // Verify the three expected pairs are present
        assert_eq!(seeds.entries[0].seed, 42);
        assert_eq!(seeds.entries[0].preset_name, "volcanic_single");

        assert_eq!(seeds.entries[1].seed, 123);
        assert_eq!(seeds.entries[1].preset_name, "volcanic_twin");

        assert_eq!(seeds.entries[2].seed, 777);
        assert_eq!(seeds.entries[2].preset_name, "caldera");
    }

    #[test]
    fn golden_seeds_roundtrip() {
        let original = GoldenSeeds {
            entries: vec![
                GoldenSeedEntry {
                    seed: 42,
                    preset_name: "volcanic_single".to_string(),
                },
                GoldenSeedEntry {
                    seed: 123,
                    preset_name: "volcanic_twin".to_string(),
                },
                GoldenSeedEntry {
                    seed: 777,
                    preset_name: "caldera".to_string(),
                },
            ],
        };

        // Serialize to RON string
        let ron_str = ron::to_string(&original).expect("serialization should succeed");

        // Deserialize back from RON string
        let restored = ron::from_str::<GoldenSeeds>(&ron_str)
            .expect("deserialization should succeed");

        assert_eq!(original, restored);
    }
}
