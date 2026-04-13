//! Simulation pipeline: an ordered sequence of [`SimulationStage`]s.
//!
//! The pipeline is intentionally trivial in Sprint 0 — its only job is to
//! exist as a CPU-only, graphics-free scaffold so Sprint 1A+ can drop stages
//! in without any coupling to `wgpu` / `winit` / `egui`. The hard invariant
//! is the [`tests::pipeline_runs_without_graphics`] test below: if a future
//! dependency leak creeps into `core`, that test will still build and run,
//! but `cargo tree -p core` will start flagging graphics crates — catch it
//! in CI.

use crate::world::WorldState;

// ─── trait ───────────────────────────────────────────────────────────────────

/// One stage of the simulation pipeline.
///
/// Object-safe on purpose: [`SimulationPipeline`] stores `Box<dyn SimulationStage>`.
pub trait SimulationStage {
    /// Short, stable identifier used in `tracing` output and logs.
    fn name(&self) -> &'static str;

    /// Advance `world` by this stage's contribution. Errors bubble up and
    /// short-circuit the pipeline.
    fn run(&self, world: &mut WorldState) -> anyhow::Result<()>;
}

// ─── NoopStage ───────────────────────────────────────────────────────────────

/// Sprint 0 placeholder stage. Does nothing; used by the headline
/// `pipeline_runs_without_graphics` invariant test and as a smoke-test
/// building block for downstream crates until real stages land.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopStage;

impl SimulationStage for NoopStage {
    fn name(&self) -> &'static str {
        "noop"
    }

    fn run(&self, _world: &mut WorldState) -> anyhow::Result<()> {
        Ok(())
    }
}

// ─── pipeline ────────────────────────────────────────────────────────────────

/// An ordered sequence of stages that operate on a shared [`WorldState`].
///
/// Stages are stored behind `Box<dyn SimulationStage>` so new stage types
/// can be added in downstream crates without touching `core`.
pub struct SimulationPipeline {
    stages: Vec<Box<dyn SimulationStage>>,
}

impl SimulationPipeline {
    /// Build an empty pipeline.
    pub fn new() -> Self {
        Self { stages: Vec::new() }
    }

    /// Append `stage` to the end of the pipeline.
    pub fn push(&mut self, stage: Box<dyn SimulationStage>) {
        self.stages.push(stage);
    }

    /// Number of stages currently in the pipeline.
    pub fn len(&self) -> usize {
        self.stages.len()
    }

    /// `true` iff the pipeline has no stages.
    pub fn is_empty(&self) -> bool {
        self.stages.is_empty()
    }

    /// Run every stage in push order. Short-circuits on the first error.
    pub fn run(&self, world: &mut WorldState) -> anyhow::Result<()> {
        for s in &self.stages {
            tracing::info!(stage = s.name(), "running");
            s.run(world)?;
        }
        Ok(())
    }
}

impl Default for SimulationPipeline {
    fn default() -> Self {
        Self::new()
    }
}

// ─── tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::preset::{IslandAge, IslandArchetypePreset};
    use crate::seed::Seed;
    use crate::world::{Resolution, WorldState};
    use std::cell::RefCell;
    use std::rc::Rc;

    // We deliberately do NOT import `data::presets::load_preset` here — that
    // would turn `data` into a `core` dev-dep and poison the
    // `cargo tree -p core` invariant (`core` must stay free of any graphics
    // or data-file baggage). Build the preset inline instead.
    fn test_preset() -> IslandArchetypePreset {
        IslandArchetypePreset {
            name: "test".into(),
            island_radius: 0.5,
            max_relief: 0.5,
            volcanic_center_count: 1,
            island_age: IslandAge::Young,
            prevailing_wind_dir: 0.0,
            marine_moisture_strength: 0.5,
            sea_level: 0.3,
        }
    }

    // 1. THE Sprint 0 headline invariant: a full WorldState + pipeline can
    //    be constructed and run in plain `cargo test -p core`, with no
    //    graphics / windowing / UI crates in the link line.
    #[test]
    fn pipeline_runs_without_graphics() {
        let mut world = WorldState::new(Seed(42), test_preset(), Resolution::new(256, 256));
        assert!(world.authoritative.height.is_none()); // Sprint 0: still empty

        let mut pipeline = SimulationPipeline::new();
        pipeline.push(Box::new(NoopStage));
        pipeline
            .run(&mut world)
            .expect("pipeline should run cleanly");

        assert!(world.authoritative.height.is_none()); // NoopStage leaves it empty
    }

    // 2. A stage that returns Err short-circuits the pipeline and the error
    //    propagates out of `run`.
    struct BoomStage;
    impl SimulationStage for BoomStage {
        fn name(&self) -> &'static str {
            "boom"
        }
        fn run(&self, _world: &mut WorldState) -> anyhow::Result<()> {
            Err(anyhow::anyhow!("boom"))
        }
    }

    #[test]
    fn pipeline_propagates_stage_error() {
        let mut world = WorldState::new(Seed(0), test_preset(), Resolution::new(16, 16));
        let mut pipeline = SimulationPipeline::new();
        pipeline.push(Box::new(BoomStage));
        let res = pipeline.run(&mut world);
        let err = res.expect_err("BoomStage should have short-circuited the pipeline");
        assert!(
            err.to_string().contains("boom"),
            "expected error to mention 'boom', got: {err}"
        );
    }

    // 3. Stages run in push order.
    struct CountingStage {
        label: &'static str,
        log: Rc<RefCell<Vec<&'static str>>>,
    }
    impl SimulationStage for CountingStage {
        fn name(&self) -> &'static str {
            self.label
        }
        fn run(&self, _world: &mut WorldState) -> anyhow::Result<()> {
            self.log.borrow_mut().push(self.label);
            Ok(())
        }
    }

    #[test]
    fn pipeline_runs_all_stages_in_order() {
        let mut world = WorldState::new(Seed(1), test_preset(), Resolution::new(8, 8));
        let log: Rc<RefCell<Vec<&'static str>>> = Rc::new(RefCell::new(Vec::new()));

        let mut pipeline = SimulationPipeline::new();
        pipeline.push(Box::new(CountingStage {
            label: "a",
            log: log.clone(),
        }));
        pipeline.push(Box::new(CountingStage {
            label: "b",
            log: log.clone(),
        }));
        pipeline.push(Box::new(CountingStage {
            label: "c",
            log: log.clone(),
        }));
        assert_eq!(pipeline.len(), 3);
        assert!(!pipeline.is_empty());

        pipeline
            .run(&mut world)
            .expect("counting stages should succeed");
        assert_eq!(*log.borrow(), vec!["a", "b", "c"]);
    }
}
