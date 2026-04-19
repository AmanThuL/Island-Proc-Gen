//! World panel — preset picker, seed `DragValue`, three geometry sliders,
//! and a `Regenerate` button.
//!
//! Owns only UI state (selected preset name, seed, override slider values).
//! Runtime maps the returned `WorldPanelEvent` to the appropriate
//! regeneration path (full rebuild for Regenerate, fast `sea_level` path
//! for drag-release).

use island_core::preset::IslandArchetypePreset;

/// Transient state owned by the World panel between frames.
pub struct WorldPanel {
    pub preset_name: String,
    pub seed: u64,
    pub island_radius: f32,
    pub max_relief: f32,
    pub sea_level: f32,
}

/// Events produced by one `WorldPanel::show` call.
///
/// Both flags may be false (no user action this frame), or one may be true.
/// They are mutually exclusive in practice — `drag_stopped` fires on sea_level
/// release while the user is still dragging; `regenerate` fires on button
/// click only.
#[derive(Default, Debug, Clone, Copy)]
pub struct WorldPanelEvent {
    /// User clicked the `Regenerate` button. Runtime should do a full world
    /// rebuild from the current panel state.
    pub regenerate: bool,
    /// The `sea_level` slider was released after a drag. Runtime should run
    /// the `Coastal` fast path instead of a full rebuild.
    pub sea_level_released: bool,
}

impl WorldPanel {
    /// Construct from the currently loaded preset and initial seed.
    pub fn new(current: &IslandArchetypePreset, seed: u64) -> Self {
        Self {
            preset_name: current.name.clone(),
            seed,
            island_radius: current.island_radius,
            max_relief: current.max_relief,
            sea_level: current.sea_level,
        }
    }

    /// Draw the panel into `ui` and return any events that occurred this frame.
    pub fn show(&mut self, ui: &mut egui::Ui) -> WorldPanelEvent {
        let mut event = WorldPanelEvent::default();

        // ── Preset picker ─────────────────────────────────────────────────────
        ui.horizontal(|ui| {
            ui.label("Preset:");
            egui::ComboBox::from_id_salt("world_panel_preset")
                .selected_text(self.preset_name.as_str())
                .show_ui(ui, |ui| {
                    for name in data::presets::list_builtin() {
                        ui.selectable_value(&mut self.preset_name, name.to_string(), name);
                    }
                });
        });

        // ── Seed ──────────────────────────────────────────────────────────────
        ui.horizontal(|ui| {
            ui.label("Seed:");
            ui.add(egui::DragValue::new(&mut self.seed).speed(1.0));
        });

        ui.separator();

        // ── Geometry sliders ──────────────────────────────────────────────────
        ui.add(
            egui::Slider::new(&mut self.island_radius, 0.3..=0.9)
                .text("Island radius")
                .fixed_decimals(2)
                .step_by(0.01),
        );

        ui.add(
            egui::Slider::new(&mut self.max_relief, 0.2..=1.0)
                .text("Max relief")
                .fixed_decimals(2)
                .step_by(0.01),
        );

        // Sea level: drag-release triggers the fast path, not Regenerate.
        let sl_resp = ui.add(
            egui::Slider::new(&mut self.sea_level, 0.1..=0.5)
                .text("Sea level")
                .fixed_decimals(2)
                .step_by(0.01),
        );
        if sl_resp.drag_stopped() {
            event.sea_level_released = true;
        }

        ui.separator();

        // ── Regenerate button ─────────────────────────────────────────────────
        if ui
            .add_sized(
                [ui.available_width(), 28.0],
                egui::Button::new("Regenerate"),
            )
            .clicked()
        {
            event.regenerate = true;
        }

        event
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_copies_preset_fields() {
        let preset =
            data::presets::load_preset("volcanic_single").expect("volcanic_single must load");
        let panel = WorldPanel::new(&preset, 42);
        assert_eq!(panel.preset_name, preset.name);
        assert_eq!(panel.seed, 42);
        assert!((panel.island_radius - preset.island_radius).abs() < 1e-6);
        assert!((panel.max_relief - preset.max_relief).abs() < 1e-6);
        assert!((panel.sea_level - preset.sea_level).abs() < 1e-6);
    }

    #[test]
    fn preset_combobox_lists_all_builtin() {
        let names = data::presets::list_builtin();
        assert!(!names.is_empty(), "list_builtin must return >= 1 preset");
        for n in names {
            data::presets::load_preset(n)
                .unwrap_or_else(|e| panic!("load_preset({n}) failed: {e}"));
        }
    }

    #[test]
    fn slider_ranges_match_preset_schema() {
        // Smoke: the range constants in the panel body must not exclude any
        // stock preset values. If `island_radius` slider range is (0.3, 0.9)
        // and a preset ships with 0.95, the user can't represent it.
        for name in data::presets::list_builtin() {
            let p = data::presets::load_preset(name).unwrap();
            assert!(
                (0.3..=0.9).contains(&p.island_radius),
                "{name}.island_radius {} outside slider range 0.3..=0.9",
                p.island_radius,
            );
            assert!(
                (0.2..=1.0).contains(&p.max_relief),
                "{name}.max_relief {} outside slider range 0.2..=1.0",
                p.max_relief,
            );
            assert!(
                (0.1..=0.5).contains(&p.sea_level),
                "{name}.sea_level {} outside slider range 0.1..=0.5",
                p.sea_level,
            );
        }
    }
}
