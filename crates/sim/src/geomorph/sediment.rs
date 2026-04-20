//! SPACE-lite sediment support — Sprint 3 DD2.
//!
//! This module owns:
//!
//! * Locked SPACE-lite constants ([`SPACE_K_BED_DEFAULT`],
//!   [`SPACE_K_SED_DEFAULT`], [`H_STAR`], [`HS_ENTRAIN_MAX`]).
//! * [`SedimentUpdateStage`] — a unit struct registered inside
//!   [`crate::geomorph::ErosionOuterLoop`]'s inner loop between
//!   [`crate::geomorph::StreamPowerIncisionStage`] and
//!   [`crate::geomorph::HillslopeDiffusionStage`]. Task 3.2's `run` is a
//!   **no-op placeholder**; Task 3.3 fills in the Qs routing +
//!   deposition flux step.
//!
//! ## Why a placeholder stage in 3.2?
//!
//! DD2's pseudo-code updates `hs` inline inside SPIM
//! (`hs += E_bed·dt - E_sed·dt`), so Task 3.2 has no residual per-cell
//! work to do for the sediment update. The stage exists in 3.2 to:
//!
//! * Reserve the slot in `ErosionOuterLoop::run` so Task 3.3 can hang
//!   Qs_in / Qs_out routing on it without restructuring the inner loop.
//! * Register a unique `name() == "sediment_update"` string for future
//!   overlay / validation code.
//! * Lock the inner-step order
//!   `[stream_power_incision, sediment_update, hillslope_diffusion]` via
//!   a dedicated test so Task 3.3 can't silently rearrange it.
//!
//! The placeholder is a pure `Ok(())` — no fake diagnostics or debug
//! asserts that would have to be deleted in 3.3.

use island_core::pipeline::SimulationStage;
use island_core::world::WorldState;

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

// ─── SedimentUpdateStage ─────────────────────────────────────────────────────

/// Sprint 3 DD2: sediment update placeholder (Task 3.2) — Qs routing stub
/// for Task 3.3.
///
/// In Task 3.2 this stage is a **no-op**: DD2's pseudo-code rolls the
/// `hs += E_bed·dt - E_sed·dt` update into the SPIM inner loop, so there
/// is no per-cell work left for this stage in 3.2. The stage is registered
/// inside [`crate::geomorph::ErosionOuterLoop`] between
/// [`crate::geomorph::StreamPowerIncisionStage`] and
/// [`crate::geomorph::HillslopeDiffusionStage`] so Task 3.3 has a stable
/// slot in the inner loop to hang the Qs_in / Qs_out routing +
/// deposition-flux computation on.
///
/// Task 3.3 (not yet implemented) will add:
///
/// * `Qs_in[p] = Σ_{q: flow_dir[q] == p} Qs_out[q]` — upstream sediment flux.
/// * `Qs_out[p]` — downstream flux with entrainment / deposition balance.
/// * `derived.deposition_flux[p]` — deposition per cell, feeding the
///   `deposition_flux` overlay.
///
/// # Naming contract
///
/// The stage `name()` is `"sediment_update"` and is a consumer-visible
/// identifier: future overlay / validation code may key off it. Do not
/// rename without a co-ordinated update across every consumer.
pub struct SedimentUpdateStage;

impl SimulationStage for SedimentUpdateStage {
    fn name(&self) -> &'static str {
        "sediment_update"
    }

    fn run(&self, _world: &mut WorldState) -> anyhow::Result<()> {
        // Task 3.2 placeholder. Task 3.3 will add Qs_in / Qs_out routing +
        // deposition flux here.
        Ok(())
    }
}

// ─── tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    use island_core::preset::{ErosionParams, IslandAge, IslandArchetypePreset};
    use island_core::seed::Seed;
    use island_core::world::{Resolution, WorldState};

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
        }
    }

    /// The stage name string is a consumer-visible identifier. Lock it so
    /// a future accidental rename shows up as a test failure.
    #[test]
    fn sediment_update_stage_name_is_stable() {
        assert_eq!(SedimentUpdateStage.name(), "sediment_update");
    }

    /// Task 3.2 placeholder: `run` must not mutate `world`. Snapshot a few
    /// field identities (height Option + sediment Option) and verify they
    /// are unchanged after the call.
    #[test]
    fn sediment_update_stage_is_noop_in_task_3_2() {
        let mut world = WorldState::new(Seed(0), trivial_preset(), Resolution::new(8, 8));
        // Snapshot Option state.
        let height_was_some = world.authoritative.height.is_some();
        let sediment_was_some = world.authoritative.sediment.is_some();
        SedimentUpdateStage
            .run(&mut world)
            .expect("placeholder stage must not fail");
        assert_eq!(
            world.authoritative.height.is_some(),
            height_was_some,
            "SedimentUpdateStage must not touch authoritative.height"
        );
        assert_eq!(
            world.authoritative.sediment.is_some(),
            sediment_was_some,
            "SedimentUpdateStage must not touch authoritative.sediment"
        );
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
}
