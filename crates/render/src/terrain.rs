//! Placeholder terrain renderer — draws a single coloured quad on the XZ
//! plane at y = 0.
//!
//! Sprint 1A will replace this with a real height-mesh pipeline. For Sprint 0
//! the goal is just to prove the wgpu render pipeline compiles and produces
//! visible output.

use bytemuck::{Pod, Zeroable};
use glam::Mat4;
use gpu::GpuContext;

// ── WGSL ─────────────────────────────────────────────────────────────────────

const TERRAIN_WGSL: &str = r#"
struct Uniforms {
    view_proj: mat4x4<f32>,
}
@group(0) @binding(0) var<uniform> u: Uniforms;

struct VsIn {
    @location(0) position: vec3<f32>,
    @location(1) color:    vec3<f32>,
}
struct VsOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0)       color:         vec3<f32>,
}

@vertex
fn vs_main(input: VsIn) -> VsOut {
    var out: VsOut;
    out.clip_position = u.view_proj * vec4<f32>(input.position, 1.0);
    out.color = input.color;
    return out;
}

@fragment
fn fs_main(input: VsOut) -> @location(0) vec4<f32> {
    return vec4<f32>(input.color, 1.0);
}
"#;

// ── Vertex layout ─────────────────────────────────────────────────────────────

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct Vertex {
    position: [f32; 3],
    color: [f32; 3],
}

impl Vertex {
    const ATTRIBS: [wgpu::VertexAttribute; 2] = wgpu::vertex_attr_array![
        0 => Float32x3,
        1 => Float32x3
    ];

    fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBS,
        }
    }
}

// Quad vertices: XZ plane at y = 0
//   (-1, 0, -1) → red
//   ( 1, 0, -1) → green
//   ( 1, 0,  1) → blue
//   (-1, 0,  1) → yellow
const VERTICES: [Vertex; 4] = [
    Vertex {
        position: [-1.0, 0.0, -1.0],
        color: [1.0, 0.0, 0.0],
    },
    Vertex {
        position: [1.0, 0.0, -1.0],
        color: [0.0, 1.0, 0.0],
    },
    Vertex {
        position: [1.0, 0.0, 1.0],
        color: [0.0, 0.0, 1.0],
    },
    Vertex {
        position: [-1.0, 0.0, 1.0],
        color: [1.0, 1.0, 0.0],
    },
];

const INDICES: [u16; 6] = [0, 1, 2, 0, 2, 3];

// ── Uniform buffer ─────────────────────────────────────────────────────────────

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct Uniforms {
    view_proj: [[f32; 4]; 4],
}

// ── TerrainRenderer ───────────────────────────────────────────────────────────

/// Draws a single placeholder quad. Holds a wgpu render pipeline, vertex /
/// index buffers, and a uniform buffer for the view-projection matrix.
pub struct TerrainRenderer {
    pipeline: wgpu::RenderPipeline,
    vertex_buf: wgpu::Buffer,
    index_buf: wgpu::Buffer,
    uniform_buf: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}

impl TerrainRenderer {
    /// Construct the renderer from an already-initialised [`GpuContext`].
    pub fn new(gpu: &GpuContext) -> Self {
        use wgpu::util::DeviceExt as _;

        let device = &gpu.device;

        // Shader
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("terrain_shader"),
            source: wgpu::ShaderSource::Wgsl(TERRAIN_WGSL.into()),
        });

        // Vertex buffer
        let vertex_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("terrain_vertices"),
            contents: bytemuck::cast_slice(&VERTICES),
            usage: wgpu::BufferUsages::VERTEX,
        });

        // Index buffer
        let index_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("terrain_indices"),
            contents: bytemuck::cast_slice(&INDICES),
            usage: wgpu::BufferUsages::INDEX,
        });

        // Uniform buffer (view-projection matrix, identity for now)
        let identity = Uniforms {
            view_proj: Mat4::IDENTITY.to_cols_array_2d(),
        };
        let uniform_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("terrain_uniforms"),
            contents: bytemuck::cast_slice(&[identity]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // Bind group layout
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("terrain_bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        // Bind group
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("terrain_bg"),
            layout: &bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buf.as_entire_binding(),
            }],
        });

        // Pipeline layout
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("terrain_pipeline_layout"),
            bind_group_layouts: &[Some(&bgl)],
            immediate_size: 0,
        });

        // Render pipeline — no depth attachment for Sprint 0
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("terrain_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[Vertex::layout()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
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
                cull_mode: None, // show both sides of the quad
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None, // no depth buffer in Sprint 0
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
            vertex_buf,
            index_buf,
            uniform_buf,
            bind_group,
        }
    }

    /// Upload an updated view-projection matrix before each frame.
    pub fn update_view_proj(&self, queue: &wgpu::Queue, view_proj: Mat4) {
        let uniforms = Uniforms {
            view_proj: view_proj.to_cols_array_2d(),
        };
        queue.write_buffer(&self.uniform_buf, 0, bytemuck::cast_slice(&[uniforms]));
    }

    /// Record the terrain draw call into `render_pass`.
    pub fn draw<'rp>(&'rp self, render_pass: &mut wgpu::RenderPass<'rp>) {
        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.vertex_buf.slice(..));
        render_pass.set_index_buffer(self.index_buf.slice(..), wgpu::IndexFormat::Uint16);
        render_pass.draw_indexed(0..INDICES.len() as u32, 0, 0..1);
    }
}
