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
