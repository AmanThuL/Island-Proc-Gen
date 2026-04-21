//! Volcanic-island topography synthesis — Task 1A.1.
//!
//! Fills `world.authoritative.height` (z_raw), `world.authoritative.sediment`
//! (zeros), and `world.derived.initial_uplift` (pre-falloff snapshot).
//! All RNG is consumed during volcano/ridge placement; composition is purely
//! geometric, so same seed + same preset → bit-exact z_raw.

use rand::Rng;

use island_core::field::ScalarField2D;
use island_core::pipeline::SimulationStage;
use island_core::world::WorldState;

// ─── stream constants ─────────────────────────────────────────────────────────

/// RNG stream for volcano placement and per-volcano parameters.
const TOPOGRAPHY_VOLCANO_STREAM: u64 = 0x0001_0A01;

/// RNG stream for ridge arm sampling.
const TOPOGRAPHY_RIDGE_STREAM: u64 = 0x0001_0A02;

// ─── helpers ──────────────────────────────────────────────────────────────────

/// Standard cubic smoothstep in `[edge0, edge1]`.
#[inline]
fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Shortest distance from point `p` to the line segment `(a, b)`.
#[inline]
fn point_to_segment_dist(px: f32, py: f32, ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    let dx = bx - ax;
    let dy = by - ay;
    let len_sq = dx * dx + dy * dy;
    if len_sq < 1e-12 {
        // Degenerate segment: treat as a point
        return ((px - ax) * (px - ax) + (py - ay) * (py - ay)).sqrt();
    }
    let t = (((px - ax) * dx + (py - ay) * dy) / len_sq).clamp(0.0, 1.0);
    let cx = ax + t * dx;
    let cy = ay + t * dy;
    ((px - cx) * (px - cx) + (py - cy) * (py - cy)).sqrt()
}

// ─── volcano placement ────────────────────────────────────────────────────────

struct VolcanoParams {
    cx: f32,
    cy: f32,
    height: f32,
    radius: f32,
}

fn place_volcanoes(
    preset: &island_core::preset::IslandArchetypePreset,
    seed: &island_core::seed::Seed,
) -> Vec<VolcanoParams> {
    let n = preset.volcanic_center_count as usize;
    let r = preset.island_radius;
    let mut rng = seed.fork(TOPOGRAPHY_VOLCANO_STREAM).to_rng();

    let mut centers: Vec<(f32, f32)> = match n {
        0 | 1 => {
            let jx: f32 = rng.random_range(-0.05 * r..=0.05 * r);
            let jy: f32 = rng.random_range(-0.05 * r..=0.05 * r);
            vec![(0.5 + jx, 0.5 + jy)]
        }
        2 => {
            let offset = 0.25 * r;
            let jx0: f32 = rng.random_range(-0.03 * r..=0.03 * r);
            let jy0: f32 = rng.random_range(-0.03 * r..=0.03 * r);
            let jx1: f32 = rng.random_range(-0.03 * r..=0.03 * r);
            let jy1: f32 = rng.random_range(-0.03 * r..=0.03 * r);
            vec![
                (0.5 - offset + jx0, 0.5 - offset + jy0),
                (0.5 + offset + jx1, 0.5 + offset + jy1),
            ]
        }
        _ => {
            // n >= 3: evenly spaced around a ring
            let ring_r = 0.15 * r;
            (0..n)
                .map(|i| {
                    let angle = (i as f32) * std::f32::consts::TAU / (n as f32);
                    let jx: f32 = rng.random_range(-0.02 * r..=0.02 * r);
                    let jy: f32 = rng.random_range(-0.02 * r..=0.02 * r);
                    (
                        0.5 + ring_r * angle.cos() + jx,
                        0.5 + ring_r * angle.sin() + jy,
                    )
                })
                .collect()
        }
    };

    centers.truncate(n.max(1)); // n=0 degenerate: keep the 1 jittered centre

    centers
        .into_iter()
        .map(|(cx, cy)| {
            let h: f32 = rng.random_range(0.85_f32..=1.0_f32) * preset.max_relief;
            let vr: f32 = rng.random_range(0.55_f32..=0.85_f32) * r;
            VolcanoParams {
                cx,
                cy,
                height: h,
                radius: vr,
            }
        })
        .collect()
}

// ─── volcanic base ────────────────────────────────────────────────────────────

fn build_volcanic_base(
    w: u32,
    h: u32,
    volcanoes: &[VolcanoParams],
    is_caldera: bool,
) -> ScalarField2D<f32> {
    let mut field = ScalarField2D::<f32>::new(w, h);
    for iy in 0..h {
        for ix in 0..w {
            let px = (ix as f32 + 0.5) / w as f32;
            let py = (iy as f32 + 0.5) / h as f32;
            let mut val = 0.0_f32;
            for v in volcanoes {
                let dist = ((px - v.cx) * (px - v.cx) + (py - v.cy) * (py - v.cy)).sqrt();
                let u = dist / v.radius;
                let t = 1.0 - u.clamp(0.0, 1.0);
                let cone = v.height * t * t * (3.0 - 2.0 * t);

                let cone = if is_caldera {
                    let caldera_depth = 0.2 * v.height;
                    let r_inner = 0.1 * v.radius;
                    let r_outer = 0.25 * v.radius;
                    let bowl = if dist < r_outer {
                        let bowl_t = ((dist - r_inner) / (r_outer - r_inner)).clamp(0.0, 1.0);
                        let s = bowl_t * bowl_t * (3.0 - 2.0 * bowl_t);
                        caldera_depth * (1.0 - s)
                    } else {
                        0.0
                    };
                    cone - bowl
                } else {
                    cone
                };

                val = val.max(cone);
            }
            field.data[iy as usize * w as usize + ix as usize] = val;
        }
    }
    field
}

// ─── ridge arms ──────────────────────────────────────────────────────────────

struct ArmSegment {
    ax: f32,
    ay: f32,
    bx: f32,
    by: f32,
}

fn build_ridge_field(
    w: u32,
    h: u32,
    volcanoes: &[VolcanoParams],
    island_radius: f32,
    max_relief: f32,
    seed: &island_core::seed::Seed,
) -> ScalarField2D<f32> {
    let ridge_sigma = 0.03_f32;
    let ridge_height = 0.30 * max_relief;
    let arm_length = 0.6 * island_radius;

    let mut rng = seed.fork(TOPOGRAPHY_RIDGE_STREAM).to_rng();

    let mut arms: Vec<ArmSegment> = Vec::new();
    for (vi, v) in volcanoes.iter().enumerate() {
        let n_arms: u32 = rng.random_range(3_u32..=5_u32);
        let angle_offset = vi as f32 * 0.37; // per-volcano phase shift reduces arm aliasing
        for ai in 0..n_arms {
            let base_angle = ai as f32 * std::f32::consts::TAU / n_arms as f32 + angle_offset;
            let jitter: f32 = rng.random_range(-0.15_f32..=0.15_f32);
            let angle = base_angle + jitter;
            let bx = v.cx + arm_length * angle.cos();
            let by = v.cy + arm_length * angle.sin();
            arms.push(ArmSegment {
                ax: v.cx,
                ay: v.cy,
                bx,
                by,
            });
        }
    }

    let mut field = ScalarField2D::<f32>::new(w, h);
    for iy in 0..h {
        for ix in 0..w {
            let px = (ix as f32 + 0.5) / w as f32;
            let py = (iy as f32 + 0.5) / h as f32;

            let min_dist = arms
                .iter()
                .map(|arm| point_to_segment_dist(px, py, arm.ax, arm.ay, arm.bx, arm.by))
                .fold(f32::INFINITY, f32::min);

            let val = ridge_height * (-min_dist / ridge_sigma).exp();
            field.data[iy as usize * w as usize + ix as usize] = val;
        }
    }
    field
}

// ─── coastal falloff ──────────────────────────────────────────────────────────

fn build_coastal_falloff(
    w: u32,
    h: u32,
    island_radius: f32,
    max_relief: f32,
    centroid: (f32, f32),
) -> ScalarField2D<f32> {
    let coastal_amplitude = max_relief * 1.05;
    let edge0 = island_radius * 0.9;
    let edge1 = island_radius;
    let (cx, cy) = centroid;

    let mut field = ScalarField2D::<f32>::new(w, h);
    for iy in 0..h {
        for ix in 0..w {
            let px = (ix as f32 + 0.5) / w as f32;
            let py = (iy as f32 + 0.5) / h as f32;
            let dist = ((px - cx) * (px - cx) + (py - cy) * (py - cy)).sqrt();
            // NOTE: spec §D5 wrote `(1 - smoothstep(...))` which would invert the
            // ramp and zero out the centre. The correct intent is smoothstep rising
            // from 0 (inside) to 1 (outside), so subtraction erases terrain beyond
            // the island radius while leaving the volcanic peak untouched.
            let mask = smoothstep(edge0, edge1, dist);
            field.data[iy as usize * w as usize + ix as usize] = coastal_amplitude * mask;
        }
    }
    field
}

// ─── TopographyStage ─────────────────────────────────────────────────────────

/// Sprint 1A Task 1A.1: synthesises the initial volcanic-island heightfield.
///
/// Sets `world.authoritative.{height, sediment}` and `world.derived.initial_uplift`.
pub struct TopographyStage;

impl SimulationStage for TopographyStage {
    fn name(&self) -> &'static str {
        "topography"
    }

    fn run(&self, world: &mut WorldState) -> anyhow::Result<()> {
        let w = world.resolution.sim_width;
        let h = world.resolution.sim_height;
        let preset = &world.preset;
        let seed = &world.seed;

        let is_caldera = preset.name.contains("caldera");

        // §D3 Volcanic base
        let volcanoes = place_volcanoes(preset, seed);
        let volcanic_base = build_volcanic_base(w, h, &volcanoes, is_caldera);

        // §D4 Ridge mask (v0: straight segments)
        let ridge_field = build_ridge_field(
            w,
            h,
            &volcanoes,
            preset.island_radius,
            preset.max_relief,
            seed,
        );

        // §D5 Coastal falloff — centroid of volcano centres
        let n = volcanoes.len() as f32;
        let centroid = volcanoes
            .iter()
            .fold((0.0_f32, 0.0_f32), |(sx, sy), v| (sx + v.cx, sy + v.cy));
        let centroid = (centroid.0 / n, centroid.1 / n);
        let coastal_falloff =
            build_coastal_falloff(w, h, preset.island_radius, preset.max_relief, centroid);

        let n_cells = (w as usize) * (h as usize);
        let mut initial_uplift = ScalarField2D::<f32>::new(w, h);
        let mut height = ScalarField2D::<f32>::new(w, h);
        for i in 0..n_cells {
            let uplift = volcanic_base.data[i] + ridge_field.data[i];
            initial_uplift.data[i] = uplift;
            height.data[i] = (uplift - coastal_falloff.data[i]).clamp(0.0, 1.0);
        }

        world.derived.initial_uplift = Some(initial_uplift);
        world.authoritative.height = Some(height);
        world.authoritative.sediment = Some(ScalarField2D::<f32>::new(w, h));

        // Sprint 3 DD6: expose the sampled volcanic centers (in normalized
        // [0, 1]² grid coordinates, the same space `build_volcanic_base`
        // uses) to downstream stages. Consumed by the v2 `CoastTypeStage`
        // classifier for LavaDelta detection. Invalidated under the
        // Topography arm of `sim::invalidation::clear_stage_outputs`.
        world.derived.volcanic_centers = Some(volcanoes.iter().map(|v| [v.cx, v.cy]).collect());

        Ok(())
    }
}

// ─── tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use island_core::preset::{IslandAge, IslandArchetypePreset};
    use island_core::save::{LoadedWorld, SaveMode, read_world, write_world};
    use island_core::seed::Seed;
    use island_core::world::{Resolution, WorldState};

    use super::*;

    fn preset_single() -> IslandArchetypePreset {
        IslandArchetypePreset {
            name: "test_single".into(),
            island_radius: 0.45,
            max_relief: 0.8,
            volcanic_center_count: 1,
            island_age: IslandAge::Young,
            prevailing_wind_dir: 0.0,
            marine_moisture_strength: 0.5,
            sea_level: 0.3,
            erosion: Default::default(),
            climate: Default::default(),
        }
    }

    fn preset_caldera() -> IslandArchetypePreset {
        IslandArchetypePreset {
            name: "caldera_test".into(),
            island_radius: 0.45,
            max_relief: 0.8,
            volcanic_center_count: 1,
            island_age: IslandAge::Mature,
            prevailing_wind_dir: 0.0,
            marine_moisture_strength: 0.5,
            sea_level: 0.3,
            erosion: Default::default(),
            climate: Default::default(),
        }
    }

    fn run_stage(seed: u64, preset: IslandArchetypePreset, res: u32) -> WorldState {
        let mut world = WorldState::new(Seed(seed), preset, Resolution::new(res, res));
        TopographyStage
            .run(&mut world)
            .expect("TopographyStage failed");
        world
    }

    // 1. Max of z_raw should be in [0.85 * max_relief, 1.0] for single volcano.
    // (coastal_amplitude = max_relief * 1.05 > max_relief, so some cells will be sea)
    #[test]
    fn single_volcano_peak_height_matches_max_relief() {
        let preset = preset_single();
        let max_relief = preset.max_relief;
        let world = run_stage(42, preset, 64);
        let height = world.authoritative.height.as_ref().unwrap();
        let stats = height.stats().unwrap();
        assert!(
            stats.max <= 1.0,
            "max z_raw={} exceeds 1.0 (clamp violation)",
            stats.max
        );
        assert!(
            stats.max >= 0.85 * max_relief,
            "max z_raw={} is below 0.85 * max_relief={}",
            stats.max,
            0.85 * max_relief
        );
    }

    // 2. A cell clearly outside island_radius should be below sea_level.
    #[test]
    fn below_island_radius_goes_under_sea_level() {
        let preset = preset_single();
        let sea_level = preset.sea_level;
        let res = 64_u32;
        let world = run_stage(42, preset, res);
        let height = world.authoritative.height.as_ref().unwrap();

        // Pick a corner cell — clearly outside island_radius
        // domain coords: x=0, y=0 → px~0, py~0; dist from centre ~0.707
        // island_radius = 0.45 < 0.707, so well outside
        let corner_val = height.get(0, 0);
        assert!(
            corner_val < sea_level,
            "corner z_raw={corner_val} should be below sea_level={sea_level}"
        );
    }

    // 3. Same seed + same preset → bit-exact z_raw.
    #[test]
    fn determinism_bit_exact() {
        let p1 = preset_single();
        let p2 = preset_single();
        let w1 = run_stage(99, p1, 32);
        let w2 = run_stage(99, p2, 32);
        assert_eq!(
            w1.authoritative.height.as_ref().unwrap().data,
            w2.authoritative.height.as_ref().unwrap().data,
            "z_raw must be bit-exact for same seed+preset"
        );
    }

    // 4. All z_raw values in [0.0, 1.0].
    #[test]
    fn height_stays_in_unit_range() {
        let world = run_stage(7, preset_single(), 64);
        let stats = world
            .authoritative
            .height
            .as_ref()
            .unwrap()
            .stats()
            .unwrap();
        assert!(stats.min >= 0.0, "min z_raw={} < 0.0", stats.min);
        assert!(stats.max <= 1.0, "max z_raw={} > 1.0", stats.max);
    }

    // 5. Sediment is a zero field after run.
    #[test]
    fn sediment_is_zero_field_after_run() {
        let world = run_stage(1, preset_single(), 32);
        let sed = world
            .authoritative
            .sediment
            .as_ref()
            .expect("sediment must be Some");
        let stats = sed.stats().unwrap();
        assert_eq!(stats.min, 0.0, "sediment min={}", stats.min);
        assert_eq!(stats.max, 0.0, "sediment max={}", stats.max);
        assert_eq!(stats.mean, 0.0, "sediment mean={}", stats.mean);
        assert!(sed.data.iter().all(|&v| v == 0.0));
    }

    // 6. initial_uplift is Some, and uplift >= z_raw at every cell.
    #[test]
    fn initial_uplift_is_cached() {
        let world = run_stage(5, preset_single(), 32);
        let uplift = world
            .derived
            .initial_uplift
            .as_ref()
            .expect("initial_uplift must be Some");
        let height = world.authoritative.height.as_ref().unwrap();
        for (i, (&u, &z)) in uplift.data.iter().zip(height.data.iter()).enumerate() {
            assert!(
                u >= z - 1e-6,
                "cell {i}: uplift={u} < z_raw={z} (coastal falloff must only reduce)"
            );
        }
    }

    // 7. Minimal save round-trip: no MissingAuthoritativeField, height/sediment byte-equal.
    #[test]
    fn save_roundtrip_after_topography_stage() {
        let world = run_stage(42, preset_single(), 32);

        let mut buf = Vec::new();
        write_world(&world, SaveMode::Minimal, &mut buf).expect(
            "write_world should not return MissingAuthoritativeField after TopographyStage",
        );

        let mut cursor = Cursor::new(buf);
        let loaded = read_world(&mut cursor).expect("read_world failed");

        match loaded {
            LoadedWorld::Minimal(w2) => {
                assert_eq!(
                    w2.authoritative.height.as_ref().unwrap().to_bytes(),
                    world.authoritative.height.as_ref().unwrap().to_bytes(),
                    "height bytes mismatch after round-trip"
                );
                assert_eq!(
                    w2.authoritative.sediment.as_ref().unwrap().to_bytes(),
                    world.authoritative.sediment.as_ref().unwrap().to_bytes(),
                    "sediment bytes mismatch after round-trip"
                );
            }
            other => panic!("expected LoadedWorld::Minimal, got {other:?}"),
        }
    }

    // Sprint 3 DD6: TopographyStage populates derived.volcanic_centers with
    // one entry per sampled volcanic center, each in normalized [0, 1]² space.
    #[test]
    fn volcanic_centers_populated_after_topography_run() {
        let preset = preset_single();
        let expected_count = preset.volcanic_center_count as usize;
        let world = run_stage(42, preset, 64);
        let centers = world
            .derived
            .volcanic_centers
            .as_ref()
            .expect("derived.volcanic_centers must be Some after TopographyStage");
        assert_eq!(
            centers.len(),
            expected_count.max(1),
            "volcanic_centers length must match preset.volcanic_center_count (min 1)"
        );
        for (i, [cx, cy]) in centers.iter().enumerate() {
            assert!(
                (0.0..=1.0).contains(cx),
                "volcanic_centers[{i}].x = {cx} must be in [0, 1]"
            );
            assert!(
                (0.0..=1.0).contains(cy),
                "volcanic_centers[{i}].y = {cy} must be in [0, 1]"
            );
        }
    }

    // Sprint 3 DD6: volcanic_centers is deterministic (same seed → same centers).
    #[test]
    fn volcanic_centers_are_deterministic_per_seed() {
        let w1 = run_stage(99, preset_single(), 32);
        let w2 = run_stage(99, preset_single(), 32);
        assert_eq!(
            w1.derived.volcanic_centers, w2.derived.volcanic_centers,
            "volcanic_centers must be bit-exact for same seed + preset"
        );
    }

    // 8. Caldera preset: stage completes and height is in range.
    #[test]
    fn caldera_preset_completes() {
        let world = run_stage(11, preset_caldera(), 32);
        let stats = world
            .authoritative
            .height
            .as_ref()
            .unwrap()
            .stats()
            .unwrap();
        assert!(stats.min >= 0.0);
        assert!(stats.max <= 1.0);
    }
}
