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
use render::CameraPreset;

/// Hard pitch clamp (≈ 89°, just shy of +π/2) to keep `look_at_rh` non-singular
/// at the zenith and nadir. Shared by interactive orbit and preset snap.
const PITCH_CLAMP: f32 = 1.553;

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
        self.pitch = (self.pitch + dy * SENSITIVITY).clamp(-PITCH_CLAMP, PITCH_CLAMP);
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

    /// Snap the orbit camera to a canonical capture preset.
    ///
    /// Sets `target`, `distance`, `yaw`, and `pitch` from the preset table.
    /// `fov_y` and `aspect` are left untouched — presets control geometry, not
    /// the interactive camera's FOV.
    ///
    /// * `extent` — horizontal world span in world units. Pass
    ///   [`render::DEFAULT_WORLD_XZ_EXTENT`] for the baseline geometry;
    ///   `Runtime` passes `self.world_xz_extent` so the preset snap respects
    ///   the current World-panel aspect selection.
    ///
    /// The target is placed at `(extent*0.5, 0.0, extent*0.5)`,
    /// matching `render::camera::view_projection`'s canonical island centre.
    pub fn apply_preset(&mut self, preset: CameraPreset, island_radius: f32, extent: f32) {
        self.target = Vec3::new(extent * 0.5, 0.0, extent * 0.5);
        self.distance = preset.distance_factor * island_radius.max(0.01) * extent;
        self.yaw = preset.yaw;
        // Note: TopDebug's pitch (π/2 − 0.01 ≈ 1.5608) is clamped to ~89°
        // because look_at_rh becomes singular at exactly +π/2.
        self.pitch = preset.pitch.clamp(-PITCH_CLAMP, PITCH_CLAMP);
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
    use render::{ALL_PRESETS, DEFAULT_WORLD_XZ_EXTENT, PRESET_HERO, PRESET_TOP_DEBUG};

    #[test]
    fn apply_preset_hero_sets_spherical_coords() {
        let mut cam = Camera::new(1.0);
        let default_fov = cam.fov_y;
        cam.apply_preset(PRESET_HERO, 0.5, DEFAULT_WORLD_XZ_EXTENT);

        let expected_centre = Vec3::new(
            DEFAULT_WORLD_XZ_EXTENT * 0.5,
            0.0,
            DEFAULT_WORLD_XZ_EXTENT * 0.5,
        );
        assert_eq!(
            cam.target, expected_centre,
            "target must be set to canonical island centre"
        );
        assert!(
            (cam.distance - PRESET_HERO.distance_factor * 0.5 * DEFAULT_WORLD_XZ_EXTENT).abs()
                < 1e-6,
            "distance must equal distance_factor * island_radius * DEFAULT_WORLD_XZ_EXTENT"
        );
        assert!(
            (cam.yaw - PRESET_HERO.yaw).abs() < 1e-6,
            "yaw must match preset"
        );
        assert!(
            (cam.pitch - PRESET_HERO.pitch).abs() < 1e-6,
            "Hero pitch ({}) is within clamp, must match preset exactly",
            PRESET_HERO.pitch
        );
        assert!(
            (cam.fov_y - default_fov).abs() < 1e-6,
            "apply_preset must not touch fov_y"
        );
    }

    #[test]
    fn apply_preset_top_debug_clamps_pitch() {
        let mut cam = Camera::new(1.0);
        cam.apply_preset(PRESET_TOP_DEBUG, 0.5, DEFAULT_WORLD_XZ_EXTENT);

        assert!(
            cam.pitch <= PITCH_CLAMP,
            "TopDebug pitch must be clamped to ≤ {PITCH_CLAMP}, got {}",
            cam.pitch
        );
        assert!(
            cam.pitch >= 1.55,
            "TopDebug pitch must remain near the top (≥ 1.55), got {}",
            cam.pitch
        );
    }

    #[test]
    fn apply_preset_round_trip_all_three_presets() {
        let island_radius = 0.45_f32;
        for preset in ALL_PRESETS {
            let mut cam = Camera::new(16.0 / 9.0);
            cam.apply_preset(preset, island_radius, DEFAULT_WORLD_XZ_EXTENT);

            assert!(
                cam.distance > 0.0,
                "distance must be positive after apply_preset({:?})",
                preset.id
            );

            let eye = cam.eye();
            assert!(
                eye.x.is_finite() && eye.y.is_finite() && eye.z.is_finite(),
                "eye() must be finite after apply_preset({:?})",
                preset.id
            );

            let vp = cam.view_projection();
            for elem in vp.to_cols_array() {
                assert!(
                    elem.is_finite(),
                    "view_projection() must be all-finite after apply_preset({:?})",
                    preset.id
                );
            }
        }
    }

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
        assert!(cam.pitch <= PITCH_CLAMP, "pitch must be clamped below 89°");
        cam.orbit(0.0, -200.0);
        assert!(
            cam.pitch >= -PITCH_CLAMP,
            "pitch must be clamped above -89°"
        );
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
