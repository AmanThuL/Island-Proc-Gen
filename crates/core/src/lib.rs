pub mod field;
pub mod neighborhood;
pub mod pipeline;
pub mod preset;
pub mod save;
pub mod seed;
pub mod validation;
pub mod world;

pub use field::{FieldDecodeError, FieldStats, MaskField2D, ScalarField2D, VectorField2D};
pub use neighborhood::{
    COAST_DETECT_NEIGHBORHOOD, Neighborhood, RIVER_CC_NEIGHBORHOOD, RIVER_COAST_CONTACT,
    neighbour_offsets,
};
pub use pipeline::{NoopStage, SimulationPipeline, SimulationStage};
pub use preset::{IslandAge, IslandArchetypePreset};
pub use save::{LoadedWorld, SaveError, SaveMode};
pub use seed::Seed;
pub use validation::{
    ValidationError, accumulation_monotone, basin_partition_dag, coastline_consistency,
    river_termination,
};
pub use world::{
    AuthoritativeFields, BakedSnapshot, CoastMask, D8_OFFSETS, DerivedCaches, FLOW_DIR_SINK,
    Resolution, WorldState,
};
