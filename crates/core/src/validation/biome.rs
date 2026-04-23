//! Biome invariants — partition-of-unity normalization for per-cell biome
//! weight vectors.
//!
//! `biome_weights_normalized` is the only member of this family in v1.
//! Future sprint biome-suitability rework (Sprint 3.5.D) will add further
//! invariants here.

use crate::world::WorldState;

use super::ValidationError;

/// Per-land-cell biome weight sum approximately equals `1.0`.
pub fn biome_weights_normalized(world: &WorldState) -> Result<(), ValidationError> {
    let bw = world
        .baked
        .biome_weights
        .as_ref()
        .ok_or(ValidationError::MissingPrecondition {
            field: "baked.biome_weights",
        })?;
    let coast = world
        .derived
        .coast_mask
        .as_ref()
        .ok_or(ValidationError::MissingPrecondition {
            field: "derived.coast_mask",
        })?;

    const TOL: f32 = 1e-4;
    for y in 0..bw.height {
        for x in 0..bw.width {
            if coast.is_land.get(x, y) != 1 {
                continue;
            }
            let idx = bw.index(x, y);
            let sum: f32 = bw.weights.iter().map(|row| row[idx]).sum();
            if (sum - 1.0).abs() > TOL {
                return Err(ValidationError::BiomeWeightsNotNormalized {
                    x,
                    y,
                    sum,
                    tol: TOL,
                });
            }
        }
    }
    Ok(())
}

// ─── tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::field::MaskField2D;
    use crate::seed::Seed;
    use crate::test_support::test_preset;
    use crate::world::{BakedSnapshot, BiomeWeights, CoastMask, Resolution, WorldState};

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
    fn biome_weights_normalized_happy_path() {
        let mut world = minimal_world_for_1b(2, 2);
        let mut bw = BiomeWeights::new(2, 2);
        let idx = crate::world::BiomeType::LowlandForest as usize;
        for row in bw.weights.iter_mut() {
            row.fill(0.0);
        }
        for cell in 0..4 {
            bw.weights[idx][cell] = 1.0;
        }
        world.baked.biome_weights = Some(bw);
        assert!(biome_weights_normalized(&world).is_ok());
    }

    #[test]
    fn biome_weights_normalized_detects_drift() {
        let mut world = minimal_world_for_1b(2, 2);
        let mut bw = BiomeWeights::new(2, 2);
        // Leave everything at zero → sum = 0, fails tolerance.
        world.baked.biome_weights = Some(bw.clone());
        let err = biome_weights_normalized(&world).unwrap_err();
        assert!(matches!(
            err,
            ValidationError::BiomeWeightsNotNormalized { .. }
        ));

        // Fix cell (0, 0) to sum to 1 but leave (1, 0) drifting by 0.01.
        let idx = crate::world::BiomeType::LowlandForest as usize;
        bw.weights[idx][0] = 1.0;
        bw.weights[idx][1] = 0.5; // still wrong
        world.baked.biome_weights = Some(bw);
        let err = biome_weights_normalized(&world).unwrap_err();
        assert!(matches!(
            err,
            ValidationError::BiomeWeightsNotNormalized { x: 1, y: 0, .. }
        ));
    }
}
