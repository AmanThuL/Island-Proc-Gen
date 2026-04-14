//! §3.2 A3 procedural sky gradient renderer.
//!
//! `SkyRenderer` draws a full-screen gradient (horizon → zenith) using a
//! single-triangle "no-VBO" trick driven by `@builtin(vertex_index)`.  It
//! writes colour but NOT depth, so the terrain's depth test against the
//! cleared 1.0 depth buffer still works correctly when `draw()` is called
//! first in the terrain pass.

use bytemuck::{Pod, Zeroable};
use gpu::GpuContext;
use wgpu::util::DeviceExt as _;

// ── SkyUniform ────────────────────────────────────────────────────────────────

/// GPU-side sky uniform — 32 bytes, field order must match `sky.wgsl::Sky`.
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct SkyUniform {
    horizon: [f32; 4], // 16 bytes — bottom of screen
    zenith: [f32; 4],  // 16 bytes — top of screen
}

// ── SkyRenderer ───────────────────────────────────────────────────────────────

/// Sprint 1A §3.2 A3 sky gradient renderer.
///
/// Draws a full-screen vertical gradient.  Call [`draw`] BEFORE
/// [`TerrainRenderer::draw`] in the same render pass so the sky forms the
/// background and the terrain depth test still resolves correctly.
pub struct SkyRenderer {
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
}

impl SkyRenderer {
    /// Construct the sky renderer from an already-initialised [`GpuContext`].
    pub fn new(gpu: &GpuContext) -> Self {
        let device = &gpu.device;

        // ── Shader ────────────────────────────────────────────────────────────
        let wgsl_src = include_str!("../../../shaders/sky.wgsl");
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("sky_shader"),
            source: wgpu::ShaderSource::Wgsl(wgsl_src.into()),
        });

        // ── Uniform buffer ────────────────────────────────────────────────────
        let uniform_data = SkyUniform {
            horizon: crate::palette::SKY_HORIZON,
            zenith: crate::palette::SKY_ZENITH,
        };
        let buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("sky_uniform_buf"),
            contents: bytemuck::cast_slice(&[uniform_data]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // ── Bind group layout (single FRAGMENT uniform at binding 0) ──────────
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sky_bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        // ── Bind group ────────────────────────────────────────────────────────
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("sky_bg"),
            layout: &bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buf.as_entire_binding(),
            }],
        });
        // `buf` drops here — wgpu 29 BindGroup refcounts its bound buffers.

        // ── Pipeline layout ───────────────────────────────────────────────────
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("sky_pipeline_layout"),
            bind_group_layouts: &[Some(&bgl)],
            immediate_size: 0,
        });

        // ── Render pipeline ───────────────────────────────────────────────────
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("sky_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_sky"),
                // No VBO — vertex_index drives the full-screen triangle.
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_sky"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: gpu.surface_format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                // Single unordered triangle — no culling to avoid orientation risk.
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            // LOAD-BEARING: write=false leaves the cleared 1.0 depth intact so
            // the terrain's Less test still resolves; compare=Always lets the
            // sky itself draw over the cleared 1.0 (Less would reject it).
            depth_stencil: Some(wgpu::DepthStencilState {
                format: gpu.depth_format,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::Always),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview_mask: None,
            cache: None,
        });

        Self {
            pipeline,
            bind_group,
        }
    }

    /// Record sky draw call into `render_pass`.
    ///
    /// Must be called BEFORE [`TerrainRenderer::draw`] in the same pass.
    pub fn draw<'rp>(&'rp self, render_pass: &mut wgpu::RenderPass<'rp>) {
        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.bind_group, &[]);
        render_pass.draw(0..3, 0..1);
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    #[test]
    fn sky_wgsl_parses_successfully() {
        let src = include_str!("../../../shaders/sky.wgsl");
        use naga::front::wgsl;
        use naga::valid::{Capabilities, ValidationFlags, Validator};
        let module = wgsl::parse_str(src).expect("WGSL parse failed");
        let mut validator = Validator::new(ValidationFlags::all(), Capabilities::all());
        validator.validate(&module).expect("WGSL validation failed");
    }

    #[test]
    fn sky_wgsl_has_no_literal_colors() {
        let src = include_str!("../../../shaders/sky.wgsl");
        assert!(
            !src.contains('#'),
            "sky.wgsl contains '#' — possible hex literal"
        );
        assert!(
            !src.contains("vec3<f32>(0.") && !src.contains("vec3<f32>(1."),
            "sky.wgsl contains vec3 colour literal"
        );
        assert!(
            !src.contains("vec4<f32>(0.") && !src.contains("vec4<f32>(1."),
            "sky.wgsl contains vec4 colour literal"
        );
    }
}
