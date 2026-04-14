// §3.2 Visual Package — terrain + sea shader.
//
// All colors come from the Palette uniform buffer (group 0, binding 1).
// No RGB literals exist in this file. The Rust side populates the buffer
// from crates/render/src/palette.rs constants.

struct View {
    view_proj: mat4x4<f32>,
    // xyz = eye position; w is padding for std140 alignment.
    eye_pos: vec4<f32>,
}

struct Palette {
    deep_water:      vec4<f32>,
    shallow_water:   vec4<f32>,
    lowland:         vec4<f32>,
    midland:         vec4<f32>,
    highland:        vec4<f32>,
    river:           vec4<f32>,
    basin_accent:    vec4<f32>,
    overlay_neutral: vec4<f32>,
}

struct LightRig {
    // §3.2 A4 three-term rig.  Direction vectors point FROM the light TOWARD
    // the surface (negated from "light position" convention).
    key_dir:  vec4<f32>, // xyz = direction, w = intensity
    fill_dir: vec4<f32>, // xyz = direction, w = intensity
    ambient:  vec4<f32>, // rgb = color * intensity, a = scalar intensity
    // sea_level packed into x; y/z/w unused (keeps struct std140-aligned).
    sea_level: vec4<f32>,
}

@group(0) @binding(0) var<uniform> view:    View;
@group(0) @binding(1) var<uniform> palette: Palette;
@group(0) @binding(2) var<uniform> light:   LightRig;
@group(0) @binding(3) var          blue_noise:         texture_2d<f32>;
@group(0) @binding(4) var          blue_noise_sampler: sampler;

// ── Vertex stage ─────────────────────────────────────────────────────────────

struct VsIn {
    @location(0) position: vec3<f32>,
    @location(1) normal:   vec3<f32>,
    @location(2) uv:       vec2<f32>,
}

struct VsOut {
    @builtin(position) clip_pos:  vec4<f32>,
    @location(0)       world_pos: vec3<f32>,
    @location(1)       normal:    vec3<f32>,
    @location(2)       uv:        vec2<f32>,
}

@vertex
fn vs_terrain(input: VsIn) -> VsOut {
    var out: VsOut;
    out.clip_pos  = view.view_proj * vec4<f32>(input.position, 1.0);
    out.world_pos = input.position;
    out.normal    = normalize(input.normal);
    out.uv        = input.uv;
    return out;
}

// ── Fragment helpers ──────────────────────────────────────────────────────────

// §3.2 A1 height ramp: LOWLAND → MIDLAND → HIGHLAND over t ∈ [0, 1].
// Two-segment lerp with midpoint at t = 0.5.  Mirrors the Rust
// PaletteId::TerrainHeight implementation in crates/render/src/palette.rs.
fn terrain_height_ramp(t: f32) -> vec3<f32> {
    let tc = clamp(t, 0.0, 1.0);
    if tc < 0.5 {
        return mix(palette.lowland.rgb, palette.midland.rgb, tc / 0.5);
    } else {
        return mix(palette.midland.rgb, palette.highland.rgb, (tc - 0.5) / 0.5);
    }
}

// §3.2 A2 sea gradient: linearly interpolates shallow_water → deep_water by
// depth below sea level.  depth is clamped to [0, 0.3] (normalized height
// units), giving full deep color at 0.3 units below sea level.
fn sea_color(world_y: f32, sea_level: f32) -> vec3<f32> {
    let depth = clamp(sea_level - world_y, 0.0, 0.3);
    let t = depth / 0.3;
    return mix(palette.shallow_water.rgb, palette.deep_water.rgb, t);
}

// §3.2 A4 directional term: key + fill diffuse, returns a scalar multiplier.
fn light_terrain(n: vec3<f32>) -> f32 {
    let ndotl_key  = max(dot(n, -light.key_dir.xyz),  0.0) * light.key_dir.w;
    let ndotl_fill = max(dot(n, -light.fill_dir.xyz), 0.0) * light.fill_dir.w;
    return ndotl_key + ndotl_fill + light.ambient.a;
}

// ── Fragment stage ────────────────────────────────────────────────────────────

@fragment
fn fs_terrain(input: VsOut) -> @location(0) vec4<f32> {
    let sea_level = light.sea_level.x;
    let is_sea    = input.world_pos.y < sea_level;

    var base_rgb: vec3<f32>;
    if is_sea {
        base_rgb = sea_color(input.world_pos.y, sea_level);
    } else {
        // Map [sea_level, 1.0] → [0, 1]; heightfield is always in [0, 1].
        let t = (input.world_pos.y - sea_level) / max(1.0 - sea_level, 1e-3);
        base_rgb = terrain_height_ramp(t);
    }

    // Directional lighting only on terrain; sea surface stays flat-lit.
    let lit_rgb = select(
        base_rgb,
        base_rgb * (light.ambient.rgb + vec3<f32>(light_terrain(input.normal))),
        !is_sea,
    );

    // §3.2 B3 blue noise dither — breaks the height ramp's gradient banding.
    // DITHER_TILE = 8.0 repeats the 64×64 noise tile across each 1/8 of UV
    // space; combined with Repeat address mode this gives a pleasantly
    // random-looking dither across the full mesh. Amplitude is 1.0/255.0
    // centred on zero — ±½ LSB of the final 8-bit sRGB output, visible only
    // as reduced banding.
    let dither_tile = 8.0;
    let dither = (textureSample(blue_noise, blue_noise_sampler,
                                input.uv * dither_tile).r - 0.5) * (1.0 / 255.0);

    return vec4<f32>(lit_rgb + vec3<f32>(dither), 1.0);
}
