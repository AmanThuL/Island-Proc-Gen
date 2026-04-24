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

use crate::{AxialCoord, OffsetCoord};

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
// Sim-space / world-space bridge (c8)
// ─────────────────────────────────────────────────────────────────────────────

/// Sim-space → world-space scale factor for DD2 hex grids.
///
/// The DD2 hex kernel in `build_hex_grid` produces `HexGrid.hex_size` and
/// `hex_id_of_cell` positions in **sim-resolution units** (one unit = one
/// sim cell). Terrain rendering uses **world-space units** `[0, extent]`
/// (see `crates/render/src/terrain.rs::build_terrain_mesh`).
///
/// This helper returns the multiplicative scale factor that converts a
/// sim-space coordinate (centre, size, distance) to its world-space
/// equivalent, matching terrain's per-cell step **exactly**:
/// `world = sim * (extent / (sim_width - 1))`.
///
/// Rationale for `(sim_width - 1)` over `sim_width`:
/// terrain.rs's mesh builder places vertex at sim index 0 at world x=0 and
/// vertex at sim index `sim_width - 1` at world x=extent — so per-cell step
/// = `extent / (sim_width - 1)`. Matching this exactly gives hex edges that
/// align with terrain cell corners within the rendered region. Using
/// `extent / sim_width` would produce a ~0.8% (~10 px at 1280w) systematic
/// rim offset between hex overlay and terrain mesh.
///
/// Edge case: when `sim_width <= 1` the formula degenerates; we clamp the
/// divisor to 1 to avoid division by zero, which is only reachable on
/// degenerate 1×1 test grids.
///
/// Callers that position hex instances in world space should apply this
/// to both [`offset_to_pixel`] results (hex centres) AND to `hex_size`
/// before passing to `HexSurfaceRenderer::update_view_projection`.
///
/// # Example
///
/// ```
/// use hex::geometry::sim_to_world_scale;
///
/// // 128² sim domain, 5.0 world extent (DEFAULT_WORLD_XZ_EXTENT).
/// // Matches terrain's 1/(W-1) cell step exactly.
/// let scale = sim_to_world_scale(128, 5.0);
/// assert!((scale - 5.0 / 127.0).abs() < 1e-6);
/// ```
#[inline]
pub fn sim_to_world_scale(sim_width: u32, world_extent: f32) -> f32 {
    let denom = (sim_width.max(2) - 1) as f32;
    world_extent / denom
}

// ─────────────────────────────────────────────────────────────────────────────
// Offset-coord ↔ pixel conversion (DD2 odd-r-offset convention)
// ─────────────────────────────────────────────────────────────────────────────

/// The canonical origin for the DD2 sim-aligned hex grid in **sim space**.
///
/// Returns `(0.5 * hex_width, 0.5 * row_spacing)`, which is the **sim-space**
/// centre of hex `(col=0, row=0)` as placed by `build_hex_grid`. Pass this
/// as the `origin` argument to [`offset_to_pixel`] and [`pixel_to_offset`]
/// to work in the same coordinate frame as the aggregation kernel.
///
/// The returned coordinates are in **sim-space units** (one unit = one sim
/// cell). Multiply by [`sim_to_world_scale`] to convert to world-space
/// units `[0, extent]`.
///
/// ## Derivation
///
/// `build_hex_grid` places hex `(col, row)` at:
/// ```text
/// x = (col + 0.5) * hex_width + (row & 1) * 0.5 * hex_width
/// y = (row + 0.5) * row_spacing
/// ```
/// At `(col=0, row=0)` this gives `x = 0.5 * hex_width`, `y = 0.5 * row_spacing`.
#[inline]
pub fn default_grid_origin(hex_size: f32) -> (f32, f32) {
    let hex_width = hex_size * SQRT_3;
    let row_spacing = hex_size * 1.5;
    (0.5 * hex_width, 0.5 * row_spacing)
}

/// Convert an offset coord `(col, row)` to the **sim-space** pixel centre of
/// its hex under the DD2 odd-r-offset convention used by `build_hex_grid`.
///
/// Odd rows (`row & 1 == 1`) are shifted horizontally by `+0.5 * hex_width`
/// relative to even rows, matching `build_hex_grid`'s Voronoi construction.
///
/// Unlike [`axial_to_pixel`] (which puts axial origin `(0, 0)` at pixel
/// `(0, 0)`), this function places hex `(col=0, row=0)` at `origin`.  For
/// the DD2 sim-aligned grid, pass `default_grid_origin(hex_size)`.
///
/// The returned coordinates are in **sim-space units** (one unit = one sim
/// cell). To convert to world space — the `[0, extent]` coordinate frame used
/// by the terrain renderer — multiply both components by
/// [`sim_to_world_scale`]`(sim_width, extent)`.
///
/// ## Formula (matches `build_hex_grid`)
///
/// ```text
/// hex_width  = hex_size * SQRT_3
/// row_spacing = hex_size * 1.5
/// row_x_offset = (row & 1) as f32 * 0.5 * hex_width
/// pixel_x = origin.0 + col as f32 * hex_width + row_x_offset
/// pixel_y = origin.1 + row as f32 * row_spacing
/// ```
///
/// This is the offset-coord counterpart to [`axial_to_pixel`]; use it whenever
/// you need sim-space positions from DD2-produced `(col, row)` indices.
/// For rendering and per-instance buffer population, apply the
/// [`sim_to_world_scale`] factor after this call.
#[inline]
pub fn offset_to_pixel(col: u32, row: u32, hex_size: f32, origin: (f32, f32)) -> (f32, f32) {
    let hex_width = hex_size * SQRT_3;
    let row_spacing = hex_size * 1.5;
    let row_x_offset = (row & 1) as f32 * 0.5 * hex_width;
    let px = origin.0 + col as f32 * hex_width + row_x_offset;
    let py = origin.1 + row as f32 * row_spacing;
    (px, py)
}

/// Convert a **sim-space** pixel position to the DD2-offset `(col, row)` of
/// the nearest hex.
///
/// The `(px, py)` argument must be in the same **sim-space units** (one unit =
/// one sim cell) as the values produced by [`offset_to_pixel`]. If you have a
/// world-space point from the terrain renderer, divide by
/// [`sim_to_world_scale`]`(sim_width, extent)` first.
///
/// Returns `None` when the nearest hex falls outside `[0, cols) × [0, rows)`;
/// there is no silent out-of-range clamping.
///
/// Internally uses [`pixel_to_axial`] after subtracting `origin`, then
/// converts axial `(q, r)` → odd-r-offset with the standard formula:
/// `col = q + (r - (r & 1)) / 2`, `row = r`.  This is the exact inverse of
/// the offset → axial direction used by [`offset_to_pixel`], so the two
/// functions round-trip cleanly at hex centres.
///
/// # Boundary behaviour
///
/// `pixel_to_axial` uses cube-rounding, so any pixel inside a hex's Voronoi
/// polygon maps to that hex's offset coord.  Points exactly on a shared
/// boundary may land on either neighbour — this matches `build_hex_grid`'s
/// own tie-break (deterministic but not specified which side).
pub fn pixel_to_offset(
    px: f32,
    py: f32,
    hex_size: f32,
    origin: (f32, f32),
    cols: u32,
    rows: u32,
) -> Option<OffsetCoord> {
    // Shift into the axial frame where axial (0,0) = DD2 (col=0, row=0).
    let ax = pixel_to_axial(px - origin.0, py - origin.1, hex_size);

    // Convert axial (q, r) → odd-r offset.
    // Formula: col = q + (r - (r & 1)) / 2,  row = r.
    // For non-negative r: (r - (r & 1)) / 2 == r / 2 (integer).
    // Negative r means out-of-bounds, caught by the range check below.
    let r = ax.r;
    let q = ax.q;
    // Avoid panic on negative values — convert to i64 for the arithmetic.
    let col_i = q as i64 + (r as i64 - ((r & 1) as i64)) / 2;
    let row_i = r as i64;

    if col_i < 0 || row_i < 0 || col_i >= cols as i64 || row_i >= rows as i64 {
        return None;
    }
    Some(OffsetCoord::new(col_i as u32, row_i as u32))
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

    // ── Offset-coord ↔ pixel tests ────────────────────────────────────────────

    /// Core parity lock: `offset_to_pixel(col, row, ...)` must exactly match
    /// the DD2 kernel's hex-centre formula used in `build_hex_grid`.
    ///
    /// For every cell in a 4×4 grid, compute the expected centre from the DD2
    /// formula directly and compare against `offset_to_pixel` output.
    #[test]
    fn offset_to_pixel_matches_build_hex_grid_centres() {
        let hex_size = 1.0_f32;
        let hex_width = hex_size * SQRT_3;
        let row_spacing = hex_size * 1.5;
        let origin = default_grid_origin(hex_size);

        for row in 0_u32..4 {
            for col in 0_u32..4 {
                // Reference formula directly from `build_hex_grid` comments:
                // x = (col + 0.5) * hex_width + (row & 1) * 0.5 * hex_width
                // y = (row + 0.5) * row_spacing
                let expected_x =
                    (col as f32 + 0.5) * hex_width + (row & 1) as f32 * 0.5 * hex_width;
                let expected_y = (row as f32 + 0.5) * row_spacing;

                let (px, py) = offset_to_pixel(col, row, hex_size, origin);
                assert!(
                    approx_eq(px, expected_x, 1e-5),
                    "col={col} row={row}: offset_to_pixel x={px} != build_hex_grid formula x={expected_x}"
                );
                assert!(
                    approx_eq(py, expected_y, 1e-5),
                    "col={col} row={row}: offset_to_pixel y={py} != build_hex_grid formula y={expected_y}"
                );
            }
        }
    }

    /// Odd-row hexes must be shifted east by exactly half a hex_width compared
    /// to their same-column even-row neighbour.
    #[test]
    fn offset_to_pixel_odd_row_shifted_right_by_half_hex_width() {
        let hex_size = 2.0_f32;
        let hex_width = hex_size * SQRT_3;
        let origin = default_grid_origin(hex_size);

        // Compare (col=0, row=0) and (col=0, row=1).
        let (x_even, _) = offset_to_pixel(0, 0, hex_size, origin);
        let (x_odd, _) = offset_to_pixel(0, 1, hex_size, origin);
        let expected_shift = 0.5 * hex_width;
        assert!(
            approx_eq(x_odd - x_even, expected_shift, 1e-5),
            "odd-row x_shift = {} expected 0.5 * hex_width = {}",
            x_odd - x_even,
            expected_shift
        );
    }

    /// Round-trip: `pixel_to_offset(offset_to_pixel(col, row, ...), ...)` must
    /// recover the original `(col, row)` for all cells in a 4×4 grid.
    #[test]
    fn pixel_to_offset_round_trips_through_offset_to_pixel() {
        use crate::OffsetCoord;
        let hex_size = 1.5_f32;
        let origin = default_grid_origin(hex_size);
        let cols = 4_u32;
        let rows = 4_u32;

        for row in 0..rows {
            for col in 0..cols {
                let (px, py) = offset_to_pixel(col, row, hex_size, origin);
                let result = pixel_to_offset(px, py, hex_size, origin, cols, rows);
                assert_eq!(
                    result,
                    Some(OffsetCoord::new(col, row)),
                    "round-trip failed for (col={col}, row={row}): got {result:?}"
                );
            }
        }
    }

    // ── sim_to_world_scale ────────────────────────────────────────────────────

    /// For `sim_width=128` and `world_extent=5.0`, `sim_to_world_scale` must
    /// return exactly `5.0 / 128 = 0.0390625` (within floating-point tolerance).
    ///
    /// This locks the c8 bridge formula: `world = sim * (extent / (sim_width - 1))`,
    /// matching terrain.rs's per-cell step exactly (vertices at sim 0 and sim W-1
    /// map to world 0 and world extent, so per-step = extent/(W-1)).
    #[test]
    fn sim_to_world_scale_matches_terrain_extent_ratio() {
        let scale = sim_to_world_scale(128, 5.0);
        let expected = 5.0_f32 / 127.0_f32;
        assert!(
            (scale - expected).abs() < 1e-6,
            "sim_to_world_scale(128, 5.0) = {scale}, expected {expected} \
             (terrain uses 1/(W-1) cell step; scale formula must match)"
        );
    }

    /// Degenerate sim_width ≤ 1 must not panic on divide-by-zero.
    #[test]
    fn sim_to_world_scale_clamps_degenerate_domain() {
        // Values here are not physically meaningful — we just confirm no panic
        // and that the clamp kicks in via the `.max(2) - 1` guard.
        let s0 = sim_to_world_scale(0, 4.0);
        let s1 = sim_to_world_scale(1, 4.0);
        assert!(s0.is_finite());
        assert!(s1.is_finite());
        // Both clamp to the same divisor = 1.
        assert!((s0 - s1).abs() < 1e-6);
    }

    /// Pixels far outside the grid bounds must return `None`.
    #[test]
    fn pixel_to_offset_out_of_range_returns_none() {
        let hex_size = 1.0_f32;
        let origin = default_grid_origin(hex_size);
        let cols = 4_u32;
        let rows = 4_u32;

        // Far to the right (well outside col < 4).
        assert_eq!(
            pixel_to_offset(1000.0, 1.0, hex_size, origin, cols, rows),
            None,
            "large x should be out of range"
        );
        // Far below (well outside row < 4).
        assert_eq!(
            pixel_to_offset(1.0, 1000.0, hex_size, origin, cols, rows),
            None,
            "large y should be out of range"
        );
        // Negative x (left of col 0).
        assert_eq!(
            pixel_to_offset(-100.0, 1.0, hex_size, origin, cols, rows),
            None,
            "negative x should be out of range"
        );
        // Negative y (above row 0).
        assert_eq!(
            pixel_to_offset(1.0, -100.0, hex_size, origin, cols, rows),
            None,
            "negative y should be out of range"
        );
    }

    // ── Sprint 3.5.E — pick edge-case coverage ──

    /// Hex vertices are equidistant between 3 hex neighbours. `pixel_to_axial`
    /// uses cube-rounding, which must choose one of the 3 valid neighbours
    /// deterministically (no NaN, no panic). Calling twice must return the
    /// same value.
    #[test]
    fn pixel_to_axial_at_hex_vertex_is_deterministic() {
        // Vertex between E and NE of hex (0,0): angle π/6 from centre,
        // radius = hex_size. Coordinates: (cos(π/6), sin(π/6)) = (√3/2, 0.5).
        let hex_size = 1.0_f32;
        let vx = hex_size * FRAC_PI_6.cos(); // √3/2
        let vy = hex_size * FRAC_PI_6.sin(); // 0.5

        let first = pixel_to_axial(vx, vy, hex_size);
        let second = pixel_to_axial(vx, vy, hex_size);
        // Must be one of the 3 hex corners sharing this vertex: (0,0) itself,
        // E neighbour (1,0), NE neighbour (1,-1). (NE is the mirror of SW=(-1,1)
        // per the axial_to_pixel_matches_known_neighbors lock.)
        assert!(
            (first.q == 0 && first.r == 0)
                || (first.q == 1 && first.r == 0)
                || (first.q == 1 && first.r == -1),
            "vertex landed on unexpected hex ({}, {})",
            first.q,
            first.r
        );
        // Determinism: two calls with identical input must agree.
        assert_eq!(
            first, second,
            "pixel_to_axial must be deterministic at a vertex"
        );
    }

    /// An edge midpoint is shared by exactly 2 hexes. `pixel_to_axial` must
    /// return one of them deterministically — the doc comment contracts
    /// "deterministic but not specified which side".
    #[test]
    fn pixel_to_axial_at_shared_edge_midpoint_is_deterministic() {
        let hex_size = 1.0_f32;
        // Midpoint of the E edge of hex (0,0) — on the boundary between (0,0) and (1,0).
        let (mx, my) = edge_midpoint((0.0, 0.0), HexEdge::E, hex_size);

        let first = pixel_to_axial(mx, my, hex_size);
        let second = pixel_to_axial(mx, my, hex_size);
        // Must be one of the two hexes sharing this edge.
        assert!(
            (first.q == 0 && first.r == 0) || (first.q == 1 && first.r == 0),
            "edge midpoint landed on unexpected hex ({}, {})",
            first.q,
            first.r
        );
        assert_eq!(
            first, second,
            "pixel_to_axial must be deterministic at an edge midpoint"
        );
    }

    /// A world-space point above row 0 resolves to a negative axial `r`. The
    /// function must return `None` cleanly — no `as u32` cast panic, no
    /// index underflow.
    #[test]
    fn pixel_to_offset_rejects_negative_axial_without_panic() {
        let hex_size = 1.0_f32;
        let origin = default_grid_origin(hex_size);
        // Subtract ~2 row-spacings upward from origin to land at r ≈ -2.
        let above_grid_y = origin.1 - 2.0 * hex_size * 1.5;
        let result = pixel_to_offset(origin.0, above_grid_y, hex_size, origin, 4, 4);
        assert_eq!(result, None, "negative axial r must return None, not panic");
    }

    /// A zero-dimension grid (`cols=0` or `rows=0`) must return `None` for any
    /// input — protects the click-handler path against a race with resolution
    /// changes.
    #[test]
    fn pixel_to_offset_on_degenerate_grid_returns_none() {
        let hex_size = 1.0_f32;
        let origin = default_grid_origin(hex_size);
        assert_eq!(
            pixel_to_offset(0.5, 0.5, hex_size, origin, 0, 4),
            None,
            "cols=0 must return None"
        );
        assert_eq!(
            pixel_to_offset(0.5, 0.5, hex_size, origin, 4, 0),
            None,
            "rows=0 must return None"
        );
    }

    /// The odd-row right-edge half-shift makes the rightmost column
    /// boundary fragile. A point one hex-width past the last odd-row hex
    /// centre must return `None`, not wrap or silently clamp.
    #[test]
    fn pixel_to_offset_just_past_odd_row_right_edge_returns_none() {
        let hex_size = 1.0_f32;
        let hex_width = hex_size * SQRT_3;
        let origin = default_grid_origin(hex_size);
        let cols = 4_u32;
        let rows = 4_u32;

        // Centre of rightmost odd-row hex (col=3, row=1).
        let (px, py) = offset_to_pixel(3, 1, hex_size, origin);
        // Shift just past the right boundary of col=3 in the odd row.
        let past_right = px + hex_width * 1.01;

        assert_eq!(
            pixel_to_offset(past_right, py, hex_size, origin, cols, rows),
            None,
            "point just past odd-row right edge must return None"
        );
    }

    /// Round-trip at a representative shipping hex_size (10.0 sim units,
    /// in the range produced by HexProjectionStage at 128² / 64-hex target).
    /// Verifies: forward produces finite non-zero output; inverse recovers
    /// the original coord; a point 0.9 steps toward SE still round-trips.
    #[test]
    fn pixel_to_offset_round_trips_at_shipping_hex_size() {
        let hex_size = 10.0_f32;
        let origin = default_grid_origin(hex_size);
        let cols = 8_u32;
        let rows = 8_u32;

        let (px, py) = offset_to_pixel(2, 3, hex_size, origin);
        assert!(
            px.is_finite() && px != 0.0 && py.is_finite() && py != 0.0,
            "forward (px, py) = ({px}, {py}) must both be finite and non-zero"
        );

        // Exact centre round-trips.
        assert_eq!(
            pixel_to_offset(px, py, hex_size, origin, cols, rows),
            Some(OffsetCoord::new(2, 3)),
            "exact centre must round-trip"
        );

        // A point 0.9 × hex_size toward SE (half-step, safely inside the polygon).
        let diag_x = px + 0.9 * hex_size * (SQRT_3 / 2.0) * 0.5;
        let diag_y = py + 0.9 * hex_size * 1.5 * 0.5;
        assert_eq!(
            pixel_to_offset(diag_x, diag_y, hex_size, origin, cols, rows),
            Some(OffsetCoord::new(2, 3)),
            "point 0.9 half-SE-step from centre must still map to (2,3)"
        );
    }
}
