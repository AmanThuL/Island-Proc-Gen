//! Pipeline-end validation wrapper.
//!
//! `ValidationStage` runs every `core::validation` invariant as a
//! [`SimulationStage`] so `SimulationPipeline::run` can assert
//! correctness automatically at the pipeline tail. Each invariant is
//! still available standalone from `island_core::validation`.
//!
//! Sprint 1A ships 4 invariants that depend only on the routing /
//! coast stages. Sprint 1B adds 4 more (`precipitation_nonneg`,
//! `biome_weights_normalized`, `temperature_physical_range`,
//! `hex_attrs_present`). To keep the stage working for pipelines
//! that stop at 1A (e.g. validation unit tests that only need the
//! routing stack), each 1B check is gated on its input field: if the
//! field is still `None`, the invariant is skipped with a
//! `MissingPrecondition` error, and we treat that specific error as
//! "not applicable yet" rather than a failure. Every other error
//! (actual invariant violations) propagates normally.

use island_core::pipeline::SimulationStage;
use island_core::validation::{
    ValidationError, accumulation_monotone, basin_partition_dag, biome_weights_normalized,
    coastline_consistency, hex_attrs_present, precipitation_nonneg, river_termination,
    temperature_physical_range,
};
use island_core::world::WorldState;

/// Run every core validation invariant in order, short-circuiting on
/// the first real failure. 1B invariants whose preconditions are
/// missing are silently skipped.
pub struct ValidationStage;

impl SimulationStage for ValidationStage {
    fn name(&self) -> &'static str {
        "validation"
    }

    fn run(&self, world: &mut WorldState) -> anyhow::Result<()> {
        // Sprint 1A: hard-required invariants — every Sprint 1A
        // pipeline has these fields populated.
        coastline_consistency(world)?;
        basin_partition_dag(world)?;
        accumulation_monotone(world)?;
        river_termination(world)?;

        // Sprint 1B: run if the upstream stage has populated the
        // input field, otherwise skip silently.
        skip_if_missing(precipitation_nonneg(world))?;
        skip_if_missing(biome_weights_normalized(world))?;
        skip_if_missing(temperature_physical_range(world))?;
        skip_if_missing(hex_attrs_present(world))?;
        Ok(())
    }
}

/// Collapse `MissingPrecondition` into `Ok(())`: a 1B invariant that
/// can't run because its stage hasn't fired yet is "not applicable",
/// not a failure. Every other `ValidationError` still propagates.
fn skip_if_missing(result: Result<(), ValidationError>) -> anyhow::Result<()> {
    match result {
        Ok(()) => Ok(()),
        Err(ValidationError::MissingPrecondition { .. }) => Ok(()),
        Err(other) => Err(other.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        AccumulationStage, BasinsStage, CoastMaskStage, DerivedGeomorphStage, FlowRoutingStage,
        PitFillStage, RiverExtractionStage, TopographyStage,
    };
    use island_core::pipeline::SimulationPipeline;
    use island_core::preset::{IslandAge, IslandArchetypePreset};
    use island_core::seed::Seed;
    use island_core::world::{Resolution, WorldState};

    fn volcanic_preset() -> IslandArchetypePreset {
        IslandArchetypePreset {
            name: "volcanic_test".into(),
            island_radius: 0.45,
            max_relief: 0.8,
            volcanic_center_count: 1,
            island_age: IslandAge::Young,
            prevailing_wind_dir: 0.0,
            marine_moisture_strength: 0.5,
            sea_level: 0.25,
        }
    }

    #[test]
    fn full_sprint_1a_pipeline_passes_all_invariants() {
        let mut world = WorldState::new(Seed(42), volcanic_preset(), Resolution::new(64, 64));

        let mut pipeline = SimulationPipeline::new();
        pipeline.push(Box::new(TopographyStage));
        pipeline.push(Box::new(CoastMaskStage));
        pipeline.push(Box::new(PitFillStage));
        pipeline.push(Box::new(DerivedGeomorphStage));
        pipeline.push(Box::new(FlowRoutingStage));
        pipeline.push(Box::new(AccumulationStage));
        pipeline.push(Box::new(BasinsStage));
        pipeline.push(Box::new(RiverExtractionStage));
        pipeline.push(Box::new(ValidationStage));

        pipeline
            .run(&mut world)
            .expect("full Sprint 1A pipeline must pass all four invariants");
    }

    /// End-to-end Sprint 1B integration test: run all 16 stages + tail
    /// validation on the synthetic volcanic preset, and assert every
    /// Sprint 1B field is populated. If any climate/ecology/hex stage
    /// errors out on a real `TopographyStage` output, this test fires
    /// immediately at the pipeline boundary.
    #[test]
    fn full_sprint_1b_pipeline_passes_all_invariants() {
        use crate::{
            BiomeWeightsStage, FogLikelihoodStage, HexProjectionStage, PetStage,
            PrecipitationStage, SoilMoistureStage, TemperatureStage, WaterBalanceStage,
        };
        let mut world = WorldState::new(Seed(42), volcanic_preset(), Resolution::new(64, 64));

        let mut pipeline = SimulationPipeline::new();
        pipeline.push(Box::new(TopographyStage));
        pipeline.push(Box::new(CoastMaskStage));
        pipeline.push(Box::new(PitFillStage));
        pipeline.push(Box::new(DerivedGeomorphStage));
        pipeline.push(Box::new(FlowRoutingStage));
        pipeline.push(Box::new(AccumulationStage));
        pipeline.push(Box::new(BasinsStage));
        pipeline.push(Box::new(RiverExtractionStage));
        pipeline.push(Box::new(TemperatureStage));
        pipeline.push(Box::new(PrecipitationStage));
        pipeline.push(Box::new(FogLikelihoodStage));
        pipeline.push(Box::new(PetStage));
        pipeline.push(Box::new(WaterBalanceStage));
        pipeline.push(Box::new(SoilMoistureStage));
        pipeline.push(Box::new(BiomeWeightsStage));
        pipeline.push(Box::new(HexProjectionStage));
        pipeline.push(Box::new(ValidationStage));

        pipeline
            .run(&mut world)
            .expect("full Sprint 1B pipeline must pass all eight invariants");

        // Every Sprint 1B output field is populated.
        assert!(world.derived.curvature.is_some());
        assert!(world.baked.temperature.is_some());
        assert!(world.baked.precipitation.is_some());
        assert!(world.derived.fog_likelihood.is_some());
        assert!(world.derived.pet.is_some());
        assert!(world.derived.et.is_some());
        assert!(world.derived.runoff.is_some());
        assert!(world.baked.soil_moisture.is_some());
        assert!(world.baked.biome_weights.is_some());
        assert!(world.derived.hex_grid.is_some());
        assert!(world.derived.hex_attrs.is_some());
    }

    #[test]
    fn validation_stage_errors_on_empty_world() {
        let mut world = WorldState::new(Seed(0), volcanic_preset(), Resolution::new(16, 16));
        let stage = ValidationStage;
        let res = stage.run(&mut world);
        assert!(
            res.is_err(),
            "ValidationStage must Err when preconditions are missing"
        );
    }
}
