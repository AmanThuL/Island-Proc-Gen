//! Sprint 3.5 DD1: 6-edge hex geometry.
//!
//! This module supplies the pure math the Sprint 3.5 hex surface needs:
//! axial ↔ pixel conversion, edge midpoint lookup, and polygon vertex
//! enumeration for procedural mesh construction.
//!
//! # Orientation
//!
//! The project's [`island_core::world::HexLayout::FlatTop`] variant fixes
//! the convention used throughout Sprint 2.5 / 3.5: rows run horizontally,
//! each hex has **width = `sqrt(3) * hex_size`**, **height = `2 * hex_size`**
//! (where `hex_size` is the centre-to-vertex radius), row vertical spacing
//! is `1.5 * hex_size`, and consecutive rows are offset horizontally by
//! `hex_size * sqrt(3) / 2`. The rightmost and leftmost extents of each
//! hex are **vertical edges** (labelled [`HexEdge::E`] / [`HexEdge::W`]).
//! DD2's aggregation kernel (Sprint 3.5.A c4, forthcoming) assumes exactly
//! this layout.
//!
//! # Edge numbering (DD1, load-bearing)
//!
//! 6 edges numbered counter-clockwise from east:
//!
//! ```text
//!         2 (NW)  1 (NE)
//!            \   /
//!  3 (W) --|    |-- 0 (E)
//!            /   \
//!         4 (SW)  5 (SE)
//! ```
//!
//! Edge angle from the hex centre is `edge_index * 60°`, i.e. E = 0°,
//! NE = 60°, NW = 120°, W = 180°, SW = 240°, SE = 300°. The edge midpoint
//! sits at the apothem distance `hex_size * sqrt(3) / 2` along that ray.
//!
//! **No raw 0..=5 edge indices allowed outside this file.** Downstream
//! consumers must use the [`HexEdge`] enum by name (enforced by reviewer
//! grep on close-out; compile-time enum discipline is the enforcement
//! mechanism).

use crate::AxialCoord;

/// `sqrt(3)` as a 32-bit float. `SQRT_3` is still
/// unstable (`more_float_constants`, rust-lang#146939), so carry our own.
const SQRT_3: f32 = 1.7320508_f32;

// ─────────────────────────────────────────────────────────────────────────────
// HexEdge enum
// ─────────────────────────────────────────────────────────────────────────────

/// One of the 6 edges of a hex, per the DD1 convention.
///
/// Discriminants are load-bearing: they match the numbering used by
/// [`HexDebugAttributes::river_crossing`](island_core::world::HexDebugAttributes)
/// once DD3 promotes the encoding from 4-edge to 6-edge (Sprint 3.5.B c1).
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HexEdge {
    /// East edge — vertical right side. Angle 0°.
    E = 0,
    /// North-east edge — upper-right diagonal. Angle 60°.
    NE = 1,
    /// North-west edge — upper-left diagonal. Angle 120°.
    NW = 2,
    /// West edge — vertical left side. Angle 180°.
    W = 3,
    /// South-west edge — lower-left diagonal. Angle 240°.
    SW = 4,
    /// South-east edge — lower-right diagonal. Angle 300°.
    SE = 5,
}

impl HexEdge {
    /// All 6 edges in CCW order starting from [`HexEdge::E`].
    pub const ALL: [HexEdge; 6] = [Self::E, Self::NE, Self::NW, Self::W, Self::SW, Self::SE];

    /// Angle of this edge's midpoint ray from the hex centre, in radians.
    ///
    /// `edge_index * PI/3` — E=0°, NE=60°, NW=120°, W=180°, SW=240°, SE=300°.
    #[inline]
    pub fn angle_rad(self) -> f32 {
        (self as u8 as f32) * std::f32::consts::FRAC_PI_3
    }

    /// Parse a raw discriminant. Returns `None` for out-of-range values.
    ///
    /// Useful at deserialization boundaries where a `u8` from disk needs to
    /// be validated before landing in a [`HexEdge`]. Elsewhere downstream
    /// code should use [`HexEdge`] variants by name.
    #[inline]
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::E),
            1 => Some(Self::NE),
            2 => Some(Self::NW),
            3 => Some(Self::W),
            4 => Some(Self::SW),
            5 => Some(Self::SE),
            _ => None,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Axial ↔ pixel conversion
// ─────────────────────────────────────────────────────────────────────────────

/// Convert an axial coordinate to the pixel (world-space) centre of its hex.
///
/// Origin: `AxialCoord { q: 0, r: 0 }` maps to `(0.0, 0.0)`.
/// Formula (see Red Blob Games axial-to-pixel reference, pointy-top form):
/// `x = size * sqrt(3) * (q + r/2)`, `y = size * 3/2 * r`.
#[inline]
pub fn axial_to_pixel(c: AxialCoord, hex_size: f32) -> (f32, f32) {
    let q = c.q as f32;
    let r = c.r as f32;
    let x = hex_size * SQRT_3 * (q + 0.5 * r);
    let y = hex_size * 1.5 * r;
    (x, y)
}

/// Convert a pixel (world-space) position to the nearest hex axial coord.
///
/// Inverts [`axial_to_pixel`] then snaps via standard cube-rounding so that
/// any `(px, py)` within a hex's polygon maps to that hex's axial coord.
#[inline]
pub fn pixel_to_axial(px: f32, py: f32, hex_size: f32) -> AxialCoord {
    let fq = (SQRT_3 / 3.0 * px - py / 3.0) / hex_size;
    let fr = (2.0 / 3.0 * py) / hex_size;
    axial_round(fq, fr)
}

/// Round a fractional axial coord to the nearest integer hex using cube
/// rounding (re-derive `s = -q - r`, round all three, fix up the largest
/// delta to preserve `q + r + s = 0`).
fn axial_round(fq: f32, fr: f32) -> AxialCoord {
    let fs = -fq - fr;
    let mut q_r = fq.round();
    let mut r_r = fr.round();
    let s_r = fs.round();
    let dq = (q_r - fq).abs();
    let dr = (r_r - fr).abs();
    let ds = (s_r - fs).abs();
    if dq > dr && dq > ds {
        q_r = -r_r - s_r;
    } else if dr > ds {
        r_r = -q_r - s_r;
    }
    // s is recomputed but not stored — axial coords only keep q and r.
    AxialCoord {
        q: q_r as i32,
        r: r_r as i32,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Edge midpoint + polygon vertices
// ─────────────────────────────────────────────────────────────────────────────

/// World-space midpoint of the requested edge.
///
/// Distance from `hex_center` is the apothem `hex_size * sqrt(3) / 2`.
#[inline]
pub fn edge_midpoint(hex_center: (f32, f32), edge: HexEdge, hex_size: f32) -> (f32, f32) {
    let apothem = hex_size * SQRT_3 * 0.5;
    let angle = edge.angle_rad();
    (
        hex_center.0 + apothem * angle.cos(),
        hex_center.1 + apothem * angle.sin(),
    )
}

/// The 6 polygon vertices of a hex in CCW order starting from the upper-right
/// vertex (between [`HexEdge::E`] and [`HexEdge::NE`]).
///
/// All vertices are at distance `hex_size` from the hex centre, at angles
/// `30° + 60° * k` for `k = 0..6` — which puts them between the edge-midpoint
/// rays returned by [`HexEdge::angle_rad`].
pub fn hex_polygon_vertices(hex_center: (f32, f32), hex_size: f32) -> [(f32, f32); 6] {
    let mut verts = [(0.0_f32, 0.0_f32); 6];
    let base = std::f32::consts::FRAC_PI_6;
    for (k, slot) in verts.iter_mut().enumerate() {
        let angle = base + (k as f32) * std::f32::consts::FRAC_PI_3;
        *slot = (
            hex_center.0 + hex_size * angle.cos(),
            hex_center.1 + hex_size * angle.sin(),
        );
    }
    verts
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::{FRAC_PI_3, FRAC_PI_6};

    fn approx_eq(a: f32, b: f32, tol: f32) -> bool {
        (a - b).abs() < tol
    }

    #[test]
    fn hex_edge_discriminants_match_dd1() {
        assert_eq!(HexEdge::E as u8, 0);
        assert_eq!(HexEdge::NE as u8, 1);
        assert_eq!(HexEdge::NW as u8, 2);
        assert_eq!(HexEdge::W as u8, 3);
        assert_eq!(HexEdge::SW as u8, 4);
        assert_eq!(HexEdge::SE as u8, 5);
    }

    #[test]
    fn hex_edge_all_is_ccw_from_east() {
        assert_eq!(
            HexEdge::ALL,
            [
                HexEdge::E,
                HexEdge::NE,
                HexEdge::NW,
                HexEdge::W,
                HexEdge::SW,
                HexEdge::SE
            ]
        );
    }

    #[test]
    fn hex_edge_angle_rad_ccw_from_east() {
        // Each edge is 60° further CCW than the previous.
        for (i, e) in HexEdge::ALL.iter().enumerate() {
            let expected = (i as f32) * FRAC_PI_3;
            assert!(
                approx_eq(e.angle_rad(), expected, 1e-6),
                "edge {:?} angle {} vs expected {}",
                e,
                e.angle_rad(),
                expected
            );
        }
    }

    #[test]
    fn hex_edge_from_u8_roundtrip() {
        for e in HexEdge::ALL {
            assert_eq!(HexEdge::from_u8(e as u8), Some(e));
        }
        assert_eq!(HexEdge::from_u8(6), None);
        assert_eq!(HexEdge::from_u8(0xFF), None);
    }

    #[test]
    fn axial_origin_maps_to_pixel_origin() {
        let (x, y) = axial_to_pixel(AxialCoord::new(0, 0), 1.0);
        assert!(approx_eq(x, 0.0, 1e-6));
        assert!(approx_eq(y, 0.0, 1e-6));
    }

    #[test]
    fn axial_to_pixel_matches_known_neighbors() {
        // For pointy-top layout, moving from (0,0) to (1,0) is pure east by
        // width = sqrt(3) * size. Moving to (0,1) is SE by (size*sqrt(3)/2,
        // size*3/2). Moving to (-1, 1) is SW by (-size*sqrt(3)/2, size*3/2).
        let size = 2.0;
        let (x, y) = axial_to_pixel(AxialCoord::new(1, 0), size);
        assert!(approx_eq(x, size * SQRT_3, 1e-5), "east neighbor x");
        assert!(approx_eq(y, 0.0, 1e-6), "east neighbor y");

        let (x, y) = axial_to_pixel(AxialCoord::new(0, 1), size);
        assert!(
            approx_eq(x, size * SQRT_3 * 0.5, 1e-5),
            "SE neighbor x = size * sqrt(3)/2"
        );
        assert!(approx_eq(y, size * 1.5, 1e-5), "SE neighbor y = size * 3/2");

        let (x, y) = axial_to_pixel(AxialCoord::new(-1, 1), size);
        assert!(
            approx_eq(x, -size * SQRT_3 * 0.5, 1e-5),
            "SW neighbor x = -size * sqrt(3)/2"
        );
        assert!(approx_eq(y, size * 1.5, 1e-5), "SW neighbor y = size * 3/2");
    }

    #[test]
    fn axial_pixel_roundtrip_at_hex_centers() {
        let size = 1.7;
        for &(q, r) in &[
            (0, 0),
            (1, 0),
            (0, 1),
            (-1, 1),
            (3, -2),
            (-4, 5),
            (7, 7),
            (-10, -3),
        ] {
            let (px, py) = axial_to_pixel(AxialCoord::new(q, r), size);
            let back = pixel_to_axial(px, py, size);
            assert_eq!(
                back,
                AxialCoord::new(q, r),
                "roundtrip failed for ({q}, {r}) at ({px}, {py})"
            );
        }
    }

    #[test]
    fn pixel_to_axial_rounds_offset_points_to_nearest_hex() {
        // A point slightly off the centre of hex (2, -1) should still round
        // to (2, -1).
        let size = 1.0;
        let (cx, cy) = axial_to_pixel(AxialCoord::new(2, -1), size);
        let eps = 0.1 * size;
        assert_eq!(
            pixel_to_axial(cx + eps, cy - eps, size),
            AxialCoord::new(2, -1)
        );
    }

    #[test]
    fn edge_midpoints_lie_at_apothem_distance() {
        let size = 3.0;
        let apothem = size * SQRT_3 * 0.5;
        let center = (5.0, -7.0);
        for edge in HexEdge::ALL {
            let (mx, my) = edge_midpoint(center, edge, size);
            let dx = mx - center.0;
            let dy = my - center.1;
            let d = (dx * dx + dy * dy).sqrt();
            assert!(
                approx_eq(d, apothem, 1e-5),
                "edge {edge:?} midpoint distance {d} != apothem {apothem}"
            );
        }
    }

    #[test]
    fn east_edge_midpoint_is_pure_east() {
        let size = 2.0;
        let (mx, my) = edge_midpoint((0.0, 0.0), HexEdge::E, size);
        assert!(approx_eq(mx, size * SQRT_3 * 0.5, 1e-5));
        assert!(approx_eq(my, 0.0, 1e-5));
    }

    #[test]
    fn west_edge_midpoint_is_pure_west() {
        let size = 2.0;
        let (mx, my) = edge_midpoint((0.0, 0.0), HexEdge::W, size);
        assert!(approx_eq(mx, -size * SQRT_3 * 0.5, 1e-5));
        assert!(approx_eq(my, 0.0, 1e-5));
    }

    #[test]
    fn polygon_vertices_at_radius_and_6_count() {
        let size = 4.0;
        let center = (1.0, 2.0);
        let verts = hex_polygon_vertices(center, size);
        assert_eq!(verts.len(), 6);
        for (i, (vx, vy)) in verts.iter().enumerate() {
            let dx = vx - center.0;
            let dy = vy - center.1;
            let d = (dx * dx + dy * dy).sqrt();
            assert!(
                approx_eq(d, size, 1e-5),
                "vertex {i} distance {d} != hex_size {size}"
            );
        }
    }

    #[test]
    fn polygon_vertices_ccw_and_start_between_e_and_ne() {
        // Vertex 0 should be at angle 30° (between E edge at 0° and NE edge
        // at 60°). Subsequent vertices are CCW at 60° steps.
        let size = 1.0;
        let verts = hex_polygon_vertices((0.0, 0.0), size);
        for (k, (vx, vy)) in verts.iter().enumerate() {
            let expected_angle = FRAC_PI_6 + (k as f32) * FRAC_PI_3;
            assert!(
                approx_eq(*vx, size * expected_angle.cos(), 1e-5),
                "vertex {k} x"
            );
            assert!(
                approx_eq(*vy, size * expected_angle.sin(), 1e-5),
                "vertex {k} y"
            );
        }
    }
}
