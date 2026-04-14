// §3.2 A3 procedural sky gradient. Colours are supplied by the Sky uniform
// buffer (@group(0) @binding(0)) which is populated from
// render::palette::{SKY_HORIZON, SKY_ZENITH} — there are NO RGB literals in
// this file. The same §3.2 grep that guards terrain.wgsl guards this one.

struct Sky {
    horizon: vec4<f32>, // bottom of screen
    zenith:  vec4<f32>, // top of screen
}
@group(0) @binding(0) var<uniform> sky: Sky;

struct VsOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0)       uv:       vec2<f32>,
}

// Full-screen triangle via vertex_index. Vertices (-1,-1), (3,-1), (-1,3)
// overdraw the NDC rectangle; the rasterizer clips to the viewport. This is
// the standard "one-triangle clear" trick — avoids a VBO and shares one
// interpolated attribute (uv) across the whole frame.
@vertex
fn vs_sky(@builtin(vertex_index) vi: u32) -> VsOut {
    let x = f32((vi << 1u) & 2u) * 2.0 - 1.0;
    let y = f32(vi & 2u) * 2.0 - 1.0;
    var out: VsOut;
    out.clip_pos = vec4<f32>(x, y, 1.0, 1.0);
    out.uv       = vec2<f32>(x * 0.5 + 0.5, y * 0.5 + 0.5);
    return out;
}

// Vertical gradient: uv.y = 0 at the bottom of the screen (horizon), 1 at
// the top (zenith). Clamped so values outside [0, 1] don't overshoot.
@fragment
fn fs_sky(in: VsOut) -> @location(0) vec4<f32> {
    let t   = clamp(in.uv.y, 0.0, 1.0);
    let rgb = mix(sky.horizon.rgb, sky.zenith.rgb, t);
    return vec4<f32>(rgb, 1.0);
}
