//! Golden-seed regression tests for the full Sprint 1A + 1B pipeline.
//!
//! Each test runs the canonical linear pipeline against a fixed
//! `(Seed, preset)` pair, computes a SummaryMetrics snapshot, and
//! compares it against the committed RON file in
//! `crates/data/golden/snapshots/`.
//!
//! Set `SNAPSHOT_UPDATE=1` to overwrite the snapshot with the observed
//! output (used to bootstrap the files or after a deliberate pipeline
//! change).

use std::path::PathBuf;

use data::golden::SummaryMetrics;
use island_core::seed::Seed;
use island_core::world::{Resolution, WorldState};

const RESOLUTION: u32 = 128; // smaller than production 256 to keep tests fast

fn run_pipeline(seed: u64, preset_name: &str) -> WorldState {
    let preset = data::presets::load_preset(preset_name).expect("preset must exist");
    let mut world = WorldState::new(Seed(seed), preset, Resolution::new(RESOLUTION, RESOLUTION));
    sim::default_pipeline()
        .run(&mut world)
        .expect("pipeline must succeed");
    world
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

    // Sprint 1B float summaries (same abs tolerance).
    assert!(
        (observed.mean_precipitation - expected.mean_precipitation).abs() < ABS_TOL,
        "mean_precipitation: {} vs {}",
        observed.mean_precipitation,
        expected.mean_precipitation
    );
    assert!(
        (observed.windward_leeward_precip_ratio - expected.windward_leeward_precip_ratio).abs()
            < ABS_TOL,
        "windward_leeward_precip_ratio"
    );
    assert!(
        (observed.mean_temperature_c - expected.mean_temperature_c).abs() < ABS_TOL,
        "mean_temperature_c"
    );
    assert!(
        (observed.mean_soil_moisture - expected.mean_soil_moisture).abs() < ABS_TOL,
        "mean_soil_moisture"
    );
    for i in 0..8 {
        assert!(
            (observed.biome_coverage_percent[i] - expected.biome_coverage_percent[i]).abs()
                < ABS_TOL,
            "biome_coverage_percent[{i}]"
        );
    }
    assert_eq!(observed.hex_count, expected.hex_count, "hex_count");

    // ── Sprint 3.5 DD8 hash witnesses ─────────────────────────────────────────
    // Compared as strings; committed pre-3.5 snapshots default to empty string
    // which will not match the live-compute values — this is the intended red
    // signal that a snapshot regen is needed (fixed by the 3.5.A c2 regen).
    assert_eq!(
        observed.hex_attrs_hash, expected.hex_attrs_hash,
        "hex_attrs_hash"
    );
    assert_eq!(
        observed.hex_debug_river_crossing_hash, expected.hex_debug_river_crossing_hash,
        "hex_debug_river_crossing_hash"
    );
    assert_eq!(
        observed.hex_coast_class_hash, expected.hex_coast_class_hash,
        "hex_coast_class_hash"
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
    let metrics = SummaryMetrics::compute(&world);
    compare_or_update("seed_42_volcanic_single.ron", &metrics);
}

#[test]
fn seed_123_volcanic_twin() {
    let world = run_pipeline(123, "volcanic_twin");
    let metrics = SummaryMetrics::compute(&world);
    compare_or_update("seed_123_volcanic_twin.ron", &metrics);
}

#[test]
fn seed_777_caldera() {
    let world = run_pipeline(777, "caldera");
    let metrics = SummaryMetrics::compute(&world);
    compare_or_update("seed_777_caldera.ron", &metrics);
}
