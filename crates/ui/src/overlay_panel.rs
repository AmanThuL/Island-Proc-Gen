//! Overlay visibility panel — checkboxes bound to [`OverlayRegistry`].

use render::overlay::OverlayRegistry;

/// egui panel that lets the user toggle overlay visibility.
///
/// Multi-select is supported: each overlay has an independent checkbox.
pub struct OverlayPanel;

impl OverlayPanel {
    /// Draw the "Overlays" window.
    ///
    /// Visibility changes take effect immediately via
    /// [`OverlayRegistry::set_visibility`] and persist to the next frame
    /// because the caller owns the registry.
    pub fn show(ctx: &egui::Context, registry: &mut OverlayRegistry) {
        egui::Window::new("Overlays")
            .default_pos(egui::pos2(16.0, 16.0))
            .show(ctx, |ui| {
                // Snapshot ids first to avoid a simultaneous shared + mutable
                // borrow of `registry` inside the closure.
                let ids: Vec<(&'static str, bool, &'static str)> = registry
                    .all()
                    .iter()
                    .map(|d| (d.id, d.visible, d.label))
                    .collect();

                for (id, mut visible, label) in ids {
                    if ui.checkbox(&mut visible, label).changed() {
                        registry.set_visibility(id, visible);
                    }
                }
            });
    }
}
