//! Rendering layer — terrain placeholder quad + overlay descriptor registry.

pub mod overlay;
pub mod palette;
pub mod terrain;

pub use overlay::{OverlayDescriptor, OverlayRegistry, OverlaySource, ValueRange};
pub use palette::{sample as palette_sample, PaletteId};
pub use terrain::TerrainRenderer;
