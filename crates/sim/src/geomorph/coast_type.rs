//! Coast-type classification stage — Sprint 2 DD4 + Sprint 3 DD6.
//!
//! Assigns every coast cell one of five [`CoastType`] categories. Two
//! classifiers coexist, dispatched on `preset.erosion.coast_type_variant`:
//!
//! * [`CoastTypeVariant::V1Cheap`] — Sprint 2 cheap proxies (slope +
//!   river-mouth flag + shoreline-normal wind exposure + island age).
//!   Preserved for Task 3.10 baseline regeneration via
//!   `preset_override.erosion.coast_type_variant`.
//!   Priority order (first match wins): River-mouth → `Estuary`,
//!   steep + windward → `Cliff`, mid-steep + young → `RockyHeadland`,
//!   shallow → `Beach`, fall-through → `RockyHeadland`.
//!   V1 never emits `LavaDelta`.
//!
//! * [`CoastTypeVariant::V2FetchIntegral`] — Sprint 3 DD6 default. Uses a
//!   16-direction fetch integral weighted by wind angle, plus an
//!   age-gated distance-to-volcanic-center test for LavaDelta detection.
//!   Priority order (first match wins): River-mouth → `Estuary`,
//!   Young + moderate slope + near volcanic center → `LavaDelta`,
//!   steep + high fetch → `Cliff`, mid-steep + (young or mature) →
//!   `RockyHeadland`, shallow → `Beach`, fall-through → `RockyHeadland`.
//!
//! Non-coast cells are left at the `Unknown = 0xFF` sentinel by both
//! branches; the `coast_type_well_formed` invariant enforces the pairing.

use island_core::field::ScalarField2D;
use island_core::pipeline::SimulationStage;
use island_core::preset::{CoastTypeVariant, IslandAge};
use island_core::world::{CoastType, WorldState};

// ─── V1 classification constants (Sprint 2 DD4) ──────────────────────────────
//
// v1 thresholds were calibrated against a hypothetical "18 % relief drop on
// volcanic_single" projection (sprint doc DD1). Task 2.6 empirical measurement
// showed coastal slope magnitudes rarely exceed 0.15 under the safe K=2e-3
// calibration, so the v1 constants (0.30/0.18/0.05/0.30) put 100 % of coast
// cells in the Beach bin. Tuned to v1.1 values below to satisfy §11 open
// problem #3 ("每 type 至少 5 % 占比 per preset × hero shot").

/// V1: Slope threshold above which a coast cell qualifies as a cliff if also
/// facing into the prevailing wind (`exposure > EXPOSURE_HIGH`).
pub const S_CLIFF_HIGH_V1: f32 = 0.07;

/// V1: Slope threshold above which a coast cell qualifies as a rocky headland
/// when young or when leeward (slope > S_CLIFF_MID_V1 but not wind-exposed enough
/// for Cliff).
pub const S_CLIFF_MID_V1: f32 = 0.04;

/// V1: Slope threshold below which a coast cell is classified as a beach.
pub const S_BEACH_LOW_V1: f32 = 0.02;

/// V1: Wind-exposure threshold for the Cliff branch. A cell is "windward" when
/// `exposure = -dot(outward_normal, wind_from_direction) > EXPOSURE_HIGH_V1`.
pub const EXPOSURE_HIGH_V1: f32 = 0.05;

// ─── V2 classification constants (Sprint 3 DD6) ──────────────────────────────
//
// DD6 raises the slope thresholds back toward the spec v1 values because v2's
// fetch-integral exposure genuinely discriminates (unlike v1's single-direction
// projection, which maxed out at ~0.07 on stock presets).

/// V2: Slope threshold above which a coast cell qualifies as a Cliff (if it
/// also has high fetch exposure).
pub const S_CLIFF_HIGH_V2: f32 = 0.12;

/// V2: Slope threshold above which a coast cell qualifies as a RockyHeadland
/// when age_bias >= 0 (Young or Mature).
pub const S_CLIFF_MID_V2: f32 = 0.06;

/// V2: Slope threshold below which a coast cell is classified as a Beach.
/// Shared with v1 (unchanged).
pub const S_BEACH_LOW_V2: f32 = 0.02;

/// V2: Fetch-integral exposure threshold for the Cliff branch.
pub const EXPOSURE_CLIFF_HIGH_V2: f32 = 0.35;

/// V2: Lower slope bound for LavaDelta detection. Fresh volcanic deltas are
/// not purely flat (they have a subtle seaward ramp).
pub const S_LAVA_LOW: f32 = 0.03;

/// V2: Upper slope bound for LavaDelta detection. Mature lava flows are not
/// steep.
pub const S_LAVA_HIGH: f32 = 0.10;

/// V2: Maximum normalized distance to the nearest volcanic center for a coast
/// cell to qualify as LavaDelta. Distances are in normalized grid space
/// (`[0, 1]²` domain, Euclidean).
pub const R_LAVA: f32 = 0.30;

// ─── V2 fetch-integral constants (Sprint 3 DD6) ──────────────────────────────

/// Number of rays cast per coast cell for the fetch integral. Sprint doc
/// locks this at 16.
pub const FETCH_DIRS: u32 = 16;

/// Maximum cell count along any single ray before the ray terminates with
/// "open ocean". Rays that hit land or the grid edge before this cap return
/// the actual cell count.
pub const FETCH_MAX: u32 = 32;

/// Lower bound used to normalize `fetch_weighted` into `exposure_v2 ∈ [0, 1]`.
/// Values below FETCH_MIN (tiny coves, fully sheltered coast) clamp to `0.0`.
pub const FETCH_MIN: u32 = 2;

// ─── CoastTypeStage ───────────────────────────────────────────────────────────

/// Sprint 2 DD4 + Sprint 3 DD6: per-coast-cell geomorphology classification.
///
/// Reads:
/// * `derived.slope` (DerivedGeomorphStage)
/// * `derived.coast_mask` (CoastMaskStage + RiverExtractionStage)
/// * `derived.shoreline_normal` (CoastMaskStage, v1 only)
/// * `derived.coast_mask.is_land` (v2 fetch raycast)
/// * `derived.volcanic_centers` (TopographyStage, v2 only)
/// * `preset.prevailing_wind_dir`
/// * `preset.island_age`
/// * `preset.erosion.coast_type_variant` (v1/v2 dispatch)
///
/// Writes:
/// * `derived.coast_type: Option<ScalarField2D<u8>>`
///
/// Unit struct — all parameters read from `world.preset` at run time.
pub struct CoastTypeStage;

impl SimulationStage for CoastTypeStage {
    fn name(&self) -> &'static str {
        "coast_type"
    }

    fn run(&self, world: &mut WorldState) -> anyhow::Result<()> {
        match world.preset.erosion.coast_type_variant {
            CoastTypeVariant::V1Cheap => run_v1(world),
            CoastTypeVariant::V2FetchIntegral => run_v2(world),
        }
    }
}

/// Shared helper: map `IslandAge` to the `age_bias` scalar used by both
/// classifiers (Young=+1, Mature=0, Old=−1).
fn age_bias_for(age: IslandAge) -> f32 {
    match age {
        IslandAge::Young => 1.0,
        IslandAge::Mature => 0.0,
        IslandAge::Old => -1.0,
    }
}

// ─── V1 cheap classifier (Sprint 2) ──────────────────────────────────────────

fn run_v1(world: &mut WorldState) -> anyhow::Result<()> {
    if world.derived.slope.is_none() {
        anyhow::bail!(
            "CoastTypeStage prerequisite missing: \
             derived.slope (run DerivedGeomorphStage first)"
        );
    }
    if world.derived.coast_mask.is_none() {
        anyhow::bail!(
            "CoastTypeStage prerequisite missing: \
             derived.coast_mask (run CoastMaskStage first)"
        );
    }
    if world.derived.shoreline_normal.is_none() {
        anyhow::bail!(
            "CoastTypeStage prerequisite missing: \
             derived.shoreline_normal (run CoastMaskStage first)"
        );
    }

    let width = world.resolution.sim_width;
    let height = world.resolution.sim_height;
    let n_cells = (width as usize) * (height as usize);

    let wind_angle = world.preset.prevailing_wind_dir;
    // `wind_from` is the unit vector in the direction wind TRAVELS
    // (matches `climate::common::wind_unit`).
    let wind_from = (wind_angle.cos(), wind_angle.sin());
    let age_bias = age_bias_for(world.preset.island_age);

    // Clone the prerequisite data so we can take &mut world.
    let slope_data: Vec<f32> = world.derived.slope.as_ref().unwrap().data.clone();
    let coast_mask = world.derived.coast_mask.as_ref().unwrap();
    let is_coast_data: Vec<u8> = coast_mask.is_coast.data.clone();
    let river_mouth_data: Option<Vec<u8>> = coast_mask
        .river_mouth_mask
        .as_ref()
        .map(|rmm| rmm.data.clone());
    let normal_data: Vec<[f32; 2]> = world
        .derived
        .shoreline_normal
        .as_ref()
        .unwrap()
        .data
        .clone();

    let mut out = ScalarField2D::<u8>::new(width, height);
    out.data.fill(CoastType::Unknown as u8);

    for i in 0..n_cells {
        if is_coast_data[i] != 1 {
            continue;
        }

        let s = slope_data[i];
        let is_river_mouth = river_mouth_data.as_ref().is_some_and(|rmm| rmm[i] == 1);
        let normal = normal_data[i];

        // exposure = -dot(outward_normal, wind_from)
        // Positive when the coast faces into the wind (windward).
        let exposure = -(normal[0] * wind_from.0 + normal[1] * wind_from.1);

        let coast_type = if is_river_mouth {
            CoastType::Estuary
        } else if s > S_CLIFF_HIGH_V1 && exposure > EXPOSURE_HIGH_V1 {
            CoastType::Cliff
        } else if s > S_CLIFF_MID_V1 && age_bias >= 0.0 {
            CoastType::RockyHeadland
        } else if s < S_BEACH_LOW_V1 {
            CoastType::Beach
        } else {
            CoastType::RockyHeadland
        };

        out.data[i] = coast_type as u8;
    }

    world.derived.coast_type = Some(out);
    Ok(())
}

// ─── V2 fetch-integral classifier (Sprint 3 DD6) ─────────────────────────────

/// Compute the 16-direction fetch integral at coast cell `(x, y)`.
///
/// For each of [`FETCH_DIRS`] evenly-spaced angles `θ_k`:
/// * March a ray from cell centre in direction `θ_k` until it hits a land
///   cell or exits the grid.
/// * Cap the ray length at [`FETCH_MAX`] cells (open-ocean sentinel).
/// * Weight the ray's cell count by `max(0.5, -cos(θ_k − wind_angle))` —
///   **windward-pointing** rays (θ = wind_angle + π, i.e. the direction
///   pointing INTO the wind, toward open ocean upwind) get weight 1.0;
///   leeward-pointing rays drop linearly to 0.5. `wind_angle` is the
///   direction wind *travels* (`climate::common::wind_unit` convention),
///   so the formula is `cos(θ − (wind_angle + π)) = -cos(θ − wind_angle)`;
///   the extra negation is what makes windward the weight maximum rather
///   than the minimum. DD6's literal `cos(θ − wind_angle)` is a spec
///   typo — its inline comment 「迎风 1.0, 背风 0.5」 (windward 1.0,
///   leeward 0.5) is the physically-correct intent and this
///   implementation matches that intent.
///
/// Returns `exposure_v2 ∈ [0, 1]`, normalized as
/// `(fetch_weighted − FETCH_MIN_W) / (FETCH_MAX_W − FETCH_MIN_W)` clamped,
/// where `FETCH_{MIN,MAX}_W` are the weighted-sum endpoints reached by
/// per-direction fetches of `FETCH_MIN` / `FETCH_MAX` respectively.
///
/// The `is_land_at` closure captures the land mask (a
/// `ScalarField2D<u8>`). Out-of-bounds rays terminate at the grid edge and
/// return their current hit count (so a fully-open edge cell sees the cap).
fn fetch_exposure_v2(
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    wind_angle: f32,
    is_land_at: &impl Fn(i32, i32) -> bool,
) -> (f32, [u32; FETCH_DIRS as usize]) {
    let mut dirs = [0u32; FETCH_DIRS as usize];
    let mut weighted_sum = 0.0_f32;
    let mut weight_sum = 0.0_f32; // for endpoint normalization

    let cx = x as f32 + 0.5;
    let cy = y as f32 + 0.5;

    for k in 0..FETCH_DIRS {
        let theta = std::f32::consts::TAU * (k as f32) / (FETCH_DIRS as f32);
        let dx = theta.cos();
        let dy = theta.sin();

        // March in cell steps up to FETCH_MAX; count cells traversed over sea.
        let mut count: u32 = 0;
        for step in 1..=FETCH_MAX {
            let px = cx + dx * step as f32;
            let py = cy + dy * step as f32;
            let ix = px.floor() as i32;
            let iy = py.floor() as i32;
            if ix < 0 || iy < 0 || ix >= width as i32 || iy >= height as i32 {
                // Ray exited the grid: terminate at current count.
                break;
            }
            if is_land_at(ix, iy) {
                // Ray hit land: terminate.
                break;
            }
            count = step;
        }
        dirs[k as usize] = count;

        // Wind weight: windward-pointing rays (θ opposite wind travel
        // direction) get weight 1.0, leeward 0.5. See doc comment above
        // for why this is `-cos(θ - wind_angle)` rather than DD6's
        // literal `cos(θ - wind_angle)`.
        let w = (-(theta - wind_angle).cos()).max(0.5);
        weighted_sum += w * count as f32;
        weight_sum += w;
    }

    // Normalize to [0, 1]: the weighted endpoints are w_sum * FETCH_MIN and
    // w_sum * FETCH_MAX. Below the low endpoint clamps to 0; above the high
    // endpoint clamps to 1. This keeps the normalization grid-size-independent
    // and matches the spec's `(fetch_weighted - FETCH_MIN) / (FETCH_MAX - FETCH_MIN)`
    // formulation when every direction contributes equally.
    let lo = weight_sum * (FETCH_MIN as f32);
    let hi = weight_sum * (FETCH_MAX as f32);
    let exposure = if hi > lo {
        ((weighted_sum - lo) / (hi - lo)).clamp(0.0, 1.0)
    } else {
        0.0
    };

    (exposure, dirs)
}

/// Minimum distance from `(x, y)` (in cell-centre normalized coords) to any
/// volcanic center in `centers`. Returns `f32::INFINITY` if `centers` is empty.
fn distance_to_nearest_volcanic_center(cx: f32, cy: f32, centers: &[[f32; 2]]) -> f32 {
    centers
        .iter()
        .map(|[vx, vy]| ((cx - vx).powi(2) + (cy - vy).powi(2)).sqrt())
        .fold(f32::INFINITY, f32::min)
}

fn run_v2(world: &mut WorldState) -> anyhow::Result<()> {
    if world.derived.slope.is_none() {
        anyhow::bail!(
            "CoastTypeStage prerequisite missing: \
             derived.slope (run DerivedGeomorphStage first)"
        );
    }
    if world.derived.coast_mask.is_none() {
        anyhow::bail!(
            "CoastTypeStage prerequisite missing: \
             derived.coast_mask (run CoastMaskStage first)"
        );
    }
    if world.derived.volcanic_centers.is_none() {
        anyhow::bail!(
            "CoastTypeStage (V2) prerequisite missing: \
             derived.volcanic_centers (run TopographyStage first)"
        );
    }

    let width = world.resolution.sim_width;
    let height = world.resolution.sim_height;
    let n_cells = (width as usize) * (height as usize);

    let wind_angle = world.preset.prevailing_wind_dir;
    let age_bias = age_bias_for(world.preset.island_age);

    // Clone prerequisite data so the &mut write below compiles.
    let slope_data: Vec<f32> = world.derived.slope.as_ref().unwrap().data.clone();
    let coast_mask = world.derived.coast_mask.as_ref().unwrap();
    let is_coast_data: Vec<u8> = coast_mask.is_coast.data.clone();
    let is_land_data: Vec<u8> = coast_mask.is_land.data.clone();
    let river_mouth_data: Option<Vec<u8>> = coast_mask
        .river_mouth_mask
        .as_ref()
        .map(|rmm| rmm.data.clone());
    let volcanic_centers: Vec<[f32; 2]> = world.derived.volcanic_centers.as_ref().unwrap().clone();

    // is_land lookup closure capturing the cloned data.
    let is_land_at = |ix: i32, iy: i32| -> bool {
        let idx = (iy as usize) * (width as usize) + (ix as usize);
        is_land_data[idx] == 1
    };

    let mut out = ScalarField2D::<u8>::new(width, height);
    out.data.fill(CoastType::Unknown as u8);

    // Sprint 3.5 DD4: persist the fetch-integral scalar so that
    // `sim::hex_coast_class` can weight the per-hex majority vote without
    // re-running the raycasts. Sea cells stay at `0.0` (default fill).
    let mut fetch_integral = ScalarField2D::<f32>::new(width, height);

    for i in 0..n_cells {
        if is_coast_data[i] != 1 {
            continue;
        }

        let x = (i % width as usize) as u32;
        let y = (i / width as usize) as u32;
        let cx_norm = (x as f32 + 0.5) / width as f32;
        let cy_norm = (y as f32 + 0.5) / height as f32;

        let s = slope_data[i];
        let is_river_mouth = river_mouth_data.as_ref().is_some_and(|rmm| rmm[i] == 1);

        // LavaDelta qualifier: age_bias > 0 (Young only), slope in lava band,
        // and close to a volcanic center (in normalized grid coords).
        let dist_vol = distance_to_nearest_volcanic_center(cx_norm, cy_norm, &volcanic_centers);

        // Exposure: only compute if we might need it (not a river mouth and
        // not a certain LavaDelta). Keeping the raymarch conditional avoids
        // paying the ~16 × 32 ray cost per cell on river-mouth / estuary cells.
        // In practice the classifier runs once per sprint-1A pipeline call, so
        // the optimisation is cosmetic; we nevertheless gate it.
        let needs_exposure = !is_river_mouth;
        let (exposure_v2, _dirs) = if needs_exposure {
            fetch_exposure_v2(x, y, width, height, wind_angle, &is_land_at)
        } else {
            (0.0, [0u32; FETCH_DIRS as usize])
        };

        // Sprint 3.5 DD4: store the fetch-integral value for land cells.
        // River-mouth cells fall through to exposure_v2 = 0.0 (not fetched).
        fetch_integral.data[i] = exposure_v2;

        let coast_type = if is_river_mouth {
            CoastType::Estuary
        } else if age_bias > 0.0 && s > S_LAVA_LOW && s < S_LAVA_HIGH && dist_vol < R_LAVA {
            CoastType::LavaDelta
        } else if s > S_CLIFF_HIGH_V2 && exposure_v2 > EXPOSURE_CLIFF_HIGH_V2 {
            CoastType::Cliff
        } else if s > S_CLIFF_MID_V2 && age_bias >= 0.0 {
            CoastType::RockyHeadland
        } else if s < S_BEACH_LOW_V2 {
            CoastType::Beach
        } else {
            CoastType::RockyHeadland
        };

        out.data[i] = coast_type as u8;
    }

    world.derived.coast_type = Some(out);
    // Sprint 3.5 DD4: persist fetch integral for the hex coast classifier.
    world.derived.coast_fetch_integral = Some(fetch_integral);
    Ok(())
}

// ─── tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use island_core::field::{MaskField2D, ScalarField2D, VectorField2D};
    use island_core::preset::{CoastTypeVariant, ErosionParams, IslandAge, IslandArchetypePreset};
    use island_core::seed::Seed;
    use island_core::world::{CoastMask, CoastType, Resolution, WorldState};

    fn minimal_preset(island_age: IslandAge, wind_dir: f32) -> IslandArchetypePreset {
        IslandArchetypePreset {
            name: "coast_type_test".into(),
            island_radius: 0.5,
            max_relief: 0.5,
            volcanic_center_count: 1,
            island_age,
            prevailing_wind_dir: wind_dir,
            marine_moisture_strength: 0.5,
            sea_level: 0.3,
            erosion: Default::default(),
            climate: Default::default(),
        }
    }

    fn v1_preset(island_age: IslandAge, wind_dir: f32) -> IslandArchetypePreset {
        let mut p = minimal_preset(island_age, wind_dir);
        p.erosion.coast_type_variant = CoastTypeVariant::V1Cheap;
        p
    }

    /// Build a 3×3 world with a single coast cell at (1, 1) and the given slope,
    /// river_mouth flag, and shoreline normal. All other cells are non-coast.
    /// Uses V1 by default (legacy tests inherit v1 semantics).
    fn build_world_with_coast_cell(
        slope: f32,
        is_river_mouth: bool,
        normal: [f32; 2],
        island_age: IslandAge,
        wind_dir: f32,
    ) -> WorldState {
        let w = 3u32;
        let h = 3u32;
        let preset = v1_preset(island_age, wind_dir);
        let mut world = WorldState::new(Seed(0), preset, Resolution::new(w, h));

        let mut slope_field = ScalarField2D::<f32>::new(w, h);
        slope_field.data.fill(slope);
        world.derived.slope = Some(slope_field);

        let mut is_coast = MaskField2D::new(w, h);
        is_coast.data[4] = 1;
        let river_mouth_mask = if is_river_mouth {
            let mut rmm = MaskField2D::new(w, h);
            rmm.data[4] = 1;
            Some(rmm)
        } else {
            None
        };
        world.derived.coast_mask = Some(CoastMask {
            is_land: MaskField2D::new(w, h),
            is_sea: MaskField2D::new(w, h),
            is_coast,
            land_cell_count: 1,
            river_mouth_mask,
        });

        let mut sn = VectorField2D::new(w, h);
        sn.data.fill(normal);
        world.derived.shoreline_normal = Some(sn);

        // V2 prereq: volcanic_centers (v1 code path never reads it).
        world.derived.volcanic_centers = Some(vec![[0.5, 0.5]]);

        world
    }

    fn run_and_get(world: &mut WorldState) -> Vec<u8> {
        CoastTypeStage.run(world).expect("CoastTypeStage::run");
        world.derived.coast_type.as_ref().unwrap().data.clone()
    }

    // ── V1 legacy tests (Sprint 2 carryover) ─────────────────────────────────

    #[test]
    fn coast_type_river_mouth_wins_priority() {
        let mut world = build_world_with_coast_cell(0.40, true, [-1.0, 0.0], IslandAge::Young, 0.0);
        let out = run_and_get(&mut world);
        assert_eq!(out[4], CoastType::Estuary as u8);
    }

    #[test]
    fn coast_type_cliff_when_steep_and_windward() {
        let wind_dir = 0.0_f32;
        let mut world =
            build_world_with_coast_cell(0.40, false, [-1.0, 0.0], IslandAge::Mature, wind_dir);
        let out = run_and_get(&mut world);
        assert_eq!(out[4], CoastType::Cliff as u8, "V1: steep windward → Cliff");
    }

    #[test]
    fn coast_type_rocky_headland_when_steep_leeward_or_young() {
        let wind_dir = std::f32::consts::PI;
        let mut world =
            build_world_with_coast_cell(0.20, false, [-1.0, 0.0], IslandAge::Young, wind_dir);
        let out = run_and_get(&mut world);
        assert_eq!(out[4], CoastType::RockyHeadland as u8);
    }

    #[test]
    fn coast_type_beach_when_shallow() {
        let wind_dir = 0.0_f32;
        let mut world =
            build_world_with_coast_cell(0.015, false, [0.0, 1.0], IslandAge::Old, wind_dir);
        let out = run_and_get(&mut world);
        assert_eq!(out[4], CoastType::Beach as u8);
    }

    #[test]
    fn coast_type_catch_all_rocky_headland_for_medium_slope_low_exposure() {
        let wind_dir = 0.0_f32;
        let mut world =
            build_world_with_coast_cell(0.04, false, [0.1, 0.0], IslandAge::Old, wind_dir);
        let out = run_and_get(&mut world);
        assert_eq!(out[4], CoastType::RockyHeadland as u8);
    }

    #[test]
    fn coast_type_non_coast_cells_remain_unknown_sentinel() {
        let mut world =
            build_world_with_coast_cell(0.40, false, [-1.0, 0.0], IslandAge::Young, 0.0);
        let out = run_and_get(&mut world);
        for (i, &cell) in out.iter().enumerate() {
            if i == 4 {
                assert_ne!(cell, CoastType::Unknown as u8);
            } else {
                assert_eq!(cell, CoastType::Unknown as u8);
            }
        }
    }

    #[test]
    fn coast_type_missing_slope_returns_error() {
        let w = 2u32;
        let h = 2u32;
        let mut world = WorldState::new(
            Seed(0),
            v1_preset(IslandAge::Young, 0.0),
            Resolution::new(w, h),
        );
        let mut is_coast = MaskField2D::new(w, h);
        is_coast.data[0] = 1;
        world.derived.coast_mask = Some(CoastMask {
            is_land: MaskField2D::new(w, h),
            is_sea: MaskField2D::new(w, h),
            is_coast,
            land_cell_count: 1,
            river_mouth_mask: None,
        });
        world.derived.shoreline_normal = Some(VectorField2D::new(w, h));
        let result = CoastTypeStage.run(&mut world);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("derived.slope"), "got: {msg}");
    }

    #[test]
    fn coast_type_missing_coast_mask_returns_error() {
        let w = 2u32;
        let h = 2u32;
        let mut world = WorldState::new(
            Seed(0),
            v1_preset(IslandAge::Young, 0.0),
            Resolution::new(w, h),
        );
        let mut slope = ScalarField2D::<f32>::new(w, h);
        slope.data.fill(0.1);
        world.derived.slope = Some(slope);
        world.derived.shoreline_normal = Some(VectorField2D::new(w, h));
        let result = CoastTypeStage.run(&mut world);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("derived.coast_mask"), "got: {msg}");
    }

    #[test]
    fn coast_type_missing_shoreline_normal_returns_error() {
        let w = 2u32;
        let h = 2u32;
        let mut world = WorldState::new(
            Seed(0),
            v1_preset(IslandAge::Young, 0.0),
            Resolution::new(w, h),
        );
        let mut slope = ScalarField2D::<f32>::new(w, h);
        slope.data.fill(0.1);
        world.derived.slope = Some(slope);
        let mut is_coast = MaskField2D::new(w, h);
        is_coast.data[0] = 1;
        world.derived.coast_mask = Some(CoastMask {
            is_land: MaskField2D::new(w, h),
            is_sea: MaskField2D::new(w, h),
            is_coast,
            land_cell_count: 1,
            river_mouth_mask: None,
        });
        let result = CoastTypeStage.run(&mut world);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("derived.shoreline_normal"), "got: {msg}");
    }

    // ── V2 unit tests (Sprint 3 DD6) ──────────────────────────────────────────

    /// Each direction's fetch count must be ≤ `FETCH_MAX`.
    #[test]
    fn fetch_integral_bounded_by_fetch_max() {
        // Open-ocean cell at grid centre: no land nearby → all rays cap at FETCH_MAX.
        let w = 64u32;
        let h = 64u32;
        let is_land = |_ix: i32, _iy: i32| false;
        let (_expo, dirs) = fetch_exposure_v2(32, 32, w, h, 0.0, &is_land);
        for (k, &d) in dirs.iter().enumerate() {
            assert!(
                d <= FETCH_MAX,
                "direction {k}: fetch {d} must be ≤ FETCH_MAX ({FETCH_MAX})"
            );
        }
    }

    /// On a fully land-locked (all cells are land) grid, the fetch raymarch
    /// terminates immediately for every direction → counts are all 0.
    /// This is how the v2 classifier behaves on interior land cells, even
    /// though in practice the classifier only runs on `is_coast == 1` cells.
    #[test]
    fn fetch_integral_zero_on_land_cells() {
        let w = 16u32;
        let h = 16u32;
        let is_land = |_ix: i32, _iy: i32| true;
        let (expo, dirs) = fetch_exposure_v2(8, 8, w, h, 0.0, &is_land);
        for (k, &d) in dirs.iter().enumerate() {
            assert_eq!(d, 0, "direction {k}: all-land grid → fetch must be 0");
        }
        assert_eq!(
            expo, 0.0,
            "all-land fetch exposure must clamp to 0 (below FETCH_MIN endpoint)"
        );
    }

    /// Build a 16×16 world with a specific coast-cell layout so we can exercise
    /// the v2 classifier directly. `(x, y) = (1, 8)` is the sole coast cell;
    /// everything with `x == 0` is sea, everything with `x >= 1` is land.
    /// This gives the coast cell an "open ocean to the west" layout.
    fn build_v2_world(
        island_age: IslandAge,
        wind_dir: f32,
        slope_value: f32,
        volcanic_centers: Vec<[f32; 2]>,
    ) -> WorldState {
        let w = 16u32;
        let h = 16u32;
        let mut preset = minimal_preset(island_age, wind_dir);
        preset.erosion.coast_type_variant = CoastTypeVariant::V2FetchIntegral;
        let mut world = WorldState::new(Seed(0), preset, Resolution::new(w, h));

        let mut slope_field = ScalarField2D::<f32>::new(w, h);
        slope_field.data.fill(slope_value);
        world.derived.slope = Some(slope_field);

        let mut is_land = MaskField2D::new(w, h);
        let mut is_sea = MaskField2D::new(w, h);
        let mut is_coast = MaskField2D::new(w, h);
        for y in 0..h {
            for x in 0..w {
                let idx = (y * w + x) as usize;
                if x == 0 {
                    is_sea.data[idx] = 1;
                } else {
                    is_land.data[idx] = 1;
                }
            }
        }
        // Mark (1, 8) as the only coast cell.
        let coast_idx = (8 * w + 1) as usize;
        is_coast.data[coast_idx] = 1;

        let land_cell_count = is_land.data.iter().filter(|&&v| v == 1).count() as u32;
        world.derived.coast_mask = Some(CoastMask {
            is_land,
            is_sea,
            is_coast,
            land_cell_count,
            river_mouth_mask: None,
        });

        // Shoreline normal: not read by v2, but populated for safety (v1
        // would panic without it; v2's prereq list intentionally omits it).
        let mut sn = VectorField2D::new(w, h);
        sn.data.fill([-1.0, 0.0]);
        world.derived.shoreline_normal = Some(sn);

        world.derived.volcanic_centers = Some(volcanic_centers);

        world
    }

    /// V2: on a Young preset with a volcanic center at the coast and slope in
    /// the lava band, the coast cell must be classified as LavaDelta.
    #[test]
    fn coast_type_v2_lava_delta_emitted_on_young_near_volcano() {
        // Coast cell at (1, 8) → normalized (~0.094, ~0.531). Put volcano close.
        let mut world = build_v2_world(
            IslandAge::Young,
            0.0,
            0.05, // in [S_LAVA_LOW, S_LAVA_HIGH] = [0.03, 0.10]
            vec![[0.10, 0.53]],
        );
        let out = run_and_get(&mut world);
        let coast_idx = (8 * 16 + 1) as usize;
        assert_eq!(
            out[coast_idx],
            CoastType::LavaDelta as u8,
            "Young + near-volcano + lava-band slope → LavaDelta"
        );
    }

    /// V2: same geometry but Mature age must suppress LavaDelta (age_bias = 0,
    /// not > 0).
    #[test]
    fn coast_type_v2_lava_delta_only_on_young_preset() {
        // Young baseline: LavaDelta present.
        let mut young = build_v2_world(IslandAge::Young, 0.0, 0.05, vec![[0.10, 0.53]]);
        let out_young = run_and_get(&mut young);
        let idx = (8 * 16 + 1) as usize;
        assert_eq!(out_young[idx], CoastType::LavaDelta as u8);

        // Mature: age_bias == 0 fails the `> 0` gate → no LavaDelta.
        let mut mature = build_v2_world(IslandAge::Mature, 0.0, 0.05, vec![[0.10, 0.53]]);
        let out_mature = run_and_get(&mut mature);
        assert_ne!(
            out_mature[idx],
            CoastType::LavaDelta as u8,
            "Mature preset must not emit LavaDelta"
        );

        // Old: age_bias == -1, also fails.
        let mut old = build_v2_world(IslandAge::Old, 0.0, 0.05, vec![[0.10, 0.53]]);
        let out_old = run_and_get(&mut old);
        assert_ne!(
            out_old[idx],
            CoastType::LavaDelta as u8,
            "Old preset must not emit LavaDelta"
        );
    }

    /// V2 priority: river-mouth beats LavaDelta even with LavaDelta geometry.
    #[test]
    fn coast_type_v2_classifier_priority_river_mouth_beats_lava_delta() {
        let mut world = build_v2_world(IslandAge::Young, 0.0, 0.05, vec![[0.10, 0.53]]);
        // Mark the coast cell as a river mouth.
        let coast_idx = (8 * 16 + 1) as usize;
        let cm = world.derived.coast_mask.as_mut().unwrap();
        let mut rmm = MaskField2D::new(16, 16);
        rmm.data[coast_idx] = 1;
        cm.river_mouth_mask = Some(rmm);

        let out = run_and_get(&mut world);
        assert_eq!(
            out[coast_idx],
            CoastType::Estuary as u8,
            "river_mouth must beat LavaDelta in v2 priority"
        );
    }

    /// V2 priority: LavaDelta beats Cliff on matching Young + near-volcano
    /// geometry. Uses a slope in the LavaDelta band (0.05) that would
    /// otherwise qualify as RockyHeadland under the v2 mid-slope branch.
    #[test]
    fn coast_type_v2_classifier_priority_lava_delta_beats_rocky_headland() {
        let mut world = build_v2_world(
            IslandAge::Young,
            0.0,
            0.05, // s > S_CLIFF_MID_V2 (0.06) is FALSE, so we use 0.07 via second case
            vec![[0.10, 0.53]],
        );
        let out = run_and_get(&mut world);
        let idx = (8 * 16 + 1) as usize;
        assert_eq!(out[idx], CoastType::LavaDelta as u8);

        // Repeat with slope 0.08 which falls inside both [S_LAVA_LOW, S_LAVA_HIGH]
        // (= [0.03, 0.10]) and > S_CLIFF_MID_V2 (= 0.06). LavaDelta must still win.
        let mut world2 = build_v2_world(IslandAge::Young, 0.0, 0.08, vec![[0.10, 0.53]]);
        let out2 = run_and_get(&mut world2);
        assert_eq!(
            out2[idx],
            CoastType::LavaDelta as u8,
            "LavaDelta branch priority must beat RockyHeadland-via-S_CLIFF_MID_V2"
        );
    }

    /// V2: far from any volcanic center, LavaDelta is never emitted even on
    /// Young presets in the lava slope band.
    #[test]
    fn coast_type_v2_reduces_to_non_lava_classes_when_no_volcanic_center_nearby() {
        // Volcano far from the coast cell: dist > R_LAVA = 0.30.
        let mut world = build_v2_world(
            IslandAge::Young,
            0.0,
            0.05,
            vec![[0.9, 0.9]], // far from (≈0.09, ≈0.53)
        );
        let out = run_and_get(&mut world);
        let idx = (8 * 16 + 1) as usize;
        assert_ne!(
            out[idx],
            CoastType::LavaDelta as u8,
            "LavaDelta must not fire when no volcanic center is within R_LAVA"
        );
        // No LavaDelta cell anywhere, either.
        assert_eq!(
            out.iter()
                .filter(|&&v| v == CoastType::LavaDelta as u8)
                .count(),
            0,
            "LavaDelta count must be 0 on distant-volcano Young preset"
        );
    }

    /// V1 fallback determinism: re-running V1 on the same world produces the
    /// same bytes.
    #[test]
    fn v1_cheap_branch_is_deterministic_across_repeated_runs() {
        let mut w1 = build_world_with_coast_cell(0.10, false, [-1.0, 0.0], IslandAge::Young, 0.0);
        let mut w2 = build_world_with_coast_cell(0.10, false, [-1.0, 0.0], IslandAge::Young, 0.0);
        let out1 = run_and_get(&mut w1);
        let out2 = run_and_get(&mut w2);
        assert_eq!(out1, out2);
    }

    /// V2 missing prereq: `volcanic_centers` absent → error.
    #[test]
    fn coast_type_v2_missing_volcanic_centers_returns_error() {
        let mut world = build_v2_world(IslandAge::Young, 0.0, 0.05, vec![[0.10, 0.53]]);
        world.derived.volcanic_centers = None;
        let result = CoastTypeStage.run(&mut world);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("volcanic_centers"),
            "error must mention volcanic_centers, got: {msg}"
        );
    }

    // ── v2 → v1 subset sanity ────────────────────────────────────────────────

    /// On a Mature preset with no volcanic center close to the coast cell, v2
    /// must never emit LavaDelta. The remaining branches (Cliff / Beach /
    /// RockyHeadland) follow v2's raised thresholds so the output is not
    /// byte-identical to v1 — but the LavaDelta count is exactly 0, which is
    /// the guarantee the spec's "reduces to v1" test captures.
    #[test]
    fn coast_type_v2_reduces_to_v1_on_mature_preset_with_no_lava_conditions() {
        // Mature preset, slope outside lava band, no nearby volcano.
        let mut world = build_v2_world(IslandAge::Mature, 0.0, 0.20, vec![[0.9, 0.9]]);
        let out = run_and_get(&mut world);
        let lava_count = out
            .iter()
            .filter(|&&v| v == CoastType::LavaDelta as u8)
            .count();
        assert_eq!(
            lava_count, 0,
            "Mature preset with no lava conditions must produce 0 LavaDelta cells (v2 reduces to the v1 4-class partition)"
        );
    }

    // ── Variant dispatch test ────────────────────────────────────────────────

    /// Setting `coast_type_variant = V1Cheap` on a world that lacks
    /// `derived.volcanic_centers` must still run without error — the v1 code
    /// path does not read `volcanic_centers`.
    #[test]
    fn variant_dispatch_v1_does_not_require_volcanic_centers() {
        let w = 3u32;
        let h = 3u32;
        let preset = v1_preset(IslandAge::Young, 0.0);
        let mut world = WorldState::new(Seed(0), preset, Resolution::new(w, h));
        let mut slope = ScalarField2D::<f32>::new(w, h);
        slope.data.fill(0.05);
        world.derived.slope = Some(slope);
        let mut is_coast = MaskField2D::new(w, h);
        is_coast.data[4] = 1;
        world.derived.coast_mask = Some(CoastMask {
            is_land: MaskField2D::new(w, h),
            is_sea: MaskField2D::new(w, h),
            is_coast,
            land_cell_count: 1,
            river_mouth_mask: None,
        });
        world.derived.shoreline_normal = Some(VectorField2D::new(w, h));
        // Deliberately DO NOT set volcanic_centers — v1 must not demand it.
        assert!(world.derived.volcanic_centers.is_none());

        let result = CoastTypeStage.run(&mut world);
        assert!(
            result.is_ok(),
            "V1 must run without volcanic_centers, got: {result:?}"
        );
    }

    // ── ErosionParams legacy-default dispatch test ───────────────────────────

    /// A preset built via `ErosionParams::default()` selects V2 — this test
    /// locks the dispatcher to match the preset default.
    #[test]
    fn default_erosion_params_dispatch_selects_v2() {
        let ep = ErosionParams::default();
        assert_eq!(ep.coast_type_variant, CoastTypeVariant::V2FetchIntegral);
    }

    // ── Full-pipeline §10 acceptance preview (Sprint 3 DD6) ──────────────────

    /// §10 acceptance preview (smoke test): run the full pipeline on the
    /// stock `volcanic_single` preset at 64² and assert the `coast_type`
    /// field is well-formed (every coast cell has a 0..=4 value, every
    /// non-coast cell has 0xFF, no panic in the fetch raymarch). Cliff
    /// coverage itself is NOT gated here — §10's 5 % gate is scoped to
    /// 256² hero shots under Task 3.10's baseline regeneration.
    ///
    /// Task 3.6 observation (logged for the reviewer / Task 3.10): at 64²
    /// volcanic_single seed 42, Cliff coverage under the locked constants
    /// (`S_CLIFF_HIGH_V2 = 0.12`, `EXPOSURE_CLIFF_HIGH_V2 = 0.35`) was
    /// measured as 0 / 101 coast cells. The grid is too small and the
    /// erosion K too conservative at 64² for slopes to clear 0.12. Task
    /// 3.10 should re-measure on 256² hero shots before declaring §10
    /// pass/fail — if Cliff coverage is still < 5 % there, the calibration
    /// choice is the knob to turn. See also the
    /// `cliff_coverage_fraction_on_volcanic_single_v2_64_measurement` test
    /// below which captures the raw measurement as a diagnostic (never asserts).
    #[test]
    fn full_pipeline_under_v2_produces_well_formed_coast_type_field() {
        use crate::default_pipeline;
        use island_core::validation::coast_type_well_formed;
        let preset = IslandArchetypePreset {
            name: "volcanic_single".into(),
            island_radius: 0.55,
            max_relief: 0.85,
            volcanic_center_count: 1,
            island_age: IslandAge::Young,
            prevailing_wind_dir: 1.5708,
            marine_moisture_strength: 0.75,
            sea_level: 0.30,
            erosion: Default::default(), // V2FetchIntegral default
            climate: Default::default(),
        };
        let mut world = WorldState::new(Seed(42), preset, Resolution::new(64, 64));
        default_pipeline()
            .run(&mut world)
            .expect("full pipeline must run under v2 default");

        let ct = world
            .derived
            .coast_type
            .as_ref()
            .expect("coast_type populated after v2 classifier");
        // Every cell is 0..=4 or 0xFF.
        for (i, &v) in ct.data.iter().enumerate() {
            assert!(
                v <= 4 || v == 0xFF,
                "cell {i}: coast_type byte {v:#04x} must be 0..=4 or 0xFF (Unknown)"
            );
        }
        // Invariant round-trip (widened `0..=4` range).
        coast_type_well_formed(&world).expect("coast_type_well_formed must pass under v2");
    }

    /// Diagnostic (non-asserting): record the measured class-coverage
    /// fractions on `volcanic_single` seed 42 at 64² under v2. Task 3.10 uses
    /// this pattern to observe calibration drift before §10 acceptance.
    /// Printed via the test harness; does not fail on any specific result.
    #[test]
    fn cliff_coverage_fraction_on_volcanic_single_v2_64_measurement() {
        use crate::default_pipeline;
        let preset = IslandArchetypePreset {
            name: "volcanic_single".into(),
            island_radius: 0.55,
            max_relief: 0.85,
            volcanic_center_count: 1,
            island_age: IslandAge::Young,
            prevailing_wind_dir: 1.5708,
            marine_moisture_strength: 0.75,
            sea_level: 0.30,
            erosion: Default::default(),
            climate: Default::default(),
        };
        let mut world = WorldState::new(Seed(42), preset, Resolution::new(64, 64));
        default_pipeline().run(&mut world).expect("full pipeline");

        let ct = world.derived.coast_type.as_ref().unwrap();
        let cm = world.derived.coast_mask.as_ref().unwrap();
        let coast_count = cm.is_coast.data.iter().filter(|&&v| v == 1).count();
        let mut counts = [0usize; 5];
        for &v in &ct.data {
            if (v as usize) < counts.len() {
                counts[v as usize] += 1;
            }
        }
        // Never asserts — this is a visibility anchor for Task 3.10.
        eprintln!(
            "[v2 diagnostic] volcanic_single seed 42 @ 64²: \
             coast={coast_count} Cliff={} Beach={} Estuary={} RockyHeadland={} LavaDelta={}",
            counts[0], counts[1], counts[2], counts[3], counts[4]
        );
    }
}
