//! Rendering layer — terrain placeholder quad + overlay descriptor registry.

pub mod camera;
pub mod noise;
pub mod overlay;
pub mod overlay_export;
pub mod overlay_render;
pub mod palette;
pub mod sky;
pub mod terrain;
pub mod viewport;

pub use camera::{
    ALL_PRESETS, CameraPreset, CameraPresetId, PRESET_HERO, PRESET_LOW_OBLIQUE, PRESET_TOP_DEBUG,
    eye_position, preset_by_id, preset_by_name, view_projection as camera_view_projection,
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
pub use viewport::ViewportTextureSet;

/// Horizontal world extent applied to the terrain mesh and sea quad.
///
/// The heightfield `z_filled` lives in `[0.0, ~max_relief]` (typically
/// `~0.85`), while without this constant the XZ plane would span `[0, 1]`
/// — a 1 : 0.85 aspect ratio far steeper than any real volcanic island
/// (Pico ≈ 0.056, Mt. Fuji ≈ 0.17). Stretching XZ to `[0, 3]` yields
/// `~0.28` aspect, still a little dramatic but recognisable as an island.
///
/// Single source of truth: mesh builder, sea quad, camera preset LUT,
/// and interactive camera target all read this constant. No slider, no
/// `vertical_scale` compensation — see Sprint 2.6.A for the history.
pub const WORLD_XZ_EXTENT: f32 = 3.0;
