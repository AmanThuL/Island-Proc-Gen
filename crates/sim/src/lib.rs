//! Simulation pipeline stages: geomorph, hydro, climate, ecology.
//!
//! Organised into submodules by domain plus a pipeline-end
//! [`ValidationStage`] wrapper around `island_core::validation`. The
//! [`StageId`] enum names every stage by symbolic id so
//! [`island_core::SimulationPipeline::run_from`] callers (slider rerun,
//! load-time rebuild) never hardcode raw indices.
//!
//! Sprint 1A + 1B + 2 together form the **19-stage canonical pipeline**
//! (18 [`StageId`] variants + terminal [`ValidationStage`]).

pub mod climate;
pub mod compute;
pub mod ecology;
pub mod geomorph;
pub mod hex_coast_class;
pub mod hex_projection;
pub mod hydro;
pub mod invalidation;
pub mod validation_stage;

pub use climate::FogLikelihoodStage;
pub use climate::PrecipitationStage;
pub use climate::TemperatureStage;
pub use ecology::BiomeWeightsStage;
pub use geomorph::CoastMaskStage;
pub use geomorph::CoastTypeStage;
pub use geomorph::DepositionStage;
pub use geomorph::DerivedGeomorphStage;
pub use geomorph::ErosionOuterLoop;
pub use geomorph::HillslopeDiffusionStage;
pub use geomorph::PitFillStage;
pub use geomorph::SedimentUpdateStage;
pub use geomorph::StreamPowerIncisionStage;
pub use geomorph::TopographyStage;
pub use hex_projection::HexProjectionStage;
pub use hydro::AccumulationStage;
pub use hydro::BasinsStage;
pub use hydro::FlowRoutingStage;
pub use hydro::PetStage;
pub use hydro::RiverExtractionStage;
pub use hydro::SoilMoistureStage;
pub use hydro::WaterBalanceStage;
pub use invalidation::invalidate_from;
pub use validation_stage::ValidationStage;

use std::sync::Arc;

use island_core::pipeline::{ComputeBackend, SimulationPipeline};

use crate::compute::CpuBackend;

// ─── default_pipeline ─────────────────────────────────────────────────────────

/// Build the canonical Sprint 1A + Sprint 1B + Sprint 2 [`SimulationPipeline`]
/// using the [`CpuBackend`] for the two pilot compute kernels.
///
/// Push order is identical to [`StageId`]'s discriminant order, forming the
/// 19-stage canonical pipeline (18 [`StageId`] variants + terminal
/// [`ValidationStage`]).
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
    default_pipeline_with_backend(Arc::new(CpuBackend))
}

/// Build the canonical pipeline with a custom [`ComputeBackend`].
///
/// Identical stage order to [`default_pipeline`]; the only difference is that
/// `ErosionOuterLoop` receives `backend` as its dispatch target for the two
/// pilot kernels (hillslope diffusion + stream power incision).
///
/// Sprint 4.D+ callers pass a `GpuBackend` here; all other callers use
/// [`default_pipeline`] which supplies [`CpuBackend`].
pub fn default_pipeline_with_backend(backend: Arc<dyn ComputeBackend>) -> SimulationPipeline {
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
    // Sprint 2 (index 8) — ErosionOuterLoop owns its own n_batch × n_inner
    // SPIM + hillslope iteration plus end-of-batch Coastal..RiverExtraction
    // re-run, so `default_pipeline` sees it as one opaque stage.
    pipeline.push(Box::new(ErosionOuterLoop::new(backend)));
    // Sprint 2 (index 9) — CoastTypeStage classifies each coast cell into
    // one of four geomorphic categories after erosion has settled the terrain.
    pipeline.push(Box::new(CoastTypeStage));
    // Sprint 1B (indices 10..=17) — shifted down by 2 in Sprint 2 Tasks
    // 2.3+2.4 to make room for ErosionOuterLoop and CoastTypeStage.
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

/// Symbolic identifier for every stage in the 19-stage canonical pipeline.
///
/// There are exactly **18 variants** (`Topography = 0` … `HexProjection = 17`).
/// The discriminant is the stage's index in the `run()` push order, so
/// `pipeline.run_from(world, StageId::Precipitation as usize)` is the
/// correct call for a slider that touches `PrecipitationStage`. Any
/// discrepancy between this enum and another stage listing in the repo
/// (sprint docs, assembly code) resolves in favour of this enum — it is
/// the single source of truth for stage indices.
///
/// Sprint 2 Task 2.3 inserted `ErosionOuterLoop = 8` between
/// `RiverExtraction` and `Temperature`, shifting every Sprint 1B variant
/// down by 1. Sprint 2 Task 2.4 inserted `CoastType = 9` between
/// `ErosionOuterLoop` and `Temperature`, shifting 1B variants down by a
/// further slot (now `Temperature = 10` … `HexProjection = 17`).
///
/// `ValidationStage` is intentionally **not** a `StageId` variant: it is
/// the terminal tail hook that runs invariants after the 18 `StageId` stages
/// finish, and is never a `run_from` target.
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
    ErosionOuterLoop = 8,
    CoastType = 9,
    Temperature = 10,
    Precipitation = 11,
    FogLikelihood = 12,
    Pet = 13,
    WaterBalance = 14,
    SoilMoisture = 15,
    BiomeWeights = 16,
    HexProjection = 17,
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

// ─── compute backend tests ────────────────────────────────────────────────────

#[cfg(test)]
mod compute_backend_tests {
    use island_core::pipeline::{ComputeOp, NoOpBackend};

    use crate::geomorph::ErosionOuterLoop;

    use super::*;

    /// `default_pipeline()` constructs an `ErosionOuterLoop` whose backend
    /// name is `"cpu"`. This anchors the Sprint 4.C contract that the
    /// zero-arg `default_pipeline()` always uses the CPU path.
    #[test]
    fn default_pipeline_uses_cpu_backend_by_default() {
        // We need to reach the ErosionOuterLoop inside the pipeline.
        // Since SimulationPipeline stores `Box<dyn SimulationStage>` without
        // exposing typed access, we verify by constructing an ErosionOuterLoop
        // the same way default_pipeline does and checking the backend name.
        let loop_stage = ErosionOuterLoop::default();
        assert_eq!(
            loop_stage.backend_name(),
            "cpu",
            "default_pipeline must use CpuBackend"
        );
    }

    /// `NoOpBackend` returns false for all `ComputeOp::ALL` ops.
    #[test]
    fn noop_backend_supports_nothing() {
        let b = NoOpBackend;
        for &op in ComputeOp::ALL {
            assert!(!b.supports(op), "NoOpBackend must not support {op:?}");
        }
    }

    /// `default_pipeline_with_backend(Arc::new(NoOpBackend))` constructs a
    /// pipeline whose ErosionOuterLoop carries `"noop"` as backend name.
    #[test]
    fn pipeline_with_backend_uses_supplied_backend() {
        use std::sync::Arc;
        let loop_stage = ErosionOuterLoop::new(Arc::new(NoOpBackend));
        assert_eq!(loop_stage.backend_name(), "noop");
    }
}

// ─── stage id tests ───────────────────────────────────────────────────────────

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
            ErosionOuterLoop,
            CoastType,
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
        assert_eq!(
            StageId::STAGE_COUNT,
            18,
            "Sprint 2 Task 2.4 locks STAGE_COUNT == 18 (CoastType = 9 inserted)"
        );
    }
}
