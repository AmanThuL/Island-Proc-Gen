//! Simulation pipeline stages: geomorph, hydro, climate, ecology.
//!
//! Organised into submodules by domain plus a pipeline-end
//! [`ValidationStage`] wrapper around `island_core::validation`. The
//! [`StageId`] enum names every stage by symbolic id so
//! [`island_core::SimulationPipeline::run_from`] callers (slider rerun,
//! load-time rebuild) never hardcode raw indices.

pub mod climate;
pub mod ecology;
pub mod geomorph;
pub mod hex_projection;
pub mod hydro;
pub mod validation_stage;

pub use climate::FogLikelihoodStage;
pub use climate::PrecipitationStage;
pub use climate::TemperatureStage;
pub use ecology::BiomeWeightsStage;
pub use geomorph::CoastMaskStage;
pub use geomorph::DerivedGeomorphStage;
pub use geomorph::PitFillStage;
pub use geomorph::TopographyStage;
pub use hex_projection::HexProjectionStage;
pub use hydro::AccumulationStage;
pub use hydro::BasinsStage;
pub use hydro::FlowRoutingStage;
pub use hydro::PetStage;
pub use hydro::RiverExtractionStage;
pub use hydro::SoilMoistureStage;
pub use hydro::WaterBalanceStage;
pub use validation_stage::ValidationStage;

use island_core::pipeline::SimulationPipeline;

// ─── default_pipeline ─────────────────────────────────────────────────────────

/// Build the canonical Sprint 1A + Sprint 1B [`SimulationPipeline`].
///
/// Push order is identical to [`StageId`]'s discriminant order; the tail
/// [`ValidationStage`] runs after the 16 "real" stages.
///
/// Both the interactive runtime ([`app::runtime::Runtime`]), the golden-seed
/// regression tests, and the Sprint 1C headless executor build from this
/// helper so the stage list has a single source of truth.
///
/// # Example
///
/// ```
/// use island_core::{pipeline::SimulationPipeline, seed::Seed, world::{Resolution, WorldState}};
///
/// # fn doc_example(preset: island_core::preset::IslandArchetypePreset) -> anyhow::Result<()> {
/// let mut world = WorldState::new(Seed(42), preset, Resolution::new(64, 64));
/// let pipeline = sim::default_pipeline();
/// pipeline.run(&mut world)?;
/// # Ok(())
/// # }
/// ```
pub fn default_pipeline() -> SimulationPipeline {
    let mut pipeline = SimulationPipeline::new();
    // Sprint 1A (indices 0..=7)
    pipeline.push(Box::new(TopographyStage));
    pipeline.push(Box::new(CoastMaskStage));
    pipeline.push(Box::new(PitFillStage));
    pipeline.push(Box::new(DerivedGeomorphStage));
    pipeline.push(Box::new(FlowRoutingStage));
    pipeline.push(Box::new(AccumulationStage));
    pipeline.push(Box::new(BasinsStage));
    pipeline.push(Box::new(RiverExtractionStage));
    // Sprint 1B (indices 8..=15)
    pipeline.push(Box::new(TemperatureStage));
    pipeline.push(Box::new(PrecipitationStage));
    pipeline.push(Box::new(FogLikelihoodStage));
    pipeline.push(Box::new(PetStage));
    pipeline.push(Box::new(WaterBalanceStage));
    pipeline.push(Box::new(SoilMoistureStage));
    pipeline.push(Box::new(BiomeWeightsStage));
    pipeline.push(Box::new(HexProjectionStage));
    // Tail hook — runs all 8 invariants.
    pipeline.push(Box::new(ValidationStage));
    pipeline
}

// ─── StageId ──────────────────────────────────────────────────────────────────

/// Symbolic identifier for every stage in the canonical linear pipeline.
///
/// The discriminant is the stage's index in the `run()` push order, so
/// `pipeline.run_from(world, StageId::Precipitation as usize)` is the
/// correct call for a slider that touches `PrecipitationStage`. Any
/// discrepancy between this enum and another stage listing in the repo
/// (sprint docs, assembly code) resolves in favour of this enum — it is
/// the single source of truth for stage indices.
///
/// `ValidationStage` is intentionally **not** a `StageId` variant: it is
/// a tail hook that runs invariants after the "real" pipeline finishes,
/// not a stage that any slider should ever target with `run_from`.
#[repr(usize)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StageId {
    Topography = 0,
    Coastal = 1,
    PitFill = 2,
    DerivedGeomorph = 3,
    FlowRouting = 4,
    Accumulation = 5,
    Basins = 6,
    RiverExtraction = 7,
    Temperature = 8,
    Precipitation = 9,
    FogLikelihood = 10,
    Pet = 11,
    WaterBalance = 12,
    SoilMoisture = 13,
    BiomeWeights = 14,
    HexProjection = 15,
}

impl StageId {
    /// Pipeline index for use with
    /// [`island_core::SimulationPipeline::run_from`].
    pub const fn index(self) -> usize {
        self as usize
    }

    /// Number of real stages in the canonical pipeline (excluding the
    /// tail `ValidationStage`). Derived from the highest variant so it
    /// tracks the enum automatically.
    pub const STAGE_COUNT: usize = Self::HexProjection as usize + 1;
}

#[cfg(test)]
mod stage_id_tests {
    use super::StageId;

    // Lock every ordinal: if a future sprint reshuffles the enum, every
    // consumer (params panel, load-time rebuild, pipeline assembly) has
    // to be audited — this test fires first.
    #[test]
    fn stage_id_indices_are_dense_and_canonical() {
        use StageId::*;
        let ordered = [
            Topography,
            Coastal,
            PitFill,
            DerivedGeomorph,
            FlowRouting,
            Accumulation,
            Basins,
            RiverExtraction,
            Temperature,
            Precipitation,
            FogLikelihood,
            Pet,
            WaterBalance,
            SoilMoisture,
            BiomeWeights,
            HexProjection,
        ];
        for (i, id) in ordered.iter().enumerate() {
            assert_eq!(id.index(), i, "StageId::{:?} is not at index {}", id, i);
        }
        assert_eq!(ordered.len(), StageId::STAGE_COUNT);
    }
}
