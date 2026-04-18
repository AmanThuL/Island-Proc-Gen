//! Geomorphology simulation stages.
//!
//! Sprint 1A: Task 1A.1 `TopographyStage`, Task 1A.2 `CoastMaskStage`,
//! Task 1A.3 `PitFillStage`, Task 1A.4 `DerivedGeomorphStage`.
//! Sprint 2: Task 2.1 `StreamPowerIncisionStage`,
//!           Task 2.2 `HillslopeDiffusionStage`.

mod coastal;
mod derived_geomorph;
mod hillslope;
mod pit_fill;
mod stream_power;
mod topography;

pub use coastal::CoastMaskStage;
pub use derived_geomorph::DerivedGeomorphStage;
pub use hillslope::HillslopeDiffusionStage;
pub use pit_fill::PitFillStage;
pub use stream_power::StreamPowerIncisionStage;
pub use topography::TopographyStage;

pub(crate) use island_core::neighborhood::neighbour_offsets;
