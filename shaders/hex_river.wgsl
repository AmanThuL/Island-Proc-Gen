// Sprint 3.5.B c4 — hex river polyline renderer.
//
// Renders river threads as two-segment polylines (entry_midpoint → hex_center
// → exit_midpoint) approximated as flat quads with thickness derived from the
// river width bucket. Per-instance data carries the hex centre, edge indices
// (packed as bits in a single u32), and a width bucket.
//
// All colour is sourced from the uniform river_color field — no RGB literals
// appear in this file. The §3.2 literal-colour guard covers this shader.
//
// Depth: (write=false, compare=Always) so river threads draw on top of the
// hex surface fill pass. Alpha blending is enabled on the colour target.

// ── Uniforms ──────────────────────────────────────────────────────────────────
//
// 112 bytes total, explicitly padded so the repr(C) Rust struct and the WGSL
// struct span are identical byte-for-byte.
//
// | Field        | Offset  | Size | Notes                                |
// |--------------|---------|------|--------------------------------------|
// | view_proj    |   0     |  64  | mat4x4<f32>                          |
// | hex_size     |  64     |   4  | world-space centre-to-vertex radius  |
// | _pad0a/b/c   |  68     |  12  | three f32 scalars, NOT vec3<f32>     |
// | river_color  |  80     |  16  | vec4<f32> RGBA river tint            |
// | _pad1        |  96     |  16  | reserved                             |
//
// Total: 112 bytes.
//
// Three f32 scalars for _pad0 (NOT a vec3<f32>) — WGSL's 16-byte alignment on
// vec3 would silently push the struct to 128 bytes. Three f32 scalars pack at
// 4-byte alignment, keeping the layout aligned with the repr(C) Rust side.
// Verified by `uniforms_buffer_size_matches_wgsl_layout`.

struct Uniforms {
    view_proj: mat4x4<f32>,   //  0..64 bytes
    hex_size: f32,            // 64..68 bytes — world-space radius
    _pad0a: f32,              // 68..72 bytes
    _pad0b: f32,              // 72..76 bytes
    _pad0c: f32,              // 76..80 bytes
    river_color: vec4<f32>,   // 80..96 bytes — RGBA river colour from palette
    _pad1: vec4<f32>,         // 96..112 bytes — reserved
}

@group(0) @binding(0) var<uniform> u: Uniforms;

// ── Vertex + instance inputs ──────────────────────────────────────────────────

/// Unit-segment local position.
///   local_pos.x ∈ {0.0, 1.0} — start vs end of this segment half
///   local_pos.y ∈ {-0.5, 0.5} — left vs right of centreline
struct VertexInput {
    @location(0) local_pos:  vec2<f32>,  // (t, n) in segment space
    @location(1) segment_id: u32,        // 0 = entry half, 1 = exit half
}

/// Per-instance attributes — 12 bytes packed.
///
/// `hex_center_xy` — world-space hex centre on the XZ plane.
/// `edges_and_width_bits` — packed u32:
///   bits  0..7  = entry_edge  (HexEdge discriminant 0..=5)
///   bits  8..15 = exit_edge   (HexEdge discriminant 0..=5)
///   bits 16..23 = width_bucket (RiverWidth discriminant 0..=2)
///   bits 24..31 = _pad
///
/// The Rust side packs these via `HexRiverInstance::edges_and_width_bits()`.
struct InstanceInput {
    @location(2) hex_center_xy:     vec2<f32>,  //  8 bytes
    @location(3) edges_and_width_bits: u32,     //  4 bytes
}

// ── Thickness constants (DD3, pick-once-and-commit) ───────────────────────────

/// Relative half-width for Small rivers (RiverWidth::Small = 0).
/// Factor applied to hex_size. Full thickness = 2 * THICK_SMALL * hex_size.
const THICK_SMALL: f32 = 0.08;

/// Relative half-width for Medium rivers (RiverWidth::Medium = 1).
const THICK_MEDIUM: f32 = 0.15;

/// Relative half-width for Main rivers (RiverWidth::Main = 2).
const THICK_MAIN: f32 = 0.25;

// ── APOTHEM_RATIO ─────────────────────────────────────────────────────────────

/// Ratio: apothem / hex_size = sqrt(3) / 2 ≈ 0.866025.
/// The edge midpoint lies at `hex_center + apothem * edge_direction`.
const APOTHEM_RATIO: f32 = 0.8660254037844387;

// ── VS/FS IO ──────────────────────────────────────────────────────────────────

struct VSOut {
    @builtin(position) clip_pos: vec4<f32>,
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Edge angle (radians): `edge_index * PI/3`.
/// E=0° (index 0), NE=60° (index 1), NW=120° (index 2),
/// W=180° (index 3), SW=240° (index 4), SE=300° (index 5).
fn edge_angle_rad(edge_index: u32) -> f32 {
    return f32(edge_index) * 1.0471975511965976; // PI/3
}

/// World-space edge midpoint for a given hex center and edge index.
/// Distance = apothem = hex_size * sqrt(3) / 2.
fn edge_midpoint_world(hex_center: vec2<f32>, edge_index: u32, hex_size: f32) -> vec2<f32> {
    let angle = edge_angle_rad(edge_index);
    let apothem = hex_size * APOTHEM_RATIO;
    return hex_center + vec2<f32>(cos(angle), sin(angle)) * apothem;
}

/// Half-thickness in world units for a width bucket (0=Small, 1=Medium, 2=Main).
fn half_thickness(width_bucket: u32, hex_size: f32) -> f32 {
    if width_bucket == 0u {
        return THICK_SMALL * hex_size;
    } else if width_bucket == 1u {
        return THICK_MEDIUM * hex_size;
    } else {
        return THICK_MAIN * hex_size;
    }
}

// ── Vertex shader ─────────────────────────────────────────────────────────────

@vertex
fn vs_main(v: VertexInput, inst: InstanceInput) -> VSOut {
    let hex_size = u.hex_size;

    // Unpack edge indices and width bucket from the packed u32.
    let entry_edge  = (inst.edges_and_width_bits       ) & 0xFFu;
    let exit_edge   = (inst.edges_and_width_bits >>  8u) & 0xFFu;
    let width_bucket = (inst.edges_and_width_bits >> 16u) & 0xFFu;

    // Compute entry and exit edge midpoints.
    let entry_mid = edge_midpoint_world(inst.hex_center_xy, entry_edge, hex_size);
    let exit_mid  = edge_midpoint_world(inst.hex_center_xy, exit_edge,  hex_size);
    let center    = inst.hex_center_xy;

    // Half-thickness in world units.
    let ht = half_thickness(width_bucket, hex_size);

    // Segment endpoints:
    //   segment_id == 0: entry_mid → center
    //   segment_id == 1: center → exit_mid
    var seg_start: vec2<f32>;
    var seg_end:   vec2<f32>;
    if v.segment_id == 0u {
        seg_start = entry_mid;
        seg_end   = center;
    } else {
        seg_start = center;
        seg_end   = exit_mid;
    }

    // Segment direction + perpendicular.
    let dir = seg_end - seg_start;
    let dir_len = length(dir);
    var norm_dir: vec2<f32>;
    if dir_len < 1e-6 {
        // Degenerate segment (entry == exit, same-side river): fall back to
        // a unit direction so we don't produce NaN positions.
        norm_dir = vec2<f32>(1.0, 0.0);
    } else {
        norm_dir = dir / dir_len;
    }
    // Perpendicular (rotate 90° CCW).
    let perp = vec2<f32>(-norm_dir.y, norm_dir.x);

    // local_pos.x: 0 = seg_start, 1 = seg_end (along segment)
    // local_pos.y: -0.5 = right edge, +0.5 = left edge (across segment)
    let along = mix(seg_start, seg_end, v.local_pos.x);
    let world_xz = along + perp * (v.local_pos.y * ht * 2.0);

    // Lift to Y=0 world plane (same as hex surface).
    let world_pos = vec4<f32>(world_xz.x, 0.0, world_xz.y, 1.0);

    var out: VSOut;
    out.clip_pos = u.view_proj * world_pos;
    return out;
}

// ── Fragment shader ───────────────────────────────────────────────────────────

@fragment
fn fs_main(in: VSOut) -> @location(0) vec4<f32> {
    // Output river colour with full opacity. Alpha blending is enabled on the
    // pipeline colour target, so overlapping segments blend naturally.
    return u.river_color;
}
