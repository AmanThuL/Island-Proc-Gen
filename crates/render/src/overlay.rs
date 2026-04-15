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

use island_core::{
    field::{MaskField2D, ScalarField2D},
    world::WorldState,
};

use crate::palette::PaletteId;

// ─── ValueRange ──────────────────────────────────────────────────────────────

/// Governs how to map a field's raw value range to the `[0, 1]` palette input.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ValueRange {
    /// Derive the mapping from the actual field min/max at render time.
    Auto,
    /// Fixed `[lo, hi]` mapping regardless of the field's actual range.
    Fixed(f32, f32),
    /// Auto-ranged on `log(value + 1)`. Used for flow accumulation where
    /// the raw distribution spans several decades.
    LogCompressed,
}

impl ValueRange {
    /// Resolve this range to a concrete `(lo, hi)` pair.
    ///
    /// * `Auto` → returns `(field_min, field_max)` from the supplied values.
    /// * `Fixed(lo, hi)` → returns `(lo, hi)` unchanged.
    /// * `LogCompressed` → returns `(ln(1+field_min), ln(1+field_max))`.
    pub fn resolve(self, field_min: f32, field_max: f32) -> (f32, f32) {
        match self {
            ValueRange::Auto => (field_min, field_max),
            ValueRange::Fixed(lo, hi) => (lo, hi),
            ValueRange::LogCompressed => (
                (1.0 + field_min.max(0.0)).ln(),
                (1.0 + field_max.max(0.0)).ln(),
            ),
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
    /// Return the Sprint 1A overlay registry wired to the 6 real `DerivedCaches` fields.
    ///
    /// `final_elevation` reads `z_filled` (not `height`) — `authoritative.height` stores
    /// `z_raw` pre-pit-fill; the render path and flow routing both see `z_filled`.
    pub fn sprint_1a_defaults() -> Self {
        Self {
            entries: vec![
                OverlayDescriptor {
                    id: "initial_uplift",
                    label: "Initial uplift",
                    source: OverlaySource::ScalarDerived("initial_uplift"),
                    palette: PaletteId::Grayscale,
                    value_range: ValueRange::Auto,
                    visible: false,
                },
                OverlayDescriptor {
                    id: "final_elevation",
                    label: "Final elevation",
                    source: OverlaySource::ScalarDerived("z_filled"),
                    palette: PaletteId::TerrainHeight,
                    value_range: ValueRange::Auto,
                    visible: true,
                },
                OverlayDescriptor {
                    id: "slope",
                    label: "Slope",
                    source: OverlaySource::ScalarDerived("slope"),
                    palette: PaletteId::Viridis,
                    value_range: ValueRange::Auto,
                    visible: false,
                },
                OverlayDescriptor {
                    id: "flow_accumulation",
                    label: "Flow accumulation",
                    source: OverlaySource::ScalarDerived("accumulation"),
                    palette: PaletteId::Turbo,
                    value_range: ValueRange::LogCompressed,
                    visible: false,
                },
                OverlayDescriptor {
                    id: "basin_partition",
                    label: "Basin partition",
                    source: OverlaySource::ScalarDerived("basin_id"),
                    palette: PaletteId::Categorical,
                    value_range: ValueRange::Auto,
                    visible: false,
                },
                OverlayDescriptor {
                    id: "river_network",
                    label: "River network",
                    source: OverlaySource::Mask("river_mask"),
                    palette: PaletteId::BinaryBlue,
                    value_range: ValueRange::Fixed(0.0, 1.0),
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
        Self::sprint_1a_defaults()
    }
}

// ─── ResolvedField ────────────────────────────────────────────────────────────

/// Typed borrow of a `WorldState` field — returned by [`resolve_scalar_source`].
///
/// Confines all `&'static str` field-key dispatch to this file. `overlay_render`
/// receives this typed handle and never sees the string keys.
pub(crate) enum ResolvedField<'a> {
    F32(&'a ScalarField2D<f32>),
    U32(&'a ScalarField2D<u32>),
    Mask(&'a MaskField2D),
}

/// Resolve an [`OverlaySource`] to a typed field borrow from `world`.
///
/// Returns `None` if the field has not been populated yet (e.g. the pipeline
/// has not run) or if the source key is unrecognised for Sprint 1A.
/// All `&'static str` dispatch lives **only** here — callers work with the
/// typed [`ResolvedField`] enum.
pub(crate) fn resolve_scalar_source<'w>(
    world: &'w WorldState,
    source: OverlaySource,
) -> Option<ResolvedField<'w>> {
    use OverlaySource::*;
    match source {
        ScalarDerived("initial_uplift") => world
            .derived
            .initial_uplift
            .as_ref()
            .map(ResolvedField::F32),
        ScalarDerived("z_filled") => world.derived.z_filled.as_ref().map(ResolvedField::F32),
        ScalarDerived("slope") => world.derived.slope.as_ref().map(ResolvedField::F32),
        ScalarDerived("accumulation") => {
            world.derived.accumulation.as_ref().map(ResolvedField::F32)
        }
        ScalarDerived("basin_id") => world.derived.basin_id.as_ref().map(ResolvedField::U32),
        Mask("river_mask") => world.derived.river_mask.as_ref().map(ResolvedField::Mask),
        // Sprint 1A has no authoritative / baked scalar sources wired, and
        // no vector overlays. Return None so the renderer silently skips them
        // rather than panicking on an unrecognised key.
        _ => None,
    }
}

// ─── tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_has_6_sprint_1a_defaults() {
        assert_eq!(OverlayRegistry::sprint_1a_defaults().all().len(), 6);
    }

    #[test]
    fn by_id_queries_all_defaults() {
        let reg = OverlayRegistry::sprint_1a_defaults();
        assert!(reg.by_id("initial_uplift").is_some());
        assert!(reg.by_id("final_elevation").is_some());
        assert!(reg.by_id("slope").is_some());
        assert!(reg.by_id("flow_accumulation").is_some());
        assert!(reg.by_id("basin_partition").is_some());
        assert!(reg.by_id("river_network").is_some());
    }

    #[test]
    fn by_id_unknown_returns_none() {
        let reg = OverlayRegistry::sprint_1a_defaults();
        assert!(reg.by_id("nope").is_none());
    }

    #[test]
    fn set_visibility_changes_flag() {
        let mut reg = OverlayRegistry::sprint_1a_defaults();
        assert!(!reg.by_id("initial_uplift").unwrap().visible);
        reg.set_visibility("initial_uplift", true);
        assert!(reg.by_id("initial_uplift").unwrap().visible);
    }

    // Sprint 1A defaults: only final_elevation is visible → count == 1.
    #[test]
    fn visible_entries_filters() {
        let reg = OverlayRegistry::sprint_1a_defaults();
        assert_eq!(reg.visible_entries().count(), 1);
    }

    #[test]
    fn source_field_keys_match_sprint_1a_plan() {
        let reg = OverlayRegistry::sprint_1a_defaults();

        assert_eq!(
            reg.by_id("initial_uplift").unwrap().source,
            OverlaySource::ScalarDerived("initial_uplift"),
        );
        // §7 acceptance criterion: z_filled, NOT height.
        assert_eq!(
            reg.by_id("final_elevation").unwrap().source,
            OverlaySource::ScalarDerived("z_filled"),
        );
        assert_eq!(
            reg.by_id("slope").unwrap().source,
            OverlaySource::ScalarDerived("slope"),
        );
        assert_eq!(
            reg.by_id("flow_accumulation").unwrap().source,
            OverlaySource::ScalarDerived("accumulation"),
        );
        assert_eq!(
            reg.by_id("basin_partition").unwrap().source,
            OverlaySource::ScalarDerived("basin_id"),
        );
        assert_eq!(
            reg.by_id("river_network").unwrap().source,
            OverlaySource::Mask("river_mask"),
        );
    }

    // Dedicated guard: future refactorers must not silently revert to height.
    #[test]
    fn final_elevation_not_authoritative_height() {
        let reg = OverlayRegistry::sprint_1a_defaults();
        let d = reg.by_id("final_elevation").unwrap();
        assert_ne!(d.source, OverlaySource::ScalarAuthoritative("height"));
        assert_eq!(d.source, OverlaySource::ScalarDerived("z_filled"));
    }

    #[test]
    fn flow_accumulation_uses_log_compressed() {
        let reg = OverlayRegistry::sprint_1a_defaults();
        assert_eq!(
            reg.by_id("flow_accumulation").unwrap().value_range,
            ValueRange::LogCompressed,
        );
    }

    #[test]
    fn river_network_uses_binary_blue() {
        let reg = OverlayRegistry::sprint_1a_defaults();
        let d = reg.by_id("river_network").unwrap();
        assert_eq!(d.palette, PaletteId::BinaryBlue);
        assert_eq!(d.source, OverlaySource::Mask("river_mask"));
    }

    #[test]
    fn log_compressed_resolve() {
        let (lo, hi) = ValueRange::LogCompressed.resolve(0.0, std::f32::consts::E - 1.0);
        assert!((lo - 0.0).abs() < 1e-5, "lo={lo}");
        assert!((hi - 1.0).abs() < 1e-4, "hi={hi}");
    }

    #[test]
    fn log_compressed_clamps_negative_min() {
        // Negative field_min treated as 0 before ln.
        let (lo, _hi) = ValueRange::LogCompressed.resolve(-5.0, 10.0);
        assert!((lo - 0.0).abs() < 1e-5, "lo={lo}");
    }
}
