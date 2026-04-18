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
}

fn default_spim_k() -> f32 {
    1.0e-3
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
        assert_eq!(ep.spim_k, 1.0e-3, "spim_k");
        assert_eq!(ep.spim_m, 0.35, "spim_m");
        assert_eq!(ep.spim_n, 1.0, "spim_n");
        assert_eq!(ep.hillslope_d, 1.0e-3, "hillslope_d");
        assert_eq!(ep.n_diff_substep, 4, "n_diff_substep");
        assert_eq!(ep.n_batch, 10, "n_batch");
        assert_eq!(ep.n_inner, 10, "n_inner");
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
            },
        };
        let serialized = ron::to_string(&original).expect("serialize failed");
        let deserialized: IslandArchetypePreset =
            ron::from_str(&serialized).expect("deserialize failed");
        assert_eq!(original, deserialized);
    }
}
