//! Geomorphology simulation stages.
//!
//! Sprint 1A: Task 1A.1 `TopographyStage`, Task 1A.2 `CoastMaskStage`.

mod coastal;
mod topography;

pub use coastal::CoastMaskStage;
pub use topography::TopographyStage;
