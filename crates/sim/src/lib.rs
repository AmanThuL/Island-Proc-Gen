//! Simulation pipeline stages: geomorph, hydro, climate, ecology.
//!
//! Sprint 1A fills in `geomorph` and `hydro`.

pub mod geomorph;

pub use geomorph::CoastMaskStage;
pub use geomorph::PitFillStage;
pub use geomorph::TopographyStage;
