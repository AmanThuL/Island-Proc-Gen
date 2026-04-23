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
//! `hex_attrs_present`). Sprint 2 adds 3 more (`coast_type_well_formed`,
//! `erosion_no_explosion`, `erosion_no_excessive_sea_crossing`). Sprint
//! 2.5 Task 2.5.G adds 1 more (`basin_partition_post_erosion_well_formed`),
//! for a running total of 12. Sprint 3 Task 3.9 adds 4 more
//! (`sediment_bounded`, `deposition_zone_fraction_realistic`,
//! `coast_type_v2_well_formed`, `precipitation_mass_balance`), for a
//! total of **16 invariants** (the spec says 15; see note below).
//!
//! Sprint 3 note: the sprint spec quotes "15 invariants" (1A 4 + 1B 4 +
//! Sprint 2 3 + Sprint 3 4) but forgot to count
//! `basin_partition_post_erosion_well_formed`, which Sprint 2.5 added as
//! the 12th invariant. The real total is therefore 12 pre-Sprint-3 + 4 =
//! **16**, which is what `validation_stage_runs_all_16_sprint_3_invariants`
//! locks.
//!
//! To keep the stage working for pipelines that stop at 1A (e.g. validation
//! unit tests that only need the routing stack), each 1B check is gated on
//! its input field: if the field is still `None`, the invariant is skipped
//! with a `MissingPrecondition` error, and we treat that specific error as
//! "not applicable yet" rather than a failure. Every other error (actual
//! invariant violations) propagates normally.
//!
//! The Sprint 2 / 2.5 / 3 invariants self-skip (return `Ok(())`) when their
//! precondition fields are `None`, so they are called directly with `?`.

use island_core::pipeline::SimulationStage;
use island_core::preset::PrecipitationVariant;
use island_core::validation::{
    ValidationError, accumulation_monotone, basin_partition_dag,
    basin_partition_post_erosion_well_formed, biome_weights_normalized, coast_type_v2_well_formed,
    coast_type_well_formed, coastline_consistency, deposition_zone_fraction_realistic,
    erosion_no_excessive_sea_crossing, erosion_no_explosion, hex_attrs_present,
    hex_river_crossing_edges_in_range, precipitation_mass_balance, precipitation_nonneg,
    river_termination, sediment_bounded, temperature_physical_range,
};
use island_core::world::WorldState;

/// Run all 16 core validation invariants in order, short-circuiting on
/// the first real failure. 1B invariants whose preconditions are missing
/// are silently skipped. Sprint 2 / 2.5 / 3 invariants self-skip when
/// their precondition fields are `None`.
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

        // Sprint 3.5.B (DD3): 6-edge river crossing range check.
        // Self-skips (Ok) when hex_debug is None; runs after hex_attrs_present
        // so the hex_debug is always present when hex_attrs is.
        hex_river_crossing_edges_in_range(world)?;

        // Sprint 2 / 2.5: self-skipping invariants — return Ok(()) when their
        // precondition fields are None, so no `skip_if_missing` wrapper needed.
        coast_type_well_formed(world)?;
        erosion_no_explosion(world)?;
        erosion_no_excessive_sea_crossing(world)?;
        basin_partition_post_erosion_well_formed(world)?;

        // Sprint 3: 4 new invariants.
        //
        // `sediment_bounded` and `deposition_zone_fraction_realistic` both
        // need `authoritative.sediment`; they fire `SedimentFieldMissing`
        // (not `MissingPrecondition`) when the field is absent.  We treat
        // that error as "not applicable yet" via `skip_if_sediment_missing`
        // so the pre-Sprint-3 1A pipeline still works.
        skip_if_sediment_missing(sediment_bounded(world))?;
        skip_if_sediment_missing(deposition_zone_fraction_realistic(world))?;

        // `coast_type_v2_well_formed` self-skips when coast_type is None.
        coast_type_v2_well_formed(world)?;

        // `precipitation_mass_balance` is only meaningful for V3Lfpm.
        if matches!(
            world.preset.climate.precipitation_variant,
            PrecipitationVariant::V3Lfpm
        ) {
            skip_if_missing(precipitation_mass_balance(world))?;
        }

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

/// Collapse `SedimentFieldMissing` into `Ok(())`: Sprint 3 sediment
/// invariants can't run on pre-Sprint-3 pipelines that never initialised
/// `authoritative.sediment`. Treat the absence as "not applicable".
/// Every other `ValidationError` still propagates.
fn skip_if_sediment_missing(result: Result<(), ValidationError>) -> anyhow::Result<()> {
    match result {
        Ok(()) => Ok(()),
        Err(ValidationError::SedimentFieldMissing) => Ok(()),
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
            erosion: Default::default(),
            climate: Default::default(),
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

    /// End-to-end Sprint 2 integration test: run the full 19-stage canonical
    /// pipeline (18 `StageId` variants + tail `ValidationStage`) and assert
    /// all 12 invariants pass, including the 3 Sprint 2 additions and the
    /// Sprint 2.5 Task 2.5.G addition.
    ///
    /// Mirrors `full_sprint_1b_pipeline_passes_all_invariants` but uses
    /// `sim::default_pipeline()` (which includes `ErosionOuterLoop` and
    /// `CoastTypeStage`) and additionally checks that `erosion_baseline` and
    /// `coast_type` are populated before explicitly calling each of the 12
    /// invariants to confirm they all return `Ok`.
    #[test]
    fn full_sprint_2_pipeline_passes_all_12_invariants() {
        use island_core::validation::{
            accumulation_monotone, basin_partition_dag, basin_partition_post_erosion_well_formed,
            biome_weights_normalized, coast_type_well_formed, coastline_consistency,
            erosion_no_excessive_sea_crossing, erosion_no_explosion, hex_attrs_present,
            precipitation_nonneg, river_termination, temperature_physical_range,
        };

        let mut world = WorldState::new(Seed(42), volcanic_preset(), Resolution::new(64, 64));
        let pipeline = crate::default_pipeline();

        pipeline
            .run(&mut world)
            .expect("full Sprint 2 canonical pipeline must pass all 12 invariants");

        // Sprint 2 output fields must be populated.
        assert!(
            world.derived.erosion_baseline.is_some(),
            "erosion_baseline must be Some after ErosionOuterLoop"
        );
        assert!(
            world.derived.coast_type.is_some(),
            "coast_type must be Some after CoastTypeStage"
        );

        // Explicitly call each of the 12 invariants — if any regresses,
        // the test names the failing invariant directly.
        coastline_consistency(&world).expect("1. coastline_consistency");
        basin_partition_dag(&world).expect("2. basin_partition_dag");
        accumulation_monotone(&world).expect("3. accumulation_monotone");
        river_termination(&world).expect("4. river_termination");
        precipitation_nonneg(&world).expect("5. precipitation_nonneg");
        biome_weights_normalized(&world).expect("6. biome_weights_normalized");
        temperature_physical_range(&world).expect("7. temperature_physical_range");
        hex_attrs_present(&world).expect("8. hex_attrs_present");
        coast_type_well_formed(&world).expect("9. coast_type_well_formed");
        erosion_no_explosion(&world).expect("10. erosion_no_explosion");
        erosion_no_excessive_sea_crossing(&world).expect("11. erosion_no_excessive_sea_crossing");
        basin_partition_post_erosion_well_formed(&world)
            .expect("12. basin_partition_post_erosion_well_formed");
    }

    /// Integration test: full Sprint 2 canonical pipeline passes the new
    /// `basin_partition_post_erosion_well_formed` invariant (Task 2.5.G).
    ///
    /// Uses `sim::default_pipeline()` which runs all 18 stages + tail
    /// `ValidationStage`, ensuring the CC labelling pass in `BasinsStage`
    /// produces a well-formed partition that the new 12th invariant accepts.
    #[test]
    fn full_sprint_2_pipeline_passes_basin_partition_post_erosion_well_formed() {
        use island_core::validation::basin_partition_post_erosion_well_formed;

        let mut world = WorldState::new(Seed(42), volcanic_preset(), Resolution::new(64, 64));
        let pipeline = crate::default_pipeline();

        pipeline
            .run(&mut world)
            .expect("full Sprint 2 canonical pipeline must pass all 12 invariants");

        assert!(
            world.derived.basin_id.is_some(),
            "basin_id must be Some after BasinsStage"
        );
        assert!(
            world.derived.coast_mask.is_some(),
            "coast_mask must be Some after CoastMaskStage"
        );

        basin_partition_post_erosion_well_formed(&world)
            .expect("basin_partition_post_erosion_well_formed must pass on the Sprint 2 pipeline");
    }

    /// Sprint 3 Task 3.9: run the full canonical pipeline on the volcanic_single
    /// preset and assert all 16 invariants pass (12 Sprint 1A/1B/2/2.5 + 4 Sprint 3).
    ///
    /// Each invariant is called explicitly so that a future regression names the
    /// failing invariant directly rather than hiding behind the pipeline's
    /// short-circuit.
    #[test]
    fn validation_stage_runs_all_16_sprint_3_invariants() {
        use island_core::validation::{
            accumulation_monotone, basin_partition_dag, basin_partition_post_erosion_well_formed,
            biome_weights_normalized, coast_type_v2_well_formed, coast_type_well_formed,
            coastline_consistency, deposition_zone_fraction_realistic,
            erosion_no_excessive_sea_crossing, erosion_no_explosion, hex_attrs_present,
            precipitation_mass_balance, precipitation_nonneg, river_termination, sediment_bounded,
            temperature_physical_range,
        };

        let mut world = WorldState::new(Seed(42), volcanic_preset(), Resolution::new(64, 64));
        let pipeline = crate::default_pipeline();

        pipeline
            .run(&mut world)
            .expect("full Sprint 3 canonical pipeline must pass all 16 invariants");

        // Sprint 3 output fields must be populated.
        assert!(
            world.authoritative.sediment.is_some(),
            "authoritative.sediment must be Some after SedimentUpdateStage"
        );
        assert!(
            world.derived.coast_type.is_some(),
            "coast_type must be Some after CoastTypeStage"
        );
        assert!(
            world.baked.precipitation.is_some(),
            "baked.precipitation must be Some after PrecipitationStage"
        );

        // ── Sprint 1A (4) ─────────────────────────────────────────────────────
        coastline_consistency(&world).expect("1. coastline_consistency");
        basin_partition_dag(&world).expect("2. basin_partition_dag");
        accumulation_monotone(&world).expect("3. accumulation_monotone");
        river_termination(&world).expect("4. river_termination");

        // ── Sprint 1B (4) ─────────────────────────────────────────────────────
        precipitation_nonneg(&world).expect("5. precipitation_nonneg");
        biome_weights_normalized(&world).expect("6. biome_weights_normalized");
        temperature_physical_range(&world).expect("7. temperature_physical_range");
        hex_attrs_present(&world).expect("8. hex_attrs_present");

        // ── Sprint 2 / 2.5 (4) ───────────────────────────────────────────────
        coast_type_well_formed(&world).expect("9. coast_type_well_formed");
        erosion_no_explosion(&world).expect("10. erosion_no_explosion");
        erosion_no_excessive_sea_crossing(&world).expect("11. erosion_no_excessive_sea_crossing");
        basin_partition_post_erosion_well_formed(&world)
            .expect("12. basin_partition_post_erosion_well_formed");

        // ── Sprint 3 (4) ──────────────────────────────────────────────────────
        sediment_bounded(&world).expect("13. sediment_bounded");
        deposition_zone_fraction_realistic(&world).expect("14. deposition_zone_fraction_realistic");
        coast_type_v2_well_formed(&world).expect("15. coast_type_v2_well_formed");
        precipitation_mass_balance(&world).expect("16. precipitation_mass_balance");
    }

    /// Regression guard for the Sprint 1B wind-slider 0↔π acceptance pair.
    ///
    /// The runtime slider handler syncs the new `wind_dir` into
    /// `world.preset` and calls
    /// `pipeline.run_from(world, StageId::Precipitation as usize)`.
    /// If any downstream stage (Fog, Pet, WaterBalance, SoilMoisture,
    /// BiomeWeights) fails to re-execute or silently reads a stale
    /// input, the overlay renders identical before/after — exactly
    /// the visual-acceptance failure mode the Sprint 1B 60↔61
    /// screenshot pair was meant to catch.
    ///
    /// Snapshots five wind-dependent outputs and asserts all five
    /// mutate on `run_from`: `precipitation` (entry point),
    /// `fog_likelihood` (reads wind_dir directly, consumed by
    /// BiomeWeights), `soil_moisture`, `biome_weights`, and
    /// `dominant_biome_per_cell` (what the overlay renders). In
    /// practice `precipitation` and `dominant_biome_per_cell` carry
    /// the independent signal — the middle three are implied by the
    /// first in most regressions. They're listed anyway so the full
    /// re-run contract is explicit at the test site; a reader
    /// diagnosing a future regression can see the whole chain
    /// without jumping files.
    #[test]
    fn wind_dir_rerun_propagates_through_biome_chain() {
        use crate::StageId;
        let mut preset = volcanic_preset();
        preset.prevailing_wind_dir = 0.0;
        let mut world = WorldState::new(Seed(42), preset, Resolution::new(64, 64));

        // Use the canonical pipeline builder so `StageId::Precipitation as usize`
        // correctly resolves to PrecipitationStage's index. A bespoke pipeline
        // that omits any StageId variant (e.g. ErosionOuterLoop) shifts every
        // downstream index and silently breaks this symbolic lookup — that
        // regression hit on Sprint 2 Task 2.3 when ErosionOuterLoop was inserted.
        let pipeline = crate::default_pipeline();

        pipeline.run(&mut world).expect("initial run");
        let precip_a = world.baked.precipitation.as_ref().unwrap().data.clone();
        let fog_a = world.derived.fog_likelihood.as_ref().unwrap().data.clone();
        let soil_a = world.baked.soil_moisture.as_ref().unwrap().data.clone();
        let biome_a = world.baked.biome_weights.as_ref().unwrap().weights.clone();
        let dominant_a = world
            .derived
            .dominant_biome_per_cell
            .as_ref()
            .unwrap()
            .data
            .clone();

        world.preset.prevailing_wind_dir = std::f32::consts::PI;
        pipeline
            .run_from(&mut world, StageId::Precipitation as usize)
            .expect("rerun from Precipitation");

        let precip_b = &world.baked.precipitation.as_ref().unwrap().data;
        let fog_b = &world.derived.fog_likelihood.as_ref().unwrap().data;
        let soil_b = &world.baked.soil_moisture.as_ref().unwrap().data;
        let biome_b = &world.baked.biome_weights.as_ref().unwrap().weights;
        let dominant_b = &world.derived.dominant_biome_per_cell.as_ref().unwrap().data;

        assert_ne!(
            &precip_a, precip_b,
            "precipitation must change when wind flips 180°"
        );
        assert_ne!(
            &fog_a, fog_b,
            "fog_likelihood must change when wind flips 180°"
        );
        assert_ne!(
            &soil_a, soil_b,
            "soil_moisture must change when wind flips 180°"
        );
        assert_ne!(
            &biome_a, biome_b,
            "biome_weights must change when wind flips 180°"
        );
        assert_ne!(
            &dominant_a, dominant_b,
            "dominant_biome_per_cell must change when wind flips 180° — \
             if this fails, the wind-slider 0↔π screenshot pair renders identically"
        );
    }
}
