//! Hex-surface invariants — shape and biome-weight-vector consistency of the
//! hex attribute grid.
//!
//! `hex_attrs_present` is the only member of this family in v1.
//! Sprint 5 S1's real-hex rework and the hex-grammar extensions planned for
//! Sprint 3.5.D will add further invariants here.

use crate::world::WorldState;

use super::ValidationError;

/// `hex_attrs.attrs.len() == cols * rows`, and every entry's
/// `biome_weights` vector length matches the canonical biome count.
pub fn hex_attrs_present(world: &WorldState) -> Result<(), ValidationError> {
    let attrs = world
        .derived
        .hex_attrs
        .as_ref()
        .ok_or(ValidationError::MissingPrecondition {
            field: "derived.hex_attrs",
        })?;

    let expected = (attrs.cols * attrs.rows) as usize;
    if attrs.attrs.len() != expected {
        return Err(ValidationError::HexAttrsShapeMismatch {
            cols: attrs.cols,
            rows: attrs.rows,
            got: attrs.attrs.len(),
        });
    }

    let expected_biome_count = crate::world::BiomeType::COUNT;
    for (i, hex) in attrs.attrs.iter().enumerate() {
        if hex.biome_weights.len() != expected_biome_count {
            let col = (i as u32) % attrs.cols;
            let row = (i as u32) / attrs.cols;
            return Err(ValidationError::HexBiomeWeightsLengthMismatch {
                col,
                row,
                got: hex.biome_weights.len(),
                expected: expected_biome_count,
            });
        }
    }
    Ok(())
}

// ─── tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::field::MaskField2D;
    use crate::preset::IslandAge;
    use crate::preset::IslandArchetypePreset;
    use crate::seed::Seed;
    use crate::world::{
        BakedSnapshot, CoastMask, HexAttributeField, HexAttributes, Resolution, WorldState,
    };

    fn test_preset() -> IslandArchetypePreset {
        IslandArchetypePreset {
            name: "validation_test".into(),
            island_radius: 0.5,
            max_relief: 0.5,
            volcanic_center_count: 1,
            island_age: IslandAge::Young,
            prevailing_wind_dir: 0.0,
            marine_moisture_strength: 0.5,
            sea_level: 0.3,
            erosion: Default::default(),
            climate: Default::default(),
        }
    }

    /// Build a minimal CoastMask from raw Vec<u8> data.
    fn make_coast_mask(
        w: u32,
        h: u32,
        is_land: Vec<u8>,
        is_sea: Vec<u8>,
        is_coast: Vec<u8>,
    ) -> CoastMask {
        let land_cell_count = is_land.iter().map(|&v| v as u32).sum();
        let mut land = MaskField2D::new(w, h);
        land.data = is_land;
        let mut sea = MaskField2D::new(w, h);
        sea.data = is_sea;
        let mut coast = MaskField2D::new(w, h);
        coast.data = is_coast;
        CoastMask {
            is_land: land,
            is_sea: sea,
            is_coast: coast,
            land_cell_count,
            river_mouth_mask: None,
        }
    }

    fn minimal_world_for_1b(w: u32, h: u32) -> WorldState {
        let mut world = WorldState::new(Seed(0), test_preset(), Resolution::new(w, h));
        world.baked = BakedSnapshot::default();
        world.derived.coast_mask = Some(make_coast_mask(
            w,
            h,
            vec![1u8; (w * h) as usize],
            vec![0u8; (w * h) as usize],
            vec![0u8; (w * h) as usize],
        ));
        world
    }

    #[test]
    fn hex_attrs_present_happy_path() {
        let mut world = minimal_world_for_1b(4, 4);
        let n_hex = 16;
        let attrs: Vec<HexAttributes> = (0..n_hex)
            .map(|_| HexAttributes {
                elevation: 0.0,
                slope: 0.0,
                rainfall: 0.0,
                temperature: 0.0,
                moisture: 0.0,
                biome_weights: vec![0.0; crate::world::BiomeType::COUNT],
                dominant_biome: crate::world::BiomeType::CoastalScrub,
                has_river: false,
            })
            .collect();
        world.derived.hex_attrs = Some(HexAttributeField {
            attrs,
            cols: 4,
            rows: 4,
        });
        assert!(hex_attrs_present(&world).is_ok());
    }

    #[test]
    fn hex_attrs_present_detects_biome_row_length_mismatch() {
        let mut world = minimal_world_for_1b(4, 4);
        let attrs = (0..16)
            .map(|i| HexAttributes {
                elevation: 0.0,
                slope: 0.0,
                rainfall: 0.0,
                temperature: 0.0,
                moisture: 0.0,
                biome_weights: if i == 5 {
                    vec![0.0; 3] // wrong length on one hex
                } else {
                    vec![0.0; crate::world::BiomeType::COUNT]
                },
                dominant_biome: crate::world::BiomeType::CoastalScrub,
                has_river: false,
            })
            .collect();
        world.derived.hex_attrs = Some(HexAttributeField {
            attrs,
            cols: 4,
            rows: 4,
        });
        let err = hex_attrs_present(&world).unwrap_err();
        assert!(matches!(
            err,
            ValidationError::HexBiomeWeightsLengthMismatch { col: 1, row: 1, .. }
        ));
    }
}
