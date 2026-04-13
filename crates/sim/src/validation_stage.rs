//! Pipeline-end validation wrapper.
//!
//! `ValidationStage` runs the four `core::validation` invariants as a
//! [`SimulationStage`] so `SimulationPipeline::run` can assert correctness
//! automatically at the end of every Sprint 1A run. Each invariant is still
//! available standalone from `island_core::validation`.

use island_core::pipeline::SimulationStage;
use island_core::validation::{
    accumulation_monotone, basin_partition_dag, coastline_consistency, river_termination,
};
use island_core::world::WorldState;

/// Runs the four Sprint 1A correctness invariants in order. Short-circuits
/// on the first failure so the error message identifies the originating
/// invariant.
pub struct ValidationStage;

impl SimulationStage for ValidationStage {
    fn name(&self) -> &'static str {
        "validation"
    }

    fn run(&self, world: &mut WorldState) -> anyhow::Result<()> {
        coastline_consistency(world)?;
        basin_partition_dag(world)?;
        accumulation_monotone(world)?;
        river_termination(world)?;
        Ok(())
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
