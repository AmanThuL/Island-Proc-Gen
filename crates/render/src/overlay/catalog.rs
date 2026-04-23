//! Overlay catalog — 20-descriptor Sprint 3 default registry.
//!
//! **Guardrail (CLAUDE.md / AGENTS.md invariant #8):** this file must NOT
//! contain raw string field keys. All overlay sources are constructed via
//! [`source_for`] using [`SourceKey`] enum handles; the string-to-field
//! mapping lives exclusively in `overlay/resolve.rs`.

use crate::palette::PaletteId;

use super::{
    OverlayDescriptor, OverlayRegistry,
    range::ValueRange,
    resolve::{SourceKey, source_for},
};

impl OverlayRegistry {
    /// Return the Sprint 3 overlay registry — 6 Sprint 1A geomorph
    /// overlays + 6 Sprint 1B climate / ecology / hex overlays + 1 Sprint 2
    /// coastal geomorphology overlay + 2 Sprint 2.5 hex debug overlays +
    /// 1 Sprint 2.5.C river crossing mask overlay + 4 Sprint 3 sediment /
    /// climate overlays, total 20.
    ///
    /// `final_elevation` reads `z_filled` (not `height`) —
    /// `authoritative.height` stores `z_raw` pre-pit-fill; the render
    /// path and flow routing both see `z_filled`. Only
    /// `final_elevation` is visible by default; everything else is
    /// hidden so the user can toggle overlays without fighting alpha
    /// stacking.
    pub fn sprint_3_defaults() -> Self {
        Self {
            entries: vec![
                // ── Sprint 1A (geomorph + hydro) ──────────────────────
                OverlayDescriptor {
                    id: "initial_uplift",
                    label: "Initial uplift",
                    source: source_for(SourceKey::InitialUplift),
                    palette: PaletteId::Grayscale,
                    value_range: ValueRange::Auto,
                    visible: false,
                    alpha: 0.6,
                },
                OverlayDescriptor {
                    id: "final_elevation",
                    label: "Final elevation",
                    source: source_for(SourceKey::ZFilled),
                    palette: PaletteId::TerrainHeight,
                    value_range: ValueRange::Auto,
                    visible: true,
                    alpha: 0.6,
                },
                OverlayDescriptor {
                    id: "slope",
                    label: "Slope",
                    source: source_for(SourceKey::Slope),
                    palette: PaletteId::Viridis,
                    value_range: ValueRange::Auto,
                    visible: false,
                    alpha: 0.6,
                },
                OverlayDescriptor {
                    id: "flow_accumulation",
                    label: "Flow accumulation",
                    source: source_for(SourceKey::Accumulation),
                    palette: PaletteId::Turbo,
                    // LogCompressedClampPercentile(0.99): Sprint 2.5.H audit
                    // showed P90/max = 0.023 on volcanic_twin 128² — 90 % of
                    // cells cluster at raw values 1–11 out of a max of 484.
                    // Pure LogCompressed assigns the top palette band to those
                    // outlier main-channel cells, washing out the rest. Clamping
                    // at P99 (raw ≈ 45) compresses the scale to the 99th
                    // percentile and maps the top 1 % of cells to t = 1.0.
                    value_range: ValueRange::LogCompressedClampPercentile(0.99),
                    visible: false,
                    alpha: 0.6,
                },
                OverlayDescriptor {
                    id: "basin_partition",
                    label: "Basin partition",
                    source: source_for(SourceKey::BasinId),
                    palette: PaletteId::Categorical,
                    value_range: ValueRange::Auto,
                    visible: false,
                    alpha: 0.6,
                },
                OverlayDescriptor {
                    id: "river_network",
                    label: "River network",
                    source: source_for(SourceKey::RiverMask),
                    palette: PaletteId::BinaryBlue,
                    value_range: ValueRange::Fixed(0.0, 1.0),
                    visible: false,
                    alpha: 0.6,
                },
                // ── Sprint 1B (climate + ecology + hex) ───────────────
                OverlayDescriptor {
                    id: "precipitation",
                    label: "Precipitation",
                    source: source_for(SourceKey::Precipitation),
                    palette: PaletteId::Viridis,
                    value_range: ValueRange::Fixed(0.0, 1.0),
                    visible: false,
                    alpha: 0.6,
                },
                OverlayDescriptor {
                    id: "temperature",
                    label: "Temperature",
                    source: source_for(SourceKey::Temperature),
                    palette: PaletteId::Turbo,
                    value_range: ValueRange::Auto,
                    visible: false,
                    alpha: 0.6,
                },
                OverlayDescriptor {
                    id: "soil_moisture",
                    label: "Soil moisture",
                    source: source_for(SourceKey::SoilMoisture),
                    palette: PaletteId::Viridis,
                    value_range: ValueRange::Fixed(0.0, 1.0),
                    visible: false,
                    alpha: 0.6,
                },
                OverlayDescriptor {
                    id: "dominant_biome",
                    label: "Dominant biome",
                    source: source_for(SourceKey::DominantBiomePerCell),
                    palette: PaletteId::Categorical,
                    value_range: ValueRange::Fixed(0.0, 7.0),
                    visible: false,
                    alpha: 0.6,
                },
                OverlayDescriptor {
                    id: "curvature",
                    label: "Curvature",
                    source: source_for(SourceKey::Curvature),
                    palette: PaletteId::Turbo,
                    value_range: ValueRange::Auto,
                    visible: false,
                    alpha: 0.6,
                },
                OverlayDescriptor {
                    id: "hex_aggregated",
                    label: "Hex aggregated",
                    source: source_for(SourceKey::HexDominantPerCell),
                    palette: PaletteId::Categorical,
                    value_range: ValueRange::Fixed(0.0, 7.0),
                    visible: false,
                    alpha: 0.6,
                },
                // ── Sprint 2 / Sprint 3 DD6 (coastal geomorphology) ────
                // SourceKey::CoastType handle maps to "coast_type" in resolve.rs
                // (invariant #8). Sprint 3 widened the range from `Fixed(0.0, 4.0)`
                // (4 bins) to `Fixed(0.0, 5.0)` (5 bins) to admit LavaDelta
                // (disc=4). With the range set to 5, discriminants 0..=4 normalise
                // to `t = disc / 5`, yielding `idx = (t * 5.0) as usize = disc`
                // exactly in `PaletteId::CoastType`. Unknown sentinel (0xFF) clamps
                // to `t = 1.0` → `idx = 5` → transparent, which is the intended
                // non-coast behaviour.
                OverlayDescriptor {
                    id: "coast_type",
                    label: "Coast type",
                    source: source_for(SourceKey::CoastType),
                    palette: PaletteId::CoastType,
                    value_range: ValueRange::Fixed(0.0, 5.0),
                    visible: false,
                    alpha: 0.6,
                },
                // ── Sprint 2.5.B (hex slope variance) ─────────────────
                // `ValueRange::Auto`: variance has no fixed upper bound.
                OverlayDescriptor {
                    id: "hex_projection_error",
                    label: "Hex projection error",
                    source: source_for(SourceKey::HexSlopeVariancePerCell),
                    palette: PaletteId::Viridis,
                    value_range: ValueRange::Auto,
                    visible: false,
                    alpha: 0.6,
                },
                // ── Sprint 2.5.D (hex accessibility cost) ─────────────
                OverlayDescriptor {
                    id: "hex_accessibility",
                    label: "Hex accessibility cost",
                    source: source_for(SourceKey::HexAccessibilityPerCell),
                    palette: PaletteId::Viridis,
                    value_range: ValueRange::Auto,
                    visible: false,
                    alpha: 0.6,
                },
                // ── hex river crossing mask ────────────────────────────
                // Pre-rasterised Bresenham line from entry_edge midpoint to
                // exit_edge midpoint per river-bearing hex. BinaryBlue gives
                // transparent on 0-cells and the river-blue colour on 1-cells,
                // matching the existing river_network overlay style.
                // Fixed(0.0, 1.0): mask values are exactly 0 or 1.
                OverlayDescriptor {
                    id: "hex_river_crossing",
                    label: "Hex river crossing",
                    source: source_for(SourceKey::HexRiverCrossingMask),
                    palette: PaletteId::BinaryBlue,
                    value_range: ValueRange::Fixed(0.0, 1.0),
                    visible: false,
                    alpha: 0.6,
                },
                // ── Sprint 3 sediment / climate overlays ──────────────
                // `sediment_thickness` reads the authoritative sediment field
                // (ScalarAuthoritative). Fixed(0.0, 1.0) matches the
                // normalised [0, 1] sediment range written by Sprint 3.3
                // DepositionStage.
                OverlayDescriptor {
                    id: "sediment_thickness",
                    label: "Sediment thickness",
                    source: source_for(SourceKey::Sediment),
                    palette: PaletteId::Turbo,
                    value_range: ValueRange::Fixed(0.0, 1.0),
                    visible: false,
                    alpha: 0.6,
                },
                // `deposition_flux` is `derived.deposition_flux`, the per-cell
                // D[p] written after the last SPACE-lite inner step.
                // LogCompressedClampPercentile(0.99): like flow_accumulation,
                // deposition has a long tail driven by a few delta cells.
                OverlayDescriptor {
                    id: "deposition_flux",
                    label: "Deposition flux",
                    source: source_for(SourceKey::DepositionFlux),
                    palette: PaletteId::Viridis,
                    value_range: ValueRange::LogCompressedClampPercentile(0.99),
                    visible: false,
                    alpha: 0.6,
                },
                // `fog_water_input` is the per-cell fog-derived moisture
                // contribution from FogStage (Task 3.5). Auto range: the
                // absolute magnitude depends on FOG_WATER_GAIN and the
                // fog_likelihood distribution.
                OverlayDescriptor {
                    id: "fog_water_input",
                    label: "Fog water input",
                    source: source_for(SourceKey::FogWaterInput),
                    palette: PaletteId::Blues,
                    value_range: ValueRange::Auto,
                    visible: false,
                    alpha: 0.6,
                },
                // `lava_delta_mask` reuses the `coast_type` field with the
                // LavaDeltaMask palette, which renders only discriminant 4
                // opaque — all other coast types and the 0xFF sentinel are
                // transparent. Paired with Fixed(0.0, 5.0) so the normalisation
                // math aligns with the 5-bin CoastType encoding.
                OverlayDescriptor {
                    id: "lava_delta_mask",
                    label: "Lava delta mask",
                    source: source_for(SourceKey::CoastType),
                    palette: PaletteId::LavaDeltaMask,
                    value_range: ValueRange::Fixed(0.0, 5.0),
                    visible: false,
                    alpha: 0.6,
                },
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overlay::{
        OverlayRegistry,
        resolve::{OverlaySource, SourceKey, source_for},
    };

    #[test]
    fn registry_has_20_sprint_3_defaults() {
        assert_eq!(OverlayRegistry::sprint_3_defaults().all().len(), 20);
    }

    #[test]
    fn overlay_descriptor_alpha_default_is_0_6() {
        let reg = OverlayRegistry::sprint_3_defaults();
        // Every descriptor in sprint_3_defaults uses alpha = 0.6.
        for d in reg.all() {
            assert!(
                (d.alpha - 0.6).abs() < f32::EPSILON,
                "descriptor '{}' alpha must be 0.6, got {}",
                d.id,
                d.alpha
            );
        }
    }

    #[test]
    fn coast_type_descriptor_is_correct() {
        let reg = OverlayRegistry::sprint_3_defaults();
        let d = reg
            .by_id("coast_type")
            .expect("coast_type overlay must exist");
        assert_eq!(
            d.source,
            source_for(SourceKey::CoastType),
            "coast_type source must be source_for(SourceKey::CoastType)"
        );
        assert_eq!(
            d.palette,
            PaletteId::CoastType,
            "coast_type palette must be PaletteId::CoastType"
        );
        // Sprint 3 DD6: value range is now `Fixed(0.0, 5.0)` (was 4.0 in
        // Sprint 2) to admit the LavaDelta discriminant (4).
        assert_eq!(
            d.value_range,
            ValueRange::Fixed(0.0, 5.0),
            "coast_type value_range must be Fixed(0.0, 5.0) after Sprint 3 DD6 LavaDelta expansion"
        );
        assert!(!d.visible, "coast_type must default to hidden");
    }

    #[test]
    fn by_id_queries_all_sprint_1a_defaults() {
        let reg = OverlayRegistry::sprint_3_defaults();
        assert!(reg.by_id("initial_uplift").is_some());
        assert!(reg.by_id("final_elevation").is_some());
        assert!(reg.by_id("slope").is_some());
        assert!(reg.by_id("flow_accumulation").is_some());
        assert!(reg.by_id("basin_partition").is_some());
        assert!(reg.by_id("river_network").is_some());
    }

    #[test]
    fn by_id_queries_all_sprint_1b_defaults() {
        let reg = OverlayRegistry::sprint_3_defaults();
        assert!(reg.by_id("precipitation").is_some());
        assert!(reg.by_id("temperature").is_some());
        assert!(reg.by_id("soil_moisture").is_some());
        assert!(reg.by_id("dominant_biome").is_some());
        assert!(reg.by_id("curvature").is_some());
        assert!(reg.by_id("hex_aggregated").is_some());
    }

    #[test]
    fn by_id_unknown_returns_none() {
        let reg = OverlayRegistry::sprint_3_defaults();
        assert!(reg.by_id("nope").is_none());
    }

    #[test]
    fn set_visibility_changes_flag() {
        let mut reg = OverlayRegistry::sprint_3_defaults();
        assert!(!reg.by_id("initial_uplift").unwrap().visible);
        reg.set_visibility("initial_uplift", true);
        assert!(reg.by_id("initial_uplift").unwrap().visible);
    }

    // Sprint 1A defaults: only final_elevation is visible → count == 1.
    #[test]
    fn visible_entries_filters() {
        let reg = OverlayRegistry::sprint_3_defaults();
        assert_eq!(reg.visible_entries().count(), 1);
    }

    #[test]
    fn source_field_keys_match_sprint_1a_plan() {
        let reg = OverlayRegistry::sprint_3_defaults();

        assert_eq!(
            reg.by_id("initial_uplift").unwrap().source,
            source_for(SourceKey::InitialUplift),
        );
        // §7 acceptance criterion: z_filled, NOT height.
        assert_eq!(
            reg.by_id("final_elevation").unwrap().source,
            source_for(SourceKey::ZFilled),
        );
        assert_eq!(
            reg.by_id("slope").unwrap().source,
            source_for(SourceKey::Slope),
        );
        assert_eq!(
            reg.by_id("flow_accumulation").unwrap().source,
            source_for(SourceKey::Accumulation),
        );
        assert_eq!(
            reg.by_id("basin_partition").unwrap().source,
            source_for(SourceKey::BasinId),
        );
        assert_eq!(
            reg.by_id("river_network").unwrap().source,
            source_for(SourceKey::RiverMask),
        );
    }

    // Dedicated guard: future refactorers must not silently revert to height.
    #[test]
    fn final_elevation_not_authoritative_height() {
        let reg = OverlayRegistry::sprint_3_defaults();
        let d = reg.by_id("final_elevation").unwrap();
        // Must not be any ScalarAuthoritative source.
        assert!(
            !matches!(d.source, OverlaySource::ScalarAuthoritative(_)),
            "final_elevation must not use ScalarAuthoritative"
        );
        assert_eq!(d.source, source_for(SourceKey::ZFilled));
    }

    #[test]
    fn flow_accumulation_uses_log_compressed_clamp_percentile() {
        let reg = OverlayRegistry::sprint_3_defaults();
        assert_eq!(
            reg.by_id("flow_accumulation").unwrap().value_range,
            ValueRange::LogCompressedClampPercentile(0.99),
            "flow_accumulation must use P99-clamped log compression (Sprint 2.5.H fix)"
        );
    }

    #[test]
    fn river_network_uses_binary_blue() {
        let reg = OverlayRegistry::sprint_3_defaults();
        let d = reg.by_id("river_network").unwrap();
        assert_eq!(d.palette, PaletteId::BinaryBlue);
        assert_eq!(d.source, source_for(SourceKey::RiverMask));
    }

    #[test]
    fn sprint_2_5_hex_debug_overlay_descriptors_exist() {
        let reg = OverlayRegistry::sprint_3_defaults();

        let d = reg
            .by_id("hex_projection_error")
            .expect("hex_projection_error overlay must exist");
        assert_eq!(
            d.source,
            source_for(SourceKey::HexSlopeVariancePerCell),
            "hex_projection_error must source HexSlopeVariancePerCell"
        );
        assert_eq!(d.palette, PaletteId::Viridis);
        assert_eq!(d.value_range, ValueRange::Auto);
        assert!(!d.visible);

        let d = reg
            .by_id("hex_accessibility")
            .expect("hex_accessibility overlay must exist");
        assert_eq!(
            d.source,
            source_for(SourceKey::HexAccessibilityPerCell),
            "hex_accessibility must source HexAccessibilityPerCell"
        );
        assert_eq!(d.palette, PaletteId::Viridis);
        assert_eq!(d.value_range, ValueRange::Auto);
        assert!(!d.visible);
    }

    // ── Sprint 3 Task 3.7: new overlay descriptors ────────────────────────────

    #[test]
    fn sprint_3_defaults_includes_sediment_thickness_descriptor() {
        let reg = OverlayRegistry::sprint_3_defaults();
        let d = reg
            .by_id("sediment_thickness")
            .expect("sediment_thickness overlay must exist");
        assert_eq!(
            d.source,
            source_for(SourceKey::Sediment),
            "sediment_thickness must source SourceKey::Sediment"
        );
        assert_eq!(
            d.palette,
            PaletteId::Turbo,
            "sediment_thickness palette must be Turbo"
        );
        assert_eq!(
            d.value_range,
            ValueRange::Fixed(0.0, 1.0),
            "sediment_thickness value_range must be Fixed(0.0, 1.0)"
        );
        assert!(!d.visible, "sediment_thickness must default to hidden");
    }

    #[test]
    fn sprint_3_defaults_includes_deposition_flux_descriptor() {
        let reg = OverlayRegistry::sprint_3_defaults();
        let d = reg
            .by_id("deposition_flux")
            .expect("deposition_flux overlay must exist");
        assert_eq!(
            d.source,
            source_for(SourceKey::DepositionFlux),
            "deposition_flux must source SourceKey::DepositionFlux"
        );
        assert_eq!(
            d.palette,
            PaletteId::Viridis,
            "deposition_flux palette must be Viridis"
        );
        assert_eq!(
            d.value_range,
            ValueRange::LogCompressedClampPercentile(0.99),
            "deposition_flux value_range must be LogCompressedClampPercentile(0.99)"
        );
        assert!(!d.visible, "deposition_flux must default to hidden");
    }

    #[test]
    fn sprint_3_defaults_includes_fog_water_input_descriptor() {
        let reg = OverlayRegistry::sprint_3_defaults();
        let d = reg
            .by_id("fog_water_input")
            .expect("fog_water_input overlay must exist");
        assert_eq!(
            d.source,
            source_for(SourceKey::FogWaterInput),
            "fog_water_input must source SourceKey::FogWaterInput"
        );
        assert_eq!(
            d.palette,
            PaletteId::Blues,
            "fog_water_input palette must be Blues"
        );
        assert_eq!(
            d.value_range,
            ValueRange::Auto,
            "fog_water_input value_range must be Auto"
        );
        assert!(!d.visible, "fog_water_input must default to hidden");
    }

    #[test]
    fn sprint_3_defaults_includes_lava_delta_mask_descriptor() {
        let reg = OverlayRegistry::sprint_3_defaults();
        let d = reg
            .by_id("lava_delta_mask")
            .expect("lava_delta_mask overlay must exist");
        assert_eq!(
            d.source,
            source_for(SourceKey::CoastType),
            "lava_delta_mask must source SourceKey::CoastType"
        );
        assert_eq!(
            d.palette,
            PaletteId::LavaDeltaMask,
            "lava_delta_mask palette must be LavaDeltaMask"
        );
        assert_eq!(
            d.value_range,
            ValueRange::Fixed(0.0, 5.0),
            "lava_delta_mask value_range must be Fixed(0.0, 5.0)"
        );
        assert!(!d.visible, "lava_delta_mask must default to hidden");
    }

    #[test]
    fn lava_delta_mask_palette_renders_only_discriminant_4() {
        use crate::palette::sample_f32;
        // With ValueRange::Fixed(0.0, 5.0), t = disc / 5.0.
        for disc in 0u8..=3u8 {
            let t = disc as f32 / 5.0;
            let rgba = sample_f32(PaletteId::LavaDeltaMask, t);
            assert_eq!(
                rgba[3], 0.0,
                "discriminant {disc} must be transparent (alpha=0), got alpha={}",
                rgba[3]
            );
        }
        // Discriminant 4 (LavaDelta) must be opaque.
        let t4 = 4.0_f32 / 5.0;
        let rgba4 = sample_f32(PaletteId::LavaDeltaMask, t4);
        assert!(
            rgba4[3] > 0.5,
            "discriminant 4 (LavaDelta) must be opaque, got alpha={}",
            rgba4[3]
        );
        // Discriminant 5 (sentinel clamp to t=1.0) must be transparent.
        let rgba5 = sample_f32(PaletteId::LavaDeltaMask, 1.0);
        assert_eq!(
            rgba5[3], 0.0,
            "discriminant 5 (sentinel) must be transparent, got alpha={}",
            rgba5[3]
        );
    }

    #[test]
    fn sprint_3_defaults_preserves_sprint_2_5_overlays() {
        let reg = OverlayRegistry::sprint_3_defaults();
        // Verify all 16 pre-existing Sprint 2.5 overlays are still present.
        let expected_ids = [
            "initial_uplift",
            "final_elevation",
            "slope",
            "flow_accumulation",
            "basin_partition",
            "river_network",
            "precipitation",
            "temperature",
            "soil_moisture",
            "dominant_biome",
            "curvature",
            "hex_aggregated",
            "coast_type",
            "hex_projection_error",
            "hex_accessibility",
            "hex_river_crossing",
        ];
        for id in &expected_ids {
            assert!(
                reg.by_id(id).is_some(),
                "Sprint 2.5 overlay '{id}' must still be present in sprint_3_defaults"
            );
        }
    }
}
