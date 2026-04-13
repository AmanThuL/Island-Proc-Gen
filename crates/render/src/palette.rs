//! Colour palette definitions for overlay rendering and CPU batch export.
//!
//! [`sample`] maps a normalised `t ∈ [0, 1]` value to an RGBA byte quad
//! using the chosen [`PaletteId`].
//!
//! Sprint 0: only [`PaletteId::Grayscale`] is fully implemented. The
//! remaining variants return transparent black `[0, 0, 0, 0]` as a
//! placeholder; Sprint 1A will fill them in with Viridis, Turbo, and
//! Categorical lookup tables.

// ─── PaletteId ───────────────────────────────────────────────────────────────

/// Identifies a colour palette for overlay rendering / export.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteId {
    /// Linear grayscale: `t = 0` → black, `t = 1` → white. (Sprint 0)
    Grayscale,
    /// Perceptually-uniform blue-green-yellow. Placeholder until Sprint 1A.
    Viridis,
    /// High-contrast rainbow (Turbo). Placeholder until Sprint 1A.
    Turbo,
    /// Discrete colour categories. Placeholder until Sprint 1A.
    Categorical,
}

// ─── sample ──────────────────────────────────────────────────────────────────

/// Sample a palette at normalised position `t ∈ [0, 1]`.
///
/// Returns `[R, G, B, A]` bytes.
///
/// | Palette      | Sprint 0 behaviour                          |
/// |--------------|---------------------------------------------|
/// | `Grayscale`  | `[v, v, v, 255]` where `v = clamp(t)*255`   |
/// | `Viridis`    | `[0, 0, 0, 0]` — Sprint 1A placeholder      |
/// | `Turbo`      | `[0, 0, 0, 0]` — Sprint 1A placeholder      |
/// | `Categorical`| `[0, 0, 0, 0]` — Sprint 1A placeholder      |
pub fn sample(palette: PaletteId, t: f32) -> [u8; 4] {
    match palette {
        PaletteId::Grayscale => {
            let v = (t.clamp(0.0, 1.0) * 255.0) as u8;
            [v, v, v, 255]
        }
        // Sprint 1A: replace these placeholders with real lookup tables.
        PaletteId::Viridis | PaletteId::Turbo | PaletteId::Categorical => [0, 0, 0, 0],
    }
}

// ─── tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grayscale_black_at_zero() {
        assert_eq!(sample(PaletteId::Grayscale, 0.0), [0, 0, 0, 255]);
    }

    #[test]
    fn grayscale_white_at_one() {
        assert_eq!(sample(PaletteId::Grayscale, 1.0), [255, 255, 255, 255]);
    }

    #[test]
    fn grayscale_clamps_below_zero() {
        assert_eq!(sample(PaletteId::Grayscale, -1.0), [0, 0, 0, 255]);
    }

    #[test]
    fn grayscale_clamps_above_one() {
        assert_eq!(sample(PaletteId::Grayscale, 2.0), [255, 255, 255, 255]);
    }

    #[test]
    fn sprint_0_placeholders_transparent() {
        assert_eq!(sample(PaletteId::Viridis, 0.5), [0, 0, 0, 0]);
        assert_eq!(sample(PaletteId::Turbo, 0.5), [0, 0, 0, 0]);
        assert_eq!(sample(PaletteId::Categorical, 0.5), [0, 0, 0, 0]);
    }
}
