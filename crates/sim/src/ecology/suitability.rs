//! Per-biome suitability functions for DD6.
//!
//! Every function returns a value in `[0, 1]`: higher values mean the
//! biome is better suited to the cell's (T, θ, z_norm, slope, fog,
//! river_proximity, coast_proximity) tuple. The shapes are "bell
//! curves on each axis multiplied together" — specifically,
//! `bell(x, mu, sigma) = exp(-((x - mu) / sigma)^2)` for continuous
//! preferences and `smoothstep(lo, hi, x)` for "wants above
//! threshold" axes.
//!
//! The parameter values below are picked to give plausible tropical-
//! volcanic-island biome distributions at v1 scale; Task 1B.9 will
//! expose a subset to the params panel for interactive tuning.
//!
//! ## Sprint 3.5.D DD6: CloudForest `f_t` envelope widening
//!
//! The CloudForest temperature bell was previously peaked at 15 °C with
//! `σ = 4.0`. Archetype mean temperatures are 19–24 °C, so the old bell
//! never rose above ~0.37 in the active temperature range, suppressing
//! CloudForest biome weight across all archetypes. Sprint 3.5.D raises
//! `CLOUD_FOREST_T_PEAK` to 18.0 and widens `CLOUD_FOREST_T_SIGMA` to
//! 6.0, pushing the bell into the archetype temperature range without
//! altering the bell formula structure or any other suitability gate.

use crate::climate::common::smoothstep;

/// Gaussian bell curve centred at `mu` with width `sigma`.
#[inline]
pub fn bell(x: f32, mu: f32, sigma: f32) -> f32 {
    let d = (x - mu) / sigma;
    (-(d * d)).exp()
}

/// Per-cell environment inputs passed to every suitability function.
/// Fields are in the same `[0, 1]` (or °C for temperature) conventions
/// used across the Sprint 1B pipeline.
#[derive(Debug, Clone, Copy)]
pub struct EnvSample {
    pub temperature_c: f32,
    pub soil_moisture: f32,
    pub z_norm: f32,
    pub slope: f32,
    pub fog_likelihood: f32,
    pub river_proximity: f32,
    pub coast_proximity: f32,
}

// ── suitability kernels (see DD6 §256 for the shape rationale) ────────────────

pub fn coastal_scrub(env: EnvSample) -> f32 {
    let f_z = bell(env.z_norm, 0.05, 0.08);
    let f_coast = smoothstep(0.15, 0.80, env.coast_proximity);
    let f_dry = smoothstep(0.10, 0.50, 1.0 - env.soil_moisture);
    f_z * f_coast * f_dry
}

pub fn lowland_forest(env: EnvSample) -> f32 {
    let f_t = bell(env.temperature_c, 24.0, 6.0);
    let f_theta = smoothstep(0.20, 0.60, env.soil_moisture);
    let f_z = bell(env.z_norm, 0.15, 0.15);
    f_t * f_theta * f_z
}

pub fn montane_wet_forest(env: EnvSample) -> f32 {
    let f_t = bell(env.temperature_c, 18.0, 4.0);
    let f_theta = smoothstep(0.25, 0.65, env.soil_moisture);
    let f_z = bell(env.z_norm, 0.4, 0.17);
    f_t * f_theta * f_z
}

/// Sprint 3 Task 3.1.C candidate A: fog sigma widened from 0.08 to 0.15.
/// The wider bell spreads the fog-dependence band across a broader
/// inversion layer so CloudForest suitability fires on more land cells.
/// Original DD5 target was 0.08 (concentrated inversion layer); the 0.15
/// value was chosen empirically to satisfy the G7 gate (CloudForest > 0 %
/// on ≥ 1 archetype).
pub(crate) const CLOUD_FOREST_SIGMA_FOG: f32 = 0.15;

/// Sprint 3 Task 3.1.C candidate A: peak weight increased from 0.30 to 0.40.
/// Raised alongside the soil_moisture coupling increase
/// (FOG_TO_SM_COUPLING 0.40 → 0.60) to amplify the direct fog suitability
/// signal when inversion-layer fog is high. Fog still feeds CloudForest
/// primarily through SoilMoistureStage's `fog_water_input` coupling; the
/// direct term is a secondary enhancer for high-fog cells.
pub(crate) const CLOUD_FOREST_FOG_PEAK_WEIGHT: f32 = 0.40;

/// Sprint 3.5.D DD6: CloudForest temperature bell peak.
///
/// Raised from 15.0 °C to 18.0 °C so the bell overlaps with archetype
/// mean temperatures (19–24 °C). The old peak at 15 °C produced a
/// maximum `f_t ≈ 0.37` at 19 °C, suppressing CloudForest across all
/// archetypes. At 18 °C the bell peaks inside the archetype range.
/// Value-locked by `cloud_forest_f_t_envelope_matches_sprint_3_5_lock`.
pub(crate) const CLOUD_FOREST_T_PEAK: f32 = 18.0;

/// Sprint 3.5.D DD6: CloudForest temperature bell width (sigma).
///
/// Widened from 4.0 °C to 6.0 °C alongside the peak shift so the bell
/// has meaningful coverage across the 19–24 °C archetype range without
/// a sharp cliff at the peak. The bell formula (`exp(-((T-T_PEAK)/T_SIGMA)^2)`)
/// is unchanged; only the two constants shift.
/// Value-locked by `cloud_forest_f_t_envelope_matches_sprint_3_5_lock`.
pub(crate) const CLOUD_FOREST_T_SIGMA: f32 = 6.0;

pub fn cloud_forest(env: EnvSample) -> f32 {
    let f_t = bell(env.temperature_c, CLOUD_FOREST_T_PEAK, CLOUD_FOREST_T_SIGMA);
    let f_theta = smoothstep(0.30, 0.75, env.soil_moisture);
    // z-bell is intentionally tighter than montane_wet_forest (0.15 vs 0.17)
    // to keep the two biomes from overlapping in elevation space.
    let f_z = bell(env.z_norm, 0.60, 0.15);
    // Sprint 3 Task 3.1.C: fog contributes via a Gaussian bell (sigma=0.15,
    // widened from DD5's 0.08) capped at CLOUD_FOREST_FOG_PEAK_WEIGHT (0.40,
    // raised from DD5's 0.30). Fog's main hydrology foothold is through
    // SoilMoistureStage's fog-drip coupling (FOG_TO_SM_COUPLING=0.60);
    // the direct fog suitability term is a secondary enhancer for cells
    // at the inversion layer.
    let f_fog = (1.0 - CLOUD_FOREST_FOG_PEAK_WEIGHT)
        + CLOUD_FOREST_FOG_PEAK_WEIGHT * bell(env.fog_likelihood, 1.0, CLOUD_FOREST_SIGMA_FOG);
    f_t * f_theta * f_z * f_fog
}

pub fn dry_shrub(env: EnvSample) -> f32 {
    let f_z = bell(env.z_norm, 0.15, 0.18);
    let f_dry = smoothstep(0.35, 0.85, 1.0 - env.soil_moisture);
    let f_warm = smoothstep(18.0, 24.0, env.temperature_c);
    f_z * f_dry * f_warm
}

pub fn grassland(env: EnvSample) -> f32 {
    let f_z = bell(env.z_norm, 0.3, 0.15);
    let f_mid = bell(env.soil_moisture, 0.40, 0.15);
    let f_warm = smoothstep(10.0, 22.0, env.temperature_c);
    f_z * f_mid * f_warm
}

pub fn bare_rock_lava(env: EnvSample) -> f32 {
    let f_high = smoothstep(0.65, 0.95, env.z_norm);
    let f_steep = smoothstep(0.3, 0.7, env.slope);
    let f_dry = smoothstep(0.1, 0.4, 1.0 - env.soil_moisture);
    (f_high + f_steep).min(1.0) * f_dry
}

pub fn riparian_vegetation(env: EnvSample) -> f32 {
    let f_river = smoothstep(0.4, 0.95, env.river_proximity);
    let f_not_alpine = 1.0 - smoothstep(0.55, 0.8, env.z_norm);
    let f_warm = smoothstep(10.0, 20.0, env.temperature_c);
    f_river * f_not_alpine * f_warm
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env_at(z: f32, t: f32, theta: f32) -> EnvSample {
        EnvSample {
            temperature_c: t,
            soil_moisture: theta,
            z_norm: z,
            slope: 0.0,
            fog_likelihood: 0.0,
            river_proximity: 0.0,
            coast_proximity: 0.0,
        }
    }

    #[test]
    fn bell_centered_is_one() {
        assert!((bell(1.23, 1.23, 0.4) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn bell_off_peak_is_smaller() {
        let a = bell(0.5, 0.5, 0.2);
        let b = bell(0.7, 0.5, 0.2);
        assert!(a > b);
    }

    #[test]
    fn lowland_forest_prefers_warm_wet_low_elevation() {
        let ideal = env_at(0.1, 24.0, 0.6);
        let mountain = env_at(0.6, 24.0, 0.6);
        let cold = env_at(0.1, 5.0, 0.6);
        let dry = env_at(0.1, 24.0, 0.1);
        assert!(lowland_forest(ideal) > lowland_forest(mountain));
        assert!(lowland_forest(ideal) > lowland_forest(cold));
        assert!(lowland_forest(ideal) > lowland_forest(dry));
    }

    /// Sprint 3 DD5: cloud forest fog is now a supporting factor (not a gate).
    ///
    /// - Cells with fog near 1.0 get slightly higher suitability than cells
    ///   with fog = 0 (by the `FOG_PEAK_WEIGHT = 0.3` margin).
    /// - Both high-fog and no-fog cells at ideal T/θ/z produce positive
    ///   suitability (fog no longer kills the biome when absent).
    #[test]
    fn cloud_forest_fog_is_supporting_factor_not_gating() {
        let ideal = EnvSample {
            temperature_c: 15.0,
            soil_moisture: 0.85,
            z_norm: 0.60,
            slope: 0.0,
            fog_likelihood: 1.0,
            river_proximity: 0.0,
            coast_proximity: 0.0,
        };
        let no_fog = EnvSample {
            fog_likelihood: 0.0,
            ..ideal
        };

        let s_ideal = cloud_forest(ideal);
        let s_no_fog = cloud_forest(no_fog);

        // Both cases produce positive suitability.
        assert!(
            s_ideal > 0.2,
            "ideal cloud-forest cell should have suitability > 0.2, got {s_ideal}"
        );
        assert!(
            s_no_fog > 0.1,
            "no-fog cloud-forest cell should still have suitability > 0.1, got {s_no_fog}"
        );

        // High-fog beats no-fog, but ratio must reflect a supporting
        // contribution (not a 20× gate like the Sprint 1B smoothstep).
        assert!(
            s_ideal > s_no_fog,
            "high-fog cell must exceed no-fog cell ({s_ideal} > {s_no_fog})"
        );
        let ratio = s_no_fog / s_ideal;
        assert!(
            ratio > 0.5,
            "no-fog/ideal ratio should be > 0.5 (fog is supporting, not gating), got {ratio}"
        );
    }

    /// Sprint 3 Task 3.1.C: lock the sigma_fog and peak-weight constants so
    /// future tuning changes surface as a compile-time test diff.
    /// Candidate A values: sigma_fog=0.15, peak_weight=0.40.
    #[test]
    fn cloud_forest_bell_tuning_matches_dd5() {
        // sigma_fog widened from 0.08 to 0.15 in Task 3.1.C candidate A
        // to spread the fog-dependence band across a broader inversion layer.
        assert!(
            (CLOUD_FOREST_SIGMA_FOG - 0.15).abs() < f32::EPSILON,
            "CLOUD_FOREST_SIGMA_FOG must be 0.15 (3.1.C candidate A value), got {}",
            CLOUD_FOREST_SIGMA_FOG
        );
        // fog peak weight increased from 0.30 to 0.40 in Task 3.1.C candidate A
        // to strengthen the direct fog suitability term alongside the
        // soil_moisture coupling increase.
        assert!(
            (CLOUD_FOREST_FOG_PEAK_WEIGHT - 0.40).abs() < f32::EPSILON,
            "CLOUD_FOREST_FOG_PEAK_WEIGHT must be 0.40 (3.1.C candidate A value), got {}",
            CLOUD_FOREST_FOG_PEAK_WEIGHT
        );
    }

    #[test]
    fn bare_rock_favours_high_elevation_or_steep_slope() {
        let high = EnvSample {
            z_norm: 0.9,
            slope: 0.1,
            soil_moisture: 0.2,
            temperature_c: 0.0,
            fog_likelihood: 0.0,
            river_proximity: 0.0,
            coast_proximity: 0.0,
        };
        let steep = EnvSample {
            z_norm: 0.2,
            slope: 0.8,
            soil_moisture: 0.2,
            temperature_c: 20.0,
            fog_likelihood: 0.0,
            river_proximity: 0.0,
            coast_proximity: 0.0,
        };
        let mild = EnvSample {
            z_norm: 0.2,
            slope: 0.1,
            soil_moisture: 0.7,
            temperature_c: 20.0,
            fog_likelihood: 0.0,
            river_proximity: 0.0,
            coast_proximity: 0.0,
        };
        assert!(bare_rock_lava(high) > 0.3);
        assert!(bare_rock_lava(steep) > 0.1);
        assert!(bare_rock_lava(mild) < 0.1);
    }
}
