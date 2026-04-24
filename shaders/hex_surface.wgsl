// Sprint 3.5.A c6 + c7 + 3.5.C c3 — hex surface fill pass with DD5 tonal-ramp
// elevation cue and DD4 coast-class edge-band tinting.
//
// All per-hex colour is sourced exclusively from per-instance data
// (`fill_color_rgba` packed RGBA8) or from the `coast_class_tints` uniform
// array. There are NO RGB literals in this file.
// The same §3.2 grep that guards terrain.wgsl and sky.wgsl guards this one.
//
// DD5 tonal ramp (pick-once-and-commit per Sprint 3.5 §2 DD5):
//   The fragment multiplies the biome fill RGB by a scalar factor derived from
//   per-instance `elevation` ∈ [0, 1]. Lower elevations darken the fill;
//   higher elevations run at the full biome-identity colour. The ramp stays
//   multiplicative on RGB only; alpha is preserved. Biome identity never
//   drops — a "darker LowlandForest" still reads as LowlandForest, exactly
//   per DD5's "tonal ramp composes cleanly with biome fill" clause.
//
// DD4 edge-band tinting (Sprint 3.5.C c3):
//   Fragments in the outer `edge_band_start`..1.0 radial zone (measured by
//   `length(local_xy)` where the unit-hex corners sit at radius 1) receive a
//   class-specific tint from `u.coast_class_tints[class - 2]`. Inland (0) and
//   OpenOcean (1) are excluded; both get no edge tint. The tint blends over the
//   post-tonal-ramp fill colour via `mix(fill, tint.rgb, t * tint.a)` where
//   `t` is the normalised distance within the edge band [0, 1]. This preserves
//   biome identity outside the edge band and produces a clear class signal at
//   the hex perimeter without covering the fill interior.
//
//   `edge_band_start = 0.82` is the pick-once value. At radius 0.82 the band
//   covers roughly the outer 18 % of the hex area (visually ~2-3 px thick at
//   typical zoom levels). Narrower (0.90) would be almost invisible; wider
//   (0.70) would overwhelm the biome fill. Documented deferral: per-edge glyph,
//   dash, stipple, and Z-extrusion effects are Sprint 3.5.F polish.

// ── Uniforms ──────────────────────────────────────────────────────────────────

struct Uniforms {
    view_proj: mat4x4<f32>,   //   0..64 bytes
    // hex_size carries the world-space centre-to-vertex radius of the hex grid.
    // c8 sets this from `HexGrid.hex_size` via `update_view_projection`. All
    // vertex positions are computed as `center_xy + local_xy * hex_size`, so
    // forgetting to set this produces 1.0-world-unit hexes regardless of the
    // actual grid scale.
    hex_size: f32,            //  64..68 bytes
    // Padding to align `coast_class_tints` to 80-byte (16-byte boundary).
    // Three f32 scalars (not a vec3<f32>) because WGSL imposes 16-byte
    // alignment on vec3 types, which would force the struct to 112 bytes and
    // diverge from the Rust repr(C) layout. Three f32 scalars pack naturally
    // at 4-byte alignment. The 176-byte lock is verified by
    // `uniforms_buffer_size_matches_wgsl_layout` (parses this file via naga
    // and asserts struct span == 176).
    _pad0a: f32,              //  68..72 bytes
    _pad0b: f32,              //  72..76 bytes
    _pad0c: f32,              //  76..80 bytes
    // DD4 edge-tint colours. Indexed [class - 2] for class ∈ 2..=6.
    // Each entry is vec4<f32>(r, g, b, alpha). Alpha controls blend intensity
    // in the edge band. Inland (0) and OpenOcean (1) skip the tint path.
    //   [0] Beach          (HexCoastClass::Beach        = discriminant 2)
    //   [1] RockyHeadland  (HexCoastClass::RockyHeadland = discriminant 3)
    //   [2] Estuary        (HexCoastClass::Estuary      = discriminant 4)
    //   [3] Cliff          (HexCoastClass::Cliff        = discriminant 5)
    //   [4] LavaDelta      (HexCoastClass::LavaDelta    = discriminant 6)
    coast_class_tints: array<vec4<f32>, 5>,  //  80..160 bytes — 5 × 16
    _pad1: vec4<f32>,         // 160..176 bytes — pads to 176-byte struct
}

@group(0) @binding(0) var<uniform> u: Uniforms;

// ── Vertex + instance inputs ──────────────────────────────────────────────────

/// Unit-hex-local position (centre at origin, radius 1).
struct VertexInput {
    @location(0) local_xy: vec2<f32>,
}

/// Per-instance attributes — 32 bytes, matches `HexInstance` in `hex_surface.rs`.
struct InstanceInput {
    /// World-space hex centre (XZ plane, Y=0).
    @location(1) center_xy: vec2<f32>,
    /// Normalised [0, 1] elevation. Consumed by the tonal-ramp in fs_main.
    @location(2) elevation: f32,
    /// Packed RGBA8 dominant-biome fill colour (r | g<<8 | b<<16 | a<<24).
    /// Unpacked by `unpack_rgba8` below.
    @location(3) fill_color_rgba: u32,
    /// Packed coast-class bits. Low byte = HexCoastClass (0..=6); high bytes
    /// reserved. Populated by `build_hex_instances` from
    /// `DerivedCaches.hex_coast_class`.
    @location(4) coast_class_bits: u32,
    /// Packed river-flag bits. Low byte = river-flag mask; high bytes reserved.
    @location(5) river_mask_bits: u32,
    /// Padding — aligns the instance to 32 bytes.
    @location(6) _pad0: u32,
    @location(7) _pad1: u32,
}

// ── Tonal-ramp constants (DD5, pick-once-and-commit) ─────────────────────────

/// Multiplicative RGB factor at elevation = 0 (sea level). Locks the
/// darkest base-fill shade. Keep in `[0.4, 0.7]`; tighter values saturate
/// into the biome identity, looser values wash out coastal readability.
const TONAL_MIN: f32 = 0.55;

/// Multiplicative RGB factor at elevation = 1 (highest peaks). 1.0 keeps
/// the base fill un-brightened; values > 1.0 would require a tone-mapper
/// to stay in gamut.
const TONAL_MAX: f32 = 1.0;

// ── DD4 edge-band constant (Sprint 3.5.C c3, pick-once-and-commit) ───────────

/// Normalised hex-local radius at which the edge tint band begins. Fragments
/// with `length(local_xy) > EDGE_BAND_START` are in the edge zone and receive
/// a coast-class-specific tint. Value 0.82 covers roughly the outer 18 % of
/// hex area — visible as a clear ~2-3 px band at typical camera distances
/// without overwhelming the biome fill.
///
/// Locked here to document the Sprint 3.5.C pick. Adjust only after visual
/// review; tighter (0.90) reduces visibility; wider (0.70) swamps the interior.
const EDGE_BAND_START: f32 = 0.82;

// ── VS/FS IO ──────────────────────────────────────────────────────────────────

struct VSOut {
    @builtin(position) clip_pos:   vec4<f32>,
    @location(0)       fill_color: vec4<f32>,
    @location(1)       elevation:  f32,
    /// Unit-hex-local position, interpolated from vertex corners so fs_main
    /// can compute `length(local_xy)` for the edge-band distance heuristic.
    /// Centre = (0, 0); corners lie at radius 1.
    @location(2)       local_xy:   vec2<f32>,
    /// Coast-class discriminant (0..=6), passed from instance data.
    /// Inland = 0; OpenOcean = 1; Beach = 2; RockyHeadland = 3;
    /// Estuary = 4; Cliff = 5; LavaDelta = 6.
    @location(3)       coast_cls: u32,
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Unpack an RGBA8 u32 into a linear vec4<f32> in [0, 1].
/// Layout: bits 0..7 = R, 8..15 = G, 16..23 = B, 24..31 = A.
fn unpack_rgba8(packed: u32) -> vec4<f32> {
    let r = f32(packed        & 0xffu) / 255.0;
    let g = f32((packed >> 8u)  & 0xffu) / 255.0;
    let b = f32((packed >> 16u) & 0xffu) / 255.0;
    let a = f32((packed >> 24u) & 0xffu) / 255.0;
    return vec4<f32>(r, g, b, a);
}

/// DD5 tonal ramp: linear interpolation between `TONAL_MIN` and `TONAL_MAX`
/// over `elevation ∈ [0, 1]`. Returns a scalar RGB multiplier; alpha is
/// always preserved by the caller.
fn tonal_factor(elevation: f32) -> f32 {
    let t = clamp(elevation, 0.0, 1.0);
    return mix(TONAL_MIN, TONAL_MAX, t);
}

// ── Vertex shader ─────────────────────────────────────────────────────────────

@vertex
fn vs_main(v: VertexInput, i: InstanceInput) -> VSOut {
    // hex_size is the world-space centre-to-vertex radius sourced from the
    // Uniforms struct. c8 sets this via `HexSurfaceRenderer::update_hex_size`.
    // `new()` initialises it to 1.0 to preserve pre-c8 test behaviour.
    let hex_size = u.hex_size;

    // World-space position: instance centre (XZ) + scaled local vertex offset.
    // Y stays at 0.0 — DD5 explicitly does NOT extrude hexes in Z; the tonal
    // ramp in `fs_main` is the sole elevation cue. Z-extrusion would compete
    // with DD4 cliff decorations for the "vertical signal" budget.
    let world_xz = i.center_xy + v.local_xy * hex_size;
    let world_pos = vec4<f32>(world_xz.x, 0.0, world_xz.y, 1.0);

    var out: VSOut;
    out.clip_pos    = u.view_proj * world_pos;
    out.fill_color  = unpack_rgba8(i.fill_color_rgba);
    out.elevation   = i.elevation;
    // Pass the unit-hex-local position so fs_main can compute edge distance.
    out.local_xy   = v.local_xy;
    // Low byte of coast_class_bits is the HexCoastClass discriminant.
    // `class` is a WGSL reserved keyword; use `coast_cls` instead.
    out.coast_cls  = i.coast_class_bits & 0xffu;
    return out;
}

// ── Fragment shader ───────────────────────────────────────────────────────────

@fragment
fn fs_main(in: VSOut) -> @location(0) vec4<f32> {
    // DD5 tonal ramp: scale biome fill RGB by an elevation-derived factor.
    // Alpha is preserved — the ramp never affects transparency / compositing.
    let factor = tonal_factor(in.elevation);
    var rgb = in.fill_color.rgb * factor;

    // DD4 edge-band tinting: apply a coast-class-specific tint at the hex
    // perimeter for classes Beach (2) through LavaDelta (6).
    // Inland (0) and OpenOcean (1) are skipped entirely.
    // NOTE: `class` is a WGSL reserved keyword; the field is named `coast_cls`.
    if in.coast_cls >= 2u && in.coast_cls <= 6u {
        // `length(local_xy)` is the radial distance from the hex centre in
        // unit-hex space. Centre = 0.0; corners = 1.0; flat-edge apothem ≈ 0.87.
        // Fragments with radius > EDGE_BAND_START are in the tint band.
        let local_radius = length(in.local_xy);
        if local_radius > EDGE_BAND_START {
            // Normalise to [0, 1] across the band width.
            let t = (local_radius - EDGE_BAND_START) / (1.0 - EDGE_BAND_START);
            // coast_class_tints indexed [coast_cls - 2] for coast_cls ∈ 2..=6.
            let tint = u.coast_class_tints[in.coast_cls - 2u];
            rgb = mix(rgb, tint.rgb, t * tint.a);
        }
    }

    return vec4<f32>(rgb, in.fill_color.a);
}
