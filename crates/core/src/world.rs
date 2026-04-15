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

    /// Sprint 1A CoastMaskStage: land / sea / coast masks + cached counts.
    pub coast_mask: Option<CoastMask>,

    /// Sprint 1A CoastMaskStage: per-coast-cell outward shoreline normal.
    pub shoreline_normal: Option<VectorField2D>,

    /// Sprint 1A FlowRoutingStage: D8 downstream direction code
    /// (see `FlowDir` constants; 0xFF = sink / no downstream).
    pub flow_dir: Option<ScalarField2D<u8>>,

    /// Sprint 1A AccumulationStage: upstream cell count (f32 for `A^m` in stream power).
    pub accumulation: Option<ScalarField2D<f32>>,

    /// Sprint 1A BasinsStage: drainage basin id (0 = sea/unlabeled; 1+ = by row-major sink order).
    pub basin_id: Option<ScalarField2D<u32>>,

    /// Sprint 1A RiverExtractionStage: extracted main river network.
    pub river_mask: Option<MaskField2D>,
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
        }
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
