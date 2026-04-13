//! Overlay descriptor registry.
//!
//! The "source of truth" for overlays is **data descriptors**, not render
//! closures. The same [`OverlayDescriptor`] feeds:
//!
//! * Sprint 1A+ real-time GPU render path (reads `OverlaySource` to locate
//!   the right field in `WorldState`).
//! * Sprint 4+ CPU batch-export path (same descriptor, CPU-side field → PNG
//!   conversion, no GPU involvement).
//!
//! **Guardrail**: the `&'static str` field-key strings inside [`OverlaySource`]
//! appear **only** in this file. `crates/sim`, `crates/core`, `crates/hex`,
//! and `crates/data` must never use string-key access to `WorldState` fields —
//! they always go through typed struct field paths.

use crate::palette::PaletteId;

// ─── ValueRange ──────────────────────────────────────────────────────────────

/// Governs how to map a field's raw value range to the `[0, 1]` palette input.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ValueRange {
    /// Derive the mapping from the actual field min/max at render time.
    Auto,
    /// Fixed `[lo, hi]` mapping regardless of the field's actual range.
    Fixed(f32, f32),
}

impl ValueRange {
    /// Resolve this range to a concrete `(lo, hi)` pair.
    ///
    /// * `Auto` → returns `(field_min, field_max)` from the supplied values.
    /// * `Fixed(lo, hi)` → returns `(lo, hi)` unchanged.
    pub fn resolve(self, field_min: f32, field_max: f32) -> (f32, f32) {
        match self {
            ValueRange::Auto => (field_min, field_max),
            ValueRange::Fixed(lo, hi) => (lo, hi),
        }
    }
}

// ─── OverlaySource ───────────────────────────────────────────────────────────

/// Identifies which `WorldState` sub-field an overlay reads from.
///
/// The `&'static str` values are **field-key strings** that Sprint 1A's render
/// path will use to locate the correct `Option<ScalarField2D<f32>>` (or
/// equivalent) in `AuthoritativeFields`, `BakedSnapshot`, or `DerivedCaches`.
///
/// **These string literals must stay confined to this file.** See the module
/// doc for the full guardrail explanation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlaySource {
    /// A field in `island_core::world::AuthoritativeFields` — world-truth data.
    ScalarAuthoritative(&'static str),
    /// A field in `island_core::world::BakedSnapshot` — stable derived snapshot.
    ScalarBaked(&'static str),
    /// A field in `island_core::world::DerivedCaches` — pure runtime cache.
    ScalarDerived(&'static str),
    /// A mask field (boolean/u8) from any layer.
    Mask(&'static str),
    /// A vector field from any layer.
    Vector(&'static str),
}

// ─── OverlayDescriptor ───────────────────────────────────────────────────────

/// A fully self-describing overlay: everything needed to both render the
/// overlay on-screen and export it as a PNG in batch mode.
///
/// Derives `Copy` because it is trivial POD (all fields are either primitives
/// or `&'static str`).
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
}

// ─── OverlayRegistry ─────────────────────────────────────────────────────────

/// Holds all registered [`OverlayDescriptor`]s for the session.
pub struct OverlayRegistry {
    entries: Vec<OverlayDescriptor>,
}

impl OverlayRegistry {
    /// Return the Sprint 0 placeholder registry.
    ///
    /// Three entries whose `OverlaySource` key strings match the field names
    /// Sprint 1A will introduce under `AuthoritativeFields` and
    /// `DerivedCaches`. The render path draws a transparent placeholder
    /// texture while those fields remain `None`.
    pub fn sprint_0_defaults() -> Self {
        Self {
            entries: vec![
                OverlayDescriptor {
                    id: "initial_uplift",
                    label: "Initial uplift",
                    // Sprint 1A: add `initial_uplift: Option<ScalarField2D<f32>>`
                    // to DerivedCaches and keep this key in sync.
                    source: OverlaySource::ScalarDerived("initial_uplift"),
                    palette: PaletteId::Grayscale,
                    value_range: ValueRange::Auto,
                    visible: false,
                },
                OverlayDescriptor {
                    id: "final_elevation",
                    label: "Final elevation",
                    // Sprint 1A: `AuthoritativeFields.height` maps to this key.
                    source: OverlaySource::ScalarAuthoritative("height"),
                    palette: PaletteId::Grayscale,
                    value_range: ValueRange::Auto,
                    visible: true, // default on
                },
                OverlayDescriptor {
                    id: "flow_accumulation",
                    label: "Flow accumulation",
                    // Sprint 1A: add `accumulation: Option<ScalarField2D<f32>>`
                    // to DerivedCaches and keep this key in sync.
                    source: OverlaySource::ScalarDerived("accumulation"),
                    palette: PaletteId::Grayscale,
                    value_range: ValueRange::Auto,
                    visible: false,
                },
            ],
        }
    }

    /// Return a slice of all registered descriptors.
    pub fn all(&self) -> &[OverlayDescriptor] {
        &self.entries
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
        Self::sprint_0_defaults()
    }
}

// ─── tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // 1. Sprint 0 defaults contain exactly 3 entries.
    #[test]
    fn registry_has_3_sprint_0_defaults() {
        assert_eq!(OverlayRegistry::sprint_0_defaults().all().len(), 3);
    }

    // 2. All three default ids can be queried via by_id.
    #[test]
    fn by_id_queries_all_defaults() {
        let reg = OverlayRegistry::sprint_0_defaults();
        assert!(reg.by_id("initial_uplift").is_some());
        assert!(reg.by_id("final_elevation").is_some());
        assert!(reg.by_id("flow_accumulation").is_some());
    }

    // 3. Unknown id returns None.
    #[test]
    fn by_id_unknown_returns_none() {
        let reg = OverlayRegistry::sprint_0_defaults();
        assert!(reg.by_id("nope").is_none());
    }

    // 4. set_visibility flips the visible flag.
    #[test]
    fn set_visibility_changes_flag() {
        let mut reg = OverlayRegistry::sprint_0_defaults();
        // initial_uplift starts invisible
        assert!(!reg.by_id("initial_uplift").unwrap().visible);
        reg.set_visibility("initial_uplift", true);
        assert!(reg.by_id("initial_uplift").unwrap().visible);
    }

    // 5. visible_entries returns only visible ones.
    // Sprint 0 defaults: only final_elevation is visible → count == 1.
    #[test]
    fn visible_entries_filters() {
        let reg = OverlayRegistry::sprint_0_defaults();
        assert_eq!(reg.visible_entries().count(), 1);
    }

    // 6. Source field-key strings match the names Sprint 1A will use.
    //
    // Sprint 1A must keep these in sync:
    //   * "height"         → AuthoritativeFields.height
    //   * "initial_uplift" → DerivedCaches.initial_uplift
    //   * "accumulation"   → DerivedCaches.accumulation
    #[test]
    fn source_field_keys_match_sprint_1a_plan() {
        let reg = OverlayRegistry::sprint_0_defaults();

        // final_elevation → AuthoritativeFields.height
        let final_elev = reg.by_id("final_elevation").unwrap();
        assert_eq!(
            final_elev.source,
            OverlaySource::ScalarAuthoritative("height"),
            // Sprint 1A: keep in sync with AuthoritativeFields.height field name
        );

        // initial_uplift → DerivedCaches.initial_uplift
        let initial_uplift = reg.by_id("initial_uplift").unwrap();
        assert_eq!(
            initial_uplift.source,
            OverlaySource::ScalarDerived("initial_uplift"),
            // Sprint 1A: keep in sync with DerivedCaches.initial_uplift field name
        );

        // flow_accumulation → DerivedCaches.accumulation
        let flow_accum = reg.by_id("flow_accumulation").unwrap();
        assert_eq!(
            flow_accum.source,
            OverlaySource::ScalarDerived("accumulation"),
            // Sprint 1A: keep in sync with DerivedCaches.accumulation field name
        );
    }
}
