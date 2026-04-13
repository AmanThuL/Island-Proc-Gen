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

use crate::field::ScalarField2D;
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
        Self { sim_width, sim_height }
    }
}

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
/// Sprint 0 leaves this empty on purpose: Sprint 1B onward will append
/// fields without breaking `#[derive(Default)]`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BakedSnapshot {}

/// Roadmap §数据层分离 §Derived fields — pure runtime caches (flow_dir,
/// flow_accumulation, coast_mask, river_mask, …).
///
/// **Not serialized.** Reconstructable from `authoritative + preset` on
/// load/replay. Sprint 0 leaves it empty.
#[derive(Debug, Clone, Default)]
pub struct DerivedCaches {}

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
        let Resolution { sim_width, sim_height } = r;
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
