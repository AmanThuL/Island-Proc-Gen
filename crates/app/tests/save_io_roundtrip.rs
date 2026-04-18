//! Integration test for `app::save_io` — exercises the `std::fs`-level Path
//! wrapper using `tempfile` for a real on-disk round-trip.
//!
//! These tests run against the `app` lib target, not `core`, so `tempfile`
//! stays out of `core`'s dependency tree.

use app::save_io::{load_world_from_file, save_world_to_file};
use island_core::preset::{IslandAge, IslandArchetypePreset};
use island_core::save::{LoadedWorld, SaveMode};
use island_core::seed::Seed;
use island_core::world::{Resolution, WorldState};

fn make_test_preset() -> IslandArchetypePreset {
    IslandArchetypePreset {
        name: "integration_test".into(),
        island_radius: 0.5,
        max_relief: 0.5,
        volcanic_center_count: 1,
        island_age: IslandAge::Young,
        prevailing_wind_dir: 0.0,
        marine_moisture_strength: 0.5,
        sea_level: 0.3,
        erosion: Default::default(),
    }
}

// ── Test 1: SeedReplay round-trip via tempfile ────────────────────────────────

#[test]
fn save_io_seed_replay_roundtrip_via_tempfile() {
    let tempdir = tempfile::tempdir().expect("tempdir");
    let path = tempdir.path().join("world.ipgs");

    let world = WorldState::new(Seed(42), make_test_preset(), Resolution::new(128, 128));

    save_world_to_file(&world, SaveMode::SeedReplay, &path).expect("save");
    let loaded = load_world_from_file(&path).expect("load");

    match loaded {
        LoadedWorld::SeedReplay {
            seed,
            preset_name,
            resolution,
        } => {
            assert_eq!(seed, world.seed);
            assert_eq!(preset_name, "integration_test");
            assert_eq!(resolution, world.resolution);
        }
        _ => panic!("expected SeedReplay variant"),
    }
}

// ── Test 2: Minimal round-trip via tempfile ───────────────────────────────────

#[test]
fn save_io_minimal_roundtrip_via_tempfile() {
    use island_core::field::ScalarField2D;

    let tempdir = tempfile::tempdir().expect("tempdir");
    let path = tempdir.path().join("world_minimal.ipgs");

    let mut world = WorldState::new(Seed(99), make_test_preset(), Resolution::new(8, 8));

    // Populate both authoritative fields
    let mut height = ScalarField2D::<f32>::new(8, 8);
    for (i, elem) in height.data.iter_mut().enumerate() {
        *elem = i as f32 * 0.01;
    }
    let mut sediment = ScalarField2D::<f32>::new(8, 8);
    for (i, elem) in sediment.data.iter_mut().enumerate() {
        *elem = i as f32 * 0.005;
    }
    world.authoritative.height = Some(height);
    world.authoritative.sediment = Some(sediment);

    save_world_to_file(&world, SaveMode::Minimal, &path).expect("save");
    let loaded = load_world_from_file(&path).expect("load");

    match loaded {
        LoadedWorld::Minimal(w2) => {
            assert_eq!(w2.seed, world.seed);
            assert_eq!(w2.preset, world.preset);
            assert_eq!(w2.resolution, world.resolution);
            assert_eq!(
                w2.authoritative.height.as_ref().unwrap().to_bytes(),
                world.authoritative.height.as_ref().unwrap().to_bytes(),
            );
            assert_eq!(
                w2.authoritative.sediment.as_ref().unwrap().to_bytes(),
                world.authoritative.sediment.as_ref().unwrap().to_bytes(),
            );
        }
        _ => panic!("expected Minimal variant"),
    }
}
