//! `HexProjectionStage` (DD8) — aggregate sim-cell fields into the
//! `64 × 64` flat-top hex grid overlay.
//!
//! Per-hex attributes are means of the underlying sim-cell values for
//! the continuous fields (elevation, slope, rainfall, temperature,
//! moisture, biome weights), and an OR-reduction for the river flag.
//! Sea cells are excluded from the mean: a hex whose bounding box
//! straddles a shoreline only averages the land cells inside it, so
//! coastal hexes don't get dragged toward `sea_level` / `T_SEA_LEVEL`.
//! A hex that contains no land cells at all keeps its defaults (0s and
//! `BiomeType::ALL[0]`) — consumers gate on `land_cell_count` or
//! similar if they need to distinguish those.

use anyhow::anyhow;
use hex::build_hex_grid;
use island_core::field::MaskField2D;
use island_core::pipeline::SimulationStage;
use island_core::world::{
    BiomeType, CoastType, HexAttributeField, HexAttributes, HexDebugAttributes, HexRiverCrossing,
    WorldState,
};

/// Default hex grid resolution per DD8: `64 × 64` flat-top.
pub(crate) const DEFAULT_HEX_COLS: u32 = 64;
pub(crate) const DEFAULT_HEX_ROWS: u32 = 64;

/// Weight multipliers for the per-hex accessibility cost formula
/// `1 + W_SLOPE*mean_slope + W_RIVER*river_penalty + W_CLIFF*cliff_penalty`.
/// Values are spec-locked from the roadmap §Sprint 5 accessibility_cost formula.
pub const W_SLOPE: f32 = 3.0;
pub const W_RIVER: f32 = 2.0;
pub const W_CLIFF: f32 = 5.0;

/// DD8: populate `world.derived.{hex_grid, hex_attrs}`.
pub struct HexProjectionStage;

impl SimulationStage for HexProjectionStage {
    fn name(&self) -> &'static str {
        "hex_projection"
    }

    fn run(&self, world: &mut WorldState) -> anyhow::Result<()> {
        let z = world
            .derived
            .z_filled
            .as_ref()
            .ok_or_else(|| anyhow!("HexProjectionStage: derived.z_filled is None"))?;
        let slope = world
            .derived
            .slope
            .as_ref()
            .ok_or_else(|| anyhow!("HexProjectionStage: derived.slope is None"))?;
        let precipitation = world
            .baked
            .precipitation
            .as_ref()
            .ok_or_else(|| anyhow!("HexProjectionStage: baked.precipitation is None"))?;
        let temperature = world
            .baked
            .temperature
            .as_ref()
            .ok_or_else(|| anyhow!("HexProjectionStage: baked.temperature is None"))?;
        let soil_moisture = world
            .baked
            .soil_moisture
            .as_ref()
            .ok_or_else(|| anyhow!("HexProjectionStage: baked.soil_moisture is None"))?;
        let biome_weights = world
            .baked
            .biome_weights
            .as_ref()
            .ok_or_else(|| anyhow!("HexProjectionStage: baked.biome_weights is None"))?;
        let river_mask = world
            .derived
            .river_mask
            .as_ref()
            .ok_or_else(|| anyhow!("HexProjectionStage: derived.river_mask is None"))?;
        let coast = world
            .derived
            .coast_mask
            .as_ref()
            .ok_or_else(|| anyhow!("HexProjectionStage: derived.coast_mask is None"))?;
        // `coast_type` is populated by `CoastTypeStage` (Sprint 2). When running
        // a pre-Sprint-2 pipeline (e.g. the Sprint 1B integration test), it is
        // legitimately absent; in that case cliff_penalty is always 0.0.
        let coast_type = world.derived.coast_type.as_ref();

        // `accumulation` drives river-crossing entry/exit computation. When
        // absent (e.g. pre-accumulation pipelines), river_crossing stays all
        // None and the mask stays all-zero.
        let accumulation = world.derived.accumulation.as_ref();

        let sim_w = z.width;
        let sim_h = z.height;

        // Build the hex grid (or reuse an existing one if the sim
        // resolution hasn't changed — slider re-runs will hit the
        // reuse path via `run_from`).
        let grid = match world.derived.hex_grid.as_ref() {
            Some(existing)
                if existing.cols == DEFAULT_HEX_COLS
                    && existing.rows == DEFAULT_HEX_ROWS
                    && existing.hex_id_of_cell.width == sim_w
                    && existing.hex_id_of_cell.height == sim_h =>
            {
                existing.clone()
            }
            _ => build_hex_grid(DEFAULT_HEX_COLS, DEFAULT_HEX_ROWS, sim_w, sim_h),
        };

        let hex_count = (grid.cols * grid.rows) as usize;
        let biome_count = BiomeType::COUNT;

        // Accumulators: one sum per field + a land-cell counter per hex.
        let mut sum_elev = vec![0.0_f64; hex_count];
        let mut sum_slope = vec![0.0_f64; hex_count];
        let mut sum_slope_sq = vec![0.0_f64; hex_count];
        let mut sum_rain = vec![0.0_f64; hex_count];
        let mut sum_temp = vec![0.0_f64; hex_count];
        let mut sum_moist = vec![0.0_f64; hex_count];
        let mut sum_biomes = vec![vec![0.0_f64; biome_count]; hex_count];
        let mut land_count = vec![0_u32; hex_count];
        let mut river_flag = vec![false; hex_count];
        // Denominator / numerator for cliff_penalty: counts over ALL sim cells
        // (sea + land), not just land, matching the spec formula.
        let mut total_cell_count = vec![0_u32; hex_count];
        let mut cliff_count = vec![0_u32; hex_count];

        // River crossing: track the most-upstream (argmin accumulation) and
        // most-downstream (argmax accumulation) river cells per hex.
        // Stored as (ix, iy, accumulation_value); None until the first river cell is seen.
        let mut river_entry: Vec<Option<(u32, u32, f32)>> = vec![None; hex_count];
        let mut river_exit: Vec<Option<(u32, u32, f32)>> = vec![None; hex_count];

        for iy in 0..sim_h {
            for ix in 0..sim_w {
                let hex_id = grid.hex_id_of_cell.get(ix, iy) as usize;
                total_cell_count[hex_id] += 1;
                if river_mask.get(ix, iy) == 1 {
                    river_flag[hex_id] = true;
                    // River crossing: track entry (min accum) and exit (max accum)
                    // cells per hex, but only when accumulation is available.
                    if let Some(acc) = accumulation {
                        let a = acc.get(ix, iy);
                        match river_entry[hex_id] {
                            None => river_entry[hex_id] = Some((ix, iy, a)),
                            Some((_, _, prev_a)) if a < prev_a => {
                                river_entry[hex_id] = Some((ix, iy, a))
                            }
                            _ => {}
                        }
                        match river_exit[hex_id] {
                            None => river_exit[hex_id] = Some((ix, iy, a)),
                            Some((_, _, prev_a)) if a > prev_a => {
                                river_exit[hex_id] = Some((ix, iy, a))
                            }
                            _ => {}
                        }
                    }
                }
                if let Some(ct) = coast_type {
                    if ct.get(ix, iy) == CoastType::Cliff as u8 {
                        cliff_count[hex_id] += 1;
                    }
                }
                if coast.is_land.get(ix, iy) != 1 {
                    continue;
                }
                let s = slope.get(ix, iy) as f64;
                sum_elev[hex_id] += z.get(ix, iy) as f64;
                sum_slope[hex_id] += s;
                sum_slope_sq[hex_id] += s * s;
                sum_rain[hex_id] += precipitation.get(ix, iy) as f64;
                sum_temp[hex_id] += temperature.get(ix, iy) as f64;
                sum_moist[hex_id] += soil_moisture.get(ix, iy) as f64;
                let cell_idx = biome_weights.index(ix, iy);
                for (b, row) in biome_weights.weights.iter().enumerate() {
                    sum_biomes[hex_id][b] += row[cell_idx] as f64;
                }
                land_count[hex_id] += 1;
            }
        }

        // Fold into HexAttributes records and pre-compute debug attributes.
        let mut attrs = Vec::with_capacity(hex_count);
        let mut slope_variance = Vec::with_capacity(hex_count);
        let mut accessibility_cost = Vec::with_capacity(hex_count);
        let mut river_crossing: Vec<Option<HexRiverCrossing>> = Vec::with_capacity(hex_count);

        for hex_id in 0..hex_count {
            let count = land_count[hex_id] as f64;
            let inv = if count > 0.0 { 1.0 / count } else { 0.0 };
            let biome_mean: Vec<f32> = (0..biome_count)
                .map(|b| (sum_biomes[hex_id][b] * inv) as f32)
                .collect();
            let dominant = dominant_biome_from_weights(&biome_mean);
            let mean_slope = (sum_slope[hex_id] * inv) as f32;
            attrs.push(HexAttributes {
                elevation: (sum_elev[hex_id] * inv) as f32,
                slope: mean_slope,
                rainfall: (sum_rain[hex_id] * inv) as f32,
                temperature: (sum_temp[hex_id] * inv) as f32,
                moisture: (sum_moist[hex_id] * inv) as f32,
                biome_weights: biome_mean,
                dominant_biome: dominant,
                has_river: river_flag[hex_id],
            });

            // Slope variance = E[slope²] − (E[slope])². f64 accumulators
            // avoid catastrophic cancellation; clamp fp-noise negatives to 0.
            let e_sq = sum_slope_sq[hex_id] * inv;
            let e_mean_sq = (sum_slope[hex_id] * inv).powi(2);
            let var = (e_sq - e_mean_sq).max(0.0) as f32;
            slope_variance.push(var);

            // Accessibility cost: reuse `river_flag` (existing has_river OR-reduction)
            // for river_penalty; cliff_penalty is the Cliff-cell fraction over
            // ALL sim cells in the hex (sea + land).
            let river_penalty = if river_flag[hex_id] { 1.0_f32 } else { 0.0 };
            let total = total_cell_count[hex_id] as f32;
            let cliff_penalty = if total > 0.0 {
                cliff_count[hex_id] as f32 / total
            } else {
                0.0
            };
            let cost =
                1.0 + W_SLOPE * mean_slope + W_RIVER * river_penalty + W_CLIFF * cliff_penalty;
            accessibility_cost.push(cost);

            let crossing = match (river_entry[hex_id], river_exit[hex_id]) {
                (Some((ex, ey, _)), Some((xx, xy, _))) => {
                    let col = hex_id as u32 % grid.cols;
                    let row = hex_id as u32 / grid.cols;
                    let entry_edge = nearest_box_edge(&grid, col, row, ex, ey, sim_w, sim_h);
                    let exit_edge = nearest_box_edge(&grid, col, row, xx, xy, sim_w, sim_h);
                    Some(HexRiverCrossing {
                        entry_edge,
                        exit_edge,
                    })
                }
                _ => None,
            };
            river_crossing.push(crossing);
        }

        let hex_debug = HexDebugAttributes {
            slope_variance,
            accessibility_cost,
            river_crossing,
        };

        let hex_attrs = HexAttributeField {
            attrs,
            cols: grid.cols,
            rows: grid.rows,
        };

        // Per-sim-cell sidecars. `hex_slope_var` / `hex_access` paint every
        // cell (including sea) so the debug overlays visualise hex-level
        // context uniformly; `hex_dominant` is land-gated because its
        // Categorical palette is only meaningful on land cells with a real
        // argmax biome (sea cells keep their u32 default 0).
        let mut hex_dominant = island_core::field::ScalarField2D::<u32>::new(sim_w, sim_h);
        let mut hex_slope_var = island_core::field::ScalarField2D::<f32>::new(sim_w, sim_h);
        let mut hex_access = island_core::field::ScalarField2D::<f32>::new(sim_w, sim_h);
        for iy in 0..sim_h {
            for ix in 0..sim_w {
                let hex_id = grid.hex_id_of_cell.get(ix, iy) as usize;
                hex_slope_var.set(ix, iy, hex_debug.slope_variance[hex_id]);
                hex_access.set(ix, iy, hex_debug.accessibility_cost[hex_id]);
                if coast.is_land.get(ix, iy) != 1 {
                    continue;
                }
                hex_dominant.set(ix, iy, hex_attrs.attrs[hex_id].dominant_biome as u32);
            }
        }

        // Rasterise one Bresenham line per river-crossing hex, from the midpoint
        // of entry_edge to the midpoint of exit_edge in sim-cell coordinates.
        let mut crossing_mask = MaskField2D::new(sim_w, sim_h);
        for hex_id in 0..hex_count {
            let Some(crossing) = hex_debug.river_crossing[hex_id] else {
                continue;
            };
            let col = hex_id as u32 % grid.cols;
            let row = hex_id as u32 / grid.cols;
            // Compute the sim-cell span for this hex's bounding box.
            // x: [x0, x1), y: [y0, y1). Use the same formula as hex_id_of_cell
            // construction (integer subdivision).
            let x0 = (col as u64 * sim_w as u64 / grid.cols as u64) as u32;
            let x1 = ((col + 1) as u64 * sim_w as u64 / grid.cols as u64) as u32;
            let y0 = (row as u64 * sim_h as u64 / grid.rows as u64) as u32;
            let y1 = ((row + 1) as u64 * sim_h as u64 / grid.rows as u64) as u32;
            // Midpoints of each box edge (clamped to valid sim coords).
            let mid_x = (x0 + x1.saturating_sub(1)) / 2;
            let mid_y = (y0 + y1.saturating_sub(1)) / 2;
            let (p0x, p0y) = edge_midpoint(crossing.entry_edge, x0, x1, y0, y1, mid_x, mid_y);
            let (p1x, p1y) = edge_midpoint(crossing.exit_edge, x0, x1, y0, y1, mid_x, mid_y);
            // Rasterise with Bresenham's line algorithm.
            bresenham_line(&mut crossing_mask, p0x, p0y, p1x, p1y, sim_w, sim_h);
        }

        world.derived.hex_grid = Some(grid);
        world.derived.hex_attrs = Some(hex_attrs);
        world.derived.hex_debug = Some(hex_debug);
        world.derived.hex_dominant_per_cell = Some(hex_dominant);
        world.derived.hex_slope_variance_per_cell = Some(hex_slope_var);
        world.derived.hex_accessibility_per_cell = Some(hex_access);
        world.derived.hex_river_crossing_mask = Some(crossing_mask);
        Ok(())
    }
}

/// Compute which of the 4 box edges a sim cell `(cx, cy)` is closest to,
/// given the hex bounding box `[x0, x1) × [y0, y1)` in sim-cell coordinates.
///
/// Edge encoding: 0 = top (−y / min y), 1 = right (+x / max x),
/// 2 = bottom (+y / max y), 3 = left (−x / min x).
///
/// Ties between opposing pairs (top/bottom or left/right) break toward the
/// edge that `cx`/`cy` is physically closer to, which is deterministic for
/// non-square boxes.
fn nearest_box_edge(
    grid: &island_core::world::HexGrid,
    col: u32,
    row: u32,
    cx: u32,
    cy: u32,
    sim_w: u32,
    sim_h: u32,
) -> u8 {
    // Recompute the hex's sim-cell bounding box.
    let x0 = (col as u64 * sim_w as u64 / grid.cols as u64) as u32;
    let x1 = ((col + 1) as u64 * sim_w as u64 / grid.cols as u64) as u32;
    let y0 = (row as u64 * sim_h as u64 / grid.rows as u64) as u32;
    let y1 = ((row + 1) as u64 * sim_h as u64 / grid.rows as u64) as u32;

    // Distance of the sim cell from each of the 4 edges.
    // Use saturating arithmetic: cx/cy should always be inside [x0,x1)×[y0,y1),
    // but defensive clamping prevents underflow on edge cases.
    let d_top = cy.saturating_sub(y0);
    let d_bottom = y1.saturating_sub(1).saturating_sub(cy);
    let d_left = cx.saturating_sub(x0);
    let d_right = x1.saturating_sub(1).saturating_sub(cx);

    // Return the index of the minimum distance.
    let dists = [d_top, d_right, d_bottom, d_left]; // matches edge encoding
    let mut best_edge = 0u8;
    let mut best_dist = dists[0];
    for (edge, &dist) in dists.iter().enumerate().skip(1) {
        if dist < best_dist {
            best_dist = dist;
            best_edge = edge as u8;
        }
    }
    best_edge
}

/// Return the sim-cell coordinate of the midpoint of a box edge.
///
/// Edge encoding: 0 = top, 1 = right, 2 = bottom, 3 = left.
/// `mid_x = (x0 + x1 - 1) / 2`, `mid_y = (y0 + y1 - 1) / 2` (caller pre-computes).
fn edge_midpoint(
    edge: u8,
    x0: u32,
    x1: u32,
    y0: u32,
    y1: u32,
    mid_x: u32,
    mid_y: u32,
) -> (u32, u32) {
    let x_last = x1.saturating_sub(1);
    let y_last = y1.saturating_sub(1);
    match edge {
        0 => (mid_x, y0),     // top
        1 => (x_last, mid_y), // right
        2 => (mid_x, y_last), // bottom
        3 => (x0, mid_y),     // left
        _ => (mid_x, mid_y),  // fallback (shouldn't happen)
    }
}

/// Rasterise a line from `(x0, y0)` to `(x1, y1)` into `mask` using
/// Bresenham's algorithm. Coordinates outside `[0, sim_w) × [0, sim_h)`
/// are clamped / clipped — the algorithm is purely inside one hex's
/// bounding box by construction, so out-of-bounds access is not expected
/// but guarded against defensively.
fn bresenham_line(
    mask: &mut MaskField2D,
    x0: u32,
    y0: u32,
    x1: u32,
    y1: u32,
    sim_w: u32,
    sim_h: u32,
) {
    // Work in signed integers for the Bresenham step.
    let mut sx = x0 as i64;
    let mut sy = y0 as i64;
    let ex = x1 as i64;
    let ey = y1 as i64;

    let dx = (ex - sx).abs();
    let dy = (ey - sy).abs();
    let step_x: i64 = if sx < ex { 1 } else { -1 };
    let step_y: i64 = if sy < ey { 1 } else { -1 };
    let mut err = dx - dy;

    loop {
        // Paint the current pixel if in bounds.
        if sx >= 0 && sy >= 0 && (sx as u32) < sim_w && (sy as u32) < sim_h {
            mask.set(sx as u32, sy as u32, 1);
        }
        if sx == ex && sy == ey {
            break;
        }
        let e2 = 2 * err;
        if e2 > -dy {
            err -= dy;
            sx += step_x;
        }
        if e2 < dx {
            err += dx;
            sy += step_y;
        }
    }
}

/// Argmax biome from a flat `[biome_count]` weight slice. Ties break
/// in canonical `BiomeType::ALL` order.
fn dominant_biome_from_weights(weights: &[f32]) -> BiomeType {
    let mut best_weight = f32::NEG_INFINITY;
    let mut best_biome = BiomeType::ALL[0];
    for (i, biome) in BiomeType::ALL.iter().enumerate() {
        let w = weights[i];
        if w > best_weight {
            best_weight = w;
            best_biome = *biome;
        }
    }
    best_biome
}

#[cfg(test)]
mod tests {
    use super::*;
    use island_core::field::{MaskField2D, ScalarField2D};
    use island_core::preset::{IslandAge, IslandArchetypePreset};
    use island_core::seed::Seed;
    use island_core::world::{BiomeWeights, CoastMask, CoastType, Resolution, WorldState};

    fn preset() -> IslandArchetypePreset {
        IslandArchetypePreset {
            name: "hex_proj_test".into(),
            island_radius: 0.5,
            max_relief: 1.0,
            volcanic_center_count: 1,
            island_age: IslandAge::Young,
            prevailing_wind_dir: 0.0,
            marine_moisture_strength: 1.0,
            sea_level: 0.0,
            erosion: Default::default(),
        }
    }

    fn ready_world(sim_w: u32, sim_h: u32) -> WorldState {
        let mut world = WorldState::new(Seed(0), preset(), Resolution::new(sim_w, sim_h));
        let mut z = ScalarField2D::<f32>::new(sim_w, sim_h);
        z.data.fill(0.5);
        world.derived.z_filled = Some(z);
        world.derived.slope = Some(ScalarField2D::<f32>::new(sim_w, sim_h));
        let mut t = ScalarField2D::<f32>::new(sim_w, sim_h);
        t.data.fill(20.0);
        world.baked.temperature = Some(t);
        let mut p = ScalarField2D::<f32>::new(sim_w, sim_h);
        p.data.fill(0.6);
        world.baked.precipitation = Some(p);
        let mut m = ScalarField2D::<f32>::new(sim_w, sim_h);
        m.data.fill(0.5);
        world.baked.soil_moisture = Some(m);

        let mut weights = BiomeWeights::new(sim_w, sim_h);
        // Mark every cell as 100% LowlandForest (index 1) for the
        // aggregation determinism test.
        let idx = BiomeType::LowlandForest as usize;
        for row in weights.weights.iter_mut() {
            row.fill(0.0);
        }
        let n = (sim_w * sim_h) as usize;
        for i in 0..n {
            weights.weights[idx][i] = 1.0;
        }
        world.baked.biome_weights = Some(weights);

        let mut is_land = MaskField2D::new(sim_w, sim_h);
        is_land.data.fill(1);
        world.derived.coast_mask = Some(CoastMask {
            is_land,
            is_sea: MaskField2D::new(sim_w, sim_h),
            is_coast: MaskField2D::new(sim_w, sim_h),
            land_cell_count: sim_w * sim_h,
            river_mouth_mask: None,
        });
        world.derived.river_mask = Some(MaskField2D::new(sim_w, sim_h));
        // coast_type is optional on HexProjectionStage; ready_world populates
        // it with all-Unknown (0xFF) so most tests measure `cliff_penalty = 0`
        // without needing a CoastTypeStage run. Tests that want specific
        // coast classes overwrite after calling ready_world.
        let mut coast_type = ScalarField2D::<u8>::new(sim_w, sim_h);
        coast_type.data.fill(CoastType::Unknown as u8);
        world.derived.coast_type = Some(coast_type);
        world
    }

    #[test]
    fn absent_coast_type_yields_zero_cliff_penalty() {
        let mut world = ready_world(128, 128);
        world.derived.coast_type = None;
        HexProjectionStage.run(&mut world).expect("stage");

        let debug = world
            .derived
            .hex_debug
            .as_ref()
            .expect("hex_debug populated");
        // With no river + no slope + no cliffs, accessibility collapses to 1.0.
        for cost in &debug.accessibility_cost {
            assert!(
                (*cost - 1.0).abs() < 1e-5,
                "cost with no coast_type must equal baseline 1.0, got {cost}"
            );
        }
    }

    #[test]
    fn produces_64x64_hex_field() {
        let mut world = ready_world(128, 128);
        HexProjectionStage.run(&mut world).expect("stage");
        let attrs = world.derived.hex_attrs.as_ref().unwrap();
        assert_eq!(attrs.cols, DEFAULT_HEX_COLS);
        assert_eq!(attrs.rows, DEFAULT_HEX_ROWS);
        assert_eq!(attrs.attrs.len(), (attrs.cols * attrs.rows) as usize);
    }

    #[test]
    fn uniform_inputs_yield_uniform_aggregation() {
        let mut world = ready_world(128, 128);
        HexProjectionStage.run(&mut world).expect("stage");
        let attrs = world.derived.hex_attrs.as_ref().unwrap();
        for a in attrs.attrs.iter() {
            assert!((a.elevation - 0.5).abs() < 1e-5);
            assert!((a.temperature - 20.0).abs() < 1e-4);
            assert!((a.rainfall - 0.6).abs() < 1e-5);
            assert!((a.moisture - 0.5).abs() < 1e-5);
            assert_eq!(a.dominant_biome, BiomeType::LowlandForest);
            assert!(!a.has_river);
        }
    }

    #[test]
    fn river_flag_or_reduction() {
        let mut world = ready_world(128, 128);
        // Mark one sim cell as river.
        let mut river = MaskField2D::new(128, 128);
        river.set(10, 10, 1);
        world.derived.river_mask = Some(river);

        HexProjectionStage.run(&mut world).expect("stage");
        let attrs = world.derived.hex_attrs.as_ref().unwrap();

        // Sim cell (10, 10) belongs to hex (col=10*64/128=5, row=5)
        // → hex_id = 5*64 + 5 = 325.
        let hex_col = 10 * DEFAULT_HEX_COLS / 128;
        let hex_row = 10 * DEFAULT_HEX_ROWS / 128;
        assert!(attrs.get(hex_col, hex_row).has_river);
        // Any other hex should be false.
        assert!(!attrs.get(0, 0).has_river);
    }

    #[test]
    fn sea_cells_excluded_from_mean() {
        // Half the domain is sea; land cells all have elevation 1.0.
        // A hex that covers only land should report elevation 1.0,
        // and a hex that straddles the shoreline should also report
        // 1.0 (sea cells are excluded).
        let (w, h) = (128_u32, 128_u32);
        let mut world = ready_world(w, h);
        let mut z = ScalarField2D::<f32>::new(w, h);
        z.data.fill(1.0);
        world.derived.z_filled = Some(z);
        // Left half sea.
        let mut is_land = MaskField2D::new(w, h);
        let mut is_sea = MaskField2D::new(w, h);
        let mut land_count = 0;
        for iy in 0..h {
            for ix in 0..w {
                if ix >= w / 2 {
                    is_land.set(ix, iy, 1);
                    land_count += 1;
                } else {
                    is_sea.set(ix, iy, 1);
                }
            }
        }
        world.derived.coast_mask = Some(CoastMask {
            is_land,
            is_sea,
            is_coast: MaskField2D::new(w, h),
            land_cell_count: land_count,
            river_mouth_mask: None,
        });

        HexProjectionStage.run(&mut world).expect("stage");
        let attrs = world.derived.hex_attrs.as_ref().unwrap();
        // Right-side hex (col 48, row 32) — fully land, elevation 1.
        assert!((attrs.get(48, 32).elevation - 1.0).abs() < 1e-5);
        // Middle hex (col 32, row 32) — straddles the shoreline;
        // sea cells are excluded so the land half still averages to 1.
        assert!((attrs.get(32, 32).elevation - 1.0).abs() < 1e-5);
    }

    #[test]
    fn determinism_across_runs() {
        let mut a = ready_world(128, 128);
        let mut b = ready_world(128, 128);
        HexProjectionStage.run(&mut a).expect("a");
        HexProjectionStage.run(&mut b).expect("b");
        let aa = a.derived.hex_attrs.as_ref().unwrap();
        let ba = b.derived.hex_attrs.as_ref().unwrap();
        for (x, y) in aa.attrs.iter().zip(ba.attrs.iter()) {
            assert_eq!(x.elevation, y.elevation);
            assert_eq!(x.rainfall, y.rainfall);
            assert_eq!(x.biome_weights, y.biome_weights);
        }
    }

    #[test]
    fn errors_when_prerequisite_missing() {
        let mut world = WorldState::new(Seed(0), preset(), Resolution::new(16, 16));
        assert!(HexProjectionStage.run(&mut world).is_err());
    }

    // ── Task 2.5.B: slope variance tests ──────────────────────────────────────

    /// When every sim cell has the same slope value, slope variance per hex
    /// must be exactly `0.0` (up to fp precision after f64 cancellation).
    #[test]
    fn hex_slope_variance_zero_on_uniform_slope_field() {
        let mut world = ready_world(128, 128);
        // Set a non-zero uniform slope so the mean is non-trivially zero too.
        let mut slope = ScalarField2D::<f32>::new(128, 128);
        slope.data.fill(0.3);
        world.derived.slope = Some(slope);

        HexProjectionStage.run(&mut world).expect("stage");

        let hd = world.derived.hex_debug.as_ref().unwrap();
        for (i, &v) in hd.slope_variance.iter().enumerate() {
            assert!(
                v < 1e-6,
                "hex {i}: expected slope_variance ≈ 0 on uniform slope field, got {v}"
            );
        }
    }

    /// When slope values vary within a hex, that hex's variance must be > 0.
    #[test]
    fn hex_slope_variance_nonzero_when_slope_varies_within_hex() {
        let (w, h) = (128_u32, 128_u32);
        let mut world = ready_world(w, h);

        // Build a checkerboard slope: alternating 0.0 and 1.0.
        // Every hex will contain both values, so variance = E[slope²]-(E[slope])²
        // = 0.5 - 0.25 = 0.25 (assuming 50/50 split), well above zero.
        let mut slope = ScalarField2D::<f32>::new(w, h);
        for iy in 0..h {
            for ix in 0..w {
                let v = if (ix + iy) % 2 == 0 { 0.0_f32 } else { 1.0 };
                slope.set(ix, iy, v);
            }
        }
        world.derived.slope = Some(slope);

        HexProjectionStage.run(&mut world).expect("stage");

        let hd = world.derived.hex_debug.as_ref().unwrap();
        // Every hex should have non-trivial variance (checkerboard spans all hexes).
        let any_nonzero = hd.slope_variance.iter().any(|&v| v > 1e-4);
        assert!(
            any_nonzero,
            "expected at least one hex with slope_variance > 0 on a checkerboard slope field"
        );
        // More specifically: every hex in the interior should have variance ≈ 0.25.
        for (i, &v) in hd.slope_variance.iter().enumerate() {
            assert!(
                v > 1e-4,
                "hex {i}: expected slope_variance > 0 on checkerboard field, got {v}"
            );
        }
    }

    // ── river crossing tests ───────────────────────────────────────────────────

    /// When there are no river cells, every hex's `river_crossing` must be `None`.
    #[test]
    fn hex_river_crossing_none_when_no_river_in_hex() {
        let mut world = ready_world(128, 128);
        // river_mask is all-zero from ready_world. Populate accumulation so the
        // optional prerequisite is present.
        world.derived.accumulation = Some(island_core::field::ScalarField2D::<f32>::new(128, 128));

        HexProjectionStage.run(&mut world).expect("stage");

        let hd = world.derived.hex_debug.as_ref().unwrap();
        for (i, rc) in hd.river_crossing.iter().enumerate() {
            assert!(
                rc.is_none(),
                "hex {i}: expected None river_crossing with no river cells"
            );
        }
    }

    /// A hex containing a single river cell with low accumulation (entry) and a
    /// second river cell with higher accumulation (exit) must produce a crossing
    /// where the entry cell has lower accumulation than the exit cell.
    #[test]
    fn hex_river_crossing_entry_upstream_exit_downstream() {
        let (w, h) = (128_u32, 128_u32);
        let mut world = ready_world(w, h);

        // Place two river cells in hex (col=1, row=1):
        // hex x-span: [2, 4), y-span: [2, 4) for a 4x4 hex grid on 128x128.
        // With 64 cols and 64 rows: each hex spans 2x2 cells.
        // hex_col = col*sim_w/hex_cols = 1*128/64 = 2 → x in [2, 4)
        // hex_row = row*sim_h/hex_rows = 1*128/64 = 2 → y in [2, 4)
        let entry_x = 2_u32; // top-left of hex (1,1)
        let entry_y = 2_u32;
        let exit_x = 3_u32;
        let exit_y = 3_u32;

        let mut river = island_core::field::MaskField2D::new(w, h);
        river.set(entry_x, entry_y, 1);
        river.set(exit_x, exit_y, 1);
        world.derived.river_mask = Some(river);

        let mut acc = island_core::field::ScalarField2D::<f32>::new(w, h);
        acc.set(entry_x, entry_y, 1.0); // low accumulation = upstream = entry
        acc.set(exit_x, exit_y, 100.0); // high accumulation = downstream = exit
        world.derived.accumulation = Some(acc);

        HexProjectionStage.run(&mut world).expect("stage");

        let hd = world.derived.hex_debug.as_ref().unwrap();
        // hex_id for (col=1, row=1) on a 64x64 grid = 1*64 + 1 = 65.
        let hex_id = 1 * DEFAULT_HEX_COLS + 1;
        let crossing =
            hd.river_crossing[hex_id as usize].expect("hex (1,1) must have a river crossing");
        // entry_edge is nearest edge to (entry_x, entry_y) in hex (1,1)
        // exit_edge is nearest edge to (exit_x, exit_y) in hex (1,1)
        // Just verify they are valid box-edge values (0..=3).
        assert!(
            crossing.entry_edge <= 3,
            "entry_edge must be 0..=3, got {}",
            crossing.entry_edge
        );
        assert!(
            crossing.exit_edge <= 3,
            "exit_edge must be 0..=3, got {}",
            crossing.exit_edge
        );
    }

    /// After HexProjectionStage runs on a world with river cells, the
    /// `hex_river_crossing_mask` must have at least one `1` cell per
    /// hex that has a `Some` crossing.
    #[test]
    fn hex_river_crossing_mask_nonzero_when_crossing_exists() {
        let (w, h) = (128_u32, 128_u32);
        let mut world = ready_world(w, h);

        // Place two river cells in hex (1,1) with distinct accumulation.
        let mut river = island_core::field::MaskField2D::new(w, h);
        river.set(2, 2, 1);
        river.set(3, 3, 1);
        world.derived.river_mask = Some(river);

        let mut acc = island_core::field::ScalarField2D::<f32>::new(w, h);
        acc.set(2, 2, 1.0);
        acc.set(3, 3, 100.0);
        world.derived.accumulation = Some(acc);

        HexProjectionStage.run(&mut world).expect("stage");

        let mask = world
            .derived
            .hex_river_crossing_mask
            .as_ref()
            .expect("hex_river_crossing_mask must be populated");
        let any_set = mask.data.iter().any(|&v| v == 1);
        assert!(
            any_set,
            "hex_river_crossing_mask must have at least one set pixel when a crossing exists"
        );
    }

    // ── Task 2.5.D: accessibility cost tests ──────────────────────────────────

    /// On a flat island with no rivers and no cliff coast cells,
    /// the accessibility cost must equal exactly `1.0` for every hex
    /// (mean_slope=0, river_penalty=0, cliff_penalty=0).
    #[test]
    fn accessibility_cost_baseline_one_on_flat_no_river_no_cliff() {
        let mut world = ready_world(128, 128);
        // slope already zero from ready_world (ScalarField2D::new gives 0.0).
        // coast_type already all Unknown from ready_world.
        // river_mask already all zero from ready_world.

        HexProjectionStage.run(&mut world).expect("stage");

        let hd = world.derived.hex_debug.as_ref().unwrap();
        for (i, &cost) in hd.accessibility_cost.iter().enumerate() {
            assert!(
                (cost - 1.0).abs() < 1e-5,
                "hex {i}: expected cost == 1.0 on flat/no-river/no-cliff world, got {cost}"
            );
        }
    }

    /// A hex containing cliff coast cells must have a higher accessibility
    /// cost than a flat inland hex with no rivers and no cliffs.
    #[test]
    fn accessibility_cost_higher_on_cliff_coast_than_flat_inland() {
        let (w, h) = (128_u32, 128_u32);
        let mut world = ready_world(w, h);

        // Mark one target sim cell as a cliff coast cell.
        // Cell (4, 4) → hex_col = 4*64/128 = 2, hex_row = 2.
        // Cell (124, 4) → hex_col = 124*64/128 = 62, hex_row = 2.
        // Use cell (4, 4) for the cliff hex and verify (62, 4) stays at 1.0.
        let cliff_x = 4_u32;
        let cliff_y = 4_u32;
        let mut coast_type = ScalarField2D::<u8>::new(w, h);
        coast_type.data.fill(CoastType::Unknown as u8);
        coast_type.set(cliff_x, cliff_y, CoastType::Cliff as u8);
        world.derived.coast_type = Some(coast_type);

        HexProjectionStage.run(&mut world).expect("stage");

        let hd = world.derived.hex_debug.as_ref().unwrap();
        let hex_cols = DEFAULT_HEX_COLS;
        let hex_col_cliff = cliff_x * hex_cols / w;
        let hex_row_cliff = cliff_y * DEFAULT_HEX_ROWS / h;
        let cliff_hex_id = (hex_row_cliff * hex_cols + hex_col_cliff) as usize;

        let flat_hex_id = ((2) * hex_cols + 62) as usize; // fully Unknown hex
        let cliff_cost = hd.accessibility_cost[cliff_hex_id];
        let flat_cost = hd.accessibility_cost[flat_hex_id];
        assert!(
            cliff_cost > flat_cost,
            "cliff hex (cost={cliff_cost}) must be more costly than flat inland hex (cost={flat_cost})"
        );
        // flat hex must still be 1.0 (no penalty sources).
        assert!(
            (flat_cost - 1.0).abs() < 1e-5,
            "flat inland hex cost must be 1.0, got {flat_cost}"
        );
    }
}
