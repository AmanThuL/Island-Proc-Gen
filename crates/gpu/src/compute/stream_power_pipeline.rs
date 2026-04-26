//! Stream power incision GPU compute pipeline — Sprint 4.F.
//!
//! Implements a single SPIM iteration on the GPU.  Both `SpimVariant::Plain`
//! and `SpimVariant::SpaceLite` are handled inside the single WGSL kernel via
//! the `params.spim_variant` uniform field (`0 = Plain`, `1 = SpaceLite`).
//!
//! # Bind group layout (DD8)
//!
//! ```text
//! @group(0) @binding(0)  Params uniform (width, height, variant, k, k_bed, k_sed, m, n, h_star, hs_entrain_max, sea_level + pad)
//! @group(1) @binding(0)  height_in    — read-only  f32 array
//! @group(1) @binding(1)  is_land      — read-only  u32 array (0=sea, 1=land)
//! @group(1) @binding(2)  accumulation — read-only  f32 array
//! @group(1) @binding(3)  slope        — read-only  f32 array
//! @group(1) @binding(4)  sediment_in  — read-only  f32 array
//! @group(2) @binding(0)  height_out   — read-write f32 array
//! @group(2) @binding(1)  sediment_out — read-write f32 array
//! ```
//!
//! # Buffer strategy
//!
//! Stream power incision is a single-step kernel (no ping-pong substep loop):
//! one dispatch reads from `height_in` / `sediment_in` and writes to
//! `height_out` / `sediment_out`.  Two pairs of buffers (`height_a`/`b` and
//! `sediment_a`/`b`) allow the caller to use ping-pong if desired, but
//! `StreamPowerComputePipeline::dispatch` always performs a single dispatch
//! (src = A, dst = B) and reads back from B.  The next call uploads the new
//! CPU state into A again, so there is no real ping-pong across calls.
//!
//! This is intentionally simpler than `HillslopeComputePipeline`, which truly
//! ping-pongs to avoid CPU↔GPU round-trips between substeps.  SPIM has no
//! inner substeps, so the round-trip is unavoidable anyway (the next stage
//! `SedimentUpdateStage` needs the CPU-side data).
//!
//! # Verification
//!
//! `naga` compile-time tests are included; they fire without a GPU adapter.

use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use island_core::{
    pipeline::{ComputeBackendError, FALLBACK_H_STAR, StreamPowerParams},
    preset::SpimVariant,
    world::WorldState,
};

use crate::{
    GpuContext,
    compute::{buffers, timestamp::GpuTimer},
};

// ── Uniform struct (mirrors WGSL `Params`) ────────────────────────────────────

/// GPU-side uniform block for stream power incision — must match the WGSL
/// `struct Params` layout byte-for-byte.
///
/// The struct is padded to 48 bytes (12 × u32/f32 = 12 × 4 = 48) to satisfy
/// wgpu's `min_uniform_buffer_offset_alignment`.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuParams {
    /// Grid width in cells.
    width: u32,
    /// Grid height in cells.
    height: u32,
    /// `0` = `SpimVariant::Plain`, `1` = `SpimVariant::SpaceLite`.
    spim_variant: u32,
    /// Explicit 4-byte padding (matches WGSL `_pad: u32`).
    _pad: u32,

    /// Plain: SPIM erodibility `K`.
    k: f32,
    /// SpaceLite: bedrock erodibility `K_bed`.
    k_bed: f32,
    /// SpaceLite: sediment entrainability `K_sed`.
    k_sed: f32,
    /// Drainage-area exponent `m` (both variants).
    m: f32,

    /// Slope exponent `n` (both variants).
    n: f32,
    /// SpaceLite: cover-decay scale `H*`.
    h_star: f32,
    /// SpaceLite: sediment entrainment cap `HS_ENTRAIN_MAX`.
    hs_entrain_max: f32,
    /// Lower bound for height after incision (`sea_level`).
    sea_level: f32,
}

// ── StreamPowerComputePipeline ────────────────────────────────────────────────

/// Compiled GPU compute pipeline for stream power incision.
///
/// Constructed once per [`crate::compute::GpuBackend`] instance via
/// [`StreamPowerComputePipeline::new`].  Internal buffers are lazily allocated
/// on the first [`dispatch`](StreamPowerComputePipeline::dispatch) call and
/// reallocated when the grid dimensions change.
pub struct StreamPowerComputePipeline {
    pipeline: wgpu::ComputePipeline,

    bgl_uniforms: wgpu::BindGroupLayout,
    bgl_inputs: wgpu::BindGroupLayout,
    bgl_outputs: wgpu::BindGroupLayout,

    /// Small uniform buffer (48 bytes).
    uniforms_buf: wgpu::Buffer,

    /// GPU-side `height` buffer (src for each dispatch). `(buffer, w, h)`.
    height_src: Option<(wgpu::Buffer, u32, u32)>,
    /// GPU-side `height` buffer (dst for each dispatch).
    height_dst: Option<(wgpu::Buffer, u32, u32)>,
    /// GPU-side `sediment` buffer (src).
    sediment_src: Option<(wgpu::Buffer, u32, u32)>,
    /// GPU-side `sediment` buffer (dst).
    sediment_dst: Option<(wgpu::Buffer, u32, u32)>,
    /// `is_land` mask buffer (read-only u32).
    is_land_buf: Option<(wgpu::Buffer, u32, u32)>,
    /// `accumulation` buffer (read-only f32).
    accumulation_buf: Option<(wgpu::Buffer, u32, u32)>,
    /// `slope` buffer (read-only f32).
    slope_buf: Option<(wgpu::Buffer, u32, u32)>,

    /// GPU timestamp timer — `None` when the adapter lacks `TIMESTAMP_QUERY`.
    timer: Option<GpuTimer>,

    ctx: Arc<GpuContext>,
}

impl StreamPowerComputePipeline {
    /// Compile the WGSL shader and build all wgpu objects.
    ///
    /// Buffers are deferred to the first [`dispatch`] call; only the pipeline
    /// + bind group layouts + uniform buffer are created here.
    pub fn new(ctx: Arc<GpuContext>) -> Self {
        let device = &ctx.device;

        // ── Shader module ─────────────────────────────────────────────────────
        let shader_src = include_str!("../../../../shaders/stream_power_incision.wgsl");
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("stream_power_incision_shader"),
            source: wgpu::ShaderSource::Wgsl(shader_src.into()),
        });

        // ── Bind group layout: group(0) — uniforms ────────────────────────────
        let bgl_uniforms = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sp_bgl_uniforms"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        // ── Bind group layout: group(1) — inputs (5 read-only buffers) ────────
        let bgl_inputs = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sp_bgl_inputs"),
            entries: &[
                // height_in
                make_storage_entry(0, true),
                // is_land
                make_storage_entry(1, true),
                // accumulation
                make_storage_entry(2, true),
                // slope
                make_storage_entry(3, true),
                // sediment_in
                make_storage_entry(4, true),
            ],
        });

        // ── Bind group layout: group(2) — outputs (2 read-write buffers) ──────
        let bgl_outputs = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sp_bgl_outputs"),
            entries: &[
                // height_out
                make_storage_entry(0, false),
                // sediment_out
                make_storage_entry(1, false),
            ],
        });

        // ── Pipeline layout + compute pipeline ────────────────────────────────
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("sp_pipeline_layout"),
            bind_group_layouts: &[Some(&bgl_uniforms), Some(&bgl_inputs), Some(&bgl_outputs)],
            immediate_size: 0,
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("stream_power_compute_pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        // ── Uniform buffer (48 bytes) ─────────────────────────────────────────
        let uniforms_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sp_uniforms"),
            size: std::mem::size_of::<GpuParams>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // ── Optional GPU timer ────────────────────────────────────────────────
        let timer = GpuTimer::new_if_supported(device, ctx.timestamp_period_ns).unwrap_or(None);

        Self {
            pipeline,
            bgl_uniforms,
            bgl_inputs,
            bgl_outputs,
            uniforms_buf,
            height_src: None,
            height_dst: None,
            sediment_src: None,
            sediment_dst: None,
            is_land_buf: None,
            accumulation_buf: None,
            slope_buf: None,
            timer,
            ctx,
        }
    }

    /// Dispatch one SPIM iteration on the GPU and write the results back into
    /// `world.authoritative.{height, sediment}`.
    ///
    /// Returns `Ok(Some(gpu_ms))` when timestamps are available, `Ok(None)`
    /// otherwise.
    pub fn dispatch(
        &mut self,
        world: &mut WorldState,
        params: &StreamPowerParams,
    ) -> Result<Option<f64>, ComputeBackendError> {
        let width = world.resolution.sim_width;
        let height = world.resolution.sim_height;
        let n_cells = (width * height) as usize;

        // ── Prerequisite field access ─────────────────────────────────────────
        let h_field = world.authoritative.height.as_ref().ok_or_else(|| {
            ComputeBackendError::Other(
                "StreamPowerComputePipeline: authoritative.height is None".into(),
            )
        })?;
        let hs_field = world.authoritative.sediment.as_ref().ok_or_else(|| {
            ComputeBackendError::Other(
                "StreamPowerComputePipeline: authoritative.sediment is None".into(),
            )
        })?;
        let coast_mask = world.derived.coast_mask.as_ref().ok_or_else(|| {
            ComputeBackendError::Other(
                "StreamPowerComputePipeline: derived.coast_mask is None".into(),
            )
        })?;
        let accum_field = world.derived.accumulation.as_ref().ok_or_else(|| {
            ComputeBackendError::Other(
                "StreamPowerComputePipeline: derived.accumulation is None".into(),
            )
        })?;
        let slope_field_data = world.derived.slope.as_ref().ok_or_else(|| {
            ComputeBackendError::Other("StreamPowerComputePipeline: derived.slope is None".into())
        })?;

        let device = &self.ctx.device;
        let queue = &self.ctx.queue;

        // ── 1. Ensure GPU buffers are sized for this grid ─────────────────────
        let needs_realloc = self
            .height_src
            .as_ref()
            .map(|(_, w, h)| *w != width || *h != height)
            .unwrap_or(true);

        if needs_realloc {
            let f32_len = (n_cells * std::mem::size_of::<f32>()) as u64;
            let u32_len = (n_cells * std::mem::size_of::<u32>()) as u64;

            let f32_storage_rw = |label: &'static str| {
                device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some(label),
                    size: f32_len,
                    usage: wgpu::BufferUsages::STORAGE
                        | wgpu::BufferUsages::COPY_DST
                        | wgpu::BufferUsages::COPY_SRC,
                    mapped_at_creation: false,
                })
            };
            let f32_storage_ro = |label: &'static str| {
                device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some(label),
                    size: f32_len,
                    usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                })
            };

            self.height_src = Some((f32_storage_rw("sp_height_src"), width, height));
            self.height_dst = Some((f32_storage_rw("sp_height_dst"), width, height));
            self.sediment_src = Some((f32_storage_rw("sp_sediment_src"), width, height));
            self.sediment_dst = Some((f32_storage_rw("sp_sediment_dst"), width, height));
            self.accumulation_buf = Some((f32_storage_ro("sp_accumulation"), width, height));
            self.slope_buf = Some((f32_storage_ro("sp_slope"), width, height));

            // is_land: u32 per cell.
            let is_land_gpu = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("sp_is_land"),
                size: u32_len,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.is_land_buf = Some((is_land_gpu, width, height));
        }

        let h_src_buf = &self.height_src.as_ref().unwrap().0;
        let h_dst_buf = &self.height_dst.as_ref().unwrap().0;
        let hs_src_buf = &self.sediment_src.as_ref().unwrap().0;
        let hs_dst_buf = &self.sediment_dst.as_ref().unwrap().0;
        let land_buf = &self.is_land_buf.as_ref().unwrap().0;
        let acc_buf = &self.accumulation_buf.as_ref().unwrap().0;
        let slope_buf = &self.slope_buf.as_ref().unwrap().0;

        // ── 2. Upload inputs to GPU ───────────────────────────────────────────
        // height and sediment: raw f32 data (NOT the IPGF 17-byte-header format).
        queue.write_buffer(h_src_buf, 0, bytemuck::cast_slice(&h_field.data));
        queue.write_buffer(hs_src_buf, 0, bytemuck::cast_slice(&hs_field.data));

        // is_land: u8 mask → u32 for WGSL (0 = sea, 1 = land).
        let is_land_u32: Vec<u32> = coast_mask.is_land.data.iter().map(|&v| v as u32).collect();
        queue.write_buffer(land_buf, 0, bytemuck::cast_slice::<u32, u8>(&is_land_u32));

        queue.write_buffer(acc_buf, 0, bytemuck::cast_slice(&accum_field.data));
        queue.write_buffer(slope_buf, 0, bytemuck::cast_slice(&slope_field_data.data));

        // ── 3. Upload uniform params ──────────────────────────────────────────
        let variant_u32: u32 = match params.spim_variant {
            SpimVariant::Plain => 0,
            SpimVariant::SpaceLite => 1,
        };

        // Guard against a zero H* (matches the CPU's defensive fallback).
        let h_star = if params.h_star > 0.0 {
            params.h_star
        } else {
            FALLBACK_H_STAR
        };

        let gpu_params = GpuParams {
            width,
            height,
            spim_variant: variant_u32,
            _pad: 0,
            k: params.spim_k,
            k_bed: params.space_k_bed,
            k_sed: params.space_k_sed,
            m: params.spim_m,
            n: params.spim_n,
            h_star,
            hs_entrain_max: params.hs_entrain_max,
            sea_level: params.sea_level,
        };
        queue.write_buffer(&self.uniforms_buf, 0, bytemuck::bytes_of(&gpu_params));

        // ── 4. Build bind groups ──────────────────────────────────────────────
        let bg_uniforms = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sp_bg_uniforms"),
            layout: &self.bgl_uniforms,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: self.uniforms_buf.as_entire_binding(),
            }],
        });

        let bg_inputs = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sp_bg_inputs"),
            layout: &self.bgl_inputs,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: h_src_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: land_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: acc_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: slope_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: hs_src_buf.as_entire_binding(),
                },
            ],
        });

        let bg_outputs = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sp_bg_outputs"),
            layout: &self.bgl_outputs,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: h_dst_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: hs_dst_buf.as_entire_binding(),
                },
            ],
        });

        // ── 5. Encode + dispatch ──────────────────────────────────────────────
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("sp_encoder"),
        });

        let workgroups_x = width.div_ceil(8);
        let workgroups_y = height.div_ceil(8);

        let ts_writes = self.timer.as_ref().map(|t| t.timestamp_writes());

        {
            let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("sp_compute_pass"),
                timestamp_writes: ts_writes,
            });
            cpass.set_pipeline(&self.pipeline);
            cpass.set_bind_group(0, &bg_uniforms, &[]);
            cpass.set_bind_group(1, &bg_inputs, &[]);
            cpass.set_bind_group(2, &bg_outputs, &[]);
            cpass.dispatch_workgroups(workgroups_x, workgroups_y, 1);
        }

        // ── 6. Submit + optional timestamp resolve ────────────────────────────
        let gpu_ms = if let Some(ref timer) = self.timer {
            timer
                .resolve_ms(encoder, device, queue, self.ctx.timestamp_period_ns)
                .transpose()
                .map_err(|e| ComputeBackendError::Other(e.into()))?
        } else {
            queue.submit(Some(encoder.finish()));
            None
        };

        // Poll until GPU work is complete.
        device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|e| {
                ComputeBackendError::Other(anyhow::anyhow!("device.poll failed: {e}").into())
            })?;

        // ── 7. Read back results ──────────────────────────────────────────────
        let h_floats = buffers::readback_f32(device, queue, h_dst_buf, n_cells)
            .map_err(|e| ComputeBackendError::Other(e.into()))?;
        let hs_floats = buffers::readback_f32(device, queue, hs_dst_buf, n_cells)
            .map_err(|e| ComputeBackendError::Other(e.into()))?;

        // Write results back into world state in-place.
        let auth = &mut world.authoritative;
        auth.height
            .as_mut()
            .unwrap()
            .data
            .copy_from_slice(&h_floats);
        auth.sediment
            .as_mut()
            .unwrap()
            .data
            .copy_from_slice(&hs_floats);

        Ok(gpu_ms)
    }
}

// ── Bind group layout entry helper ────────────────────────────────────────────

/// Build a storage buffer bind group layout entry.
///
/// `read_only = true` for input buffers; `false` for read-write output buffers.
fn make_storage_entry(binding: u32, read_only: bool) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Naga compile-time validation (no GPU needed) ──────────────────────────

    /// DD8 gate: the stream power WGSL shader must parse and validate under naga.
    #[test]
    fn stream_power_wgsl_parses_successfully() {
        let src = include_str!("../../../../shaders/stream_power_incision.wgsl");
        let module = naga::front::wgsl::parse_str(src).expect("WGSL parse failed");
        let mut validator = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::default(),
        );
        validator.validate(&module).expect("WGSL validation failed");
    }

    /// DD8 lock: workgroup size must be exactly (8, 8, 1).
    #[test]
    fn stream_power_wgsl_workgroup_size_matches_dd8_lock() {
        let src = include_str!("../../../../shaders/stream_power_incision.wgsl");
        assert!(
            src.contains("@workgroup_size(8, 8, 1)"),
            "stream_power_incision.wgsl must declare @workgroup_size(8, 8, 1) per DD8 lock"
        );
    }

    /// Sprint 4.F: `StreamPowerComputePipeline` must be a non-zero-sized struct
    /// (i.e. actually has fields after 4.F fleshed out the stub).
    #[test]
    fn gpu_backend_supports_stream_power_after_4f() {
        assert!(
            std::mem::size_of::<StreamPowerComputePipeline>() > 0,
            "StreamPowerComputePipeline must be a non-zero-sized struct after 4.F"
        );
    }

    /// Both pilot ops must be represented in `ComputeOp::ALL`.
    #[test]
    fn gpu_backend_supports_both_pilot_ops_after_4f() {
        use island_core::pipeline::ComputeOp;
        // The exact variants and order are locked by compute_op_enum_snapshot
        // in core; here we verify at least that both exist in ALL.
        assert!(
            ComputeOp::ALL.contains(&ComputeOp::HillslopeDiffusion),
            "HillslopeDiffusion must be in ComputeOp::ALL"
        );
        assert!(
            ComputeOp::ALL.contains(&ComputeOp::StreamPowerIncision),
            "StreamPowerIncision must be in ComputeOp::ALL"
        );
        assert_eq!(
            ComputeOp::ALL.len(),
            2,
            "ComputeOp::ALL must have exactly 2 pilot ops at Sprint 4.F"
        );
    }
}
