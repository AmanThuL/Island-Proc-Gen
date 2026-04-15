//! Hydrological simulation stages.
//!
//! Covers routing (flow direction, accumulation, basins, river
//! extraction) plus the Sprint 1B water-balance stack (PET, Budyko ET/R
//! split, soil moisture).

pub mod accumulation;
pub mod basins;
pub mod flow_routing;
pub mod rivers;
pub mod soil_moisture;
pub mod water_balance;

pub use accumulation::AccumulationStage;
pub use basins::BasinsStage;
pub use flow_routing::FlowRoutingStage;
pub use rivers::RiverExtractionStage;
pub use soil_moisture::SoilMoistureStage;
pub use water_balance::{PetStage, WaterBalanceStage};

pub use island_core::world::{D8_OFFSETS, FLOW_DIR_SINK};
