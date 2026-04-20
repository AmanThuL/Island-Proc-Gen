//! Coast classification — Task 1A.2.
//!
//! Reads `world.authoritative.height` (z_raw, pre-pit-fill) and writes
//! `world.derived.coast_mask`, `world.derived.shoreline_normal`, and
//! `world.authoritative.sediment` (Task 3.1 initial condition).
//!
//! Coast semantics must lock onto pre-pit-fill truth: PitFillStage may raise
//! interior cells above sea_level, which would fabricate coastline where none
//! exists in the authoritative heightfield.  By reading z_raw we ensure coast
//! classification is independent of the routing correction.
//!
//! ## Sediment initialization (Task 3.1)
//!
//! After coast classification, `authoritative.sediment` is initialized to the
//! locked initial condition `hs_init(p) = 0.1 * is_land(p)` (sea cells = 0.0,
//! land cells = 0.1). Allocation is skipped when an existing field already has
//! the correct resolution; in that case only the values are overwritten in
//! place.

use island_core::field::{MaskField2D, ScalarField2D, VectorField2D};
use island_core::neighborhood::COAST_DETECT_NEIGHBORHOOD;
use island_core::pipeline::SimulationStage;
use island_core::world::{CoastMask, WorldState};

use super::neighbour_offsets;

/// Sprint 1A Task 1A.2: land / sea / coast classification on z_raw.
pub struct CoastMaskStage;

impl SimulationStage for CoastMaskStage {
    fn name(&self) -> &'static str {
        "coast_mask"
    }

    fn run(&self, world: &mut WorldState) -> anyhow::Result<()> {
        let height = world.authoritative.height.as_ref().ok_or_else(|| {
            anyhow::anyhow!(
                "CoastMaskStage: authoritative.height is None (TopographyStage must run first)"
            )
        })?;

        let w = height.width;
        let h = height.height;
        let sea_level = world.preset.sea_level;
        let n = (w as usize) * (h as usize);

        // ── is_land / is_sea ──────────────────────────────────────────────────
        // Strictly greater: cells exactly at sea_level are classified as sea.
        let mut is_land = MaskField2D::new(w, h);
        let mut is_sea = MaskField2D::new(w, h);
        let mut land_cell_count: u32 = 0;
        for i in 0..n {
            if height.data[i] > sea_level {
                is_land.data[i] = 1;
                land_cell_count += 1;
            } else {
                is_sea.data[i] = 1;
            }
        }

        // ── is_coast ─────────────────────────────────────────────────────────
        // Out-of-bounds does NOT count as sea (§ spec domain boundary rule).
        let coast_offsets = neighbour_offsets(COAST_DETECT_NEIGHBORHOOD);

        let mut is_coast = MaskField2D::new(w, h);
        for iy in 0..h {
            for ix in 0..w {
                if is_land.get(ix, iy) == 0 {
                    continue;
                }
                let mut has_sea_neighbour = false;
                for &(dx, dy) in coast_offsets {
                    let nx = ix as i32 + dx;
                    let ny = iy as i32 + dy;
                    // out-of-bounds → not sea → no coast contribution
                    if nx >= 0
                        && nx < w as i32
                        && ny >= 0
                        && ny < h as i32
                        && is_sea.get(nx as u32, ny as u32) == 1
                    {
                        has_sea_neighbour = true;
                        break;
                    }
                }
                if has_sea_neighbour {
                    is_coast.set(ix, iy, 1);
                }
            }
        }

        // ── shoreline_normal ─────────────────────────────────────────────────
        // For coast cells: finite-difference gradient of z_raw, then flip sign
        // so the vector points from land toward sea (negative gradient direction).
        // Single-sided diff at domain boundaries.
        // For non-coast cells: [0.0, 0.0].
        let mut shoreline_normal = VectorField2D::new(w, h);
        for iy in 0..h {
            for ix in 0..w {
                if is_coast.get(ix, iy) == 0 {
                    continue; // default [0.0, 0.0] is already in place
                }

                // x-gradient (central diff where possible, one-sided at edges)
                let gx = if ix == 0 {
                    height.get(1, iy) - height.get(0, iy)
                } else if ix == w - 1 {
                    height.get(w - 1, iy) - height.get(w - 2, iy)
                } else {
                    (height.get(ix + 1, iy) - height.get(ix - 1, iy)) * 0.5
                };

                // y-gradient (central diff where possible, one-sided at edges)
                let gy = if iy == 0 {
                    height.get(ix, 1) - height.get(ix, 0)
                } else if iy == h - 1 {
                    height.get(ix, h - 1) - height.get(ix, h - 2)
                } else {
                    (height.get(ix, iy + 1) - height.get(ix, iy - 1)) * 0.5
                };

                let norm = (gx * gx + gy * gy).sqrt();
                if norm > 0.0 {
                    // flip sign: gradient points uphill (land), we want downhill (sea)
                    shoreline_normal.set(ix, iy, [-gx / norm, -gy / norm]);
                }
                // if norm == 0.0: leave [0.0, 0.0] (flat neighbourhood)
            }
        }

        world.derived.coast_mask = Some(CoastMask {
            is_land,
            is_sea,
            is_coast,
            land_cell_count,
            river_mouth_mask: None, // backfilled by RiverExtractionStage (Task 1A.8)
        });
        world.derived.shoreline_normal = Some(shoreline_normal);

        // ── Sediment initialization (Task 3.1) ───────────────────────────────
        // hs_init(p) = 0.1 * is_land(p): land cells start with a uniform 0.1
        // weathering layer; sea cells are 0.0.
        //
        // Re-allocation rule: reuse the existing Vec if the field is already
        // the correct resolution; otherwise allocate fresh. This avoids a heap
        // allocation on every run_from(Coastal) while still handling resolution
        // changes correctly.
        //
        // The borrow on `derived.coast_mask` above already moved the CoastMask
        // into place, so we re-read is_land from the new derived field.
        let is_land_ref = &world.derived.coast_mask.as_ref().unwrap().is_land;
        let needs_alloc = match &world.authoritative.sediment {
            Some(s) => s.width != w || s.height != h,
            None => true,
        };
        if needs_alloc {
            world.authoritative.sediment = Some(ScalarField2D::<f32>::new(w, h));
        }
        let sediment = world.authoritative.sediment.as_mut().unwrap();
        for i in 0..n {
            sediment.data[i] = if is_land_ref.data[i] == 1 { 0.1 } else { 0.0 };
        }

        Ok(())
    }
}

// ─── tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use island_core::field::ScalarField2D;
    use island_core::pipeline::SimulationStage;
    use island_core::preset::{IslandAge, IslandArchetypePreset};
    use island_core::seed::Seed;
    use island_core::world::{Resolution, WorldState};

    use super::CoastMaskStage;

    fn base_preset(sea_level: f32) -> IslandArchetypePreset {
        IslandArchetypePreset {
            name: "coast_test".into(),
            island_radius: 0.45,
            max_relief: 0.8,
            volcanic_center_count: 1,
            island_age: IslandAge::Young,
            prevailing_wind_dir: 0.0,
            marine_moisture_strength: 0.5,
            sea_level,
            erosion: Default::default(),
        }
    }

    fn world_with_height(height: ScalarField2D<f32>, sea_level: f32) -> WorldState {
        let w = height.width;
        let h = height.height;
        let mut world = WorldState::new(Seed(0), base_preset(sea_level), Resolution::new(w, h));
        world.authoritative.height = Some(height);
        world
    }

    /// 4×4 field: centre 2×2 = 0.8 (land), border ring = 0.2 (sea); sea_level = 0.5.
    fn make_4x4_field() -> ScalarField2D<f32> {
        let mut f = ScalarField2D::<f32>::new(4, 4);
        for y in 0..4_u32 {
            for x in 0..4_u32 {
                let is_centre = (1..=2).contains(&x) && (1..=2).contains(&y);
                f.set(x, y, if is_centre { 0.8 } else { 0.2 });
            }
        }
        f
    }

    // 1. Basic land/sea split: 4 centre cells = land, 12 border cells = sea.
    //    All 4 centre cells are coast (each has at least one Von4 sea neighbour).
    #[test]
    fn land_sea_split_on_synthetic_heightfield() {
        let mut world = world_with_height(make_4x4_field(), 0.5);
        CoastMaskStage
            .run(&mut world)
            .expect("CoastMaskStage failed");

        let cm = world.derived.coast_mask.as_ref().unwrap();

        // Centre 2×2 = land
        for y in 1..=2_u32 {
            for x in 1..=2_u32 {
                assert_eq!(cm.is_land.get(x, y), 1, "({x},{y}) should be land");
                assert_eq!(cm.is_sea.get(x, y), 0, "({x},{y}) should not be sea");
            }
        }

        // Border ring = sea
        for y in 0..4_u32 {
            for x in 0..4_u32 {
                if !((1..=2).contains(&x) && (1..=2).contains(&y)) {
                    assert_eq!(cm.is_sea.get(x, y), 1, "({x},{y}) should be sea");
                    assert_eq!(cm.is_land.get(x, y), 0, "({x},{y}) should not be land");
                }
            }
        }

        // All 4 centre cells are coast
        for y in 1..=2_u32 {
            for x in 1..=2_u32 {
                assert_eq!(cm.is_coast.get(x, y), 1, "({x},{y}) should be coast");
            }
        }
    }

    // 2. land_cell_count equals popcount of is_land.
    #[test]
    fn land_cell_count_matches_popcount() {
        let mut world = world_with_height(make_4x4_field(), 0.5);
        CoastMaskStage
            .run(&mut world)
            .expect("CoastMaskStage failed");

        let cm = world.derived.coast_mask.as_ref().unwrap();
        assert_eq!(cm.land_cell_count, 4, "expected 4 land cells");

        let popcount = cm.is_land.data.iter().filter(|&&v| v == 1).count() as u32;
        assert_eq!(
            cm.land_cell_count, popcount,
            "land_cell_count must equal popcount"
        );
    }

    // 3. Coast ring is one cell thick: interior cell (2,2) is NOT coast in a 5×5
    //    field where the inner 3×3 is land.
    #[test]
    fn coast_ring_is_one_cell_thick() {
        // 5×5 field: cells (1..4, 1..4) are land, everything else sea.
        let mut f = ScalarField2D::<f32>::new(5, 5);
        for y in 0..5_u32 {
            for x in 0..5_u32 {
                let is_inner = (1..=3).contains(&x) && (1..=3).contains(&y);
                f.set(x, y, if is_inner { 0.8 } else { 0.2 });
            }
        }

        let mut world = world_with_height(f, 0.5);
        CoastMaskStage
            .run(&mut world)
            .expect("CoastMaskStage failed");

        let cm = world.derived.coast_mask.as_ref().unwrap();

        // The ring around the inner block should be coast
        for y in 1..=3_u32 {
            for x in 1..=3_u32 {
                let is_edge = x == 1 || x == 3 || y == 1 || y == 3;
                if is_edge {
                    assert_eq!(cm.is_coast.get(x, y), 1, "({x},{y}) edge should be coast");
                }
            }
        }

        // The very centre (2,2) has only land Von4 neighbours — NOT coast
        assert_eq!(cm.is_coast.get(2, 2), 0, "(2,2) interior must not be coast");
    }

    // 4. Shoreline normal points from land toward sea.
    //    Linear ramp z = 0.1 + 0.1*x on an 8×8 field; sea_level = 0.4.
    //    Column x=3 has z=0.4 (sea), x=4 has z=0.5 (land) — first land column.
    //    Coast cells in column x=4 should have normal.x < 0 (pointing toward sea)
    //    and normal.y ≈ 0 (no y-gradient), and magnitude ≈ 1.
    #[test]
    fn shoreline_normal_points_land_to_sea() {
        let mut f = ScalarField2D::<f32>::new(8, 8);
        for y in 0..8_u32 {
            for x in 0..8_u32 {
                f.set(x, y, 0.1 + 0.1 * x as f32);
            }
        }

        let mut world = world_with_height(f, 0.4);
        CoastMaskStage
            .run(&mut world)
            .expect("CoastMaskStage failed");

        let cm = world.derived.coast_mask.as_ref().unwrap();
        let sn = world.derived.shoreline_normal.as_ref().unwrap();

        // x=4, y=4 should be land and coast (x=3 is z=0.4=sea, exactly at sea_level)
        assert_eq!(cm.is_land.get(4, 4), 1, "x=4 should be land");
        assert_eq!(cm.is_coast.get(4, 4), 1, "x=4 should be coast");

        let normal = sn.get(4, 4);
        assert!(
            normal[0] < 0.0,
            "normal.x should be negative (pointing toward sea, i.e. lower x); got {}",
            normal[0]
        );
        assert!(
            normal[1].abs() < 1e-4,
            "normal.y should be ~0 (no y-gradient); got {}",
            normal[1]
        );

        let mag = (normal[0] * normal[0] + normal[1] * normal[1]).sqrt();
        assert!(
            (mag - 1.0).abs() < 1e-5,
            "normal should be unit length; magnitude={}",
            mag
        );
    }

    // 5. Non-coast land cell has zero normal.
    #[test]
    fn non_coast_normal_is_zero() {
        // Same ramp as #4; pick a deep-interior land cell far from the coast.
        let mut f = ScalarField2D::<f32>::new(8, 8);
        for y in 0..8_u32 {
            for x in 0..8_u32 {
                f.set(x, y, 0.1 + 0.1 * x as f32);
            }
        }

        let mut world = world_with_height(f, 0.4);
        CoastMaskStage
            .run(&mut world)
            .expect("CoastMaskStage failed");

        let cm = world.derived.coast_mask.as_ref().unwrap();
        let sn = world.derived.shoreline_normal.as_ref().unwrap();

        // x=7 is z=0.8, deep inland — not coast
        assert_eq!(cm.is_land.get(7, 4), 1);
        assert_eq!(cm.is_coast.get(7, 4), 0, "x=7 should not be coast");
        assert_eq!(sn.get(7, 4), [0.0, 0.0], "non-coast normal must be [0,0]");
    }

    // 6. Coast reads z_raw, not z_filled.
    //    A basin cell below sea_level in z_raw must stay sea even if we
    //    manually inject a pit-filled version into derived.z_filled.
    #[test]
    fn reads_z_raw_not_z_filled() {
        // 4×4 field: interior cell (2,2)=0.2, everything else=0.6; sea_level=0.5.
        let mut f = ScalarField2D::<f32>::new(4, 4);
        for y in 0..4_u32 {
            for x in 0..4_u32 {
                f.set(x, y, if x == 2 && y == 2 { 0.2 } else { 0.6 });
            }
        }

        let mut world = world_with_height(f.clone(), 0.5);
        CoastMaskStage.run(&mut world).expect("first run failed");

        let first_is_land_bytes = world
            .derived
            .coast_mask
            .as_ref()
            .unwrap()
            .is_land
            .to_bytes();
        let first_is_coast_bytes = world
            .derived
            .coast_mask
            .as_ref()
            .unwrap()
            .is_coast
            .to_bytes();

        // Simulate a pit-fill: raise the basin cell to 0.6 in derived.z_filled.
        let mut z_filled = f.clone();
        z_filled.set(2, 2, 0.6);
        world.derived.z_filled = Some(z_filled);

        // Re-run CoastMaskStage — it must still read authoritative.height (z_raw).
        CoastMaskStage.run(&mut world).expect("second run failed");

        let second_is_land_bytes = world
            .derived
            .coast_mask
            .as_ref()
            .unwrap()
            .is_land
            .to_bytes();
        let second_is_coast_bytes = world
            .derived
            .coast_mask
            .as_ref()
            .unwrap()
            .is_coast
            .to_bytes();

        assert_eq!(
            first_is_land_bytes, second_is_land_bytes,
            "is_land must be byte-equal regardless of z_filled"
        );
        assert_eq!(
            first_is_coast_bytes, second_is_coast_bytes,
            "is_coast must be byte-equal regardless of z_filled"
        );

        // Also verify the centre cell is classified as sea (it's in z_raw)
        let cm = world.derived.coast_mask.as_ref().unwrap();
        assert_eq!(cm.is_sea.get(2, 2), 1, "basin cell must be sea in z_raw");

        // Its 4 Von4 land neighbours should be coast
        for (nx, ny) in [(2_u32, 1_u32), (3, 2), (2, 3), (1, 2)] {
            assert_eq!(
                cm.is_coast.get(nx, ny),
                1,
                "({nx},{ny}) adjacent to basin should be coast"
            );
        }
    }

    // 7. Stage returns Err when authoritative.height is None.
    #[test]
    fn stage_errors_when_height_missing() {
        let mut world = WorldState::new(Seed(0), base_preset(0.5), Resolution::new(16, 16));
        // height is None by default
        let result = CoastMaskStage.run(&mut world);
        assert!(result.is_err(), "expected Err when height is None");
    }

    // 8. (Task 3.1) sediment is initialized after CoastMaskStage runs:
    //    land cells = 0.1, sea cells = 0.0.
    #[test]
    fn sediment_initialized_on_land_cells() {
        // 4×4 field: centre 2×2 = land (0.8), border ring = sea (0.2); sea_level = 0.5.
        let mut world = world_with_height(make_4x4_field(), 0.5);
        CoastMaskStage
            .run(&mut world)
            .expect("CoastMaskStage failed");

        let sediment = world
            .authoritative
            .sediment
            .as_ref()
            .expect("authoritative.sediment must be Some after CoastMaskStage");

        for y in 0..4_u32 {
            for x in 0..4_u32 {
                let is_centre = (1..=2).contains(&x) && (1..=2).contains(&y);
                let idx = (y * 4 + x) as usize;
                let expected = if is_centre { 0.1 } else { 0.0 };
                assert!(
                    (sediment.data[idx] - expected).abs() < f32::EPSILON,
                    "({x},{y}) sediment must be {expected}, got {}",
                    sediment.data[idx]
                );
            }
        }
    }

    // 9. (Task 3.1) sediment is reallocated when resolution changes.
    #[test]
    fn sediment_reallocated_on_resolution_change() {
        let mut world = world_with_height(make_4x4_field(), 0.5);
        CoastMaskStage.run(&mut world).expect("first run failed");

        let first_len = world.authoritative.sediment.as_ref().unwrap().data.len();
        assert_eq!(
            first_len, 16,
            "4×4 field should produce 16-element sediment"
        );

        // Switch to an 8×8 resolution with a matching height field.
        let mut f8 = ScalarField2D::<f32>::new(8, 8);
        for y in 0..8_u32 {
            for x in 0..8_u32 {
                let is_centre = (2..=5).contains(&x) && (2..=5).contains(&y);
                f8.set(x, y, if is_centre { 0.8 } else { 0.2 });
            }
        }
        world.resolution = island_core::world::Resolution::new(8, 8);
        world.authoritative.height = Some(f8);

        CoastMaskStage.run(&mut world).expect("second run failed");

        let sediment = world.authoritative.sediment.as_ref().unwrap();
        assert_eq!(
            sediment.data.len(),
            64,
            "sediment must be reallocated to 8×8 = 64 elements after resolution change"
        );
        assert_eq!(sediment.width, 8);
        assert_eq!(sediment.height, 8);
    }

    // 10. (Task 3.1) sediment buffer is reused (not reallocated) across reruns
    //     at the same resolution.
    #[test]
    fn sediment_reused_across_reruns_when_resolution_unchanged() {
        let mut world = world_with_height(make_4x4_field(), 0.5);
        CoastMaskStage.run(&mut world).expect("first run failed");

        // Capture the capacity and pointer of the backing Vec before the second run.
        let sediment_before = world.authoritative.sediment.as_ref().unwrap();
        let cap_before = sediment_before.data.capacity();
        let ptr_before = sediment_before.data.as_ptr();

        CoastMaskStage.run(&mut world).expect("second run failed");

        let sediment = world.authoritative.sediment.as_ref().unwrap();
        assert_eq!(
            sediment.data.capacity(),
            cap_before,
            "sediment Vec capacity must be unchanged (no reallocation)"
        );
        assert_eq!(
            sediment.data.as_ptr(),
            ptr_before,
            "sediment Vec pointer must be unchanged (same backing store reused)"
        );
        // Values must still be correct after the reuse.
        for y in 0..4_u32 {
            for x in 0..4_u32 {
                let is_centre = (1..=2).contains(&x) && (1..=2).contains(&y);
                let idx = (y * 4 + x) as usize;
                let expected = if is_centre { 0.1 } else { 0.0 };
                assert!(
                    (sediment.data[idx] - expected).abs() < f32::EPSILON,
                    "({x},{y}) sediment must be {expected} after reuse, got {}",
                    sediment.data[idx]
                );
            }
        }
    }
}
