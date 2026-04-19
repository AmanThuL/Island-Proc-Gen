//! egui_dock layout state for the main application window.
//!
//! Defines the [`TabKind`] enum (one variant per dockable tab) and a
//! [`DockLayout`] wrapper around `egui_dock::DockState<TabKind>` that hides
//! serde / file-IO behind simple `load_or_default` / `save` calls.
//!
//! B.2: state lives in memory only; `load_or_default` returns the default
//! layout unconditionally. Persistence arrives in B.3.

use egui_dock::{DockState, NodeIndex};

/// One variant per dockable tab in the main window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabKind {
    /// The 3-D scene viewport (rendered via offscreen texture).
    Viewport,
    /// Overlay visibility/alpha controls.
    Overlays,
    /// World generation controls (placeholder body in B.2; full impl in 2.6.C).
    World,
    /// Orbit camera readouts and controls.
    Camera,
    /// Preset parameter sliders.
    Params,
    /// Runtime statistics (FPS, resolution, seed).
    Stats,
}

impl TabKind {
    /// Human-readable title shown in the tab bar.
    pub fn title(self) -> &'static str {
        match self {
            TabKind::Viewport => "Viewport",
            TabKind::Overlays => "Overlays",
            TabKind::World => "World",
            TabKind::Camera => "Camera",
            TabKind::Params => "Params",
            TabKind::Stats => "Stats",
        }
    }

    /// Whether this tab can be closed by the user.
    ///
    /// Only [`TabKind::Viewport`] is non-closeable — closing the viewport
    /// would leave the user with no 3-D view. All other tabs can be closed
    /// and reopened via a "View" menu (out of scope for B.2).
    pub fn closeable(self) -> bool {
        !matches!(self, TabKind::Viewport)
    }
}

/// Wrapper around `egui_dock::DockState<TabKind>` with a stable public API.
///
/// B.2: in-memory only. B.3 will add `load_or_default` / `save` persistence.
pub struct DockLayout {
    /// The underlying dock state. Exposed publicly so `Runtime::tick` can
    /// pass `&mut self.dock.state` directly to `DockArea::new`.
    pub state: DockState<TabKind>,
}

impl DockLayout {
    /// Build the initial three-column layout:
    ///
    /// ```text
    /// ┌──────────┬─────────────────────────┬────────────┐
    /// │ Overlays │       Viewport          │  World     │
    /// │  (~20 %) │        (~60 %)          │  Camera    │
    /// │          │                         │  Params    │
    /// │          │                         │  Stats     │
    /// │          │                         │  (~20 %)   │
    /// └──────────┴─────────────────────────┴────────────┘
    /// ```
    ///
    /// The exact pixel widths are suggestions; the user can drag the
    /// separators to taste.
    pub fn default_layout() -> Self {
        // Start with Viewport as the sole tab in the root node.
        let mut state = DockState::new(vec![TabKind::Viewport]);

        let surface = state.main_surface_mut();

        // Split left: Overlays occupies 20 % of root, Viewport keeps 80 %.
        // split_left returns [old_node, new_node]; old_node is the Viewport
        // side (right), new_node is Overlays (left).
        //
        // Note: in egui_dock 0.19 the `fraction` argument to split_left
        // controls the OLD node's share, NOT the new node's.  Passing 0.8
        // therefore gives Viewport 80 % and Overlays 20 %.
        let [centre_node, _overlays_node] =
            surface.split_left(NodeIndex::root(), 0.8, vec![TabKind::Overlays]);

        // Split right: right-side info column occupies the remaining 20 %
        // of the centre node.  `split_right` returns [old_node, new_node];
        // old_node is Viewport (left), new_node is the right column.
        // Passing 0.75 gives Viewport 75 % of the centre_node slice and
        // the right column the remaining 25 %, yielding a rough 60/20 split
        // across the full window.
        let _right_node = surface.split_right(
            centre_node,
            0.75,
            vec![
                TabKind::World,
                TabKind::Camera,
                TabKind::Params,
                TabKind::Stats,
            ],
        );

        Self { state }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_layout_has_viewport_tab() {
        let layout = DockLayout::default_layout();
        let tabs: Vec<TabKind> = layout.state.iter_all_tabs().map(|(_, t)| *t).collect();
        assert!(
            tabs.contains(&TabKind::Viewport),
            "viewport tab must exist in default layout"
        );
    }

    #[test]
    fn viewport_tab_is_not_closeable() {
        assert!(!TabKind::Viewport.closeable());
    }

    #[test]
    fn all_panels_closeable_except_viewport() {
        for kind in [
            TabKind::Overlays,
            TabKind::World,
            TabKind::Camera,
            TabKind::Params,
            TabKind::Stats,
        ] {
            assert!(kind.closeable(), "{kind:?} must be user-closeable");
        }
    }

    #[test]
    fn default_layout_contains_all_six_tab_kinds() {
        let layout = DockLayout::default_layout();
        let tabs: Vec<TabKind> = layout.state.iter_all_tabs().map(|(_, t)| *t).collect();
        for kind in [
            TabKind::Viewport,
            TabKind::Overlays,
            TabKind::World,
            TabKind::Camera,
            TabKind::Params,
            TabKind::Stats,
        ] {
            assert!(tabs.contains(&kind), "{kind:?} must be in default layout");
        }
    }
}
