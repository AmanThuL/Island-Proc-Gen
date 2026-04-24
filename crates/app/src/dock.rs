//! egui_dock layout state for the main application window.
//!
//! Defines the [`TabKind`] enum (one variant per dockable tab) and a
//! [`DockLayout`] wrapper around `egui_dock::DockState<TabKind>` that hides
//! serde / file-IO behind simple `load_or_default` / `save` calls.

use std::{fs, io, path::Path};

use egui_dock::{DockState, NodeIndex};
use serde::{Deserialize, Serialize};

/// One variant per dockable tab in the main window.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
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
    /// Read-only hex attribute inspector (Sprint 3.5.E DD7).
    HexInspect,
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
            TabKind::HexInspect => "Hex Inspect",
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
                TabKind::HexInspect,
            ],
        );

        Self { state }
    }

    /// Load a `DockState<TabKind>` from `path`, or fall back to
    /// [`Self::default_layout`] if:
    /// - the file does not exist,
    /// - the file exists but cannot be read (permissions etc.),
    /// - the file exists but cannot be deserialised (schema drift, corruption).
    ///
    /// Never crashes. Failures are logged at `warn` level with the reason;
    /// the caller gets a working default layout.
    pub fn load_or_default(path: &Path) -> Self {
        match fs::read_to_string(path) {
            Ok(contents) => match ron::from_str::<DockState<TabKind>>(&contents) {
                Ok(state) => Self { state },
                Err(err) => {
                    tracing::warn!(
                        path = %path.display(),
                        error = %err,
                        "dock_layout.ron failed to parse; falling back to default layout"
                    );
                    Self::default_layout()
                }
            },
            Err(err) if err.kind() == io::ErrorKind::NotFound => Self::default_layout(),
            Err(err) => {
                tracing::warn!(
                    path = %path.display(),
                    error = %err,
                    "dock_layout.ron read failed; falling back to default layout"
                );
                Self::default_layout()
            }
        }
    }

    /// Write the current layout to `path` in RON format. Creates any missing
    /// parent directories. Returns `Ok(())` on success, bubbles up IO /
    /// serialisation errors otherwise.
    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let ron_text = ron::ser::to_string_pretty(&self.state, ron::ser::PrettyConfig::default())?;
        fs::write(path, ron_text)?;
        Ok(())
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
            TabKind::HexInspect,
        ] {
            assert!(kind.closeable(), "{kind:?} must be user-closeable");
        }
    }

    #[test]
    fn default_layout_contains_all_seven_tab_kinds() {
        let layout = DockLayout::default_layout();
        let tabs: Vec<TabKind> = layout.state.iter_all_tabs().map(|(_, t)| *t).collect();
        // 7 tabs: Viewport + Overlays + World + Camera + Params + Stats + HexInspect
        for kind in [
            TabKind::Viewport,
            TabKind::Overlays,
            TabKind::World,
            TabKind::Camera,
            TabKind::Params,
            TabKind::Stats,
            TabKind::HexInspect,
        ] {
            assert!(tabs.contains(&kind), "{kind:?} must be in default layout");
        }
    }

    #[test]
    fn roundtrip_serde_ron_preserves_layout() {
        let original = DockLayout::default_layout();
        let path = std::env::temp_dir().join("island_proc_gen_dock_roundtrip.ron");
        original.save(&path).expect("save should succeed");

        let loaded = DockLayout::load_or_default(&path);
        let mut orig_tabs: Vec<TabKind> = original.state.iter_all_tabs().map(|(_, t)| *t).collect();
        let mut loaded_tabs: Vec<TabKind> = loaded.state.iter_all_tabs().map(|(_, t)| *t).collect();
        // Sort both so comparison is order-independent across serde round-trips.
        orig_tabs.sort_by_key(|t| *t as u8);
        loaded_tabs.sort_by_key(|t| *t as u8);
        assert_eq!(orig_tabs, loaded_tabs, "tab set must round-trip");

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn load_graceful_fallback_on_corrupt_file() {
        let path = std::env::temp_dir().join("island_proc_gen_dock_corrupt.ron");
        std::fs::write(&path, "not valid RON at all :::").expect("write corrupt");
        let layout = DockLayout::load_or_default(&path);
        // Should fall back to default, which contains Viewport.
        let tabs: Vec<TabKind> = layout.state.iter_all_tabs().map(|(_, t)| *t).collect();
        assert!(tabs.contains(&TabKind::Viewport));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn load_graceful_fallback_on_missing_file() {
        let path = std::env::temp_dir().join("island_proc_gen_dock_missing.ron");
        let _ = std::fs::remove_file(&path); // ensure missing
        let layout = DockLayout::load_or_default(&path);
        let tabs: Vec<TabKind> = layout.state.iter_all_tabs().map(|(_, t)| *t).collect();
        assert!(tabs.contains(&TabKind::Viewport));
    }
}
