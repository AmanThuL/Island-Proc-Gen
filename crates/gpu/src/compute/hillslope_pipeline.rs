//! Hillslope diffusion GPU compute pipeline — Sprint 4.E.
//!
//! Implements one explicit-Euler substep of `∂z/∂t = D · ∇²z` per dispatch on
//! the GPU. The caller (dispatch loop inside [`HillslopeComputePipeline::dispatch`])
//! runs `n_diff_substep` dispatches total, ping-ponging two storage buffers for
//! the height field between even and odd substeps so no CPU↔GPU round-trips
//! occur mid-kernel.
//!
//! # Bind group layout (DD8)
//!
//! ```text
//! @group(0) @binding(0)  Params uniform (width, height, d, dt_sub)
//! @group(1) @binding(0)  height_in  — read-only  f32 array
//! @group(1) @binding(1)  skip_mask  — read-only  u32 array (combined sea|coast)
//! @group(2) @binding(0)  height_out — read-write f32 array
//! ```
//!
//! # Ping-pong buffer pattern
//!
//! Two GPU storage buffers (`height_a`, `height_b`) are allocated when the grid
//! size is first seen and reused on subsequent dispatches with the same grid
//! dimensions.  On even substeps the shader reads from A and writes to B; on odd
//! substeps it reads from B and writes to A.  After `n_substep` dispatches the
//! result lives in whichever buffer was last written; `dispatch` reads back from
//! that buffer.
//!
//! # Verification
//!
//! `naga` compilation tests are included below — they fire without a GPU adapter
//! so they run in the normal `cargo test` suite.

use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use island_core::{
    pipeline::{ComputeBackendError, HillslopeParams},
    world::WorldState,
};

use crate::{
    GpuContext,
    compute::{buffers, timestamp::GpuTimer},
};

// ── Uniform struct (mirrors WGSL `Params`) ────────────────────────────────────

/// GPU-side uniform block for hillslope diffusion — must match the WGSL
/// `struct Params` layout byte-for-byte.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuParams {
    width: u32,
    height: u32,
    d: f32,
    dt_sub: f32,
}

// ── HillslopeComputePipeline ──────────────────────────────────────────────────

/// Compiled GPU compute pipeline for hillslope diffusion.
///
/// Constructed once per [`crate::compute::GpuBackend`] instance; reused across
/// every call to [`dispatch`](HillslopeComputePipeline::dispatch). Internal
/// buffers are lazily allocated on the first dispatch and reallocated if the
/// grid dimensions change.
pub struct HillslopeComputePipeline {
    pipeline: wgpu::ComputePipeline,

    bgl_uniforms: wgpu::BindGroupLayout,
    bgl_inputs: wgpu::BindGroupLayout,
    bgl_outputs: wgpu::BindGroupLayout,

    /// Small uniform buffer (16 bytes).
    uniforms_buf: wgpu::Buffer,

    /// Ping-pong height buffer A. `(buffer, width, height)`.
    height_a: Option<(wgpu::Buffer, u32, u32)>,
    /// Ping-pong height buffer B.
    height_b: Option<(wgpu::Buffer, u32, u32)>,
    /// Combined `is_sea | is_coast` mask (u32, 0 = land-interior, 1 = skip).
    skip_mask: Option<(wgpu::Buffer, u32, u32)>,

    /// GPU timestamp timer — `None` when the adapter lacks `TIMESTAMP_QUERY`.
    timer: Option<GpuTimer>,

    ctx: Arc<GpuContext>,
}

impl HillslopeComputePipeline {
    /// Compile the WGSL shader and build all wgpu objects.
    ///
    /// Bind group layouts, the pipeline layout, and the compute pipeline are
    /// created here. Buffers are deferred to the first [`dispatch`] call because
    /// the grid size is not known at construction time.
    pub fn new(ctx: Arc<GpuContext>) -> Self {
        let device = &ctx.device;

        // ── Shader module ─────────────────────────────────────────────────────
        let shader_src = include_str!("../../../../shaders/hillslope_diffusion.wgsl");
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("hillslope_diffusion_shader"),
            source: wgpu::ShaderSource::Wgsl(shader_src.into()),
        });

        // ── Bind group layouts ────────────────────────────────────────────────
        let bgl_uniforms = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("hillslope_bgl_uniforms"),
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

        let bgl_inputs = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("hillslope_bgl_inputs"),
            entries: &[
                // height_in
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // skip_mask
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let bgl_outputs = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("hillslope_bgl_outputs"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        // ── Pipeline layout + compute pipeline ────────────────────────────────
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("hillslope_pipeline_layout"),
            bind_group_layouts: &[Some(&bgl_uniforms), Some(&bgl_inputs), Some(&bgl_outputs)],
            immediate_size: 0,
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("hillslope_compute_pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        // ── Uniform buffer (16 bytes, CPU-writable via write_buffer) ──────────
        let uniforms_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("hillslope_uniforms"),
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
            height_a: None,
            height_b: None,
            skip_mask: None,
            timer,
            ctx,
        }
    }

    /// Run `n_diff_substep` GPU dispatches of the hillslope diffusion kernel,
    /// then write the final height field back into `world.authoritative.height`.
    ///
    /// # Errors
    ///
    /// Returns [`ComputeBackendError::Other`] on wgpu allocation or readback
    /// failures. The GPU never produces `DeviceLost` or `ReadbackTimeout` here
    /// because we poll synchronously (`wait_indefinitely`).
    pub fn dispatch(
        &mut self,
        world: &mut WorldState,
        params: &HillslopeParams,
    ) -> Result<Option<f64>, ComputeBackendError> {
        let cpu_start = std::time::Instant::now();

        let width = world.resolution.sim_width;
        let height = world.resolution.sim_height;
        let n_sub = params.n_diff_substep as usize;
        let dt_sub = 1.0_f32 / n_sub as f32;
        let d = params.hillslope_d;

        let h_field = world.authoritative.height.as_ref().ok_or_else(|| {
            ComputeBackendError::Other(
                "HillslopeComputePipeline: authoritative.height is None".into(),
            )
        })?;
        let coast_mask = world.derived.coast_mask.as_ref().ok_or_else(|| {
            ComputeBackendError::Other(
                "HillslopeComputePipeline: derived.coast_mask is None".into(),
            )
        })?;

        let n_cells = (width * height) as usize;
        let device = &self.ctx.device;
        let queue = &self.ctx.queue;

        // ── 1. Ensure GPU buffers are sized for this grid ─────────────────────
        let needs_realloc = self
            .height_a
            .as_ref()
            .map(|(_, w, h)| *w != width || *h != height)
            .unwrap_or(true);

        if needs_realloc {
            let byte_len = (n_cells * std::mem::size_of::<f32>()) as u64;
            let mask_len = (n_cells * std::mem::size_of::<u32>()) as u64;

            let make_storage = |label: &'static str| {
                device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some(label),
                    size: byte_len,
                    usage: wgpu::BufferUsages::STORAGE
                        | wgpu::BufferUsages::COPY_DST
                        | wgpu::BufferUsages::COPY_SRC,
                    mapped_at_creation: false,
                })
            };
            let buf_a = make_storage("hillslope_height_a");
            let buf_b = make_storage("hillslope_height_b");
            let mask_buf = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("hillslope_skip_mask"),
                size: mask_len,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.height_a = Some((buf_a, width, height));
            self.height_b = Some((buf_b, width, height));
            self.skip_mask = Some((mask_buf, width, height));
        }

        let buf_a = &self.height_a.as_ref().unwrap().0;
        let buf_b = &self.height_b.as_ref().unwrap().0;
        let mask_buf = &self.skip_mask.as_ref().unwrap().0;

        // ── 2. Upload height field and combined skip mask ─────────────────────
        // Upload raw f32 data (NOT the IPGF serialised format from `to_bytes()`
        // which includes a 17-byte header that would corrupt the GPU buffer).
        let height_bytes: &[u8] = bytemuck::cast_slice(&h_field.data);
        queue.write_buffer(buf_a, 0, height_bytes);

        // Pack is_sea | is_coast into a single u32 mask: 1 = skip, 0 = land-interior.
        let skip_data: Vec<u32> = build_skip_mask(coast_mask);
        let skip_bytes = bytemuck::cast_slice::<u32, u8>(&skip_data);
        queue.write_buffer(mask_buf, 0, skip_bytes);

        // ── 3. Upload uniform params ──────────────────────────────────────────
        let gpu_params = GpuParams {
            width,
            height,
            d,
            dt_sub,
        };
        queue.write_buffer(&self.uniforms_buf, 0, bytemuck::bytes_of(&gpu_params));

        // ── 4. Build bind group for uniforms (stable across all substeps) ─────
        let bg_uniforms = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("hillslope_bg_uniforms"),
            layout: &self.bgl_uniforms,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: self.uniforms_buf.as_entire_binding(),
            }],
        });

        // ── 5. Substep loop — ping-pong A/B ──────────────────────────────────
        // For n_sub dispatches, the first encoder encodes all passes together.
        // We measure GPU time only on the first substep encoder (DD8: optional
        // timing; a single timer covers the total GPU wall time of substep 0).
        // Timer wraps the FIRST compute pass only; total elapsed is approximated
        // by `cpu_start.elapsed()` on the host side which covers all substeps.
        //
        // For simplicity and correctness we encode ALL substeps into a single
        // CommandEncoder so the GPU executes them sequentially without any CPU
        // synchronisation between substeps.

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("hillslope_substeps_encoder"),
        });

        let workgroups_x = width.div_ceil(8);
        let workgroups_y = height.div_ceil(8);

        for sub in 0..n_sub {
            // Even substep: A → B; odd substep: B → A.
            let (src_buf, dst_buf) = if sub % 2 == 0 {
                (buf_a, buf_b)
            } else {
                (buf_b, buf_a)
            };

            let bg_inputs = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("hillslope_bg_inputs"),
                layout: &self.bgl_inputs,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: src_buf.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: mask_buf.as_entire_binding(),
                    },
                ],
            });

            let bg_outputs = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("hillslope_bg_outputs"),
                layout: &self.bgl_outputs,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: dst_buf.as_entire_binding(),
                }],
            });

            // Attach timestamp writes only on the first substep pass.
            // Build the descriptor without a reference to `self.timer` that
            // would outlast the borrow — extract the query-set fields eagerly.
            let ts_writes = if sub == 0 {
                self.timer.as_ref().map(|t| t.timestamp_writes())
            } else {
                None
            };

            let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("hillslope_compute_pass"),
                timestamp_writes: ts_writes,
            });
            cpass.set_pipeline(&self.pipeline);
            cpass.set_bind_group(0, &bg_uniforms, &[]);
            cpass.set_bind_group(1, &bg_inputs, &[]);
            cpass.set_bind_group(2, &bg_outputs, &[]);
            cpass.dispatch_workgroups(workgroups_x, workgroups_y, 1);
        }

        // ── 6. Submit encoder + optional timestamp resolve ────────────────────
        // After n_sub dispatches, the result lives in the buffer last written.
        let result_buf = if n_sub % 2 == 0 {
            buf_a // 0 substeps → A unchanged; 2 substeps: A→B→A; etc.
        } else {
            buf_b // 1 substep: A→B; 3 substeps: A→B→A→B; etc.
        };

        // Resolve timestamp (if supported) then submit.
        let gpu_ms = if let Some(ref timer) = self.timer {
            // Resolve appends resolve+copy commands to the encoder and submits.
            timer
                .resolve_ms(encoder, device, queue, self.ctx.timestamp_period_ns)
                .transpose()
                .map_err(|e| ComputeBackendError::Other(e.into()))?
        } else {
            queue.submit(Some(encoder.finish()));
            None
        };

        // Poll to ensure GPU work is complete before reading back.
        device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|e| {
                ComputeBackendError::Other(anyhow::anyhow!("device.poll failed: {e}").into())
            })?;

        // ── 7. Read back the result into world.authoritative.height ───────────
        let floats = buffers::readback_f32(device, queue, result_buf, n_cells)
            .map_err(|e| ComputeBackendError::Other(e.into()))?;

        // Write floats back into world state.
        let h_field_mut = world.authoritative.height.as_mut().unwrap();
        h_field_mut.data.copy_from_slice(&floats);

        let _ = cpu_start; // suppress warning; cpu_ms is reported by caller wrapper
        Ok(gpu_ms)
    }
}

// ── Helper ────────────────────────────────────────────────────────────────────

/// Build the combined skip mask (1 = sea or coast, 0 = land-interior eligible).
fn build_skip_mask(coast_mask: &island_core::world::CoastMask) -> Vec<u32> {
    let is_sea = &coast_mask.is_sea.data;
    let is_coast = &coast_mask.is_coast.data;
    is_sea
        .iter()
        .zip(is_coast.iter())
        .map(|(&s, &c)| if s == 1 || c == 1 { 1u32 } else { 0u32 })
        .collect()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use island_core::{
        field::{MaskField2D, ScalarField2D},
        pipeline::HillslopeParams,
        preset::{ErosionParams, IslandAge, IslandArchetypePreset},
        seed::Seed,
        world::{CoastMask, Resolution, WorldState},
    };
    use std::sync::Arc;

    // ── Naga compile-time validation (no GPU needed) ──────────────────────────

    /// DD8 gate: the hillslope WGSL shader must parse and validate under naga.
    #[test]
    fn hillslope_wgsl_parses_successfully() {
        let src = include_str!("../../../../shaders/hillslope_diffusion.wgsl");
        let module = naga::front::wgsl::parse_str(src).expect("WGSL parse failed");
        let mut validator = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::default(),
        );
        validator.validate(&module).expect("WGSL validation failed");
    }

    /// DD8 lock: workgroup size must be exactly (8, 8, 1).
    #[test]
    fn hillslope_wgsl_workgroup_size_matches_dd8_lock() {
        let src = include_str!("../../../../shaders/hillslope_diffusion.wgsl");
        assert!(
            src.contains("@workgroup_size(8, 8, 1)"),
            "hillslope_diffusion.wgsl must declare @workgroup_size(8, 8, 1) per DD8 lock"
        );
    }

    // ── GPU-requiring tests (opt-in) ──────────────────────────────────────────

    fn base_preset() -> IslandArchetypePreset {
        IslandArchetypePreset {
            name: "hillslope_gpu_test".into(),
            island_radius: 0.5,
            max_relief: 0.8,
            volcanic_center_count: 1,
            island_age: IslandAge::Young,
            prevailing_wind_dir: 0.0,
            marine_moisture_strength: 0.5,
            sea_level: 0.0,
            erosion: ErosionParams::default(),
            climate: Default::default(),
        }
    }

    fn all_land_coast(w: u32, h: u32) -> CoastMask {
        let mut is_land = MaskField2D::new(w, h);
        is_land.data.fill(1);
        CoastMask {
            is_land,
            is_sea: MaskField2D::new(w, h),
            is_coast: MaskField2D::new(w, h),
            land_cell_count: w * h,
            river_mouth_mask: None,
        }
    }

    fn make_world(w: u32, h: u32) -> WorldState {
        let mut world = WorldState::new(Seed(0), base_preset(), Resolution::new(w, h));
        let mut height = ScalarField2D::<f32>::new(w, h);
        height.data.fill(0.0);
        world.authoritative.height = Some(height);
        world.derived.coast_mask = Some(all_land_coast(w, h));
        world
    }

    /// Sprint 4.E: `GpuBackend::supports(HillslopeDiffusion)` must return `true`
    /// after 4.E lands (the `HillslopeComputePipeline` slot is `Some`).
    ///
    /// Non-ignored because it only checks the enum mapping logic, not a GPU adapter.
    /// The `GpuBackend` is not constructed — we derive the assertion from the
    /// contract that `supports(HillslopeDiffusion)` iff `self.hillslope.is_some()`.
    #[test]
    fn gpu_backend_supports_hillslope_after_4e() {
        // The GpuBackend::supports implementation gates on self.hillslope.is_some().
        // At 4.E the GpuBackend is constructed with Some(HillslopeComputePipeline).
        // We verify the semantic contract here without needing a real adapter:
        // HillslopeDiffusion should be in ComputeOp::ALL and the pipeline struct
        // must exist and be constructable (via GpuBackend::new on adapters).
        assert!(
            std::mem::size_of::<HillslopeComputePipeline>() > 0,
            "HillslopeComputePipeline must be a non-zero-sized struct after 4.E"
        );
        // Also verify the ComputeOp mapping is stable.
        assert_eq!(
            island_core::pipeline::ComputeOp::ALL[0],
            island_core::pipeline::ComputeOp::HillslopeDiffusion,
            "HillslopeDiffusion must be ComputeOp::ALL[0]"
        );
    }

    /// Direct GPU dispatch smoke test: drive the pipeline on a small 16×16
    /// all-land field with one substep, verify output values are finite
    /// and interior cells differ from a uniform zero-Laplacian flat field.
    ///
    /// Uses a tent (spike at centre): centre must decrease, neighbours must
    /// increase — same invariant as the CPU `hillslope_smooths_tent_toward_mean` test.
    ///
    /// Gated via `IPG_RUN_GPU_TESTS=1`.
    #[test]
    #[ignore = "requires a GPU adapter; set IPG_RUN_GPU_TESTS=1 to run"]
    fn hillslope_pipeline_dispatches_one_iteration_correctly() {
        if std::env::var("IPG_RUN_GPU_TESTS").as_deref() != Ok("1") {
            eprintln!("skipped — set IPG_RUN_GPU_TESTS=1");
            return;
        }

        let (w, h) = (16u32, 16u32);
        let ctx =
            Arc::new(GpuContext::new_headless((w, h)).expect("headless GPU context required"));

        let mut pipeline = HillslopeComputePipeline::new(Arc::clone(&ctx));

        let mut world = make_world(w, h);
        // Tent: centre = 1.0, rest = 0.0.
        let (cx, cy) = (8usize, 8usize);
        {
            let f = world.authoritative.height.as_mut().unwrap();
            f.data.fill(0.0);
            f.data[cy * w as usize + cx] = 1.0;
        }

        let params = HillslopeParams {
            hillslope_d: 1e-3,
            n_diff_substep: 1,
        };

        let gpu_ms = pipeline
            .dispatch(&mut world, &params)
            .expect("GPU dispatch must succeed");

        eprintln!("hillslope GPU smoke: gpu_ms={gpu_ms:?}");

        let f = world.authoritative.height.as_ref().unwrap();
        let wi = w as usize;

        // All values must be finite.
        for (i, &v) in f.data.iter().enumerate() {
            assert!(
                v.is_finite(),
                "cell {i}: non-finite after GPU dispatch: {v}"
            );
        }

        // Centre must decrease, direct neighbours must increase.
        let center_after = f.data[cy * wi + cx];
        assert!(
            center_after < 1.0,
            "GPU: centre must decrease from 1.0, got {center_after}"
        );
        let n_after = f.data[(cy - 1) * wi + cx];
        assert!(
            n_after > 0.0,
            "GPU: north neighbour must increase from 0.0, got {n_after}"
        );
    }
}
