//! Island archetype presets.
//!
//! [`IslandArchetypePreset`] is the primary configuration struct that Sprint 1A
//! `TopographyStage` and later pipeline stages consume.  The actual `.ron`
//! files and loading logic live in `crates/data/src/presets.rs`; this module
//! only provides the types so that `core` has no dependency on `data`.

// ─── types ────────────────────────────────────────────────────────────────────

/// Age of the island (affects erosion, relief, and geomorphology).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum IslandAge {
    /// Volcanic shield is active; sharp peaks, high relief.
    Young,
    /// Caldera stage; moderate erosion, mid-range relief.
    Mature,
    /// Heavily eroded atoll-like form; low relief, wide lagoons.
    Old,
}

/// Configuration for a single island archetype.
///
/// All floating-point fields use the following conventions unless noted:
/// * `[0, 1]` values are fractions of the half-domain (radius) or normalised
///   elevation/moisture intensities.
/// * Angles are in **radians**.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct IslandArchetypePreset {
    /// Human-readable identifier (matches the `.ron` file stem).
    pub name: String,

    /// Radius of the main island mass as a fraction of half the domain size.
    /// Range: `[0, 1]`.
    pub island_radius: f32,

    /// Peak elevation as a fraction of the maximum possible relief.
    /// Range: `[0, 1]`.
    pub max_relief: f32,

    /// Number of distinct volcanic summit centres.
    pub volcanic_center_count: u32,

    /// Geomorphological age; controls erosion and surface roughness.
    pub island_age: IslandAge,

    /// Direction of the prevailing trade winds, in radians (0 = east).
    pub prevailing_wind_dir: f32,

    /// Intensity of marine moisture advection from the ocean.
    /// Range: `[0, 1]`.
    pub marine_moisture_strength: f32,

    /// Fraction of domain elevation range that defines the ocean surface.
    /// Range: `[0, 1]`.
    pub sea_level: f32,
}

// ─── tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn example_preset() -> IslandArchetypePreset {
        IslandArchetypePreset {
            name: "test_island".to_string(),
            island_radius: 0.5,
            max_relief: 0.8,
            volcanic_center_count: 1,
            island_age: IslandAge::Young,
            prevailing_wind_dir: std::f32::consts::FRAC_PI_2,
            marine_moisture_strength: 0.75,
            sea_level: 0.30,
        }
    }

    // 1. full preset RON serde round-trip
    #[test]
    fn island_archetype_serde_roundtrip() {
        let original = example_preset();
        let serialized = ron::to_string(&original).expect("serialize failed");
        let deserialized: IslandArchetypePreset =
            ron::from_str(&serialized).expect("deserialize failed");
        assert_eq!(original, deserialized);
    }

    // 2. each IslandAge variant survives round-trip
    #[test]
    fn island_age_enum_roundtrip() {
        for variant in [IslandAge::Young, IslandAge::Mature, IslandAge::Old] {
            let s = ron::to_string(&variant).expect("serialize failed");
            let decoded: IslandAge = ron::from_str(&s).expect("deserialize failed");
            assert_eq!(variant, decoded);
        }
    }
}
