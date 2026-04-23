// Sprint 3.5.A c6 + c7 — hex surface fill pass with DD5 tonal-ramp elevation cue.
//
// All per-hex colour is sourced exclusively from per-instance data
// (`fill_color_rgba` packed RGBA8). There are NO RGB literals in this file.
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
// c8 will wire `coast_class_bits` and `river_mask_bits` for edge decorations.

// ── Uniforms ──────────────────────────────────────────────────────────────────

struct Uniforms {
    view_proj: mat4x4<f32>,
    // Reserved padding — used by later sub-commits if additional per-grid
    // uniforms (e.g. hex_size for non-unit local scaling) become needed.
    // DD5 tonal ramp is a pair of `const`s rather than uniforms so the
    // ramp is pick-once-and-commit at build time.
    _pad0: vec4<f32>,
    _pad1: vec4<f32>,
}

@group(0) @binding(0) var<uniform> u: Uniforms;

// ── Vertex + instance inputs ──────────────────────────────────────────────────

/// Unit-hex-local position (centre at origin, radius 1).
struct VertexInput {
    @location(0) local_xy: vec2<f32>,
}

/// Per-instance attributes — 32 bytes, matches `HexInstance` in `hex_surface.rs`.
struct InstanceInput {
    /// World-space hex centre (XZ plane, Y=0 in this commit; c7 uses elevation).
    @location(1) center_xy: vec2<f32>,
    /// Normalised [0, 1] elevation. Consumed by the tonal-ramp in c7.
    @location(2) elevation: f32,
    /// Packed RGBA8 dominant-biome fill colour (r | g<<8 | b<<16 | a<<24).
    /// Unpacked by `unpack_rgba8` below.
    @location(3) fill_color_rgba: u32,
    /// Packed coast-class bits. Low byte = HexCoastClass (0..=6); high bytes
    /// reserved. Populated by c8 from `DerivedCaches.hex_coast_class`.
    @location(4) coast_class_bits: u32,
    /// Packed river-flag bits. Low byte = river-flag mask; high bytes reserved.
    /// Populated by c8 from `HexAttributes.has_river`.
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

// ── VS/FS IO ──────────────────────────────────────────────────────────────────

struct VSOut {
    @builtin(position) clip_pos:   vec4<f32>,
    @location(0)       fill_color: vec4<f32>,
    @location(1)       elevation:  f32,
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
    // hex_size is baked as 1.0 in the unit mesh; the caller scales the grid
    // by populating `center_xy` from `axial_to_pixel(coord, hex_size)`.
    let hex_size = 1.0;

    // World-space position: instance centre (XZ) + scaled local vertex offset.
    // Y stays at 0.0 — DD5 explicitly does NOT extrude hexes in Z; the tonal
    // ramp in `fs_main` is the sole elevation cue. Z-extrusion would compete
    // with DD4 cliff decorations for the "vertical signal" budget.
    let world_xz = i.center_xy + v.local_xy * hex_size;
    let world_pos = vec4<f32>(world_xz.x, 0.0, world_xz.y, 1.0);

    var out: VSOut;
    out.clip_pos   = u.view_proj * world_pos;
    out.fill_color = unpack_rgba8(i.fill_color_rgba);
    out.elevation  = i.elevation;
    return out;
}

// ── Fragment shader ───────────────────────────────────────────────────────────

@fragment
fn fs_main(in: VSOut) -> @location(0) vec4<f32> {
    // DD5 tonal ramp: scale biome fill RGB by an elevation-derived factor.
    // Alpha is preserved — the ramp never affects transparency / compositing.
    let factor = tonal_factor(in.elevation);
    let rgb = in.fill_color.rgb * factor;
    return vec4<f32>(rgb, in.fill_color.a);
}
