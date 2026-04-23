//! Overlay descriptor registry.
//!
//! **Guardrail (CLAUDE.md / AGENTS.md invariant #8):** raw string field
//! keys (e.g. `"height"`, `"sediment"`, `"deposition_flux"`,
//! `"fog_water_input"`) are allowed ONLY inside `overlay/resolve.rs`.
//! The catalog / range / mod files refer to overlay sources via
//! [`SourceKey`] enum handles, never by raw string. Adding a new overlay
//! means adding a [`SourceKey`] variant AND its resolver arm in `resolve.rs`.
//!
//! The "source of truth" for overlays is **data descriptors**, not render
//! closures. The same [`OverlayDescriptor`] feeds:
//!
//! * Sprint 1A+ real-time GPU render path (reads `OverlaySource` to locate
//!   the right field in `WorldState`).
//! * Sprint 4+ CPU batch-export path (same descriptor, CPU-side field → PNG
//!   conversion, no GPU involvement).

pub mod catalog;
pub mod range;
pub mod resolve;

pub use range::ValueRange;
pub use resolve::{OverlaySource, SourceKey};
pub(crate) use resolve::{ResolvedField, resolve_scalar_source};

use crate::palette::PaletteId;

// ─── OverlayDescriptor ───────────────────────────────────────────────────────

/// A fully self-describing overlay: everything needed to both render the
/// overlay on-screen and export it as a PNG in batch mode.
///
/// Derives `Copy` because it is trivial POD (all fields are either primitives
/// or enum variants).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OverlayDescriptor {
    /// Stable identifier used for registry lookups.
    pub id: &'static str,
    /// Human-readable name shown in the UI panel.
    pub label: &'static str,
    /// Which `WorldState` field this overlay reads.
    pub source: OverlaySource,
    /// Colour mapping to apply to normalised field values.
    pub palette: PaletteId,
    /// How to map the field value range to `[0, 1]` palette input.
    pub value_range: ValueRange,
    /// Whether this overlay is currently shown.
    pub visible: bool,
    /// Opacity applied when blending this overlay over the terrain.
    /// Range `[0.0, 1.0]`; default `0.6`.
    pub alpha: f32,
}

// ─── OverlayRegistry ─────────────────────────────────────────────────────────

/// Holds all registered [`OverlayDescriptor`]s for the session.
pub struct OverlayRegistry {
    pub(crate) entries: Vec<OverlayDescriptor>,
}

impl OverlayRegistry {
    /// Return a slice of all registered descriptors.
    pub fn all(&self) -> &[OverlayDescriptor] {
        &self.entries
    }

    /// Return a mutable iterator over all registered descriptors.
    ///
    /// Used by [`crate::overlay_panel::OverlayPanel`] to update per-descriptor
    /// `visible` and `alpha` fields from the egui UI without requiring a
    /// string-keyed setter for each property.
    pub fn entries_mut(&mut self) -> impl Iterator<Item = &mut OverlayDescriptor> {
        self.entries.iter_mut()
    }

    /// Look up a descriptor by its `id`.
    pub fn by_id(&self, id: &str) -> Option<&OverlayDescriptor> {
        self.entries.iter().find(|e| e.id == id)
    }

    /// Mutable look-up by `id`.
    pub fn by_id_mut(&mut self, id: &str) -> Option<&mut OverlayDescriptor> {
        self.entries.iter_mut().find(|e| e.id == id)
    }

    /// Set the `visible` flag for the entry with the given `id`.
    /// No-op if the id is not found.
    pub fn set_visibility(&mut self, id: &str, visible: bool) {
        if let Some(d) = self.by_id_mut(id) {
            d.visible = visible;
        }
    }

    /// Iterate over all currently-visible descriptors.
    pub fn visible_entries(&self) -> impl Iterator<Item = &OverlayDescriptor> {
        self.entries.iter().filter(|d| d.visible)
    }
}

impl Default for OverlayRegistry {
    fn default() -> Self {
        Self::sprint_3_defaults()
    }
}
