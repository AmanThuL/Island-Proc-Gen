//! Golden-seed regression tests for the Sprint 1A pipeline.
//!
//! Each test runs the full Sprint 1A sim pipeline against a fixed
//! `(Seed, preset)` pair, computes a SummaryMetrics snapshot, and
//! compares it against the committed RON file in
//! `crates/data/golden/snapshots/`.
//!
//! Set SNAPSHOT_UPDATE=1 to overwrite the snapshot with the observed
//! output (used to bootstrap the files or after a deliberate pipeline
//! change).

use std::path::PathBuf;

use data::golden::SummaryMetrics;
use island_core::pipeline::SimulationPipeline;
use island_core::seed::Seed;
use island_core::world::{Resolution, WorldState};
use sim::{
    AccumulationStage, BasinsStage, CoastMaskStage, DerivedGeomorphStage, FlowRoutingStage,
    PitFillStage, RiverExtractionStage, TopographyStage, ValidationStage,
};

const RESOLUTION: u32 = 128; // smaller than production 256 to keep tests fast

fn run_pipeline(seed: u64, preset_name: &str) -> WorldState {
    let preset = data::presets::load_preset(preset_name).expect("preset must exist");
    let mut world = WorldState::new(
        Seed(seed),
        preset,
        Resolution::new(RESOLUTION, RESOLUTION),
    );
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
    pipeline.run(&mut world).expect("pipeline must succeed");
    world
}

fn compute_metrics(world: &WorldState) -> SummaryMetrics {
    let height = world.authoritative.height.as_ref().unwrap();
    let z_filled = world.derived.z_filled.as_ref().unwrap();
    let slope = world.derived.slope.as_ref().unwrap();
    let coast = world.derived.coast_mask.as_ref().unwrap();
    let flow_dir = world.derived.flow_dir.as_ref().unwrap();
    let accum = world.derived.accumulation.as_ref().unwrap();
    let basin_id = world.derived.basin_id.as_ref().unwrap();
    let river = world.derived.river_mask.as_ref().unwrap();

    let land_cell_count = coast.land_cell_count;
    let coast_cell_count = coast.is_coast.data.iter().filter(|&&v| v == 1).count() as u32;
    let river_cell_count = river.data.iter().filter(|&&v| v == 1).count() as u32;
    let basin_count = basin_id.data.iter().copied().max().unwrap_or(0);
    let river_mouth_count = coast
        .river_mouth_mask
        .as_ref()
        .map(|m| m.data.iter().filter(|&&v| v == 1).count() as u32)
        .unwrap_or(0);

    let max_elevation = height.data.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    let max_elevation_filled = z_filled
        .data
        .iter()
        .cloned()
        .fold(f32::NEG_INFINITY, f32::max);
    let mean_slope = slope.data.iter().sum::<f32>() / slope.data.len() as f32;
    // longest_river_length: cheapest definition — largest accumulation on a river cell.
    // Sprint 2 will refine this to actual path length.
    let longest_river_length = river
        .data
        .iter()
        .zip(accum.data.iter())
        .filter_map(|(&r, &a)| if r == 1 { Some(a) } else { None })
        .fold(0.0f32, f32::max);
    let total_drainage_area = accum.data.iter().sum::<f32>() / accum.data.len() as f32;

    let height_blake3 = blake3_field_f32(&height.data);
    let z_filled_blake3 = blake3_field_f32(&z_filled.data);
    let flow_dir_blake3 = blake3_field_u8(&flow_dir.data);
    let accumulation_blake3 = blake3_field_f32(&accum.data);
    let basin_id_blake3 = blake3_field_u32(&basin_id.data);
    let river_mask_blake3 = blake3_field_u8(&river.data);

    SummaryMetrics {
        land_cell_count,
        coast_cell_count,
        river_cell_count,
        basin_count,
        river_mouth_count,
        max_elevation,
        max_elevation_filled,
        mean_slope,
        longest_river_length,
        total_drainage_area,
        height_blake3,
        z_filled_blake3,
        flow_dir_blake3,
        accumulation_blake3,
        basin_id_blake3,
        river_mask_blake3,
    }
}

fn blake3_field_f32(data: &[f32]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    for v in data {
        hasher.update(&v.to_le_bytes());
    }
    *hasher.finalize().as_bytes()
}

fn blake3_field_u8(data: &[u8]) -> [u8; 32] {
    *blake3::hash(data).as_bytes()
}

fn blake3_field_u32(data: &[u32]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    for v in data {
        hasher.update(&v.to_le_bytes());
    }
    *hasher.finalize().as_bytes()
}

fn snapshot_path(filename: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("golden")
        .join("snapshots")
        .join(filename)
}

fn compare_or_update(filename: &str, observed: &SummaryMetrics) {
    let path = snapshot_path(filename);
    let update = std::env::var("SNAPSHOT_UPDATE").is_ok();

    if update || !path.exists() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let ron_str = ron::ser::to_string_pretty(observed, ron::ser::PrettyConfig::default())
            .expect("serialize");
        std::fs::write(&path, ron_str).expect("write snapshot");
        println!("wrote snapshot: {}", path.display());
        return;
    }

    let expected_str = std::fs::read_to_string(&path).expect("read snapshot");
    let expected: SummaryMetrics = ron::from_str(&expected_str).expect("parse snapshot");

    // =============================================================
    // Field hash vs. abs-tolerance semantics — read before `unwrap`
    // =============================================================
    // `*_blake3` fields below are bit-exact locks on the sim pipeline.
    //
    // When the hash DIFFERS, first classify:
    //
    //   (A) Same host, same toolchain, same `cargo test` path as
    //       snapshot creation → THIS IS A BUG. Something in the
    //       deterministic sim pipeline changed (stage order, RNG
    //       fork name, field dtype, sediment init, pit-fill ε…).
    //       Fix the code or explicitly bump the snapshot with a
    //       commit message that names the change.
    //
    //   (B) Different host / CPU vector path / compiler version /
    //       GPU or wasm backend → EXPECTED DRIFT. Fall through to
    //       the float abs_tolerance checks; they remain the
    //       authoritative cross-platform contract. Do NOT update
    //       the committed snapshot from the drifted machine.
    //
    // Rule of thumb: the local dev machine's snapshot IS the lock;
    // CI / GPU / wasm paths only see the abs-tolerance half.
    // =============================================================

    // Integer fields: exact match.
    assert_eq!(
        observed.land_cell_count, expected.land_cell_count,
        "land_cell_count"
    );
    assert_eq!(
        observed.coast_cell_count, expected.coast_cell_count,
        "coast_cell_count"
    );
    assert_eq!(
        observed.river_cell_count, expected.river_cell_count,
        "river_cell_count"
    );
    assert_eq!(observed.basin_count, expected.basin_count, "basin_count");
    assert_eq!(
        observed.river_mouth_count, expected.river_mouth_count,
        "river_mouth_count"
    );

    // Float fields: abs tolerance 1e-4.
    const ABS_TOL: f32 = 1e-4;
    assert!(
        (observed.max_elevation - expected.max_elevation).abs() < ABS_TOL,
        "max_elevation: {} vs {}",
        observed.max_elevation,
        expected.max_elevation
    );
    assert!(
        (observed.max_elevation_filled - expected.max_elevation_filled).abs() < ABS_TOL,
        "max_elevation_filled"
    );
    assert!(
        (observed.mean_slope - expected.mean_slope).abs() < ABS_TOL,
        "mean_slope"
    );
    assert!(
        (observed.longest_river_length - expected.longest_river_length).abs() < ABS_TOL,
        "longest_river_length"
    );
    assert!(
        (observed.total_drainage_area - expected.total_drainage_area).abs() < ABS_TOL,
        "total_drainage_area"
    );

    // Field hashes: bit-exact on same host.
    assert_eq!(
        observed.height_blake3, expected.height_blake3,
        "height_blake3"
    );
    assert_eq!(
        observed.z_filled_blake3, expected.z_filled_blake3,
        "z_filled_blake3"
    );
    assert_eq!(
        observed.flow_dir_blake3, expected.flow_dir_blake3,
        "flow_dir_blake3"
    );
    assert_eq!(
        observed.accumulation_blake3, expected.accumulation_blake3,
        "accumulation_blake3"
    );
    assert_eq!(
        observed.basin_id_blake3, expected.basin_id_blake3,
        "basin_id_blake3"
    );
    assert_eq!(
        observed.river_mask_blake3, expected.river_mask_blake3,
        "river_mask_blake3"
    );
}

#[test]
fn seed_42_volcanic_single() {
    let world = run_pipeline(42, "volcanic_single");
    let metrics = compute_metrics(&world);
    compare_or_update("seed_42_volcanic_single.ron", &metrics);
}

#[test]
fn seed_123_volcanic_twin() {
    let world = run_pipeline(123, "volcanic_twin");
    let metrics = compute_metrics(&world);
    compare_or_update("seed_123_volcanic_twin.ron", &metrics);
}

#[test]
fn seed_777_caldera() {
    let world = run_pipeline(777, "caldera");
    let metrics = compute_metrics(&world);
    compare_or_update("seed_777_caldera.ron", &metrics);
}
