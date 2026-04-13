//! Terrain rendering: Sprint 0 placeholder quad renderer and Sprint 1A CPU
//! mesh builder.
//!
//! The existing `TerrainRenderer` is kept untouched — Runtime still drives it
//! while the window path is live.  The new `build_terrain_mesh` and
//! `build_sea_quad` functions are standalone and used by tests and the
//! upcoming Sprint 1A pipeline.

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

// ── Sprint 1A: CPU-side mesh builder ─────────────────────────────────────────

/// GPU-ready vertex for the Sprint 1A terrain mesh.
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct TerrainVertex {
    pub position: [f32; 3],
    pub normal:   [f32; 3],
    pub uv:       [f32; 2],
}

/// CPU-side mesh ready for GPU upload.
pub struct MeshData {
    pub vertices: Vec<TerrainVertex>,
    pub indices:  Vec<u32>,
}

impl MeshData {
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    pub fn vertex_count(&self) -> usize {
        self.vertices.len()
    }
}

/// Build a triangle mesh from a heightfield.
///
/// Vertices are placed at every cell of `z_filled`.  World-space layout:
/// `x / (W-1)` along X, `y / (H-1)` along Z, elevation on Y.  Matches the
/// Y-up convention used by `camera::view_projection`.
pub fn build_terrain_mesh(z_filled: &island_core::field::ScalarField2D<f32>) -> MeshData {
    let w = z_filled.width as usize;
    let h = z_filled.height as usize;

    let inv_w = if w > 1 { 1.0 / (w - 1) as f32 } else { 1.0 };
    let inv_h = if h > 1 { 1.0 / (h - 1) as f32 } else { 1.0 };

    // ── Normals via central-difference cross product ──────────────────────────

    // Read elevation with boundary clamping.
    let z = |xi: isize, yi: isize| -> f32 {
        let cx = xi.clamp(0, (w - 1) as isize) as u32;
        let cy = yi.clamp(0, (h - 1) as isize) as u32;
        z_filled.get(cx, cy)
    };

    let mut vertices = Vec::with_capacity(w * h);
    for y in 0..h {
        for x in 0..w {
            let xi = x as isize;
            let yi = y as isize;

            // Central diff interior, single-sided at boundaries. The shared
            // `* 0.5` factor below underestimates boundary slopes by half
            // (single-sided has step 1, not 2) — pre-approved for Sprint 1A
            // since §4 Task 1A.9 tolerates a weak edge seam; Sprint 2+ can
            // tighten if render smoothing surfaces it.
            let (z_xm, z_xp) = if x == 0 {
                (z(xi, yi), z(xi + 1, yi))
            } else if x == w - 1 {
                (z(xi - 1, yi), z(xi, yi))
            } else {
                (z(xi - 1, yi), z(xi + 1, yi))
            };

            let (z_ym, z_yp) = if y == 0 {
                (z(xi, yi), z(xi, yi + 1))
            } else if y == h - 1 {
                (z(xi, yi - 1), z(xi, yi))
            } else {
                (z(xi, yi - 1), z(xi, yi + 1))
            };

            let dz_dx = (z_xp - z_xm) * 0.5 * inv_w;
            let dz_dy = (z_yp - z_ym) * 0.5 * inv_h;

            // tangent_x = (inv_w, dz_dx, 0),  tangent_y = (0, dz_dy, inv_h)
            // normal = tangent_y × tangent_x  (yields +Y for a flat plane)
            let tx = [inv_w, dz_dx, 0.0_f32];
            let ty = [0.0_f32, dz_dy, inv_h];
            let nx = ty[1] * tx[2] - ty[2] * tx[1];
            let ny = ty[2] * tx[0] - ty[0] * tx[2];
            let nz = ty[0] * tx[1] - ty[1] * tx[0];
            let len = (nx * nx + ny * ny + nz * nz).sqrt().max(1e-12);
            let normal = [nx / len, ny / len, nz / len];

            vertices.push(TerrainVertex {
                position: [x as f32 * inv_w, z_filled.get(x as u32, y as u32), y as f32 * inv_h],
                normal,
                uv: [x as f32 * inv_w, y as f32 * inv_h],
            });
        }
    }

    // ── Index buffer ─────────────────────────────────────────────────────────

    // Two triangles per cell, CCW when viewed from +Y.
    // v00 = (x, y), v10 = (x+1, y), v01 = (x, y+1), v11 = (x+1, y+1)
    // Triangle A: [v00, v11, v10]
    // Triangle B: [v00, v01, v11]
    let mut indices = Vec::with_capacity(6 * (w - 1) * (h - 1));
    for y in 0..(h - 1) {
        for x in 0..(w - 1) {
            let v00 = (y * w + x) as u32;
            let v10 = (y * w + (x + 1)) as u32;
            let v01 = ((y + 1) * w + x) as u32;
            let v11 = ((y + 1) * w + (x + 1)) as u32;
            indices.extend_from_slice(&[v00, v11, v10, v00, v01, v11]);
        }
    }

    MeshData { vertices, indices }
}

/// Build the sea-plane quad at `y = sea_level`.
///
/// A single CCW 2-triangle quad covering `[0, 1] × [0, 1]` on XZ.
pub fn build_sea_quad(sea_level: f32) -> MeshData {
    let vertices = vec![
        TerrainVertex { position: [0.0, sea_level, 0.0], normal: [0.0, 1.0, 0.0], uv: [0.0, 0.0] },
        TerrainVertex { position: [1.0, sea_level, 0.0], normal: [0.0, 1.0, 0.0], uv: [1.0, 0.0] },
        TerrainVertex { position: [0.0, sea_level, 1.0], normal: [0.0, 1.0, 0.0], uv: [0.0, 1.0] },
        TerrainVertex { position: [1.0, sea_level, 1.0], normal: [0.0, 1.0, 0.0], uv: [1.0, 1.0] },
    ];
    // CCW winding viewed from +Y: triangle A = [0, 2, 1], triangle B = [1, 2, 3]
    let indices = vec![0, 2, 1, 1, 2, 3];
    MeshData { vertices, indices }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use island_core::field::ScalarField2D;

    fn flat_field(w: u32, h: u32, z: f32) -> ScalarField2D<f32> {
        let mut f = ScalarField2D::new(w, h);
        for y in 0..h {
            for x in 0..w {
                f.set(x, y, z);
            }
        }
        f
    }

    fn ramp_x_field(w: u32, h: u32, scale: f32) -> ScalarField2D<f32> {
        let mut f = ScalarField2D::new(w, h);
        for y in 0..h {
            for x in 0..w {
                f.set(x, y, x as f32 * scale);
            }
        }
        f
    }

    #[test]
    fn mesh_vertex_count_matches_grid() {
        let field = flat_field(4, 4, 0.5);
        let mesh = build_terrain_mesh(&field);
        assert_eq!(mesh.vertex_count(), 16);
    }

    #[test]
    fn mesh_triangle_count_matches_grid() {
        let field = flat_field(4, 4, 0.5);
        let mesh = build_terrain_mesh(&field);
        // (W-1)*(H-1)*2 = 3*3*2 = 18
        assert_eq!(mesh.triangle_count(), 18);
    }

    #[test]
    fn flat_plane_normals_point_up() {
        let field = flat_field(8, 8, 0.5);
        let mesh = build_terrain_mesh(&field);
        for v in &mesh.vertices {
            let [nx, ny, nz] = v.normal;
            let len = (nx * nx + ny * ny + nz * nz).sqrt();
            assert!((len - 1.0).abs() < 1e-5, "normal not unit-length: len={len}");
            assert!(ny > 0.99, "normal does not point up: ny={ny}");
        }
    }

    #[test]
    fn inclined_plane_normals_tilt_away() {
        // z = 0.1 * x — ramp in +x, so downhill is +x, normal tilts to -x.
        let field = ramp_x_field(8, 8, 0.1);
        let mesh = build_terrain_mesh(&field);
        // Check interior vertices (avoid boundary rows/cols).
        let w = field.width as usize;
        for y in 1..7usize {
            for x in 1..7usize {
                let v = &mesh.vertices[y * w + x];
                let [nx, ny, nz] = v.normal;
                let len = (nx * nx + ny * ny + nz * nz).sqrt();
                assert!((len - 1.0).abs() < 1e-5, "normal not unit-length at ({x},{y})");
                assert!(nx < 0.0, "interior normal should tilt toward -x (downhill opposite), got nx={nx} at ({x},{y})");
            }
        }
    }

    #[test]
    fn uvs_cover_unit_square() {
        let field = flat_field(4, 4, 0.0);
        let mesh = build_terrain_mesh(&field);
        let w = field.width as usize;
        let eps = 1e-6_f32;

        // vertex (0, 0) → uv [0, 0]
        let uv_00 = mesh.vertices[0].uv;
        assert!((uv_00[0]).abs() < eps && (uv_00[1]).abs() < eps, "uv(0,0) wrong: {:?}", uv_00);

        // vertex (3, 3) → uv [1, 1]
        let uv_33 = mesh.vertices[3 * w + 3].uv;
        assert!((uv_33[0] - 1.0).abs() < eps && (uv_33[1] - 1.0).abs() < eps, "uv(3,3) wrong: {:?}", uv_33);

        // vertex (1, 1) → uv [1/3, 1/3]
        let uv_11 = mesh.vertices[w + 1].uv;
        let third = 1.0_f32 / 3.0;
        assert!((uv_11[0] - third).abs() < eps && (uv_11[1] - third).abs() < eps, "uv(1,1) wrong: {:?}", uv_11);
    }

    #[test]
    fn index_range_is_valid() {
        let field = flat_field(4, 4, 0.0);
        let mesh = build_terrain_mesh(&field);
        let vcount = mesh.vertices.len();
        for &idx in &mesh.indices {
            assert!((idx as usize) < vcount, "index {idx} out of range (vcount={vcount})");
        }
    }

    #[test]
    fn sea_quad_has_4_vertices_and_2_triangles() {
        let mesh = build_sea_quad(0.3);
        assert_eq!(mesh.vertex_count(), 4);
        assert_eq!(mesh.triangle_count(), 2);
        for v in &mesh.vertices {
            assert!((v.position[1] - 0.3).abs() < 1e-6, "y != 0.3: {}", v.position[1]);
        }
    }

    #[test]
    fn terrain_wgsl_has_no_literal_colors() {
        let src = include_str!("../../../shaders/terrain.wgsl");
        // Guard: no hex literals
        assert!(
            !src.contains('#'),
            "terrain.wgsl contains '#' — possible hex color literal"
        );
        // Guard: no vec3/vec4 starting with a digit (RGB literal pattern)
        assert!(
            !src.contains("vec3<f32>(0.") && !src.contains("vec3<f32>(1."),
            "terrain.wgsl contains vec3 color literal"
        );
        assert!(
            !src.contains("vec4<f32>(0.") && !src.contains("vec4<f32>(1."),
            "terrain.wgsl contains vec4 color literal"
        );
    }

    #[test]
    fn terrain_wgsl_parses_successfully() {
        let src = include_str!("../../../shaders/terrain.wgsl");
        use naga::front::wgsl;
        use naga::valid::{Capabilities, ValidationFlags, Validator};
        let module = wgsl::parse_str(src).expect("WGSL parse failed");
        let mut validator = Validator::new(ValidationFlags::all(), Capabilities::all());
        validator.validate(&module).expect("WGSL validation failed");
    }

    #[test]
    fn sea_level_height_matches_input() {
        let mesh = build_sea_quad(0.42);
        assert!((mesh.vertices[0].position[1] - 0.42).abs() < 1e-6);
    }
}
