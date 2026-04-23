use render::overlay::OverlayRegistry;

use crate::camera::Camera;
use crate::dock::TabKind;
use crate::world_panel::{WorldPanel, WorldPanelEvent};

use super::view_mode::ViewMode;

/// `egui_dock::TabViewer` implementation for the main window.
///
/// Holds short-lived borrows to all state the tab bodies need for one frame.
/// Constructed inside `tick()` immediately before `DockArea::show_inside` and
/// dropped right after.
pub(super) struct AppTabViewer<'a> {
    pub(super) viewport_tex_id: egui::TextureId,
    pub(super) overlay_registry: &'a mut OverlayRegistry,
    pub(super) camera: &'a mut Camera,
    pub(super) island_radius: f32,
    /// Current horizontal world extent (from `Runtime::world_xz_extent`).
    /// Forwarded to the Camera panel so preset snaps and Reset view use the
    /// same extent as the terrain mesh.
    pub(super) world_xz_extent: f32,
    pub(super) view_mode: ViewMode,
    pub(super) new_view_mode: &'a mut Option<ViewMode>,
    pub(super) preset: &'a mut island_core::preset::IslandArchetypePreset,
    pub(super) params_result: &'a mut ui::ParamsPanelResult,
    pub(super) stats_data: &'a ui::StatsPanelData,
    /// Written each frame by the Viewport tab body so `handle_window_event`
    /// can gate camera input to cursor-inside-viewport-only.
    pub(super) viewport_rect: &'a mut Option<egui::Rect>,
    pub(super) world_panel: &'a mut WorldPanel,
    pub(super) world_event: &'a mut WorldPanelEvent,
}

impl egui_dock::TabViewer for AppTabViewer<'_> {
    type Tab = TabKind;

    fn title(&mut self, tab: &mut TabKind) -> egui::WidgetText {
        tab.title().into()
    }

    fn is_closeable(&self, tab: &TabKind) -> bool {
        tab.closeable()
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut TabKind) {
        match tab {
            TabKind::Viewport => {
                // Capture the rect before adding the image widget so we get
                // the full available area including the pixel the image will
                // occupy. This rect (in logical points) is used by
                // `handle_window_event` to gate mouse input to the viewport.
                let rect = ui.available_rect_before_wrap();
                *self.viewport_rect = Some(rect);
                let avail = rect.size();
                ui.add(egui::Image::new(egui::load::SizedTexture::new(
                    self.viewport_tex_id,
                    avail,
                )));
            }
            TabKind::Overlays => {
                ui::OverlayPanel::show(ui, self.overlay_registry);
            }
            TabKind::World => {
                *self.world_event = self.world_panel.show(ui);
            }
            TabKind::Camera => {
                if let Some(mode) = crate::camera_panel::CameraPanel::show(
                    ui,
                    self.camera,
                    self.island_radius,
                    self.world_xz_extent,
                    self.view_mode,
                ) {
                    *self.new_view_mode = Some(mode);
                }
            }
            TabKind::Params => {
                *self.params_result = ui::ParamsPanel::show(ui, self.preset);
            }
            TabKind::Stats => {
                ui::StatsPanel::show(ui, self.stats_data);
            }
        }
    }
}
