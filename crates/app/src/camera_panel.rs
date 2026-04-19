//! Camera panel — orbit camera readouts + editable distance/yaw/pitch/FOV.

use crate::camera::Camera;
use crate::runtime::{INITIAL_CAMERA_DISTANCE, INITIAL_CAMERA_PITCH, INITIAL_CAMERA_YAW, ViewMode};
use render::{ALL_PRESETS, CameraPreset, CameraPresetId, WORLD_XZ_EXTENT};

/// Human-readable label for a preset id, shown in the dropdown.
fn preset_label(id: CameraPresetId) -> &'static str {
    match id {
        CameraPresetId::Hero => "Hero 3/4",
        CameraPresetId::TopDebug => "Top debug",
        CameraPresetId::LowOblique => "Low oblique",
    }
}

/// egui panel exposing the interactive orbit camera state.
pub struct CameraPanel;

impl CameraPanel {
    /// Draw the "Camera" tab body inline into the provided `ui`. Edits to
    /// `camera` take effect on the next frame.
    ///
    /// `island_radius` is used to scale the preset's `distance_factor` when
    /// the user snaps to a canonical capture angle via the preset ComboBox.
    ///
    /// Returns `Some(new_mode)` if the user selected a different [`ViewMode`],
    /// `None` if unchanged.
    pub fn show(
        ui: &mut egui::Ui,
        camera: &mut Camera,
        island_radius: f32,
        view_mode: ViewMode,
    ) -> Option<ViewMode> {
        let mut new_view_mode: Option<ViewMode> = None;

        ui.label(format!(
            "target: {:.3}, {:.3}, {:.3}",
            camera.target.x, camera.target.y, camera.target.z
        ));
        let eye = camera.eye();
        ui.label(format!("eye: {:.3}, {:.3}, {:.3}", eye.x, eye.y, eye.z));

        ui.separator();

        // ── Preset snap ───────────────────────────────────────────────
        // One-shot ComboBox: selecting a preset immediately jumps the
        // orbit camera to the canonical capture angle and returns the
        // dropdown to "— choose —". Orbit / pan / zoom continue from
        // the new position.
        let mut selected: Option<CameraPreset> = None;
        egui::ComboBox::from_label("preset")
            .selected_text("— choose —")
            .show_ui(ui, |ui| {
                for preset in ALL_PRESETS {
                    if ui
                        .selectable_label(false, preset_label(preset.id))
                        .clicked()
                    {
                        selected = Some(preset);
                    }
                }
            });
        if let Some(preset) = selected {
            camera.apply_preset(preset, island_radius);
        }

        ui.separator();

        ui.horizontal(|ui| {
            ui.label("distance");
            ui.add(
                egui::DragValue::new(&mut camera.distance)
                    .speed(0.05)
                    .range(0.5_f32..=20.0),
            );
        });

        let mut yaw_deg = camera.yaw.to_degrees();
        ui.horizontal(|ui| {
            ui.label("yaw (°)");
            if ui
                .add(egui::DragValue::new(&mut yaw_deg).speed(1.0))
                .changed()
            {
                camera.yaw = yaw_deg.to_radians();
            }
        });

        let mut pitch_deg = camera.pitch.to_degrees();
        ui.horizontal(|ui| {
            ui.label("pitch (°)");
            if ui
                .add(
                    egui::DragValue::new(&mut pitch_deg)
                        .speed(1.0)
                        .range(-89.0_f32..=89.0),
                )
                .changed()
            {
                camera.pitch = pitch_deg.to_radians();
            }
        });

        let mut fov_deg = camera.fov_y.to_degrees();
        ui.horizontal(|ui| {
            ui.label("fov (°)");
            if ui
                .add(
                    egui::DragValue::new(&mut fov_deg)
                        .speed(0.5)
                        .range(10.0_f32..=120.0),
                )
                .changed()
            {
                camera.fov_y = fov_deg.to_radians();
            }
        });

        ui.separator();

        if ui.button("Reset view").clicked() {
            camera.distance = INITIAL_CAMERA_DISTANCE * WORLD_XZ_EXTENT;
            camera.yaw = INITIAL_CAMERA_YAW;
            camera.pitch = INITIAL_CAMERA_PITCH;
        }

        ui.separator();

        // ── ViewMode selector ─────────────────────────────────────────
        // Continuous: user controls overlay visibility freely.
        // HexOverlay: keeps user overlays AND forces hex_aggregated on.
        // HexOnly: hides everything except hex_aggregated (saves/restores prior state).
        let all_modes = [
            ViewMode::Continuous,
            ViewMode::HexOverlay,
            ViewMode::HexOnly,
        ];
        egui::ComboBox::from_label("view mode")
            .selected_text(view_mode.label())
            .show_ui(ui, |ui| {
                for &mode in &all_modes {
                    if ui
                        .selectable_label(view_mode == mode, mode.label())
                        .clicked()
                        && mode != view_mode
                    {
                        new_view_mode = Some(mode);
                    }
                }
            });

        new_view_mode
    }
}
