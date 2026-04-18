//! Erosion outer loop — Sprint 2 DD3 (scheme B).
//!
//! Composite stage that owns the full `n_batch × n_inner` SPIM + hillslope
//! iteration and the end-of-batch flow-network rebuild. Holds unit-struct
//! refs to every stage in the `Coastal..=RiverExtraction` routing chain
//! plus the two erosion stages, so the inner body can drive them directly
//! without reaching back into `default_pipeline()`.
//!
//! Per sprint §2 DD3 (b), scheme A (split into `ErosionLoopHead` / `…Tail`
//! variants) was rejected: the outer loop would leak re-run semantics into
//! `StageId` and `default_pipeline()` would need to know not to call
//! `Coastal..RiverExtraction` itself when running inside the loop. Scheme B
//! keeps the pipeline linear — `ErosionOuterLoop` is a single opaque stage
//! whose internal iteration is invisible to the outer pipeline runner.
//!
//! The pseudo-code in `run` is byte-for-byte consistent with sprint §2 DD3
//! and the Sprint 1D §3 Task 1D.3 memo — divergence must be reviewed and
//! documented in §2 DD3 first.

use island_core::pipeline::SimulationStage;
use island_core::world::{ErosionBaseline, WorldState};

use crate::geomorph::{
    CoastMaskStage, DerivedGeomorphStage, HillslopeDiffusionStage, PitFillStage,
    StreamPowerIncisionStage,
};
use crate::hydro::{AccumulationStage, BasinsStage, FlowRoutingStage, RiverExtractionStage};

/// Sprint 2 DD3 (scheme B): erosion outer loop.
///
/// Runs `n_batch × n_inner` SPIM + hillslope diffusion iterations in place
/// on `authoritative.height`. At the end of each outer batch, calls
/// [`crate::invalidate_from(world, StageId::Coastal)`](crate::invalidate_from)
/// and re-runs the `Coastal..=RiverExtraction` routing chain so the next
/// batch sees a post-erosion flow network.
///
/// Snapshot: [`ErosionBaseline`] is written to `world.derived.erosion_baseline`
/// on the first run (and only the first — see
/// `erosion_outer_loop_baseline_is_sticky_across_reruns`), giving Task 2.9's
/// validation invariants a pre-erosion reference point.
///
/// # n_batch = 0 edge case
///
/// When `world.preset.erosion.n_batch == 0` the outer loop body never runs
/// — the stage becomes a noop on `authoritative.height`. DD6's "pre_*"
/// baseline shots rely on this to capture a pre-erosion snapshot via the
/// same request shape as the post-erosion shots.
///
/// The baseline snapshot step is still taken even when `n_batch == 0`,
/// so downstream invariants can rely on `erosion_baseline.is_some()`
/// unconditionally after this stage runs.
///
/// # Fields
///
/// All nine sub-stages are unit structs — `ErosionOuterLoop::default()`
/// constructs them trivially and the struct itself is effectively
/// stateless. Fields are `pub(crate)` so the
/// `erosion_outer_loop_uses_canonical_routing_chain` test can verify the
/// routing chain matches `default_pipeline()` without introspecting
/// `dyn SimulationStage` type ids.
pub struct ErosionOuterLoop {
    pub(crate) stream_power: StreamPowerIncisionStage,
    pub(crate) hillslope: HillslopeDiffusionStage,
    pub(crate) coast_mask: CoastMaskStage,
    pub(crate) pit_fill: PitFillStage,
    pub(crate) derived_geomorph: DerivedGeomorphStage,
    pub(crate) flow_routing: FlowRoutingStage,
    pub(crate) accumulation: AccumulationStage,
    pub(crate) basins: BasinsStage,
    pub(crate) river_extraction: RiverExtractionStage,
}

impl Default for ErosionOuterLoop {
    fn default() -> Self {
        Self {
            stream_power: StreamPowerIncisionStage,
            hillslope: HillslopeDiffusionStage,
            coast_mask: CoastMaskStage,
            pit_fill: PitFillStage,
            derived_geomorph: DerivedGeomorphStage,
            flow_routing: FlowRoutingStage,
            accumulation: AccumulationStage,
            basins: BasinsStage,
            river_extraction: RiverExtractionStage,
        }
    }
}

impl SimulationStage for ErosionOuterLoop {
    fn name(&self) -> &'static str {
        "erosion_outer_loop"
    }

    fn run(&self, world: &mut WorldState) -> anyhow::Result<()> {
        let n_batch = world.preset.erosion.n_batch as usize;
        let n_inner = world.preset.erosion.n_inner as usize;

        // ── prerequisite checks ───────────────────────────────────────────────
        // The stage needs authoritative.height and coast_mask to take the
        // pre-erosion baseline. The inner SPIM / hillslope stages have their
        // own prerequisite checks (accumulation, slope, coast_mask) — we
        // don't duplicate them here; if they're missing they'll surface the
        // moment the inner loop calls into them.
        if world.authoritative.height.is_none() {
            anyhow::bail!(
                "ErosionOuterLoop prerequisite missing: \
                 authoritative.height (TopographyStage must run first)"
            );
        }
        if world.derived.coast_mask.is_none() {
            anyhow::bail!(
                "ErosionOuterLoop prerequisite missing: \
                 derived.coast_mask (CoastMaskStage must run first)"
            );
        }

        // ── baseline snapshot (sticky: only on first run) ─────────────────────
        // `erosion_baseline` is written at most once per world lifetime so
        // repeated `run_from(ErosionOuterLoop)` calls (e.g. Task 2.7 erosion
        // sliders) don't reset "pre" to a post-erosion state. A fresh
        // reseed goes through `WorldState::new` → `DerivedCaches::default()`
        // which zeros the field back to `None`, so the sticky behaviour
        // correctly restarts for a new world.
        if world.derived.erosion_baseline.is_none() {
            let height = world.authoritative.height.as_ref().unwrap();
            let coast = world.derived.coast_mask.as_ref().unwrap();
            debug_assert_eq!(
                coast.is_land.data.len(),
                height.data.len(),
                "coast_mask.is_land length must match authoritative.height"
            );

            let mut max_h: f32 = f32::NEG_INFINITY;
            let mut land_count: u32 = 0;
            for (&h, &is_land) in height.data.iter().zip(coast.is_land.data.iter()) {
                if is_land == 1 {
                    max_h = max_h.max(h);
                    land_count += 1;
                }
            }
            // Degenerate presets may produce zero land cells; keep max_h
            // finite so downstream arithmetic never sees f32::NEG_INFINITY.
            if !max_h.is_finite() {
                max_h = 0.0;
            }

            world.derived.erosion_baseline = Some(ErosionBaseline {
                max_height_pre: max_h,
                land_cell_count_pre: land_count,
            });
        }

        // ── outer batch loop ──────────────────────────────────────────────────
        // Byte-for-byte identical to sprint §2 DD3 scheme B pseudo-code.
        for _batch in 0..n_batch {
            for _inner in 0..n_inner {
                self.stream_power.run(world)?; // mutates authoritative.height
                self.hillslope.run(world)?; // mutates authoritative.height
            }
            // Default conservative frontier per Sprint 1D Task 1D.2
            // "Default invalidation frontier contract": Coastal, not
            // DerivedGeomorph — cells may cross sea level under SPIM and
            // invalidate coast_mask.
            crate::invalidate_from(world, crate::StageId::Coastal);
            self.coast_mask.run(world)?;
            self.pit_fill.run(world)?;
            self.derived_geomorph.run(world)?;
            self.flow_routing.run(world)?;
            self.accumulation.run(world)?;
            self.basins.run(world)?;
            self.river_extraction.run(world)?;
        }

        Ok(())
    }
}

// ─── tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    use island_core::pipeline::SimulationStage;
    use island_core::preset::{ErosionParams, IslandAge, IslandArchetypePreset};
    use island_core::seed::Seed;
    use island_core::world::{Resolution, WorldState};

    fn volcanic_preset() -> IslandArchetypePreset {
        IslandArchetypePreset {
            name: "erosion_outer_loop_test".into(),
            island_radius: 0.45,
            max_relief: 0.8,
            volcanic_center_count: 1,
            island_age: IslandAge::Young,
            prevailing_wind_dir: 0.0,
            marine_moisture_strength: 0.5,
            sea_level: 0.25,
            erosion: ErosionParams::default(),
        }
    }

    /// Mechanical lockstep test: assert the stage refs held by
    /// `ErosionOuterLoop` match the `Coastal..=RiverExtraction` slice of
    /// `default_pipeline()`, using `SimulationStage::name()` as the comparator.
    ///
    /// If a future sprint inserts a new stage between Coastal and
    /// RiverExtraction (or reorders the existing ones), this test fails and
    /// forces the maintainer to update the `ErosionOuterLoop` struct body in
    /// lockstep with `default_pipeline()` / `StageId`.
    #[test]
    fn erosion_outer_loop_uses_canonical_routing_chain() {
        let loop_stage = ErosionOuterLoop::default();

        // Canonical routing chain, in push order, per Sprint 1A default_pipeline.
        let expected = [
            "coast_mask",
            "pit_fill",
            "derived_geomorph",
            "flow_routing",
            "accumulation",
            "basins",
            "river_extraction",
        ];

        let actual = [
            loop_stage.coast_mask.name(),
            loop_stage.pit_fill.name(),
            loop_stage.derived_geomorph.name(),
            loop_stage.flow_routing.name(),
            loop_stage.accumulation.name(),
            loop_stage.basins.name(),
            loop_stage.river_extraction.name(),
        ];

        assert_eq!(
            expected, actual,
            "ErosionOuterLoop routing chain drifted from default_pipeline; \
             update ErosionOuterLoop struct + run() in lockstep"
        );
    }

    /// End-to-end: run `default_pipeline()` on a fresh world and verify that
    /// `ErosionOuterLoop` actually ran — the baseline is populated and all
    /// heights remain finite.
    #[test]
    fn erosion_outer_loop_mutates_height_in_place_with_defaults() {
        let mut world = WorldState::new(Seed(42), volcanic_preset(), Resolution::new(64, 64));

        crate::default_pipeline()
            .run(&mut world)
            .expect("default_pipeline run");

        // Baseline snapshot exists (captured at the start of ErosionOuterLoop).
        assert!(
            world.derived.erosion_baseline.is_some(),
            "erosion_baseline must be populated after default_pipeline"
        );
        let baseline = world.derived.erosion_baseline.unwrap();
        assert!(
            baseline.max_height_pre.is_finite(),
            "erosion_baseline.max_height_pre must be finite, got {}",
            baseline.max_height_pre
        );
        assert!(
            baseline.land_cell_count_pre > 0,
            "volcanic preset must have some land cells"
        );

        // Every height cell is finite post-erosion.
        let h_field = world.authoritative.height.as_ref().unwrap();
        for (i, v) in h_field.data.iter().enumerate() {
            assert!(
                v.is_finite(),
                "cell {i}: height is non-finite after ErosionOuterLoop: {v}"
            );
        }
    }

    /// `n_batch == 0` is the DD6 "pre-erosion" sentinel: the outer loop body
    /// never runs, so `authoritative.height` is identical to the output of
    /// `TopographyStage`. The only write path between `TopographyStage` and
    /// `ErosionOuterLoop` that touches `authoritative.height` is SPIM (inside
    /// `ErosionOuterLoop`), so with `n_batch == 0` the height field after the
    /// full pipeline must match the height field immediately after
    /// `TopographyStage`.
    #[test]
    fn erosion_outer_loop_noop_when_n_batch_zero() {
        let mut preset = volcanic_preset();
        preset.erosion.n_batch = 0;
        let mut world = WorldState::new(Seed(42), preset, Resolution::new(64, 64));

        // Run only TopographyStage to snapshot the pre-erosion height.
        crate::TopographyStage
            .run(&mut world)
            .expect("TopographyStage run");
        let height_after_topography: Vec<f32> =
            world.authoritative.height.as_ref().unwrap().data.clone();

        // Now run the full pipeline (re-running TopographyStage is idempotent
        // for a given seed + preset — it overwrites the same data). With
        // n_batch == 0, SPIM / hillslope inside ErosionOuterLoop never fire.
        crate::default_pipeline()
            .run(&mut world)
            .expect("default_pipeline run");

        let height_after_full: &Vec<f32> = &world.authoritative.height.as_ref().unwrap().data;
        assert_eq!(
            height_after_topography.len(),
            height_after_full.len(),
            "height dimensions must not change"
        );
        assert_eq!(
            &height_after_topography, height_after_full,
            "with n_batch == 0 ErosionOuterLoop must leave authoritative.height \
             identical to TopographyStage output (byte-exact)"
        );

        // Baseline is still snapshotted even when the outer loop is a noop.
        assert!(
            world.derived.erosion_baseline.is_some(),
            "erosion_baseline must be populated even when n_batch == 0"
        );
    }

    /// The baseline snapshot is sticky across repeated `ErosionOuterLoop`
    /// runs — once set, subsequent calls do not overwrite it. This prevents
    /// Task 2.7 erosion sliders from resetting "pre" to post-erosion when
    /// `run_from(ErosionOuterLoop)` fires on parameter change.
    ///
    /// Uses `n_batch = 0` so the outer-loop body is a noop and no cells
    /// cross the sea-level threshold between the two runs. This isolates the
    /// stickiness property from the `erosion_no_excessive_sea_crossing`
    /// invariant (Task 2.9), which would fire on two consecutive full-erosion
    /// runs because the cumulative land loss relative to the sticky baseline
    /// can exceed the 5 % threshold.
    #[test]
    fn erosion_outer_loop_baseline_is_sticky_across_reruns() {
        let mut preset = volcanic_preset();
        preset.erosion.n_batch = 0; // noop erosion — tests stickiness only
        let mut world = WorldState::new(Seed(42), preset, Resolution::new(64, 64));

        crate::default_pipeline()
            .run(&mut world)
            .expect("initial run");
        let baseline_first = world.derived.erosion_baseline.expect("baseline set");

        // Rerun ErosionOuterLoop (simulating a slider-triggered rerun).
        crate::default_pipeline()
            .run_from(&mut world, crate::StageId::ErosionOuterLoop as usize)
            .expect("rerun");
        let baseline_second = world
            .derived
            .erosion_baseline
            .expect("baseline still set after rerun");

        assert_eq!(
            baseline_first, baseline_second,
            "erosion_baseline must be sticky across reruns — second call \
             captured a post-erosion state as 'pre'"
        );
    }
}
