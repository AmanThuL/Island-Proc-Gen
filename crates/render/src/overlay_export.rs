//! CPU overlay bake — shared by both the GPU pipeline and the headless export.
//!
//! [`bake_overlay_to_rgba8`] is the single implementation used by two callers:
//! * [`crate::overlay_render`]'s `bake_descriptor` — uploads the result as a
//!   wgpu texture for interactive rendering.
//! * `app::headless::executor` — writes the bytes directly to PNG as the
//!   Sprint 1C determinism truth path.
//!
//! Both callers receive byte-identical output for the same
//! `(descriptor, world)` pair (contract AD7).

use island_core::world::WorldState;

use crate::overlay::{OverlayDescriptor, ResolvedField, ValueRange, resolve_scalar_source};
use crate::palette;

/// Bake one overlay descriptor to a row-major RGBA8 byte buffer.
///
/// Returns `None` if the descriptor's source field is not yet populated
/// in `world` (e.g. `derived.z_filled` before the pipeline has run).
pub fn bake_overlay_to_rgba8(
    desc: &OverlayDescriptor,
    world: &WorldState,
) -> Option<(Vec<u8>, u32, u32)> {
    // Collect per-cell values as f32 plus width/height.
    let (width, height, values) = match resolve_scalar_source(world, desc.source)? {
        ResolvedField::F32(f) => (f.width, f.height, f.data.to_vec()),
        ResolvedField::U32(f) => (
            f.width,
            f.height,
            f.data.iter().map(|&v| v as f32).collect(),
        ),
        ResolvedField::Mask(m) => (
            m.width,
            m.height,
            m.data.iter().map(|&v| v as f32).collect(),
        ),
    };

    // Per-value transform: LogCompressed works in ln(1 + max(v, 0)) space;
    // all other ranges use identity.
    //
    // `ValueRange::LogCompressed::resolve()` applies the same ln transform to
    // the min/max internally — callers pass RAW extents. The per-pixel pass
    // here must apply the same transform to stay consistent (no double-log).
    let transform = |v: f32| -> f32 {
        match desc.value_range {
            ValueRange::LogCompressed => (1.0_f32 + v.max(0.0)).ln(),
            _ => v,
        }
    };

    let (raw_min, raw_max) = values
        .iter()
        .copied()
        .fold((f32::INFINITY, f32::NEG_INFINITY), |(mn, mx), v| {
            (mn.min(v), mx.max(v))
        });
    let (raw_min, raw_max) = if raw_min.is_finite() && raw_max.is_finite() {
        (raw_min, raw_max)
    } else {
        (0.0, 1.0)
    };

    let (lo, hi) = desc.value_range.resolve(raw_min, raw_max);
    let span = (hi - lo).max(1e-6);

    let mut rgba = Vec::with_capacity(width as usize * height as usize * 4);
    for &v in &values {
        let t = ((transform(v) - lo) / span).clamp(0.0, 1.0);
        rgba.extend_from_slice(&palette::sample(desc.palette, t));
    }

    Some((rgba, width, height))
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use island_core::{
        field::{MaskField2D, ScalarField2D},
        preset::{IslandAge, IslandArchetypePreset},
        seed::Seed,
        world::{Resolution, WorldState},
    };

    use crate::overlay::{OverlayDescriptor, OverlaySource, ValueRange};
    use crate::palette::{self, PaletteId};

    fn test_preset() -> IslandArchetypePreset {
        IslandArchetypePreset {
            name: "test".into(),
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

    fn test_world() -> WorldState {
        WorldState::new(Seed(0), test_preset(), Resolution::new(4, 4))
    }

    // 1. Elevation bake: known ramp values map to the expected palette colours.
    #[test]
    fn bake_overlay_to_rgba8_elevation_matches_palette() {
        let mut world = test_world();

        // Build a 4×1 field with values [0.0, 0.25, 0.5, 0.75].
        // Width=4, Height=1 so row_major index == x.
        let mut field = ScalarField2D::<f32>::new(4, 1);
        field.set(0, 0, 0.0);
        field.set(1, 0, 0.25);
        field.set(2, 0, 0.5);
        field.set(3, 0, 0.75);
        world.derived.z_filled = Some(field);

        let desc = OverlayDescriptor {
            id: "test_elev",
            label: "Test elevation",
            source: OverlaySource::ScalarDerived("z_filled"),
            palette: PaletteId::TerrainHeight,
            value_range: ValueRange::Auto,
            visible: true,
        };

        let (rgba, width, height) = bake_overlay_to_rgba8(&desc, &world)
            .expect("z_filled is populated, should return Some");

        assert_eq!(width, 4);
        assert_eq!(height, 1);
        assert_eq!(rgba.len(), 16); // 4 pixels × 4 bytes

        // With Auto, lo=0.0, hi=0.75. Check pixel 0 (t=0) and pixel 3 (t=1).
        // t=0.0 → min of [0.0,0.25,0.5,0.75] → t=(0.0-0.0)/(0.75)=0.0
        let expected_t0 = palette::sample(PaletteId::TerrainHeight, 0.0);
        assert_eq!(&rgba[0..4], &expected_t0, "pixel 0 should match t=0.0");

        // t=1.0 → max = 0.75 → t=(0.75-0.0)/0.75 = 1.0
        let expected_t1 = palette::sample(PaletteId::TerrainHeight, 1.0);
        assert_eq!(&rgba[12..16], &expected_t1, "pixel 3 should match t=1.0");

        // t=0.5 for pixel 2 (value=0.5): t=(0.5-0.0)/0.75 = 0.6667
        let expected_t_mid = palette::sample(PaletteId::TerrainHeight, 0.5 / 0.75);
        assert_eq!(&rgba[8..12], &expected_t_mid, "pixel 2 mismatch");
    }

    // 2. Binary-blue mask: 0 → transparent, 1 → RIVER colour.
    #[test]
    fn bake_overlay_to_rgba8_mask_binary_blue() {
        let mut world = test_world();

        let mut mask = MaskField2D::new(2, 1);
        mask.set(0, 0, 0); // should be transparent
        mask.set(1, 0, 1); // should be RIVER colour
        world.derived.river_mask = Some(mask);

        let desc = OverlayDescriptor {
            id: "test_river",
            label: "Test river",
            source: OverlaySource::Mask("river_mask"),
            palette: PaletteId::BinaryBlue,
            value_range: ValueRange::Fixed(0.0, 1.0),
            visible: true,
        };

        let (rgba, width, height) =
            bake_overlay_to_rgba8(&desc, &world).expect("river_mask is populated");

        assert_eq!(width, 2);
        assert_eq!(height, 1);
        assert_eq!(rgba.len(), 8);

        // Pixel 0 (value=0 → t=0.0 < 0.5): BinaryBlue gives alpha=0.
        assert_eq!(rgba[3], 0, "mask=0 pixel must be transparent");

        // Pixel 1 (value=1 → t=1.0 >= 0.5): BinaryBlue gives RIVER colour opaque.
        let river_px = palette::sample(PaletteId::BinaryBlue, 1.0);
        assert_eq!(
            &rgba[4..8],
            &river_px,
            "mask=1 pixel should be RIVER colour"
        );
        assert_eq!(rgba[7], 255, "mask=1 pixel must be opaque");
    }

    // 3. None when the field is not populated.
    #[test]
    fn bake_overlay_to_rgba8_returns_none_if_field_missing() {
        let world = test_world(); // no pipeline run, all derived fields are None

        let desc = OverlayDescriptor {
            id: "test_missing",
            label: "Missing",
            source: OverlaySource::ScalarDerived("z_filled"),
            palette: PaletteId::TerrainHeight,
            value_range: ValueRange::Auto,
            visible: true,
        };

        assert!(
            bake_overlay_to_rgba8(&desc, &world).is_none(),
            "should return None when z_filled is not populated"
        );
    }

    // 4. Determinism contract: same descriptor + same world → byte-identical output.
    #[test]
    fn bake_is_deterministic_for_same_world() {
        let mut world = test_world();

        let mut field = ScalarField2D::<f32>::new(4, 4);
        for y in 0..4u32 {
            for x in 0..4u32 {
                field.set(x, y, (x + y * 4) as f32 / 16.0);
            }
        }
        world.derived.z_filled = Some(field);

        let desc = OverlayDescriptor {
            id: "test_determinism",
            label: "Determinism",
            source: OverlaySource::ScalarDerived("z_filled"),
            palette: PaletteId::TerrainHeight,
            value_range: ValueRange::Auto,
            visible: true,
        };

        let first = bake_overlay_to_rgba8(&desc, &world).expect("z_filled is populated");
        let second =
            bake_overlay_to_rgba8(&desc, &world).expect("z_filled is populated on second call");

        assert_eq!(
            first.0, second.0,
            "bake_overlay_to_rgba8 must be byte-identical across two calls on the same world"
        );
        assert_eq!(first.1, second.1, "width must be identical");
        assert_eq!(first.2, second.2, "height must be identical");
    }
}
