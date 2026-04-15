//! Neighborhood connectivity kinds used by Sprint 1A terrain/hydro stages.
//!
//! Two connectivity modes appear in the hydrology and coast-detection
//! algorithms.  Encoding them as a small enum (rather than bare `bool`
//! or integer) makes call-sites self-documenting.

/// Which cells are considered "neighbours" for a grid operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Neighborhood {
    /// 4-connected (von Neumann): N / E / S / W only.
    Von4,
    /// 8-connected (Moore): N / NE / E / SE / S / SW / W / NW.
    Moore8,
}

// ─── Sprint 1A constants ──────────────────────────────────────────────────────

/// Neighborhood used when deciding whether a land cell touches the sea
/// (coast detection in `CoastMaskStage`).
pub const COAST_DETECT_NEIGHBORHOOD: Neighborhood = Neighborhood::Von4;

/// Neighborhood used for connected-component labelling of the river network
/// in `RiverExtractionStage`.
pub const RIVER_CC_NEIGHBORHOOD: Neighborhood = Neighborhood::Moore8;

/// Neighborhood used when testing whether a river cell touches the coast
/// (river-mouth detection in `RiverExtractionStage`).
pub const RIVER_COAST_CONTACT: Neighborhood = Neighborhood::Moore8;

/// Neighbour offsets for the given [`Neighborhood`] kind.
///
/// Returns a `'static` slice of `(dx, dy)` pairs. Von4 uses N/E/S/W order;
/// Moore8 uses row-major order omitting the centre.
///
/// `#[rustfmt::skip]` preserves the 3×3 grid layout of the Moore8 arm — the
/// visual hole at the centre documents "skip self" and is load-bearing for
/// readability of the hydro/coast-detection code that depends on this table.
#[rustfmt::skip]
pub const fn neighbour_offsets(kind: Neighborhood) -> &'static [(i32, i32)] {
    match kind {
        Neighborhood::Von4 => &[(0, -1), (1, 0), (0, 1), (-1, 0)],
        Neighborhood::Moore8 => &[
            (-1, -1), (0, -1), (1, -1),
            (-1,  0),          (1,  0),
            (-1,  1), (0,  1), (1,  1),
        ],
    }
}
