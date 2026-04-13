//! Thin `std::fs`-level wrapper around `island_core::save`.
//!
//! This module is the only place in the codebase that opens real files for
//! save/load. `island_core::save` is `&Path`-free so the wasm/web target can
//! plug in its own byte source (IndexedDB blobs) without touching it.

use std::path::Path;

use island_core::save::{LoadedWorld, SaveMode};
use island_core::world::WorldState;

/// Write `world` to `path` using the given `mode`.
///
/// Creates the file (truncating if it exists). Delegates to
/// [`island_core::save::write_world`] for the actual byte-level encoding.
pub fn save_world_to_file(
    world: &WorldState,
    mode: SaveMode,
    path: &Path,
) -> anyhow::Result<()> {
    let mut f = std::fs::File::create(path)?;
    island_core::save::write_world(world, mode, &mut f)?;
    Ok(())
}

/// Load a [`LoadedWorld`] from `path`.
///
/// Opens the file for reading and delegates to
/// [`island_core::save::read_world`] for byte-level decoding.
pub fn load_world_from_file(path: &Path) -> anyhow::Result<LoadedWorld> {
    let mut f = std::fs::File::open(path)?;
    let loaded = island_core::save::read_world(&mut f)?;
    Ok(loaded)
}
