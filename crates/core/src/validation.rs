//! Post-pipeline correctness invariants for the Sprint 1A `WorldState`.
//!
//! Each function checks one invariant and returns `Ok(())` on success or a
//! descriptive [`ValidationError`] variant on the first violation found.
//! None of these functions panic — a missing precondition field returns
//! `Err(MissingPrecondition)` instead.

use crate::neighborhood::{neighbour_offsets, Neighborhood};
use crate::world::{D8_OFFSETS, FLOW_DIR_SINK, WorldState};

// ─── error type ───────────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum ValidationError {
    #[error("river termination: cell ({x}, {y}) in river_mask cannot reach a coast cell along flow_dir")]
    RiverDoesNotTerminate { x: u32, y: u32 },

    #[error("river termination: river_mask contains cell ({x}, {y}) that is sea")]
    RiverInSea { x: u32, y: u32 },

    #[error("flow_dir forms a cycle containing ({x}, {y})")]
    FlowDirCycle { x: u32, y: u32 },

    #[error("accumulation monotone: cell ({x}, {y}) has A = {a_p} but downstream has A = {a_q}")]
    AccumulationNotMonotone { x: u32, y: u32, a_p: f32, a_q: f32 },

    #[error("coastline: cell ({x}, {y}) with z={z} below sea_level={sea_level} is not marked sea")]
    CoastlineBelowSeaLevelNotSea { x: u32, y: u32, z: f32, sea_level: f32 },

    #[error("coastline: cell ({x}, {y}) is coast but has no sea neighbour")]
    CoastlineCoastWithoutSeaNeighbour { x: u32, y: u32 },

    #[error("validation: missing precondition field '{field}' (stage must have run first)")]
    MissingPrecondition { field: &'static str },
}

// ─── public validators ────────────────────────────────────────────────────────

/// Every river cell must be able to reach a coast or sea cell along `flow_dir`.
pub fn river_termination(world: &WorldState) -> Result<(), ValidationError> {
    let river_mask = world
        .derived
        .river_mask
        .as_ref()
        .ok_or(ValidationError::MissingPrecondition { field: "derived.river_mask" })?;

    let coast_mask = world
        .derived
        .coast_mask
        .as_ref()
        .ok_or(ValidationError::MissingPrecondition { field: "derived.coast_mask" })?;

    let flow_dir = world
        .derived
        .flow_dir
        .as_ref()
        .ok_or(ValidationError::MissingPrecondition { field: "derived.flow_dir" })?;

    let w = river_mask.width as usize;
    let h = river_mask.height as usize;
    let max_steps = w * h;

    for y in 0..h {
        for x in 0..w {
            if river_mask.get(x as u32, y as u32) == 0 {
                continue;
            }

            let (ox, oy) = (x as u32, y as u32);

            // River cells must be on land, not sea.
            if coast_mask.is_sea.get(ox, oy) == 1 {
                return Err(ValidationError::RiverInSea { x: ox, y: oy });
            }

            // Walk along flow_dir until we reach a water body or exhaust steps.
            let (mut cx, mut cy) = (x as i32, y as i32);
            let mut ok = false;

            for _ in 0..=max_steps {
                let (cxu, cyu) = (cx as u32, cy as u32);

                if coast_mask.is_coast.get(cxu, cyu) == 1
                    || coast_mask.is_sea.get(cxu, cyu) == 1
                {
                    ok = true;
                    break;
                }

                let dir = flow_dir.get(cxu, cyu);
                if dir == FLOW_DIR_SINK {
                    // Non-coast, non-sea sink — closed basin.
                    break;
                }

                let (dx, dy) = D8_OFFSETS[dir as usize];
                let nx = cx + dx;
                let ny = cy + dy;
                if nx < 0 || nx >= w as i32 || ny < 0 || ny >= h as i32 {
                    // Flowed off-grid — treat as terminated at boundary.
                    ok = true;
                    break;
                }
                cx = nx;
                cy = ny;
            }

            if !ok {
                return Err(ValidationError::RiverDoesNotTerminate { x: ox, y: oy });
            }
        }
    }

    Ok(())
}

/// `flow_dir` forms a DAG (no cycles). Cycle detection via topological sort.
pub fn basin_partition_dag(world: &WorldState) -> Result<(), ValidationError> {
    let flow_dir = world
        .derived
        .flow_dir
        .as_ref()
        .ok_or(ValidationError::MissingPrecondition { field: "derived.flow_dir" })?;

    let w = flow_dir.width as usize;
    let h = flow_dir.height as usize;
    let n = w * h;

    // Build in-degree table.
    let mut indeg: Vec<u32> = vec![0; n];

    for y in 0..h {
        for x in 0..w {
            let dir = flow_dir.get(x as u32, y as u32);
            if dir == FLOW_DIR_SINK {
                continue;
            }
            let (dx, dy) = D8_OFFSETS[dir as usize];
            let qx = x as i32 + dx;
            let qy = y as i32 + dy;
            if qx >= 0 && qx < w as i32 && qy >= 0 && qy < h as i32 {
                indeg[qy as usize * w + qx as usize] += 1;
            }
        }
    }

    // Kahn's BFS: visit all indeg=0 cells.
    let mut queue: std::collections::VecDeque<u32> =
        (0..n as u32).filter(|&p| indeg[p as usize] == 0).collect();
    let mut visited: u32 = 0;

    while let Some(p) = queue.pop_front() {
        visited += 1;
        let x = p as usize % w;
        let y = p as usize / w;
        let dir = flow_dir.get(x as u32, y as u32);
        if dir == FLOW_DIR_SINK {
            continue;
        }
        let (dx, dy) = D8_OFFSETS[dir as usize];
        let qx = x as i32 + dx;
        let qy = y as i32 + dy;
        if qx < 0 || qx >= w as i32 || qy < 0 || qy >= h as i32 {
            continue;
        }
        let q = qy as usize * w + qx as usize;
        indeg[q] -= 1;
        if indeg[q] == 0 {
            queue.push_back(q as u32);
        }
    }

    if visited < n as u32 {
        // Find the first unvisited cell (residual indeg > 0) to report.
        for (p, &deg) in indeg.iter().enumerate() {
            if deg > 0 {
                let x = (p % w) as u32;
                let y = (p / w) as u32;
                return Err(ValidationError::FlowDirCycle { x, y });
            }
        }
    }

    Ok(())
}

/// `A[down(p)] >= A[p]` for every non-sink cell p.
pub fn accumulation_monotone(world: &WorldState) -> Result<(), ValidationError> {
    let accumulation = world
        .derived
        .accumulation
        .as_ref()
        .ok_or(ValidationError::MissingPrecondition { field: "derived.accumulation" })?;

    let flow_dir = world
        .derived
        .flow_dir
        .as_ref()
        .ok_or(ValidationError::MissingPrecondition { field: "derived.flow_dir" })?;

    let w = flow_dir.width as usize;
    let h = flow_dir.height as usize;
    const EPS: f32 = 1e-5;

    for y in 0..h {
        for x in 0..w {
            let dir = flow_dir.get(x as u32, y as u32);
            if dir == FLOW_DIR_SINK {
                continue;
            }
            let (dx, dy) = D8_OFFSETS[dir as usize];
            let qx = x as i32 + dx;
            let qy = y as i32 + dy;
            if qx < 0 || qx >= w as i32 || qy < 0 || qy >= h as i32 {
                continue;
            }
            let a_p = accumulation.get(x as u32, y as u32);
            let a_q = accumulation.get(qx as u32, qy as u32);
            if a_q < a_p - EPS {
                return Err(ValidationError::AccumulationNotMonotone {
                    x: x as u32,
                    y: y as u32,
                    a_p,
                    a_q,
                });
            }
        }
    }

    Ok(())
}

/// Two sub-checks: z < sea_level → is_sea; is_coast → has at least one Von4 sea neighbour.
pub fn coastline_consistency(world: &WorldState) -> Result<(), ValidationError> {
    let height = world
        .authoritative
        .height
        .as_ref()
        .ok_or(ValidationError::MissingPrecondition { field: "authoritative.height" })?;

    let coast_mask = world
        .derived
        .coast_mask
        .as_ref()
        .ok_or(ValidationError::MissingPrecondition { field: "derived.coast_mask" })?;

    let sea_level = world.preset.sea_level;
    let w = height.width as usize;
    let h = height.height as usize;

    for y in 0..h {
        for x in 0..w {
            let (xu, yu) = (x as u32, y as u32);
            let z = height.get(xu, yu);

            // Sub-check 1: z < sea_level must be is_sea.
            if z < sea_level && coast_mask.is_sea.get(xu, yu) == 0 {
                return Err(ValidationError::CoastlineBelowSeaLevelNotSea {
                    x: xu,
                    y: yu,
                    z,
                    sea_level,
                });
            }

            // Sub-check 2: coast cell must have at least one Von4 sea neighbour.
            if coast_mask.is_coast.get(xu, yu) == 1 {
                let has_sea_nbr = neighbour_offsets(Neighborhood::Von4).iter().any(|&(dx, dy)| {
                    let nx = x as i32 + dx;
                    let ny = y as i32 + dy;
                    nx >= 0
                        && nx < w as i32
                        && ny >= 0
                        && ny < h as i32
                        && coast_mask.is_sea.get(nx as u32, ny as u32) == 1
                });
                if !has_sea_nbr {
                    return Err(ValidationError::CoastlineCoastWithoutSeaNeighbour {
                        x: xu,
                        y: yu,
                    });
                }
            }
        }
    }

    Ok(())
}

// ─── tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::field::{MaskField2D, ScalarField2D};
    use crate::preset::IslandAge;
    use crate::preset::IslandArchetypePreset;
    use crate::seed::Seed;
    use crate::world::{CoastMask, Resolution, WorldState};

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
        CoastMask { is_land: land, is_sea: sea, is_coast: coast, land_cell_count, river_mouth_mask: None }
    }

    // ── 1: river_termination happy path ──────────────────────────────────────
    //
    // 3x3 grid:
    //   (0,0)=land  (1,0)=land  (2,0)=sea
    //   (0,1)=land  (1,1)=land  (2,1)=coast
    //   (0,2)=land  (1,2)=land  (2,2)=sea
    //
    // flow_dir: (0,0)->E(0) (1,0)->SE(7 but clamp to S=6)
    //   Actually: (0,0) E→(1,0), (1,0) E→(2,0)[sea, valid terminus], etc.
    // Let's keep it simple: river cell (0,0) flows E→(1,0) flows E→coast(2,1)?
    // No — let's just do a linear 3-cell chain: (0,1) -> (1,1) -> (2,1)=coast.
    // river_mask: only (0,1) is river.
    // flow_dir: (0,1)->E(0) (1,1)->E(0) (2,1)->SINK.
    // coast: (2,1)=coast, (2,0)=sea, (2,2)=sea.
    #[test]
    fn river_termination_happy_path() {
        let w = 3_u32;
        let h = 3_u32;
        let n = (w * h) as usize;
        let idx = |x: u32, y: u32| (y * w + x) as usize;

        let mut is_land = vec![1u8; n];
        let mut is_sea = vec![0u8; n];
        let mut is_coast = vec![0u8; n];

        is_land[idx(2, 0)] = 0; is_sea[idx(2, 0)] = 1;
        is_land[idx(2, 2)] = 0; is_sea[idx(2, 2)] = 1;
        is_land[idx(2, 1)] = 1; is_coast[idx(2, 1)] = 1;

        let mut flow_dir = ScalarField2D::<u8>::new(w, h);
        for y in 0..h {
            for x in 0..w {
                flow_dir.set(x, y, FLOW_DIR_SINK);
            }
        }
        flow_dir.set(0, 1, 0); // E
        flow_dir.set(1, 1, 0); // E → (2,1)=coast

        let mut river_mask = MaskField2D::new(w, h);
        river_mask.set(0, 1, 1);

        let mut world = WorldState::new(Seed(0), test_preset(), Resolution::new(w, h));
        world.derived.coast_mask = Some(make_coast_mask(w, h, is_land, is_sea, is_coast));
        world.derived.flow_dir = Some(flow_dir);
        world.derived.river_mask = Some(river_mask);

        assert!(river_termination(&world).is_ok());
    }

    // ── 2: river_termination detects disconnected river ───────────────────────
    //
    // All-land 3x3; no coast, no sea. River cell at (1,1) with FLOW_DIR_SINK.
    // Must return RiverDoesNotTerminate.
    #[test]
    fn river_termination_detects_disconnected_river() {
        let w = 3_u32;
        let h = 3_u32;
        let n = (w * h) as usize;

        let is_land = vec![1u8; n];
        let is_sea = vec![0u8; n];
        let is_coast = vec![0u8; n];

        let mut flow_dir = ScalarField2D::<u8>::new(w, h);
        for y in 0..h {
            for x in 0..w {
                flow_dir.set(x, y, FLOW_DIR_SINK);
            }
        }

        let mut river_mask = MaskField2D::new(w, h);
        river_mask.set(1, 1, 1);

        let mut world = WorldState::new(Seed(0), test_preset(), Resolution::new(w, h));
        world.derived.coast_mask = Some(make_coast_mask(w, h, is_land, is_sea, is_coast));
        world.derived.flow_dir = Some(flow_dir);
        world.derived.river_mask = Some(river_mask);

        let err = river_termination(&world).unwrap_err();
        assert!(
            matches!(err, ValidationError::RiverDoesNotTerminate { x: 1, y: 1 }),
            "expected RiverDoesNotTerminate at (1,1), got: {err}"
        );
    }

    // ── 3: basin_partition_dag passes on acyclic flow ──────────────────────────
    //
    // Linear chain: (0,0)->E->(1,0)->E->(2,0)->SINK.
    #[test]
    fn basin_partition_dag_passes_on_acyclic_flow() {
        let w = 3_u32;
        let h = 1_u32;
        let mut flow_dir = ScalarField2D::<u8>::new(w, h);
        flow_dir.set(0, 0, 0); // E
        flow_dir.set(1, 0, 0); // E
        flow_dir.set(2, 0, FLOW_DIR_SINK);

        let mut world = WorldState::new(Seed(0), test_preset(), Resolution::new(w, h));
        world.derived.flow_dir = Some(flow_dir);

        assert!(basin_partition_dag(&world).is_ok());
    }

    // ── 4: basin_partition_dag detects cycle ───────────────────────────────────
    //
    // 2-cell cycle: (0,0)->E->(1,0)->W->(0,0). Both have indeg 1 → cycle.
    #[test]
    fn basin_partition_dag_detects_cycle() {
        let w = 2_u32;
        let h = 1_u32;
        let mut flow_dir = ScalarField2D::<u8>::new(w, h);
        flow_dir.set(0, 0, 0); // E → (1,0)
        flow_dir.set(1, 0, 4); // W → (0,0)

        let mut world = WorldState::new(Seed(0), test_preset(), Resolution::new(w, h));
        world.derived.flow_dir = Some(flow_dir);

        let err = basin_partition_dag(&world).unwrap_err();
        assert!(
            matches!(err, ValidationError::FlowDirCycle { .. }),
            "expected FlowDirCycle, got: {err}"
        );
    }

    // ── 5: accumulation_monotone happy path ───────────────────────────────────
    //
    // (0,0) A=1 -> E -> (1,0) A=2 -> E -> (2,0) A=3 -> SINK.
    #[test]
    fn accumulation_monotone_happy_path() {
        let w = 3_u32;
        let h = 1_u32;
        let mut flow_dir = ScalarField2D::<u8>::new(w, h);
        flow_dir.set(0, 0, 0);
        flow_dir.set(1, 0, 0);
        flow_dir.set(2, 0, FLOW_DIR_SINK);

        let mut accum = ScalarField2D::<f32>::new(w, h);
        accum.set(0, 0, 1.0);
        accum.set(1, 0, 2.0);
        accum.set(2, 0, 3.0);

        let mut world = WorldState::new(Seed(0), test_preset(), Resolution::new(w, h));
        world.derived.flow_dir = Some(flow_dir);
        world.derived.accumulation = Some(accum);

        assert!(accumulation_monotone(&world).is_ok());
    }

    // ── 6: accumulation_monotone detects violation ────────────────────────────
    //
    // (0,0) A=5 -> E -> (1,0) A=1 — downstream is less.
    #[test]
    fn accumulation_monotone_detects_violation() {
        let w = 2_u32;
        let h = 1_u32;
        let mut flow_dir = ScalarField2D::<u8>::new(w, h);
        flow_dir.set(0, 0, 0); // E
        flow_dir.set(1, 0, FLOW_DIR_SINK);

        let mut accum = ScalarField2D::<f32>::new(w, h);
        accum.set(0, 0, 5.0);
        accum.set(1, 0, 1.0);

        let mut world = WorldState::new(Seed(0), test_preset(), Resolution::new(w, h));
        world.derived.flow_dir = Some(flow_dir);
        world.derived.accumulation = Some(accum);

        let err = accumulation_monotone(&world).unwrap_err();
        assert!(
            matches!(err, ValidationError::AccumulationNotMonotone { .. }),
            "expected AccumulationNotMonotone, got: {err}"
        );
    }

    // ── 7: coastline_consistency happy path ───────────────────────────────────
    //
    // 3x1: (0,0)=sea z=0.1, (1,0)=coast z=0.4, (2,0)=land z=0.8.
    // sea_level=0.3: z=0.1 < 0.3 → sea ✓; z=0.4 >= 0.3 → land/coast ✓.
    // (1,0) is coast with Von4 W=(0,0)=sea → ok.
    #[test]
    fn coastline_consistency_happy_path() {
        let w = 3_u32;
        let h = 1_u32;
        let n = (w * h) as usize;

        let mut is_land = vec![0u8; n];
        let mut is_sea = vec![0u8; n];
        let mut is_coast = vec![0u8; n];
        is_sea[0] = 1;
        is_land[1] = 1; is_coast[1] = 1;
        is_land[2] = 1;

        let mut height = ScalarField2D::<f32>::new(w, h);
        height.set(0, 0, 0.1);
        height.set(1, 0, 0.4);
        height.set(2, 0, 0.8);

        let mut world = WorldState::new(Seed(0), test_preset(), Resolution::new(w, h));
        world.authoritative.height = Some(height);
        world.derived.coast_mask = Some(make_coast_mask(w, h, is_land, is_sea, is_coast));

        assert!(coastline_consistency(&world).is_ok());
    }

    // ── 8: coastline_consistency detects below-sea-level not marked sea ────────
    //
    // 1x1: z=0.1 < sea_level=0.3 but is_sea=0. Must fail.
    #[test]
    fn coastline_consistency_detects_below_sea_level_as_land() {
        let w = 1_u32;
        let h = 1_u32;

        let is_land = vec![1u8]; // wrongly marked land
        let is_sea = vec![0u8];
        let is_coast = vec![0u8];

        let mut height = ScalarField2D::<f32>::new(w, h);
        height.set(0, 0, 0.1); // below sea_level=0.3

        let mut world = WorldState::new(Seed(0), test_preset(), Resolution::new(w, h));
        world.authoritative.height = Some(height);
        world.derived.coast_mask = Some(make_coast_mask(w, h, is_land, is_sea, is_coast));

        let err = coastline_consistency(&world).unwrap_err();
        assert!(
            matches!(err, ValidationError::CoastlineBelowSeaLevelNotSea { .. }),
            "expected CoastlineBelowSeaLevelNotSea, got: {err}"
        );
    }

    // ── 9: coastline_consistency detects coast without sea neighbour ──────────
    //
    // 3x1: all land, middle marked coast. No sea anywhere → coast has no sea nbr.
    #[test]
    fn coastline_consistency_detects_coast_without_sea_neighbour() {
        let w = 3_u32;
        let h = 1_u32;
        let n = (w * h) as usize;

        let is_land = vec![1u8; n];
        let is_sea = vec![0u8; n];
        let mut is_coast = vec![0u8; n];
        is_coast[1] = 1; // (1,0) marked coast but no sea neighbours

        // Heights all above sea_level so sub-check-1 passes.
        let mut height = ScalarField2D::<f32>::new(w, h);
        height.set(0, 0, 0.5);
        height.set(1, 0, 0.5);
        height.set(2, 0, 0.5);

        let mut world = WorldState::new(Seed(0), test_preset(), Resolution::new(w, h));
        world.authoritative.height = Some(height);
        world.derived.coast_mask = Some(make_coast_mask(w, h, is_land, is_sea, is_coast));

        let err = coastline_consistency(&world).unwrap_err();
        assert!(
            matches!(err, ValidationError::CoastlineCoastWithoutSeaNeighbour { x: 1, y: 0 }),
            "expected CoastlineCoastWithoutSeaNeighbour at (1,0), got: {err}"
        );
    }

    // ── 10: missing precondition returns Err ───────────────────────────────────
    //
    // Fresh empty world has no derived fields. All four validators must fail
    // with MissingPrecondition.
    #[test]
    fn missing_precondition_returns_err() {
        let world = WorldState::new(Seed(0), test_preset(), Resolution::new(4, 4));

        assert!(matches!(
            river_termination(&world),
            Err(ValidationError::MissingPrecondition { .. })
        ));
        assert!(matches!(
            basin_partition_dag(&world),
            Err(ValidationError::MissingPrecondition { .. })
        ));
        assert!(matches!(
            accumulation_monotone(&world),
            Err(ValidationError::MissingPrecondition { .. })
        ));
        assert!(matches!(
            coastline_consistency(&world),
            Err(ValidationError::MissingPrecondition { .. })
        ));
    }

    // ── bonus: river cell marked sea returns RiverInSea ───────────────────────
    #[test]
    fn river_termination_detects_river_in_sea() {
        let w = 2_u32;
        let h = 1_u32;
        let is_land = vec![0u8, 1u8];
        let is_sea = vec![1u8, 0u8];
        let is_coast = vec![0u8, 0u8];

        let flow_dir = ScalarField2D::<u8>::new(w, h); // all FLOW_DIR_SINK=0, but we never read

        let mut river_mask = MaskField2D::new(w, h);
        river_mask.set(0, 0, 1); // river cell in sea

        let mut world = WorldState::new(Seed(0), test_preset(), Resolution::new(w, h));
        world.derived.coast_mask = Some(make_coast_mask(w, h, is_land, is_sea, is_coast));
        world.derived.flow_dir = Some(flow_dir);
        world.derived.river_mask = Some(river_mask);

        let err = river_termination(&world).unwrap_err();
        assert!(
            matches!(err, ValidationError::RiverInSea { x: 0, y: 0 }),
            "expected RiverInSea at (0,0), got: {err}"
        );
    }
}
