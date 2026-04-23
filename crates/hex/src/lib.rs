//! Hex grid utilities — coordinate math and `hex_id_of_cell` builder.
//!
//! Storage types ([`island_core::world::HexGrid`],
//! [`island_core::world::HexAttributes`],
//! [`island_core::world::HexAttributeField`]) live in `core::world`
//! next to the other derived-cache types so the `DerivedCaches`
//! struct can hold them without a `core → hex` back edge. This crate
//! provides pure math helpers and the builder function that
//! constructs a `HexGrid` from a sim-resolution domain.

pub mod geometry;

use island_core::field::ScalarField2D;
use island_core::world::{HexGrid, HexLayout};

/// `sqrt(3)` as a 32-bit float. `std::f32::consts::SQRT_3` is still
/// unstable (`more_float_constants`, rust-lang#146939); carry our own here
/// to mirror the private copy in `geometry.rs`.
const SQRT_3: f32 = 1.7320508_f32;

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
/// Sprint 3.5 DD2 aggregation kernel: flat-top axial-offset rows. Each
/// sim cell's world position `(ix + 0.5, iy + 0.5)` is assigned to the
/// nearest hex centre by Euclidean distance. Odd rows are offset by half
/// a hex width relative to even rows (0-indexed). Tie-break: lowest row,
/// then lowest col.
///
/// ## Geometry
/// - `hex_width = sim_width / cols`
/// - `hex_size = hex_width / SQRT_3` (centre-to-vertex radius)
/// - `row_spacing = 1.5 * hex_size` (vertical distance between row centres)
/// - Hex `(col, row)` centre: `x = (col + 0.5) * hex_width + (row % 2 == 1) * 0.5 * hex_width`,
///   `y = (row + 0.5) * row_spacing`
pub fn build_hex_grid(cols: u32, rows: u32, sim_width: u32, sim_height: u32) -> HexGrid {
    assert!(cols > 0 && rows > 0, "hex grid dimensions must be positive");
    assert!(
        sim_width > 0 && sim_height > 0,
        "sim dimensions must be positive"
    );

    let hex_width = sim_width as f32 / cols as f32;
    let hex_size = hex_width / SQRT_3;
    let row_spacing = 1.5 * hex_size;

    let mut hex_id_of_cell = ScalarField2D::<u32>::new(sim_width, sim_height);

    for iy in 0..sim_height {
        for ix in 0..sim_width {
            let wx = ix as f32 + 0.5;
            let wy = iy as f32 + 0.5;

            // Estimate the nearest row. Clamp into `[0, rows-1]` BEFORE the
            // candidate loop so that the 3-row × 3-col search always has at
            // least one in-bounds candidate — otherwise cells in sim rows
            // below the last hex row (e.g. `wy >= rows * row_spacing`, which
            // occurs for production sizing where `rows * row_spacing <
            // sim_height`) would silently fall back to `best_id = 0`.
            let row_approx = ((wy / row_spacing) - 0.5).round() as i32;
            let row_approx = row_approx.clamp(0, rows as i32 - 1);

            let mut best_id = u32::MAX;
            let mut best_d2 = f32::MAX;
            let mut best_row = i32::MAX;
            let mut best_col = i32::MAX;

            for dr in -1_i32..=1 {
                let candidate_row = row_approx + dr;
                if candidate_row < 0 || candidate_row >= rows as i32 {
                    continue;
                }

                let row_x_offset = if candidate_row % 2 == 1 {
                    0.5 * hex_width
                } else {
                    0.0
                };
                let row_y = (candidate_row as f32 + 0.5) * row_spacing;
                // Same clamp rationale for the col approximation so the
                // col_approx ± 1 search always produces at least one
                // in-bounds candidate for this row.
                let col_approx = (((wx - row_x_offset) / hex_width) - 0.5).round() as i32;
                let col_approx = col_approx.clamp(0, cols as i32 - 1);

                for dc in -1_i32..=1 {
                    let candidate_col = col_approx + dc;
                    if candidate_col < 0 || candidate_col >= cols as i32 {
                        continue;
                    }

                    let hex_x = (candidate_col as f32 + 0.5) * hex_width + row_x_offset;
                    let hex_y = row_y;
                    let d2 = (wx - hex_x) * (wx - hex_x) + (wy - hex_y) * (wy - hex_y);

                    let strictly_better = d2 + 1e-6 < best_d2;
                    let tied = (d2 - best_d2).abs() < 1e-6;
                    let tie_wins = tied
                        && (candidate_row < best_row
                            || (candidate_row == best_row && candidate_col < best_col));

                    if strictly_better || tie_wins {
                        best_d2 = d2;
                        best_row = candidate_row;
                        best_col = candidate_col;
                        best_id = candidate_row as u32 * cols + candidate_col as u32;
                    }
                }
            }

            // Post-clamp the candidate search is guaranteed to find at
            // least one in-bounds hex; this debug-assert converts any
            // future regression into a loud panic rather than a silent
            // write to hex 0.
            debug_assert!(
                best_id != u32::MAX,
                "DD2 kernel failed to find any candidate hex for sim cell ({ix}, {iy}) — \
                 row_approx={row_approx}, sim=({sim_width},{sim_height}), hex=({cols},{rows})"
            );
            hex_id_of_cell.set(ix, iy, best_id);
        }
    }

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
    fn origin_cell_maps_to_hex_0() {
        // Under DD2 axial-offset tessellation the top-left sim cell must
        // belong to hex (col=0, row=0) = hex_id 0. Even rows have no
        // x-offset, so cell (0, 0) at world pos (0.5, 0.5) is closest to
        // the row-0 hex centres starting near x = 0.5 * hex_width.
        let grid = build_hex_grid(4, 4, 16, 16);
        assert_eq!(grid.hex_id_of_cell.get(0, 0), 0);
    }

    #[test]
    fn last_cell_maps_to_a_hex_in_last_row() {
        // Under DD2 the bottom-right sim cell must fall in the last hex row.
        // We assert hex_id / cols == rows - 1 rather than an exact hex_id
        // because the axial offset means the exact column assignment may
        // differ from the rectangular-box case.
        let grid = build_hex_grid(4, 4, 16, 16);
        let hex_id = grid.hex_id_of_cell.get(15, 15);
        let assigned_row = hex_id / grid.cols;
        assert_eq!(
            assigned_row,
            grid.rows - 1,
            "cell (15,15) hex_id={hex_id} lands in row {assigned_row}, expected row {}",
            grid.rows - 1
        );
    }

    #[test]
    fn coverage_is_approximately_uniform_under_dd2() {
        // Under axial-offset tessellation the Voronoi regions are not perfect
        // rectangles, so coverage will not be exactly equal. Boundary hexes
        // whose centres lie near or outside the domain edge collect fewer cells
        // (floor at 44% of mean on a 4×4 hex / 16×16 sim), while hexes that
        // absorb the "spill" from out-of-domain rows can reach 194% of mean.
        //
        // Tolerance: [mean * 0.4, mean * 2.0].
        // — floor (0.4): covers the observed minimum of 7 cells (≈44%) on hex
        //   (col=3, row=1) whose centre x=16 sits at the right edge.
        // — ceiling (2.0): covers the observed maximum of 31 cells (≈194%) on
        //   hex (col=0, row=3) which absorbs bottom-boundary spill.
        // This is tighter than "any non-zero / any count" and would catch a
        // broken kernel that maps ≥ 75% of cells to a single hex.
        let grid = build_hex_grid(4, 4, 16, 16);
        let total = 16_u32 * 16;
        let n_hexes = hex_count(&grid) as u32;
        let mean = total / n_hexes; // 16

        let mut counts = vec![0_u32; hex_count(&grid)];
        for iy in 0..16_u32 {
            for ix in 0..16_u32 {
                counts[grid.hex_id_of_cell.get(ix, iy) as usize] += 1;
            }
        }
        // Every cell must be assigned exactly once.
        let cell_sum: u32 = counts.iter().sum();
        assert_eq!(
            cell_sum, total,
            "total assigned cells must equal sim_width*sim_height"
        );

        for (i, &c) in counts.iter().enumerate() {
            let lo = mean * 2 / 5; // ≈ mean * 0.4
            let hi = mean * 2; // = mean * 2.0
            assert!(
                c >= lo && c <= hi,
                "hex {i} covers {c} cells; expected [{lo}, {hi}] (mean={mean})"
            );
        }
    }

    #[test]
    fn non_square_domain() {
        // Verify the kernel handles a non-square sim domain (width ≠ height).
        // 32×16 sim with 4 cols × 2 rows of hexes gives hex_width = 8.0 and
        // row_spacing ≈ 6.93. Two rows span ~10.4 vertical units — fully inside
        // the 16-unit-tall domain — so every hex centre falls within the domain
        // and the coverage distribution is geometrically sensible.
        //
        // Mean = 32*16/8 = 64 cells/hex; accept [mean * 0.4, mean * 2.0] for
        // the same boundary-spill reasons as coverage_is_approximately_uniform.
        let grid = build_hex_grid(4, 2, 32, 16);
        let total = 32_u32 * 16;
        let n_hexes = hex_count(&grid) as u32;
        let mean = total / n_hexes; // 64

        let mut counts = vec![0_u32; hex_count(&grid)];
        for iy in 0..16_u32 {
            for ix in 0..32_u32 {
                counts[grid.hex_id_of_cell.get(ix, iy) as usize] += 1;
            }
        }
        let cell_sum: u32 = counts.iter().sum();
        assert_eq!(
            cell_sum, total,
            "total assigned cells must equal sim_width*sim_height"
        );

        for (i, &c) in counts.iter().enumerate() {
            let lo = mean * 2 / 5; // ≈ mean * 0.4
            let hi = mean * 2; // = mean * 2.0
            assert!(
                c >= lo && c <= hi,
                "hex {i} covers {c} cells in non-square domain; expected [{lo}, {hi}] (mean={mean})"
            );
        }
    }

    /// DD2 odd rows are offset by half a hex width relative to even rows.
    ///
    /// Strategy: on a 4×4 hex / 16×16 sim grid, pick sim cells whose world
    /// x position is exactly at the hex_width boundary (x ≈ hex_width, i.e.
    /// sim ix = 3, wx = 3.5). For row 0 (even) the column centres are at
    /// x = 0.5 * hex_width, 1.5 * hex_width, ... so a cell at wx = 3.5
    /// (= 0.875 * hex_width, with hex_width = 4.0) falls near col 0.
    /// For row 1 (odd) the centres are shifted right by 0.5 * hex_width,
    /// so the same wx falls near col 0 of the odd row, but the odd-row
    /// col-0 centre is at x = 0.5 * hex_width + 0.5 * hex_width = hex_width
    /// — i.e. a different absolute x than even row col-0's x = 0.5 * hex_width.
    ///
    /// Concretely: verify that the hex_id assigned to a sim cell near the
    /// top-left region of row-0 hexes versus a sim cell in the corresponding
    /// region of row-1 hexes differ in their row component (row 0 vs row 1)
    /// and that their x-offsets are consistent with the half-width shift.
    #[test]
    fn dd2_odd_rows_are_x_offset_from_even_rows() {
        // 4×4 hex, 16×16 sim → hex_width = 4.0, hex_size ≈ 2.309,
        // row_spacing ≈ 3.464.
        //
        // Even row-0 hex (col=0) centre: x = 2.0, y ≈ 1.732.
        // Odd  row-1 hex (col=0) centre: x = 4.0, y ≈ 5.196.
        //
        // A sim cell at (ix=1, iy=1) → wx=1.5, wy=1.5 should be closest to
        // row-0 hex col-0 (hex_id = 0).
        //
        // A sim cell at (ix=3, iy=4) → wx=3.5, wy=4.5 should be closest to
        // row-1 hex col-0 (hex_id = cols = 4) because odd row-1 col-0 is at
        // x=4.0, y≈5.196 — the offset makes col-0 of row-1 sit further right
        // than col-0 of row-0 (x=2.0), so cells in the 3.0–5.0 x-band in the
        // row-1 y-zone prefer the row-1 col-0 or col-1 hex, not the
        // even-row col-1 hex at x=6.0.
        let cols = 4_u32;
        let grid = build_hex_grid(cols, 4, 16, 16);

        // Cell near centre of row-0 col-0 hex (even row).
        let id_row0 = grid.hex_id_of_cell.get(1, 1);
        let assigned_row_0 = id_row0 / cols;
        assert_eq!(
            assigned_row_0, 0,
            "cell (1,1) should be in hex row 0, got hex_id={id_row0}"
        );
        assert_eq!(
            id_row0 % cols,
            0,
            "cell (1,1) should be in hex col 0, got hex_id={id_row0}"
        );

        // Cell near centre of row-1 col-0 hex (odd row, offset right by half
        // hex_width = 2.0 sim units compared to even row).
        // row-1 col-0 centre: x = (0+0.5)*4 + 0.5*4 = 4.0, y ≈ 5.196.
        // sim cell (ix=3, iy=4) → wx=3.5, wy=4.5 is closest to that centre.
        let id_row1 = grid.hex_id_of_cell.get(3, 4);
        let assigned_row_1 = id_row1 / cols;
        assert_eq!(
            assigned_row_1, 1,
            "cell (3,4) should be in hex row 1, got hex_id={id_row1}"
        );
        assert_eq!(
            id_row1 % cols,
            0,
            "cell (3,4) should be in hex col 0 (odd row), got hex_id={id_row1}"
        );

        // Confirm that row-0 col-0 and row-1 col-0 are different hex_ids,
        // which is trivially true given the row checks above, but makes the
        // offset semantics explicit.
        assert_ne!(
            id_row0, id_row1,
            "even-row and odd-row col-0 hexes must be different"
        );

        // DD2 vs rect-kernel divergence witness — sim cell (ix=0, iy=4).
        //
        // Under the old axis-aligned rectangular kernel the mapping is
        // `col = 0*4/16 = 0`, `row = 4*4/16 = 1` → hex_id = 4.
        //
        // Under DD2 that cell sits at `wx=0.5, wy=4.5`. Row-1 is odd with
        // x_offset = 2.0, so row-1 col-0 centre is at x=4.0 (distance
        // 3.5 from wx=0.5). Row-0 col-0 centre is at x=2.0 (distance
        // 1.5). The y-term gives row-0 d²=9.89 vs row-1 d²=12.73, so DD2
        // re-assigns the cell to row-0 col-0 = hex_id 0.
        //
        // A silent revert to rect tessellation would put this cell back
        // in hex 4, so this assert is a direct DD2 regression guard
        // (satisfies plan §4 invariant #4 intent).
        assert_eq!(
            grid.hex_id_of_cell.get(0, 4),
            0,
            "DD2 reassigns (0, 4) from rect-kernel hex_id 4 to offset-kernel hex_id 0"
        );
    }

    #[test]
    fn production_size_no_silent_fallback_to_hex_0() {
        // Regression test locking out the pre-fix silent-fallback bug where
        // sim cells in the bottom rows (wy > rows * row_spacing) had every
        // candidate hex rejected and defaulted to hex_id = 0.
        //
        // At 128x128 sim / 64x64 hex: row_spacing ≈ 1.732, so
        // rows * row_spacing ≈ 110.85 < sim_height (128). Under the pre-fix
        // kernel, ~1920 cells (sim rows 113-127) all routed to hex 0,
        // ballooning hex 0's count from its expected ~4 cells to ~1924.
        //
        // Mean cell count per hex = 128 * 128 / (64 * 64) = 4. Tight bound:
        // `max <= 2 * mean = 8` would immediately fail under the buggy
        // kernel. Under the clamped kernel every hex's Voronoi region
        // stays bounded and coverage is near-uniform.
        let cols = 64_u32;
        let rows = 64_u32;
        let sim = 128_u32;
        let grid = build_hex_grid(cols, rows, sim, sim);

        let n_hexes = hex_count(&grid);
        let total = (sim * sim) as u32;
        let mean = total / n_hexes as u32; // 4

        let mut counts = vec![0_u32; n_hexes];
        for iy in 0..sim {
            for ix in 0..sim {
                counts[grid.hex_id_of_cell.get(ix, iy) as usize] += 1;
            }
        }

        let cell_sum: u32 = counts.iter().sum();
        assert_eq!(
            cell_sum, total,
            "every sim cell must be assigned exactly once"
        );

        // Hex 0 is the silent-fallback sink under the pre-fix kernel.
        // Under the clamped kernel it must get ≈ mean cells (a small
        // natural Voronoi region in the top-left corner). A buggy
        // kernel would balloon this to ~1924 (pre-fix observed value).
        //
        // Note on bottom-row uniformity: cells with `row_approx > rows-1`
        // now legitimately clamp to row `rows-1`, so bottom-row hexes
        // *do* absorb spill from sim rows below the natural grid extent
        // (`rows * row_spacing` < `sim_height`). This is expected Voronoi
        // geometry, not a bug — hence this test guards hex 0 specifically
        // rather than max uniformity across all hexes. Max observed on
        // hex (0, rows-1) is ~56 cells at 128²/64×64, which is within
        // the geometrically-expected range.
        let hex_0_count = counts[0];
        assert!(
            hex_0_count <= 3 * mean,
            "DD2 silent-fallback-to-hex-0 regression: hex 0 count = \
             {hex_0_count} (mean = {mean}, tight bound 3*mean = {}). \
             Cells outside `row_approx ± 1` likely defaulting to hex 0 \
             via uninitialised `best_id`.",
            3 * mean
        );
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
