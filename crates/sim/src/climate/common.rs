//! Shared climate helpers: sign conventions and cell-level field samplers
//! reused by `TemperatureStage`, `PrecipitationStage`, and
//! `FogLikelihoodStage`.

use glam::Vec2;
use island_core::field::{MaskField2D, ScalarField2D};

/// Signed orographic uplift rate at the current cell: `-wind · grad_z`.
///
/// # Convention
///
/// `wind` is the **prevailing wind direction of origin** (meteorology:
/// "easterly" = a wind *from* the east), a unit vector. Air parcels move
/// along `-wind`, so:
///
/// * `signed > 0` → air is rising into a slope (windward ascent →
///   condensation + precipitation + fog).
/// * `signed < 0` → air is descending a slope (leeward → adiabatic
///   drying, rain shadow, fog suppression).
///
/// Consumers clamp to `max(0, signed)` when they only care about the
/// ascent branch; precipitation splits the two paths explicitly.
pub fn signed_uplift(wind: Vec2, grad_z: Vec2) -> f32 {
    -wind.dot(grad_z)
}

/// Central-difference gradient of a scalar field at `(x, y)`, with
/// one-sided (Neumann) fallback at domain boundaries. `dx = dy = 1.0`
/// in cell units, matching `DerivedGeomorphStage`.
///
/// Shared by every climate stage that wants `grad z_filled` at a single
/// cell (precipitation, fog). Stages that need the gradient at every
/// cell should call this in a loop — inner-loop callers can amortize
/// the boundary checks if profiling demands it, but v1 keeps things
/// simple.
///
/// Note: `DerivedGeomorphStage` intentionally does not call this helper.
/// That stage reuses its four stencil samples for both the slope and
/// the 5-point laplacian, so pulling the gradient out into a function
/// would either duplicate the field reads or force this helper to
/// return the full stencil tuple — neither is an improvement.
pub fn grad_scalar_at(field: &ScalarField2D<f32>, x: u32, y: u32) -> Vec2 {
    let w = field.width;
    let h = field.height;

    let gx = if x == 0 {
        field.get(1, y) - field.get(0, y)
    } else if x == w - 1 {
        field.get(w - 1, y) - field.get(w - 2, y)
    } else {
        (field.get(x + 1, y) - field.get(x - 1, y)) * 0.5
    };

    let gy = if y == 0 {
        field.get(x, 1) - field.get(x, 0)
    } else if y == h - 1 {
        field.get(x, h - 1) - field.get(x, h - 2)
    } else {
        (field.get(x, y + 1) - field.get(x, y - 1)) * 0.5
    };

    Vec2::new(gx, gy)
}

/// Unit vector pointing along the prevailing wind direction (radians).
/// `0` = east, following the angle convention used elsewhere in the
/// workspace (`preset.prevailing_wind_dir`).
pub fn wind_unit(wind_dir_rad: f32) -> Vec2 {
    Vec2::new(wind_dir_rad.cos(), wind_dir_rad.sin())
}

/// Hermite smoothstep: `0` below `edge0`, `1` above `edge1`, smooth
/// cubic `3t² - 2t³` in between. Matches the GLSL / HLSL convention
/// used throughout the climate + ecology suitability functions.
pub fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    if edge0 == edge1 {
        return if x < edge0 { 0.0 } else { 1.0 };
    }
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Manhattan (L1) distance from each cell to the nearest coast cell,
/// measured in cell units via a multi-source Von4 BFS. Non-coast cells
/// that can't reach a coast (e.g. a fully land-locked island with no
/// coast cells at all) get `f32::MAX`.
///
/// Shared by `TemperatureStage` (coastal modifier), `PrecipitationStage`
/// (marine moisture seeding), and any later stage that needs a cheap
/// proxy for "how far inland are we". `O(sim_cells)` work, single
/// sweep.
pub fn compute_distance_to_coast(coast: &MaskField2D, w: u32, h: u32) -> ScalarField2D<f32> {
    let mut dist = ScalarField2D::<f32>::new(w, h);
    dist.data.fill(f32::MAX);

    let mut frontier: Vec<(u32, u32)> = Vec::new();
    for iy in 0..h {
        for ix in 0..w {
            if coast.get(ix, iy) == 1 {
                dist.set(ix, iy, 0.0);
                frontier.push((ix, iy));
            }
        }
    }

    let mut next: Vec<(u32, u32)> = Vec::new();
    let mut step = 1.0_f32;
    while !frontier.is_empty() {
        for &(x, y) in &frontier {
            for (dx, dy) in [(-1_i32, 0_i32), (1, 0), (0, -1), (0, 1)] {
                let nx = x as i32 + dx;
                let ny = y as i32 + dy;
                if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                    continue;
                }
                let (nxu, nyu) = (nx as u32, ny as u32);
                if dist.get(nxu, nyu) > step {
                    dist.set(nxu, nyu, step);
                    next.push((nxu, nyu));
                }
            }
        }
        frontier.clear();
        std::mem::swap(&mut frontier, &mut next);
        step += 1.0;
    }

    dist
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec2;

    #[test]
    fn signed_uplift_sign_conventions() {
        // Wind from the east (preset angle 0): air flows west.
        let wind = wind_unit(0.0);

        // Upslope to the west (grad_z = (-1, 0)): air climbs → positive uplift.
        let grad_up_west = Vec2::new(-1.0, 0.0);
        assert!(signed_uplift(wind, grad_up_west) > 0.0);

        // Downslope to the west (grad_z = (1, 0)): air descends → negative.
        let grad_down_west = Vec2::new(1.0, 0.0);
        assert!(signed_uplift(wind, grad_down_west) < 0.0);

        // Flat terrain.
        assert_eq!(signed_uplift(wind, Vec2::ZERO), 0.0);
    }

    #[test]
    fn wind_unit_cardinal_directions() {
        use std::f32::consts::FRAC_PI_2;
        let east = wind_unit(0.0);
        assert!((east - Vec2::new(1.0, 0.0)).length() < 1e-6);
        let north = wind_unit(FRAC_PI_2);
        assert!((north - Vec2::new(0.0, 1.0)).length() < 1e-6);
    }

    #[test]
    fn smoothstep_edge_cases() {
        assert_eq!(smoothstep(0.0, 1.0, -0.5), 0.0);
        assert_eq!(smoothstep(0.0, 1.0, 1.5), 1.0);
        assert!((smoothstep(0.0, 1.0, 0.5) - 0.5).abs() < 1e-6);
        // Degenerate edges → step function.
        assert_eq!(smoothstep(0.5, 0.5, 0.4), 0.0);
        assert_eq!(smoothstep(0.5, 0.5, 0.6), 1.0);
    }

    #[test]
    fn smoothstep_monotonic() {
        let mut prev = -1.0_f32;
        for i in 0..=20 {
            let x = i as f32 * 0.05;
            let y = smoothstep(0.0, 1.0, x);
            assert!(y >= prev);
            prev = y;
        }
    }

    #[test]
    fn grad_scalar_at_linear_plane() {
        // z = 0.3 * x + 0.1 * y → grad = (0.3, 0.1) at every interior cell.
        let (w, h) = (8_u32, 8_u32);
        let mut z = ScalarField2D::<f32>::new(w, h);
        for iy in 0..h {
            for ix in 0..w {
                z.set(ix, iy, 0.3 * ix as f32 + 0.1 * iy as f32);
            }
        }
        for iy in 1..(h - 1) {
            for ix in 1..(w - 1) {
                let g = grad_scalar_at(&z, ix, iy);
                assert!((g.x - 0.3).abs() < 1e-5);
                assert!((g.y - 0.1).abs() < 1e-5);
            }
        }
    }
}
