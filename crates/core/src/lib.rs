pub mod field;
pub mod neighborhood;
pub mod pipeline;
pub mod preset;
pub mod save;
pub mod seed;
pub mod validation;
pub mod world;

#[cfg(test)]
pub(crate) mod test_support;

pub use field::{FieldDecodeError, FieldStats, MaskField2D, ScalarField2D, VectorField2D};
pub use neighborhood::{
    COAST_DETECT_NEIGHBORHOOD, Neighborhood, RIVER_CC_NEIGHBORHOOD, RIVER_COAST_CONTACT,
    neighbour_offsets,
};
pub use pipeline::{NoopStage, PipelineError, SimulationPipeline, SimulationStage};
pub use preset::{IslandAge, IslandArchetypePreset, MAX_RELIEF_REF_M};
pub use save::{LoadedWorld, SaveError, SaveMode};
pub use seed::Seed;
pub use validation::{
    ValidationError, accumulation_monotone, basin_partition_dag, biome_weights_normalized,
    coastline_consistency, hex_attrs_present, precipitation_nonneg, river_termination,
    temperature_physical_range,
};
pub use world::{
    AuthoritativeFields, BakedSnapshot, BiomeType, BiomeWeights, CoastMask, D8_OFFSETS,
    DerivedCaches, FLOW_DIR_SINK, HexAttributeField, HexAttributes, HexGrid, HexLayout, Resolution,
    WorldState,
};
