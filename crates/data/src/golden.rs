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

use island_core::world::{BiomeType, WorldState};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

// ─── SummaryMetrics ───────────────────────────────────────────────────────────

/// Per-run numerical summary used by the golden-seed regression tests.
///
/// Integer fields are compared exactly; float fields use `abs_tol = 1e-4`;
/// `*_blake3` fields are bit-exact on the same host/toolchain (see
/// `crates/data/tests/golden_seed_regression.rs` for the full comparison
/// semantics and the cross-platform relaxation policy).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SummaryMetrics {
    // ── integer metrics — regression must be exact ──────────────────────────
    pub land_cell_count: u32,
    pub coast_cell_count: u32,
    pub river_cell_count: u32,
    pub basin_count: u32,
    pub river_mouth_count: u32,

    // ── float metrics — abs tolerance 1e-4 ──────────────────────────────────
    pub max_elevation: f32,
    pub max_elevation_filled: f32,
    pub mean_slope: f32,
    /// Approximate: max accumulation on any river cell (refined in Sprint 2).
    pub longest_river_length: f32,
    /// Mean upstream cell count across the whole grid.
    pub total_drainage_area: f32,

    // ── Sprint 1B climate + ecology + hex summaries ─────────────────────────
    /// Mean precipitation over all land cells.
    pub mean_precipitation: f32,
    /// Ratio of mean precipitation in the upwind half of the domain to
    /// mean precipitation in the downwind half. Values > 1.0 indicate
    /// the expected "windward-wetter-than-leeward" direction.
    pub windward_leeward_precip_ratio: f32,
    /// Mean temperature in °C over all land cells.
    pub mean_temperature_c: f32,
    /// Mean soil moisture over all land cells, `[0, 1]`.
    pub mean_soil_moisture: f32,
    /// Percentage coverage of each biome type over all land cells,
    /// in canonical `BiomeType::ALL` order. Sum ≈ 100.0.
    pub biome_coverage_percent: [f32; 8],
    /// Total number of hex cells in the projection (canonical 64×64).
    pub hex_count: u32,

    // ── field hashes — bit-exact on same host/toolchain ──────────────────────
    pub height_blake3: [u8; 32],
    pub z_filled_blake3: [u8; 32],
    pub flow_dir_blake3: [u8; 32],
    pub accumulation_blake3: [u8; 32],
    pub basin_id_blake3: [u8; 32],
    pub river_mask_blake3: [u8; 32],

    // ── Sprint 2 erosion + coast summaries ──────────────────────────────────
    /// Fraction of relief lost by erosion, `(max_pre - max_post) / max_pre`
    /// clamped to `[0, 1]`. `0.0` when no baseline was recorded (e.g. a
    /// partial pipeline run without `ErosionOuterLoop`).
    pub erosion_relief_drop_fraction: f32,
    /// Count per `CoastType` variant in canonical enum order
    /// `[Cliff, Beach, Estuary, RockyHeadland]`. Sum ≈ `coast_cell_count`
    /// (non-coast cells carry the `Unknown = 0xFF` sentinel and are not
    /// counted).
    pub coast_type_counts: [u32; 4],
    /// Cells that crossed the `sea_level` threshold during erosion, computed
    /// as `|baseline.land_cell_count_pre - current_land_cell_count|`. The
    /// `erosion_no_excessive_sea_crossing` invariant asserts this is ≤ 5 %
    /// of `baseline.land_cell_count_pre`.
    pub erosion_sea_crossing_count: u32,
    /// blake3 of the `derived.coast_type.data` byte field. Bit-exact on the
    /// same host.
    pub coast_type_blake3: [u8; 32],
}

impl SummaryMetrics {
    /// Compute a [`SummaryMetrics`] snapshot from a fully-run [`WorldState`].
    ///
    /// Every `authoritative`, `baked`, and `derived` field read below must be
    /// populated — otherwise the call panics. This matches the golden-seed
    /// regression test's original contract: `compute` is only called on a world
    /// that has run through the canonical Sprint 1A + 1B pipeline.
    ///
    /// Used by both `crates/data/tests/golden_seed_regression.rs` and
    /// `crates/app/src/headless/executor.rs` so the metric definition has a
    /// single source of truth.
    pub fn compute(world: &WorldState) -> Self {
        let height = world
            .authoritative
            .height
            .as_ref()
            .expect("SummaryMetrics::compute: authoritative.height must be populated");
        let z_filled = world
            .derived
            .z_filled
            .as_ref()
            .expect("SummaryMetrics::compute: derived.z_filled must be populated");
        let slope = world
            .derived
            .slope
            .as_ref()
            .expect("SummaryMetrics::compute: derived.slope must be populated");
        let coast = world
            .derived
            .coast_mask
            .as_ref()
            .expect("SummaryMetrics::compute: derived.coast_mask must be populated");
        let flow_dir = world
            .derived
            .flow_dir
            .as_ref()
            .expect("SummaryMetrics::compute: derived.flow_dir must be populated");
        let accum = world
            .derived
            .accumulation
            .as_ref()
            .expect("SummaryMetrics::compute: derived.accumulation must be populated");
        let basin_id = world
            .derived
            .basin_id
            .as_ref()
            .expect("SummaryMetrics::compute: derived.basin_id must be populated");
        let river = world
            .derived
            .river_mask
            .as_ref()
            .expect("SummaryMetrics::compute: derived.river_mask must be populated");

        let land_cell_count = coast.land_cell_count;
        let coast_cell_count = coast.is_coast.data.iter().filter(|&&v| v == 1).count() as u32;
        let river_cell_count = river.data.iter().filter(|&&v| v == 1).count() as u32;
        let basin_count = basin_id.data.iter().copied().max().unwrap_or(0);
        let river_mouth_count = coast
            .river_mouth_mask
            .as_ref()
            .map(|m| m.data.iter().filter(|&&v| v == 1).count() as u32)
            .unwrap_or(0);

        let max_elevation = height
            .data
            .iter()
            .cloned()
            .fold(f32::NEG_INFINITY, f32::max);
        let max_elevation_filled = z_filled
            .data
            .iter()
            .cloned()
            .fold(f32::NEG_INFINITY, f32::max);
        let mean_slope = slope.data.iter().sum::<f32>() / slope.data.len() as f32;
        // longest_river_length: cheapest definition — largest accumulation on a river cell.
        // Sprint 2 will refine this to actual path length.
        let longest_river_length = river
            .data
            .iter()
            .zip(accum.data.iter())
            .filter_map(|(&r, &a)| if r == 1 { Some(a) } else { None })
            .fold(0.0f32, f32::max);
        let total_drainage_area = accum.data.iter().sum::<f32>() / accum.data.len() as f32;

        let height_blake3 = blake3_field_f32(&height.data);
        let z_filled_blake3 = blake3_field_f32(&z_filled.data);
        let flow_dir_blake3 = blake3_field_u8(&flow_dir.data);
        let accumulation_blake3 = blake3_field_f32(&accum.data);
        let basin_id_blake3 = blake3_field_u32(&basin_id.data);
        let river_mask_blake3 = blake3_field_u8(&river.data);

        // ── Sprint 1B summaries ────────────────────────────────────────────────
        let precipitation = world
            .baked
            .precipitation
            .as_ref()
            .expect("SummaryMetrics::compute: baked.precipitation must be populated");
        let temperature = world
            .baked
            .temperature
            .as_ref()
            .expect("SummaryMetrics::compute: baked.temperature must be populated");
        let soil_moisture = world
            .baked
            .soil_moisture
            .as_ref()
            .expect("SummaryMetrics::compute: baked.soil_moisture must be populated");
        let biome_weights = world
            .baked
            .biome_weights
            .as_ref()
            .expect("SummaryMetrics::compute: baked.biome_weights must be populated");
        let hex_attrs = world
            .derived
            .hex_attrs
            .as_ref()
            .expect("SummaryMetrics::compute: derived.hex_attrs must be populated");

        let mut land_n = 0_u32;
        let mut precip_sum = 0.0_f64;
        let mut temp_sum = 0.0_f64;
        let mut moist_sum = 0.0_f64;
        let mut biome_counts = [0_u32; 8];
        let w = coast.is_land.width;
        let h = coast.is_land.height;
        for iy in 0..h {
            for ix in 0..w {
                if coast.is_land.get(ix, iy) != 1 {
                    continue;
                }
                land_n += 1;
                precip_sum += precipitation.get(ix, iy) as f64;
                temp_sum += temperature.get(ix, iy) as f64;
                moist_sum += soil_moisture.get(ix, iy) as f64;
                let dominant = biome_weights.dominant_biome_at(ix, iy) as usize;
                biome_counts[dominant] += 1;
            }
        }
        let land_n_f = land_n.max(1) as f64;
        let mean_precipitation = (precip_sum / land_n_f) as f32;
        let mean_temperature_c = (temp_sum / land_n_f) as f32;
        let mean_soil_moisture = (moist_sum / land_n_f) as f32;

        let mut biome_coverage_percent = [0.0_f32; 8];
        for (i, c) in biome_counts.iter().enumerate() {
            biome_coverage_percent[i] = (*c as f64 * 100.0 / land_n_f) as f32;
        }

        // Windward vs leeward: project each land cell onto `wind` (the
        // direction wind comes FROM). Cells whose projection is above the
        // median of all land cells are "upwind" (windward); below the
        // median are "downwind" (leeward). Ratio > 1 means the windward
        // side is wetter, which is the qualitative spec acceptance
        // criterion for DD2.
        let wind_dir = world.preset.prevailing_wind_dir;
        let wind_x = wind_dir.cos();
        let wind_y = wind_dir.sin();
        let mut projections: Vec<(f32, f32)> = Vec::with_capacity(land_n as usize);
        for iy in 0..h {
            for ix in 0..w {
                if coast.is_land.get(ix, iy) != 1 {
                    continue;
                }
                let proj = ix as f32 * wind_x + iy as f32 * wind_y;
                projections.push((proj, precipitation.get(ix, iy)));
            }
        }
        let mut proj_vals: Vec<f32> = projections.iter().map(|(p, _)| *p).collect();
        proj_vals.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let median = proj_vals
            .get(proj_vals.len() / 2)
            .copied()
            .unwrap_or_default();

        let mut windward_sum = 0.0_f64;
        let mut windward_n = 0_u32;
        let mut leeward_sum = 0.0_f64;
        let mut leeward_n = 0_u32;
        for (proj, p) in &projections {
            if *proj >= median {
                windward_sum += *p as f64;
                windward_n += 1;
            } else {
                leeward_sum += *p as f64;
                leeward_n += 1;
            }
        }
        let windward_mean = windward_sum / windward_n.max(1) as f64;
        let leeward_mean = leeward_sum / leeward_n.max(1) as f64;
        let windward_leeward_precip_ratio = (windward_mean / leeward_mean.max(1e-9)) as f32;

        let hex_count = hex_attrs.cols * hex_attrs.rows;
        debug_assert_eq!(biome_weights.types, BiomeType::ALL);

        // ── Sprint 2 summaries ────────────────────────────────────────────────
        // `erosion_baseline` and `coast_type` are populated by ErosionOuterLoop
        // and CoastTypeStage, both members of `sim::default_pipeline()`. If
        // the caller ran a partial pipeline that omits them, the Sprint 2
        // summaries fall back to zero / empty rather than panicking — matching
        // the `skip_if_missing` semantics that `core::validation`'s Sprint 2
        // invariants use.
        let (erosion_relief_drop_fraction, erosion_sea_crossing_count) =
            match world.derived.erosion_baseline.as_ref() {
                Some(baseline) => {
                    let max_pre = baseline.max_height_pre;
                    let drop = if max_pre > 0.0 {
                        ((max_pre - max_elevation) / max_pre).clamp(0.0, 1.0)
                    } else {
                        0.0
                    };
                    let crossed = (baseline.land_cell_count_pre as i64 - land_cell_count as i64)
                        .unsigned_abs() as u32;
                    (drop, crossed)
                }
                None => (0.0, 0),
            };

        let (coast_type_counts, coast_type_blake3) = match world.derived.coast_type.as_ref() {
            Some(ct) => {
                let mut counts = [0_u32; 4];
                for &v in &ct.data {
                    if (v as usize) < 4 {
                        counts[v as usize] += 1;
                    }
                    // Unknown = 0xFF and any out-of-range sentinel is not counted.
                }
                (counts, blake3_field_u8(&ct.data))
            }
            None => ([0_u32; 4], [0_u8; 32]),
        };

        SummaryMetrics {
            land_cell_count,
            coast_cell_count,
            river_cell_count,
            basin_count,
            river_mouth_count,
            max_elevation,
            max_elevation_filled,
            mean_slope,
            longest_river_length,
            total_drainage_area,
            mean_precipitation,
            windward_leeward_precip_ratio,
            mean_temperature_c,
            mean_soil_moisture,
            biome_coverage_percent,
            hex_count,
            height_blake3,
            z_filled_blake3,
            flow_dir_blake3,
            accumulation_blake3,
            basin_id_blake3,
            river_mask_blake3,
            erosion_relief_drop_fraction,
            coast_type_counts,
            erosion_sea_crossing_count,
            coast_type_blake3,
        }
    }
}

fn blake3_field_f32(data: &[f32]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    for v in data {
        hasher.update(&v.to_le_bytes());
    }
    *hasher.finalize().as_bytes()
}

fn blake3_field_u8(data: &[u8]) -> [u8; 32] {
    *blake3::hash(data).as_bytes()
}

fn blake3_field_u32(data: &[u32]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    for v in data {
        hasher.update(&v.to_le_bytes());
    }
    *hasher.finalize().as_bytes()
}

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
        let restored =
            ron::from_str::<GoldenSeeds>(&ron_str).expect("deserialization should succeed");

        assert_eq!(original, restored);
    }
}
