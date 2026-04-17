//! Rendering layer — terrain placeholder quad + overlay descriptor registry.

pub mod camera;
pub mod noise;
pub mod overlay;
pub mod overlay_export;
pub mod overlay_render;
pub mod palette;
pub mod sky;
pub mod terrain;

pub use camera::{
    ALL_PRESETS, CameraPreset, CameraPresetId, PRESET_HERO, PRESET_LOW_OBLIQUE, PRESET_TOP_DEBUG,
    preset_by_id, view_projection as camera_view_projection,
};
pub use noise::{BlueNoiseTexture, NoiseLoadError, load_blue_noise_2d};
pub use overlay::{OverlayDescriptor, OverlayRegistry, OverlaySource, ValueRange};
pub use overlay_export::bake_overlay_to_rgba8;
pub use overlay_render::OverlayRenderer;
pub use palette::{
    BASIN_ACCENT, DEEP_WATER, HIGHLAND, LOWLAND, MIDLAND, OVERLAY_NEUTRAL, PaletteId, RIVER,
    SHALLOW_WATER, SKY_HORIZON, SKY_ZENITH, sample as palette_sample,
    sample_f32 as palette_sample_f32,
};
pub use sky::SkyRenderer;
pub use terrain::{MeshData, TerrainRenderer, TerrainVertex, build_sea_quad, build_terrain_mesh};
