//! Post-pipeline correctness invariants for the Island Proc-Gen `WorldState`.
//!
//! Validators are grouped by invariant family:
//!
//! | Family | File | Validators |
//! |--------|------|-----------|
//! | Hydrological | [`hydro`] | coastline topology, D8 flow DAG, accumulation monotonicity, river termination, post-erosion basin partition |
//! | Climate | [`climate`] | precipitation non-negativity, temperature physical range, V3Lfpm mass balance |
//! | Erosion / coast-type | [`erosion`] | height explosion, sea-crossing fraction, sediment bounds, deposition zone, coast-type v1/v2 |
//! | Biome | [`biome`] | biome-weight partition-of-unity normalisation |
//! | Hex | [`hex`] | hex attribute grid shape and biome-weight vector consistency |
//!
//! All public validator names and constants are re-exported from this module so
//! that downstream `use island_core::validation::<name>` paths remain
//! byte-identical after the split.
//!
//! None of these functions panic — a missing precondition field returns
//! `Err(MissingPrecondition)` instead.

use crate::preset::IslandAge;

// ─── submodules ───────────────────────────────────────────────────────────────

pub mod biome;
pub mod climate;
pub mod erosion;
pub mod hex;
pub mod hydro;

// ─── flat re-exports — keeps `island_core::validation::<name>` paths intact ─

pub use biome::biome_weights_normalized;

pub use climate::{precipitation_mass_balance, precipitation_nonneg, temperature_physical_range};

pub use erosion::{
    coast_type_v2_well_formed, coast_type_well_formed, deposition_zone_fraction_realistic,
    erosion_no_excessive_sea_crossing, erosion_no_explosion, sediment_bounded,
};

pub use hex::{hex_attrs_present, hex_river_crossing_edges_in_range};

pub use hydro::{
    accumulation_monotone, basin_partition_dag, basin_partition_post_erosion_well_formed,
    coastline_consistency, river_termination,
};

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

    /// A `HexRiverCrossing` has an `entry_edge` or `exit_edge` value outside
    /// the DD3 6-edge range `0..=5`.
    ///
    /// Added in Sprint 3.5.B c1. The valid range was `0..=3` (4 box edges)
    /// in Sprint 2.5; it expands to `0..=5` (6 hex edges, CCW from east)
    /// after DD3 promotion.
    #[error(
        "hex_river_crossing_edges_in_range: hex_id {hex_id} has entry_edge={entry_edge} exit_edge={exit_edge} (expected both 0..=5)"
    )]
    HexRiverCrossingEdgeOutOfRange {
        hex_id: usize,
        entry_edge: u8,
        exit_edge: u8,
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

// ─── shared constants (used by family submodules via `super::`) ──────────────

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
