//! Coast-type classification stage — Sprint 2 DD4.
//!
//! Assigns every coast cell one of four [`CoastType`] categories using cheap
//! proxies (slope, river-mouth flag, shoreline-normal wind exposure, and
//! island age). Non-coast cells are left at the `Unknown = 0xFF` sentinel.
//!
//! Priority order (first match wins):
//! 1. River-mouth cells → `Estuary`
//! 2. Steep + high wind exposure → `Cliff`
//! 3. Mid-steep + (young island OR leeward) → `RockyHeadland`
//! 4. Shallow slope → `Beach`
//! 5. Fall-through → `RockyHeadland`

use island_core::field::ScalarField2D;
use island_core::pipeline::SimulationStage;
use island_core::preset::IslandAge;
use island_core::world::{CoastType, WorldState};

// ─── Classification constants (DD4) ──────────────────────────────────────────
//
// v1 thresholds were calibrated against a hypothetical "18 % relief drop on
// volcanic_single" projection (sprint doc DD1). Task 2.6 empirical measurement
// showed coastal slope magnitudes rarely exceed 0.15 under the safe K=2e-3
// calibration, so the v1 constants (0.30/0.18/0.05/0.30) put 100 % of coast
// cells in the Beach bin. Tuned to v1.1 values below to satisfy §11 open
// problem #3 ("每 type 至少 5 % 占比 per preset × hero shot").

/// Slope threshold above which a coast cell qualifies as a cliff if also
/// facing into the prevailing wind (`exposure > EXPOSURE_HIGH`).
pub const S_CLIFF_HIGH: f32 = 0.07;

/// Slope threshold above which a coast cell qualifies as a rocky headland
/// when young or when leeward (slope > S_CLIFF_MID but not wind-exposed enough
/// for Cliff).
pub const S_CLIFF_MID: f32 = 0.04;

/// Slope threshold below which a coast cell is classified as a beach.
pub const S_BEACH_LOW: f32 = 0.02;

/// Wind-exposure threshold for the Cliff branch. A cell is "windward" when
/// `exposure = -dot(outward_normal, wind_from_direction) > EXPOSURE_HIGH`.
pub const EXPOSURE_HIGH: f32 = 0.05;

// ─── CoastTypeStage ───────────────────────────────────────────────────────────

/// Sprint 2 DD4: per-coast-cell geomorphology classification.
///
/// Reads:
/// * `derived.slope` (DerivedGeomorphStage)
/// * `derived.coast_mask` (CoastMaskStage + RiverExtractionStage)
/// * `derived.shoreline_normal` (CoastMaskStage)
/// * `preset.prevailing_wind_dir`
/// * `preset.island_age`
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
        // ── prerequisite checks ───────────────────────────────────────────────
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

        // ── read parameters ───────────────────────────────────────────────────
        let wind_angle = world.preset.prevailing_wind_dir;
        // `wind_from` is the unit vector in the direction wind TRAVELS
        // (matches `climate::common::wind_unit`). The name retains the
        // existing convention from the sprint doc DD4 pseudo-code; see
        // `exposure` below, which flips the sign so "coast faces wind" →
        // positive exposure.
        let wind_from = (wind_angle.cos(), wind_angle.sin());
        let age_bias = match world.preset.island_age {
            IslandAge::Young => 1.0_f32,
            IslandAge::Mature => 0.0_f32,
            IslandAge::Old => -1.0_f32,
        };

        // ── read derived prerequisites via raw pointers ───────────────────────
        // We need immutable borrows of slope, coast_mask, shoreline_normal while
        // writing derived.coast_type. Rust's borrow checker disallows having
        // &world.derived.X and &mut world.derived simultaneously, so we read
        // the data we need before taking &mut world.
        let slope_data: Vec<f32> = world.derived.slope.as_ref().unwrap().data.clone();
        let is_coast_data: Vec<u8> = world
            .derived
            .coast_mask
            .as_ref()
            .unwrap()
            .is_coast
            .data
            .clone();
        let river_mouth_data: Option<Vec<u8>> = world
            .derived
            .coast_mask
            .as_ref()
            .unwrap()
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

        // ── classify ──────────────────────────────────────────────────────────
        let mut out = ScalarField2D::<u8>::new(width, height);
        // Fill with Unknown sentinel so non-coast cells never get a valid type.
        out.data.fill(CoastType::Unknown as u8);

        for i in 0..n_cells {
            if is_coast_data[i] != 1 {
                continue; // non-coast: leave 0xFF in place
            }

            let s = slope_data[i];
            let is_river_mouth = river_mouth_data.as_ref().is_some_and(|rmm| rmm[i] == 1);
            let normal = normal_data[i]; // outward unit normal [f32; 2]

            // exposure = -dot(outward_normal, wind_from)
            // Positive when the coast faces into the wind (windward).
            let exposure = -(normal[0] * wind_from.0 + normal[1] * wind_from.1);

            let coast_type = if is_river_mouth {
                CoastType::Estuary
            } else if s > S_CLIFF_HIGH && exposure > EXPOSURE_HIGH {
                CoastType::Cliff
            } else if s > S_CLIFF_MID && age_bias >= 0.0 {
                CoastType::RockyHeadland
            } else if s < S_BEACH_LOW {
                CoastType::Beach
            } else {
                CoastType::RockyHeadland
            };

            out.data[i] = coast_type as u8;
        }

        world.derived.coast_type = Some(out);
        Ok(())
    }
}

// ─── tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use island_core::field::{MaskField2D, ScalarField2D, VectorField2D};
    use island_core::preset::{IslandAge, IslandArchetypePreset};
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
        }
    }

    /// Build a 3×3 world with a single coast cell at (1, 1) and the given slope,
    /// river_mouth flag, and shoreline normal. All other cells are non-coast.
    fn build_world_with_coast_cell(
        slope: f32,
        is_river_mouth: bool,
        normal: [f32; 2],
        island_age: IslandAge,
        wind_dir: f32,
    ) -> WorldState {
        let w = 3u32;
        let h = 3u32;
        let preset = minimal_preset(island_age, wind_dir);
        let mut world = WorldState::new(Seed(0), preset, Resolution::new(w, h));

        // slope field — all cells get the requested slope
        let mut slope_field = ScalarField2D::<f32>::new(w, h);
        slope_field.data.fill(slope);
        world.derived.slope = Some(slope_field);

        // coast_mask — only cell (1,1) = index 4 is coast
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

        // shoreline_normal — all cells get the requested normal
        let mut sn = VectorField2D::new(w, h);
        sn.data.fill(normal);
        world.derived.shoreline_normal = Some(sn);

        world
    }

    fn run_and_get(world: &mut WorldState) -> Vec<u8> {
        CoastTypeStage.run(world).expect("CoastTypeStage::run");
        world.derived.coast_type.as_ref().unwrap().data.clone()
    }

    // ── Test 1: river-mouth wins priority over steep + windward ──────────────

    #[test]
    fn coast_type_river_mouth_wins_priority() {
        // slope > S_CLIFF_HIGH and exposure 1.0 > EXPOSURE_HIGH, but is_river_mouth takes priority.
        // wind_dir=0 → wind_from=(1,0); outward normal=(-1,0) → exposure = -((-1)*1) = 1.0
        let mut world = build_world_with_coast_cell(
            0.40,        // steep
            true,        // river_mouth
            [-1.0, 0.0], // outward normal pointing west (exposure 1.0 with wind from west)
            IslandAge::Young,
            0.0, // wind_dir
        );
        let out = run_and_get(&mut world);
        assert_eq!(
            out[4],
            CoastType::Estuary as u8,
            "river_mouth must win even when slope={S_CLIFF_HIGH} and exposure is high"
        );
    }

    // ── Test 2: cliff when steep + windward ───────────────────────────────────

    #[test]
    fn coast_type_cliff_when_steep_and_windward() {
        let wind_dir = 0.0_f32; // wind_from = (1, 0)
        // outward normal = (-1, 0) → exposure = -((-1)*1 + 0*0) = 1.0 > 0.30
        let mut world =
            build_world_with_coast_cell(0.40, false, [-1.0, 0.0], IslandAge::Mature, wind_dir);
        let out = run_and_get(&mut world);
        assert_eq!(out[4], CoastType::Cliff as u8, "steep windward → Cliff");
    }

    // ── Test 3: rocky headland when steep + leeward or young ─────────────────

    #[test]
    fn coast_type_rocky_headland_when_steep_leeward_or_young() {
        // slope > S_CLIFF_MID (0.20 > 0.06), exposure < EXPOSURE_HIGH (leeward)
        // age_bias = Young → 1.0 ≥ 0.0 → RockyHeadland
        let wind_dir = std::f32::consts::PI; // wind_from = (-1, 0)
        // outward normal = (-1, 0) → exposure = -((-1)*(-1) + 0*0) = -1 < EXPOSURE_HIGH
        let mut world =
            build_world_with_coast_cell(0.20, false, [-1.0, 0.0], IslandAge::Young, wind_dir);
        let out = run_and_get(&mut world);
        assert_eq!(
            out[4],
            CoastType::RockyHeadland as u8,
            "steep leeward + Young → RockyHeadland"
        );
    }

    // ── Test 4: beach when shallow slope ─────────────────────────────────────

    #[test]
    fn coast_type_beach_when_shallow() {
        // slope 0.015 < S_BEACH_LOW (0.02)
        let wind_dir = 0.0_f32;
        let mut world = build_world_with_coast_cell(
            0.015,
            false,
            [0.0, 1.0], // any normal
            IslandAge::Old,
            wind_dir,
        );
        let out = run_and_get(&mut world);
        assert_eq!(out[4], CoastType::Beach as u8, "shallow slope → Beach");
    }

    // ── Test 5: catch-all rocky headland for medium slope + leeward ──────────

    #[test]
    fn coast_type_catch_all_rocky_headland_for_medium_slope_low_exposure() {
        // slope 0.04: between S_BEACH_LOW (0.02) and S_CLIFF_MID (0.06).
        // Not a Beach (slope > S_BEACH_LOW), exposure -0.1 < EXPOSURE_HIGH so
        // not a Cliff, and age_bias = Old (−1 < 0) fails the mid-slope
        // RockyHeadland-via-MID branch. Falls through to catch-all
        // RockyHeadland.
        let wind_dir = 0.0_f32; // wind_from = (1, 0)
        // Want exposure = -0.1: -(n[0]*1 + n[1]*0) = -n[0] = -0.1 → n[0] = 0.1
        let mut world = build_world_with_coast_cell(
            0.04,
            false,
            [0.1, 0.0], // n[0]=0.1 → exposure = -0.1
            IslandAge::Old,
            wind_dir,
        );
        let out = run_and_get(&mut world);
        assert_eq!(
            out[4],
            CoastType::RockyHeadland as u8,
            "medium slope + leeward + Old → fall-through RockyHeadland"
        );
    }

    // ── Test 6: non-coast cells remain 0xFF ───────────────────────────────────

    #[test]
    fn coast_type_non_coast_cells_remain_unknown_sentinel() {
        let mut world =
            build_world_with_coast_cell(0.40, false, [-1.0, 0.0], IslandAge::Young, 0.0);
        let out = run_and_get(&mut world);
        // Only cell 4 is coast; all others must be 0xFF
        for (i, &cell) in out.iter().enumerate() {
            if i == 4 {
                // coast cell may be any valid type
                assert_ne!(
                    cell,
                    CoastType::Unknown as u8,
                    "coast cell must not be Unknown"
                );
            } else {
                assert_eq!(
                    cell,
                    CoastType::Unknown as u8,
                    "non-coast cell {i} must be Unknown sentinel (0xFF)"
                );
            }
        }
    }

    // ── Tests 7a–7c: prerequisite missing returns error ───────────────────────

    #[test]
    fn coast_type_missing_slope_returns_error() {
        let w = 2u32;
        let h = 2u32;
        let mut world = WorldState::new(
            Seed(0),
            minimal_preset(IslandAge::Young, 0.0),
            Resolution::new(w, h),
        );
        // Populate coast_mask and shoreline_normal but NOT slope
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
        // slope is None → must error
        let result = CoastTypeStage.run(&mut world);
        assert!(result.is_err(), "expected error when slope is missing");
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("derived.slope"),
            "error message must mention derived.slope, got: {msg}"
        );
    }

    #[test]
    fn coast_type_missing_coast_mask_returns_error() {
        let w = 2u32;
        let h = 2u32;
        let mut world = WorldState::new(
            Seed(0),
            minimal_preset(IslandAge::Young, 0.0),
            Resolution::new(w, h),
        );
        let mut slope = ScalarField2D::<f32>::new(w, h);
        slope.data.fill(0.1);
        world.derived.slope = Some(slope);
        world.derived.shoreline_normal = Some(VectorField2D::new(w, h));
        // coast_mask is None → must error
        let result = CoastTypeStage.run(&mut world);
        assert!(result.is_err(), "expected error when coast_mask is missing");
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("derived.coast_mask"),
            "error message must mention derived.coast_mask, got: {msg}"
        );
    }

    #[test]
    fn coast_type_missing_shoreline_normal_returns_error() {
        let w = 2u32;
        let h = 2u32;
        let mut world = WorldState::new(
            Seed(0),
            minimal_preset(IslandAge::Young, 0.0),
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
        // shoreline_normal is None → must error
        let result = CoastTypeStage.run(&mut world);
        assert!(
            result.is_err(),
            "expected error when shoreline_normal is missing"
        );
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("derived.shoreline_normal"),
            "error message must mention derived.shoreline_normal, got: {msg}"
        );
    }
}
