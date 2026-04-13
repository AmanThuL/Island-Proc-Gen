//! Rendering layer — terrain placeholder quad + overlay descriptor registry.

pub mod camera;
pub mod overlay;
pub mod palette;
pub mod terrain;

pub use camera::{
    ALL_PRESETS, CameraPreset, CameraPresetId, PRESET_HERO, PRESET_LOW_OBLIQUE, PRESET_TOP_DEBUG,
    preset_by_id, view_projection as camera_view_projection,
};
pub use overlay::{OverlayDescriptor, OverlayRegistry, OverlaySource, ValueRange};
pub use palette::{
    BASIN_ACCENT, DEEP_WATER, HIGHLAND, LOWLAND, MIDLAND, OVERLAY_NEUTRAL, PaletteId, RIVER,
    SHALLOW_WATER, sample as palette_sample, sample_f32 as palette_sample_f32,
};
pub use terrain::TerrainRenderer;
