//! `PrecipitationStage` (DD2) — upwind raymarch orographic precipitation
//! proxy.
//!
//! For each target cell `p`, trace a ray `N` cells upwind (in the
//! direction the wind is coming from). Starting with marine moisture
//! charged at the upstream end, march back toward `p`: ascents condense
//! moisture into accumulated precipitation, descents apply rain-shadow
//! attenuation to what has already been condensed. The final
//! accumulation is the raw precipitation at `p`; all cells are then
//! normalised to `[0, 1]`.
//!
//! # Sign convention
//!
//! `wind = wind_unit(preset.prevailing_wind_dir)` is the direction the
//! wind is **coming from** (meteorology: "easterly" blows from the
//! east). Air parcels advance along `-wind`. A cell with
//! `signed_uplift(wind, grad_z) > 0` is ascending (windward) and
//! condenses moisture; `< 0` is descending (leeward) and triggers
//! rain-shadow drying. See `climate::common::signed_uplift` for the
//! single source of truth on this convention.

use anyhow::anyhow;
use island_core::field::ScalarField2D;
use island_core::pipeline::SimulationStage;
use island_core::world::WorldState;

use crate::climate::common::{compute_distance_to_mask, grad_scalar_at, signed_uplift, wind_unit};

// ── tunable constants (v1 hardcoded; promoted to runtime config in Task 1B.9)

/// Number of upwind steps per target cell. `32` covers about 12 % of a
/// 256-cell-wide domain per the spec; tune via `Task 1B.9` if rays are
/// too short for wider islands.
pub(crate) const RAYMARCH_STEPS: u32 = 32;

/// Condensation efficiency: fraction of moisture dropped per unit of
/// positive uplift per step.
pub(crate) const CONDENSATION_RATE: f32 = 1.5;

/// Exponential rain-shadow attenuation coefficient. Higher values
/// produce sharper leeward dry zones.
pub(crate) const RAIN_SHADOW_K: f32 = 2.0;

/// Ocean moisture availability scale at the raymarch start. Normalised
/// `[0, 1]` fraction of `marine_moisture_strength`.
pub(crate) const COAST_MOISTURE_DECAY: f32 = 0.1;

// ── stage ────────────────────────────────────────────────────────────────────

/// DD2: populate `world.baked.precipitation`.
pub struct PrecipitationStage;

impl SimulationStage for PrecipitationStage {
    fn name(&self) -> &'static str {
        "precipitation"
    }

    fn run(&self, world: &mut WorldState) -> anyhow::Result<()> {
        let z = world
            .derived
            .z_filled
            .as_ref()
            .ok_or_else(|| anyhow!("PrecipitationStage: z_filled is None"))?;
        let coast = world
            .derived
            .coast_mask
            .as_ref()
            .ok_or_else(|| anyhow!("PrecipitationStage: coast_mask is None"))?;

        let w = z.width;
        let h = z.height;
        let wind = wind_unit(world.preset.prevailing_wind_dir);
        let marine_moisture = world.preset.marine_moisture_strength;

        let dist_to_coast = compute_distance_to_mask(&coast.is_coast, w, h);
        // Normalising by the smaller dimension couples the effective
        // decay length to domain aspect ratio. v1 ships this as-is;
        // Task 1B.9 tuning may switch to `(w + h) / 2` for shape
        // invariance once non-square preview resolutions matter.
        let dist_norm = w.min(h) as f32;

        // Raw accumulation pass — one raymarch per target cell. The
        // outer loop is embarrassingly parallel (per-cell writes,
        // read-only inputs); Task 1B.9 is the place to add rayon
        // `par_iter_mut` if the slider target ever exceeds 200 ms.
        let mut raw = ScalarField2D::<f32>::new(w, h);
        for iy in 0..h {
            for ix in 0..w {
                let contribution = raymarch_contribution(
                    ix,
                    iy,
                    wind,
                    z,
                    &dist_to_coast,
                    dist_norm,
                    marine_moisture,
                );
                raw.set(ix, iy, contribution);
            }
        }

        // Normalise to [0, 1]. `raw` was just built with `new(w, h)`
        // where both are positive (enforced by a valid WorldState), so
        // `stats()` only returns `None` for a zero-dimension field.
        let stats = raw.stats().expect("raw has positive dimensions");
        let max = stats.max.max(f32::EPSILON);
        let mut precipitation = raw;
        for v in precipitation.data.iter_mut() {
            *v = (*v / max).clamp(0.0, 1.0);
        }

        world.baked.precipitation = Some(precipitation);
        Ok(())
    }
}

// ── raymarch kernel ──────────────────────────────────────────────────────────

fn raymarch_contribution(
    tx: u32,
    ty: u32,
    wind: glam::Vec2,
    z: &ScalarField2D<f32>,
    dist_to_coast: &ScalarField2D<f32>,
    dist_norm: f32,
    marine_moisture: f32,
) -> f32 {
    let w = z.width;
    let h = z.height;

    // Upstream start = target + wind * N (wind points FROM the direction
    // the air is coming from, so air advances along -wind; the upstream
    // end is back along +wind from the target).
    let start = {
        let sx = tx as f32 + wind.x * RAYMARCH_STEPS as f32;
        let sy = ty as f32 + wind.y * RAYMARCH_STEPS as f32;
        sample_nearest_cell(sx, sy, w, h)
    };

    // Initial moisture from the coast-proximity of the upstream start.
    let d_start = dist_to_coast.get(start.0, start.1) / dist_norm;
    let coast_prox = (-d_start / COAST_MOISTURE_DECAY).exp();
    let mut moisture = marine_moisture * coast_prox;

    // Walk from upstream toward the target along -wind.
    let mut p = 0.0_f32;
    for step in (0..RAYMARCH_STEPS).rev() {
        let fx = tx as f32 + wind.x * step as f32;
        let fy = ty as f32 + wind.y * step as f32;
        let (cx, cy) = sample_nearest_cell(fx, fy, w, h);

        let grad = grad_scalar_at(z, cx, cy);
        let signed = signed_uplift(wind, grad);

        if signed > 0.0 {
            // Ascent: condense moisture into precipitation at the target.
            let condensed = (CONDENSATION_RATE * signed * moisture).min(moisture);
            moisture -= condensed;
            p += condensed;
        } else if signed < 0.0 {
            // Descent: rain-shadow attenuation of accumulated precipitation.
            let descent = -signed;
            p *= (-RAIN_SHADOW_K * descent).exp();
        }
    }

    p
}

fn sample_nearest_cell(fx: f32, fy: f32, w: u32, h: u32) -> (u32, u32) {
    let x = fx.round().clamp(0.0, (w - 1) as f32) as u32;
    let y = fy.round().clamp(0.0, (h - 1) as f32) as u32;
    (x, y)
}

// ── tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use island_core::field::{MaskField2D, ScalarField2D};
    use island_core::preset::{IslandAge, IslandArchetypePreset};
    use island_core::seed::Seed;
    use island_core::world::{CoastMask, Resolution, WorldState};

    fn preset(wind_dir: f32) -> IslandArchetypePreset {
        IslandArchetypePreset {
            name: "precip_test".into(),
            island_radius: 0.5,
            max_relief: 1.0,
            volcanic_center_count: 1,
            island_age: IslandAge::Young,
            prevailing_wind_dir: wind_dir,
            marine_moisture_strength: 1.0,
            sea_level: 0.0,
        }
    }

    /// Build a world with a single ridge running along the y-axis at
    /// x = cx. The ridge is a tent (triangular) in x. Wind blows along
    /// the +x axis (from east, `wind_unit(0)` = (1, 0) ), so the windward
    /// face is the eastern side of the ridge and the leeward face is
    /// the western side.
    fn world_with_ns_ridge(w: u32, h: u32, cx: u32, wind_dir: f32) -> WorldState {
        let mut world = WorldState::new(Seed(0), preset(wind_dir), Resolution::new(w, h));

        let mut z = ScalarField2D::<f32>::new(w, h);
        for iy in 0..h {
            for ix in 0..w {
                let dx = (ix as i32 - cx as i32).unsigned_abs();
                let height = (8_u32.saturating_sub(dx)) as f32 * 0.1;
                z.set(ix, iy, height);
            }
        }
        world.derived.z_filled = Some(z);

        let mut is_land = MaskField2D::new(w, h);
        is_land.data.fill(1);
        let mut is_coast = MaskField2D::new(w, h);
        for iy in 0..h {
            is_coast.set(0, iy, 1);
            is_coast.set(w - 1, iy, 1);
        }
        world.derived.coast_mask = Some(CoastMask {
            is_land,
            is_sea: MaskField2D::new(w, h),
            is_coast,
            land_cell_count: w * h,
            river_mouth_mask: None,
        });
        world
    }

    // 1. Windward cells (east of a N-S ridge, wind from east) receive
    //    substantially more precipitation than leeward cells (west of
    //    the ridge). Spec requires >= 30 % higher on the windward side.
    #[test]
    fn windward_beats_leeward_by_30_percent() {
        let (w, h) = (64_u32, 32_u32);
        let ridge_x = 32_u32;
        let mut world = world_with_ns_ridge(w, h, ridge_x, 0.0); // wind from east

        PrecipitationStage.run(&mut world).expect("stage failed");
        let p = world.baked.precipitation.as_ref().unwrap();

        // Take the mid-height row and average over a 4-cell window on
        // each flank so we don't happen-to-land on a dead pixel.
        let mut windward = 0.0_f32;
        let mut leeward = 0.0_f32;
        for ix in 0..4 {
            windward += p.get(ridge_x + 2 + ix, h / 2);
            leeward += p.get(ridge_x - 5 - ix, h / 2);
        }
        windward /= 4.0;
        leeward /= 4.0;

        assert!(
            windward > leeward * 1.3,
            "windward {windward} should exceed leeward {leeward} by 30%"
        );
    }

    // 2. Determinism: two identical worlds give bit-exact output.
    #[test]
    fn precipitation_determinism() {
        let (w, h) = (32_u32, 32_u32);
        let mut w1 = world_with_ns_ridge(w, h, 16, 0.0);
        let mut w2 = world_with_ns_ridge(w, h, 16, 0.0);
        PrecipitationStage.run(&mut w1).expect("run1");
        PrecipitationStage.run(&mut w2).expect("run2");
        assert_eq!(
            &w1.baked.precipitation.as_ref().unwrap().data,
            &w2.baked.precipitation.as_ref().unwrap().data
        );
    }

    // 3. Output is confined to [0, 1] after normalization.
    #[test]
    fn output_range_is_normalized() {
        let (w, h) = (32_u32, 32_u32);
        let mut world = world_with_ns_ridge(w, h, 16, 0.0);
        PrecipitationStage.run(&mut world).expect("stage failed");
        let p = world.baked.precipitation.as_ref().unwrap();
        let stats = p.stats().expect("non-empty");
        assert!(stats.min >= 0.0);
        assert!(stats.max <= 1.0);
        // And somewhere above 0 so we didn't nuke the whole field.
        assert!(stats.max > 0.1);
    }

    // 4. Missing precondition errors cleanly.
    #[test]
    fn errors_when_z_filled_missing() {
        let mut world = WorldState::new(Seed(0), preset(0.0), Resolution::new(8, 8));
        let result = PrecipitationStage.run(&mut world);
        assert!(result.is_err());
    }

    // 5. A flat domain produces zero precipitation (no uplift anywhere).
    //    This catches a bug where the coast-proximity seed is double-
    //    counted as "rain".
    #[test]
    fn flat_domain_has_no_precipitation() {
        let (w, h) = (16_u32, 16_u32);
        let mut world = WorldState::new(Seed(0), preset(0.0), Resolution::new(w, h));
        let z = ScalarField2D::<f32>::new(w, h); // all 0.0
        world.derived.z_filled = Some(z);
        let mut is_land = MaskField2D::new(w, h);
        is_land.data.fill(1);
        let mut is_coast = MaskField2D::new(w, h);
        for iy in 0..h {
            is_coast.set(0, iy, 1);
        }
        world.derived.coast_mask = Some(CoastMask {
            is_land,
            is_sea: MaskField2D::new(w, h),
            is_coast,
            land_cell_count: w * h,
            river_mouth_mask: None,
        });

        PrecipitationStage.run(&mut world).expect("stage failed");
        let p = world.baked.precipitation.as_ref().unwrap();
        let stats = p.stats().expect("non-empty");
        // On a truly flat domain `raw` is all zero → `stats.max == 0` →
        // `max.max(EPSILON)` keeps the division finite → every cell is
        // `0 / EPSILON = 0`. Tight threshold catches the regression
        // where the coast-proximity seed leaks into precipitation
        // output even without any orographic uplift.
        assert!(
            stats.max < 1e-9,
            "flat domain produced non-zero precip: max={}",
            stats.max
        );
    }
}
