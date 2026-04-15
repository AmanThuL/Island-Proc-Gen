//! egui panel widgets for the Island Proc-Gen application.

pub mod overlay_panel;
pub mod params_panel;
pub mod stats_panel;

pub use overlay_panel::OverlayPanel;
pub use params_panel::{ParamsPanel, ParamsPanelResult};
pub use stats_panel::{StatsPanel, StatsPanelData};
