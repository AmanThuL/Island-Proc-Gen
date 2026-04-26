// hillslope_diffusion.wgsl — Sprint 4.E
//
// One explicit-Euler substep of ∂z/∂t = D · ∇²z applied to a 2-D height field.
//
// The CPU equivalent is `hillslope_diffusion_kernel` in
// `crates/sim/src/geomorph/hillslope.rs`. Math is identical; the only
// difference is that the CPU uses a substep loop with `mem::swap`, while the
// GPU runs one dispatch per substep and the Rust caller ping-pongs two storage
// buffers for height.
//
// Bind group layout (DD8):
//   @group(0) @binding(0)  Params uniform
//   @group(1) @binding(0)  height_in   — read-only f32 array (current step input)
//   @group(1) @binding(1)  skip_mask   — read-only u32 array (1 = sea-or-coast, 0 = land-interior)
//   @group(2) @binding(0)  height_out  — read-write f32 array (current step output)
//
// Workgroup size: 8×8×1 — matches wgpu::Limits::downlevel_defaults() (DD8 lock).

struct Params {
    width:  u32,
    height: u32,
    d:      f32,   // diffusivity (hillslope_d)
    dt_sub: f32,   // 1.0 / n_diff_substep
}

@group(0) @binding(0) var<uniform> params: Params;

@group(1) @binding(0) var<storage, read>       height_in:  array<f32>;
@group(1) @binding(1) var<storage, read>       skip_mask:  array<u32>;
@group(2) @binding(0) var<storage, read_write> height_out: array<f32>;

@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let ix = gid.x;
    let iy = gid.y;
    let w  = params.width;
    let h  = params.height;

    // Out-of-bounds guard — dispatches are rounded up to workgroup size.
    if (ix >= w || iy >= h) {
        return;
    }

    let i = iy * w + ix;

    // Boundary ring: cells on the grid perimeter are preserved (no full 4-neighbour
    // stencil). Copy through and return; matches the CPU path where copy_from_slice
    // seeds z_new with the current state before the interior loop.
    if (ix == 0u || ix >= w - 1u || iy == 0u || iy >= h - 1u) {
        height_out[i] = height_in[i];
        return;
    }

    // Sea or coast cells: preserved, never written by the diffusion kernel.
    if (skip_mask[i] == 1u) {
        height_out[i] = height_in[i];
        return;
    }

    // Interior land cell: apply 5-point Laplacian stencil.
    let z_here = height_in[i];
    let z_n    = height_in[(iy - 1u) * w + ix];
    let z_s    = height_in[(iy + 1u) * w + ix];
    let z_w    = height_in[iy * w + (ix - 1u)];
    let z_e    = height_in[iy * w + (ix + 1u)];

    let lap = z_n + z_s + z_e + z_w - 4.0 * z_here;
    height_out[i] = z_here + params.d * lap * params.dt_sub;
}
