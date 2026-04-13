//! Orbit / pan / zoom camera for the main 3-D view.
//!
//! The camera is a spherical-coordinate orbit camera: it keeps a *target*
//! point in world space and orbits around it at a given *distance*, *yaw*
//! (rotation around Y) and *pitch* (elevation above XZ plane).
//!
//! Input state (which buttons are held, last cursor position) lives in
//! [`InputState`] which is updated by `Runtime::handle_window_event` and
//! then passed to [`Camera::process_drag`] / [`Camera::process_scroll`].

use glam::{Mat4, Vec3};

// ── Camera ────────────────────────────────────────────────────────────────────

/// Spherical orbit camera.
pub struct Camera {
    /// World-space point the camera orbits around.
    pub target: Vec3,
    /// Distance from target to eye.
    pub distance: f32,
    /// Yaw angle in radians (rotation around the world Y axis).
    pub yaw: f32,
    /// Pitch angle in radians (elevation above the XZ plane), clamped ±89°.
    pub pitch: f32,
    /// Vertical field-of-view in radians.
    pub fov_y: f32,
    /// Viewport aspect ratio (width / height).
    pub aspect: f32,
}

impl Camera {
    /// Create a camera looking at the origin from a sensible default position.
    pub fn new(aspect: f32) -> Self {
        Self {
            target: Vec3::ZERO,
            distance: 3.0,
            yaw: 0.0,
            pitch: -0.5,
            fov_y: 45_f32.to_radians(),
            aspect,
        }
    }

    /// Compute the eye position from the current spherical coordinates.
    pub fn eye(&self) -> Vec3 {
        let x = self.distance * self.yaw.cos() * self.pitch.cos();
        let y = self.distance * self.pitch.sin();
        let z = self.distance * self.yaw.sin() * self.pitch.cos();
        self.target + Vec3::new(x, y, z)
    }

    /// Combined view × projection matrix (row-major, ready for the GPU).
    pub fn view_projection(&self) -> Mat4 {
        let view = Mat4::look_at_rh(self.eye(), self.target, Vec3::Y);
        let proj = Mat4::perspective_rh(self.fov_y, self.aspect, 0.1, 100.0);
        proj * view
    }

    /// Orbit around the target point (left-button drag).
    ///
    /// `dx` / `dy` are normalised deltas (screen pixels / screen dimension).
    pub fn orbit(&mut self, dx: f32, dy: f32) {
        const SENSITIVITY: f32 = 2.5;
        self.yaw += dx * SENSITIVITY;
        self.pitch = (self.pitch + dy * SENSITIVITY).clamp(-1.553, 1.553);
    }

    /// Pan the target point in screen-space XZ (right-button drag).
    pub fn pan(&mut self, dx: f32, dy: f32) {
        const SENSITIVITY: f32 = 0.5;
        self.target.x += dx * self.distance * SENSITIVITY;
        self.target.z += dy * self.distance * SENSITIVITY;
    }

    /// Zoom by adjusting distance (scroll wheel).
    ///
    /// `scroll` is the raw scroll delta in lines (positive = zoom in).
    pub fn zoom(&mut self, scroll: f32) {
        self.distance = (self.distance * (1.0 - scroll * 0.1)).clamp(0.5, 100.0);
    }

    /// Update the aspect ratio after a window resize.
    pub fn set_aspect(&mut self, aspect: f32) {
        self.aspect = aspect;
    }
}

// ── InputState ────────────────────────────────────────────────────────────────

/// Mouse / keyboard state consumed by `Runtime` to drive camera updates.
#[derive(Default)]
pub struct InputState {
    pub left_pressed: bool,
    pub right_pressed: bool,
    pub shift_held: bool,
    pub last_cursor: Option<(f64, f64)>,
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orbit_changes_yaw() {
        let mut cam = Camera::new(1.0);
        let yaw_before = cam.yaw;
        cam.orbit(0.1, 0.0);
        assert!(
            cam.yaw > yaw_before,
            "yaw should increase after positive dx orbit"
        );
    }

    #[test]
    fn pitch_clamped() {
        let mut cam = Camera::new(1.0);
        cam.orbit(0.0, 100.0);
        // Pitch must not exceed ~89°
        assert!(cam.pitch <= 1.553, "pitch must be clamped below 89°");
        cam.orbit(0.0, -200.0);
        assert!(cam.pitch >= -1.553, "pitch must be clamped above -89°");
    }

    #[test]
    fn zoom_clamped() {
        let mut cam = Camera::new(1.0);
        // Huge zoom-in
        for _ in 0..1000 {
            cam.zoom(10.0);
        }
        assert!(cam.distance >= 0.5, "distance must not go below 0.5");
        // Huge zoom-out
        for _ in 0..1000 {
            cam.zoom(-10.0);
        }
        assert!(cam.distance <= 100.0, "distance must not exceed 100");
    }

    #[test]
    fn view_projection_does_not_panic() {
        let cam = Camera::new(16.0 / 9.0);
        let vp = cam.view_projection();
        // All elements should be finite
        for col in vp.to_cols_array() {
            assert!(
                col.is_finite(),
                "view_projection should produce finite values"
            );
        }
    }
}
