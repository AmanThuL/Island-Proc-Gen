// §3.2 Sprint 1A overlay pass — paints a baked-RGBA texture over the terrain.
//
// All colours come from the baked texture (per-cell RGBA8 uploaded from
// Rust's palette::sample). NO RGB/hex literals appear in this file — the
// overlay_wgsl_has_no_literal_colors test in overlay_render.rs enforces this.

// ── View uniform (group 0, binding 0) — same layout as terrain.wgsl::View ──
// Must stay in sync with TerrainRenderer's ViewUniform struct.
struct View {
    view_proj: mat4x4<f32>,
    // xyz = eye position; w is padding for std140 alignment.
    eye_pos: vec4<f32>,
}

// ── OverlayUniform (group 1, binding 0) — per-descriptor alpha + padding ───
// Only .x is used (global alpha for this overlay). The remaining xyz are
// padding to satisfy the 16-byte minimum uniform alignment (std140 rule).
struct OverlayUniform {
    alpha: vec4<f32>,
}

@group(0) @binding(0) var<uniform> view:            View;
@group(1) @binding(0) var<uniform> overlay_uniform: OverlayUniform;
@group(1) @binding(1) var          overlay_tex:     texture_2d<f32>;
@group(1) @binding(2) var          overlay_sampler: sampler;

// ── Vertex input — same layout as terrain.wgsl TerrainVertex ──────────────
// location 0: position (Float32x3)
// location 1: normal   (Float32x3)
// location 2: uv       (Float32x2)
struct VsIn {
    @location(0) position: vec3<f32>,
    @location(1) normal:   vec3<f32>,
    @location(2) uv:       vec2<f32>,
}

struct VsOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0)       uv:       vec2<f32>,
}

@vertex
fn vs_overlay(input: VsIn) -> VsOut {
    var out: VsOut;
    out.clip_pos = view.view_proj * vec4<f32>(input.position, 1.0);
    out.uv       = input.uv;
    return out;
}

// ── Fragment stage ────────────────────────────────────────────────────────────
// Samples the baked RGBA texture and applies the per-overlay alpha multiplier.
// The pipeline uses SrcAlpha / OneMinusSrcAlpha blending, so the fragment
// alpha (s.a * overlay_uniform.alpha.x) drives how much the overlay covers
// the terrain underneath.

@fragment
fn fs_overlay(input: VsOut) -> @location(0) vec4<f32> {
    let s = textureSample(overlay_tex, overlay_sampler, input.uv);
    return vec4<f32>(s.rgb, s.a * overlay_uniform.alpha.x);
}
