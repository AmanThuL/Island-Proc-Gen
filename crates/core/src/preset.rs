//! Island archetype presets.
//!
//! [`IslandArchetypePreset`] is the primary configuration struct that Sprint 1A
//! `TopographyStage` and later pipeline stages consume.  The actual `.ron`
//! files and loading logic live in `crates/data/src/presets.rs`; this module
//! only provides the types so that `core` has no dependency on `data`.

// ─── physical scale constants ────────────────────────────────────────────────

/// Reference peak elevation in metres used to map `z_norm ∈ [0, 1]` to a
/// physical height, via `peak_m = MAX_RELIEF_REF_M * preset.max_relief`.
///
/// `2500 m` is the Réunion / Haleakalā order of magnitude (both roughly
/// 3 km peaks, but the proxy undershoots intentionally to keep the
/// lapse-driven temperature gradients conservative). The only v1 place
/// where a dimensional length unit appears — callers that need "peak
/// in metres" derive it from here rather than hardcoding local copies.
/// Sprint 3's physical calibration sprint will re-examine this value.
pub const MAX_RELIEF_REF_M: f32 = 2500.0;

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

// ─── PrecipitationVariant / ClimateParams ────────────────────────────────────

/// Sprint 3 DD4: which precipitation algorithm `PrecipitationStage` runs.
///
/// * [`PrecipitationVariant::V2Raymarch`] — Sprint 1B per-cell upwind
///   raymarch fallback. Preserved for Task 3.10 baseline regeneration:
///   `preset_override.climate.precipitation_variant = Some(V2Raymarch)`.
/// * [`PrecipitationVariant::V3Lfpm`] — Sprint 3 LFPM-inspired sequential
///   upwind sweep. Default for all new runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum PrecipitationVariant {
    /// Sprint 1B upwind raymarch fallback.
    V2Raymarch,
    /// Sprint 3 LFPM-inspired sequential sweep. Default.
    #[default]
    V3Lfpm,
}

/// Sprint 3 DD4: parameters for the LFPM-inspired precipitation model
/// (`PrecipitationVariant::V3Lfpm`).
///
/// Only the 4 new fields live here. `prevailing_wind_dir` and
/// `marine_moisture_strength` remain top-level on [`IslandArchetypePreset`]
/// for RON compatibility; a future sprint may consolidate them here.
///
/// All fields have `#[serde(default = "…")]` so existing `.ron` presets
/// that pre-date Sprint 3 parse without a `climate` field and receive the
/// locked v1 defaults automatically.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ClimateParams {
    /// Which precipitation algorithm `PrecipitationStage` runs.
    /// `V3Lfpm` is the default.
    #[serde(default)]
    pub precipitation_variant: PrecipitationVariant,

    /// Initial water-vapour mixing ratio at the upwind boundary.
    /// Dimensionless proxy; default `1.0`.
    #[serde(default = "default_q_0")]
    pub q_0: f32,

    /// Condensation time scale `τ_c` (explicit Euler with `CONDENSATION_DT`).
    /// Smaller values → faster condensation on windward slopes; default `0.15`.
    #[serde(default = "default_tau_c")]
    pub tau_c: f32,

    /// Generic fallout time scale `τ_f`.
    /// Smaller values → stronger rain shadow; default `0.60`.
    #[serde(default = "default_tau_f")]
    pub tau_f: f32,
}

// Serde defaults for `ClimateParams`. Values must stay bit-identical to
// `sim::climate::precipitation_v3::{Q_0_DEFAULT, TAU_C_DEFAULT,
// TAU_F_DEFAULT}` (the canonical home; reviewer S3). The duplication is
// structural: invariant #1 forbids `core` depending on `sim`, so these
// values cannot be imported. Sprint 3.8's ParamsPanel work surfaces this
// single-source-of-truth question again; resolution there (or in a later
// sprint) could be to relocate the constants into `core::preset` and
// re-export from `precipitation_v3`. Not worth the move for Task 3.4.
fn default_q_0() -> f32 {
    1.0
}
fn default_tau_c() -> f32 {
    0.15
}
fn default_tau_f() -> f32 {
    0.60
}

impl Default for ClimateParams {
    fn default() -> Self {
        Self {
            precipitation_variant: PrecipitationVariant::default(),
            q_0: default_q_0(),
            tau_c: default_tau_c(),
            tau_f: default_tau_f(),
        }
    }
}

/// Sprint 3 DD2: which Stream Power Incision Model variant to use inside
/// [`crate::pipeline::SimulationPipeline`].
///
/// * [`SpimVariant::Plain`] — Sprint 2 single-equation fallback
///   (`E_f = K · A^m · S^n`, no coupling to sediment thickness). Preserved
///   for baseline regeneration (Task 3.10's `pre_*` shots rely on
///   `preset_override.erosion.spim_variant = Some(Plain)`) and for Sprint 3
///   ablations against the old physics.
/// * [`SpimVariant::SpaceLite`] — Sprint 3 SPACE-lite dual equation
///   (default). Incises bedrock with a sediment-cover exponential shield
///   `exp(-hs / H*)` and separately entrains sediment proportional to
///   `min(hs, HS_ENTRAIN_MAX)`. Deposition is added in Task 3.3.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum SpimVariant {
    /// Sprint 2 single-equation SPIM (no `hs` coupling).
    Plain,
    /// Sprint 3 SPACE-lite dual-equation SPIM (default).
    #[default]
    SpaceLite,
}

// ─── ErosionParams ────────────────────────────────────────────────────────────

/// Sprint 2 DD1: parameters for the two-pass erosion loop
/// (`StreamPowerIncisionStage` + `HillslopeDiffusionStage`).
///
/// All fields have `#[serde(default = "…")]` so existing `.ron` preset files
/// that pre-date Sprint 2 parse without an `erosion` field and receive the
/// locked v1 defaults automatically.
///
/// ## Stream Power Incision Model (SPIM)
///
/// The erosion flux per cell is `Ef = K · A^m · S^n` (Whipple & Tucker 1999,
/// KP17 §3.1). In v1 we fix `n = 1.0` to avoid the KP17 pathological regime
/// where `m/n = 0.5` produces runaway incision on low-relief platforms; `m =
/// 0.35` is calibrated to Réunion-class basaltic shields. `K = 1e-3` is a
/// dimensionless proxy — Sprint 3 will replace it with a physically-grounded
/// value once the domain scaling is locked.
///
/// ## Hillslope diffusion
///
/// `hillslope_d` is the linear diffusivity `D` in `∂z/∂t = D · ∇²z`.
/// `n_diff_substep` sub-divides each outer erosion tick for stability
/// (CFL condition requires `Δt ≤ Δx² / (4D)`; with normalised cell
/// spacing `Δx = 1` and `D = 1e-3` four sub-steps suffice at v1 params).
///
/// ## Outer loop
///
/// `n_batch × n_inner` controls the total number of SPIM + diffusion
/// iterations executed by `ErosionOuterLoop` (Task 2.3): `n_batch` outer
/// ticks drive cache-invalidation and re-routing; inside each tick SPIM
/// runs `n_inner` times before the flow network is rebuilt.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ErosionParams {
    /// Stream power erodibility coefficient `K`. Dimensionless proxy in v1.
    /// Range: `(0, ∞)`; typical v1 value `1e-3`.
    #[serde(default = "default_spim_k")]
    pub spim_k: f32,

    /// Drainage area exponent `m`. Fixed at `0.35` in v1 (KP17 §3.1 avoids
    /// the `m/n = 0.5` pathological regime). Range: `(0, 1]`.
    #[serde(default = "default_spim_m")]
    pub spim_m: f32,

    /// Slope exponent `n`. Locked at `1.0` in v1 (linear slope coupling).
    /// Range: `(0, ∞)`.
    #[serde(default = "default_spim_n")]
    pub spim_n: f32,

    /// Hillslope linear diffusivity `D` in `∂z/∂t = D · ∇²z`.
    /// Range: `(0, ∞)`; typical v1 value `1e-3`.
    #[serde(default = "default_hillslope_d")]
    pub hillslope_d: f32,

    /// Number of sub-steps used inside each diffusion tick for CFL stability.
    /// With `D = 1e-3` and normalised cell spacing, `4` sub-steps suffice.
    #[serde(default = "default_n_diff_substep")]
    pub n_diff_substep: u32,

    /// Number of outer erosion batches (flow-network rebuilds). Each batch
    /// runs `n_inner` SPIM iterations before routing is recomputed.
    #[serde(default = "default_n_batch")]
    pub n_batch: u32,

    /// Number of SPIM iterations per outer batch before flow-network rebuild.
    #[serde(default = "default_n_inner")]
    pub n_inner: u32,

    /// Sprint 3 DD2: SPACE-lite bedrock erodibility `K_bed`. Used in the
    /// `SpimVariant::SpaceLite` branch; ignored under `SpimVariant::Plain`.
    /// Dimensionless proxy in v1; typical value `5e-3`.
    #[serde(default = "default_space_k_bed")]
    pub space_k_bed: f32,

    /// Sprint 3 DD2: SPACE-lite sediment entrainability `K_sed`. Larger
    /// `K_sed` ⇒ faster erosion of the sediment layer. Dimensionless proxy
    /// in v1; typical value `1.5e-2`.
    #[serde(default = "default_space_k_sed")]
    pub space_k_sed: f32,

    /// Sprint 3 DD2: cover-thickness `H*` in the bedrock shielding term
    /// `exp(-hs / H*)`. Controls how quickly bedrock incision decays as
    /// sediment thickens; typical value `0.05` (in normalised height units).
    #[serde(default = "default_h_star")]
    pub h_star: f32,

    /// Sprint 3 DD2: which SPIM variant drives the inner erosion step.
    /// Defaults to [`SpimVariant::SpaceLite`]; Sprint 2 `.ron` presets
    /// without this field deserialize to the default via `#[serde(default)]`.
    #[serde(default)]
    pub spim_variant: SpimVariant,
}

fn default_spim_k() -> f32 {
    // Calibrated for Sprint 2.6 Follow-up B. v1's 1e-3 produced only
    // ~0.16–2 % max_z drop across presets (far below the sprint doc DD1
    // "~18 %" projection). Empirical Pareto search on 128² showed 2e-3 was
    // the next safe step — but that fails on 64² (small-grid tests trip the
    // 5 % sea-crossing invariant because absolute sea-cell counts ≈ 30 on
    // volcanic_single synthetic 64² out of ~600 land cells). 1.5e-3 is
    // the largest K that is safe across all grid sizes tested (64²/128²/
    // 256²) for the three stock presets.
    1.5e-3
}
fn default_spim_m() -> f32 {
    0.35
}
fn default_spim_n() -> f32 {
    1.0
}
fn default_hillslope_d() -> f32 {
    1.0e-3
}
fn default_n_diff_substep() -> u32 {
    4
}
fn default_n_batch() -> u32 {
    10
}
fn default_n_inner() -> u32 {
    10
}
fn default_space_k_bed() -> f32 {
    // Sprint 3 DD2 locked constant: SPACE_K_BED_DEFAULT. See
    // `crates/sim/src/geomorph/sediment.rs` for the canonical const.
    5.0e-3
}
fn default_space_k_sed() -> f32 {
    // Sprint 3 DD2 locked constant: SPACE_K_SED_DEFAULT.
    1.5e-2
}
fn default_h_star() -> f32 {
    // Sprint 3 DD2 locked constant: H_STAR.
    0.05
}

impl Default for ErosionParams {
    fn default() -> Self {
        Self {
            spim_k: default_spim_k(),
            spim_m: default_spim_m(),
            spim_n: default_spim_n(),
            hillslope_d: default_hillslope_d(),
            n_diff_substep: default_n_diff_substep(),
            n_batch: default_n_batch(),
            n_inner: default_n_inner(),
            space_k_bed: default_space_k_bed(),
            space_k_sed: default_space_k_sed(),
            h_star: default_h_star(),
            spim_variant: SpimVariant::default(),
        }
    }
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

    /// Sprint 2 DD1: erosion model parameters. All fields have locked v1
    /// defaults so pre-Sprint-2 `.ron` files parse without an `erosion` key.
    #[serde(default)]
    pub erosion: ErosionParams,

    /// Sprint 3 DD4: precipitation model variant + LFPM tuning parameters.
    /// Pre-Sprint-3 `.ron` files without a `climate` key deserialize to
    /// [`ClimateParams::default()`] (`V3Lfpm` + locked defaults).
    #[serde(default)]
    pub climate: ClimateParams,
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
            erosion: ErosionParams::default(),
            climate: ClimateParams::default(),
        }
    }

    // 1. full preset RON serde round-trip (includes erosion field)
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

    // 3. ErosionParams default values match the locked constants.
    #[test]
    fn erosion_params_defaults_match_locked_constants() {
        let ep = ErosionParams::default();
        assert_eq!(ep.spim_k, 1.5e-3, "spim_k");
        assert_eq!(ep.spim_m, 0.35, "spim_m");
        assert_eq!(ep.spim_n, 1.0, "spim_n");
        assert_eq!(ep.hillslope_d, 1.0e-3, "hillslope_d");
        assert_eq!(ep.n_diff_substep, 4, "n_diff_substep");
        assert_eq!(ep.n_batch, 10, "n_batch");
        assert_eq!(ep.n_inner, 10, "n_inner");
        // Sprint 3 DD2 SPACE-lite defaults.
        assert_eq!(ep.space_k_bed, 5.0e-3, "space_k_bed");
        assert_eq!(ep.space_k_sed, 1.5e-2, "space_k_sed");
        assert_eq!(ep.h_star, 0.05, "h_star");
        assert_eq!(ep.spim_variant, SpimVariant::SpaceLite, "spim_variant");
    }

    // 4. A RON string without an `erosion` field deserialises with ErosionParams::default().
    //    This proves existing pre-Sprint-2 .ron presets parse unchanged.
    #[test]
    fn island_archetype_without_erosion_field_parses_via_serde_default() {
        let ron_str = r#"IslandArchetypePreset(
            name: "no_erosion",
            island_radius: 0.55,
            max_relief: 0.85,
            volcanic_center_count: 1,
            island_age: Young,
            prevailing_wind_dir: 1.5708,
            marine_moisture_strength: 0.75,
            sea_level: 0.30,
        )"#;
        let preset: IslandArchetypePreset =
            ron::from_str(ron_str).expect("deserialize without erosion field");
        assert_eq!(
            preset.erosion,
            ErosionParams::default(),
            "missing erosion field must produce default ErosionParams"
        );
    }

    // 5. Preset with explicit erosion overrides round-trips correctly.
    #[test]
    fn island_archetype_with_erosion_overrides_roundtrip() {
        let original = IslandArchetypePreset {
            name: "custom_erosion".to_string(),
            island_radius: 0.5,
            max_relief: 0.7,
            volcanic_center_count: 2,
            island_age: IslandAge::Mature,
            prevailing_wind_dir: 0.0,
            marine_moisture_strength: 0.6,
            sea_level: 0.25,
            erosion: ErosionParams {
                spim_k: 2.0e-3,
                spim_m: 0.4,
                spim_n: 1.0,
                hillslope_d: 5.0e-4,
                n_diff_substep: 8,
                n_batch: 5,
                n_inner: 20,
                space_k_bed: 6.0e-3,
                space_k_sed: 2.0e-2,
                h_star: 0.04,
                spim_variant: SpimVariant::Plain,
            },
            climate: ClimateParams::default(),
        };
        let serialized = ron::to_string(&original).expect("serialize failed");
        let deserialized: IslandArchetypePreset =
            ron::from_str(&serialized).expect("deserialize failed");
        assert_eq!(original, deserialized);
    }

    // 6. A RON string with a partially-specified `erosion` field falls back to
    //    per-field defaults for the missing keys. Proves `#[serde(default =
    //    "…")]` on each ErosionParams field does its job when the erosion
    //    block exists but is incomplete — future preset RON files can
    //    override only the keys they care about.
    #[test]
    fn island_archetype_with_partial_erosion_fills_per_field_defaults() {
        let ron_str = r#"IslandArchetypePreset(
            name: "partial_erosion",
            island_radius: 0.55,
            max_relief: 0.85,
            volcanic_center_count: 1,
            island_age: Young,
            prevailing_wind_dir: 1.5708,
            marine_moisture_strength: 0.75,
            sea_level: 0.30,
            erosion: (
                spim_k: 5.0e-3,
                n_batch: 3,
            ),
        )"#;
        let preset: IslandArchetypePreset =
            ron::from_str(ron_str).expect("deserialize with partial erosion");
        // Overridden fields present.
        assert_eq!(preset.erosion.spim_k, 5.0e-3);
        assert_eq!(preset.erosion.n_batch, 3);
        // Unspecified fields fall back to per-field default fns.
        assert_eq!(preset.erosion.spim_m, 0.35);
        assert_eq!(preset.erosion.spim_n, 1.0);
        assert_eq!(preset.erosion.hillslope_d, 1.0e-3);
        assert_eq!(preset.erosion.n_diff_substep, 4);
        assert_eq!(preset.erosion.n_inner, 10);
        // Sprint 3 DD2 SPACE-lite fields: missing → defaults.
        assert_eq!(preset.erosion.space_k_bed, 5.0e-3);
        assert_eq!(preset.erosion.space_k_sed, 1.5e-2);
        assert_eq!(preset.erosion.h_star, 0.05);
        assert_eq!(preset.erosion.spim_variant, SpimVariant::SpaceLite);
    }

    // 7. Sprint 3 DD2: a Sprint-2-style RON preset (no `spim_variant`, no
    //    `space_k_*`, no `h_star`) deserialises to SPACE-lite defaults.
    //    Proves `#[serde(default)]` wiring for every new Sprint 3 field.
    #[test]
    fn spim_variant_deserializes_from_legacy_ron() {
        // Sprint 2-vintage preset: only the Sprint 2 erosion keys are set.
        let ron_str = r#"IslandArchetypePreset(
            name: "legacy_sprint_2",
            island_radius: 0.55,
            max_relief: 0.85,
            volcanic_center_count: 1,
            island_age: Young,
            prevailing_wind_dir: 0.0,
            marine_moisture_strength: 0.75,
            sea_level: 0.30,
            erosion: (
                spim_k: 1.5e-3,
                spim_m: 0.35,
                spim_n: 1.0,
                hillslope_d: 1.0e-3,
                n_diff_substep: 4,
                n_batch: 10,
                n_inner: 10,
            ),
        )"#;
        let preset: IslandArchetypePreset =
            ron::from_str(ron_str).expect("Sprint 2 RON must deserialize under Sprint 3 binary");
        // Legacy Sprint 2 fields round-trip unchanged.
        assert_eq!(preset.erosion.spim_k, 1.5e-3);
        assert_eq!(preset.erosion.spim_m, 0.35);
        assert_eq!(preset.erosion.spim_n, 1.0);
        // New Sprint 3 fields fall back to SPACE-lite defaults.
        assert_eq!(preset.erosion.space_k_bed, 5.0e-3);
        assert_eq!(preset.erosion.space_k_sed, 1.5e-2);
        assert_eq!(preset.erosion.h_star, 0.05);
        assert_eq!(preset.erosion.spim_variant, SpimVariant::SpaceLite);
    }

    // 8. Sprint 3 DD4: ClimateParams defaults match the locked constants.
    #[test]
    fn climate_params_defaults_match_locked_constants() {
        let cp = ClimateParams::default();
        assert_eq!(cp.precipitation_variant, PrecipitationVariant::V3Lfpm);
        assert_eq!(cp.q_0, 1.0, "q_0");
        assert_eq!(cp.tau_c, 0.15, "tau_c");
        assert_eq!(cp.tau_f, 0.60, "tau_f");
    }

    // 9. Sprint 3 DD4: a Sprint-2-vintage RON preset (no `climate:` field)
    //    deserialises to ClimateParams::default() with V3Lfpm and all locked defaults.
    #[test]
    fn climate_params_deserializes_from_legacy_ron() {
        let ron_str = r#"IslandArchetypePreset(
            name: "legacy_sprint_2",
            island_radius: 0.55,
            max_relief: 0.85,
            volcanic_center_count: 1,
            island_age: Young,
            prevailing_wind_dir: 0.0,
            marine_moisture_strength: 0.75,
            sea_level: 0.30,
            erosion: (
                spim_k: 1.5e-3,
                spim_m: 0.35,
                spim_n: 1.0,
                hillslope_d: 1.0e-3,
                n_diff_substep: 4,
                n_batch: 10,
                n_inner: 10,
            ),
        )"#;
        let preset: IslandArchetypePreset =
            ron::from_str(ron_str).expect("Sprint 2 RON must deserialize under Sprint 3 binary");
        // Missing `climate` field must produce ClimateParams::default().
        assert_eq!(
            preset.climate,
            ClimateParams::default(),
            "missing climate field must produce ClimateParams::default()"
        );
        assert_eq!(
            preset.climate.precipitation_variant,
            PrecipitationVariant::V3Lfpm
        );
        assert_eq!(preset.climate.q_0, 1.0);
        assert_eq!(preset.climate.tau_c, 0.15);
        assert_eq!(preset.climate.tau_f, 0.60);
    }

    // 10. Sprint 3 DD4: V3Lfpm is the default variant for new presets.
    #[test]
    fn v3_default_is_selected_when_preset_missing_climate_section() {
        let ron_str = r#"IslandArchetypePreset(
            name: "no_climate",
            island_radius: 0.5,
            max_relief: 0.8,
            volcanic_center_count: 1,
            island_age: Young,
            prevailing_wind_dir: 1.5708,
            marine_moisture_strength: 0.75,
            sea_level: 0.30,
        )"#;
        let preset: IslandArchetypePreset =
            ron::from_str(ron_str).expect("preset without climate field must parse");
        assert_eq!(
            preset.climate.precipitation_variant,
            PrecipitationVariant::V3Lfpm,
            "V3Lfpm must be default when climate section is absent"
        );
    }
}
