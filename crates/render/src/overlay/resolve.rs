//! Overlay source resolution — the **one** file in the codebase that maps
//! `WorldState` field-key strings to typed field borrows.
//!
//! **Guardrail (CLAUDE.md / AGENTS.md invariant #8):** raw string field
//! keys (e.g. `"height"`, `"sediment"`, `"deposition_flux"`,
//! `"fog_water_input"`) are allowed ONLY inside this file (`overlay/resolve.rs`).
//! The catalog, range, and mod files refer to overlay sources via
//! [`SourceKey`] enum handles, never by raw string. Adding a new overlay
//! means adding a [`SourceKey`] variant, a [`source_for`] arm, AND a
//! resolver arm in [`resolve_scalar_source`].

use island_core::{
    field::{MaskField2D, ScalarField2D},
    world::WorldState,
};

// ─── SourceKey ────────────────────────────────────────────────────────────────

/// Strongly-typed handle that identifies a single `WorldState` sub-field.
///
/// **All** raw `"field_name"` strings live exclusively in the
/// [`source_for`] and [`resolve_scalar_source`] functions below.
/// The catalog and every other file reference overlay sources through
/// this enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKey {
    // Sprint 1A derived scalars
    InitialUplift,
    ZFilled,
    Slope,
    Accumulation,
    BasinId,
    RiverMask,
    // Sprint 1B baked scalars
    Precipitation,
    Temperature,
    SoilMoisture,
    // Sprint 1B derived scalars
    Curvature,
    DominantBiomePerCell,
    HexDominantPerCell,
    // Sprint 2 / Sprint 3 DD6
    CoastType,
    HexSlopeVariancePerCell,
    HexAccessibilityPerCell,
    HexRiverCrossingMask,
    // Sprint 3 authoritative
    Sediment,
    // Sprint 3 derived
    DepositionFlux,
    FogWaterInput,
}

// ─── OverlaySource ───────────────────────────────────────────────────────────

/// Identifies which `WorldState` sub-field an overlay reads from.
///
/// The `&'static str` values are **field-key strings** that the render
/// path uses to locate the correct `Option<ScalarField2D<f32>>` (or
/// equivalent) in `AuthoritativeFields`, `BakedSnapshot`, or `DerivedCaches`.
///
/// **These string literals must stay confined to `overlay/resolve.rs`.** See the
/// module doc for the full guardrail explanation.
///
/// New overlays should be constructed via [`source_for`] rather than embedding
/// raw string literals in the catalog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlaySource {
    /// A field in `island_core::world::AuthoritativeFields` — world-truth data.
    ScalarAuthoritative(&'static str),
    /// A field in `island_core::world::BakedSnapshot` — stable derived snapshot.
    ScalarBaked(&'static str),
    /// A field in `island_core::world::DerivedCaches` — pure runtime cache.
    ScalarDerived(&'static str),
    /// A mask field (boolean/u8) from any layer.
    Mask(&'static str),
    /// A vector field from any layer.
    Vector(&'static str),
}

/// Map a [`SourceKey`] handle to the corresponding [`OverlaySource`] value.
///
/// This is the **canonical** place where `SourceKey` variants are translated
/// to their `&'static str` field-key strings. `catalog.rs` always calls this
/// function so that raw strings never appear outside this file.
pub fn source_for(key: SourceKey) -> OverlaySource {
    use SourceKey::*;
    match key {
        // Sprint 1A derived scalars
        InitialUplift => OverlaySource::ScalarDerived("initial_uplift"),
        ZFilled => OverlaySource::ScalarDerived("z_filled"),
        Slope => OverlaySource::ScalarDerived("slope"),
        Accumulation => OverlaySource::ScalarDerived("accumulation"),
        BasinId => OverlaySource::ScalarDerived("basin_id"),
        RiverMask => OverlaySource::Mask("river_mask"),
        // Sprint 1B baked scalars
        Precipitation => OverlaySource::ScalarBaked("precipitation"),
        Temperature => OverlaySource::ScalarBaked("temperature"),
        SoilMoisture => OverlaySource::ScalarBaked("soil_moisture"),
        // Sprint 1B derived scalars
        Curvature => OverlaySource::ScalarDerived("curvature"),
        DominantBiomePerCell => OverlaySource::ScalarDerived("dominant_biome_per_cell"),
        HexDominantPerCell => OverlaySource::ScalarDerived("hex_dominant_per_cell"),
        // Sprint 2 / Sprint 3 DD6
        CoastType => OverlaySource::ScalarDerived("coast_type"),
        HexSlopeVariancePerCell => OverlaySource::ScalarDerived("hex_slope_variance_per_cell"),
        HexAccessibilityPerCell => OverlaySource::ScalarDerived("hex_accessibility_per_cell"),
        HexRiverCrossingMask => OverlaySource::Mask("hex_river_crossing_mask"),
        // Sprint 3 authoritative
        Sediment => OverlaySource::ScalarAuthoritative("sediment"),
        // Sprint 3 derived
        DepositionFlux => OverlaySource::ScalarDerived("deposition_flux"),
        FogWaterInput => OverlaySource::ScalarDerived("fog_water_input"),
    }
}

// ─── ResolvedField ────────────────────────────────────────────────────────────

/// Typed borrow of a `WorldState` field — returned by [`resolve_scalar_source`].
///
/// Confines all `&'static str` field-key dispatch to this file. `overlay_render`
/// receives this typed handle and never sees the string keys.
pub(crate) enum ResolvedField<'a> {
    F32(&'a ScalarField2D<f32>),
    U32(&'a ScalarField2D<u32>),
    Mask(&'a MaskField2D),
}

/// Resolve an [`OverlaySource`] to a typed field borrow from `world`.
///
/// Returns `None` if the field has not been populated yet (e.g. the pipeline
/// has not run) or if the source key is unrecognised for Sprint 1A.
/// All `&'static str` dispatch lives **only** here — callers work with the
/// typed [`ResolvedField`] enum.
pub(crate) fn resolve_scalar_source<'w>(
    world: &'w WorldState,
    source: OverlaySource,
) -> Option<ResolvedField<'w>> {
    use OverlaySource::*;
    match source {
        // Sprint 1A derived scalars.
        ScalarDerived("initial_uplift") => world
            .derived
            .initial_uplift
            .as_ref()
            .map(ResolvedField::F32),
        ScalarDerived("z_filled") => world.derived.z_filled.as_ref().map(ResolvedField::F32),
        ScalarDerived("slope") => world.derived.slope.as_ref().map(ResolvedField::F32),
        ScalarDerived("accumulation") => {
            world.derived.accumulation.as_ref().map(ResolvedField::F32)
        }
        ScalarDerived("basin_id") => world.derived.basin_id.as_ref().map(ResolvedField::U32),
        Mask("river_mask") => world.derived.river_mask.as_ref().map(ResolvedField::Mask),

        // Sprint 1B baked scalar fields.
        ScalarBaked("precipitation") => world.baked.precipitation.as_ref().map(ResolvedField::F32),
        ScalarBaked("temperature") => world.baked.temperature.as_ref().map(ResolvedField::F32),
        ScalarBaked("soil_moisture") => world.baked.soil_moisture.as_ref().map(ResolvedField::F32),

        // Sprint 1B derived scalars.
        ScalarDerived("curvature") => world.derived.curvature.as_ref().map(ResolvedField::F32),
        ScalarDerived("dominant_biome_per_cell") => world
            .derived
            .dominant_biome_per_cell
            .as_ref()
            .map(ResolvedField::U32),
        ScalarDerived("hex_dominant_per_cell") => world
            .derived
            .hex_dominant_per_cell
            .as_ref()
            .map(ResolvedField::U32),

        // Sprint 2 / Sprint 3 DD6 derived scalars.
        // `coast_type` is `ScalarField2D<u8>` — same layout as `MaskField2D`
        // (which is a type alias for `ScalarField2D<u8>`), so the `Mask`
        // variant carries it without a new `ResolvedField` variant. The
        // descriptor's `ValueRange::Fixed(0.0, 5.0)` + `PaletteId::CoastType`
        // pair ensures discriminants 0..=4 sample the right entries and the
        // 0xFF sentinel clamps to transparent — all via the standard
        // `bake_overlay_to_rgba8` → `sample_f32` path, no per-palette
        // dispatch needed. Invariant #8: string key "coast_type" appears
        // only in this file.
        ScalarDerived("coast_type") => world.derived.coast_type.as_ref().map(ResolvedField::Mask),

        ScalarDerived("hex_slope_variance_per_cell") => world
            .derived
            .hex_slope_variance_per_cell
            .as_ref()
            .map(ResolvedField::F32),

        ScalarDerived("hex_accessibility_per_cell") => world
            .derived
            .hex_accessibility_per_cell
            .as_ref()
            .map(ResolvedField::F32),

        // River-crossing mask — uses the Mask variant (same as river_mask and
        // coast_type) because `MaskField2D` is `ScalarField2D<u8>`.
        Mask("hex_river_crossing_mask") => world
            .derived
            .hex_river_crossing_mask
            .as_ref()
            .map(ResolvedField::Mask),

        // Sprint 3 authoritative field: sediment thickness.
        // `ScalarAuthoritative` keys are resolved here, string confined to
        // this file per invariant #8. Returns `None` before Sprint 3 stages
        // have run (field stays `None` in Sprint 1A / 1B pipelines).
        ScalarAuthoritative("sediment") => world
            .authoritative
            .sediment
            .as_ref()
            .map(ResolvedField::F32),

        // Sprint 3 derived scalars.
        ScalarDerived("deposition_flux") => world
            .derived
            .deposition_flux
            .as_ref()
            .map(ResolvedField::F32),

        ScalarDerived("fog_water_input") => world
            .derived
            .fog_water_input
            .as_ref()
            .map(ResolvedField::F32),

        // Unknown / not-yet-populated sources silently return None so
        // the renderer skips rather than panicking on a missing field.
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sprint_2_5_hex_debug_resolve_scalar_source() {
        use island_core::{
            field::ScalarField2D,
            preset::{IslandAge, IslandArchetypePreset},
            seed::Seed,
            world::{Resolution, WorldState},
        };
        let preset = IslandArchetypePreset {
            name: "overlay_test".into(),
            island_radius: 0.5,
            max_relief: 0.5,
            volcanic_center_count: 1,
            island_age: IslandAge::Young,
            prevailing_wind_dir: 0.0,
            marine_moisture_strength: 0.5,
            sea_level: 0.3,
            erosion: Default::default(),
            climate: Default::default(),
        };
        let mut world = WorldState::new(Seed(0), preset, Resolution::new(16, 16));

        // Populate the broadcast caches that resolve_scalar_source reads.
        let mut var_field = ScalarField2D::<f32>::new(16, 16);
        var_field.data.fill(0.1);
        world.derived.hex_slope_variance_per_cell = Some(var_field);

        let mut acc_field = ScalarField2D::<f32>::new(16, 16);
        acc_field.data.fill(1.5);
        world.derived.hex_accessibility_per_cell = Some(acc_field);

        // Both keys must resolve to Some(ResolvedField::F32).
        let resolved = resolve_scalar_source(
            &world,
            OverlaySource::ScalarDerived("hex_slope_variance_per_cell"),
        );
        assert!(
            matches!(resolved, Some(ResolvedField::F32(_))),
            "hex_slope_variance_per_cell must resolve to F32 when populated"
        );

        let resolved = resolve_scalar_source(
            &world,
            OverlaySource::ScalarDerived("hex_accessibility_per_cell"),
        );
        assert!(
            matches!(resolved, Some(ResolvedField::F32(_))),
            "hex_accessibility_per_cell must resolve to F32 when populated"
        );
    }

    #[test]
    fn sprint_3_authoritative_sediment_resolves_to_f32() {
        use island_core::{
            field::ScalarField2D,
            preset::{IslandAge, IslandArchetypePreset},
            seed::Seed,
            world::{Resolution, WorldState},
        };
        let preset = IslandArchetypePreset {
            name: "sediment_resolve_test".into(),
            island_radius: 0.5,
            max_relief: 0.5,
            volcanic_center_count: 1,
            island_age: IslandAge::Young,
            prevailing_wind_dir: 0.0,
            marine_moisture_strength: 0.5,
            sea_level: 0.3,
            erosion: Default::default(),
            climate: Default::default(),
        };
        let mut world = WorldState::new(Seed(0), preset, Resolution::new(8, 8));

        // Sediment not populated yet — must resolve to None.
        let resolved =
            resolve_scalar_source(&world, OverlaySource::ScalarAuthoritative("sediment"));
        assert!(
            resolved.is_none(),
            "ScalarAuthoritative(\"sediment\") must return None when sediment is unpopulated"
        );

        // Populate sediment — must resolve to Some(F32).
        let mut sed = ScalarField2D::<f32>::new(8, 8);
        sed.data.fill(0.5);
        world.authoritative.sediment = Some(sed);

        let resolved =
            resolve_scalar_source(&world, OverlaySource::ScalarAuthoritative("sediment"));
        assert!(
            matches!(resolved, Some(ResolvedField::F32(_))),
            "ScalarAuthoritative(\"sediment\") must resolve to F32 when populated"
        );
    }
}
