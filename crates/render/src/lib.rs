//! Rendering layer — terrain placeholder quad + overlay descriptor registry.

pub mod overlay;
pub mod palette;
pub mod terrain;

pub use overlay::{OverlayDescriptor, OverlayRegistry, OverlaySource, ValueRange};
pub use palette::{
    BASIN_ACCENT, DEEP_WATER, HIGHLAND, LOWLAND, MIDLAND, OVERLAY_NEUTRAL, PaletteId, RIVER,
    SHALLOW_WATER, sample as palette_sample, sample_f32 as palette_sample_f32,
};
pub use terrain::TerrainRenderer;
