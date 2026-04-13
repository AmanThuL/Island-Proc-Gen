//! Simulation pipeline stages: geomorph, hydro, climate, ecology.
//!
//! Sprint 1A fills in `geomorph` and `hydro` plus a pipeline-end
//! [`ValidationStage`] wrapper around `island_core::validation`.

pub mod geomorph;
pub mod hydro;
pub mod validation_stage;

pub use geomorph::CoastMaskStage;
pub use geomorph::DerivedGeomorphStage;
pub use geomorph::PitFillStage;
pub use geomorph::TopographyStage;
pub use hydro::AccumulationStage;
pub use hydro::BasinsStage;
pub use hydro::FlowRoutingStage;
pub use hydro::RiverExtractionStage;
pub use validation_stage::ValidationStage;
