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

use island_core::validation::DEPOSITION_FLAG_THRESHOLD;
use island_core::world::{BiomeType, HexAttributeField, HexDebugAttributes, WorldState};
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
    /// v1 fallback — count per `CoastType` variant in canonical enum order
    /// `[Cliff, Beach, Estuary, RockyHeadland]` (4 bins, pre-LavaDelta). Sum
    /// ≈ `coast_cell_count` only when `preset.erosion.coast_type_variant ==
    /// CoastTypeVariant::V1Cheap`; under the default `V2FetchIntegral` path
    /// this is `[0; 4]` and [`Self::coast_type_counts_v2`] is authoritative.
    ///
    /// Renamed from `coast_type_counts` in Sprint 3 (DD6). Old `metrics.ron`
    /// files still deserialize under the original field name via
    /// `#[serde(alias = "coast_type_counts")]`.
    #[serde(default, alias = "coast_type_counts")]
    pub coast_type_counts_v1: [u32; 4],
    /// Cells that crossed the `sea_level` threshold during erosion, computed
    /// as `|baseline.land_cell_count_pre - current_land_cell_count|`. The
    /// `erosion_no_excessive_sea_crossing` invariant asserts this is ≤ 5 %
    /// of `baseline.land_cell_count_pre`.
    pub erosion_sea_crossing_count: u32,
    /// blake3 of the `derived.coast_type.data` byte field. Bit-exact on the
    /// same host.
    pub coast_type_blake3: [u8; 32],

    // ── Sprint 3 sediment + fog + coast v2 summaries ────────────────────────
    /// Sprint 3 DD2 / DD3: mean `authoritative.sediment` over land cells.
    /// `0.0` when sediment is absent (`authoritative.sediment == None`, e.g.
    /// a pipeline run that stopped before `CoastMaskStage`).
    #[serde(default)]
    pub mean_sediment_thickness: f32,
    /// Sprint 3 DD3: fraction of land cells with
    /// `hs > `[`DEPOSITION_FLAG_THRESHOLD`] (= 0.15). Matches the signal
    /// used by the `deposition_zone_fraction_realistic` invariant.
    #[serde(default)]
    pub deposition_zone_fraction: f32,
    /// Sprint 3 DD6: coast-type class histogram over coast cells, in
    /// canonical enum order `[Cliff, Beach, Estuary, RockyHeadland,
    /// LavaDelta]` (5 bins). Sum ≈ `coast_cell_count`. Authoritative under
    /// the default `CoastTypeVariant::V2FetchIntegral`; filled from the same
    /// `derived.coast_type` field whose variant disambiguates semantics.
    #[serde(default)]
    pub coast_type_counts_v2: [u32; 5],
    /// Sprint 3 DD5: mean `derived.fog_water_input` over land cells where
    /// `fog_likelihood > 0.1`. `0.0` when the fog-water derived cache is
    /// absent (partial pipeline run).
    #[serde(default)]
    pub mean_fog_water_input: f32,
    /// Sprint 3 §1 goal 8: count of basin IDs surviving post-erosion
    /// PitFill — the "promoted lake" emergent signal. Stretch, not a hard
    /// gate; `0` is acceptable and is the expected Phase A value (the
    /// pipeline does not yet track promotion; reserved for a later stage).
    #[serde(default)]
    pub promoted_lake_count: u32,
    /// Sprint 3 DD2 / DD3: blake3 of `authoritative.sediment.data` bytes.
    /// Bit-exact on the same host. All-zero `[0u8; 32]` when sediment is
    /// absent (sentinel for "nothing to hash").
    #[serde(default)]
    pub sediment_blake3: [u8; 32],

    // ── Sprint 3.5 DD8 hex-surface witnesses ────────────────────────────────
    /// blake3 of a deterministic byte layout of `derived.hex_attrs`
    /// (DD2 witness). Pre-DD2 layout hashes the existing 8-field
    /// `HexAttributes` aggregation written by `HexProjectionStage`;
    /// DD2's aggregation kernel swap at 3.5.A c4 causes this hash to move.
    ///
    /// Format: `String` (64-hex). `#[serde(default)]` so pre-3.5 snapshots
    /// deserialize into an empty string (which will not match live-compute
    /// values — this is intentional, signalling "snapshot pre-dates DD8").
    #[serde(default)]
    pub hex_attrs_hash: String,
    /// blake3 of `derived.hex_debug.river_crossing` (DD3 witness).
    /// Pre-DD3 hashes the 4-edge encoding; DD3's 6-edge promotion at
    /// 3.5.B c1 causes this hash to move.
    #[serde(default)]
    pub hex_debug_river_crossing_hash: String,
    /// blake3 of the serialised `derived.hex_coast_class` (DD4 witness).
    /// Pre-DD4 (including at schema-lift time) the field is `None`, so
    /// this hash is a stable sentinel (`blake3("hex_coast_class:none:v1")`).
    /// DD4's classifier at 3.5.C c1 populates the field, causing this hash
    /// to move from sentinel → real.
    #[serde(default)]
    pub hex_coast_class_hash: String,
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

        // ── Sprint 2 v1 cheap + Sprint 3 v2 coast-type histograms ──────────────
        //
        // Both histograms share the same `derived.coast_type` byte field, and
        // differ only in which `CoastTypeVariant` produced it. Rather than
        // round-tripping the variant through a second read of `preset`, we
        // fill both unconditionally: the 5-bin v2 array is authoritative for
        // the default `V2FetchIntegral` path, and the 4-bin v1 array matches
        // the Sprint 2 layout exactly when the classifier picks from
        // `0..=3`. On V2 the `LavaDelta = 4` bin lights up and the v1 array
        // reports the `0..=3` subset — but the actively-consumed field for
        // that path is the v2 array (spec DD6).
        let (coast_type_counts_v1, coast_type_counts_v2, coast_type_blake3) =
            match world.derived.coast_type.as_ref() {
                Some(ct) => {
                    let mut counts_v1 = [0_u32; 4];
                    let mut counts_v2 = [0_u32; 5];
                    for &v in &ct.data {
                        let idx = v as usize;
                        if idx < 4 {
                            counts_v1[idx] += 1;
                        }
                        if idx < 5 {
                            counts_v2[idx] += 1;
                        }
                        // Unknown = 0xFF and any out-of-range sentinel is not counted.
                    }
                    // Under the default V2FetchIntegral classifier the v1 array
                    // is a partial view (missing LavaDelta); zero it out to
                    // match the spec's "set to [0; 4] under V2, compute on V1Cheap".
                    let variant = world.preset.erosion.coast_type_variant;
                    if matches!(
                        variant,
                        island_core::preset::CoastTypeVariant::V2FetchIntegral
                    ) {
                        counts_v1 = [0_u32; 4];
                    }
                    (counts_v1, counts_v2, blake3_field_u8(&ct.data))
                }
                None => ([0_u32; 4], [0_u32; 5], [0_u8; 32]),
            };

        // ── Sprint 3 sediment summaries ────────────────────────────────────────
        //
        // Both `mean_sediment_thickness` and `deposition_zone_fraction`
        // average over land cells only (the `is_land` mask we already used
        // for Sprint 1B). When `authoritative.sediment` is absent we report
        // the spec-mandated defaults — a partial pipeline run that stopped
        // before `CoastMaskStage` has nothing to summarise.
        let (mean_sediment_thickness, deposition_zone_fraction, sediment_blake3) =
            match world.authoritative.sediment.as_ref() {
                Some(sed) => {
                    let mut sum = 0.0_f64;
                    let mut deposition_cells = 0_u32;
                    for iy in 0..h {
                        for ix in 0..w {
                            if coast.is_land.get(ix, iy) != 1 {
                                continue;
                            }
                            let hs = sed.get(ix, iy);
                            sum += hs as f64;
                            if hs > DEPOSITION_FLAG_THRESHOLD {
                                deposition_cells += 1;
                            }
                        }
                    }
                    let mean = (sum / land_n_f) as f32;
                    let frac = (deposition_cells as f64 / land_n_f) as f32;
                    (mean, frac, blake3_field_f32(&sed.data))
                }
                None => (0.0_f32, 0.0_f32, [0u8; 32]),
            };

        // ── Sprint 3 fog summary ──────────────────────────────────────────────
        //
        // Mean `fog_water_input` over land cells where `fog_likelihood > 0.1`
        // (DD5 classification threshold). Both fields are `Option`s — if
        // either derived cache is missing we fall back to 0.0 rather than
        // panicking, matching the `coast_type` pattern above.
        let mean_fog_water_input = match (
            world.derived.fog_likelihood.as_ref(),
            world.derived.fog_water_input.as_ref(),
        ) {
            (Some(fog_like), Some(fog_water)) => {
                let mut sum = 0.0_f64;
                let mut n = 0_u32;
                for iy in 0..h {
                    for ix in 0..w {
                        if coast.is_land.get(ix, iy) != 1 {
                            continue;
                        }
                        if fog_like.get(ix, iy) > 0.1 {
                            sum += fog_water.get(ix, iy) as f64;
                            n += 1;
                        }
                    }
                }
                if n == 0 {
                    0.0_f32
                } else {
                    (sum / n as f64) as f32
                }
            }
            _ => 0.0_f32,
        };

        // ── Sprint 3 promoted-lake count (stretch signal) ─────────────────────
        //
        // The post-erosion "promoted lake" concept (§1 goal 8 / Sprint 2.5.G)
        // requires a separate derived field tracking which basin IDs survived
        // the final `PitFill` because they represented sediment-aware
        // deposition lakes. That infrastructure does not yet exist — see
        // CLAUDE.md's "`BasinsStage` post-BFS CC pass is currently vacuous"
        // gotcha: the final `PitFill` currently fills all interior
        // depressions. Phase A returns `0`, which the sprint-3 acceptance
        // criteria explicitly allow ("All-zero is acceptable as 'current
        // preset suite geometry has no closed depressions survivable through
        // PitFill' and does not fail Sprint 3"). Phase B / a follow-up sprint
        // can replace this stub with real basin-promotion tracking without
        // breaking any serialised summary.
        let promoted_lake_count: u32 = 0;

        // ── Sprint 3.5 DD8 hex-surface witnesses ─────────────────────────────
        let hex_attrs_hash = hash_hex_attrs(hex_attrs);
        let hex_debug_river_crossing_hash =
            hash_hex_river_crossing_opt(world.derived.hex_debug.as_ref());
        let hex_coast_class_hash = hash_hex_coast_class_sentinel();

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
            coast_type_counts_v1,
            erosion_sea_crossing_count,
            coast_type_blake3,
            mean_sediment_thickness,
            deposition_zone_fraction,
            coast_type_counts_v2,
            mean_fog_water_input,
            promoted_lake_count,
            sediment_blake3,
            hex_attrs_hash,
            hex_debug_river_crossing_hash,
            hex_coast_class_hash,
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

/// Sprint 3.5 DD8: hash the full contents of a [`HexAttributeField`].
///
/// Byte layout (all LE):
/// `cols(u32) | rows(u32) | for each cell (row-major):
///   { elevation(f32) | slope(f32) | rainfall(f32) | temperature(f32) |
///     moisture(f32) | biome_weights.len(u32) | biome_weights[i](f32)... |
///     dominant_biome(u8) | has_river(u8) }`
fn hash_hex_attrs(attrs: &HexAttributeField) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(&attrs.cols.to_le_bytes());
    hasher.update(&attrs.rows.to_le_bytes());
    for cell in &attrs.attrs {
        hasher.update(&cell.elevation.to_le_bytes());
        hasher.update(&cell.slope.to_le_bytes());
        hasher.update(&cell.rainfall.to_le_bytes());
        hasher.update(&cell.temperature.to_le_bytes());
        hasher.update(&cell.moisture.to_le_bytes());
        let bw_len = cell.biome_weights.len() as u32;
        hasher.update(&bw_len.to_le_bytes());
        for w in &cell.biome_weights {
            hasher.update(&w.to_le_bytes());
        }
        hasher.update(&[cell.dominant_biome as u8]);
        hasher.update(&[cell.has_river as u8]);
    }
    hasher.finalize().to_hex().to_string()
}

/// Sprint 3.5 DD8: hash the `river_crossing` field of [`HexDebugAttributes`].
///
/// Byte layout (all LE):
/// `cell_count(u32) | for each Option<HexRiverCrossing>:
///   { 0xFF 0xFF } for None OR { entry_edge(u8) exit_edge(u8) } for Some`
///
/// When `dbg` is `None` (hex_debug not yet computed), hashes the UTF-8 bytes
/// of the sentinel string `"hex_debug_river_crossing:none:v1"`.
fn hash_hex_river_crossing_opt(dbg: Option<&HexDebugAttributes>) -> String {
    match dbg {
        None => {
            let hash = blake3::hash(b"hex_debug_river_crossing:none:v1");
            hash.to_hex().to_string()
        }
        Some(d) => {
            let mut hasher = blake3::Hasher::new();
            let cell_count = d.river_crossing.len() as u32;
            hasher.update(&cell_count.to_le_bytes());
            for crossing in &d.river_crossing {
                match crossing {
                    None => {
                        hasher.update(&[0xFF, 0xFF]);
                    }
                    Some(rc) => {
                        hasher.update(&[rc.entry_edge, rc.exit_edge]);
                    }
                }
            }
            hasher.finalize().to_hex().to_string()
        }
    }
}

/// Sprint 3.5 DD8: stable sentinel hash for `hex_coast_class` before DD4
/// populates `derived.hex_coast_class` (3.5.C c1).
///
/// Returns the blake3 hex string of `"hex_coast_class:none:v1"`. Replaced
/// by a real hash in 3.5.C once `derived.hex_coast_class` is populated.
fn hash_hex_coast_class_sentinel() -> String {
    blake3::hash(b"hex_coast_class:none:v1")
        .to_hex()
        .to_string()
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

    /// Sprint 3 DD7: a legacy Sprint 2 `metrics.ron` (no Sprint 3 fields, old
    /// `coast_type_counts` field name) must continue to deserialize under the
    /// Sprint 3 `SummaryMetrics` schema. Sprint 3 fields default to zero /
    /// zeroed arrays via `#[serde(default)]`, and the old `coast_type_counts`
    /// key maps to `coast_type_counts_v1` via `#[serde(alias = ...)]`.
    #[test]
    fn summary_metrics_deserializes_legacy_baseline_via_serde_default() {
        // This is a verbatim-shape copy of the Sprint 2
        // `crates/data/golden/headless/sprint_1a_baseline/shots/
        // hero_volcanic_single_seed42/metrics.ron` as of Sprint 3.10 Phase A
        // pre-regen — no Sprint 3 fields present, old `coast_type_counts`
        // key name, shortened hashes to keep the literal readable.
        let legacy_ron = r#"(
            land_cell_count: 3209,
            coast_cell_count: 207,
            river_cell_count: 34,
            basin_count: 258,
            river_mouth_count: 6,
            max_elevation: 0.9985648,
            max_elevation_filled: 0.9985648,
            mean_slope: 0.0055860286,
            longest_river_length: 118.0,
            total_drainage_area: 2.6813354,
            mean_precipitation: 0.0037771726,
            windward_leeward_precip_ratio: 773.45764,
            mean_temperature_c: 19.105743,
            mean_soil_moisture: 0.15815213,
            biome_coverage_percent: (0.0, 0.0, 0.0, 0.0, 27.76566, 40.105953, 27.76566, 4.36273),
            hex_count: 4096,
            height_blake3: (198, 247, 10, 124, 138, 246, 163, 65, 190, 14, 19, 42, 180, 64, 52, 249, 238, 20, 127, 83, 25, 228, 64, 43, 229, 180, 177, 96, 51, 178, 225, 130),
            z_filled_blake3: (198, 247, 10, 124, 138, 246, 163, 65, 190, 14, 19, 42, 180, 64, 52, 249, 238, 20, 127, 83, 25, 228, 64, 43, 229, 180, 177, 96, 51, 178, 225, 130),
            flow_dir_blake3: (142, 209, 252, 140, 101, 59, 137, 179, 116, 197, 136, 9, 140, 6, 21, 147, 50, 15, 20, 88, 175, 12, 250, 80, 159, 124, 134, 93, 0, 165, 44, 238),
            accumulation_blake3: (52, 171, 168, 128, 177, 12, 96, 72, 166, 203, 122, 20, 110, 0, 7, 232, 231, 65, 241, 255, 234, 222, 100, 215, 248, 217, 245, 3, 19, 168, 251, 30),
            basin_id_blake3: (71, 54, 85, 203, 145, 63, 100, 231, 253, 198, 97, 59, 230, 80, 210, 220, 246, 41, 204, 58, 164, 171, 185, 175, 120, 232, 39, 126, 186, 44, 1, 35),
            river_mask_blake3: (149, 228, 158, 47, 52, 199, 125, 142, 1, 118, 206, 17, 204, 127, 30, 115, 78, 189, 233, 142, 250, 39, 183, 232, 194, 29, 93, 157, 127, 94, 89, 46),
            erosion_relief_drop_fraction: 0.0014352202,
            coast_type_counts: (0, 38, 3, 136),
            erosion_sea_crossing_count: 36,
            coast_type_blake3: (160, 125, 236, 16, 170, 162, 53, 101, 135, 247, 39, 175, 1, 204, 70, 143, 64, 208, 94, 49, 36, 4, 52, 224, 253, 204, 231, 162, 255, 190, 235, 239),
        )"#;

        let metrics: SummaryMetrics = ron::from_str(legacy_ron)
            .expect("Sprint 2 metrics.ron must deserialize under Sprint 3 schema");

        // Legacy-aliased field resolves correctly.
        assert_eq!(
            metrics.coast_type_counts_v1,
            [0, 38, 3, 136],
            "`coast_type_counts` alias must populate `coast_type_counts_v1`"
        );

        // Sprint 3 fields default to their zero values.
        assert_eq!(metrics.mean_sediment_thickness, 0.0);
        assert_eq!(metrics.deposition_zone_fraction, 0.0);
        assert_eq!(metrics.coast_type_counts_v2, [0_u32; 5]);
        assert_eq!(metrics.mean_fog_water_input, 0.0);
        assert_eq!(metrics.promoted_lake_count, 0);
        assert_eq!(metrics.sediment_blake3, [0_u8; 32]);

        // Sprint 3.5 DD8 fields default to empty string — the deliberate
        // signal that a pre-3.5 snapshot pre-dates DD8.
        assert_eq!(metrics.hex_attrs_hash, "");
        assert_eq!(metrics.hex_debug_river_crossing_hash, "");
        assert_eq!(metrics.hex_coast_class_hash, "");
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
