//! Overlay visibility panel — checkboxes + alpha sliders bound to [`OverlayRegistry`].

use render::overlay::OverlayRegistry;

/// egui panel that lets the user toggle overlay visibility and adjust alpha.
///
/// Each registered overlay gets one row:
/// `[checkbox visible] [slider alpha 0..1] [name]`
///
/// Row count equals `registry.entries().len()` — never hardcoded. Adding new
/// overlays to the registry automatically gains a slider row here.
pub struct OverlayPanel;

impl OverlayPanel {
    /// Draw the "Overlays" window.
    ///
    /// Visibility and alpha changes take effect immediately via mutable
    /// access to each descriptor and persist to the next frame because the
    /// caller owns the registry.
    pub fn show(ctx: &egui::Context, registry: &mut OverlayRegistry) {
        egui::Window::new("Overlays")
            .default_pos(egui::pos2(16.0, 16.0))
            .show(ctx, |ui| {
                for entry in registry.entries_mut() {
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut entry.visible, "");
                        ui.add(
                            egui::Slider::new(&mut entry.alpha, 0.0..=1.0)
                                .fixed_decimals(2)
                                .clamping(egui::SliderClamping::Always),
                        );
                        ui.label(entry.label);
                    });
                }
            });
    }
}
