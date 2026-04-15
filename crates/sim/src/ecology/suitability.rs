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
    let f_z = bell(env.z_norm, 0.03, 0.05);
    let f_coast = smoothstep(0.3, 0.9, env.coast_proximity);
    let f_dry = smoothstep(0.15, 0.55, 1.0 - env.soil_moisture);
    f_z * f_coast * f_dry
}

pub fn lowland_forest(env: EnvSample) -> f32 {
    let f_t = bell(env.temperature_c, 24.0, 4.0);
    let f_theta = smoothstep(0.35, 0.75, env.soil_moisture);
    let f_z = bell(env.z_norm, 0.15, 0.12);
    f_t * f_theta * f_z
}

pub fn montane_wet_forest(env: EnvSample) -> f32 {
    let f_t = bell(env.temperature_c, 18.0, 4.0);
    let f_theta = smoothstep(0.5, 0.9, env.soil_moisture);
    let f_z = bell(env.z_norm, 0.4, 0.15);
    f_t * f_theta * f_z
}

pub fn cloud_forest(env: EnvSample) -> f32 {
    let f_t = bell(env.temperature_c, 15.0, 4.0);
    let f_theta = smoothstep(0.6, 0.95, env.soil_moisture);
    let f_z = bell(env.z_norm, 0.55, 0.18);
    let f_fog = smoothstep(0.4, 0.95, env.fog_likelihood);
    f_t * f_theta * f_z * f_fog
}

pub fn dry_shrub(env: EnvSample) -> f32 {
    let f_z = bell(env.z_norm, 0.1, 0.1);
    let f_dry = smoothstep(0.4, 0.9, 1.0 - env.soil_moisture);
    let f_warm = smoothstep(18.0, 24.0, env.temperature_c);
    f_z * f_dry * f_warm
}

pub fn grassland(env: EnvSample) -> f32 {
    let f_z = bell(env.z_norm, 0.3, 0.15);
    let f_mid = bell(env.soil_moisture, 0.45, 0.2);
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

    #[test]
    fn cloud_forest_needs_fog_and_midelevation() {
        let ideal = EnvSample {
            temperature_c: 15.0,
            soil_moisture: 0.85,
            z_norm: 0.55,
            slope: 0.0,
            fog_likelihood: 0.9,
            river_proximity: 0.0,
            coast_proximity: 0.0,
        };
        let no_fog = EnvSample {
            fog_likelihood: 0.0,
            ..ideal
        };
        assert!(cloud_forest(ideal) > 0.3);
        assert!(cloud_forest(no_fog) < cloud_forest(ideal) * 0.05);
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
