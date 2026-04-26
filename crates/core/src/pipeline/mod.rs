//! Simulation pipeline: an ordered sequence of [`SimulationStage`]s.
//!
//! The pipeline is CPU-only and graphics-free by construction. The headline
//! invariant is the [`tests::pipeline_runs_without_graphics`] test: if a
//! future dependency leak creeps into `core`, that test will still build
//! and run, but `cargo tree -p core` will start flagging graphics crates —
//! catch it in CI.
//!
//! [`SimulationPipeline::run_from`] supports incremental re-runs driven by
//! slider interactions and load-time rebuilds. The linear-chain semantics
//! make the call trivial: "run stages `[start..]` on a `WorldState` whose
//! `[0..start)` prefix is already populated".

pub mod compute;
pub mod timing;

pub use compute::{
    ComputeBackend, ComputeBackendError, ComputeOp, HillslopeParams, NoOpBackend, StreamPowerParams,
};
pub use timing::StageTiming;

use std::collections::BTreeMap;
use std::time::Instant;

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

// ─── PipelineError ───────────────────────────────────────────────────────────

/// Errors that can short-circuit [`SimulationPipeline::run_from`] before any
/// stage runs.
///
/// Stages themselves still return `anyhow::Error` — this enum only covers
/// the pre-flight checks that `run_from` owns.
#[derive(Debug, thiserror::Error)]
pub enum PipelineError {
    /// `run_from(start)` was called with `start > len()`.
    #[error("pipeline: start_index {start} exceeds pipeline length {len}")]
    StartIndexOutOfBounds { start: usize, len: usize },
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
#[derive(Default)]
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
        self.run_from(world, 0)
    }

    /// Run stages `[start_index..len()]` in push order on a `WorldState`
    /// whose `[0..start_index)` prefix has already been populated by a
    /// previous `run` / `run_from(0)`.
    ///
    /// # Preconditions
    ///
    /// * `start_index <= len()`. A value equal to `len()` is a no-op
    ///   (nothing to run).
    /// * Fields produced by the stages in `[0..start_index)` must already
    ///   be populated on `world`. The pipeline cannot introspect stage
    ///   output names, so the caller is responsible for this contract;
    ///   each stage is expected to short-circuit with its own
    ///   "missing-precondition" `Err` if a required input is `None`.
    ///
    /// # Errors
    ///
    /// * [`PipelineError::StartIndexOutOfBounds`] if `start_index > len()`.
    /// * Any stage error from the `[start_index..]` slice short-circuits
    ///   the remainder of the call, exactly like [`run`].
    ///
    /// # Typical callers
    ///
    /// * `start_index == 0` — a fresh world or a `SaveMode::Minimal` load.
    /// * Slider re-run — `ParamsPanel` maps each slider to a stage via
    ///   `StageId` and calls `run_from(world, stage as usize)` so only the
    ///   touched stage and its downstream neighbours re-run.
    /// * `SaveMode::Full` load — `run_from(StageId::Coastal as usize)`
    ///   rebuilds every `derived` field from the saved
    ///   `authoritative.height` without re-running `TopographyStage`.
    ///
    /// [`run`]: SimulationPipeline::run
    pub fn run_from(&self, world: &mut WorldState, start_index: usize) -> anyhow::Result<()> {
        if start_index > self.stages.len() {
            return Err(PipelineError::StartIndexOutOfBounds {
                start: start_index,
                len: self.stages.len(),
            }
            .into());
        }

        // Preserve any timings recorded for stages *before* start_index so a
        // partial run_from doesn't wipe them. On a fresh run_from(0) the map
        // starts empty; on an incremental run the caller can inspect the full
        // map after completion.
        let mut timings: BTreeMap<String, StageTiming> =
            world.derived.last_stage_timings.take().unwrap_or_default();

        for s in &self.stages[start_index..] {
            let name = s.name().to_owned();
            tracing::info!(stage = %name, "running");
            let cpu_start = Instant::now();
            s.run(world)?;
            let cpu_ms = cpu_start.elapsed().as_secs_f64() * 1_000.0;
            // Drain the GPU side-channel written by ComputeBackend implementations.
            // Always None in the Sprint 4.A CPU-only substrate.
            let gpu_ms = world.derived.last_stage_gpu_ms.take();
            timings.insert(name, StageTiming { cpu_ms, gpu_ms });
        }

        world.derived.last_stage_timings = Some(timings);
        Ok(())
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
            erosion: Default::default(),
            climate: Default::default(),
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

    fn make_abc_pipeline(log: Rc<RefCell<Vec<&'static str>>>) -> SimulationPipeline {
        let mut pipeline = SimulationPipeline::new();
        pipeline.push(Box::new(CountingStage {
            label: "a",
            log: log.clone(),
        }));
        pipeline.push(Box::new(CountingStage {
            label: "b",
            log: log.clone(),
        }));
        pipeline.push(Box::new(CountingStage { label: "c", log }));
        pipeline
    }

    #[test]
    fn pipeline_runs_all_stages_in_order() {
        let mut world = WorldState::new(Seed(1), test_preset(), Resolution::new(8, 8));
        let log: Rc<RefCell<Vec<&'static str>>> = Rc::new(RefCell::new(Vec::new()));

        let pipeline = make_abc_pipeline(log.clone());
        assert_eq!(pipeline.len(), 3);
        assert!(!pipeline.is_empty());

        pipeline
            .run(&mut world)
            .expect("counting stages should succeed");
        assert_eq!(*log.borrow(), vec!["a", "b", "c"]);
    }

    #[test]
    fn run_from_zero_is_equivalent_to_run() {
        let mut world_run = WorldState::new(Seed(9), test_preset(), Resolution::new(4, 4));
        let log_run: Rc<RefCell<Vec<&'static str>>> = Rc::new(RefCell::new(Vec::new()));
        make_abc_pipeline(log_run.clone())
            .run(&mut world_run)
            .expect("run should succeed");

        let mut world_run_from = WorldState::new(Seed(9), test_preset(), Resolution::new(4, 4));
        let log_run_from: Rc<RefCell<Vec<&'static str>>> = Rc::new(RefCell::new(Vec::new()));
        make_abc_pipeline(log_run_from.clone())
            .run_from(&mut world_run_from, 0)
            .expect("run_from(0) should succeed");

        assert_eq!(*log_run.borrow(), *log_run_from.borrow());
        assert_eq!(*log_run_from.borrow(), vec!["a", "b", "c"]);
    }

    #[test]
    fn run_from_n_skips_prefix_stages() {
        let mut world = WorldState::new(Seed(3), test_preset(), Resolution::new(4, 4));
        let log: Rc<RefCell<Vec<&'static str>>> = Rc::new(RefCell::new(Vec::new()));
        let pipeline = make_abc_pipeline(log.clone());

        pipeline
            .run(&mut world)
            .expect("initial run should succeed");
        assert_eq!(*log.borrow(), vec!["a", "b", "c"]);
        log.borrow_mut().clear();

        pipeline
            .run_from(&mut world, 2)
            .expect("run_from(2) should succeed");
        assert_eq!(
            *log.borrow(),
            vec!["c"],
            "only the final stage should have re-run"
        );
    }

    #[test]
    fn run_from_len_is_no_op() {
        let mut world = WorldState::new(Seed(4), test_preset(), Resolution::new(4, 4));
        let log: Rc<RefCell<Vec<&'static str>>> = Rc::new(RefCell::new(Vec::new()));
        let pipeline = make_abc_pipeline(log.clone());
        pipeline
            .run_from(&mut world, pipeline.len())
            .expect("run_from(len()) should succeed");
        assert!(
            log.borrow().is_empty(),
            "no stages should have run: {:?}",
            log.borrow()
        );
    }

    #[test]
    fn run_from_out_of_bounds_errors() {
        let mut world = WorldState::new(Seed(5), test_preset(), Resolution::new(4, 4));
        let log: Rc<RefCell<Vec<&'static str>>> = Rc::new(RefCell::new(Vec::new()));
        let pipeline = make_abc_pipeline(log.clone());

        let err = pipeline
            .run_from(&mut world, pipeline.len() + 1)
            .expect_err("run_from(len + 1) should error");
        let msg = err.to_string();
        assert!(
            msg.contains("exceeds pipeline length"),
            "expected StartIndexOutOfBounds, got: {msg}"
        );
        assert!(
            log.borrow().is_empty(),
            "no stages should have run on error: {:?}",
            log.borrow()
        );
    }

    // ── Sprint 4.A: timing capture tests ─────────────────────────────────────

    /// After `run()`, `world.derived.last_stage_timings` must be `Some` with
    /// one entry per stage (keyed by stage name).
    #[test]
    fn run_populates_last_stage_timings() {
        let mut world = WorldState::new(Seed(10), test_preset(), Resolution::new(4, 4));
        let log: Rc<RefCell<Vec<&'static str>>> = Rc::new(RefCell::new(Vec::new()));
        let pipeline = make_abc_pipeline(log.clone());

        pipeline.run(&mut world).expect("run should succeed");

        let timings = world
            .derived
            .last_stage_timings
            .as_ref()
            .expect("last_stage_timings must be Some after run()");
        assert_eq!(
            timings.len(),
            3,
            "3-stage pipeline must have 3 timing entries"
        );
        assert!(timings.contains_key("a"), "timings must have entry 'a'");
        assert!(timings.contains_key("b"), "timings must have entry 'b'");
        assert!(timings.contains_key("c"), "timings must have entry 'c'");
        for (name, t) in timings {
            assert!(
                t.cpu_ms >= 0.0,
                "stage '{name}' cpu_ms must be non-negative"
            );
            assert!(
                t.gpu_ms.is_none(),
                "stage '{name}' gpu_ms must be None in CPU substrate"
            );
        }
    }

    /// `run_from(1)` on a fresh world populates timings only for the executed
    /// stages (b, c) and does not include stage a.
    #[test]
    fn run_from_partial_populates_only_executed_stages() {
        let mut world = WorldState::new(Seed(11), test_preset(), Resolution::new(4, 4));
        let log: Rc<RefCell<Vec<&'static str>>> = Rc::new(RefCell::new(Vec::new()));
        let pipeline = make_abc_pipeline(log.clone());

        // Run from index 1 — skips 'a'.
        pipeline
            .run_from(&mut world, 1)
            .expect("run_from(1) should succeed");

        let timings = world
            .derived
            .last_stage_timings
            .as_ref()
            .expect("last_stage_timings must be Some after run_from(1)");
        // 'a' was skipped; only 'b' and 'c' ran.
        assert_eq!(
            timings.len(),
            2,
            "run_from(1) must produce 2 timing entries"
        );
        assert!(timings.contains_key("b"), "'b' must be timed");
        assert!(timings.contains_key("c"), "'c' must be timed");
        assert!(
            !timings.contains_key("a"),
            "'a' must not be timed (skipped)"
        );
    }

    /// `run_from(len())` (no-op) leaves `last_stage_timings` as an empty Some.
    #[test]
    fn run_from_len_produces_empty_timings() {
        let mut world = WorldState::new(Seed(12), test_preset(), Resolution::new(4, 4));
        let log: Rc<RefCell<Vec<&'static str>>> = Rc::new(RefCell::new(Vec::new()));
        let pipeline = make_abc_pipeline(log.clone());

        pipeline
            .run_from(&mut world, pipeline.len())
            .expect("run_from(len()) should succeed");

        // The map should be Some but empty (no stages ran).
        let timings = world
            .derived
            .last_stage_timings
            .as_ref()
            .expect("last_stage_timings must be Some even for no-op run_from");
        assert!(
            timings.is_empty(),
            "no-op run_from must produce empty timings"
        );
    }

    /// The GPU side-channel (`last_stage_gpu_ms`) is drained by `run_from` for
    /// each stage. After the run, it must be `None`.
    #[test]
    fn run_from_drains_gpu_side_channel() {
        let mut world = WorldState::new(Seed(13), test_preset(), Resolution::new(4, 4));
        // Pre-inject a GPU side-channel value; a real GpuBackend would do this.
        world.derived.last_stage_gpu_ms = Some(5.0);

        let log: Rc<RefCell<Vec<&'static str>>> = Rc::new(RefCell::new(Vec::new()));
        let pipeline = make_abc_pipeline(log.clone());

        // Only run stage 'a' (index 0). It should drain last_stage_gpu_ms.
        pipeline
            .run_from(&mut world, 0)
            .expect("run should succeed");

        // After run, the side-channel must be None (drained by the loop).
        assert!(
            world.derived.last_stage_gpu_ms.is_none(),
            "run_from must drain last_stage_gpu_ms to None"
        );

        // The pre-injected `gpu_ms = Some(5.0)` is drained by stage 'a's
        // `take()` after its run; subsequent stages see `None` because the
        // side-channel was cleared.
        let timings = world.derived.last_stage_timings.as_ref().unwrap();
        let a_timing = timings.get("a").expect("stage 'a' must have timing");
        assert_eq!(
            a_timing.gpu_ms,
            Some(5.0),
            "stage 'a' must capture the pre-injected GPU time"
        );
        let b_timing = timings.get("b").expect("stage 'b' must have timing");
        assert!(b_timing.gpu_ms.is_none(), "stage 'b' must have None gpu_ms");
    }

    /// `last_stage_timings` accumulates across sequential `run_from` calls
    /// on the same world: a second `run_from(2)` after `run_from(0)` merges
    /// the new entries rather than replacing the map.
    #[test]
    fn sequential_run_from_accumulates_timings() {
        let mut world = WorldState::new(Seed(14), test_preset(), Resolution::new(4, 4));
        let log: Rc<RefCell<Vec<&'static str>>> = Rc::new(RefCell::new(Vec::new()));
        let pipeline = make_abc_pipeline(log.clone());

        // First call: runs all stages (a, b, c).
        pipeline.run_from(&mut world, 0).expect("first run_from");
        {
            let t = world.derived.last_stage_timings.as_ref().unwrap();
            assert_eq!(t.len(), 3, "after run_from(0): 3 entries");
        }

        // Second call: re-runs only stage c (index 2). Should keep a+b from
        // the previous map and overwrite c.
        pipeline.run_from(&mut world, 2).expect("second run_from");
        let t = world.derived.last_stage_timings.as_ref().unwrap();
        assert_eq!(
            t.len(),
            3,
            "after run_from(2): still 3 entries (a+b preserved)"
        );
        assert!(
            t.contains_key("a"),
            "entry 'a' must be preserved from first run"
        );
        assert!(
            t.contains_key("b"),
            "entry 'b' must be preserved from first run"
        );
        assert!(
            t.contains_key("c"),
            "entry 'c' must be updated by second run"
        );
    }
}
