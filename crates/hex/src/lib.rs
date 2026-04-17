//! Hex grid utilities — coordinate math and `hex_id_of_cell` builder.
//!
//! Storage types ([`island_core::world::HexGrid`],
//! [`island_core::world::HexAttributes`],
//! [`island_core::world::HexAttributeField`]) live in `core::world`
//! next to the other derived-cache types so the `DerivedCaches`
//! struct can hold them without a `core → hex` back edge. This crate
//! provides pure math helpers and the builder function that
//! constructs a `HexGrid` from a sim-resolution domain.

use island_core::field::ScalarField2D;
use island_core::world::{HexGrid, HexLayout};

/// Axial hex coordinate `(q, r)` for the flat-top orientation
/// (`q` grows east, `r` grows south-east).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AxialCoord {
    pub q: i32,
    pub r: i32,
}

impl AxialCoord {
    pub const fn new(q: i32, r: i32) -> Self {
        Self { q, r }
    }
}

/// Offset hex coordinate `(col, row)` — the indexing used by
/// `HexAttributeField` and `hex_id_of_cell`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OffsetCoord {
    pub col: u32,
    pub row: u32,
}

impl OffsetCoord {
    pub const fn new(col: u32, row: u32) -> Self {
        Self { col, row }
    }
}

/// Build a 64×64 (or `cols × rows`) flat-top hex grid for a
/// simulation-resolution domain, plus the `hex_id_of_cell` lookup.
///
/// The mapping from sim cell → hex is an axis-aligned tiling: each
/// hex covers `(sim_width / cols) × (sim_height / rows)` sim cells,
/// and the cell→hex function is just `hex_col = ix / sim_per_hex_x`,
/// `hex_row = iy / sim_per_hex_y`. This is deliberately a box
/// tessellation, not a true hexagonal Voronoi — v1 ships a
/// "rectangular hex index grid" to keep the aggregation kernel a
/// trivial O(sim_cells) scatter and to defer the subtle
/// axial-to-pixel math until Sprint 5's hex-only view needs it.
/// Sprint 1B overlay #12 shows the hex cells with a box outline,
/// so the axis-aligned tessellation matches what the player sees.
pub fn build_hex_grid(cols: u32, rows: u32, sim_width: u32, sim_height: u32) -> HexGrid {
    assert!(cols > 0 && rows > 0, "hex grid dimensions must be positive");
    assert!(
        sim_width > 0 && sim_height > 0,
        "sim dimensions must be positive"
    );

    let mut hex_id_of_cell = ScalarField2D::<u32>::new(sim_width, sim_height);
    for iy in 0..sim_height {
        for ix in 0..sim_width {
            let col = ((ix as u64 * cols as u64) / sim_width as u64) as u32;
            let row = ((iy as u64 * rows as u64) / sim_height as u64) as u32;
            let hex_id = row * cols + col;
            hex_id_of_cell.set(ix, iy, hex_id);
        }
    }

    let hex_size = (sim_width as f32 / cols as f32).min(sim_height as f32 / rows as f32);

    HexGrid {
        cols,
        rows,
        hex_size,
        layout: HexLayout::FlatTop,
        hex_id_of_cell,
    }
}

/// Decompose a flat `hex_id` into `(col, row)` using the grid's `cols`.
#[inline]
pub fn hex_id_to_offset(grid: &HexGrid, hex_id: u32) -> OffsetCoord {
    OffsetCoord {
        col: hex_id % grid.cols,
        row: hex_id / grid.cols,
    }
}

/// Number of hex cells in the grid.
#[inline]
pub fn hex_count(grid: &HexGrid) -> usize {
    (grid.cols * grid.rows) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_divides_domain_exactly() {
        let grid = build_hex_grid(4, 4, 16, 16);
        assert_eq!(grid.cols, 4);
        assert_eq!(grid.rows, 4);
        assert_eq!(grid.hex_id_of_cell.width, 16);
        assert_eq!(grid.hex_id_of_cell.height, 16);
    }

    #[test]
    fn cell_0_0_maps_to_hex_0() {
        let grid = build_hex_grid(4, 4, 16, 16);
        assert_eq!(grid.hex_id_of_cell.get(0, 0), 0);
        assert_eq!(grid.hex_id_of_cell.get(3, 3), 0);
    }

    #[test]
    fn cell_last_maps_to_last_hex() {
        let grid = build_hex_grid(4, 4, 16, 16);
        // Last hex id = rows*cols - 1 = 15.
        assert_eq!(grid.hex_id_of_cell.get(15, 15), 15);
    }

    #[test]
    fn uniform_coverage_per_hex() {
        // Every hex should cover exactly 4×4 = 16 sim cells for a
        // 16×16 domain with a 4×4 hex grid.
        let grid = build_hex_grid(4, 4, 16, 16);
        let mut counts = vec![0_u32; hex_count(&grid)];
        for iy in 0..16 {
            for ix in 0..16 {
                counts[grid.hex_id_of_cell.get(ix, iy) as usize] += 1;
            }
        }
        for (i, c) in counts.iter().enumerate() {
            assert_eq!(*c, 16, "hex {i} covers {c} cells, expected 16");
        }
    }

    #[test]
    fn non_square_domain() {
        // 32×16 sim on a 4×4 hex grid: each hex covers 8×4 cells.
        let grid = build_hex_grid(4, 4, 32, 16);
        let mut counts = vec![0_u32; hex_count(&grid)];
        for iy in 0..16 {
            for ix in 0..32 {
                counts[grid.hex_id_of_cell.get(ix, iy) as usize] += 1;
            }
        }
        for (i, c) in counts.iter().enumerate() {
            assert_eq!(*c, 32, "hex {i} covers {c} cells, expected 32");
        }
    }

    #[test]
    fn hex_id_roundtrip() {
        let grid = build_hex_grid(8, 4, 64, 32);
        let coord = hex_id_to_offset(&grid, 9);
        assert_eq!(coord, OffsetCoord::new(1, 1));
    }

    #[test]
    fn axial_coord_constructor() {
        let c = AxialCoord::new(-3, 5);
        assert_eq!(c.q, -3);
        assert_eq!(c.r, 5);
    }
}
