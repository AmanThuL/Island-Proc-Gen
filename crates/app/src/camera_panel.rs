//! Camera panel — orbit camera readouts + editable distance/yaw/pitch/FOV and
//! render-path vertical scale knob.

use crate::camera::Camera;
use crate::runtime::{
    INITIAL_CAMERA_DISTANCE, INITIAL_CAMERA_PITCH, INITIAL_CAMERA_YAW, INITIAL_VERTICAL_SCALE,
};

/// egui panel exposing the interactive orbit camera state.
pub struct CameraPanel;

impl CameraPanel {
    /// Draw the "Camera" window. Edits to `camera` and `vertical_scale` take
    /// effect on the next frame.
    pub fn show(ctx: &egui::Context, camera: &mut Camera, vertical_scale: &mut f32) {
        egui::Window::new("Camera")
            .default_pos(egui::pos2(16.0, 340.0))
            .show(ctx, |ui| {
                ui.label(format!(
                    "target: {:.3}, {:.3}, {:.3}",
                    camera.target.x, camera.target.y, camera.target.z
                ));
                let eye = camera.eye();
                ui.label(format!("eye: {:.3}, {:.3}, {:.3}", eye.x, eye.y, eye.z));

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

                ui.horizontal(|ui| {
                    ui.label("vertical scale");
                    ui.add(egui::Slider::new(vertical_scale, 0.1_f32..=2.0));
                });

                ui.separator();

                if ui.button("Reset view").clicked() {
                    camera.distance = INITIAL_CAMERA_DISTANCE;
                    camera.yaw = INITIAL_CAMERA_YAW;
                    camera.pitch = INITIAL_CAMERA_PITCH;
                    *vertical_scale = INITIAL_VERTICAL_SCALE;
                }
            });
    }
}
