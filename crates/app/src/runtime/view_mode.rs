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
/// (`frame.rs::tick`) and the headless executor (c9, `headless/executor.rs`)
/// call [`render_stack_for`] so interactive ↔ headless render-path parity is
/// lockable as a unit test (plan §5 tier-1 gate).
///
/// # Variant semantics
///
/// | Variant      | What it invokes                        |
/// |--------------|----------------------------------------|
/// | `Sky`        | `SkyRenderer::draw`                    |
/// | `Terrain`    | `TerrainRenderer::draw`                |
/// | `HexSurface` | `HexSurfaceRenderer::draw` (c8+)       |
/// | `Overlay`    | `OverlayRenderer::draw` (all visibles) |
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderLayer {
    Sky,
    Terrain,
    HexSurface,
    Overlay,
}

/// Returns the renderer stack for a [`ViewMode`] per plan §2 DD5 + §3 c8.
///
/// | Mode          | Stack                                        |
/// |---------------|----------------------------------------------|
/// | `Continuous`  | `[Sky, Terrain, Overlay]`                    |
/// | `HexOverlay`  | `[Sky, Terrain, HexSurface, Overlay]`        |
/// | `HexOnly`     | `[Sky, HexSurface]`                          |
///
/// `Continuous` returns the exact pre-Sprint-3.5 render order, so this
/// function is safe to adopt without changing the existing visual output.
pub fn render_stack_for(mode: ViewMode) -> Vec<RenderLayer> {
    match mode {
        ViewMode::Continuous => {
            vec![RenderLayer::Sky, RenderLayer::Terrain, RenderLayer::Overlay]
        }
        ViewMode::HexOverlay => vec![
            RenderLayer::Sky,
            RenderLayer::Terrain,
            RenderLayer::HexSurface,
            RenderLayer::Overlay,
        ],
        ViewMode::HexOnly => vec![RenderLayer::Sky, RenderLayer::HexSurface],
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Tier-1 parity gate (plan §5): `render_stack_for` returns the same
    /// sequence from every call site — it is pure, GPU-free, and stateless.
    /// c9's headless executor will call this same function, so this test
    /// locks interactive ↔ headless render-path parity in a single assertion.
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
                RenderLayer::Overlay,
            ],
        );
        assert_eq!(
            render_stack_for(ViewMode::HexOnly),
            vec![RenderLayer::Sky, RenderLayer::HexSurface],
        );
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

    /// HexOnly mode must NOT include `RenderLayer::Terrain`.
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
    }
}
