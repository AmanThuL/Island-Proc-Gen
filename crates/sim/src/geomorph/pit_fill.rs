//! Pit-fill stage — Task 1A.3.
//!
//! Reads `world.authoritative.height` (z_raw) and `world.derived.coast_mask`,
//! runs Planchon-Darboux two-sweep fill, and writes `world.derived.z_filled`.
//! `authoritative.height` is left unchanged so re-running with different
//! parameters does not invalidate saved files.

use island_core::field::ScalarField2D;
use island_core::neighborhood::Neighborhood;
use island_core::pipeline::SimulationStage;
use island_core::world::WorldState;

use super::neighbour_offsets;

// ─── constants ────────────────────────────────────────────────────────────────

/// Planchon-Darboux tilt: micro-gradient so D8 routing never stalls on filled flats (§D7).
const PD_EPSILON: f32 = 1e-5;

/// Stop iterating once max |w_new - w_old| falls below this.
const CONVERGENCE_THRESHOLD: f32 = 1e-7;

const fn max_iters(w: u32, h: u32) -> usize {
    4 * (w as usize + h as usize)
}

// ─── fill logic ───────────────────────────────────────────────────────────────

/// Run Planchon-Darboux two-sweep fill.
///
/// Returns `z_filled` as a flat `Vec<f32>` (row-major, same layout as
/// `ScalarField2D::data`).
fn run_planchon_darboux(w: u32, h: u32, z_raw: &[f32], is_sea: &[u8]) -> anyhow::Result<Vec<f32>> {
    let w = w as usize;
    let h = h as usize;
    let n = w * h;
    let offsets = neighbour_offsets(Neighborhood::Moore8);
    let limit = max_iters(w as u32, h as u32);

    // Outlets (sea cells or domain boundary) are fixed at z_raw; all other cells start at +∞.
    let is_outlet = |x: usize, y: usize, i: usize| -> bool {
        is_sea[i] == 1 || x == 0 || x == w - 1 || y == 0 || y == h - 1
    };

    let mut cur: Vec<f32> = (0..n)
        .map(|i| {
            if is_outlet(i % w, i / w, i) {
                z_raw[i]
            } else {
                f32::INFINITY
            }
        })
        .collect();

    let mut next: Vec<f32> = cur.clone();

    for _iter in 0..limit {
        let mut max_delta: f32 = 0.0;

        // Forward sweep: left-to-right, top-to-bottom.
        for y in 0..h {
            for x in 0..w {
                let i = y * w + x;
                if next[i] == z_raw[i] && is_outlet(x, y, i) {
                    continue;
                }
                if next[i] <= z_raw[i] {
                    continue;
                }
                let mut min_nbr = f32::INFINITY;
                for &(dx, dy) in offsets {
                    let nx = x as i32 + dx;
                    let ny = y as i32 + dy;
                    if nx >= 0 && nx < w as i32 && ny >= 0 && ny < h as i32 {
                        let ni = ny as usize * w + nx as usize;
                        let (xsign, ysign) = (dx.signum(), dy.signum());
                        let is_forward = (xsign > 0) || (xsign == 0 && ysign > 0);
                        let nbr_w = if is_forward { next[ni] } else { cur[ni] };
                        min_nbr = min_nbr.min(nbr_w + PD_EPSILON);
                    }
                }
                let new_w = z_raw[i].max(min_nbr);
                if new_w < next[i] {
                    let delta = next[i] - new_w;
                    if delta > max_delta {
                        max_delta = delta;
                    }
                    next[i] = new_w;
                }
            }
        }

        // Backward sweep: right-to-left, bottom-to-top.
        cur.copy_from_slice(&next);

        for y in (0..h).rev() {
            for x in (0..w).rev() {
                let i = y * w + x;
                if next[i] == z_raw[i] && is_outlet(x, y, i) {
                    continue;
                }
                if next[i] <= z_raw[i] {
                    continue;
                }
                let mut min_nbr = f32::INFINITY;
                for &(dx, dy) in offsets {
                    let nx = x as i32 + dx;
                    let ny = y as i32 + dy;
                    if nx >= 0 && nx < w as i32 && ny >= 0 && ny < h as i32 {
                        let ni = ny as usize * w + nx as usize;
                        let (xsign, ysign) = (dx.signum(), dy.signum());
                        let is_backward = (xsign < 0) || (xsign == 0 && ysign < 0);
                        let nbr_w = if is_backward { next[ni] } else { cur[ni] };
                        min_nbr = min_nbr.min(nbr_w + PD_EPSILON);
                    }
                }
                let new_w = z_raw[i].max(min_nbr);
                if new_w < next[i] {
                    let delta = next[i] - new_w;
                    if delta > max_delta {
                        max_delta = delta;
                    }
                    next[i] = new_w;
                }
            }
        }

        cur.copy_from_slice(&next);

        tracing::trace!("pit_fill iter={} max_delta={:.2e}", _iter + 1, max_delta);

        if max_delta < CONVERGENCE_THRESHOLD {
            tracing::debug!("pit_fill converged after {} sweep-pairs", _iter + 1);
            return Ok(next);
        }
    }

    Err(anyhow::anyhow!(
        "PitFillStage: did not converge in {} iterations",
        limit
    ))
}

// ─── PitFillStage ─────────────────────────────────────────────────────────────

/// Sprint 1A Task 1A.3: Planchon-Darboux pit fill.
///
/// Writes `world.derived.z_filled`; leaves `authoritative.height` untouched.
pub struct PitFillStage;

impl SimulationStage for PitFillStage {
    fn name(&self) -> &'static str {
        "pit_fill"
    }

    fn run(&self, world: &mut WorldState) -> anyhow::Result<()> {
        let height = world.authoritative.height.as_ref().ok_or_else(|| {
            anyhow::anyhow!(
                "PitFillStage: authoritative.height is None (TopographyStage must run first)"
            )
        })?;

        let coast_mask = world.derived.coast_mask.as_ref().ok_or_else(|| {
            anyhow::anyhow!("PitFillStage: coast_mask is None (CoastMaskStage must run first)")
        })?;

        let w = height.width;
        let h = height.height;

        let filled_data = run_planchon_darboux(w, h, &height.data, &coast_mask.is_sea.data)?;

        let mut z_filled = ScalarField2D::<f32>::new(w, h);
        z_filled.data = filled_data;
        world.derived.z_filled = Some(z_filled);

        Ok(())
    }
}

// ─── tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use island_core::field::ScalarField2D;
    use island_core::neighborhood::Neighborhood;
    use island_core::pipeline::SimulationStage;
    use island_core::preset::{IslandAge, IslandArchetypePreset};
    use island_core::seed::Seed;
    use island_core::world::{Resolution, WorldState};

    use super::PitFillStage;
    use crate::geomorph::{CoastMaskStage, neighbour_offsets};

    // ── test helpers ─────────────────────────────────────────────────────────

    fn pit_fill_preset(sea_level: f32) -> IslandArchetypePreset {
        IslandArchetypePreset {
            name: "pit_fill_test".into(),
            island_radius: 0.45,
            max_relief: 0.8,
            volcanic_center_count: 1,
            island_age: IslandAge::Young,
            prevailing_wind_dir: 0.0,
            marine_moisture_strength: 0.5,
            sea_level,
        }
    }

    /// Stand up a `WorldState` with a hand-crafted height field, run
    /// `CoastMaskStage` (so `coast_mask` is populated), then return it ready
    /// for `PitFillStage`.
    fn world_with_synthetic_height(height: ScalarField2D<f32>, sea_level: f32) -> WorldState {
        let w = height.width;
        let h = height.height;
        let mut world = WorldState::new(Seed(0), pit_fill_preset(sea_level), Resolution::new(w, h));
        world.authoritative.height = Some(height);
        CoastMaskStage
            .run(&mut world)
            .expect("CoastMaskStage must succeed in test setup");
        world
    }

    /// 8×8 field: outer ring = 0.0 (sea), interior mostly 0.4 (land), except
    /// cell (4,4) = 0.1 (local basin, still land above sea_level=0.05).
    fn make_basin_8x8() -> ScalarField2D<f32> {
        let mut f = ScalarField2D::<f32>::new(8, 8);
        for y in 0..8_u32 {
            for x in 0..8_u32 {
                let on_ring = x == 0 || x == 7 || y == 0 || y == 7;
                let val = if on_ring {
                    0.0
                } else if x == 4 && y == 4 {
                    0.1 // local depression
                } else {
                    0.4
                };
                f.set(x, y, val);
            }
        }
        f
    }

    // 1. Pit fill raises the basin cell to at least the surrounding plateau.
    //    The shortest Moore8 path from (4,4) to the outlet ring is 3 steps
    //    (e.g. (4,4)→(3,3)→(2,2)→(1,1)→(0,0)), so the PD tilt can add at
    //    most ~4 × 1e-5 above the plateau.  Pragmatic check: z_filled > 0.3999.
    #[test]
    fn fills_a_toy_basin() {
        let mut world = world_with_synthetic_height(make_basin_8x8(), 0.05);
        PitFillStage.run(&mut world).expect("PitFillStage failed");

        let z_filled = world.derived.z_filled.as_ref().unwrap();
        let filled_val = z_filled.get(4, 4);
        assert!(
            filled_val > 0.3999,
            "basin cell (4,4) should be filled to ~0.4, got {filled_val}"
        );
    }

    // 2. z_filled >= z_raw at every cell (pit fill is monotonically non-decreasing).
    #[test]
    fn z_filled_geq_z_raw_pointwise() {
        let height = make_basin_8x8();
        let z_raw_data = height.data.clone();
        let mut world = world_with_synthetic_height(height, 0.05);
        PitFillStage.run(&mut world).expect("PitFillStage failed");

        let z_filled = world.derived.z_filled.as_ref().unwrap();
        for (i, (&zf, &zr)) in z_filled.data.iter().zip(z_raw_data.iter()).enumerate() {
            assert!(
                zf >= zr - 1e-9,
                "cell {i}: z_filled={zf} < z_raw={zr} (pit fill must be non-decreasing)"
            );
        }
    }

    // 3. Open terrain with monotonic descent requires no filling: z_filled == z_raw.
    //    Ramp z[y][x] = 0.9 - 0.1*(x+y) — no local minima.  sea_level = 0.05
    //    so only the very high-value corner is land.
    #[test]
    fn open_terrain_is_unchanged() {
        let mut f = ScalarField2D::<f32>::new(8, 8);
        for y in 0..8_u32 {
            for x in 0..8_u32 {
                // clamp to [0,1] so no negative values
                let v = (0.9_f32 - 0.1 * (x + y) as f32).clamp(0.0, 1.0);
                f.set(x, y, v);
            }
        }
        let z_raw_data = f.data.clone();
        let mut world = world_with_synthetic_height(f, 0.05);
        PitFillStage.run(&mut world).expect("PitFillStage failed");

        let z_filled = world.derived.z_filled.as_ref().unwrap();
        for (i, (&zf, &zr)) in z_filled.data.iter().zip(z_raw_data.iter()).enumerate() {
            assert!(
                (zf - zr).abs() < 1e-6,
                "cell {i}: open terrain should be unchanged; z_filled={zf}, z_raw={zr}"
            );
        }
    }

    // 4. After fill, every land cell has at least one Moore8 neighbour with
    //    strictly lower z_filled (or is itself a sea cell / outlet).
    //    Walk from each land cell; must reach a sea cell within w+h steps.
    #[test]
    fn descent_path_exists_from_every_land_cell() {
        let mut world = world_with_synthetic_height(make_basin_8x8(), 0.05);
        PitFillStage.run(&mut world).expect("PitFillStage failed");

        let z_filled = world.derived.z_filled.as_ref().unwrap();
        let coast_mask = world.derived.coast_mask.as_ref().unwrap();
        let w = z_filled.width as usize;
        let h = z_filled.height as usize;
        let offsets = neighbour_offsets(Neighborhood::Moore8);
        let max_steps = w + h;

        for start_y in 0..h {
            for start_x in 0..w {
                if coast_mask.is_land.get(start_x as u32, start_y as u32) == 0 {
                    continue; // sea cell — skip
                }

                let mut cx = start_x;
                let mut cy = start_y;
                let mut reached_sea = false;
                for _ in 0..max_steps {
                    if coast_mask.is_sea.get(cx as u32, cy as u32) == 1 {
                        reached_sea = true;
                        break;
                    }
                    let cur_z = z_filled.get(cx as u32, cy as u32);
                    let mut moved = false;
                    let mut best_z = cur_z;
                    let mut best_nx = cx;
                    let mut best_ny = cy;
                    for &(dx, dy) in offsets {
                        let nx = cx as i32 + dx;
                        let ny = cy as i32 + dy;
                        if nx >= 0 && nx < w as i32 && ny >= 0 && ny < h as i32 {
                            let nz = z_filled.get(nx as u32, ny as u32);
                            if nz < best_z {
                                best_z = nz;
                                best_nx = nx as usize;
                                best_ny = ny as usize;
                                moved = true;
                            }
                        }
                    }
                    if !moved {
                        // No strictly lower neighbour — if we're at a domain
                        // boundary or a sea cell that's the outlet, that's fine.
                        if cx == 0 || cx == w - 1 || cy == 0 || cy == h - 1 {
                            reached_sea = true;
                        }
                        break;
                    }
                    cx = best_nx;
                    cy = best_ny;
                }

                assert!(
                    reached_sea,
                    "land cell ({start_x},{start_y}) has no downhill path to sea"
                );
            }
        }
    }

    // 5. Two runs on fresh clones produce bit-exact z_filled.
    #[test]
    fn bit_exact_determinism() {
        let height = make_basin_8x8();
        let mut w1 = world_with_synthetic_height(height.clone(), 0.05);
        let mut w2 = world_with_synthetic_height(height, 0.05);

        PitFillStage.run(&mut w1).expect("run 1 failed");
        PitFillStage.run(&mut w2).expect("run 2 failed");

        let d1 = &w1.derived.z_filled.as_ref().unwrap().data;
        let d2 = &w2.derived.z_filled.as_ref().unwrap().data;
        assert_eq!(d1, d2, "z_filled must be bit-exact across two fresh runs");
    }

    // 6. Seed independence: pit fill must not read the seed.
    //    Seed(0) and Seed(99999) must produce identical z_filled.
    #[test]
    fn no_seed_dependent_noise() {
        let height = make_basin_8x8();
        let preset = pit_fill_preset(0.05);
        let w = height.width;
        let h = height.height;

        let mut world0 = WorldState::new(Seed(0), preset.clone(), Resolution::new(w, h));
        world0.authoritative.height = Some(height.clone());
        CoastMaskStage.run(&mut world0).expect("coast mask seed 0");
        PitFillStage.run(&mut world0).expect("pit fill seed 0");

        let mut world1 = WorldState::new(Seed(99999), preset, Resolution::new(w, h));
        world1.authoritative.height = Some(height);
        CoastMaskStage
            .run(&mut world1)
            .expect("coast mask seed 99999");
        PitFillStage.run(&mut world1).expect("pit fill seed 99999");

        let d0 = &world0.derived.z_filled.as_ref().unwrap().data;
        let d1 = &world1.derived.z_filled.as_ref().unwrap().data;
        assert_eq!(d0, d1, "z_filled must be seed-independent");
    }

    // 7. Returns Err when coast_mask is None.
    #[test]
    fn errors_when_coast_mask_missing() {
        let mut world = WorldState::new(Seed(0), pit_fill_preset(0.05), Resolution::new(8, 8));
        world.authoritative.height = Some(make_basin_8x8());
        // coast_mask deliberately left None
        let result = PitFillStage.run(&mut world);
        assert!(result.is_err(), "expected Err when coast_mask is None");
    }

    // 8. Returns Err when authoritative.height is None.
    #[test]
    fn errors_when_height_missing() {
        let w = 8_u32;
        let h = 8_u32;
        let preset = pit_fill_preset(0.05);

        // Build a minimal but valid CoastMask manually.
        use island_core::field::MaskField2D;
        use island_core::world::CoastMask;
        let mut world = WorldState::new(Seed(0), preset, Resolution::new(w, h));
        let n = (w * h) as usize;
        let mut is_sea = MaskField2D::new(w, h);
        let is_land = MaskField2D::new(w, h);
        let is_coast = MaskField2D::new(w, h);
        // mark all as sea for a valid (if degenerate) mask
        for i in 0..n {
            is_sea.data[i] = 1;
        }
        world.derived.coast_mask = Some(CoastMask {
            is_land,
            is_sea,
            is_coast,
            land_cell_count: 0,
            river_mouth_mask: None,
        });
        // height is None
        let result = PitFillStage.run(&mut world);
        assert!(result.is_err(), "expected Err when height is None");
    }
}
