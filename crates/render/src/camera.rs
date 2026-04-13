//! Camera preset pack (§3.2 A6).
//!
//! Stateless fixed presets for hero shots, overlay capture, and regression
//! screenshots. [`view_projection`] returns a ready-to-upload `glam::Mat4`.

use glam::{Mat4, Vec3};

/// The three Sprint 1A canonical capture angles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CameraPresetId {
    /// 3/4 perspective, slightly above peak, ~1.6 × radius — the README/
    /// gallery cover shot.
    Hero,
    /// Orthographic top-down, domain fully in frame — regression comparisons.
    TopDebug,
    /// Low-pitch perspective skimming the sea for ridge silhouettes.
    LowOblique,
}

/// Parameters captured in the preset table. All angles in radians.
#[derive(Debug, Clone, Copy)]
pub struct CameraPreset {
    pub id: CameraPresetId,
    /// Angle around the world Y axis (0 = +X, π/2 = +Z).
    pub yaw: f32,
    /// Elevation above the XZ plane. +π/2 looks straight down.
    pub pitch: f32,
    /// Distance multiplier applied to `island_radius` to position the eye.
    pub distance_factor: f32,
    /// Vertical field-of-view in radians. Ignored when `orthographic` is true.
    pub fov_y: f32,
    /// Use orthographic projection instead of perspective.
    pub orthographic: bool,
}

pub const PRESET_HERO: CameraPreset = CameraPreset {
    id: CameraPresetId::Hero,
    yaw: std::f32::consts::FRAC_PI_4,  // 45° — classic 3/4 angle
    pitch: std::f32::consts::FRAC_PI_6, // 30°
    distance_factor: 1.6,
    fov_y: 0.6109, // ~35°
    orthographic: false,
};

pub const PRESET_TOP_DEBUG: CameraPreset = CameraPreset {
    id: CameraPresetId::TopDebug,
    yaw: 0.0,
    pitch: std::f32::consts::FRAC_PI_2 - 0.01, // ~89° (0.01 rad margin to avoid singular look_at_rh)
    distance_factor: 1.4,
    fov_y: 0.0, // ignored
    orthographic: true,
};

pub const PRESET_LOW_OBLIQUE: CameraPreset = CameraPreset {
    id: CameraPresetId::LowOblique,
    yaw: std::f32::consts::FRAC_PI_6, // 30°
    pitch: 0.2182,                    // ~12.5°
    distance_factor: 2.0,
    fov_y: 0.6981, // ~40°
    orthographic: false,
};

pub const ALL_PRESETS: [CameraPreset; 3] = [PRESET_HERO, PRESET_TOP_DEBUG, PRESET_LOW_OBLIQUE];

/// Compute the view × projection matrix for the given preset, aimed at a
/// normalized-domain island whose total span is `[0, 1] × [0, 1]` in the
/// XZ plane with Y-up elevations in `[0, 1]`.
///
/// * `preset` — which of the three canonical angles to use.
/// * `island_radius` — the preset's `distance_factor` is scaled by this.
/// * `aspect` — viewport width / height.
pub fn view_projection(preset: CameraPreset, island_radius: f32, aspect: f32) -> Mat4 {
    let eye = eye_position(preset, island_radius);
    let target = Vec3::new(0.5, 0.0, 0.5);
    let view = Mat4::look_at_rh(eye, target, Vec3::Y);

    let proj = if preset.orthographic {
        // Orthographic frustum sized to just contain the [0,1] × [0,1] domain
        // with a small margin. Keep near/far symmetric around the eye so
        // the whole terrain elevation range is visible.
        let half_w = 0.55 * aspect.max(1e-3);
        let half_h = 0.55;
        Mat4::orthographic_rh(-half_w, half_w, -half_h, half_h, 0.01, 10.0)
    } else {
        Mat4::perspective_rh(preset.fov_y, aspect, 0.01, 100.0)
    };

    proj * view
}

/// Look up a preset by id (for UI dropdown wiring).
pub fn preset_by_id(id: CameraPresetId) -> CameraPreset {
    match id {
        CameraPresetId::Hero => PRESET_HERO,
        CameraPresetId::TopDebug => PRESET_TOP_DEBUG,
        CameraPresetId::LowOblique => PRESET_LOW_OBLIQUE,
    }
}

/// Compute the eye position in world space for the given preset and island radius.
/// Same spherical convention as `app::camera`: Y-up, yaw around Y, pitch above XZ.
pub(crate) fn eye_position(preset: CameraPreset, island_radius: f32) -> Vec3 {
    let target = Vec3::new(0.5, 0.0, 0.5);
    let distance = preset.distance_factor * island_radius.max(0.01);
    target
        + Vec3::new(
            distance * preset.yaw.cos() * preset.pitch.cos(),
            distance * preset.pitch.sin(),
            distance * preset.yaw.sin() * preset.pitch.cos(),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_presets_listed_once() {
        assert_eq!(ALL_PRESETS.len(), 3);
        let ids: Vec<CameraPresetId> = ALL_PRESETS.iter().map(|p| p.id).collect();
        assert!(ids.contains(&CameraPresetId::Hero));
        assert!(ids.contains(&CameraPresetId::TopDebug));
        assert!(ids.contains(&CameraPresetId::LowOblique));
    }

    #[test]
    fn preset_by_id_roundtrip() {
        for id in [
            CameraPresetId::Hero,
            CameraPresetId::TopDebug,
            CameraPresetId::LowOblique,
        ] {
            assert_eq!(preset_by_id(id).id, id);
        }
    }

    #[test]
    fn view_projection_is_finite() {
        let aspect = 16.0_f32 / 9.0;
        for preset in ALL_PRESETS {
            let mat = view_projection(preset, 0.45, aspect);
            for elem in mat.to_cols_array() {
                assert!(
                    elem.is_finite(),
                    "view_projection produced non-finite value for {:?}",
                    preset.id
                );
            }
        }
    }

    #[test]
    fn view_projection_distance_scales_with_radius() {
        let aspect = 16.0_f32 / 9.0;
        let mat_small = view_projection(PRESET_HERO, 0.2, aspect);
        let mat_large = view_projection(PRESET_HERO, 0.8, aspect);
        let small_cols = mat_small.to_cols_array();
        let large_cols = mat_large.to_cols_array();
        assert!(
            small_cols.iter().zip(large_cols.iter()).any(|(a, b)| a != b),
            "matrices for different island radii must differ"
        );
    }

    #[test]
    fn top_debug_uses_orthographic_branch() {
        const { assert!(PRESET_TOP_DEBUG.orthographic) };
        const { assert!(!PRESET_HERO.orthographic) };
        const { assert!(!PRESET_LOW_OBLIQUE.orthographic) };

        let aspect = 16.0_f32 / 9.0;
        let top = view_projection(PRESET_TOP_DEBUG, 0.45, aspect);
        let hero = view_projection(PRESET_HERO, 0.45, aspect);
        let top_cols = top.to_cols_array();
        let hero_cols = hero.to_cols_array();
        assert!(
            top_cols.iter().zip(hero_cols.iter()).any(|(a, b)| a != b),
            "TopDebug and Hero matrices must differ"
        );
    }

    #[test]
    fn hero_eye_is_above_target() {
        let eye = eye_position(PRESET_HERO, 0.45);
        let target_y = 0.0_f32;
        assert!(
            eye.y > target_y,
            "Hero eye (y={}) must be above the XZ plane",
            eye.y
        );
    }

    #[test]
    fn low_oblique_yaw_nonzero() {
        assert_ne!(PRESET_LOW_OBLIQUE.yaw, 0.0);
    }

    #[test]
    fn all_three_presets_produce_distinct_matrices() {
        let aspect = 16.0_f32 / 9.0;
        let radius = 0.45;
        let hero = view_projection(PRESET_HERO, radius, aspect).to_cols_array();
        let top = view_projection(PRESET_TOP_DEBUG, radius, aspect).to_cols_array();
        let low = view_projection(PRESET_LOW_OBLIQUE, radius, aspect).to_cols_array();

        assert!(
            hero.iter().zip(top.iter()).any(|(a, b)| a != b),
            "Hero and TopDebug matrices must differ"
        );
        assert!(
            hero.iter().zip(low.iter()).any(|(a, b)| a != b),
            "Hero and LowOblique matrices must differ"
        );
        assert!(
            top.iter().zip(low.iter()).any(|(a, b)| a != b),
            "TopDebug and LowOblique matrices must differ"
        );
    }
}
