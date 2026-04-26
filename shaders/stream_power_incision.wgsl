// stream_power_incision.wgsl — Sprint 4.F
//
// One iteration of the Stream Power Incision Model (SPIM) applied per-cell on
// the GPU.  Two variants are selected at dispatch time via `params.spim_variant`:
//
//   0 = Plain   (Sprint 2 single-equation: E_f = K · A^m · S^n)
//   1 = SpaceLite (Sprint 3 dual-equation: shielded bedrock + sediment
//                  entrainment, default for the canonical pipeline)
//
// CPU reference: `stream_power_incision_kernel` in
// `crates/sim/src/geomorph/stream_power.rs`.  Math is bit-equivalent on
// normal inputs; accumulated fp drift over many iterations is bounded by
// DD8 (≤ 1e-3 × max(h_cpu) after 100 iterations).
//
// Bind group layout (DD8):
//   @group(0) @binding(0)  Params uniform
//   @group(1) @binding(0)  height_in   — read-only  f32 array
//   @group(1) @binding(1)  is_land     — read-only  u32 array (0=sea, 1=land)
//   @group(1) @binding(2)  accumulation— read-only  f32 array
//   @group(1) @binding(3)  slope       — read-only  f32 array
//   @group(1) @binding(4)  sediment_in — read-only  f32 array (SpaceLite only)
//   @group(2) @binding(0)  height_out  — read-write f32 array
//   @group(2) @binding(1)  sediment_out— read-write f32 array (SpaceLite: updated hs)
//
// Workgroup size: 8×8×1 — DD8 lock (matches wgpu::Limits::downlevel_defaults()).

// ── Uniform struct ─────────────────────────────────────────────────────────────

struct Params {
    width:         u32,   // grid width  (sim_width)
    height:        u32,   // grid height (sim_height)
    spim_variant:  u32,   // 0 = Plain, 1 = SpaceLite
    _pad:          u32,   // explicit 4-byte pad; u32 alignment keeps struct at 48 bytes

    k:             f32,   // Plain: SPIM erodibility
    k_bed:         f32,   // SpaceLite: bedrock erodibility
    k_sed:         f32,   // SpaceLite: sediment entrainability
    m:             f32,   // drainage-area exponent (both variants)

    n:             f32,   // slope exponent (both variants)
    h_star:        f32,   // SpaceLite: cover-decay scale H*
    hs_entrain_max:f32,   // SpaceLite: hs entrainment cap
    sea_level:     f32,   // lower bound for height after incision
}

@group(0) @binding(0) var<uniform> params: Params;

@group(1) @binding(0) var<storage, read> height_in:    array<f32>;
@group(1) @binding(1) var<storage, read> is_land:      array<u32>;
@group(1) @binding(2) var<storage, read> accumulation: array<f32>;
@group(1) @binding(3) var<storage, read> slope_field:  array<f32>;
@group(1) @binding(4) var<storage, read> sediment_in:  array<f32>;

@group(2) @binding(0) var<storage, read_write> height_out:   array<f32>;
@group(2) @binding(1) var<storage, read_write> sediment_out: array<f32>;

// ── Helpers ────────────────────────────────────────────────────────────────────

/// Return `x * pow(a, m) * pow(s, n)` with a non-finite guard.
///
/// Mirrors `stream_power_kernel(k, a, s, m, n)` in the CPU code.
/// WGSL `pow` is IEEE-like but the standard does not guarantee exactly the
/// same bit pattern as Rust's `f32::powf`.  The NaN/Inf guard is the same
/// as the CPU: return 0.0 whenever the product is non-finite.
///
/// NaN check: `x != x` is true for NaN in IEEE 754.
/// Inf  check: `abs(x) >= 1e30` catches the practical infinity range without
///             requiring `isinf` (not available in WGSL).
fn stream_power_kernel(k: f32, a: f32, s: f32, m: f32, n: f32) -> f32 {
    let ef = k * pow(a, m) * pow(s, n);
    // Reject NaN (ef != ef) and very large / Inf values.
    if (ef != ef || abs(ef) >= 1e30) {
        return 0.0;
    }
    return ef;
}

// ── Compute entry point ────────────────────────────────────────────────────────

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

    // Default: copy through.  Sea cells always take this path after the
    // is_land gate below; land cells overwrite on exit.
    height_out[i]   = height_in[i];
    sediment_out[i] = sediment_in[i];

    // Sea cells: exact copy (is_land gate matches CPU behaviour exactly).
    if (is_land[i] == 0u) {
        return;
    }

    let a     = accumulation[i];
    let s_val = slope_field[i];
    let hs    = sediment_in[i];
    let h_here = height_in[i];

    if (params.spim_variant == 0u) {
        // ── Plain variant (Sprint 2 single-equation SPIM) ──────────────────────
        //   ef = K · A^m · S^n
        //   h_new = max(h - ef, sea_level)
        let ef = stream_power_kernel(params.k, a, s_val, params.m, params.n);
        height_out[i] = max(h_here - ef, params.sea_level);
        // sediment_out already holds sediment_in[i] (the copy-through above).

    } else {
        // ── SpaceLite variant (Sprint 3 dual-equation, default) ───────────────
        //   shield  = exp(-hs / H*)
        //   e_bed   = K_bed · A^m · S^n · shield   (NaN-guarded)
        //   hs_eff  = min(hs, HS_ENTRAIN_MAX)
        //   e_sed   = K_sed · A^m · S^n · hs_eff   (NaN-guarded)
        //   h_new   = max(h - e_bed, sea_level)
        //   hs_new  = clamp(hs + e_bed - e_sed, 0, 1)

        let shield = exp(-hs / params.h_star);

        // e_bed: shielded bedrock incision term.
        var e_bed = stream_power_kernel(params.k_bed, a, s_val, params.m, params.n) * shield;
        if (e_bed != e_bed || abs(e_bed) >= 1e30) {
            e_bed = 0.0;
        }

        // e_sed: sediment entrainment term (capped at hs_entrain_max).
        let hs_eff = min(hs, params.hs_entrain_max);
        var e_sed = stream_power_kernel(params.k_sed, a, s_val, params.m, params.n) * hs_eff;
        if (e_sed != e_sed || abs(e_sed) >= 1e30) {
            e_sed = 0.0;
        }

        height_out[i]   = max(h_here - e_bed, params.sea_level);
        sediment_out[i] = clamp(hs + e_bed - e_sed, 0.0, 1.0);
    }
}
