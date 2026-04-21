//! SPACE-lite sediment support — Sprint 3 DD2 + DD3.
//!
//! This module owns:
//!
//! * Locked SPACE-lite constants ([`SPACE_K_BED_DEFAULT`],
//!   [`SPACE_K_SED_DEFAULT`], [`H_STAR`], [`HS_ENTRAIN_MAX`]).
//! * Locked Qs-routing / transport-capacity constants ([`K_Q_DEFAULT`],
//!   [`M_Q`], [`N_Q`]).
//! * [`SedimentUpdateStage`] — runs the full DD3 topo-order Qs_in / Qs_out
//!   routing loop: accumulates upstream sediment flux, computes per-cell
//!   transport capacity, deposits the excess into `authoritative.sediment`,
//!   and writes `derived.deposition_flux[p]` as a by-product.
//! * [`DepositionStage`] — diagnostic finalization hook that runs
//!   immediately after `SedimentUpdateStage` inside the `ErosionOuterLoop`
//!   inner step. DD3's pseudo-code interleaves Qs routing and deposition
//!   in a single topo-order pass (splitting the two into two `run` calls
//!   would require either a double traversal or a cached order field);
//!   `DepositionStage` is kept as a separately-named stage so the
//!   `erosion_inner_step_canonical_order` lock test covers it, and exists
//!   as a natural hook for Task 3.7 overlay validation or future Sprint 3+
//!   diagnostic passes.
//!
//! ## Stage-responsibility split
//!
//! DD3's pseudo-code interleaves `Qs_cap` computation, deposition (`D → hs`),
//! entrainment recompute, and `Qs_out` propagation inside a single
//! topological-order loop. Splitting this across two independent stages
//! would require either (a) traversing the D8 DAG twice per inner step,
//! which is measurably wasteful at 256², or (b) caching the topo order
//! between the two stages in a dedicated `derived.*` field — a field the
//! Task 3.3 scope explicitly forbids adding beyond `deposition_flux`.
//!
//! The pragmatic split implemented here:
//!
//! * `SedimentUpdateStage::run` — the full topo-order math loop.
//!   Computes `Qs_in`, `Qs_cap`, `D`, applies `hs += D·dt`, propagates
//!   `Qs_out` to each cell's D8 downstream neighbour. Writes
//!   `derived.deposition_flux[p] = D[p]` for every land cell as a
//!   by-product. Sea cells keep 0.0.
//! * `DepositionStage::run` — diagnostic finalization, identical in shape
//!   to Task 3.2's `SedimentUpdateStage` placeholder: a pure `Ok(())` that
//!   preserves the inner-step slot for future overlay / validation
//!   injection. Does NOT duplicate the topo traversal.
//!
//! The DD3 spec in the sprint doc splits the two sub-stages for authorial
//! clarity; the code keeps them separated by responsibility (math vs
//! finalization) rather than by algorithmic phase, which is the only split
//! that avoids the double-traversal penalty on the critical inner-step
//! path.
//!
//! ## `dt = 1.0` in v1
//!
//! Matches Sprint 2 DD1 / Sprint 3 DD2: every flux term is multiplied by a
//! unit timestep so the pseudo-code equations read cleanly. A non-unit
//! `dt` is a Sprint 3+ calibration decision; this module pins `DT = 1.0`
//! explicitly to keep the dimensionality consistent with SPIM.

use island_core::field::ScalarField2D;
use island_core::pipeline::SimulationStage;
use island_core::world::{FLOW_DIR_SINK, WorldState};

use crate::hydro::D8_OFFSETS;

// ─── locked SPACE-lite constants ─────────────────────────────────────────────

/// SPACE-lite default bedrock erodibility `K_bed` (Sprint 3 DD2).
///
/// Bedrock incision is `E_bed = K_bed · A^m · S^n · exp(-hs / H*)`.
/// Larger `K_bed` ⇒ faster bedrock lowering. v1 dimensionless proxy.
pub const SPACE_K_BED_DEFAULT: f32 = 5.0e-3;

/// SPACE-lite default sediment entrainability `K_sed` (Sprint 3 DD2).
///
/// Sediment entrainment is `E_sed = K_sed · A^m · S^n · min(hs, HS_ENTRAIN_MAX)`.
/// Larger `K_sed` ⇒ faster stripping of the sediment cover. v1 dimensionless
/// proxy.
pub const SPACE_K_SED_DEFAULT: f32 = 1.5e-2;

/// SPACE-lite cover thickness `H*` in the bedrock shielding term
/// `exp(-hs / H*)` (Sprint 3 DD2).
///
/// Controls how quickly bedrock incision decays as sediment thickens.
/// In normalised height units; a value of `0.05` means `exp(-1) ≈ 0.37`
/// shielding at `hs = 0.05`.
pub const H_STAR: f32 = 0.05;

/// Upper clamp applied to `hs` when computing sediment entrainment `E_sed`.
/// Prevents `E_sed` from growing unboundedly with thick sediment piles —
/// the physical picture is that once the sediment column is thicker than
/// the flow can reach, extra thickness contributes no more entrainment.
pub const HS_ENTRAIN_MAX: f32 = 0.5;

// ─── locked Qs-routing constants ─────────────────────────────────────────────

/// Sprint 3 DD3 default transport-capacity coefficient `K_Q` for
/// `Qs_cap = K_Q · A^m_q · S^n_q`.
///
/// Sized so that under the locked `K_bed = 5e-3` / `K_sed = 1.5e-2` SPIM
/// fluxes, valley-floor deposition pulses are visible without pushing
/// `hs` to the 1.0 clamp on a single inner step. Tunable via the Sprint
/// 3.8 ParamsPanel slider; if the preset field is introduced later the
/// slider reads from there instead of this constant.
pub const K_Q_DEFAULT: f32 = 2.0e-2;

/// Transport-capacity exponent on accumulation. `A^m_q` term of
/// `Qs_cap`. `m_q = 1.0` matches common fluvial-sediment literature
/// formulations (linear in discharge proxy).
pub const M_Q: f32 = 1.0;

/// Transport-capacity exponent on slope. `S^n_q` term of `Qs_cap`.
///
/// `n_q = 1.5 > n_SPIM = 1.0` is a deliberate design choice: it makes
/// capacity drop faster than entrainment on valley-floor flats (low
/// slope), so `Qs_in - Qs_cap` goes positive and deposition accumulates
/// naturally in depositional reaches. A coincident `n_q = n_SPIM = 1.0`
/// would produce depositional behaviour only through the `A^m_q - A^m`
/// differential, which is much weaker visually.
pub const N_Q: f32 = 1.5;

/// Sprint 3 v1 timestep `dt`. Matches SPIM's unit-timestep convention.
const DT: f32 = 1.0;

// ─── SedimentUpdateStage ─────────────────────────────────────────────────────

/// Sprint 3 DD3: sediment transport routing + deposition math.
///
/// For each cell `p` in D8 topological order (upstream → downstream):
///
/// ```text
/// Qs_cap[p] = K_Q · A_p^m_q · S_p^n_q
/// D[p]      = max(0, Qs_in[p] - Qs_cap[p])
/// hs[p]    += D[p] · dt                            (clamped to [0, 1])
/// E_sed_p   = K_sed · A_p^m · S_p^n · min(hs[p], HS_ENTRAIN_MAX)
/// Qs_out[p] = (Qs_in[p] - D[p]) + E_sed_p · dt
/// Qs_in[downstream(p)] += Qs_out[p]
/// ```
///
/// Writes `derived.deposition_flux[p] = D[p]` for every land cell. Sea
/// cells keep the default `0.0` (they are sinks; Qs dropped into them is
/// simply discarded in v1 — offshore dispersal is a Sprint 4+ problem).
///
/// # Prerequisites
///
/// * `world.derived.flow_dir` — D8 direction codes.
/// * `world.derived.accumulation` — `A_p` term.
/// * `world.derived.slope` — `S_p` term.
/// * `world.derived.coast_mask` — `is_land` gate.
/// * `world.authoritative.sediment` — `hs[p]` target for in-place updates.
///
/// All are populated by earlier stages in the `ErosionOuterLoop` wrapper;
/// any missing field bails with a descriptive error.
///
/// # Naming contract
///
/// The stage `name()` is `"sediment_update"` and is consumer-visible: the
/// `erosion_inner_step_canonical_order` test pins it, and future overlay
/// / validation code may key off it. Do not rename without a co-ordinated
/// update across every consumer.
pub struct SedimentUpdateStage;

impl SimulationStage for SedimentUpdateStage {
    fn name(&self) -> &'static str {
        "sediment_update"
    }

    fn run(&self, world: &mut WorldState) -> anyhow::Result<()> {
        // ── prerequisite checks ───────────────────────────────────────────────
        if world.derived.flow_dir.is_none() {
            anyhow::bail!(
                "SedimentUpdateStage prerequisite missing: \
                 derived.flow_dir (FlowRoutingStage must run first)"
            );
        }
        if world.derived.accumulation.is_none() {
            anyhow::bail!(
                "SedimentUpdateStage prerequisite missing: \
                 derived.accumulation (AccumulationStage must run first)"
            );
        }
        if world.derived.slope.is_none() {
            anyhow::bail!(
                "SedimentUpdateStage prerequisite missing: \
                 derived.slope (DerivedGeomorphStage must run first)"
            );
        }
        if world.derived.coast_mask.is_none() {
            anyhow::bail!(
                "SedimentUpdateStage prerequisite missing: \
                 derived.coast_mask (CoastMaskStage must run first)"
            );
        }
        if world.authoritative.sediment.is_none() {
            anyhow::bail!(
                "SedimentUpdateStage: authoritative.sediment is None \
                 (CoastMaskStage must run first — Task 3.1 sets hs_init)"
            );
        }

        // ── read parameters ───────────────────────────────────────────────────
        // DD3 constants are locked module-level — future ParamsPanel sliders
        // (Task 3.8) may plumb preset fields through here.
        let k_q = K_Q_DEFAULT;
        let m_q = M_Q;
        let n_q = N_Q;
        let k_sed = world.preset.erosion.space_k_sed;
        let m_spim = world.preset.erosion.spim_m;
        let n_spim = world.preset.erosion.spim_n;

        let w = world.resolution.sim_width as usize;
        let h = world.resolution.sim_height as usize;
        let n = w * h;

        // ── borrow split: immutable derived + mutable sediment ────────────────
        let flow_dir = &world.derived.flow_dir.as_ref().unwrap().data;
        let accumulation = &world.derived.accumulation.as_ref().unwrap().data;
        let slope = &world.derived.slope.as_ref().unwrap().data;
        let is_land = &world.derived.coast_mask.as_ref().unwrap().is_land.data;

        // ── topological order via in-degree BFS ───────────────────────────────
        // Mirrors AccumulationStage's Kahn-style topo sort. We recompute it
        // fresh every inner step (O(N) per call) rather than caching in a
        // new derived field: per the Task 3.3 scope, only `deposition_flux`
        // may be added to `DerivedCaches`. At the canonical 256² × 10×10
        // inner loop that is ~6.5M extra ops per `run_from(ErosionOuterLoop)`,
        // which is measurable but well within budget.
        let mut indeg: Vec<u32> = vec![0; n];
        for (p, &dir) in flow_dir.iter().enumerate() {
            if let Some(q) = downstream_index(p, dir, w, h) {
                indeg[q] += 1;
            }
        }
        let mut topo: Vec<u32> = Vec::with_capacity(n);
        let mut head = 0_usize;
        for (p, &d) in indeg.iter().enumerate() {
            if d == 0 {
                topo.push(p as u32);
            }
        }
        while head < topo.len() {
            let p = topo[head] as usize;
            head += 1;
            let dir = flow_dir[p];
            if let Some(q) = downstream_index(p, dir, w, h) {
                indeg[q] -= 1;
                if indeg[q] == 0 {
                    topo.push(q as u32);
                }
            }
        }
        // `topo` now holds every cell in upstream-first order. If the D8
        // DAG is ever malformed (a cycle), Kahn's algorithm would leave
        // indeg[p] > 0 for cells in the cycle and they'd be missing from
        // `topo` — we bail defensively to turn that into a visible error
        // rather than silently skip a chunk of the grid.
        if topo.len() != n {
            anyhow::bail!(
                "SedimentUpdateStage: D8 flow_dir contains a cycle \
                 (topo sort covered {}/{} cells) — FlowRoutingStage / \
                 PitFillStage contract violated",
                topo.len(),
                n
            );
        }

        // ── Qs routing + deposition sweep ─────────────────────────────────────
        let mut qs_in: Vec<f32> = vec![0.0; n];
        let mut deposition: Vec<f32> = vec![0.0; n];

        // Reborrow sediment as &mut after the immutable derived borrows above
        // go out of scope by being stored as slice references; the compiler
        // needs us to take this split explicitly.
        let hs_field = world.authoritative.sediment.as_mut().unwrap();

        for &p_u32 in &topo {
            let p = p_u32 as usize;

            // Sea cells act as pure Qs absorbers in v1: any inbound flux is
            // dropped (no offshore dispersal, no deposition). They don't
            // propagate downstream either — flow_dir[p] is FLOW_DIR_SINK on
            // them by FlowRoutingStage contract.
            if is_land[p] == 0 {
                qs_in[p] = 0.0;
                continue;
            }

            let a = accumulation[p];
            let s = slope[p];
            let hs = hs_field.data[p];

            // Transport capacity. `finite_nonneg` matches the SPIM
            // kernel's defensive posture: if a pathological parameter
            // override produces NaN/Inf, clamp to 0 and continue.
            let qs_cap = finite_nonneg(k_q * a.powf(m_q) * s.powf(n_q));

            // Deposition: only the excess of inbound flux over capacity.
            let d = (qs_in[p] - qs_cap).max(0.0);
            deposition[p] = d;

            // Add deposition to the sediment column (in-place). The clamp
            // preserves the DD2 `[0, 1]` invariant — matches SPIM.
            let hs_after_dep = (hs + d * DT).clamp(0.0, 1.0);
            hs_field.data[p] = hs_after_dep;

            // Entrainment recompute on the post-deposition hs. DD3's
            // pseudo-code reads `hs_eff = min(hs, HS_ENTRAIN_MAX)`; we
            // follow that literally.
            let hs_eff = hs_after_dep.min(HS_ENTRAIN_MAX);
            let e_sed = finite_nonneg(k_sed * a.powf(m_spim) * s.powf(n_spim) * hs_eff);

            // Downstream flux: leftover in-flux (minus what we deposited)
            // plus newly entrained sediment. DD3 equation. The final
            // `max(0.0)` is a numerical floor against fp noise —
            // `qs_in - d` is non-negative by construction since
            // `d = max(0, qs_in - qs_cap) ≤ qs_in` whenever `qs_in ≥ 0`.
            let qs_out = ((qs_in[p] - d) + e_sed * DT).max(0.0);

            // Propagate to downstream neighbour (if any).
            let dir = flow_dir[p];
            if let Some(q) = downstream_index(p, dir, w, h) {
                qs_in[q] += qs_out;
            }
            // Sinks (FLOW_DIR_SINK or OOB) swallow qs_out in v1 — matches
            // the "sea cells drop inbound flux" policy.
        }

        // ── write derived.deposition_flux ─────────────────────────────────────
        // Reuse the existing backing Vec if resolution is unchanged, matching
        // the Task 3.1 `authoritative.sediment` reuse protocol. This keeps
        // slider reruns allocation-free.
        let needs_alloc = match &world.derived.deposition_flux {
            Some(df) => df.width as usize != w || df.height as usize != h,
            None => true,
        };
        if needs_alloc {
            world.derived.deposition_flux = Some(ScalarField2D::<f32>::new(w as u32, h as u32));
        }
        let df = world.derived.deposition_flux.as_mut().unwrap();
        df.data.copy_from_slice(&deposition);

        Ok(())
    }
}

// ─── DepositionStage ─────────────────────────────────────────────────────────

/// Sprint 3 DD3: deposition-diagnostic finalization hook.
///
/// DD3 splits the sediment-update step into two named sub-stages for
/// authorial clarity: "Qs routing" and "deposition". In code those two
/// phases cannot be split across separate topological traversals without
/// either (a) paying for two O(N) sweeps per inner step or (b) caching
/// the topo order in a dedicated `derived.*` field that Task 3.3 scope
/// forbids. The pragmatic split implemented here puts all the math in
/// [`SedimentUpdateStage`] (routing + deposition + `derived.deposition_flux`
/// write) and keeps `DepositionStage::run` as a no-op finalization hook —
/// it exists so the inner-step order
/// `[stream_power_incision, sediment_update, deposition, hillslope_diffusion]`
/// can be locked by `erosion_inner_step_canonical_order` with four
/// independently-named stages.
///
/// Future sprints (Task 3.7 overlay wiring, sediment-aware coast-type v2,
/// lava-delta emplacement) may upgrade this from a no-op to a diagnostic
/// finalization pass (e.g. clamping sub-ε values in `deposition_flux` to
/// zero, computing summary stats). The no-op shape is deliberate: it
/// matches the Task 3.2 `SedimentUpdateStage` pattern and keeps the
/// placeholder body free of debug asserts that would have to be deleted.
///
/// # Naming contract
///
/// `name()` returns `"deposition"`, locked by
/// `erosion_inner_step_canonical_order`.
pub struct DepositionStage;

impl SimulationStage for DepositionStage {
    fn name(&self) -> &'static str {
        "deposition"
    }

    fn run(&self, _world: &mut WorldState) -> anyhow::Result<()> {
        // Task 3.3 finalization is a no-op: all math lives in
        // SedimentUpdateStage. See the struct-level docstring for the
        // responsibility split rationale.
        Ok(())
    }
}

// ─── helpers ─────────────────────────────────────────────────────────────────

/// Clamp non-finite (NaN / Inf) flux values to 0 and floor negatives at 0.
/// Matches the SPIM kernel's defensive posture: a pathological parameter
/// override must degrade gracefully instead of poisoning the Qs sweep.
#[inline]
fn finite_nonneg(x: f32) -> f32 {
    if x.is_finite() { x.max(0.0) } else { 0.0 }
}

/// Resolve the flat-index downstream neighbour of cell `p` given D8
/// direction code `dir` on a `w × h` grid. Returns `None` for sinks
/// (`FLOW_DIR_SINK`) and for cells whose D8 offset would fall outside
/// the grid. Mirrors `AccumulationStage::downstream_cell`.
#[inline]
fn downstream_index(p: usize, dir: u8, w: usize, h: usize) -> Option<usize> {
    if dir == FLOW_DIR_SINK {
        return None;
    }
    debug_assert!(
        (dir as usize) < D8_OFFSETS.len(),
        "flow_dir contains invalid direction {dir}; FlowRoutingStage contract violated"
    );
    let (dx, dy) = D8_OFFSETS[dir as usize];
    let x = (p % w) as i32;
    let y = (p / w) as i32;
    let qx = x + dx;
    let qy = y + dy;
    if qx < 0 || qx >= w as i32 || qy < 0 || qy >= h as i32 {
        return None;
    }
    Some(qy as usize * w + qx as usize)
}

// ─── tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    use island_core::field::{MaskField2D, ScalarField2D};
    use island_core::preset::{ErosionParams, IslandAge, IslandArchetypePreset, SpimVariant};
    use island_core::seed::Seed;
    use island_core::world::{CoastMask, Resolution, WorldState};

    fn trivial_preset() -> IslandArchetypePreset {
        IslandArchetypePreset {
            name: "sediment_update_test".into(),
            island_radius: 0.5,
            max_relief: 0.5,
            volcanic_center_count: 1,
            island_age: IslandAge::Young,
            prevailing_wind_dir: 0.0,
            marine_moisture_strength: 0.5,
            sea_level: 0.3,
            erosion: ErosionParams::default(),
            climate: Default::default(),
        }
    }

    fn space_lite_preset() -> IslandArchetypePreset {
        IslandArchetypePreset {
            erosion: ErosionParams {
                spim_variant: SpimVariant::SpaceLite,
                ..ErosionParams::default()
            },
            ..trivial_preset()
        }
    }

    /// The stage name string is a consumer-visible identifier. Lock it so
    /// a future accidental rename shows up as a test failure.
    #[test]
    fn sediment_update_stage_name_is_stable() {
        assert_eq!(SedimentUpdateStage.name(), "sediment_update");
    }

    /// Task 3.3: the companion stage has the locked name `"deposition"`.
    #[test]
    fn deposition_stage_name_is_stable() {
        assert_eq!(DepositionStage.name(), "deposition");
    }

    /// The locked SPACE-lite constants match the documented DD2 values.
    /// If these drift, the sprint doc / CLAUDE.md must be updated in
    /// lockstep.
    #[test]
    fn space_lite_constants_match_dd2_lock() {
        assert_eq!(SPACE_K_BED_DEFAULT, 5.0e-3);
        assert_eq!(SPACE_K_SED_DEFAULT, 1.5e-2);
        assert_eq!(H_STAR, 0.05);
        assert_eq!(HS_ENTRAIN_MAX, 0.5);
    }

    /// DD3 transport constants: `K_Q = 2e-2`, `m_q = 1.0`, `n_q = 1.5`.
    /// `n_q > n_SPIM` is the design invariant that produces valley-floor
    /// deposition — if someone drops `n_q` to 1.0 the deposition pattern
    /// collapses and this test fires loudly.
    #[test]
    fn qs_routing_constants_match_dd3_lock() {
        assert_eq!(K_Q_DEFAULT, 2.0e-2);
        assert_eq!(M_Q, 1.0);
        assert_eq!(N_Q, 1.5);
        // Sanity: n_q > n_SPIM (locked default = 1.0) so capacity drops
        // faster than entrainment on flats.
        assert!(
            N_Q > 1.0,
            "N_Q must exceed n_SPIM = 1.0 so Qs_cap drops faster than \
             entrainment on valley floors (DD3 design invariant)"
        );
    }

    /// DepositionStage is a no-op finalization hook: `run` must not
    /// mutate any world field. Snapshot a few identities and verify.
    #[test]
    fn deposition_stage_is_finalization_noop() {
        let mut world = WorldState::new(Seed(0), trivial_preset(), Resolution::new(8, 8));
        let height_was_some = world.authoritative.height.is_some();
        let sediment_was_some = world.authoritative.sediment.is_some();
        let df_was_some = world.derived.deposition_flux.is_some();
        DepositionStage
            .run(&mut world)
            .expect("no-op stage must not fail");
        assert_eq!(world.authoritative.height.is_some(), height_was_some);
        assert_eq!(world.authoritative.sediment.is_some(), sediment_was_some);
        assert_eq!(world.derived.deposition_flux.is_some(), df_was_some);
    }

    // ── shared synthetic-world helper ────────────────────────────────────────

    /// Build a synthetic world with all SedimentUpdateStage prerequisites
    /// populated. Caller fills in slope / flow_dir / accumulation / sediment
    /// values per-test.
    fn make_synthetic_world(
        w: u32,
        h: u32,
        preset: IslandArchetypePreset,
        is_land_all: bool,
    ) -> WorldState {
        let mut world = WorldState::new(Seed(0), preset, Resolution::new(w, h));

        // Height: nominal 0.5 everywhere (SedimentUpdateStage doesn't read it
        // directly, but downstream Deposition / hillslope might).
        let mut height = ScalarField2D::<f32>::new(w, h);
        height.data.fill(0.5);
        world.authoritative.height = Some(height);

        // Sediment starts at 0.1 on land, 0.0 on sea — matches Task 3.1.
        let sediment = ScalarField2D::<f32>::new(w, h);
        world.authoritative.sediment = Some(sediment);

        // Slope / accumulation: caller overrides per-test.
        world.derived.slope = Some(ScalarField2D::<f32>::new(w, h));
        world.derived.accumulation = Some(ScalarField2D::<f32>::new(w, h));

        // flow_dir: everyone SINK by default; caller overrides per-test.
        let mut flow_dir = ScalarField2D::<u8>::new(w, h);
        flow_dir.data.fill(FLOW_DIR_SINK);
        world.derived.flow_dir = Some(flow_dir);

        // Coast mask: all land (or custom).
        let mut is_land = MaskField2D::new(w, h);
        if is_land_all {
            is_land.data.fill(1);
        }
        let is_sea = MaskField2D::new(w, h);
        let is_coast = MaskField2D::new(w, h);
        let land_cell_count = if is_land_all { w * h } else { 0 };
        world.derived.coast_mask = Some(CoastMask {
            is_land,
            is_sea,
            is_coast,
            land_cell_count,
            river_mouth_mask: None,
        });

        // Initialise sediment on land cells per the Task 3.1 convention
        // after the coast_mask has been installed.
        {
            let cm_is_land = &world.derived.coast_mask.as_ref().unwrap().is_land.data;
            let sed = world.authoritative.sediment.as_mut().unwrap();
            for (i, v) in sed.data.iter_mut().enumerate() {
                *v = if cm_is_land[i] == 1 { 0.1 } else { 0.0 };
            }
        }

        world
    }

    /// Task 3.3 Test 2: at the very first inner step of a fresh world,
    /// `Qs_in` is 0 everywhere (no upstream flux has been accumulated yet),
    /// so `D[p]` is 0 for every cell. `derived.deposition_flux` must
    /// therefore be all-zero after a single `SedimentUpdateStage::run`.
    ///
    /// This is a tight invariant that falls out of `d = max(0, qs_in - qs_cap)`
    /// combined with `qs_in[source cells] == 0.0`: a source cell can only
    /// produce entrainment, not deposition. Downstream cells receive
    /// `qs_out` and may deposit starting from the second topological step
    /// — BUT the topological order processes them later in the same `run`,
    /// so they CAN deposit within the single run if upstream-derived
    /// `qs_out` exceeds their `qs_cap`. We therefore pick a config where
    /// capacity ≫ entrainment: slope = 0 everywhere → `Qs_cap = 0` but
    /// also `E_sed = 0`, so `qs_out` stays at 0 everywhere and deposition
    /// is 0 everywhere. The stronger invariant is "zero flow anywhere →
    /// zero deposition everywhere", which is what this test captures.
    #[test]
    fn deposition_flux_is_zero_when_no_flow() {
        let (w, h) = (8u32, 8u32);
        let mut world = make_synthetic_world(w, h, space_lite_preset(), true);
        // slope = 0 everywhere → Qs_cap = 0 AND E_sed = 0.
        // flow_dir is all SINK by default → no Qs propagation.
        // accumulation = 1 everywhere is the default (actually 0 — set it).
        world.derived.accumulation.as_mut().unwrap().data.fill(1.0);

        SedimentUpdateStage
            .run(&mut world)
            .expect("sediment update run");

        let df = world
            .derived
            .deposition_flux
            .as_ref()
            .expect("deposition_flux must be Some after run");
        for (i, &d) in df.data.iter().enumerate() {
            assert_eq!(
                d, 0.0,
                "cell {i}: D[p] must be 0 when no flow is routed, got {d}"
            );
        }
    }

    /// Task 3.3 Test 1: alluvial-fan formation.
    ///
    /// Build a synthetic 1×8 world where cells 0..3 are a steep mountain
    /// (slope = 0.3) and cells 4..7 are a flat plain (slope = 0.02). Flow
    /// direction is east (0) on every cell, so flux routes 0 → 1 → … → 7.
    /// Accumulation grows linearly (1, 2, 3, …).
    ///
    /// With `n_q = 1.5 > n_SPIM = 1.0`, `Qs_cap` drops sharply at the
    /// slope transition (cell 3 → cell 4): the plain cells' capacity is
    /// too low to carry the mountain-derived flux, so deposition kicks in
    /// on the slope-transition cells. The assertion: `hs[4] > hs[0]`
    /// (the first plain cell ended up with more sediment than the first
    /// mountain cell, which had no inbound flux).
    ///
    /// This is a qualitative geomorphology check — we just need the
    /// physics to point in the right direction.
    #[test]
    fn deposition_stage_adds_sediment_on_slope_transitions() {
        let (w, h) = (8u32, 1u32);
        // Inflate K_Q so the deposition signal is large vs. fp noise on
        // this tiny synthetic grid.
        let mut world = make_synthetic_world(w, h, space_lite_preset(), true);

        // Slope: mountain (high) for x=0..3, plain (low) for x=4..7.
        {
            let s = world.derived.slope.as_mut().unwrap();
            for x in 0..w {
                s.data[x as usize] = if x < 4 { 0.3 } else { 0.02 };
            }
        }

        // Accumulation: 1, 2, 3, …, 8 (upstream cells have less
        // accumulated flow — linear growth downstream).
        {
            let a = world.derived.accumulation.as_mut().unwrap();
            for x in 0..w {
                a.data[x as usize] = (x + 1) as f32;
            }
        }

        // Flow direction: east (D8 = 0) on cells 0..6, sink on cell 7.
        {
            let fd = world.derived.flow_dir.as_mut().unwrap();
            for x in 0..7 {
                fd.data[x as usize] = 0; // east
            }
            fd.data[7] = FLOW_DIR_SINK;
        }

        let hs_before: Vec<f32> = world.authoritative.sediment.as_ref().unwrap().data.clone();

        SedimentUpdateStage
            .run(&mut world)
            .expect("sediment update run");

        let hs_after = &world.authoritative.sediment.as_ref().unwrap().data;

        // Cell 0 has no upstream flow → no deposition; hs stays at
        // initial (0.1). Cell 4 is the first slope-transition cell: it
        // receives mountain-derived qs_out with capacity dropping by a
        // factor of (0.3/0.02)^1.5 ≈ 58× — deposition is material.
        assert!(
            hs_after[4] > hs_before[4],
            "cell 4 (first plain cell) must gain sediment via deposition; \
             before={}, after={}",
            hs_before[4],
            hs_after[4]
        );
        assert!(
            hs_after[4] > hs_after[0],
            "cell 4 must have more sediment than cell 0 (alluvial fan \
             formation); hs[0]={}, hs[4]={}",
            hs_after[0],
            hs_after[4]
        );

        // Cell 0 has no inbound flux; its hs may decrease (entrainment
        // from a non-zero hs_init against non-zero slope) or stay — but
        // it must not exceed its initial value.
        assert!(
            hs_after[0] <= hs_before[0] + 1e-6,
            "cell 0 (no upstream flow) must not gain sediment; \
             before={}, after={}",
            hs_before[0],
            hs_after[0]
        );

        // Direct deposition_flux signal: the slope-transition cell must
        // have recorded a strictly positive D[p] independent of the net
        // hs direction. This catches the bug class where deposition is
        // accidentally netted against entrainment inside the same sweep
        // (reviewer I2): without a direct D[p]>0 check, a forgotten
        // `- D` in qs_out accounting could leave hs_after > hs_before
        // while D was zero — vacuously passing the hs-difference check.
        let df4 = world.derived.deposition_flux.as_ref().unwrap().data[4];
        assert!(
            df4 > 0.0,
            "deposition_flux at the slope-transition cell must be strictly \
             positive (alluvial-fan signal). got D[4]={df4}"
        );
    }

    /// Task 3.3 Test 3: `hs` upper clamp holds under heavy deposition.
    ///
    /// Saturate a source cell's accumulation + slope so it injects a huge
    /// qs_out into its downstream neighbour, forcing the deposition term
    /// to push `hs` against the 1.0 clamp. Assert no cell exceeds 1.0.
    #[test]
    fn deposition_respects_hs_upper_bound() {
        let (w, h) = (4u32, 1u32);
        let mut world = make_synthetic_world(w, h, space_lite_preset(), true);

        // Seed the upstream cell with huge accumulation + slope.
        {
            let a = world.derived.accumulation.as_mut().unwrap();
            a.data[0] = 1.0e4;
            a.data[1] = 1.0e4;
            a.data[2] = 1.0e4;
            a.data[3] = 1.0e4;
            let s = world.derived.slope.as_mut().unwrap();
            s.data.fill(0.5); // steep everywhere — high entrainment
            // But cell 3 is flat → collapse capacity → all flux deposits
            s.data[3] = 0.001;
        }

        // East-flowing chain into a sink at cell 3.
        {
            let fd = world.derived.flow_dir.as_mut().unwrap();
            fd.data[0] = 0;
            fd.data[1] = 0;
            fd.data[2] = 0;
            fd.data[3] = FLOW_DIR_SINK;
        }

        // Pre-load `hs` near the ceiling to stress the clamp.
        world
            .authoritative
            .sediment
            .as_mut()
            .unwrap()
            .data
            .fill(0.95);

        SedimentUpdateStage
            .run(&mut world)
            .expect("sediment update run");

        let hs = &world.authoritative.sediment.as_ref().unwrap().data;
        for (i, &v) in hs.iter().enumerate() {
            assert!(
                v.is_finite(),
                "cell {i}: hs non-finite after deposition: {v}"
            );
            assert!(
                (0.0..=1.0).contains(&v),
                "cell {i}: hs out of [0, 1] after deposition: {v}"
            );
        }

        // Sanity: deposition_flux is populated and finite.
        let df = world.derived.deposition_flux.as_ref().unwrap();
        for (i, &d) in df.data.iter().enumerate() {
            assert!(
                d.is_finite() && d >= 0.0,
                "cell {i}: deposition D[p] must be finite and non-negative, got {d}"
            );
        }
    }

    /// Task 3.3 Test 4: cumulative deposition cannot exceed cumulative
    /// entrainment + inbound Qs (mass balance within the grid interior).
    ///
    /// A single `SedimentUpdateStage::run` starts with `qs_in == 0`
    /// everywhere, so every non-zero deposition must be "paid for" by
    /// upstream entrainment (sediment taken out of some upstream cell's
    /// hs and moved downstream, minus what leaves through sink cells).
    /// The exact equality is `sum(D) + sink_out = sum(E_sed)`. Since we
    /// don't expose E_sed explicitly, we check the conservative global
    /// invariant: `sum(D) ≤ sum(hs_before - hs_after[deposited < 0]) +
    /// sum(initial_hs)` — i.e. deposition can't create mass from nothing.
    ///
    /// The sharp form we assert: the total sediment in the grid after
    /// the run is bounded above by `initial_sediment + sum_of_entrainment`
    /// — but since the clamps make E_sed ≤ K_sed · A^m · S^n · HS_ENTRAIN_MAX
    /// per step per cell, and we control A/S/hs_init, we instead assert
    /// the weaker but airtight invariant: `sum(hs_after) - sum(hs_before)
    /// ≤ sum(D)` (every gain in total sediment is accounted for by
    /// deposition). This catches the class of bug where Qs routing
    /// fabricates mass.
    #[test]
    fn deposition_flux_mass_balance_no_fabrication() {
        let (w, h) = (6u32, 6u32);
        let mut world = make_synthetic_world(w, h, space_lite_preset(), true);

        // Moderate slope + graded accumulation (sqrt(x+y) style) so a
        // real topological DAG has to be routed.
        for y in 0..h {
            for x in 0..w {
                let idx = (y * w + x) as usize;
                world.derived.slope.as_mut().unwrap().data[idx] = 0.1;
                world.derived.accumulation.as_mut().unwrap().data[idx] = (x + y + 1) as f32;
                // Flow east on interior cells, SE on last-column cells,
                // to create a non-trivial topo DAG. Last cell is SINK.
                world.derived.flow_dir.as_mut().unwrap().data[idx] = if x == w - 1 && y == h - 1 {
                    FLOW_DIR_SINK
                } else if x == w - 1 {
                    6 // S
                } else {
                    0 // E
                };
            }
        }

        let hs_before_total: f32 = world
            .authoritative
            .sediment
            .as_ref()
            .unwrap()
            .data
            .iter()
            .sum();

        SedimentUpdateStage
            .run(&mut world)
            .expect("sediment update run");

        let hs_after_total: f32 = world
            .authoritative
            .sediment
            .as_ref()
            .unwrap()
            .data
            .iter()
            .sum();
        let sum_deposition: f32 = world
            .derived
            .deposition_flux
            .as_ref()
            .unwrap()
            .data
            .iter()
            .sum();

        // Tautological per-cell accounting: Δhs ≤ sum(D · dt) always
        // holds since `hs += D·dt` is the only addition path in
        // SedimentUpdateStage. Kept as a sanity floor (catches gross
        // corruption where hs somehow grows without D being recorded).
        let tol = 1e-5_f32 * (hs_before_total.abs() + sum_deposition.abs() + 1.0);
        assert!(
            hs_after_total <= hs_before_total + sum_deposition * DT + tol,
            "per-cell accounting drift: Δ(total hs) = {:.6} > sum(D)·dt + tol = {:.6}",
            hs_after_total - hs_before_total,
            sum_deposition * DT + tol
        );

        // Physical mass-balance ceiling (reviewer I1): total deposition
        // cannot exceed the entrainment budget the Qs system can inject
        // in a single sweep. With initial `Qs_in == 0` everywhere, the
        // sole source of Qs is entrainment (E_sed · dt per land cell).
        // Upper-bounded per cell by `K_sed · A^m · S^n · HS_ENTRAIN_MAX`
        // (since `hs_eff = hs.min(HS_ENTRAIN_MAX)`). Catches the bug
        // class where `qs_out = qs_in + E_sed·dt` is written without
        // the `- D` term: downstream cells then see inflated Qs_in and
        // cascade into more D than the upstream entrainment could
        // possibly have paid for.
        let k_sed = world.preset.erosion.space_k_sed;
        let m = world.preset.erosion.spim_m;
        let n = world.preset.erosion.spim_n;
        let accum = &world.derived.accumulation.as_ref().unwrap().data;
        let slope = &world.derived.slope.as_ref().unwrap().data;
        let is_land = &world.derived.coast_mask.as_ref().unwrap().is_land.data;
        let entrainment_budget: f32 = (0..(w * h) as usize)
            .filter(|&i| is_land[i] == 1)
            .map(|i| k_sed * accum[i].powf(m) * slope[i].powf(n) * HS_ENTRAIN_MAX)
            .sum::<f32>()
            * DT;
        let sum_d_dt = sum_deposition * DT;
        let budget_tol = 1e-5_f32 * (entrainment_budget.abs() + sum_d_dt.abs() + 1.0);
        assert!(
            sum_d_dt <= entrainment_budget + budget_tol,
            "mass fabrication: sum(D·dt) = {:.6} exceeds entrainment budget \
             K_sed·Σ(A^m·S^n·HS_ENTRAIN_MAX)·dt = {:.6}",
            sum_d_dt,
            entrainment_budget
        );

        // Secondary: deposition is always non-negative.
        for (i, &d) in world
            .derived
            .deposition_flux
            .as_ref()
            .unwrap()
            .data
            .iter()
            .enumerate()
        {
            assert!(
                d >= 0.0,
                "cell {i}: deposition D[p] must be non-negative, got {d}"
            );
        }
    }

    /// Task 3.3: sea cells keep `deposition_flux == 0.0` (they are pure
    /// Qs absorbers in v1 — no offshore dispersal).
    #[test]
    fn deposition_flux_is_zero_on_sea_cells() {
        let (w, h) = (4u32, 4u32);
        let mut world = make_synthetic_world(w, h, space_lite_preset(), false);
        // Rebuild coast mask with a mixed land/sea split.
        {
            let mut is_land = MaskField2D::new(w, h);
            let mut is_sea = MaskField2D::new(w, h);
            for y in 0..h {
                for x in 0..w {
                    let idx = (y * w + x) as usize;
                    if x >= 2 {
                        is_land.data[idx] = 1;
                    } else {
                        is_sea.data[idx] = 1;
                    }
                }
            }
            world.derived.coast_mask = Some(CoastMask {
                is_land,
                is_sea,
                is_coast: MaskField2D::new(w, h),
                land_cell_count: (w * h / 2),
                river_mouth_mask: None,
            });
            // Reinitialise sediment on land-only per Task 3.1.
            let is_land_ref = &world.derived.coast_mask.as_ref().unwrap().is_land.data;
            let sed = world.authoritative.sediment.as_mut().unwrap();
            for (i, v) in sed.data.iter_mut().enumerate() {
                *v = if is_land_ref[i] == 1 { 0.1 } else { 0.0 };
            }
        }

        // Nonzero slope + accumulation on land to drive real flux; sea
        // cells are sinks (default FLOW_DIR_SINK).
        world.derived.slope.as_mut().unwrap().data.fill(0.05);
        world.derived.accumulation.as_mut().unwrap().data.fill(3.0);
        // Route land cells westward into the sea.
        {
            let fd = world.derived.flow_dir.as_mut().unwrap();
            for y in 0..h {
                for x in 0..w {
                    let idx = (y * w + x) as usize;
                    fd.data[idx] = if x >= 2 {
                        4 // W — heads toward sea
                    } else {
                        FLOW_DIR_SINK
                    };
                }
            }
        }

        SedimentUpdateStage
            .run(&mut world)
            .expect("sediment update run");

        let df = world.derived.deposition_flux.as_ref().unwrap();
        let is_land = &world.derived.coast_mask.as_ref().unwrap().is_land.data;
        for i in 0..(w * h) as usize {
            if is_land[i] == 0 {
                assert_eq!(
                    df.data[i], 0.0,
                    "sea cell {i} must have D[p] == 0, got {}",
                    df.data[i]
                );
            }
        }
    }

    /// Task 3.3: prerequisite checks surface clear errors when a required
    /// derived field is missing.
    #[test]
    fn sediment_update_stage_errors_on_missing_prerequisites() {
        // Baseline — all prereqs present → Ok.
        let (w, h) = (4u32, 4u32);
        let mut world = make_synthetic_world(w, h, space_lite_preset(), true);
        assert!(SedimentUpdateStage.run(&mut world).is_ok());

        // Strip flow_dir → Err.
        let mut w_no_fd = make_synthetic_world(w, h, space_lite_preset(), true);
        w_no_fd.derived.flow_dir = None;
        assert!(SedimentUpdateStage.run(&mut w_no_fd).is_err());

        // Strip accumulation → Err.
        let mut w_no_acc = make_synthetic_world(w, h, space_lite_preset(), true);
        w_no_acc.derived.accumulation = None;
        assert!(SedimentUpdateStage.run(&mut w_no_acc).is_err());

        // Strip slope → Err.
        let mut w_no_sl = make_synthetic_world(w, h, space_lite_preset(), true);
        w_no_sl.derived.slope = None;
        assert!(SedimentUpdateStage.run(&mut w_no_sl).is_err());

        // Strip coast_mask → Err.
        let mut w_no_cm = make_synthetic_world(w, h, space_lite_preset(), true);
        w_no_cm.derived.coast_mask = None;
        assert!(SedimentUpdateStage.run(&mut w_no_cm).is_err());

        // Strip sediment → Err.
        let mut w_no_sed = make_synthetic_world(w, h, space_lite_preset(), true);
        w_no_sed.authoritative.sediment = None;
        assert!(SedimentUpdateStage.run(&mut w_no_sed).is_err());
    }

    /// Task 3.3: rerun at the same resolution reuses the
    /// `deposition_flux` backing Vec (no reallocation), matching the
    /// Task 3.1 sediment reuse protocol.
    #[test]
    fn deposition_flux_vec_reused_across_reruns_at_same_resolution() {
        let (w, h) = (4u32, 4u32);
        let mut world = make_synthetic_world(w, h, space_lite_preset(), true);
        world.derived.slope.as_mut().unwrap().data.fill(0.1);
        world.derived.accumulation.as_mut().unwrap().data.fill(1.0);

        SedimentUpdateStage.run(&mut world).expect("first run");

        let df_ref = world.derived.deposition_flux.as_ref().unwrap();
        let ptr_before = df_ref.data.as_ptr();
        let cap_before = df_ref.data.capacity();

        SedimentUpdateStage.run(&mut world).expect("second run");

        let df_ref = world.derived.deposition_flux.as_ref().unwrap();
        assert_eq!(
            df_ref.data.as_ptr(),
            ptr_before,
            "deposition_flux backing Vec pointer changed — unnecessary realloc"
        );
        assert_eq!(
            df_ref.data.capacity(),
            cap_before,
            "deposition_flux backing Vec capacity changed — unnecessary realloc"
        );
    }
}
