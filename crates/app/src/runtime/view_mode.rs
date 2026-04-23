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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
