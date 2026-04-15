//! Ecology stages: biome suitability and basin-level smoothing.
//!
//! Sprint 1B ships `BiomeWeightsStage` (DD6). Sprint 2+ adds
//! succession, disturbance, and dynamic cover.

pub mod biome;
pub mod suitability;

pub use biome::BiomeWeightsStage;
