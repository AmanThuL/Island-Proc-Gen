//! Geomorphology simulation stages.
//!
//! Sprint 1A: Task 1A.1 `TopographyStage`, Task 1A.2 `CoastMaskStage`,
//! Task 1A.3 `PitFillStage`, Task 1A.4 `DerivedGeomorphStage`.

mod coastal;
mod derived_geomorph;
mod pit_fill;
mod topography;

pub use coastal::CoastMaskStage;
pub use derived_geomorph::DerivedGeomorphStage;
pub use pit_fill::PitFillStage;
pub use topography::TopographyStage;

use island_core::neighborhood::Neighborhood;

/// Neighbour offsets for the given [`Neighborhood`] kind.
pub(crate) const fn neighbour_offsets(kind: Neighborhood) -> &'static [(i32, i32)] {
    match kind {
        Neighborhood::Von4 => &[(0, -1), (1, 0), (0, 1), (-1, 0)],
        Neighborhood::Moore8 => &[
            (-1, -1), (0, -1), (1, -1),
            (-1,  0),          (1,  0),
            (-1,  1), (0,  1), (1,  1),
        ],
    }
}
