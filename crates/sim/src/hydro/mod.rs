//! Hydrological simulation stages.
//!
//! Sprint 1A: Task 1A.5 `FlowRoutingStage`, Task 1A.6 `AccumulationStage`,
//! Task 1A.7 `BasinsStage`, Task 1A.8 `RiverExtractionStage`.

pub mod accumulation;
pub mod basins;
pub mod flow_routing;
pub mod rivers;

pub use accumulation::AccumulationStage;
pub use basins::BasinsStage;
pub use flow_routing::FlowRoutingStage;
pub use rivers::RiverExtractionStage;

pub use island_core::world::{D8_OFFSETS, FLOW_DIR_SINK};
