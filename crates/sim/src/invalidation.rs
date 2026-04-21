//! Stage-output cache invalidation helper.
//!
//! [`invalidate_from`] is the single call site for clearing `derived.*` and
//! `baked.*` fields produced by stages at or after a given [`StageId`]
//! frontier. The caller is expected to follow up with
//! [`island_core::pipeline::SimulationPipeline::run_from`] to rebuild them.

use crate::StageId;
use island_core::world::WorldState;

/// Clear all `derived.*` and `baked.*` fields produced by stages at or
/// after `from`. Caller is expected to follow up with
/// `SimulationPipeline::run_from(&mut world, from as usize)` to rebuild
/// them.
///
/// Does NOT mutate `authoritative.height` — that is world truth written by
/// `TopographyStage` and `ErosionOuterLoop`.
///
/// **Exception (Task 3.1):** `authoritative.sediment` IS cleared by the
/// `Coastal` arm because its initial condition `hs_init = 0.1 * is_land`
/// is computed by `CoastMaskStage`. Any frontier at or before `Coastal`
/// (including `Topography`) therefore resets sediment; the next
/// `run_from(Coastal)` re-populates it.
///
/// Does NOT call the pipeline. The name is `invalidate_from`, not
/// `rerun_from` — keep the two actions separate so the caller can batch
/// multiple mutations before one rerun.
///
/// Lives in `sim`, not `core`: the mapping "`StageId::X` → which
/// `derived`/`baked` fields X produces" is pipeline policy, and
/// `StageId` itself is defined in `sim`. Putting this on
/// `WorldState::impl` would force `core → sim` reverse dep (violates
/// the Sprint 0 crate DAG / CLAUDE.md invariant #1).
///
/// Default frontier for `authoritative.height` mutation (Sprint 2
/// erosion and any future edit path) is `StageId::Coastal` — conservative
/// because height mutation may move cells across the sea-level threshold
/// and invalidate `coast_mask`. Advancing to `PitFill` requires a proof
/// (empirical test) that no cell crosses sea level; that optimisation is
/// NOT in scope for Sprint 1D or Sprint 2.
///
/// # Example
///
/// ```
/// use island_core::{seed::Seed, world::{Resolution, WorldState}};
/// use island_core::preset::{ErosionParams, IslandAge, IslandArchetypePreset};
/// use sim::{default_pipeline, StageId, invalidate_from};
///
/// let preset = IslandArchetypePreset {
///     name: "example".into(),
///     island_radius: 0.5,
///     max_relief: 0.5,
///     volcanic_center_count: 1,
///     island_age: IslandAge::Young,
///     prevailing_wind_dir: 0.0,
///     marine_moisture_strength: 0.5,
///     sea_level: 0.3,
///     // n_batch = 0: ErosionOuterLoop becomes a no-op so this small
///     // synthetic grid doesn't trigger the sea-crossing invariant.
///     erosion: ErosionParams { n_batch: 0, ..Default::default() },
///     climate: Default::default(),
/// };
/// let mut world = WorldState::new(Seed(42), preset, Resolution::new(32, 32));
/// let pipeline = default_pipeline();
/// pipeline.run(&mut world).expect("pipeline");
///
/// // Mutate a preset parameter that affects precipitation onward.
/// world.preset.prevailing_wind_dir = std::f32::consts::PI;
/// invalidate_from(&mut world, StageId::Precipitation);
/// pipeline
///     .run_from(&mut world, StageId::Precipitation as usize)
///     .expect("run_from");
/// ```
pub fn invalidate_from(world: &mut WorldState, from: StageId) {
    // Walk every stage from `from` to `HexProjection` (inclusive) and clear
    // the outputs that stage is responsible for. A raw numeric loop cannot
    // express this mapping — each StageId writes a different set of
    // derived/baked fields — so we iterate with an explicit match.
    //
    // `StageId` is `#[repr(usize)]` with contiguous discriminants 0..=17,
    // locked by `stage_id_indices_are_dense_and_canonical`. The `idx` range
    // is bounded by `StageId::HexProjection as usize = 17`.
    let start = from as usize;
    let end = StageId::HexProjection as usize;
    for idx in start..=end {
        let stage = match idx {
            0 => StageId::Topography,
            1 => StageId::Coastal,
            2 => StageId::PitFill,
            3 => StageId::DerivedGeomorph,
            4 => StageId::FlowRouting,
            5 => StageId::Accumulation,
            6 => StageId::Basins,
            7 => StageId::RiverExtraction,
            8 => StageId::ErosionOuterLoop,
            9 => StageId::CoastType,
            10 => StageId::Temperature,
            11 => StageId::Precipitation,
            12 => StageId::FogLikelihood,
            13 => StageId::Pet,
            14 => StageId::WaterBalance,
            15 => StageId::SoilMoisture,
            16 => StageId::BiomeWeights,
            17 => StageId::HexProjection,
            _ => unreachable!("idx is bounded by HexProjection = 17"),
        };
        clear_stage_outputs(world, stage);
    }
}

/// Clear only the `derived.*` / `baked.*` fields that `stage` is
/// responsible for writing.
///
/// Every arm lists exactly the fields that the corresponding `SimulationStage`
/// assigns in its `run` method. Adding a new `StageId` variant requires a
/// matching arm here — the `stage_id_indices_are_dense_and_canonical` test
/// enforces the enum is dense, which makes a missing arm a compile-time
/// exhaustive-match error.
fn clear_stage_outputs(world: &mut WorldState, stage: StageId) {
    match stage {
        // TopographyStage: writes derived.initial_uplift.
        // (authoritative.height + authoritative.sediment are NOT cleared —
        //  they are world truth, not caches.)
        //
        // `derived.erosion_baseline` is a one-shot pre-erosion snapshot
        // produced by `ErosionOuterLoop` on its first run. Conceptually it
        // is invalidated only by a **full reset** — re-running Topography
        // regenerates the pre-erosion height field, so the cached
        // snapshot is by definition stale. Downstream frontiers (Coastal,
        // PitFill, …, BiomeWeights) preserve the baseline so the outer
        // loop's own per-batch `invalidate_from(Coastal)` does not wipe
        // the sticky snapshot mid-iteration.
        StageId::Topography => {
            world.derived.initial_uplift = None;
            world.derived.erosion_baseline = None;
            // Task 3.3: `derived.deposition_flux` is a byproduct of
            // `ErosionOuterLoop`'s inner `SedimentUpdateStage`. It is
            // cleared here (Topography arm) — NOT in the Coastal arm —
            // because `ErosionOuterLoop` itself invokes
            // `invalidate_from(Coastal)` mid-batch, and if we cleared
            // `deposition_flux` there it would be wiped as the last
            // action of the outer loop (after the final batch's
            // routing-chain rebuild), leaving consumers with a None
            // field at the end of a successful pipeline run. Clearing
            // only on a full Topography-level reset means:
            //
            //   * `invalidate_from(Topography)` → df cleared + rerun
            //     repopulates it (this arm + ErosionOuterLoop output).
            //   * `invalidate_from(Coastal)` or later without rerun →
            //     df stays at its last-inner-step value (stale but
            //     consistent with the pre-invalidation sediment field).
            //   * `invalidate_from(Coastal)` + `run_from(Coastal)` →
            //     the downstream ErosionOuterLoop re-run overwrites df
            //     on its first inner step, so observable semantics
            //     match the "cleared" intent.
            //   * Outer loop's own mid-batch `invalidate_from(Coastal)`
            //     → df preserved, overwritten on next inner step (or
            //     kept as final state if this was the last batch).
            //
            // This is the same "produced by a late stage, safe to
            // stale-read" pattern that `erosion_baseline` above uses,
            // just with a different downstream-rerun side effect.
            world.derived.deposition_flux = None;
        }

        // CoastMaskStage: writes derived.coast_mask + derived.shoreline_normal
        // + authoritative.sediment (Task 3.1 initial condition).
        //
        // Sediment is cleared here (not in the Topography arm) because its
        // initial condition `hs_init(p) = 0.1 * is_land(p)` requires the
        // coast mask to be computed first. Any `invalidate_from(Topography)`
        // cascade propagates through here, so the sticky snapshot behaviour
        // is automatic: the next `run_from(Coastal)` will re-populate sediment.
        StageId::Coastal => {
            world.derived.coast_mask = None;
            world.derived.shoreline_normal = None;
            world.authoritative.sediment = None;
        }

        // PitFillStage: writes derived.z_filled.
        StageId::PitFill => {
            world.derived.z_filled = None;
        }

        // DerivedGeomorphStage: writes derived.slope + derived.curvature.
        StageId::DerivedGeomorph => {
            world.derived.slope = None;
            world.derived.curvature = None;
        }

        // FlowRoutingStage: writes derived.flow_dir.
        StageId::FlowRouting => {
            world.derived.flow_dir = None;
        }

        // AccumulationStage: writes derived.accumulation.
        StageId::Accumulation => {
            world.derived.accumulation = None;
        }

        // BasinsStage: writes derived.basin_id.
        StageId::Basins => {
            world.derived.basin_id = None;
        }

        // RiverExtractionStage: writes derived.river_mask and backfills
        // derived.coast_mask.river_mouth_mask in place.
        StageId::RiverExtraction => {
            world.derived.river_mask = None;
            if let Some(cm) = world.derived.coast_mask.as_mut() {
                cm.river_mouth_mask = None;
            }
        }

        // ErosionOuterLoop: mutates authoritative.height in place (which
        // is world truth, NOT a cache — never cleared here). Its
        // derived-side artefact `erosion_baseline` is a one-shot
        // pre-erosion snapshot whose only legitimate reset is a full
        // re-run of `TopographyStage` — see that arm above. Clearing it
        // here would be wiped by the outer loop's own per-batch
        // `invalidate_from(Coastal)` mid-iteration, leaving the baseline
        // as post-erosion state by the last batch. Leaving this arm as a
        // noop preserves the sticky baseline across the
        // `Coastal..=RiverExtraction` reroute.
        StageId::ErosionOuterLoop => {
            // deliberately empty — see module-level comment + docstring.
        }

        // CoastTypeStage: writes derived.coast_type.
        StageId::CoastType => {
            world.derived.coast_type = None;
        }

        // TemperatureStage: writes baked.temperature.
        StageId::Temperature => {
            world.baked.temperature = None;
        }

        // PrecipitationStage: writes baked.precipitation.
        // Sprint 3 DD4: also clears derived.precipitation_sweep_order so
        // the next V3 run rebuilds the sweep cache. Any slider-driven
        // invalidate_from(Precipitation) (e.g. wind direction change)
        // triggers a fresh sort.
        StageId::Precipitation => {
            world.baked.precipitation = None;
            world.derived.precipitation_sweep_order = None;
        }

        // FogLikelihoodStage: writes derived.fog_likelihood.
        StageId::FogLikelihood => {
            world.derived.fog_likelihood = None;
        }

        // PetStage: writes derived.pet.
        StageId::Pet => {
            world.derived.pet = None;
        }

        // WaterBalanceStage: writes derived.et + derived.runoff.
        StageId::WaterBalance => {
            world.derived.et = None;
            world.derived.runoff = None;
        }

        // SoilMoistureStage: writes baked.soil_moisture and
        // derived.fog_water_input (Sprint 3 DD5).
        StageId::SoilMoisture => {
            world.baked.soil_moisture = None;
            world.derived.fog_water_input = None;
        }

        // BiomeWeightsStage: writes baked.biome_weights + derived.dominant_biome_per_cell.
        StageId::BiomeWeights => {
            world.baked.biome_weights = None;
            world.derived.dominant_biome_per_cell = None;
        }

        // HexProjectionStage: writes derived.hex_grid + derived.hex_attrs
        // + derived.hex_dominant_per_cell + derived.hex_debug
        // + derived.hex_slope_variance_per_cell + derived.hex_accessibility_per_cell
        // + derived.hex_river_crossing_mask.
        StageId::HexProjection => {
            world.derived.hex_grid = None;
            world.derived.hex_attrs = None;
            world.derived.hex_dominant_per_cell = None;
            world.derived.hex_debug = None;
            world.derived.hex_slope_variance_per_cell = None;
            world.derived.hex_accessibility_per_cell = None;
            world.derived.hex_river_crossing_mask = None;
        }
    }
}

// ─── tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use island_core::preset::{IslandAge, IslandArchetypePreset};
    use island_core::seed::Seed;
    use island_core::world::{Resolution, WorldState};

    fn test_preset() -> IslandArchetypePreset {
        IslandArchetypePreset {
            name: "invalidation_test".into(),
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

    /// Run the full canonical pipeline on a fresh 64×64 world.
    fn run_full(seed: u64) -> WorldState {
        let mut world = WorldState::new(Seed(seed), test_preset(), Resolution::new(64, 64));
        crate::default_pipeline()
            .run(&mut world)
            .expect("default_pipeline run");
        world
    }

    // ── Test 1: invalidate_from(Topography) wipes every derived and baked field ─

    /// After a full pipeline run, `invalidate_from(Topography)` must set every
    /// `derived.*` and `baked.*` Option field to `None`. `seed`, `preset`,
    /// `resolution`, and `authoritative.height` must be unchanged.
    ///
    /// **Task 3.1 exception**: `authoritative.sediment` IS cleared by a
    /// Topography-level invalidation because it cascades through the Coastal
    /// arm, which owns sediment's initial condition. This is intentional:
    /// sediment's init (`hs_init = 0.1 * is_land`) depends on `is_land`, which
    /// is computed by `CoastMaskStage`. Re-running `Coastal` after a Topography
    /// invalidation will re-populate sediment correctly.
    #[test]
    fn invalidate_from_topography_clears_every_derived_and_baked_field() {
        let mut world = run_full(42);

        // Sanity: authoritative fields were populated by the full pipeline.
        assert!(world.authoritative.height.is_some());
        assert!(world.authoritative.sediment.is_some());
        // Sanity: CoastTypeStage (9) ran and produced a coast_type field.
        assert!(
            world.derived.coast_type.is_some(),
            "derived.coast_type must be Some after a full pipeline run"
        );
        let orig_seed = world.seed;
        let orig_preset = world.preset.clone();
        let orig_resolution = world.resolution;
        // Capture authoritative.height bytes to prove they are untouched.
        let height_bytes = world.authoritative.height.as_ref().unwrap().data.clone();

        invalidate_from(&mut world, StageId::Topography);

        // ── seed / preset / resolution are UNCHANGED ─────────────────────────
        assert_eq!(world.seed, orig_seed);
        assert_eq!(world.preset, orig_preset);
        assert_eq!(world.resolution, orig_resolution);
        // authoritative.height is NOT a cache — it is world truth and must
        // never be cleared by invalidate_from.
        assert_eq!(
            world.authoritative.height.as_ref().unwrap().data,
            height_bytes,
            "authoritative.height must not be touched by invalidate_from"
        );
        // authoritative.sediment IS cleared: its initial condition is produced
        // by CoastMaskStage, so invalidate_from(Topography) cascades through
        // the Coastal arm and resets it. The next run_from(Coastal) will
        // re-populate it from scratch (Task 3.1 spec lock).
        assert!(
            world.authoritative.sediment.is_none(),
            "authoritative.sediment must be cleared by invalidate_from(Topography) cascade"
        );

        // ── every derived.* field is None ─────────────────────────────────────
        assert!(world.derived.initial_uplift.is_none(), "initial_uplift");
        assert!(world.derived.coast_mask.is_none(), "coast_mask");
        assert!(world.derived.shoreline_normal.is_none(), "shoreline_normal");
        assert!(world.derived.coast_type.is_none(), "coast_type");
        assert!(world.derived.z_filled.is_none(), "z_filled");
        assert!(world.derived.slope.is_none(), "slope");
        assert!(world.derived.curvature.is_none(), "curvature");
        assert!(world.derived.flow_dir.is_none(), "flow_dir");
        assert!(world.derived.accumulation.is_none(), "accumulation");
        assert!(world.derived.basin_id.is_none(), "basin_id");
        assert!(world.derived.river_mask.is_none(), "river_mask");
        assert!(world.derived.erosion_baseline.is_none(), "erosion_baseline");
        assert!(world.derived.fog_likelihood.is_none(), "fog_likelihood");
        assert!(world.derived.pet.is_none(), "pet");
        assert!(world.derived.et.is_none(), "et");
        assert!(world.derived.runoff.is_none(), "runoff");
        assert!(world.derived.hex_grid.is_none(), "hex_grid");
        assert!(world.derived.hex_attrs.is_none(), "hex_attrs");
        assert!(
            world.derived.dominant_biome_per_cell.is_none(),
            "dominant_biome_per_cell"
        );
        assert!(
            world.derived.hex_dominant_per_cell.is_none(),
            "hex_dominant_per_cell"
        );
        assert!(world.derived.hex_debug.is_none(), "hex_debug");
        assert!(
            world.derived.hex_slope_variance_per_cell.is_none(),
            "hex_slope_variance_per_cell"
        );
        assert!(
            world.derived.hex_accessibility_per_cell.is_none(),
            "hex_accessibility_per_cell"
        );
        assert!(
            world.derived.hex_river_crossing_mask.is_none(),
            "hex_river_crossing_mask"
        );
        // Task 3.3: deposition_flux is cleared by the Topography arm (not
        // Coastal — see the arm comment in `clear_stage_outputs` and the
        // dedicated `invalidate_from_coastal_preserves_deposition_flux` test).
        assert!(
            world.derived.deposition_flux.is_none(),
            "deposition_flux must be cleared by invalidate_from(Topography) cascade"
        );
        // Task 3.5: fog_water_input is cleared by the SoilMoisture arm,
        // which is downstream of Topography.
        assert!(
            world.derived.fog_water_input.is_none(),
            "fog_water_input must be cleared by invalidate_from(Topography) cascade"
        );

        // ── every baked.* field is None ───────────────────────────────────────
        assert!(world.baked.temperature.is_none(), "baked.temperature");
        assert!(world.baked.precipitation.is_none(), "baked.precipitation");
        assert!(world.baked.soil_moisture.is_none(), "baked.soil_moisture");
        assert!(world.baked.biome_weights.is_none(), "baked.biome_weights");
    }

    // ── Test 2: mid-pipeline invalidation preserves upstream caches ───────────

    /// After `invalidate_from(Accumulation)`, all stages before `Accumulation`
    /// must still have their outputs populated; `Accumulation` and every
    /// downstream stage must have their outputs cleared.
    #[test]
    fn invalidate_from_accumulation_preserves_upstream_preserves_flow_dir_and_z_filled() {
        let mut world = run_full(42);

        invalidate_from(&mut world, StageId::Accumulation);

        // ── Stages 0..=4 outputs remain populated (upstream of Accumulation) ──
        // Topography (0)
        assert!(
            world.derived.initial_uplift.is_some(),
            "initial_uplift must be preserved (upstream of Accumulation)"
        );
        // Coastal (1)
        assert!(
            world.derived.coast_mask.is_some(),
            "coast_mask must be preserved"
        );
        assert!(
            world.derived.shoreline_normal.is_some(),
            "shoreline_normal must be preserved"
        );
        // PitFill (2)
        assert!(
            world.derived.z_filled.is_some(),
            "z_filled must be preserved"
        );
        // DerivedGeomorph (3)
        assert!(world.derived.slope.is_some(), "slope must be preserved");
        assert!(
            world.derived.curvature.is_some(),
            "curvature must be preserved"
        );
        // FlowRouting (4)
        assert!(
            world.derived.flow_dir.is_some(),
            "flow_dir must be preserved (directly upstream of Accumulation)"
        );

        // ── Accumulation (5) output is cleared ────────────────────────────────
        assert!(
            world.derived.accumulation.is_none(),
            "accumulation must be None (invalidated)"
        );

        // ── All downstream stage outputs are also cleared ──────────────────────
        // Basins (6)
        assert!(world.derived.basin_id.is_none(), "basin_id must be None");
        // RiverExtraction (7)
        assert!(
            world.derived.river_mask.is_none(),
            "river_mask must be None"
        );
        // ErosionOuterLoop (8): `erosion_baseline` is a sticky pre-erosion
        // snapshot, cleared only by `invalidate_from(Topography)` (see
        // Topography arm in `clear_stage_outputs`). `invalidate_from`
        // frontiers at or above Coastal leave it untouched, so the outer
        // loop's own per-batch `invalidate_from(Coastal)` cannot wipe it
        // mid-iteration. Here it must still be populated from the initial
        // full-pipeline run.
        assert!(
            world.derived.erosion_baseline.is_some(),
            "erosion_baseline must be preserved (sticky, only Topography frontier clears it)"
        );
        // CoastType (9) — downstream of Accumulation, must be cleared.
        assert!(
            world.derived.coast_type.is_none(),
            "coast_type must be None (downstream of Accumulation)"
        );
        // Temperature (10)
        assert!(
            world.baked.temperature.is_none(),
            "baked.temperature must be None"
        );
        // Precipitation (11)
        assert!(
            world.baked.precipitation.is_none(),
            "baked.precipitation must be None"
        );
        // FogLikelihood (12)
        assert!(
            world.derived.fog_likelihood.is_none(),
            "fog_likelihood must be None"
        );
        // Pet (13)
        assert!(world.derived.pet.is_none(), "pet must be None");
        // WaterBalance (14)
        assert!(world.derived.et.is_none(), "et must be None");
        assert!(world.derived.runoff.is_none(), "runoff must be None");
        // SoilMoisture (15)
        assert!(
            world.baked.soil_moisture.is_none(),
            "baked.soil_moisture must be None"
        );
        // Task 3.5: fog_water_input is written by SoilMoisture (15).
        assert!(
            world.derived.fog_water_input.is_none(),
            "fog_water_input must be None (downstream of Accumulation)"
        );
        // BiomeWeights (16)
        assert!(
            world.baked.biome_weights.is_none(),
            "baked.biome_weights must be None"
        );
        assert!(
            world.derived.dominant_biome_per_cell.is_none(),
            "dominant_biome_per_cell must be None"
        );
        // HexProjection (17)
        assert!(world.derived.hex_grid.is_none(), "hex_grid must be None");
        assert!(world.derived.hex_attrs.is_none(), "hex_attrs must be None");
        assert!(
            world.derived.hex_dominant_per_cell.is_none(),
            "hex_dominant_per_cell must be None"
        );
        assert!(
            world.derived.hex_debug.is_none(),
            "hex_debug must be None (downstream of Accumulation)"
        );
        assert!(
            world.derived.hex_slope_variance_per_cell.is_none(),
            "hex_slope_variance_per_cell must be None"
        );
        assert!(
            world.derived.hex_accessibility_per_cell.is_none(),
            "hex_accessibility_per_cell must be None"
        );
        assert!(
            world.derived.hex_river_crossing_mask.is_none(),
            "hex_river_crossing_mask must be None (downstream of Accumulation)"
        );
    }

    // ── Test 3: invalidate + run_from == fresh full run ───────────────────────

    /// Compute a deterministic composite hash over every `derived` and `baked`
    /// field that the full pipeline populates. Used to assert bit-exact
    /// equivalence between two world states after running the same pipeline.
    ///
    /// Uses blake3 directly so `data` does not need to be a dev-dep.
    fn world_cache_hash(world: &WorldState) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();

        // Helper closures to feed typed slice data into the hasher using
        // explicit little-endian byte encoding, consistent with the per-field
        // `.to_le_bytes()` calls used for `shoreline_normal` and `hex_attrs`.
        let hash_f32 = |h: &mut blake3::Hasher, data: &[f32]| {
            for v in data {
                h.update(&v.to_le_bytes());
            }
        };
        let hash_u32 = |h: &mut blake3::Hasher, data: &[u32]| {
            for v in data {
                h.update(&v.to_le_bytes());
            }
        };

        // ── derived fields ─────────────────────────────────────────────────────
        hash_f32(
            &mut hasher,
            &world
                .derived
                .initial_uplift
                .as_ref()
                .expect("initial_uplift")
                .data,
        );

        {
            let cm = world.derived.coast_mask.as_ref().expect("coast_mask");
            hasher.update(&cm.is_land.data);
            hasher.update(&cm.is_sea.data);
            hasher.update(&cm.is_coast.data);
            if let Some(rmm) = &cm.river_mouth_mask {
                hasher.update(&rmm.data);
            }
        }

        {
            let sn = world
                .derived
                .shoreline_normal
                .as_ref()
                .expect("shoreline_normal");
            // VectorField2D = ScalarField2D<[f32;2]>; each element is [f32;2].
            for pair in &sn.data {
                hasher.update(&pair[0].to_le_bytes());
                hasher.update(&pair[1].to_le_bytes());
            }
        }

        hash_f32(
            &mut hasher,
            &world.derived.z_filled.as_ref().expect("z_filled").data,
        );
        hash_f32(
            &mut hasher,
            &world.derived.slope.as_ref().expect("slope").data,
        );
        hash_f32(
            &mut hasher,
            &world.derived.curvature.as_ref().expect("curvature").data,
        );
        hasher.update(&world.derived.flow_dir.as_ref().expect("flow_dir").data);
        hash_f32(
            &mut hasher,
            &world
                .derived
                .accumulation
                .as_ref()
                .expect("accumulation")
                .data,
        );
        hash_u32(
            &mut hasher,
            &world.derived.basin_id.as_ref().expect("basin_id").data,
        );
        hasher.update(&world.derived.river_mask.as_ref().expect("river_mask").data);
        hasher.update(&world.derived.coast_type.as_ref().expect("coast_type").data);
        hash_f32(
            &mut hasher,
            &world
                .derived
                .fog_likelihood
                .as_ref()
                .expect("fog_likelihood")
                .data,
        );
        hash_f32(&mut hasher, &world.derived.pet.as_ref().expect("pet").data);
        hash_f32(&mut hasher, &world.derived.et.as_ref().expect("et").data);
        hash_f32(
            &mut hasher,
            &world.derived.runoff.as_ref().expect("runoff").data,
        );
        // Task 3.5: fog_water_input is produced by SoilMoistureStage when
        // fog_likelihood is available (full pipeline always satisfies this).
        hash_f32(
            &mut hasher,
            &world
                .derived
                .fog_water_input
                .as_ref()
                .expect("fog_water_input")
                .data,
        );
        hash_u32(
            &mut hasher,
            &world
                .derived
                .dominant_biome_per_cell
                .as_ref()
                .expect("dominant_biome_per_cell")
                .data,
        );
        hash_u32(
            &mut hasher,
            &world
                .derived
                .hex_dominant_per_cell
                .as_ref()
                .expect("hex_dominant_per_cell")
                .data,
        );

        {
            let hg = world.derived.hex_grid.as_ref().expect("hex_grid");
            hash_u32(&mut hasher, &hg.hex_id_of_cell.data);
        }

        {
            let ha = world.derived.hex_attrs.as_ref().expect("hex_attrs");
            for attr in &ha.attrs {
                hasher.update(&attr.elevation.to_le_bytes());
                hasher.update(&attr.slope.to_le_bytes());
                hasher.update(&attr.rainfall.to_le_bytes());
                hasher.update(&attr.temperature.to_le_bytes());
                hasher.update(&attr.moisture.to_le_bytes());
                for w in &attr.biome_weights {
                    hasher.update(&w.to_le_bytes());
                }
            }
        }

        {
            let hd = world.derived.hex_debug.as_ref().expect("hex_debug");
            for v in &hd.slope_variance {
                hasher.update(&v.to_le_bytes());
            }
            for v in &hd.accessibility_cost {
                hasher.update(&v.to_le_bytes());
            }
            // river_crossing: hash presence + edge values.
            for rc in &hd.river_crossing {
                match rc {
                    None => hasher.update(&[0u8]),
                    Some(c) => hasher.update(&[1u8, c.entry_edge, c.exit_edge]),
                };
            }
        }
        hash_f32(
            &mut hasher,
            &world
                .derived
                .hex_slope_variance_per_cell
                .as_ref()
                .expect("hex_slope_variance_per_cell")
                .data,
        );
        hash_f32(
            &mut hasher,
            &world
                .derived
                .hex_accessibility_per_cell
                .as_ref()
                .expect("hex_accessibility_per_cell")
                .data,
        );
        hasher.update(
            &world
                .derived
                .hex_river_crossing_mask
                .as_ref()
                .expect("hex_river_crossing_mask")
                .data,
        );

        // ── baked fields ───────────────────────────────────────────────────────
        hash_f32(
            &mut hasher,
            &world
                .baked
                .temperature
                .as_ref()
                .expect("baked.temperature")
                .data,
        );
        hash_f32(
            &mut hasher,
            &world
                .baked
                .precipitation
                .as_ref()
                .expect("baked.precipitation")
                .data,
        );
        hash_f32(
            &mut hasher,
            &world
                .baked
                .soil_moisture
                .as_ref()
                .expect("baked.soil_moisture")
                .data,
        );

        {
            let bw = world
                .baked
                .biome_weights
                .as_ref()
                .expect("baked.biome_weights");
            for row in &bw.weights {
                hash_f32(&mut hasher, row.as_slice());
            }
        }

        *hasher.finalize().as_bytes()
    }

    /// Build a Sprint 1A+1B pipeline **without** `ErosionOuterLoop`.
    ///
    /// Used by the `invalidate_plus_run_from_equals_fresh_run_at` helper so
    /// that the test's bit-exact invariant still holds. With
    /// `ErosionOuterLoop` in the pipeline, calling
    /// `invalidate_from(X) + run_from(X)` for any `X <= ErosionOuterLoop`
    /// would re-execute the SPIM + hillslope loop on an already-eroded
    /// `authoritative.height` — doubling the erosion applied to world_a
    /// versus world_b and breaking the bit-exact equality. That is a
    /// property of erosion mutating world truth, not a bug in
    /// `invalidate_from`, so the test deliberately sidesteps it by using a
    /// pipeline that doesn't touch `authoritative.height` after
    /// `TopographyStage`.
    fn non_eroding_pipeline() -> island_core::pipeline::SimulationPipeline {
        use crate::{
            AccumulationStage, BasinsStage, BiomeWeightsStage, CoastMaskStage, CoastTypeStage,
            DerivedGeomorphStage, FlowRoutingStage, FogLikelihoodStage, HexProjectionStage,
            PetStage, PitFillStage, PrecipitationStage, RiverExtractionStage, SoilMoistureStage,
            TemperatureStage, TopographyStage, ValidationStage, WaterBalanceStage,
        };
        let mut pipeline = island_core::pipeline::SimulationPipeline::new();
        pipeline.push(Box::new(TopographyStage));
        pipeline.push(Box::new(CoastMaskStage));
        pipeline.push(Box::new(PitFillStage));
        pipeline.push(Box::new(DerivedGeomorphStage));
        pipeline.push(Box::new(FlowRoutingStage));
        pipeline.push(Box::new(AccumulationStage));
        pipeline.push(Box::new(BasinsStage));
        pipeline.push(Box::new(RiverExtractionStage));
        // ErosionOuterLoop intentionally omitted — see docstring above.
        pipeline.push(Box::new(CoastTypeStage));
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
    }

    /// For non-eroding pipeline callers: translate a `StageId` discriminant
    /// (which includes `ErosionOuterLoop = 8`) into the corresponding index
    /// in [`non_eroding_pipeline`], where every post-ErosionOuterLoop stage
    /// is shifted left by one slot. **Precondition**: `id` must NOT be
    /// `StageId::ErosionOuterLoop` itself — that stage is omitted from
    /// `non_eroding_pipeline` by construction, so there is no valid index
    /// to return. Calling with `ErosionOuterLoop` is a test-helper misuse
    /// (a `run_from(ErosionOuterLoop)` under the non-eroding pipeline has
    /// no meaningful semantics); the debug_assert guards against silent
    /// mis-mapping to `RiverExtraction`.
    fn non_eroding_index(id: StageId) -> usize {
        debug_assert!(
            id != StageId::ErosionOuterLoop,
            "non_eroding_index called with StageId::ErosionOuterLoop — that stage is \
             omitted from non_eroding_pipeline, so no index exists for it"
        );
        let raw = id as usize;
        let erosion = StageId::ErosionOuterLoop as usize;
        if raw >= erosion { raw - 1 } else { raw }
    }

    /// Core of Test 3: build two identical worlds, run full pipeline on world_b
    /// as reference, then run full + invalidate_from(frontier) + run_from on
    /// world_a, and assert bit-exact equality of all derived/baked caches.
    fn invalidate_plus_run_from_equals_fresh_run_at(frontier: StageId) {
        let preset = test_preset();
        let seed = Seed(42);
        let res = Resolution::new(64, 64);

        // world_a: full run, then invalidate + run_from
        let mut world_a = WorldState::new(seed, preset.clone(), res);
        let pipeline = non_eroding_pipeline();
        pipeline
            .run(&mut world_a)
            .expect("world_a initial full run");
        invalidate_from(&mut world_a, frontier);
        pipeline
            .run_from(&mut world_a, non_eroding_index(frontier))
            .expect("world_a run_from");

        // world_b: single clean full run (reference)
        let mut world_b = WorldState::new(seed, preset, res);
        pipeline.run(&mut world_b).expect("world_b full run");

        let hash_a = world_cache_hash(&world_a);
        let hash_b = world_cache_hash(&world_b);

        assert_eq!(
            hash_a, hash_b,
            "invalidate_from({frontier:?}) + run_from must produce bit-exact output matching a fresh full run"
        );
    }

    #[test]
    fn invalidate_plus_run_from_equals_fresh_run_coastal() {
        invalidate_plus_run_from_equals_fresh_run_at(StageId::Coastal);
    }

    #[test]
    fn invalidate_plus_run_from_equals_fresh_run_pit_fill() {
        invalidate_plus_run_from_equals_fresh_run_at(StageId::PitFill);
    }

    #[test]
    fn invalidate_plus_run_from_equals_fresh_run_derived_geomorph() {
        invalidate_plus_run_from_equals_fresh_run_at(StageId::DerivedGeomorph);
    }

    #[test]
    fn invalidate_plus_run_from_equals_fresh_run_precipitation() {
        invalidate_plus_run_from_equals_fresh_run_at(StageId::Precipitation);
    }

    #[test]
    fn invalidate_plus_run_from_equals_fresh_run_biome_weights() {
        invalidate_plus_run_from_equals_fresh_run_at(StageId::BiomeWeights);
    }

    #[test]
    fn invalidate_plus_run_from_equals_fresh_run_hex_projection() {
        invalidate_plus_run_from_equals_fresh_run_at(StageId::HexProjection);
    }

    // ── Task 3.1: sediment invalidation tests ────────────────────────────────

    /// `invalidate_from(Coastal)` must clear `authoritative.sediment`.
    #[test]
    fn invalidation_clears_sediment() {
        let mut world = run_full(42);
        assert!(
            world.authoritative.sediment.is_some(),
            "sediment must be Some after a full pipeline run"
        );

        invalidate_from(&mut world, StageId::Coastal);

        assert!(
            world.authoritative.sediment.is_none(),
            "authoritative.sediment must be None after invalidate_from(Coastal)"
        );
    }

    /// `invalidate_from(Topography)` cascades through the Coastal arm and
    /// must also clear `authoritative.sediment`.
    #[test]
    fn invalidate_from_topography_cascades_to_sediment() {
        let mut world = run_full(42);
        assert!(
            world.authoritative.sediment.is_some(),
            "sediment must be Some after a full pipeline run"
        );

        invalidate_from(&mut world, StageId::Topography);

        assert!(
            world.authoritative.sediment.is_none(),
            "authoritative.sediment must be None after invalidate_from(Topography) cascade"
        );
    }

    // ── Task 3.3: deposition_flux invalidation ───────────────────────────────

    /// `invalidate_from(Topography)` must clear `derived.deposition_flux`.
    ///
    /// The field is cleared under the Topography arm (NOT Coastal), matching
    /// the `erosion_baseline` placement pattern: both are byproducts of
    /// `ErosionOuterLoop` which itself calls `invalidate_from(Coastal)` mid-
    /// batch, so attaching the clear to the Coastal arm would wipe the field
    /// as the last action of the outer loop (after the final batch's
    /// routing-chain rebuild) and leave consumers with a None field at the
    /// end of a successful pipeline run. See the `StageId::Topography` arm
    /// comment in `clear_stage_outputs` for the full reasoning.
    ///
    /// Observable semantics under `invalidate_from(Coastal) + run_from(Coastal)`
    /// are still correct: the downstream `ErosionOuterLoop` re-run
    /// overwrites `deposition_flux` on its first inner step, so the
    /// "coast-level" invalidation's consumers see fresh values.
    #[test]
    fn invalidate_from_topography_clears_deposition_flux() {
        let mut world = run_full(42);
        assert!(
            world.derived.deposition_flux.is_some(),
            "deposition_flux must be Some after a full pipeline run \
             (erosion inner step populates it)"
        );

        invalidate_from(&mut world, StageId::Topography);

        assert!(
            world.derived.deposition_flux.is_none(),
            "derived.deposition_flux must be None after invalidate_from(Topography)"
        );
    }

    /// Defensive: `invalidate_from(Coastal)` must NOT clear
    /// `derived.deposition_flux`. This is the counterpart to the Topography
    /// test above — the field lives across Coastal-level invalidations
    /// because `ErosionOuterLoop`'s own per-batch
    /// `invalidate_from(Coastal)` mid-loop would otherwise wipe it as the
    /// last action of a successful pipeline run. Downstream consumers can
    /// rely on the field being populated after any pipeline run that
    /// reaches `ErosionOuterLoop`.
    #[test]
    fn invalidate_from_coastal_preserves_deposition_flux() {
        let mut world = run_full(42);
        assert!(world.derived.deposition_flux.is_some());

        invalidate_from(&mut world, StageId::Coastal);

        assert!(
            world.derived.deposition_flux.is_some(),
            "derived.deposition_flux must be preserved across invalidate_from(Coastal) \
             — see ErosionOuterLoop mid-batch invalidation rationale"
        );
    }
}
