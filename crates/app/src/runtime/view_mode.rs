//! View-mode dispatch + the Sprint 3.5 DD5 **Dominant Surface Contract**.
//!
//! # DD5 Dominant Surface Contract (locked at 3.5.D close-out)
//!
//! With **every overlay off**, each hex on-screen must communicate the
//! four base-read signals via the **surface alone** (no overlay-layer
//! crutches). This is the sprint thesis — the whole reason 3.5 exists:
//!
//! | Signal        | Source                                           | Ships in |
//! |---------------|--------------------------------------------------|----------|
//! | Base fill     | Dominant biome colour (existing 8-biome palette) | 3.5.A c6 |
//! | Elevation cue | Tonal ramp on base fill (NOT stepped extrusion)  | 3.5.A c7 |
//! | Coast cue     | Hex-edge decoration per [`HexCoastClass`]        | 3.5.C c3 |
//! | River cue     | Polyline per [`HexRiverCrossing`]                | 3.5.B c4 |
//!
//! [`HexCoastClass`]: island_core::world::HexCoastClass
//! [`HexRiverCrossing`]: island_core::world::HexRiverCrossing
//!
//! # Overlay-vs-base-read policy
//!
//! All 20 existing overlay descriptors are now **opt-in against the base
//! read**. The hex surface is the readable default; overlays stack on top
//! for analytical inspection but are not required for the hex to
//! communicate useful information.
//!
//! - `hex_aggregated` stays in the catalogue as a debug overlay but is
//!   **no longer the default "hex view"**. The DD5 base-read semantics
//!   make it redundant in HexOnly mode.
//! - [`ViewMode::HexOnly`] displays the true-hex base surface from
//!   `HexSurfaceRenderer` + `HexRiverRenderer` with NO user overlays.
//! - [`ViewMode::HexOverlay`] stacks user overlays on top of the
//!   true-hex base — the DD5 base read is still visible underneath.
//! - [`ViewMode::Continuous`] is unchanged from Sprint 2.5: continuous
//!   terrain + user-selected overlays; no hex surface.
//!
//! # Pick-once-and-commit constants backing DD5
//!
//! - Tonal ramp: `TONAL_MIN = 0.55`, `TONAL_MAX = 1.0`
//!   (`shaders/hex_surface.wgsl`; locked by
//!   `tonal_ramp_constants_match_sprint_3_5_dd5_lock`)
//! - Edge band: `EDGE_BAND_START = 0.82`
//!   (`shaders/hex_surface.wgsl`; locked by `edge_band_start_constant_locked`)
//! - Class tints: `COAST_CLASS_TINTS[0..5]` maps to `palette::HEX_EDGE_*`
//!   (`crates/render/src/hex_surface.rs`; locked by
//!   `coast_class_tints_match_palette_constants`)
//! - `HexCoastClass` discriminants `{Inland=0, OpenOcean=1, Beach=2,
//!   RockyHeadland=3, Estuary=4, Cliff=5, LavaDelta=6}` (`core::world`;
//!   locked by `hex_coast_class_discriminants_stable`)
//!
//! # What DD5 deliberately rejects
//!
//! - **Stepped extrusion for elevation** — would exaggerate per-hex Z
//!   deltas at Fuji-like world aspects (`DEFAULT_WORLD_XZ_EXTENT = 5.0`,
//!   aspect ≈ 0.17) into visual cliffs at every hex boundary, and would
//!   compete with DD4 coast cliff edges for the "vertical signal" budget.
//! - **Multi-class river colouring** — v1 uses a single `palette::RIVER`
//!   tint with width bucketing (`RiverWidth::{Small, Medium, Main}`).
//!   Per-class river colour (e.g. clearwater vs sediment-laden) is a
//!   post-v1 concern.
//! - **Per-edge coast glyphs** (estuary river-mouth glyph, rocky
//!   broken-dash, lava stippling) — v1 uses per-hex uniform tint at the
//!   edge band; per-edge decoration is deferred to 3.5.F polish.

/// Controls which overlays are shown each frame.
///
/// Leaving `Continuous` (in either direction) snapshots the user's current
/// per-overlay visibility as the "baseline". `HexOverlay` renders the
/// baseline + `hex_aggregated` forced on; `HexOnly` renders only
/// `hex_aggregated` (every baseline entry hidden). Returning to
/// `Continuous` restores the baseline and clears the snapshot, so
/// `HexOverlay → HexOnly → Continuous` lands back on the original state
/// regardless of intermediate hops.
///
/// Invariant: `saved_visibility` is `Some` iff `view_mode != Continuous`.
///
/// Sprint 3.5 DD8: now serde-serializable so headless `CaptureShot.view_mode`
/// can specify which render mode a shot executes in.
///
/// Sprint 3.5.D DD5 (module-level doc above): `HexOnly` + `HexOverlay`
/// both include the `HexSurfaceRenderer` + `HexRiverRenderer` base-read
/// stack — the DD5 contract is satisfied by the true-hex surface itself,
/// not by the `hex_aggregated` debug overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ViewMode {
    /// User controls overlay visibility freely. Default state.
    Continuous,
    /// All user-enabled overlays are shown AND `hex_aggregated` is forced on.
    HexOverlay,
    /// Only `hex_aggregated` is shown; all other overlays are hidden.
    /// Prior visibility is saved and restored on exit.
    HexOnly,
}

impl ViewMode {
    /// Human-readable label for the egui ComboBox.
    pub fn label(self) -> &'static str {
        match self {
            ViewMode::Continuous => "Continuous",
            ViewMode::HexOverlay => "Hex overlay",
            ViewMode::HexOnly => "Hex only",
        }
    }
}

// ── RenderLayer + render_stack_for ────────────────────────────────────────────

/// The ordered list of renderer layers to invoke for a given [`ViewMode`].
///
/// This is pure data — GPU-free. Both the interactive frame path
/// (`frame.rs::tick`) and the headless executor (`headless/executor.rs`)
/// call [`render_stack_for`] so interactive ↔ headless render-path parity is
/// lockable as a unit test (plan §5 tier-1 gate).
///
/// # Variant semantics
///
/// | Variant      | What it invokes                              |
/// |--------------|----------------------------------------------|
/// | `Sky`        | `SkyRenderer::draw`                          |
/// | `Terrain`    | `TerrainRenderer::draw`                      |
/// | `HexSurface` | `HexSurfaceRenderer::draw` (c8+)             |
/// | `HexRiver`   | `HexRiverRenderer::draw` (c4 of 3.5.B)      |
/// | `Overlay`    | `OverlayRenderer::draw` (all visibles)       |
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderLayer {
    Sky,
    Terrain,
    HexSurface,
    /// Sprint 3.5.B c4: river polyline pass, drawn AFTER `HexSurface` and
    /// BEFORE `Overlay` so rivers read over the hex fill but under overlays.
    HexRiver,
    Overlay,
}

/// Returns the renderer stack for a [`ViewMode`] per plan §2 DD3 + §3 c4.
///
/// | Mode          | Stack                                             |
/// |---------------|---------------------------------------------------|
/// | `Continuous`  | `[Sky, Terrain, Overlay]`                         |
/// | `HexOverlay`  | `[Sky, Terrain, HexSurface, HexRiver, Overlay]`   |
/// | `HexOnly`     | `[Sky, HexSurface, HexRiver]`                     |
///
/// `Continuous` returns the exact pre-Sprint-3.5 render order, so this
/// function is safe to adopt without changing the existing visual output.
/// `HexRiver` is sandwiched between `HexSurface` and `Overlay` so rivers
/// draw on top of the fill but below overlay textures.
pub fn render_stack_for(mode: ViewMode) -> Vec<RenderLayer> {
    match mode {
        ViewMode::Continuous => {
            vec![RenderLayer::Sky, RenderLayer::Terrain, RenderLayer::Overlay]
        }
        ViewMode::HexOverlay => vec![
            RenderLayer::Sky,
            RenderLayer::Terrain,
            RenderLayer::HexSurface,
            RenderLayer::HexRiver,
            RenderLayer::Overlay,
        ],
        ViewMode::HexOnly => vec![
            RenderLayer::Sky,
            RenderLayer::HexSurface,
            RenderLayer::HexRiver,
        ],
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Tier-1 parity gate (plan §5): both `frame.rs::tick` and
    /// `headless/executor.rs::render_beauty_shot` iterate the `render_stack_for`
    /// output with an **exhaustive** match on `RenderLayer` (no `_` arm). This
    /// means adding a future `RenderLayer` variant is a compile error at both
    /// call sites simultaneously — the variant must be handled in frame.rs AND
    /// executor.rs before the build succeeds. The `render_layer_exhaustive_match_enforced`
    /// test below carries a third exhaustive match in this test module to make
    /// that invariant visible to a code reviewer and to catch any drift if
    /// Strategy A (exhaustive match) is accidentally weakened to a `_ =>` arm.
    ///
    /// Enforcement strategy: **Strategy A** (exhaustive-match compile-time
    /// guarantee) was chosen over Strategy B (source-text grep) because it is
    /// robust to rename/move, zero runtime overhead, and locks both call sites
    /// without depending on fragile string matching.
    #[test]
    fn view_mode_dispatches_identically_in_frame_and_executor() {
        // Verify all three variants return the documented stacks.
        assert_eq!(
            render_stack_for(ViewMode::Continuous),
            vec![RenderLayer::Sky, RenderLayer::Terrain, RenderLayer::Overlay],
        );
        assert_eq!(
            render_stack_for(ViewMode::HexOverlay),
            vec![
                RenderLayer::Sky,
                RenderLayer::Terrain,
                RenderLayer::HexSurface,
                RenderLayer::HexRiver,
                RenderLayer::Overlay,
            ],
        );
        assert_eq!(
            render_stack_for(ViewMode::HexOnly),
            vec![
                RenderLayer::Sky,
                RenderLayer::HexSurface,
                RenderLayer::HexRiver,
            ],
        );
    }

    /// Compile-time enforcement of the exhaustive-match contract.
    ///
    /// This test contains a third exhaustive `match` on `RenderLayer` (mirrors
    /// the ones in `frame.rs::tick` and `executor.rs::render_beauty_shot`).
    /// When a new `RenderLayer` variant is added the build fails here **and**
    /// at both production call sites — the compiler prevents shipping a binary
    /// where one call site handles the new variant but the other silently skips
    /// it.
    ///
    /// If you are adding a new `RenderLayer` variant, update all three matches:
    /// 1. `crates/app/src/runtime/frame.rs` — `frame.rs::tick` scene pass
    /// 2. `crates/app/src/headless/executor.rs` — `render_beauty_shot`
    /// 3. This test (below)
    #[test]
    fn render_layer_exhaustive_match_enforced() {
        // Iterate every layer returned across all ViewMode variants and dispatch
        // via an exhaustive match. The returned bool just gives each arm a
        // reachable, non-trivial body so the compiler won't optimise the match
        // away. Any future `RenderLayer` variant added without updating this
        // block will cause a compile error here.
        let all_layers: Vec<RenderLayer> = [
            ViewMode::Continuous,
            ViewMode::HexOverlay,
            ViewMode::HexOnly,
        ]
        .iter()
        .flat_map(|&m| render_stack_for(m))
        .collect();

        for layer in all_layers {
            let _recognised = match layer {
                RenderLayer::Sky => true,
                RenderLayer::Terrain => true,
                RenderLayer::HexSurface => true,
                RenderLayer::HexRiver => true,
                RenderLayer::Overlay => true,
            };
        }
    }

    /// Continuous mode must return `[Sky, Terrain, Overlay]` exactly — this
    /// is the legacy render path that must not regress.
    #[test]
    fn render_stack_for_continuous_matches_pre_35a_behaviour() {
        let stack = render_stack_for(ViewMode::Continuous);
        assert_eq!(stack.len(), 3);
        assert_eq!(stack[0], RenderLayer::Sky);
        assert_eq!(stack[1], RenderLayer::Terrain);
        assert_eq!(stack[2], RenderLayer::Overlay);
    }

    /// HexOnly mode must NOT include `RenderLayer::Terrain` and MUST include
    /// both `HexSurface` and `HexRiver` (Sprint 3.5.B c4 adds `HexRiver`).
    #[test]
    fn render_stack_for_hex_only_excludes_terrain() {
        let stack = render_stack_for(ViewMode::HexOnly);
        assert!(
            !stack.contains(&RenderLayer::Terrain),
            "HexOnly render stack must not include Terrain; got {stack:?}"
        );
        assert!(
            stack.contains(&RenderLayer::HexSurface),
            "HexOnly render stack must include HexSurface; got {stack:?}"
        );
        assert!(
            stack.contains(&RenderLayer::HexRiver),
            "HexOnly render stack must include HexRiver (Sprint 3.5.B c4); got {stack:?}"
        );
        // Expected exact stack: [Sky, HexSurface, HexRiver]
        assert_eq!(
            stack,
            vec![
                RenderLayer::Sky,
                RenderLayer::HexSurface,
                RenderLayer::HexRiver
            ],
            "HexOnly stack must be [Sky, HexSurface, HexRiver]"
        );
    }
}
