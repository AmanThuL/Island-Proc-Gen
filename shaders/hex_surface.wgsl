// Sprint 3.5.A c6 — hex surface fill pass.
//
// All per-hex colour is sourced exclusively from per-instance data
// (`fill_color_rgba` packed RGBA8). There are NO RGB literals in this file.
// The same §3.2 grep that guards terrain.wgsl and sky.wgsl guards this one.
//
// c7 will add the tonal-ramp elevation shading using `elevation`.
// c8 will wire `coast_class_bits` and `river_mask_bits` for edge decorations.

// ── Uniforms ──────────────────────────────────────────────────────────────────

struct Uniforms {
    view_proj: mat4x4<f32>,
    // Sprint 3.5.A c6 placeholder — additional uniforms (hex_size, tonal ramp
    // params, etc.) land in c7.
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

// ── VS/FS IO ──────────────────────────────────────────────────────────────────

struct VSOut {
    @builtin(position) clip_pos:   vec4<f32>,
    @location(0)       fill_color: vec4<f32>,
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

// ── Vertex shader ─────────────────────────────────────────────────────────────

@vertex
fn vs_main(v: VertexInput, i: InstanceInput) -> VSOut {
    // hex_size is baked as 1.0 in the unit mesh; the caller scales the grid
    // by populating `center_xy` from `axial_to_pixel(coord, hex_size)`.
    // c7 introduces a per-grid hex_size uniform when the tonal-ramp geometry
    // is finalised.
    let hex_size = 1.0;

    // World-space position: instance centre (XZ) + scaled local vertex offset.
    // Y stays at 0.0 in c6. c7 will use `elevation` to offset Y for the tonal
    // shading, if DD5 decides on a slight vertical displacement. Sprint 3.5.A
    // DD5 explicitly does NOT extrude hexes in Z — elevation is for shading only.
    let world_xz = i.center_xy + v.local_xy * hex_size;
    let world_pos = vec4<f32>(world_xz.x, 0.0, world_xz.y, 1.0);

    var out: VSOut;
    out.clip_pos   = u.view_proj * world_pos;
    out.fill_color = unpack_rgba8(i.fill_color_rgba);
    return out;
}

// ── Fragment shader ───────────────────────────────────────────────────────────

@fragment
fn fs_main(in: VSOut) -> @location(0) vec4<f32> {
    // c6: direct fill colour pass-through.
    // c7 will blend in elevation-derived tonal ramp here.
    return in.fill_color;
}
