//! Crate-local test fixtures shared across `#[cfg(test)]` unit tests
//! within `crates/core`.
//!
//! **Sprint 3.4 Pattern A.** This module is `#[cfg(test)] pub(crate)` —
//! it is compiled only for the test profile and is **not** visible to
//! integration tests in `crates/core/tests/*.rs` (those see the crate
//! as an external library, where `#[cfg(test)]` gating hides this
//! module). If a fixture is needed on both sides, duplicate it on the
//! integration side rather than promoting this module to the crate's
//! public API — see CLAUDE.md Sprint 3.4 Gotcha.

use crate::preset::{IslandAge, IslandArchetypePreset};

/// The shared `IslandArchetypePreset` used across `core::validation`'s
/// per-family test modules. Byte-identical to the 5 inline copies that
/// lived in `validation/{biome,climate,erosion,hex,hydro}.rs` prior to
/// Sprint 3.4.
pub(crate) fn test_preset() -> IslandArchetypePreset {
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
