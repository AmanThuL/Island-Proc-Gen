//! Post-pipeline correctness invariants for the Sprint 1A `WorldState`.
//!
//! Each function checks one invariant and returns `Ok(())` on success or a
//! descriptive [`ValidationError`] variant on the first violation found.
//! None of these functions panic — a missing precondition field returns
//! `Err(MissingPrecondition)` instead.

use crate::neighborhood::{Neighborhood, neighbour_offsets};
use crate::preset::IslandAge;
use crate::world::{D8_OFFSETS, FLOW_DIR_SINK, WorldState};

// ─── error type ───────────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum ValidationError {
    #[error(
        "river termination: cell ({x}, {y}) in river_mask cannot reach a coast cell along flow_dir"
    )]
    RiverDoesNotTerminate { x: u32, y: u32 },

    #[error("river termination: river_mask contains cell ({x}, {y}) that is sea")]
    RiverInSea { x: u32, y: u32 },

    #[error("flow_dir forms a cycle containing ({x}, {y})")]
    FlowDirCycle { x: u32, y: u32 },

    #[error("accumulation monotone: cell ({x}, {y}) has A = {a_p} but downstream has A = {a_q}")]
    AccumulationNotMonotone { x: u32, y: u32, a_p: f32, a_q: f32 },

    #[error("coastline: cell ({x}, {y}) with z={z} below sea_level={sea_level} is not marked sea")]
    CoastlineBelowSeaLevelNotSea {
        x: u32,
        y: u32,
        z: f32,
        sea_level: f32,
    },

    #[error("coastline: cell ({x}, {y}) is coast but has no sea neighbour")]
    CoastlineCoastWithoutSeaNeighbour { x: u32, y: u32 },

    #[error("precipitation non-negative: cell ({x}, {y}) has P = {value}")]
    PrecipitationNegative { x: u32, y: u32, value: f32 },

    #[error("biome weights normalized: cell ({x}, {y}) sum = {sum} (tolerance {tol})")]
    BiomeWeightsNotNormalized { x: u32, y: u32, sum: f32, tol: f32 },

    #[error(
        "temperature range: cell ({x}, {y}) T = {value}°C outside [{lo}, {hi}] (sea_level={sea_c}, relief={peak_m}m)"
    )]
    TemperatureOutOfRange {
        x: u32,
        y: u32,
        value: f32,
        lo: f32,
        hi: f32,
        sea_c: f32,
        peak_m: f32,
    },

    #[error("hex attrs: hex ({col}, {row}) biome_weights length {got}, expected {expected}")]
    HexBiomeWeightsLengthMismatch {
        col: u32,
        row: u32,
        got: usize,
        expected: usize,
    },

    #[error("hex attrs: shape mismatch — cols={cols} rows={rows} but attrs.len()={got}")]
    HexAttrsShapeMismatch { cols: u32, rows: u32, got: usize },

    #[error("validation: missing precondition field '{field}' (stage must have run first)")]
    MissingPrecondition { field: &'static str },

    // ── Sprint 2 invariant errors ────────────────────────────────────────────
    /// A coast cell (is_coast == 1) carries a `coast_type` value outside the
    /// legal range `0..=4` defined by [`crate::world::CoastType`].
    ///
    /// Sprint 3 DD6 widened this range from `0..=3` to `0..=4` when
    /// [`crate::world::CoastType::LavaDelta`] was added; `0xFF` remains the
    /// `Unknown` sentinel.
    #[error(
        "coast_type: coast cell at flat index {cell_index} has out-of-range type value {value} (expected 0..=4)"
    )]
    CoastTypeOutOfRange { cell_index: usize, value: u8 },

    /// A non-coast cell carries a `coast_type` value other than the sentinel
    /// `0xFF` (`CoastType::Unknown`).
    #[error(
        "coast_type: non-coast cell at flat index {cell_index} has value {value:#04x} (expected 0xFF)"
    )]
    NonCoastCellNotUnknown { cell_index: usize, value: u8 },

    /// A basin occupies more than 50 % of land cells, indicating the partition
    /// is degenerate (e.g. the CC labelling accidentally merged unrelated regions).
    #[error(
        "basin partition: basin id {basin_id} covers {count} cells ({fraction:.1}% of {land_total} land cells, exceeds 50% limit)"
    )]
    BasinExceedsHalfLand {
        basin_id: u32,
        count: u32,
        fraction: f32,
        land_total: u32,
    },

    /// The sum of cells with `basin_id > 0` exceeds `land_cell_count`.
    #[error(
        "basin partition: {labeled_cells} labeled cells (basin_id > 0) exceed land_cell_count {land_total}"
    )]
    BasinLabeledCellsExceedLand { labeled_cells: u32, land_total: u32 },

    // ── Sprint 3 invariant errors ────────────────────────────────────────────
    /// `authoritative.sediment` is `None` after `CoastMaskStage` has run.
    /// Sprint 3 initialises the field in the `Sediment` arm; if it is still
    /// `None` the init hook did not fire.
    #[error("sediment_bounded: authoritative.sediment is None (SedimentUpdateStage must have run)")]
    SedimentFieldMissing,

    /// A land cell carries a sediment thickness `hs` that is negative,
    /// greater than 1.0, NaN, or infinite.
    #[error(
        "sediment_bounded: land cell at flat index {cell_index} has hs = {value} (expected [0, 1], finite)"
    )]
    SedimentOutOfRange { cell_index: usize, value: f32 },

    /// A sea cell carries a non-zero sediment thickness. Sea cells must
    /// always have `hs = 0.0`.
    #[error(
        "sediment_bounded: sea cell at flat index {cell_index} has hs = {value} (expected 0.0)"
    )]
    SedimentSeaCellNonZero { cell_index: usize, value: f32 },

    /// The fraction of land cells with `hs > DEPOSITION_FLAG_THRESHOLD` fell
    /// below the `[LOW, HIGH]` realistic band. Indicates either that
    /// sediment is not accumulating at all (low) or that the entire island
    /// surface is submerged in sediment (high).
    #[error(
        "deposition_zone_fraction_realistic: fraction {fraction:.4} of land cells with hs > {threshold} is outside [{lo:.2}, {hi:.2}]"
    )]
    DepositionZoneFractionOutOfRange {
        fraction: f32,
        threshold: f32,
        lo: f32,
        hi: f32,
    },

    /// A coast cell carries a `coast_type` discriminant outside `0..=4`.
    /// Distinct from [`ValidationError::CoastTypeOutOfRange`]: this variant
    /// is emitted by [`coast_type_v2_well_formed`] which enforces the
    /// additive LavaDelta-age constraint; the original `coast_type_well_formed`
    /// still emits `CoastTypeOutOfRange`.
    #[error(
        "coast_type_v2_well_formed: coast cell at flat index {cell_index} has discriminant {value} (expected 0..=4)"
    )]
    CoastTypeV2DiscOutOfRange { cell_index: usize, value: u8 },

    /// A non-Young preset has LavaDelta coast cells. LavaDelta (discriminant 4)
    /// may only appear on islands with `IslandAge::Young`.
    #[error(
        "coast_type_v2_well_formed: {count} LavaDelta cell(s) found on non-Young preset (island_age = {island_age:?})"
    )]
    LavaDeltaOnNonYoungPreset { count: usize, island_age: IslandAge },

    /// The mean V3Lfpm precipitation across land cells is outside the
    /// physically plausible band `[PRECIP_MEAN_LO, PRECIP_MEAN_HI]`.
    ///
    /// Since the V3 sweep normalises `P ∈ [0, 1]`, a mean far below 0.01
    /// indicates the pipeline produced essentially zero rain (physics
    /// broken), and a mean above 1.0 indicates something upstream emitted
    /// values outside the normalised range (numerical explosion).
    #[error(
        "precipitation_mass_balance: mean precipitation {mean:.6} outside [{lo:.4}, {hi:.4}] on V3Lfpm"
    )]
    PrecipitationMassBalanceViolation { mean: f32, lo: f32, hi: f32 },

    /// A height value became non-finite (NaN or ±∞) during erosion.
    #[error("erosion: height at flat index {cell_index} is non-finite ({value})")]
    ErosionHeightNonFinite { cell_index: usize, value: f32 },

    /// The post-erosion height maximum grew beyond the pre-erosion ceiling times
    /// [`EROSION_MAX_GROWTH_FACTOR`].
    #[error(
        "erosion: post-erosion max height {max_post} exceeds pre-erosion max {max_pre} * {factor} growth factor"
    )]
    ErosionExplosion {
        max_pre: f32,
        max_post: f32,
        factor: f32,
    },

    /// More than [`EROSION_MAX_SEA_CROSSING_FRACTION`] of the pre-erosion
    /// land cells crossed the sea-level threshold during erosion.
    #[error(
        "erosion: land-cell count changed from {pre_land} to {post_land} ({fraction} fractional delta exceeds 0.05 limit)"
    )]
    ErosionExcessiveSeaCrossing {
        pre_land: u32,
        post_land: u32,
        fraction: f32,
    },
}

// ─── public validators ────────────────────────────────────────────────────────

/// Every river cell must be able to reach a coast or sea cell along `flow_dir`.
pub fn river_termination(world: &WorldState) -> Result<(), ValidationError> {
    let river_mask =
        world
            .derived
            .river_mask
            .as_ref()
            .ok_or(ValidationError::MissingPrecondition {
                field: "derived.river_mask",
            })?;

    let coast_mask =
        world
            .derived
            .coast_mask
            .as_ref()
            .ok_or(ValidationError::MissingPrecondition {
                field: "derived.coast_mask",
            })?;

    let flow_dir = world
        .derived
        .flow_dir
        .as_ref()
        .ok_or(ValidationError::MissingPrecondition {
            field: "derived.flow_dir",
        })?;

    let w = river_mask.width as usize;
    let h = river_mask.height as usize;
    let max_steps = w * h;

    for y in 0..h {
        for x in 0..w {
            if river_mask.get(x as u32, y as u32) == 0 {
                continue;
            }

            let (ox, oy) = (x as u32, y as u32);

            // River cells must be on land, not sea.
            if coast_mask.is_sea.get(ox, oy) == 1 {
                return Err(ValidationError::RiverInSea { x: ox, y: oy });
            }

            // Walk along flow_dir until we reach a water body or exhaust steps.
            let (mut cx, mut cy) = (x as i32, y as i32);
            let mut ok = false;

            for _ in 0..=max_steps {
                let (cxu, cyu) = (cx as u32, cy as u32);

                if coast_mask.is_coast.get(cxu, cyu) == 1 || coast_mask.is_sea.get(cxu, cyu) == 1 {
                    ok = true;
                    break;
                }

                let dir = flow_dir.get(cxu, cyu);
                if dir == FLOW_DIR_SINK {
                    // Non-coast, non-sea sink — closed basin.
                    break;
                }

                let (dx, dy) = D8_OFFSETS[dir as usize];
                let nx = cx + dx;
                let ny = cy + dy;
                if nx < 0 || nx >= w as i32 || ny < 0 || ny >= h as i32 {
                    // Flowed off-grid — treat as terminated at boundary.
                    ok = true;
                    break;
                }
                cx = nx;
                cy = ny;
            }

            if !ok {
                return Err(ValidationError::RiverDoesNotTerminate { x: ox, y: oy });
            }
        }
    }

    Ok(())
}

/// `flow_dir` forms a DAG (no cycles). Cycle detection via topological sort.
pub fn basin_partition_dag(world: &WorldState) -> Result<(), ValidationError> {
    let flow_dir = world
        .derived
        .flow_dir
        .as_ref()
        .ok_or(ValidationError::MissingPrecondition {
            field: "derived.flow_dir",
        })?;

    let w = flow_dir.width as usize;
    let h = flow_dir.height as usize;
    let n = w * h;

    // Build in-degree table.
    let mut indeg: Vec<u32> = vec![0; n];

    for y in 0..h {
        for x in 0..w {
            let dir = flow_dir.get(x as u32, y as u32);
            if dir == FLOW_DIR_SINK {
                continue;
            }
            let (dx, dy) = D8_OFFSETS[dir as usize];
            let qx = x as i32 + dx;
            let qy = y as i32 + dy;
            if qx >= 0 && qx < w as i32 && qy >= 0 && qy < h as i32 {
                indeg[qy as usize * w + qx as usize] += 1;
            }
        }
    }

    // Kahn's BFS: visit all indeg=0 cells.
    let mut queue: std::collections::VecDeque<u32> =
        (0..n as u32).filter(|&p| indeg[p as usize] == 0).collect();
    let mut visited: u32 = 0;

    while let Some(p) = queue.pop_front() {
        visited += 1;
        let x = p as usize % w;
        let y = p as usize / w;
        let dir = flow_dir.get(x as u32, y as u32);
        if dir == FLOW_DIR_SINK {
            continue;
        }
        let (dx, dy) = D8_OFFSETS[dir as usize];
        let qx = x as i32 + dx;
        let qy = y as i32 + dy;
        if qx < 0 || qx >= w as i32 || qy < 0 || qy >= h as i32 {
            continue;
        }
        let q = qy as usize * w + qx as usize;
        indeg[q] -= 1;
        if indeg[q] == 0 {
            queue.push_back(q as u32);
        }
    }

    if visited < n as u32 {
        // Find the first unvisited cell (residual indeg > 0) to report.
        for (p, &deg) in indeg.iter().enumerate() {
            if deg > 0 {
                let x = (p % w) as u32;
                let y = (p / w) as u32;
                return Err(ValidationError::FlowDirCycle { x, y });
            }
        }
    }

    Ok(())
}

/// `A[down(p)] >= A[p]` for every non-sink cell p.
pub fn accumulation_monotone(world: &WorldState) -> Result<(), ValidationError> {
    let accumulation =
        world
            .derived
            .accumulation
            .as_ref()
            .ok_or(ValidationError::MissingPrecondition {
                field: "derived.accumulation",
            })?;

    let flow_dir = world
        .derived
        .flow_dir
        .as_ref()
        .ok_or(ValidationError::MissingPrecondition {
            field: "derived.flow_dir",
        })?;

    let w = flow_dir.width as usize;
    let h = flow_dir.height as usize;
    const EPS: f32 = 1e-5;

    for y in 0..h {
        for x in 0..w {
            let dir = flow_dir.get(x as u32, y as u32);
            if dir == FLOW_DIR_SINK {
                continue;
            }
            let (dx, dy) = D8_OFFSETS[dir as usize];
            let qx = x as i32 + dx;
            let qy = y as i32 + dy;
            if qx < 0 || qx >= w as i32 || qy < 0 || qy >= h as i32 {
                continue;
            }
            let a_p = accumulation.get(x as u32, y as u32);
            let a_q = accumulation.get(qx as u32, qy as u32);
            if a_q < a_p - EPS {
                return Err(ValidationError::AccumulationNotMonotone {
                    x: x as u32,
                    y: y as u32,
                    a_p,
                    a_q,
                });
            }
        }
    }

    Ok(())
}

/// Two sub-checks: z < sea_level → is_sea; is_coast → has at least one Von4 sea neighbour.
pub fn coastline_consistency(world: &WorldState) -> Result<(), ValidationError> {
    let height =
        world
            .authoritative
            .height
            .as_ref()
            .ok_or(ValidationError::MissingPrecondition {
                field: "authoritative.height",
            })?;

    let coast_mask =
        world
            .derived
            .coast_mask
            .as_ref()
            .ok_or(ValidationError::MissingPrecondition {
                field: "derived.coast_mask",
            })?;

    let sea_level = world.preset.sea_level;
    let w = height.width as usize;
    let h = height.height as usize;

    for y in 0..h {
        for x in 0..w {
            let (xu, yu) = (x as u32, y as u32);
            let z = height.get(xu, yu);

            // Sub-check 1: z < sea_level must be is_sea.
            if z < sea_level && coast_mask.is_sea.get(xu, yu) == 0 {
                return Err(ValidationError::CoastlineBelowSeaLevelNotSea {
                    x: xu,
                    y: yu,
                    z,
                    sea_level,
                });
            }

            // Sub-check 2: coast cell must have at least one Von4 sea neighbour.
            if coast_mask.is_coast.get(xu, yu) == 1 {
                let has_sea_nbr = neighbour_offsets(Neighborhood::Von4)
                    .iter()
                    .any(|&(dx, dy)| {
                        let nx = x as i32 + dx;
                        let ny = y as i32 + dy;
                        nx >= 0
                            && nx < w as i32
                            && ny >= 0
                            && ny < h as i32
                            && coast_mask.is_sea.get(nx as u32, ny as u32) == 1
                    });
                if !has_sea_nbr {
                    return Err(ValidationError::CoastlineCoastWithoutSeaNeighbour {
                        x: xu,
                        y: yu,
                    });
                }
            }
        }
    }

    Ok(())
}

// ─── Sprint 1B invariants ─────────────────────────────────────────────────────

/// Every cell of `world.baked.precipitation` is `>= 0`.
pub fn precipitation_nonneg(world: &WorldState) -> Result<(), ValidationError> {
    let precip =
        world
            .baked
            .precipitation
            .as_ref()
            .ok_or(ValidationError::MissingPrecondition {
                field: "baked.precipitation",
            })?;
    for y in 0..precip.height {
        for x in 0..precip.width {
            let v = precip.get(x, y);
            if v < 0.0 {
                return Err(ValidationError::PrecipitationNegative { x, y, value: v });
            }
        }
    }
    Ok(())
}

/// Per-land-cell biome weight sum approximately equals `1.0`.
pub fn biome_weights_normalized(world: &WorldState) -> Result<(), ValidationError> {
    let bw = world
        .baked
        .biome_weights
        .as_ref()
        .ok_or(ValidationError::MissingPrecondition {
            field: "baked.biome_weights",
        })?;
    let coast = world
        .derived
        .coast_mask
        .as_ref()
        .ok_or(ValidationError::MissingPrecondition {
            field: "derived.coast_mask",
        })?;

    const TOL: f32 = 1e-4;
    for y in 0..bw.height {
        for x in 0..bw.width {
            if coast.is_land.get(x, y) != 1 {
                continue;
            }
            let idx = bw.index(x, y);
            let sum: f32 = bw.weights.iter().map(|row| row[idx]).sum();
            if (sum - 1.0).abs() > TOL {
                return Err(ValidationError::BiomeWeightsNotNormalized {
                    x,
                    y,
                    sum,
                    tol: TOL,
                });
            }
        }
    }
    Ok(())
}

/// Every cell temperature sits between the lapse-rate-derived minimum
/// and sea-level-plus-coastal-modifier maximum, within a small slack.
pub fn temperature_physical_range(world: &WorldState) -> Result<(), ValidationError> {
    let temperature =
        world
            .baked
            .temperature
            .as_ref()
            .ok_or(ValidationError::MissingPrecondition {
                field: "baked.temperature",
            })?;

    // Physical bounds from the Sprint 1B TemperatureStage contract.
    // `TemperatureStage` owns the numeric constants, so we recompute
    // the bounds from the preset here rather than hardcoding a copy.
    const T_SEA_LEVEL_C: f32 = 26.0;
    const LAPSE_RATE_C_PER_KM: f32 = 6.5;
    const COASTAL_MODIFIER_C: f32 = 2.0;
    const SLACK: f32 = 1.0;

    let peak_m = crate::preset::MAX_RELIEF_REF_M * world.preset.max_relief;
    let max_lapse = LAPSE_RATE_C_PER_KM * peak_m / 1000.0;
    let lo = T_SEA_LEVEL_C - max_lapse - SLACK;
    let hi = T_SEA_LEVEL_C + COASTAL_MODIFIER_C + SLACK;

    for y in 0..temperature.height {
        for x in 0..temperature.width {
            let v = temperature.get(x, y);
            if v < lo || v > hi {
                return Err(ValidationError::TemperatureOutOfRange {
                    x,
                    y,
                    value: v,
                    lo,
                    hi,
                    sea_c: T_SEA_LEVEL_C,
                    peak_m,
                });
            }
        }
    }
    Ok(())
}

/// `hex_attrs.attrs.len() == cols * rows`, and every entry's
/// `biome_weights` vector length matches the canonical biome count.
pub fn hex_attrs_present(world: &WorldState) -> Result<(), ValidationError> {
    let attrs = world
        .derived
        .hex_attrs
        .as_ref()
        .ok_or(ValidationError::MissingPrecondition {
            field: "derived.hex_attrs",
        })?;

    let expected = (attrs.cols * attrs.rows) as usize;
    if attrs.attrs.len() != expected {
        return Err(ValidationError::HexAttrsShapeMismatch {
            cols: attrs.cols,
            rows: attrs.rows,
            got: attrs.attrs.len(),
        });
    }

    let expected_biome_count = crate::world::BiomeType::COUNT;
    for (i, hex) in attrs.attrs.iter().enumerate() {
        if hex.biome_weights.len() != expected_biome_count {
            let col = (i as u32) % attrs.cols;
            let row = (i as u32) / attrs.cols;
            return Err(ValidationError::HexBiomeWeightsLengthMismatch {
                col,
                row,
                got: hex.biome_weights.len(),
                expected: expected_biome_count,
            });
        }
    }
    Ok(())
}

// ─── Sprint 2 invariants ──────────────────────────────────────────────────────

/// Maximum ratio by which the post-erosion height ceiling may exceed the
/// pre-erosion ceiling before `erosion_no_explosion` fires.
///
/// Sprint 2 §8: SPIM is a net-transport operator — sediment leaves peaks and
/// deposits downstream or at the coast. A 5 % growth allowance absorbs
/// floating-point accumulation rounding across many inner iterations while
/// still catching genuine numerical blow-up.
pub const EROSION_MAX_GROWTH_FACTOR: f32 = 1.05;

/// Maximum fraction of pre-erosion land cells that may cross the sea-level
/// threshold (land → sea) during a single full erosion run before
/// `erosion_no_excessive_sea_crossing` fires.
///
/// Sprint 2 §8: a 5 % sea-crossing limit bounds the worst-case island
/// shrinkage caused by mis-tuned erosion strength or duration parameters.
pub const EROSION_MAX_SEA_CROSSING_FRACTION: f32 = 0.05;

/// Post-erosion basin partition well-formedness check (Task 2.5.G).
///
/// Checks two sub-invariants:
/// 1. No basin has area > 50 % of total land cells (degenerate merge guard).
/// 2. `sum(cells with basin_id > 0) <= land_cell_count` (labeled cells are a
///    subset of land; small unlabeled sink CCs may keep basin_id = 0).
///
/// Returns `Ok(())` immediately if either `derived.basin_id` or
/// `derived.coast_mask` is `None` (stage hasn't run yet — skip).
pub fn basin_partition_post_erosion_well_formed(world: &WorldState) -> Result<(), ValidationError> {
    let basin_id = match world.derived.basin_id.as_ref() {
        Some(b) => b,
        None => return Ok(()),
    };
    let coast_mask = match world.derived.coast_mask.as_ref() {
        Some(m) => m,
        None => return Ok(()),
    };

    let land_total = coast_mask.land_cell_count;
    if land_total == 0 {
        return Ok(()); // all-sea preset; nothing to check.
    }

    let mut counts: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
    let mut labeled_cells: u32 = 0;

    for &id in &basin_id.data {
        if id > 0 {
            *counts.entry(id).or_insert(0) += 1;
            labeled_cells += 1;
        }
    }

    // Sub-invariant 1: no single basin > 50 % of land.
    // Integer halving is intentionally conservative (rounds down).
    let half = land_total / 2;
    for (&id, &count) in &counts {
        if count > half {
            let fraction = count as f32 / land_total as f32 * 100.0;
            return Err(ValidationError::BasinExceedsHalfLand {
                basin_id: id,
                count,
                fraction,
                land_total,
            });
        }
    }

    // Sub-invariant 2: labeled cells <= land total.
    if labeled_cells > land_total {
        return Err(ValidationError::BasinLabeledCellsExceedLand {
            labeled_cells,
            land_total,
        });
    }

    Ok(())
}

/// Every coast cell's `coast_type` byte must be in `0..=4`; every non-coast
/// cell must carry the sentinel `0xFF` (`CoastType::Unknown`).
///
/// Sprint 3 DD6 widened the legal range from `0..=3` to `0..=4` when
/// [`crate::world::CoastType::LavaDelta`] (discriminant 4) was added. The
/// Sprint 2 v1 classifier never emits discriminant 4; the Sprint 3 v2
/// classifier may emit it on Young presets near volcanic centers.
///
/// Returns `Ok(())` immediately if either `derived.coast_mask` or
/// `derived.coast_type` is `None` (stage hasn't run yet — skip rather than
/// error).
pub fn coast_type_well_formed(world: &WorldState) -> Result<(), ValidationError> {
    let coast_mask = match world.derived.coast_mask.as_ref() {
        Some(m) => m,
        None => return Ok(()),
    };
    let coast_type = match world.derived.coast_type.as_ref() {
        Some(ct) => ct,
        None => return Ok(()),
    };

    for (i, (&is_coast, &ct_value)) in coast_mask
        .is_coast
        .data
        .iter()
        .zip(coast_type.data.iter())
        .enumerate()
    {
        // Sprint 3 DD6: widened from `> 3` to `> 4` to admit LavaDelta.
        // The 0xFF Unknown sentinel on a coast cell still fails (0xFF > 4).
        if is_coast == 1 && ct_value > 4 {
            return Err(ValidationError::CoastTypeOutOfRange {
                cell_index: i,
                value: ct_value,
            });
        } else if is_coast != 1 && ct_value != 0xFF {
            return Err(ValidationError::NonCoastCellNotUnknown {
                cell_index: i,
                value: ct_value,
            });
        }
    }

    Ok(())
}

/// Post-erosion height field must be finite everywhere, and the new maximum
/// must not exceed `baseline.max_height_pre * EROSION_MAX_GROWTH_FACTOR`.
///
/// Returns `Ok(())` immediately if `authoritative.height` or
/// `derived.erosion_baseline` is `None` (skip).
pub fn erosion_no_explosion(world: &WorldState) -> Result<(), ValidationError> {
    let height = match world.authoritative.height.as_ref() {
        Some(h) => h,
        None => return Ok(()),
    };
    let baseline = match world.derived.erosion_baseline.as_ref() {
        Some(b) => b,
        None => return Ok(()),
    };

    let mut max_now = f32::NEG_INFINITY;
    for (i, &v) in height.data.iter().enumerate() {
        if !v.is_finite() {
            return Err(ValidationError::ErosionHeightNonFinite {
                cell_index: i,
                value: v,
            });
        }
        if v > max_now {
            max_now = v;
        }
    }

    let ceiling = baseline.max_height_pre * EROSION_MAX_GROWTH_FACTOR;
    if max_now > ceiling {
        return Err(ValidationError::ErosionExplosion {
            max_pre: baseline.max_height_pre,
            max_post: max_now,
            factor: EROSION_MAX_GROWTH_FACTOR,
        });
    }

    Ok(())
}

/// The fraction of land cells that crossed the sea-level threshold during
/// erosion must not exceed `EROSION_MAX_SEA_CROSSING_FRACTION`.
///
/// Returns `Ok(())` immediately if `derived.coast_mask` or
/// `derived.erosion_baseline` is `None`, or if `baseline.land_cell_count_pre
/// == 0` (all-sea preset; skip).
pub fn erosion_no_excessive_sea_crossing(world: &WorldState) -> Result<(), ValidationError> {
    let coast_mask = match world.derived.coast_mask.as_ref() {
        Some(m) => m,
        None => return Ok(()),
    };
    let baseline = match world.derived.erosion_baseline.as_ref() {
        Some(b) => b,
        None => return Ok(()),
    };

    let pre = baseline.land_cell_count_pre;
    if pre == 0 {
        return Ok(());
    }

    let post = coast_mask.land_cell_count;
    let delta = (pre as i64 - post as i64).unsigned_abs() as f32;
    let fraction = delta / pre as f32;

    if fraction > EROSION_MAX_SEA_CROSSING_FRACTION {
        return Err(ValidationError::ErosionExcessiveSeaCrossing {
            pre_land: pre,
            post_land: post,
            fraction,
        });
    }

    Ok(())
}

// ─── Sprint 3 invariants ──────────────────────────────────────────────────────

/// Sediment thickness above which a land cell is counted as a "deposition zone"
/// by [`deposition_zone_fraction_realistic`].
pub const DEPOSITION_FLAG_THRESHOLD: f32 = 0.15;

/// Lower bound on the fraction of land cells in a deposition zone.
///
/// Set to `0.0` in v1: at the Sprint 3 SPACE-lite parameter calibration
/// (K_Q = 2e-2, K_bed = 5e-3, 10×10 outer loop), transport capacity
/// generally exceeded incoming Qs on small/medium grids (64²–128²), so
/// net-deposition cells sat near 0. A non-zero lower bound would
/// false-positive on all stock presets. Sprint 3.1's K-probe outcome
/// (see `SPACE_K_BED_DEFAULT` in `crates/sim/src/geomorph/sediment.rs`)
/// left this bound at `0.0`; Sprint 4's physical-unit calibration is
/// the natural place to revisit.
pub const DEPOSITION_ZONE_FRACTION_LO: f32 = 0.0;

/// Upper bound on the fraction of land cells in a deposition zone.
/// Above this value the entire island is sediment-submerged — numerical
/// explosion in the deposition stage.
pub const DEPOSITION_ZONE_FRACTION_HI: f32 = 0.70;

/// Lower bound on mean V3Lfpm precipitation (normalised `[0, 1]`).
/// A mean below this threshold means the sweep produced essentially zero rain.
pub const PRECIP_MEAN_LO: f32 = 1e-4;

/// Upper bound on mean V3Lfpm precipitation (normalised `[0, 1]`).
/// The normalised field is bounded by 1.0 per cell; a mean above this
/// threshold signals out-of-range values from a numerical explosion.
pub const PRECIP_MEAN_HI: f32 = 1.0;

/// Every land cell's sediment thickness `hs` must be finite and in `[0, 1]`;
/// every sea cell must have `hs == 0.0`.
///
/// Returns `Err(SedimentFieldMissing)` if `authoritative.sediment` is `None`
/// (the Sprint 3 init hook in `SedimentUpdateStage` must have run).
///
/// # Errors
///
/// * [`ValidationError::SedimentFieldMissing`] — field is absent.
/// * [`ValidationError::SedimentOutOfRange`] — land cell `hs` outside `[0, 1]` or non-finite.
/// * [`ValidationError::SedimentSeaCellNonZero`] — sea cell `hs != 0.0`.
pub fn sediment_bounded(world: &WorldState) -> Result<(), ValidationError> {
    let sediment = world
        .authoritative
        .sediment
        .as_ref()
        .ok_or(ValidationError::SedimentFieldMissing)?;

    let coast_mask =
        world
            .derived
            .coast_mask
            .as_ref()
            .ok_or(ValidationError::MissingPrecondition {
                field: "derived.coast_mask",
            })?;

    for (i, (&hs, &is_sea)) in sediment
        .data
        .iter()
        .zip(coast_mask.is_sea.data.iter())
        .enumerate()
    {
        if is_sea == 1 {
            // Sea cells must be exactly zero.
            if hs != 0.0 {
                return Err(ValidationError::SedimentSeaCellNonZero {
                    cell_index: i,
                    value: hs,
                });
            }
        } else {
            // Land cells (including coast): finite and in [0, 1].
            if !hs.is_finite() || !(0.0..=1.0).contains(&hs) {
                return Err(ValidationError::SedimentOutOfRange {
                    cell_index: i,
                    value: hs,
                });
            }
        }
    }

    Ok(())
}

/// The fraction of land cells with `hs > `[`DEPOSITION_FLAG_THRESHOLD`] must
/// lie within `[`[`DEPOSITION_ZONE_FRACTION_LO`]`, `[`DEPOSITION_ZONE_FRACTION_HI`]`]`.
///
/// In v1 `DEPOSITION_ZONE_FRACTION_LO = 0.0`, so only the upper bound is
/// actively enforced. At current SPACE-lite parameter calibration, transport
/// capacity generally exceeds incoming Qs on 64²–128² grids, producing
/// near-zero deposition fractions on stock presets. The lower bound will be
/// tightened in Task 3.10 once 256² hero-shot calibration is complete.
///
/// Returns `Ok(())` immediately if `land_cell_count == 0` (all-sea preset;
/// no deposition zones to fraction-count).
///
/// # Errors
///
/// * [`ValidationError::SedimentFieldMissing`] — `authoritative.sediment` absent.
/// * [`ValidationError::MissingPrecondition`] — `derived.coast_mask` absent.
/// * [`ValidationError::DepositionZoneFractionOutOfRange`] — fraction outside `[LO, HI]`.
pub fn deposition_zone_fraction_realistic(world: &WorldState) -> Result<(), ValidationError> {
    let sediment = world
        .authoritative
        .sediment
        .as_ref()
        .ok_or(ValidationError::SedimentFieldMissing)?;

    let coast_mask =
        world
            .derived
            .coast_mask
            .as_ref()
            .ok_or(ValidationError::MissingPrecondition {
                field: "derived.coast_mask",
            })?;

    let land_total = coast_mask.land_cell_count;
    if land_total == 0 {
        return Ok(()); // all-sea preset; no land → skip.
    }

    let deposition_count = sediment
        .data
        .iter()
        .zip(coast_mask.is_land.data.iter())
        .filter(|&(&hs, &is_land)| is_land == 1 && hs > DEPOSITION_FLAG_THRESHOLD)
        .count() as u32;

    let fraction = deposition_count as f32 / land_total as f32;

    if !(DEPOSITION_ZONE_FRACTION_LO..=DEPOSITION_ZONE_FRACTION_HI).contains(&fraction) {
        return Err(ValidationError::DepositionZoneFractionOutOfRange {
            fraction,
            threshold: DEPOSITION_FLAG_THRESHOLD,
            lo: DEPOSITION_ZONE_FRACTION_LO,
            hi: DEPOSITION_ZONE_FRACTION_HI,
        });
    }

    Ok(())
}

/// Sprint 3 Task 3.9 additive coast-type constraint.
///
/// Enforces two sub-invariants on top of [`coast_type_well_formed`]:
/// 1. Every coast cell (`is_coast == 1`) has discriminant in `0..=4`.
/// 2. `LavaDelta` (discriminant 4) may only appear when
///    `preset.island_age == IslandAge::Young`. Mature and Old presets must
///    have zero LavaDelta cells.
///
/// This is a **separate, additive** invariant — [`coast_type_well_formed`]
/// is not replaced or modified.
///
/// Returns `Ok(())` immediately if either `derived.coast_mask` or
/// `derived.coast_type` is `None` (self-skipping; stage hasn't run).
///
/// # Errors
///
/// * [`ValidationError::CoastTypeV2DiscOutOfRange`] — coast cell discriminant > 4.
/// * [`ValidationError::LavaDeltaOnNonYoungPreset`] — LavaDelta cells on Mature / Old island.
pub fn coast_type_v2_well_formed(world: &WorldState) -> Result<(), ValidationError> {
    let coast_mask = match world.derived.coast_mask.as_ref() {
        Some(m) => m,
        None => return Ok(()),
    };
    let coast_type = match world.derived.coast_type.as_ref() {
        Some(ct) => ct,
        None => return Ok(()),
    };

    const LAVA_DELTA_DISC: u8 = 4;
    let mut lava_delta_count: usize = 0;

    for (i, (&is_coast, &ct_value)) in coast_mask
        .is_coast
        .data
        .iter()
        .zip(coast_type.data.iter())
        .enumerate()
    {
        if is_coast == 1 {
            if ct_value > LAVA_DELTA_DISC {
                return Err(ValidationError::CoastTypeV2DiscOutOfRange {
                    cell_index: i,
                    value: ct_value,
                });
            }
            if ct_value == LAVA_DELTA_DISC {
                lava_delta_count += 1;
            }
        }
    }

    // Young-only constraint: non-Young presets must have zero LavaDelta cells.
    if lava_delta_count > 0 && world.preset.island_age != IslandAge::Young {
        return Err(ValidationError::LavaDeltaOnNonYoungPreset {
            count: lava_delta_count,
            island_age: world.preset.island_age,
        });
    }

    Ok(())
}

/// V3Lfpm precipitation mass-balance sanity check.
///
/// Only runs when `preset.climate.precipitation_variant == V3Lfpm`; callers
/// (and [`crate::sim::ValidationStage`]) must gate on this condition and skip
/// for `V2Raymarch`.
///
/// The V3 sweep normalises `P ∈ [0, 1]` per cell in its last step. A mean
/// below [`PRECIP_MEAN_LO`] means the pipeline produced effectively zero rain;
/// a mean above [`PRECIP_MEAN_HI`] means values leaked outside the `[0, 1]`
/// normalisation range. Both are pipeline-breakage indicators, not
/// calibration issues.
///
/// Decision (Task 3.9): use a simple `mean_p ∈ [PRECIP_MEAN_LO, PRECIP_MEAN_HI]`
/// guard rather than an analytical "half-saturation" proxy, because (a) the
/// analytical proxy requires knowing the per-cell normalisation scale which is
/// internal to the V3 sweep, and (b) the simpler check reliably catches the
/// two real failure modes (zero rain, out-of-range explosion) without false-
/// positive-ing on the 5 shipped archetypes where measured mean P ∈ [0.1, 0.8].
///
/// Returns `Err(MissingPrecondition)` if `baked.precipitation` is `None`;
/// callers that gate on V2/V3 dispatch should call this only after the
/// precipitation stage has run.
///
/// # Errors
///
/// * [`ValidationError::MissingPrecondition`] — `baked.precipitation` absent.
/// * [`ValidationError::PrecipitationMassBalanceViolation`] — mean outside `[LO, HI]`.
pub fn precipitation_mass_balance(world: &WorldState) -> Result<(), ValidationError> {
    let precip =
        world
            .baked
            .precipitation
            .as_ref()
            .ok_or(ValidationError::MissingPrecondition {
                field: "baked.precipitation",
            })?;

    let coast_mask =
        world
            .derived
            .coast_mask
            .as_ref()
            .ok_or(ValidationError::MissingPrecondition {
                field: "derived.coast_mask",
            })?;

    let land_total = coast_mask.land_cell_count;
    if land_total == 0 {
        return Ok(()); // all-sea preset; no land → skip.
    }

    let sum_p: f32 = precip
        .data
        .iter()
        .zip(coast_mask.is_land.data.iter())
        .filter(|&(_, &is_land)| is_land == 1)
        .map(|(&p, _)| p)
        .sum();

    let mean_p = sum_p / land_total as f32;

    if !(PRECIP_MEAN_LO..=PRECIP_MEAN_HI).contains(&mean_p) {
        return Err(ValidationError::PrecipitationMassBalanceViolation {
            mean: mean_p,
            lo: PRECIP_MEAN_LO,
            hi: PRECIP_MEAN_HI,
        });
    }

    Ok(())
}

// ─── tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::field::{MaskField2D, ScalarField2D};
    use crate::preset::IslandAge;
    use crate::preset::IslandArchetypePreset;
    use crate::seed::Seed;
    use crate::world::{CoastMask, Resolution, WorldState};

    fn test_preset() -> IslandArchetypePreset {
        IslandArchetypePreset {
            name: "validation_test".into(),
            island_radius: 0.5,
            max_relief: 0.5,
            volcanic_center_count: 1,
            island_age: IslandAge::Young,
            prevailing_wind_dir: 0.0,
            marine_moisture_strength: 0.5,
            sea_level: 0.3,
            erosion: Default::default(),
            climate: Default::default(),
        }
    }

    /// Build a minimal CoastMask from raw Vec<u8> data.
    fn make_coast_mask(
        w: u32,
        h: u32,
        is_land: Vec<u8>,
        is_sea: Vec<u8>,
        is_coast: Vec<u8>,
    ) -> CoastMask {
        let land_cell_count = is_land.iter().map(|&v| v as u32).sum();
        let mut land = MaskField2D::new(w, h);
        land.data = is_land;
        let mut sea = MaskField2D::new(w, h);
        sea.data = is_sea;
        let mut coast = MaskField2D::new(w, h);
        coast.data = is_coast;
        CoastMask {
            is_land: land,
            is_sea: sea,
            is_coast: coast,
            land_cell_count,
            river_mouth_mask: None,
        }
    }

    // ── 1: river_termination happy path ──────────────────────────────────────
    //
    // 3x3 grid:
    //   (0,0)=land  (1,0)=land  (2,0)=sea
    //   (0,1)=land  (1,1)=land  (2,1)=coast
    //   (0,2)=land  (1,2)=land  (2,2)=sea
    //
    // flow_dir: (0,0)->E(0) (1,0)->SE(7 but clamp to S=6)
    //   Actually: (0,0) E→(1,0), (1,0) E→(2,0)[sea, valid terminus], etc.
    // Let's keep it simple: river cell (0,0) flows E→(1,0) flows E→coast(2,1)?
    // No — let's just do a linear 3-cell chain: (0,1) -> (1,1) -> (2,1)=coast.
    // river_mask: only (0,1) is river.
    // flow_dir: (0,1)->E(0) (1,1)->E(0) (2,1)->SINK.
    // coast: (2,1)=coast, (2,0)=sea, (2,2)=sea.
    #[test]
    fn river_termination_happy_path() {
        let w = 3_u32;
        let h = 3_u32;
        let n = (w * h) as usize;
        let idx = |x: u32, y: u32| (y * w + x) as usize;

        let mut is_land = vec![1u8; n];
        let mut is_sea = vec![0u8; n];
        let mut is_coast = vec![0u8; n];

        is_land[idx(2, 0)] = 0;
        is_sea[idx(2, 0)] = 1;
        is_land[idx(2, 2)] = 0;
        is_sea[idx(2, 2)] = 1;
        is_land[idx(2, 1)] = 1;
        is_coast[idx(2, 1)] = 1;

        let mut flow_dir = ScalarField2D::<u8>::new(w, h);
        for y in 0..h {
            for x in 0..w {
                flow_dir.set(x, y, FLOW_DIR_SINK);
            }
        }
        flow_dir.set(0, 1, 0); // E
        flow_dir.set(1, 1, 0); // E → (2,1)=coast

        let mut river_mask = MaskField2D::new(w, h);
        river_mask.set(0, 1, 1);

        let mut world = WorldState::new(Seed(0), test_preset(), Resolution::new(w, h));
        world.derived.coast_mask = Some(make_coast_mask(w, h, is_land, is_sea, is_coast));
        world.derived.flow_dir = Some(flow_dir);
        world.derived.river_mask = Some(river_mask);

        assert!(river_termination(&world).is_ok());
    }

    // ── 2: river_termination detects disconnected river ───────────────────────
    //
    // All-land 3x3; no coast, no sea. River cell at (1,1) with FLOW_DIR_SINK.
    // Must return RiverDoesNotTerminate.
    #[test]
    fn river_termination_detects_disconnected_river() {
        let w = 3_u32;
        let h = 3_u32;
        let n = (w * h) as usize;

        let is_land = vec![1u8; n];
        let is_sea = vec![0u8; n];
        let is_coast = vec![0u8; n];

        let mut flow_dir = ScalarField2D::<u8>::new(w, h);
        for y in 0..h {
            for x in 0..w {
                flow_dir.set(x, y, FLOW_DIR_SINK);
            }
        }

        let mut river_mask = MaskField2D::new(w, h);
        river_mask.set(1, 1, 1);

        let mut world = WorldState::new(Seed(0), test_preset(), Resolution::new(w, h));
        world.derived.coast_mask = Some(make_coast_mask(w, h, is_land, is_sea, is_coast));
        world.derived.flow_dir = Some(flow_dir);
        world.derived.river_mask = Some(river_mask);

        let err = river_termination(&world).unwrap_err();
        assert!(
            matches!(err, ValidationError::RiverDoesNotTerminate { x: 1, y: 1 }),
            "expected RiverDoesNotTerminate at (1,1), got: {err}"
        );
    }

    // ── 3: basin_partition_dag passes on acyclic flow ──────────────────────────
    //
    // Linear chain: (0,0)->E->(1,0)->E->(2,0)->SINK.
    #[test]
    fn basin_partition_dag_passes_on_acyclic_flow() {
        let w = 3_u32;
        let h = 1_u32;
        let mut flow_dir = ScalarField2D::<u8>::new(w, h);
        flow_dir.set(0, 0, 0); // E
        flow_dir.set(1, 0, 0); // E
        flow_dir.set(2, 0, FLOW_DIR_SINK);

        let mut world = WorldState::new(Seed(0), test_preset(), Resolution::new(w, h));
        world.derived.flow_dir = Some(flow_dir);

        assert!(basin_partition_dag(&world).is_ok());
    }

    // ── 4: basin_partition_dag detects cycle ───────────────────────────────────
    //
    // 2-cell cycle: (0,0)->E->(1,0)->W->(0,0). Both have indeg 1 → cycle.
    #[test]
    fn basin_partition_dag_detects_cycle() {
        let w = 2_u32;
        let h = 1_u32;
        let mut flow_dir = ScalarField2D::<u8>::new(w, h);
        flow_dir.set(0, 0, 0); // E → (1,0)
        flow_dir.set(1, 0, 4); // W → (0,0)

        let mut world = WorldState::new(Seed(0), test_preset(), Resolution::new(w, h));
        world.derived.flow_dir = Some(flow_dir);

        let err = basin_partition_dag(&world).unwrap_err();
        assert!(
            matches!(err, ValidationError::FlowDirCycle { .. }),
            "expected FlowDirCycle, got: {err}"
        );
    }

    // ── 5: accumulation_monotone happy path ───────────────────────────────────
    //
    // (0,0) A=1 -> E -> (1,0) A=2 -> E -> (2,0) A=3 -> SINK.
    #[test]
    fn accumulation_monotone_happy_path() {
        let w = 3_u32;
        let h = 1_u32;
        let mut flow_dir = ScalarField2D::<u8>::new(w, h);
        flow_dir.set(0, 0, 0);
        flow_dir.set(1, 0, 0);
        flow_dir.set(2, 0, FLOW_DIR_SINK);

        let mut accum = ScalarField2D::<f32>::new(w, h);
        accum.set(0, 0, 1.0);
        accum.set(1, 0, 2.0);
        accum.set(2, 0, 3.0);

        let mut world = WorldState::new(Seed(0), test_preset(), Resolution::new(w, h));
        world.derived.flow_dir = Some(flow_dir);
        world.derived.accumulation = Some(accum);

        assert!(accumulation_monotone(&world).is_ok());
    }

    // ── 6: accumulation_monotone detects violation ────────────────────────────
    //
    // (0,0) A=5 -> E -> (1,0) A=1 — downstream is less.
    #[test]
    fn accumulation_monotone_detects_violation() {
        let w = 2_u32;
        let h = 1_u32;
        let mut flow_dir = ScalarField2D::<u8>::new(w, h);
        flow_dir.set(0, 0, 0); // E
        flow_dir.set(1, 0, FLOW_DIR_SINK);

        let mut accum = ScalarField2D::<f32>::new(w, h);
        accum.set(0, 0, 5.0);
        accum.set(1, 0, 1.0);

        let mut world = WorldState::new(Seed(0), test_preset(), Resolution::new(w, h));
        world.derived.flow_dir = Some(flow_dir);
        world.derived.accumulation = Some(accum);

        let err = accumulation_monotone(&world).unwrap_err();
        assert!(
            matches!(err, ValidationError::AccumulationNotMonotone { .. }),
            "expected AccumulationNotMonotone, got: {err}"
        );
    }

    // ── 7: coastline_consistency happy path ───────────────────────────────────
    //
    // 3x1: (0,0)=sea z=0.1, (1,0)=coast z=0.4, (2,0)=land z=0.8.
    // sea_level=0.3: z=0.1 < 0.3 → sea ✓; z=0.4 >= 0.3 → land/coast ✓.
    // (1,0) is coast with Von4 W=(0,0)=sea → ok.
    #[test]
    fn coastline_consistency_happy_path() {
        let w = 3_u32;
        let h = 1_u32;
        let n = (w * h) as usize;

        let mut is_land = vec![0u8; n];
        let mut is_sea = vec![0u8; n];
        let mut is_coast = vec![0u8; n];
        is_sea[0] = 1;
        is_land[1] = 1;
        is_coast[1] = 1;
        is_land[2] = 1;

        let mut height = ScalarField2D::<f32>::new(w, h);
        height.set(0, 0, 0.1);
        height.set(1, 0, 0.4);
        height.set(2, 0, 0.8);

        let mut world = WorldState::new(Seed(0), test_preset(), Resolution::new(w, h));
        world.authoritative.height = Some(height);
        world.derived.coast_mask = Some(make_coast_mask(w, h, is_land, is_sea, is_coast));

        assert!(coastline_consistency(&world).is_ok());
    }

    // ── 8: coastline_consistency detects below-sea-level not marked sea ────────
    //
    // 1x1: z=0.1 < sea_level=0.3 but is_sea=0. Must fail.
    #[test]
    fn coastline_consistency_detects_below_sea_level_as_land() {
        let w = 1_u32;
        let h = 1_u32;

        let is_land = vec![1u8]; // wrongly marked land
        let is_sea = vec![0u8];
        let is_coast = vec![0u8];

        let mut height = ScalarField2D::<f32>::new(w, h);
        height.set(0, 0, 0.1); // below sea_level=0.3

        let mut world = WorldState::new(Seed(0), test_preset(), Resolution::new(w, h));
        world.authoritative.height = Some(height);
        world.derived.coast_mask = Some(make_coast_mask(w, h, is_land, is_sea, is_coast));

        let err = coastline_consistency(&world).unwrap_err();
        assert!(
            matches!(err, ValidationError::CoastlineBelowSeaLevelNotSea { .. }),
            "expected CoastlineBelowSeaLevelNotSea, got: {err}"
        );
    }

    // ── 9: coastline_consistency detects coast without sea neighbour ──────────
    //
    // 3x1: all land, middle marked coast. No sea anywhere → coast has no sea nbr.
    #[test]
    fn coastline_consistency_detects_coast_without_sea_neighbour() {
        let w = 3_u32;
        let h = 1_u32;
        let n = (w * h) as usize;

        let is_land = vec![1u8; n];
        let is_sea = vec![0u8; n];
        let mut is_coast = vec![0u8; n];
        is_coast[1] = 1; // (1,0) marked coast but no sea neighbours

        // Heights all above sea_level so sub-check-1 passes.
        let mut height = ScalarField2D::<f32>::new(w, h);
        height.set(0, 0, 0.5);
        height.set(1, 0, 0.5);
        height.set(2, 0, 0.5);

        let mut world = WorldState::new(Seed(0), test_preset(), Resolution::new(w, h));
        world.authoritative.height = Some(height);
        world.derived.coast_mask = Some(make_coast_mask(w, h, is_land, is_sea, is_coast));

        let err = coastline_consistency(&world).unwrap_err();
        assert!(
            matches!(
                err,
                ValidationError::CoastlineCoastWithoutSeaNeighbour { x: 1, y: 0 }
            ),
            "expected CoastlineCoastWithoutSeaNeighbour at (1,0), got: {err}"
        );
    }

    // ── 10: missing precondition returns Err ───────────────────────────────────
    //
    // Fresh empty world has no derived fields. All four validators must fail
    // with MissingPrecondition.
    #[test]
    fn missing_precondition_returns_err() {
        let world = WorldState::new(Seed(0), test_preset(), Resolution::new(4, 4));

        assert!(matches!(
            river_termination(&world),
            Err(ValidationError::MissingPrecondition { .. })
        ));
        assert!(matches!(
            basin_partition_dag(&world),
            Err(ValidationError::MissingPrecondition { .. })
        ));
        assert!(matches!(
            accumulation_monotone(&world),
            Err(ValidationError::MissingPrecondition { .. })
        ));
        assert!(matches!(
            coastline_consistency(&world),
            Err(ValidationError::MissingPrecondition { .. })
        ));
    }

    // ── bonus: river cell marked sea returns RiverInSea ───────────────────────
    #[test]
    fn river_termination_detects_river_in_sea() {
        let w = 2_u32;
        let h = 1_u32;
        let is_land = vec![0u8, 1u8];
        let is_sea = vec![1u8, 0u8];
        let is_coast = vec![0u8, 0u8];

        let flow_dir = ScalarField2D::<u8>::new(w, h); // all FLOW_DIR_SINK=0, but we never read

        let mut river_mask = MaskField2D::new(w, h);
        river_mask.set(0, 0, 1); // river cell in sea

        let mut world = WorldState::new(Seed(0), test_preset(), Resolution::new(w, h));
        world.derived.coast_mask = Some(make_coast_mask(w, h, is_land, is_sea, is_coast));
        world.derived.flow_dir = Some(flow_dir);
        world.derived.river_mask = Some(river_mask);

        let err = river_termination(&world).unwrap_err();
        assert!(
            matches!(err, ValidationError::RiverInSea { x: 0, y: 0 }),
            "expected RiverInSea at (0,0), got: {err}"
        );
    }

    // ── Sprint 1B invariant tests ────────────────────────────────────────────

    use crate::world::{BakedSnapshot, BiomeWeights, HexAttributeField, HexAttributes};

    fn minimal_world_for_1b(w: u32, h: u32) -> WorldState {
        let mut world = WorldState::new(Seed(0), test_preset(), Resolution::new(w, h));
        world.baked = BakedSnapshot::default();
        world.derived.coast_mask = Some(make_coast_mask(
            w,
            h,
            vec![1u8; (w * h) as usize],
            vec![0u8; (w * h) as usize],
            vec![0u8; (w * h) as usize],
        ));
        world
    }

    #[test]
    fn precipitation_nonneg_happy_path() {
        let mut world = minimal_world_for_1b(4, 4);
        let mut p = ScalarField2D::<f32>::new(4, 4);
        p.data.fill(0.3);
        world.baked.precipitation = Some(p);
        assert!(precipitation_nonneg(&world).is_ok());
    }

    #[test]
    fn precipitation_nonneg_detects_negative() {
        let mut world = minimal_world_for_1b(4, 4);
        let mut p = ScalarField2D::<f32>::new(4, 4);
        p.set(2, 1, -0.1);
        world.baked.precipitation = Some(p);
        let err = precipitation_nonneg(&world).unwrap_err();
        assert!(matches!(
            err,
            ValidationError::PrecipitationNegative { x: 2, y: 1, .. }
        ));
    }

    #[test]
    fn biome_weights_normalized_happy_path() {
        let mut world = minimal_world_for_1b(2, 2);
        let mut bw = BiomeWeights::new(2, 2);
        let idx = crate::world::BiomeType::LowlandForest as usize;
        for row in bw.weights.iter_mut() {
            row.fill(0.0);
        }
        for cell in 0..4 {
            bw.weights[idx][cell] = 1.0;
        }
        world.baked.biome_weights = Some(bw);
        assert!(biome_weights_normalized(&world).is_ok());
    }

    #[test]
    fn biome_weights_normalized_detects_drift() {
        let mut world = minimal_world_for_1b(2, 2);
        let mut bw = BiomeWeights::new(2, 2);
        // Leave everything at zero → sum = 0, fails tolerance.
        world.baked.biome_weights = Some(bw.clone());
        let err = biome_weights_normalized(&world).unwrap_err();
        assert!(matches!(
            err,
            ValidationError::BiomeWeightsNotNormalized { .. }
        ));

        // Fix cell (0, 0) to sum to 1 but leave (1, 0) drifting by 0.01.
        let idx = crate::world::BiomeType::LowlandForest as usize;
        bw.weights[idx][0] = 1.0;
        bw.weights[idx][1] = 0.5; // still wrong
        world.baked.biome_weights = Some(bw);
        let err = biome_weights_normalized(&world).unwrap_err();
        assert!(matches!(
            err,
            ValidationError::BiomeWeightsNotNormalized { x: 1, y: 0, .. }
        ));
    }

    #[test]
    fn temperature_physical_range_happy_path() {
        let mut world = minimal_world_for_1b(4, 4);
        let mut t = ScalarField2D::<f32>::new(4, 4);
        t.data.fill(20.0);
        world.baked.temperature = Some(t);
        assert!(temperature_physical_range(&world).is_ok());
    }

    #[test]
    fn temperature_physical_range_detects_too_hot() {
        let mut world = minimal_world_for_1b(4, 4);
        let mut t = ScalarField2D::<f32>::new(4, 4);
        t.data.fill(20.0);
        t.set(1, 2, 50.0); // impossibly hot
        world.baked.temperature = Some(t);
        let err = temperature_physical_range(&world).unwrap_err();
        assert!(matches!(
            err,
            ValidationError::TemperatureOutOfRange { x: 1, y: 2, .. }
        ));
    }

    #[test]
    fn hex_attrs_present_happy_path() {
        let mut world = minimal_world_for_1b(4, 4);
        let n_hex = 16;
        let attrs: Vec<HexAttributes> = (0..n_hex)
            .map(|_| HexAttributes {
                elevation: 0.0,
                slope: 0.0,
                rainfall: 0.0,
                temperature: 0.0,
                moisture: 0.0,
                biome_weights: vec![0.0; crate::world::BiomeType::COUNT],
                dominant_biome: crate::world::BiomeType::CoastalScrub,
                has_river: false,
            })
            .collect();
        world.derived.hex_attrs = Some(HexAttributeField {
            attrs,
            cols: 4,
            rows: 4,
        });
        assert!(hex_attrs_present(&world).is_ok());
    }

    #[test]
    fn hex_attrs_present_detects_biome_row_length_mismatch() {
        let mut world = minimal_world_for_1b(4, 4);
        let attrs = (0..16)
            .map(|i| HexAttributes {
                elevation: 0.0,
                slope: 0.0,
                rainfall: 0.0,
                temperature: 0.0,
                moisture: 0.0,
                biome_weights: if i == 5 {
                    vec![0.0; 3] // wrong length on one hex
                } else {
                    vec![0.0; crate::world::BiomeType::COUNT]
                },
                dominant_biome: crate::world::BiomeType::CoastalScrub,
                has_river: false,
            })
            .collect();
        world.derived.hex_attrs = Some(HexAttributeField {
            attrs,
            cols: 4,
            rows: 4,
        });
        let err = hex_attrs_present(&world).unwrap_err();
        assert!(matches!(
            err,
            ValidationError::HexBiomeWeightsLengthMismatch { col: 1, row: 1, .. }
        ));
    }

    // ── Sprint 2 invariant tests ─────────────────────────────────────────────

    use crate::world::ErosionBaseline;

    // Helper: build a WorldState with coast_mask + coast_type for well-formed checks.
    fn make_coast_type_world(
        w: u32,
        h: u32,
        is_coast_data: Vec<u8>,
        coast_type_data: Vec<u8>,
    ) -> WorldState {
        let n = (w * h) as usize;
        let is_land: Vec<u8> = is_coast_data.to_vec();
        let is_sea = vec![0u8; n];
        let mut world = WorldState::new(Seed(0), test_preset(), Resolution::new(w, h));
        world.derived.coast_mask = Some(make_coast_mask(w, h, is_land, is_sea, is_coast_data));
        let mut ct = ScalarField2D::<u8>::new(w, h);
        ct.data = coast_type_data;
        world.derived.coast_type = Some(ct);
        world
    }

    // ── 11: coast_type_well_formed — happy path ───────────────────────────────
    //
    // 5 coast cells with types 0/1/2/3/4 respectively. All valid after the
    // Sprint 3 DD6 widening from `0..=3` to `0..=4` (LavaDelta = 4).
    #[test]
    fn coast_type_well_formed_passes_when_coast_cells_have_valid_types() {
        let world = make_coast_type_world(5, 1, vec![1, 1, 1, 1, 1], vec![0, 1, 2, 3, 4]);
        assert!(
            coast_type_well_formed(&world).is_ok(),
            "expected Ok for coast types 0..=4 (Sprint 3 DD6 range)"
        );
    }

    // ── 11b: coast_type_well_formed accepts LavaDelta (Sprint 3 DD6) ─────────
    //
    // Regression guard for the 0..=3 → 0..=4 widening: a coast cell carrying
    // discriminant 4 (LavaDelta) must validate.
    #[test]
    fn coast_type_well_formed_accepts_lava_delta() {
        let world = make_coast_type_world(2, 1, vec![1, 1], vec![0, 4]);
        assert!(
            coast_type_well_formed(&world).is_ok(),
            "LavaDelta (disc=4) must be accepted by the Sprint 3 DD6-widened invariant"
        );
    }

    // ── 11c: coast_type_well_formed still rejects disc=5 ──────────────────────
    //
    // The widening is exactly one slot; disc=5 has no CoastType variant and
    // must still be flagged as out-of-range.
    #[test]
    fn coast_type_well_formed_rejects_disc_five() {
        let world = make_coast_type_world(2, 1, vec![1, 1], vec![0, 5]);
        let err = coast_type_well_formed(&world).unwrap_err();
        assert!(
            matches!(
                err,
                ValidationError::CoastTypeOutOfRange {
                    cell_index: 1,
                    value: 5
                }
            ),
            "disc=5 must still be rejected (no CoastType variant), got: {err}"
        );
    }

    // ── 12: coast_type_well_formed — failure: coast cell with 0xFF ────────────
    //
    // Coast cell at index 2 has 0xFF (Unknown sentinel), which is invalid for
    // a coast cell.
    #[test]
    fn coast_type_well_formed_fails_on_coast_cell_with_0xff() {
        let world = make_coast_type_world(4, 1, vec![1, 1, 1, 0], vec![0, 1, 0xFF, 0xFF]);
        let err = coast_type_well_formed(&world).unwrap_err();
        assert!(
            matches!(
                err,
                ValidationError::CoastTypeOutOfRange {
                    cell_index: 2,
                    value: 0xFF
                }
            ),
            "expected CoastTypeOutOfRange at index 2, got: {err}"
        );
    }

    // ── 12b: coast_type_well_formed — failure: non-coast cell with valid variant ─
    //
    // Non-coast cell at index 2 has 0x01 (Beach) instead of the Unknown sentinel.
    // Guards against a classifier that forgets to initialise non-coast cells.
    #[test]
    fn coast_type_well_formed_fails_on_non_coast_cell_with_valid_variant() {
        let world = make_coast_type_world(4, 1, vec![1, 0, 0, 0], vec![0, 0xFF, 0x01, 0xFF]);
        let err = coast_type_well_formed(&world).unwrap_err();
        assert!(
            matches!(
                err,
                ValidationError::NonCoastCellNotUnknown {
                    cell_index: 2,
                    value: 0x01
                }
            ),
            "expected NonCoastCellNotUnknown at index 2, got: {err}"
        );
    }

    // Helper: build a WorldState with height + erosion_baseline.
    fn make_erosion_world(
        w: u32,
        h: u32,
        height_data: Vec<f32>,
        baseline: ErosionBaseline,
    ) -> WorldState {
        let mut world = WorldState::new(Seed(0), test_preset(), Resolution::new(w, h));
        let mut height = ScalarField2D::<f32>::new(w, h);
        height.data = height_data;
        world.authoritative.height = Some(height);
        world.derived.erosion_baseline = Some(baseline);
        world
    }

    // ── 13: erosion_no_explosion — passes when max is within 1.05x ───────────
    #[test]
    fn erosion_no_explosion_passes_at_baseline() {
        // baseline.max_height_pre = 1.0, current max = 0.95: within 1.05x ceiling.
        let world = make_erosion_world(
            2,
            1,
            vec![0.95, 0.8],
            ErosionBaseline {
                max_height_pre: 1.0,
                land_cell_count_pre: 2,
            },
        );
        assert!(
            erosion_no_explosion(&world).is_ok(),
            "expected Ok when max is below 1.05x baseline"
        );
    }

    // ── 14: erosion_no_explosion — fails when max exceeds 1.05x ──────────────
    #[test]
    fn erosion_no_explosion_fails_beyond_factor() {
        // baseline.max_height_pre = 1.0, current max = 1.10: exceeds 1.05x ceiling.
        let world = make_erosion_world(
            2,
            1,
            vec![1.10, 0.5],
            ErosionBaseline {
                max_height_pre: 1.0,
                land_cell_count_pre: 2,
            },
        );
        let err = erosion_no_explosion(&world).unwrap_err();
        assert!(
            matches!(err, ValidationError::ErosionExplosion { .. }),
            "expected ErosionExplosion, got: {err}"
        );
    }

    // Helper: build a WorldState with coast_mask + erosion_baseline for
    // sea-crossing checks (height not needed).
    fn make_sea_crossing_world(pre_land: u32, post_land: u32) -> WorldState {
        let total = pre_land.max(post_land).max(1);
        let w = total;
        let h = 1;
        let n = total as usize;

        let is_land: Vec<u8> = (0..n)
            .map(|i| if i < post_land as usize { 1 } else { 0 })
            .collect();
        let is_sea: Vec<u8> = is_land.iter().map(|&v| 1 - v).collect();
        let is_coast = vec![0u8; n];

        let mut world = WorldState::new(Seed(0), test_preset(), Resolution::new(w, h));
        world.derived.coast_mask = Some(make_coast_mask(w, h, is_land, is_sea, is_coast));
        world.derived.erosion_baseline = Some(ErosionBaseline {
            max_height_pre: 1.0,
            land_cell_count_pre: pre_land,
        });
        world
    }

    // ── 15: erosion_no_excessive_sea_crossing — passes at 3 % ────────────────
    //
    // pre = 1000, post = 970 → 3.0 % delta, below the 5 % limit.
    #[test]
    fn erosion_no_excessive_sea_crossing_passes_at_3_percent() {
        let world = make_sea_crossing_world(1000, 970);
        assert!(
            erosion_no_excessive_sea_crossing(&world).is_ok(),
            "expected Ok for 3% sea crossing"
        );
    }

    // ── 16: erosion_no_excessive_sea_crossing — fails at 7 % ─────────────────
    //
    // pre = 1000, post = 930 → 7.0 % delta, above the 5 % limit.
    #[test]
    fn erosion_no_excessive_sea_crossing_fails_at_7_percent() {
        let world = make_sea_crossing_world(1000, 930);
        let err = erosion_no_excessive_sea_crossing(&world).unwrap_err();
        assert!(
            matches!(
                err,
                ValidationError::ErosionExcessiveSeaCrossing {
                    pre_land: 1000,
                    post_land: 930,
                    ..
                }
            ),
            "expected ErosionExcessiveSeaCrossing, got: {err}"
        );
    }

    // ── bonus: skip when erosion_baseline is None ─────────────────────────────
    #[test]
    fn erosion_no_explosion_skips_when_baseline_missing() {
        // height is present but erosion_baseline is None — should skip (Ok).
        let mut world = WorldState::new(Seed(0), test_preset(), Resolution::new(2, 1));
        let mut h = ScalarField2D::<f32>::new(2, 1);
        h.data = vec![5.0, 5.0]; // would be "explosive" if baseline were 1.0
        world.authoritative.height = Some(h);
        assert!(
            erosion_no_explosion(&world).is_ok(),
            "expected Ok when baseline is missing (ErosionOuterLoop not yet run)"
        );
    }

    // ── bonus: NaN in height triggers ErosionHeightNonFinite ─────────────────
    #[test]
    fn erosion_no_explosion_detects_nan_height() {
        let world = make_erosion_world(
            2,
            1,
            vec![f32::NAN, 0.5],
            ErosionBaseline {
                max_height_pre: 1.0,
                land_cell_count_pre: 2,
            },
        );
        let err = erosion_no_explosion(&world).unwrap_err();
        assert!(
            matches!(
                err,
                ValidationError::ErosionHeightNonFinite { cell_index: 0, .. }
            ),
            "expected ErosionHeightNonFinite at cell 0, got: {err}"
        );
    }

    // ── Sprint 3 invariant tests ─────────────────────────────────────────────

    use super::{
        coast_type_v2_well_formed, deposition_zone_fraction_realistic, precipitation_mass_balance,
        sediment_bounded,
    };
    use crate::preset::PrecipitationVariant;

    /// Build a minimal world with `coast_mask` (all-land), `sediment`, and
    /// `baked.precipitation` for Sprint 3 invariant unit tests.
    fn make_sprint3_world(
        w: u32,
        h: u32,
        is_land: Vec<u8>,
        is_sea: Vec<u8>,
        sediment_data: Vec<f32>,
        precip_data: Vec<f32>,
    ) -> WorldState {
        let n = (w * h) as usize;
        let is_coast = vec![0u8; n];
        let mut world = WorldState::new(Seed(0), test_preset(), Resolution::new(w, h));
        world.derived.coast_mask = Some(make_coast_mask(w, h, is_land, is_sea, is_coast));
        let mut sed = ScalarField2D::<f32>::new(w, h);
        sed.data = sediment_data;
        world.authoritative.sediment = Some(sed);
        let mut precip = ScalarField2D::<f32>::new(w, h);
        precip.data = precip_data;
        world.baked.precipitation = Some(precip);
        world
    }

    // ── sediment_bounded: test 1 — all-land world with valid hs ──────────────
    #[test]
    fn sediment_bounded_accepts_valid_land_state() {
        let n = 4usize;
        let world = make_sprint3_world(
            2,
            2,
            vec![1u8; n],
            vec![0u8; n],
            vec![0.0, 0.1, 0.5, 1.0],
            vec![0.3; n],
        );
        assert!(
            sediment_bounded(&world).is_ok(),
            "valid sediment values in [0,1] on land cells must pass"
        );
    }

    // ── sediment_bounded: test 2 — NaN triggers error ────────────────────────
    #[test]
    fn sediment_bounded_rejects_nan() {
        let n = 4usize;
        let world = make_sprint3_world(
            2,
            2,
            vec![1u8; n],
            vec![0u8; n],
            vec![f32::NAN, 0.1, 0.5, 0.9],
            vec![0.3; n],
        );
        let err = sediment_bounded(&world).unwrap_err();
        assert!(
            matches!(
                err,
                ValidationError::SedimentOutOfRange { cell_index: 0, .. }
            ),
            "NaN hs must fire SedimentOutOfRange at cell 0, got: {err}"
        );
    }

    // ── sediment_bounded: test 3 — hs > 1.0 triggers error ──────────────────
    #[test]
    fn sediment_bounded_rejects_above_upper() {
        let n = 4usize;
        let world = make_sprint3_world(
            2,
            2,
            vec![1u8; n],
            vec![0u8; n],
            vec![1.5, 0.1, 0.5, 0.9],
            vec![0.3; n],
        );
        let err = sediment_bounded(&world).unwrap_err();
        assert!(
            matches!(
                err,
                ValidationError::SedimentOutOfRange { cell_index: 0, .. }
            ),
            "hs=1.5 must fire SedimentOutOfRange at cell 0, got: {err}"
        );
    }

    // ── sediment_bounded: test 4 — sea cell with non-zero hs ─────────────────
    //
    // 2×1: cell 0 is sea, cell 1 is land. Sea cell has hs=0.1 → must fire.
    #[test]
    fn sediment_bounded_rejects_sea_cell_nonzero() {
        let world = make_sprint3_world(
            2,
            1,
            vec![0u8, 1u8], // is_land
            vec![1u8, 0u8], // is_sea
            vec![0.1, 0.3], // sediment — sea cell has 0.1 (wrong)
            vec![0.3, 0.3],
        );
        let err = sediment_bounded(&world).unwrap_err();
        assert!(
            matches!(
                err,
                ValidationError::SedimentSeaCellNonZero { cell_index: 0, .. }
            ),
            "sea cell with hs=0.1 must fire SedimentSeaCellNonZero at cell 0, got: {err}"
        );
    }

    // ── sediment_bounded: test 5 — missing sediment field returns error ───────
    #[test]
    fn sediment_bounded_missing_field_returns_error() {
        let n = 4usize;
        let mut world = WorldState::new(Seed(0), test_preset(), Resolution::new(2, 2));
        world.derived.coast_mask = Some(make_coast_mask(
            2,
            2,
            vec![1u8; n],
            vec![0u8; n],
            vec![0u8; n],
        ));
        // Do NOT set authoritative.sediment.
        let err = sediment_bounded(&world).unwrap_err();
        assert!(
            matches!(err, ValidationError::SedimentFieldMissing),
            "missing sediment must fire SedimentFieldMissing, got: {err}"
        );
    }

    // ── deposition_zone_fraction: test 1 — nominal fraction in [LO, HI] ──────
    //
    // 10 land cells: 3 with hs > 0.15 → fraction = 0.30 ∈ [0.05, 0.70].
    #[test]
    fn deposition_zone_fraction_realistic_accepts_nominal() {
        let n = 10usize;
        let mut sed = vec![0.05f32; n]; // all below threshold initially
        sed[0] = 0.20; // above DEPOSITION_FLAG_THRESHOLD = 0.15
        sed[4] = 0.50;
        sed[7] = 0.80;
        let world = make_sprint3_world(10, 1, vec![1u8; n], vec![0u8; n], sed, vec![0.3; n]);
        assert!(
            deposition_zone_fraction_realistic(&world).is_ok(),
            "30% deposition fraction must be accepted (in [0.0, 0.70])"
        );
    }

    // ── deposition_zone_fraction: test 2 — zero deposition fraction is accepted
    //
    // At v1 SPACE-lite calibration, small grids often produce 0% deposition
    // (transport capacity exceeds incoming Qs). LO = 0.0 so this must pass.
    // The lower bound will be tightened in Task 3.10 once the 256² deposition
    // physics is calibrated.
    #[test]
    fn deposition_zone_fraction_realistic_accepts_zero_at_v1_lo_bound() {
        let n = 10usize;
        let world = make_sprint3_world(
            10,
            1,
            vec![1u8; n],
            vec![0u8; n],
            vec![0.05f32; n], // all below threshold → fraction = 0.0
            vec![0.3; n],
        );
        // With LO = 0.0, fraction 0.0 is valid (no lower bound enforced in v1).
        assert!(
            deposition_zone_fraction_realistic(&world).is_ok(),
            "fraction 0.0 must be accepted when DEPOSITION_ZONE_FRACTION_LO = 0.0 (v1 calibration)"
        );
    }

    // ── deposition_zone_fraction: test 3 — saturated (fraction = 1.0) ────────
    //
    // All land cells have hs = 1.0 (above threshold) → fraction = 1.0 > HI.
    #[test]
    fn deposition_zone_fraction_realistic_rejects_saturated() {
        let n = 10usize;
        let world = make_sprint3_world(
            10,
            1,
            vec![1u8; n],
            vec![0u8; n],
            vec![1.0f32; n], // all above threshold
            vec![0.3; n],
        );
        let err = deposition_zone_fraction_realistic(&world).unwrap_err();
        assert!(
            matches!(
                err,
                ValidationError::DepositionZoneFractionOutOfRange { fraction, .. }
                if fraction == 1.0
            ),
            "100% fraction must fire DepositionZoneFractionOutOfRange, got: {err}"
        );
    }

    // ── deposition_zone_fraction: test 4 — all-sea world returns Ok ──────────
    #[test]
    fn deposition_zone_fraction_realistic_accepts_empty_land() {
        let n = 4usize;
        // All-sea world: land_cell_count == 0.
        let world = make_sprint3_world(
            2,
            2,
            vec![0u8; n], // no land
            vec![1u8; n], // all sea
            vec![0.0f32; n],
            vec![0.0f32; n],
        );
        assert!(
            deposition_zone_fraction_realistic(&world).is_ok(),
            "all-sea world (land_count=0) must return Ok immediately"
        );
    }

    // ── coast_type_v2_well_formed: helpers ────────────────────────────────────

    fn make_v2_coast_world(
        w: u32,
        h: u32,
        is_coast_data: Vec<u8>,
        coast_type_data: Vec<u8>,
        island_age: IslandAge,
    ) -> WorldState {
        let n = (w * h) as usize;
        let is_land = is_coast_data.clone();
        let is_sea = vec![0u8; n];
        let mut preset = test_preset();
        preset.island_age = island_age;
        let mut world = WorldState::new(Seed(0), preset, Resolution::new(w, h));
        world.derived.coast_mask = Some(make_coast_mask(w, h, is_land, is_sea, is_coast_data));
        let mut ct = ScalarField2D::<u8>::new(w, h);
        ct.data = coast_type_data;
        world.derived.coast_type = Some(ct);
        world
    }

    // ── coast_type_v2_well_formed: test 1 — Young with LavaDelta ────────────
    #[test]
    fn coast_type_v2_well_formed_accepts_young_with_lava_delta() {
        // 3 coast cells: types Beach(1), RockyHeadland(3), LavaDelta(4).
        // Young preset → LavaDelta is allowed.
        let world = make_v2_coast_world(3, 1, vec![1, 1, 1], vec![1, 3, 4], IslandAge::Young);
        assert!(
            coast_type_v2_well_formed(&world).is_ok(),
            "Young preset with LavaDelta must pass v2 invariant"
        );
    }

    // ── coast_type_v2_well_formed: test 2 — Mature with LavaDelta rejects ────
    #[test]
    fn coast_type_v2_well_formed_rejects_mature_with_lava_delta() {
        let world = make_v2_coast_world(3, 1, vec![1, 1, 1], vec![1, 3, 4], IslandAge::Mature);
        let err = coast_type_v2_well_formed(&world).unwrap_err();
        assert!(
            matches!(
                err,
                ValidationError::LavaDeltaOnNonYoungPreset {
                    count: 1,
                    island_age: IslandAge::Mature,
                }
            ),
            "Mature preset with LavaDelta must fire LavaDeltaOnNonYoungPreset, got: {err}"
        );
    }

    // ── coast_type_v2_well_formed: test 3 — Mature without LavaDelta passes ──
    #[test]
    fn coast_type_v2_well_formed_accepts_mature_without_lava() {
        let world = make_v2_coast_world(3, 1, vec![1, 1, 1], vec![1, 2, 3], IslandAge::Mature);
        assert!(
            coast_type_v2_well_formed(&world).is_ok(),
            "Mature preset without LavaDelta must pass v2 invariant"
        );
    }

    // ── coast_type_v2_well_formed: test 4 — discriminant 5 on coast cell ─────
    #[test]
    fn coast_type_v2_well_formed_rejects_disc_five_on_coast() {
        let world = make_v2_coast_world(2, 1, vec![1, 1], vec![0, 5], IslandAge::Young);
        let err = coast_type_v2_well_formed(&world).unwrap_err();
        assert!(
            matches!(
                err,
                ValidationError::CoastTypeV2DiscOutOfRange {
                    cell_index: 1,
                    value: 5
                }
            ),
            "disc=5 on a coast cell must fire CoastTypeV2DiscOutOfRange, got: {err}"
        );
    }

    // ── coast_type_v2_well_formed: test 5 — Old with LavaDelta rejects ────────
    #[test]
    fn coast_type_v2_well_formed_rejects_old_with_lava_delta() {
        let world = make_v2_coast_world(2, 1, vec![1, 1], vec![1, 4], IslandAge::Old);
        let err = coast_type_v2_well_formed(&world).unwrap_err();
        assert!(
            matches!(
                err,
                ValidationError::LavaDeltaOnNonYoungPreset {
                    island_age: IslandAge::Old,
                    ..
                }
            ),
            "Old preset with LavaDelta must fire LavaDeltaOnNonYoungPreset, got: {err}"
        );
    }

    // ── precipitation_mass_balance: helpers ───────────────────────────────────

    fn make_precip_world(
        w: u32,
        h: u32,
        precip_data: Vec<f32>,
        variant: crate::preset::PrecipitationVariant,
    ) -> WorldState {
        let n = (w * h) as usize;
        let mut preset = test_preset();
        preset.climate.precipitation_variant = variant;
        let mut world = WorldState::new(Seed(0), preset, Resolution::new(w, h));
        world.derived.coast_mask = Some(make_coast_mask(
            w,
            h,
            vec![1u8; n],
            vec![0u8; n],
            vec![0u8; n],
        ));
        let mut p = ScalarField2D::<f32>::new(w, h);
        p.data = precip_data;
        world.baked.precipitation = Some(p);
        world
    }

    // ── precipitation_mass_balance: test 1 — nominal V3 ─────────────────────
    #[test]
    fn precipitation_mass_balance_accepts_nominal_v3() {
        // Mean of 0.3 is well within [1e-4, 1.0].
        let world = make_precip_world(4, 1, vec![0.2, 0.3, 0.4, 0.3], PrecipitationVariant::V3Lfpm);
        assert!(
            precipitation_mass_balance(&world).is_ok(),
            "nominal V3 precipitation must pass mass-balance check"
        );
    }

    // ── precipitation_mass_balance: test 2 — V2 world skips even if broken ───
    //
    // The invariant is guarded at the ValidationStage level; calling it
    // directly on a V2 world with zero precip should not panic, but the test
    // exercises the skip-logic by relying on ValidationStage's guard.
    // We test the function directly here with a valid world to verify it
    // at least returns Ok when the precip values are valid (V2 = skip at
    // call-site, but here we call it directly and it does run).
    //
    // The meaningful guard test is in validation_stage.rs where the `if V3Lfpm`
    // branch lives. Here we just verify the function doesn't panic or return
    // an error when called on a V2-labelled world with valid precip values.
    #[test]
    fn precipitation_mass_balance_v2_world_with_valid_precip_passes() {
        let world = make_precip_world(4, 1, vec![0.3; 4], PrecipitationVariant::V2Raymarch);
        // The function itself doesn't check the variant — it's the caller's job.
        assert!(
            precipitation_mass_balance(&world).is_ok(),
            "valid precip on a V2 world must pass when called directly"
        );
    }

    // ── precipitation_mass_balance: test 3 — zero precip fires ───────────────
    #[test]
    fn precipitation_mass_balance_rejects_zero() {
        let world = make_precip_world(4, 1, vec![0.0f32; 4], PrecipitationVariant::V3Lfpm);
        let err = precipitation_mass_balance(&world).unwrap_err();
        assert!(
            matches!(
                err,
                ValidationError::PrecipitationMassBalanceViolation { mean, .. }
                if mean == 0.0
            ),
            "zero precipitation must fire PrecipitationMassBalanceViolation, got: {err}"
        );
    }

    // ── precipitation_mass_balance: test 4 — explosion fires ─────────────────
    //
    // All cells with P = 5.0 → mean = 5.0 > PRECIP_MEAN_HI = 1.0.
    #[test]
    fn precipitation_mass_balance_rejects_explosion() {
        let world = make_precip_world(4, 1, vec![5.0f32; 4], PrecipitationVariant::V3Lfpm);
        let err = precipitation_mass_balance(&world).unwrap_err();
        assert!(
            matches!(
                err,
                ValidationError::PrecipitationMassBalanceViolation { mean, .. }
                if mean == 5.0
            ),
            "P=5 explosion must fire PrecipitationMassBalanceViolation, got: {err}"
        );
    }
}
