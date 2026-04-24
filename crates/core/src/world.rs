//! World state: the authoritative / baked / derived 3-layer split.
//!
//! Roadmap §数据层分离 mandates that `WorldState` be split into three
//! sub-structs at the type level from Sprint 0 onward so that later sprints
//! add fields to dedicated containers instead of piling `Option`s onto the
//! top-level struct. Sprint 0 leaves most of these fields empty — the
//! important thing is the layout.
//!
//! Roadmap §分辨率分层 also requires that [`Resolution`] be **simulation-only**.
//! Render LOD / supersample factors live in `crates/render`; hex grid
//! dimensions live in `crates/hex::HexGrid`. Neither ever enters `WorldState`.

use serde::{Deserialize, Serialize};

use crate::field::{MaskField2D, ScalarField2D, VectorField2D};
use crate::preset::IslandArchetypePreset;
use crate::seed::Seed;

// ─── Resolution ──────────────────────────────────────────────────────────────

/// Simulation-grid resolution.
///
/// **This type only describes the simulation grid.** Per roadmap
/// §分辨率分层, three independent resolution layers exist:
///
/// * `sim_width` / `sim_height` — the world-truth simulation grid (this type).
/// * Render LOD / supersample factor — lives in `crates/render` (Sprint 1A+),
///   **never** in `WorldState`.
/// * Hex cols/rows — lives in `crates/hex::HexGrid` (Sprint 1B+),
///   **never** in `WorldState` canonical state.
///
/// Sprint 0 writes this invariant into the type so no future stage can
/// accidentally bolt render LOD or hex dimensions onto `WorldState`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Resolution {
    pub sim_width: u32,
    pub sim_height: u32,
}

impl Resolution {
    /// Build a new [`Resolution`] from simulation-grid dimensions.
    pub fn new(sim_width: u32, sim_height: u32) -> Self {
        Self {
            sim_width,
            sim_height,
        }
    }
}

// ─── D8 flow-direction constants ─────────────────────────────────────────────

/// Sentinel written to `DerivedCaches::flow_dir` for coast cells, sea cells,
/// and genuine sinks (no downstream). Non-sentinel values `0..=7` encode D8
/// direction indices (E=0, NE=1, ..., SE=7).
pub const FLOW_DIR_SINK: u8 = 0xFF;

/// D8 neighbour offset table. Index `i` in `0..=7` maps to `(dx, dy)`.
///
/// Order: E, NE, N, NW, W, SW, S, SE — clockwise from east.
///
/// `#[rustfmt::skip]` preserves the sign-aligned column layout and the
/// directional trailing comments. This table is tied to the `FLOW_DIR_SINK`
/// invariant (sink sentinel is `0xFF`, not `0`, because `0` is east) and the
/// comments are the first place a reader looks when debugging hydro stages.
#[rustfmt::skip]
pub const D8_OFFSETS: [(i32, i32); 8] = [
    ( 1,  0), // 0: E
    ( 1, -1), // 1: NE
    ( 0, -1), // 2: N
    (-1, -1), // 3: NW
    (-1,  0), // 4: W
    (-1,  1), // 5: SW
    ( 0,  1), // 6: S
    ( 1,  1), // 7: SE
];

// ─── 3-layer sub-structs ─────────────────────────────────────────────────────

/// Roadmap §数据层分离 §Minimal replay state — the "world truth" required to
/// fully re-run the pipeline. Sprint 1A fills `height`; Sprint 3 fills
/// `sediment`.
///
/// Serde note (Option B): both field payloads are `#[serde(skip)]` for
/// Sprint 0. The canonical path to persist heightmaps is Task 0.6's save
/// codec, which writes `ScalarField2D::to_bytes()` directly. Routing field
/// bytes through serde here would double-serialize them and couple the RON
/// save shape to the byte format of `ScalarField2D`. Sprint 0 only needs the
/// type layout — there is nothing in these fields to serialize yet.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AuthoritativeFields {
    /// Sprint 1A onward: topography output from `TopographyStage`.
    #[serde(skip)]
    pub height: Option<ScalarField2D<f32>>,

    /// Sprint 3 onward: hydraulic erosion sediment field. Sprint 1A may
    /// leave this `None` even once `height` is populated.
    #[serde(skip)]
    pub sediment: Option<ScalarField2D<f32>>,
}

/// Roadmap §数据层分离 §Baked snapshot state — cacheable derived-but-stable
/// fields (temperature, precipitation, soil moisture, biome weights, …).
///
/// Each field payload is `#[serde(skip)]`: the canonical save path for
/// these large float fields is `ScalarField2D::to_bytes` in the save
/// codec, not serde — exactly the pattern `AuthoritativeFields` uses
/// for `height` / `sediment`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BakedSnapshot {
    /// TemperatureStage (DD1): mean annual temperature in °C per cell.
    #[serde(skip)]
    pub temperature: Option<ScalarField2D<f32>>,

    /// PrecipitationStage (DD2): normalized `[0, 1]` annual precipitation
    /// proxy from an upwind raymarch. Not calibrated to mm/yr in v1.
    #[serde(skip)]
    pub precipitation: Option<ScalarField2D<f32>>,

    /// SoilMoistureStage (DD5): normalized `[0, 1]` soil-moisture proxy
    /// combining ET/PET, log-compressed accumulation, and river
    /// proximity, with a 1-pass downstream smoothing along `flow_dir`.
    /// Drives DD6 biome suitability.
    #[serde(skip)]
    pub soil_moisture: Option<ScalarField2D<f32>>,

    /// BiomeWeightsStage (DD6): per-cell normalised suitability
    /// vectors for every functional biome type.
    #[serde(skip)]
    pub biome_weights: Option<BiomeWeights>,
}

// ─── Hex layout + aggregation types ──────────────────────────────────────────

/// Orientation of a hexagonal grid. Sprint 1B ships `FlatTop` — two
/// parallel edges run along the screen X axis. `PointyTop` is on
/// reserve for the Sprint 5 full hex view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HexLayout {
    FlatTop,
}

/// 64×64 flat-top hex grid overlay on the simulation resolution.
/// `hex_id_of_cell` is a precomputed row-major lookup of "which hex
/// does this sim cell belong to", so aggregation passes are a simple
/// scatter-add without any hex-math inside the hot loop.
#[derive(Debug, Clone)]
pub struct HexGrid {
    pub cols: u32,
    pub rows: u32,
    pub hex_size: f32,
    pub layout: HexLayout,
    pub hex_id_of_cell: ScalarField2D<u32>,
}

/// Per-hex aggregated attributes from the sim-cell fields.
#[derive(Debug, Clone)]
pub struct HexAttributes {
    pub elevation: f32,
    pub slope: f32,
    pub rainfall: f32,
    pub temperature: f32,
    pub moisture: f32,
    pub biome_weights: Vec<f32>,
    pub dominant_biome: BiomeType,
    pub has_river: bool,
}

/// Flat `cols * rows` storage of [`HexAttributes`], row-major.
#[derive(Debug, Clone)]
pub struct HexAttributeField {
    pub attrs: Vec<HexAttributes>,
    pub cols: u32,
    pub rows: u32,
}

impl HexAttributeField {
    /// Row-major lookup: `(col, row) → index`.
    #[inline]
    pub fn index(&self, col: u32, row: u32) -> usize {
        (row * self.cols + col) as usize
    }

    /// Read-only access by hex coordinate.
    pub fn get(&self, col: u32, row: u32) -> &HexAttributes {
        &self.attrs[self.index(col, row)]
    }
}

/// Functional biome types used by `BiomeWeightsStage` (DD6).
///
/// Fixed ordering is load-bearing: the per-cell weight vector in
/// [`BiomeWeights::weights`] is indexed by this enum's variant ordinals
/// via [`BiomeType::ALL`], and the dominant-biome overlay relies on a
/// stable mapping from index → type. Adding a new variant is a
/// breaking change for the golden-seed regression snapshots.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BiomeType {
    CoastalScrub = 0,
    LowlandForest = 1,
    MontaneWetForest = 2,
    CloudForest = 3,
    DryShrub = 4,
    Grassland = 5,
    BareRockLava = 6,
    RiparianVegetation = 7,
}

impl BiomeType {
    /// Canonical ordering used for per-cell weight vectors.
    pub const ALL: [BiomeType; 8] = [
        BiomeType::CoastalScrub,
        BiomeType::LowlandForest,
        BiomeType::MontaneWetForest,
        BiomeType::CloudForest,
        BiomeType::DryShrub,
        BiomeType::Grassland,
        BiomeType::BareRockLava,
        BiomeType::RiparianVegetation,
    ];

    /// Number of biome types — keeps `BiomeWeights::weights` row count
    /// aligned with the enum without hand-maintained constants.
    pub const COUNT: usize = Self::ALL.len();
}

/// Per-cell normalised suitability vectors for every biome.
///
/// Layout is `[biome_index][cell_row_major_index]`. On every land cell
/// the sum across all biome indices is approximately `1.0` after the
/// DD6 basin-smoothing pass; sea cells stay at `0.0`. Produced by
/// `BiomeWeightsStage` and consumed by `HexProjectionStage`,
/// `BiomeWeightsStage::dominant_biome_at`, and overlay #10.
#[derive(Debug, Clone)]
pub struct BiomeWeights {
    pub types: [BiomeType; BiomeType::COUNT],
    pub weights: Vec<Vec<f32>>,
    pub width: u32,
    pub height: u32,
}

impl BiomeWeights {
    /// Create an all-zero weight grid of the requested size.
    pub fn new(width: u32, height: u32) -> Self {
        let cells = (width * height) as usize;
        Self {
            types: BiomeType::ALL,
            weights: vec![vec![0.0; cells]; BiomeType::COUNT],
            width,
            height,
        }
    }

    /// Row-major cell index.
    #[inline]
    pub fn index(&self, x: u32, y: u32) -> usize {
        (y * self.width + x) as usize
    }

    /// Argmax biome at `(x, y)`. Returns the first biome in canonical
    /// order on ties (deterministic).
    ///
    /// **Sea cell caveat**: `BiomeWeightsStage` leaves sea cells at
    /// all-zero weights (the stage is defined on land only). On such
    /// cells every entry ties at `0.0`, so this function returns
    /// `BiomeType::ALL[0]` (currently `CoastalScrub`). Overlays and
    /// hex aggregators must gate dominant-biome lookups on the land
    /// mask rather than assume this value is meaningful over water.
    pub fn dominant_biome_at(&self, x: u32, y: u32) -> BiomeType {
        let idx = self.index(x, y);
        let mut best_weight = f32::NEG_INFINITY;
        let mut best_biome = BiomeType::ALL[0];
        for (i, biome) in BiomeType::ALL.iter().enumerate() {
            let w = self.weights[i][idx];
            if w > best_weight {
                best_weight = w;
                best_biome = *biome;
            }
        }
        best_biome
    }
}

/// Per-hex river crossing debug info. Records which two of the 6 hex edges
/// the dominant river enters and exits through.
///
/// **DD3 6-edge encoding (Sprint 3.5.B c1).** Edge discriminants match
/// [`hex::geometry::HexEdge`] exactly — counter-clockwise from east:
///
/// ```text
///     2 (NW)  1 (NE)
///        \   /
/// 3 (W) --|    |-- 0 (E)
///        /   \
///     4 (SW)  5 (SE)
/// ```
///
/// | `u8` | Edge |
/// |------|------|
/// | 0 | E (east, vertical right side) |
/// | 1 | NE (upper-right diagonal) |
/// | 2 | NW (upper-left diagonal) |
/// | 3 | W (west, vertical left side) |
/// | 4 | SW (lower-left diagonal) |
/// | 5 | SE (lower-right diagonal) |
///
/// The struct shape (`entry_edge: u8, exit_edge: u8`) is stable between
/// Sprint 2.5's 4-box-edge encoding and Sprint 3.5.B's 6-hex-edge encoding
/// — only the value range expands from `0..=3` to `0..=5`.
///
/// Lives inside `HexDebugAttributes` (not `HexAttributes`) so the stable
/// `HexAttributes` 8-field contract is not disturbed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HexRiverCrossing {
    pub entry_edge: u8,
    pub exit_edge: u8,
}

/// Width bucket for a hex's dominant river, per DD3.
///
/// Computed per hex as the bucketised `max(flow_accumulation)` over all
/// sim cells inside the hex. `None` when the hex has no river crossing
/// (no river cells or pre-accumulation pipeline run).
///
/// Bucket boundaries are global thresholds tuned empirically against the
/// Sprint 3.5 hero seeds (`docs/design/sprints/sprint_3_5_hex_surface_readability.md`
/// §2 DD3). v1 ships a single global pair; per-archetype refinement is
/// deferred to Sprint 3.5.F polish if hero-shot visuals demand it.
///
/// `#[repr(u8)]` with explicit discriminants so `HexInstance.width_bucket_bits`
/// (populated in 3.5.B c4's `HexRiverRenderer`) can use the u8 cast for
/// per-instance buffer packing.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RiverWidth {
    Small = 0,
    Medium = 1,
    Main = 2,
}

/// Per-hex debug attributes computed by `HexProjectionStage` alongside the
/// main `HexAttributes` aggregation. All `Vec` fields have length
/// `cols * rows` and are row-major, matching `HexAttributeField`.
#[derive(Debug, Clone)]
pub struct HexDebugAttributes {
    /// Variance of `derived.slope` over the land sim cells in each hex.
    ///
    /// Computed as `E[slope²] − (E[slope])²` using `f64` accumulators to
    /// avoid catastrophic cancellation. Small negative results from fp noise
    /// are clamped to `0.0`. Hexes with no land sim cells keep `0.0`.
    pub slope_variance: Vec<f32>,

    /// Per-hex accessibility cost `1 + W_SLOPE * mean_slope
    /// + W_RIVER * river_penalty + W_CLIFF * cliff_penalty`.
    ///
    /// `river_penalty = 1.0` if any cell in the hex has `river_mask == 1`,
    /// else `0.0`. `cliff_penalty = mean(coast_type[p] == Cliff ? 1.0 : 0.0)`
    /// over all sim cells in the hex (`Unknown` sentinel cells contribute `0.0`
    /// to the numerator but count in the denominator). Hexes with no sim cells
    /// get cost `1.0`.
    pub accessibility_cost: Vec<f32>,

    /// Per-hex river crossing: which hex edges the dominant river thread
    /// enters and exits. `None` for hexes with no river sim cells or where
    /// the accumulation field was absent when the stage ran.
    ///
    /// **DD3 6-edge encoding (Sprint 3.5.B c1):** 0 = E, 1 = NE, 2 = NW,
    /// 3 = W, 4 = SW, 5 = SE — counter-clockwise from east, matching
    /// [`hex::geometry::HexEdge`]. See [`HexRiverCrossing`].
    pub river_crossing: Vec<Option<HexRiverCrossing>>,

    /// Per-hex river width bucket (DD3). `Some(bucket)` when the hex has a
    /// river crossing; `None` otherwise. Parallel to `river_crossing` in
    /// length and indexing.
    ///
    /// Read by 3.5.B c4's `HexRiverRenderer` to pick polyline thickness;
    /// NOT consumed by any DD8 hash (adding this field does NOT drift
    /// `hex_debug_river_crossing_hash`).
    pub river_width: Vec<Option<RiverWidth>>,
}

/// Sprint 2 DD3/DD8: snapshot of pre-erosion height / land statistics,
/// captured by `ErosionOuterLoop` before its first iteration and read by
/// the `erosion_no_explosion` / `erosion_no_excessive_sea_crossing`
/// invariants in `core::validation` (Task 2.9).
///
/// Lives inside `DerivedCaches` (which is `#[serde(skip)]`), so this struct
/// is purely in-memory and deliberately does **not** derive
/// `serde::Serialize` / `Deserialize`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ErosionBaseline {
    /// Maximum `authoritative.height` value across all land cells, sampled
    /// immediately before the first inner SPIM iteration runs.
    pub max_height_pre: f32,

    /// Count of cells with `coast_mask.is_land == 1` at snapshot time. Used
    /// by `erosion_no_excessive_sea_crossing` to bound how many cells
    /// crossed the sea-level threshold during erosion.
    pub land_cell_count_pre: u32,
}

/// Sprint 2 DD4 / Sprint 3 DD6: coarse coastal geomorphology classification
/// per coast cell.
///
/// v1 (Sprint 2) uses cheap proxies (slope + river_mouth + shoreline_normal ·
/// wind exposure + island_age); v2 (Sprint 3) adds a 16-direction fetch
/// integral and the [`CoastType::LavaDelta`] variant. `Unknown = 0xFF` is
/// the sentinel for non-coast cells; the `coast_type_well_formed` invariant
/// enforces that every `is_coast == 1` cell carries a `0..=4` value and
/// every non-coast cell carries `0xFF`. Sprint 3 widened the legal range
/// from `0..=3` to `0..=4` to admit LavaDelta under either classifier.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum CoastType {
    Cliff = 0,
    Beach = 1,
    Estuary = 2,
    RockyHeadland = 3,
    /// Sprint 3 DD6: fresh volcanic delta — low-to-moderate slope coast
    /// close to a volcanic center on a Young island. Only emitted by the
    /// v2 fetch-integral classifier; v1 never writes this variant.
    LavaDelta = 4,
    Unknown = 0xFF,
}

impl CoastType {
    /// Parse a raw discriminant byte. Returns `None` for out-of-range values
    /// (i.e. anything that is not `0..=4` or `0xFF`).
    ///
    /// Mirrors the `HexEdge::from_u8` pattern from `hex::geometry`.
    #[inline]
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Cliff),
            1 => Some(Self::Beach),
            2 => Some(Self::Estuary),
            3 => Some(Self::RockyHeadland),
            4 => Some(Self::LavaDelta),
            0xFF => Some(Self::Unknown),
            _ => None,
        }
    }
}

/// Sprint 3.5 DD4: per-hex coast classification derived from the fetch-weighted
/// majority vote of the underlying sim-cell [`CoastType`] values.
///
/// 7 classes total. `Inland` = hex contains no sea cells; `OpenOcean` = no
/// land cells; the 5 coast classes mirror Sprint 3's [`CoastType`] but at hex
/// granularity via the `sim::hex_coast_class` classifier.
///
/// `#[repr(u8)]` discriminants are load-bearing — they map 1:1 to the
/// `coast_class_bits` field on `HexInstance` (c4 of this sprint / later
/// edge-decoration renderer) AND are hashed by DD8's `hex_coast_class_hash`.
/// Adding a variant requires a snapshot regen.
///
/// **Placement rationale** (per plan §2 DD4): this enum lives in
/// `core::world` rather than `sim::hex_coast_class` because
/// [`DerivedCaches`] owns `Option<Vec<HexCoastClass>>` and `core`
/// cannot depend on `sim` (crate DAG invariant — core is the sink).
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HexCoastClass {
    Inland = 0,
    OpenOcean = 1,
    Beach = 2,
    RockyHeadland = 3,
    Estuary = 4,
    Cliff = 5,
    LavaDelta = 6,
}

impl HexCoastClass {
    pub const ALL: [HexCoastClass; 7] = [
        Self::Inland,
        Self::OpenOcean,
        Self::Beach,
        Self::RockyHeadland,
        Self::Estuary,
        Self::Cliff,
        Self::LavaDelta,
    ];

    /// Parse a raw discriminant byte. Returns `None` for out-of-range values.
    ///
    /// Mirrors the `HexEdge::from_u8` pattern from `hex::geometry`.
    #[inline]
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Inland),
            1 => Some(Self::OpenOcean),
            2 => Some(Self::Beach),
            3 => Some(Self::RockyHeadland),
            4 => Some(Self::Estuary),
            5 => Some(Self::Cliff),
            6 => Some(Self::LavaDelta),
            _ => None,
        }
    }
}

/// Land / sea / coast classification produced by `CoastMaskStage`.
///
/// `land_cell_count` is cached so downstream stages avoid re-popcounting
/// `is_land`. `river_mouth_mask` starts `None` and is backfilled by
/// `RiverExtractionStage`.
#[derive(Debug, Clone)]
pub struct CoastMask {
    pub is_land: MaskField2D,
    pub is_sea: MaskField2D,
    pub is_coast: MaskField2D,
    pub land_cell_count: u32,
    pub river_mouth_mask: Option<MaskField2D>,
}

/// Roadmap §数据层分离 §Derived fields — pure runtime caches (flow_dir,
/// flow_accumulation, coast_mask, river_mask, …).
///
/// **Not serialized.** Reconstructable from `authoritative + preset` on
/// load/replay. Sprint 1A fills this struct; Sprint 0 left it empty.
#[derive(Debug, Clone, Default)]
pub struct DerivedCaches {
    /// Sprint 1A TopographyStage: snapshot of `volcanic_base + ridge_field`
    /// BEFORE coastal_falloff is subtracted. Used by `initial_uplift` overlay.
    pub initial_uplift: Option<ScalarField2D<f32>>,

    /// Sprint 1A PitFillStage: pit-filled terrain (`z_filled >= z_raw`).
    /// `authoritative.height` is z_raw and stays unchanged.
    pub z_filled: Option<ScalarField2D<f32>>,

    /// Sprint 1A DerivedGeomorphStage: `|grad z_filled|` finite-diff cache.
    /// Consumed by slope overlay, Sprint 1B biome suitability, Sprint 2 SPIM.
    pub slope: Option<ScalarField2D<f32>>,

    /// DerivedGeomorphStage: `laplacian(z_filled)` via 5-point stencil.
    /// Used by the curvature overlay, Sprint 1B biome suitability, and
    /// Sprint 2 hillslope diffusion. Sea cells are forced to `0.0`.
    pub curvature: Option<ScalarField2D<f32>>,

    /// FogLikelihoodStage (DD7): `[0, 1]` proxy combining an elevation
    /// band (cloud base ↔ cloud top) with orographic uplift. Consumed
    /// by DD6 CloudForest biome suitability.
    pub fog_likelihood: Option<ScalarField2D<f32>>,

    /// PetStage (DD3): Hamon-style potential evapotranspiration proxy
    /// from temperature. Drives Budyko ET/P split in DD4.
    pub pet: Option<ScalarField2D<f32>>,

    /// WaterBalanceStage (DD4): Budyko Fu-equation actual
    /// evapotranspiration. `ET + R = P` by construction.
    pub et: Option<ScalarField2D<f32>>,

    /// WaterBalanceStage (DD4): long-term-average runoff `P - ET`.
    /// Drives DD5 soil moisture and Sprint 2 stream-power erosion.
    pub runoff: Option<ScalarField2D<f32>>,

    /// HexProjectionStage (DD8): precomputed 64×64 flat-top hex grid
    /// plus `hex_id_of_cell` lookup. Invariant under slider re-runs
    /// that don't touch simulation resolution.
    pub hex_grid: Option<HexGrid>,

    /// HexProjectionStage (DD8): aggregated per-hex attributes
    /// (elevation, slope, rainfall, temperature, moisture, biome
    /// weights, dominant biome, river flag).
    pub hex_attrs: Option<HexAttributeField>,

    /// BiomeWeightsStage (DD6): per-cell argmax biome index (cast from
    /// `BiomeType`). Rendered as the "dominant biome" overlay via
    /// the `Categorical` palette. Produced alongside the full
    /// `baked.biome_weights` during Sprint 1B pipeline runs so the
    /// overlay path can reuse the existing ScalarField2D<u32>
    /// resolve-branch that basin_id uses.
    pub dominant_biome_per_cell: Option<ScalarField2D<u32>>,

    /// HexProjectionStage (DD8): per-sim-cell dominant biome of the
    /// hex that contains it, cast to u32. Rendered as the "hex
    /// aggregated" overlay — visually shows hex-level classification
    /// at sim resolution so the terrain mesh can colour it without
    /// a dedicated hex render pipeline.
    pub hex_dominant_per_cell: Option<ScalarField2D<u32>>,

    /// Sprint 1A CoastMaskStage: land / sea / coast masks + cached counts.
    pub coast_mask: Option<CoastMask>,

    /// Sprint 1A CoastMaskStage: per-coast-cell outward shoreline normal.
    pub shoreline_normal: Option<VectorField2D>,

    /// Sprint 2 DD4 CoastTypeStage: per-cell coastal geomorphology
    /// classification (`CoastType as u8`). Coast cells carry `0..=3`;
    /// non-coast cells carry `CoastType::Unknown = 0xFF`.
    /// `None` until `CoastTypeStage` has run.
    pub coast_type: Option<ScalarField2D<u8>>,

    /// Sprint 1A FlowRoutingStage: D8 downstream direction code
    /// (see `FlowDir` constants; 0xFF = sink / no downstream).
    pub flow_dir: Option<ScalarField2D<u8>>,

    /// Sprint 1A AccumulationStage: upstream cell count (f32 for `A^m` in stream power).
    pub accumulation: Option<ScalarField2D<f32>>,

    /// Sprint 1A BasinsStage: drainage basin id (0 = sea/unlabeled; 1+ = by row-major sink order).
    pub basin_id: Option<ScalarField2D<u32>>,

    /// Sprint 1A RiverExtractionStage: extracted main river network.
    pub river_mask: Option<MaskField2D>,

    /// Sprint 2 DD3/DD8: pre-erosion snapshot captured once by
    /// `ErosionOuterLoop::run` before its first inner iteration. Consumed
    /// by the `erosion_no_explosion` / `erosion_no_excessive_sea_crossing`
    /// invariants (Task 2.9). `None` until `ErosionOuterLoop` has run.
    pub erosion_baseline: Option<ErosionBaseline>,

    /// Per-hex debug attributes (`slope_variance`, `accessibility_cost`).
    /// `None` until `HexProjectionStage` has run.
    pub hex_debug: Option<HexDebugAttributes>,

    /// Broadcast of `hex_debug.slope_variance[h]` to each sim cell in hex `h`.
    /// Drives the `hex_projection_error` overlay. `None` until
    /// `HexProjectionStage` has run.
    pub hex_slope_variance_per_cell: Option<ScalarField2D<f32>>,

    /// Broadcast of `hex_debug.accessibility_cost[h]` to each sim cell in
    /// hex `h`. Drives the `hex_accessibility` overlay. `None` until
    /// `HexProjectionStage` has run.
    pub hex_accessibility_per_cell: Option<ScalarField2D<f32>>,

    /// Pre-rasterised river-crossing line segments as a `MaskField2D` at sim
    /// resolution. For each hex with a `Some` river crossing, a 1-pixel-wide
    /// Bresenham line is drawn from the midpoint of `entry_edge` to the
    /// midpoint of `exit_edge`. `None` until `HexProjectionStage` has run
    /// (all-zero when no river hexes exist or when `derived.accumulation` was
    /// absent).
    pub hex_river_crossing_mask: Option<MaskField2D>,

    /// Sprint 3 DD3: per-cell sediment deposition rate `D[p]` produced by
    /// `SedimentUpdateStage` during the `ErosionOuterLoop` inner step.
    ///
    /// `D[p] = max(0, Qs_in[p] − Qs_cap[p])` in normalised flux units; the
    /// same value that is added to `authoritative.sediment[p]` inside the
    /// topo-order sweep. Populated every inner iteration; the final value
    /// after the last inner step is what the `deposition_flux` overlay
    /// (Task 3.7) and any validation invariants read.
    ///
    /// Not part of the save codec — this is pure runtime-derived state.
    pub deposition_flux: Option<ScalarField2D<f32>>,

    /// Sprint 3 DD4: LFPM upwind-sweep order cached across slider reruns at
    /// fixed wind direction. Each element is a flat row-major cell index
    /// `iy * width + ix`. Sorted ascending by `wind_phase = -wind · position`
    /// so upstream cells are processed before downstream cells in the sweep.
    ///
    /// Invalidated when `preset.prevailing_wind_dir` changes, via the
    /// `Precipitation` arm of `clear_stage_outputs` in `sim::invalidation`.
    /// `None` until the first V3 sweep run. Reused on repeated calls with
    /// the same wind direction.
    pub precipitation_sweep_order: Option<Vec<usize>>,

    /// Sprint 3 DD5: fog-water input derived from `fog_likelihood` and the
    /// `FOG_WATER_GAIN` coupling constant. Recomputed every `SoilMoistureStage`
    /// run; consumed by the `fog_water_input` overlay (Task 3.7). Not a
    /// baked field because it is a linear scaling of `fog_likelihood` — no
    /// reason to persist it across pipeline boundaries.
    ///
    /// `fog_water_input[p] = FOG_WATER_GAIN * fog_likelihood[p]`
    /// for land cells; `None` until `SoilMoistureStage` has run.
    pub fog_water_input: Option<ScalarField2D<f32>>,

    /// Sprint 3 DD6: volcanic-center coordinates in normalized `[0, 1]²`
    /// grid space. Populated by `TopographyStage` from the centers it
    /// samples internally; consumed by the v2 `CoastTypeStage` classifier
    /// for LavaDelta detection (distance-to-nearest-volcanic-center test).
    ///
    /// `#[serde(skip)]` by DerivedCaches convention (the whole struct is
    /// skipped) — purely runtime state. Invalidated under the Topography
    /// arm of `clear_stage_outputs`.
    pub volcanic_centers: Option<Vec<[f32; 2]>>,

    /// Sprint 3.5 DD4: per-cell fetch-integral (sum of windward open-water
    /// exposure from 16 raycast directions). Land cells hold the raycast
    /// result; sea cells hold `0.0`. Computed and persisted by
    /// [`CoastTypeStage`] — read by the `sim::hex_coast_class` classifier
    /// to produce `hex_coast_class`. Intermediate derived field — not
    /// hashed in `SummaryMetrics` directly; the downstream
    /// `hex_coast_class_hash` witnesses the classifier's output.
    ///
    /// Invalidation: cleared under `StageId::CoastType` arm (co-located
    /// with `coast_type`, NOT `Coastal`).
    pub coast_fetch_integral: Option<crate::field::ScalarField2D<f32>>,

    /// Sprint 3.5 DD4: per-hex coast classification. `Vec<HexCoastClass>`
    /// with one entry per hex (indexed in row-major order matching
    /// [`HexAttributeField`]). Populated by the `sim::hex_coast_class`
    /// classifier during [`HexProjectionStage::run`].
    ///
    /// Invalidation: cleared under `StageId::HexProjection` arm (same
    /// arm as `hex_attrs` / `hex_debug` / `hex_grid`).
    pub hex_coast_class: Option<Vec<HexCoastClass>>,
}

// ─── WorldState ──────────────────────────────────────────────────────────────

/// The top-level world state passed through the simulation pipeline.
///
/// The field layout is the architectural invariant: exactly the six fields
/// below, no extras. New data belongs inside one of `authoritative`,
/// `baked`, or `derived` — never as a new top-level `Option`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldState {
    pub seed: Seed,
    pub preset: IslandArchetypePreset,
    pub resolution: Resolution,

    /// Roadmap §数据层分离 §Minimal replay state.
    pub authoritative: AuthoritativeFields,

    /// Roadmap §数据层分离 §Baked snapshot state.
    pub baked: BakedSnapshot,

    /// Roadmap §数据层分离 §Derived fields — runtime cache only, never
    /// persisted. `#[serde(skip)]` ensures save/load never reads or writes
    /// this; on deserialize we rebuild from `Default`.
    #[serde(skip)]
    pub derived: DerivedCaches,
}

impl WorldState {
    /// Construct a fresh `WorldState`. All three sub-structs start at their
    /// `Default` values — this is the Sprint 0 "empty world": no height,
    /// no baked fields, no derived caches.
    pub fn new(seed: Seed, preset: IslandArchetypePreset, resolution: Resolution) -> Self {
        Self {
            seed,
            preset,
            resolution,
            authoritative: AuthoritativeFields::default(),
            baked: BakedSnapshot::default(),
            derived: DerivedCaches::default(),
        }
    }
}

// ─── tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::preset::IslandAge;

    // We deliberately do NOT import `data::presets::load_preset` here — that
    // would create a dev-dep from `core` to `data` and poison the
    // `cargo tree -p core` invariant. Construct the preset inline instead.
    fn test_preset() -> IslandArchetypePreset {
        IslandArchetypePreset {
            name: "test".into(),
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

    /// Sprint 3.5.C c4 DD4 discriminant lock: `HexCoastClass` `#[repr(u8)]`
    /// discriminants are load-bearing — they are packed into `HexInstance`
    /// `coast_class_bits`, hashed by DD8's `hex_coast_class_hash`, and
    /// consumed by the hex-surface WGSL shader. Any renumbering requires a
    /// snapshot regen and a documented rationale.
    ///
    /// Enforcement mechanism: explicit `as u8` assertions for every variant +
    /// `ALL.len()` to catch additions. Adding, removing, or renumbering a
    /// variant → test fails.
    #[test]
    fn hex_coast_class_discriminants_stable() {
        assert_eq!(HexCoastClass::Inland as u8, 0);
        assert_eq!(HexCoastClass::OpenOcean as u8, 1);
        assert_eq!(HexCoastClass::Beach as u8, 2);
        assert_eq!(HexCoastClass::RockyHeadland as u8, 3);
        assert_eq!(HexCoastClass::Estuary as u8, 4);
        assert_eq!(HexCoastClass::Cliff as u8, 5);
        assert_eq!(HexCoastClass::LavaDelta as u8, 6);
        assert_eq!(HexCoastClass::ALL.len(), 7);
    }

    /// Sprint 3.5 §4 item 1: `HexAttributes` must have exactly the 8 fields
    /// {elevation, slope, rainfall, temperature, moisture, biome_weights,
    /// dominant_biome, has_river}. This is the Sprint 5 S2 settlement /
    /// suitability consumer contract — adding, removing, or renaming a
    /// field here is a breaking change for downstream stages.
    ///
    /// Enforcement mechanism: exhaustive destructure of the struct. Adding a
    /// field makes the destructure inexhaustive → compile error. Removing a
    /// field makes the destructure reference a non-existent ident → compile
    /// error. Renaming a field does the same. The destructure must NOT use
    /// `..` — that would silently tolerate new fields.
    ///
    /// Net: this is a compile-time anchor, not a runtime assertion. The
    /// `#[test]` wrapper is only here so `cargo test` reports it; its body
    /// never actually exercises behaviour.
    #[test]
    fn hex_attributes_eight_fields_stable() {
        let h = HexAttributes {
            elevation: 0.0,
            slope: 0.0,
            rainfall: 0.0,
            temperature: 0.0,
            moisture: 0.0,
            biome_weights: Vec::new(),
            dominant_biome: BiomeType::CoastalScrub,
            has_river: false,
        };
        // Exhaustive destructure — no `..`. Drift in any field fails compile.
        let HexAttributes {
            elevation,
            slope,
            rainfall,
            temperature,
            moisture,
            biome_weights,
            dominant_biome,
            has_river,
        } = h;
        let _ = (
            elevation,
            slope,
            rainfall,
            temperature,
            moisture,
            biome_weights,
            dominant_biome,
            has_river,
        );
    }

    // 1. new() produces a world with empty authoritative / baked / derived.
    #[test]
    fn world_state_new_defaults() {
        let world = WorldState::new(Seed(42), test_preset(), Resolution::new(256, 256));
        assert!(world.authoritative.height.is_none());
        assert!(world.authoritative.sediment.is_none());
        // baked / derived are unit-like; constructing them via default here
        // is sufficient to prove they compile with `Default`.
        let _ = BakedSnapshot::default();
        let _ = DerivedCaches::default();
        assert_eq!(world.resolution.sim_width, 256);
        assert_eq!(world.resolution.sim_height, 256);
        assert_eq!(world.seed, Seed(42));
    }

    // 2. Resolution exposes exactly sim_width / sim_height — no render / hex.
    //    This is a compile-time pattern match that will fail to build if
    //    someone adds extra public fields to `Resolution`.
    #[test]
    fn world_state_resolution_fields() {
        let r = Resolution::new(128, 64);
        let Resolution {
            sim_width,
            sim_height,
        } = r;
        assert_eq!(sim_width, 128);
        assert_eq!(sim_height, 64);
    }

    // 3. Serde round-trip: seed / preset / resolution / baked survive, and
    //    the `derived` field is NOT present in the serialized form.
    //    (Option B: authoritative.height/sediment are also skipped.)
    #[test]
    fn world_state_serde_skips_derived() {
        let world = WorldState::new(Seed(7), test_preset(), Resolution::new(64, 32));
        let s = ron::to_string(&world).expect("serialize WorldState");

        assert!(
            !s.contains("derived"),
            "derived field must be skipped in serialization, got: {s}"
        );

        let decoded: WorldState = ron::from_str(&s).expect("deserialize WorldState");
        assert_eq!(decoded.seed, world.seed);
        assert_eq!(decoded.preset, world.preset);
        assert_eq!(decoded.resolution, world.resolution);
        // authoritative fields stay None on both sides (skipped payload)
        assert!(decoded.authoritative.height.is_none());
        assert!(decoded.authoritative.sediment.is_none());
    }
}
