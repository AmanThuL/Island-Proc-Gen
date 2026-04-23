//! Sprint 3.5.B c4 — `HexRiverRenderer`: polyline-with-thickness river pass.
//!
//! Renders one instanced draw pass per frame that paints river threads as
//! two-segment polylines: `entry_midpoint → hex_center` and
//! `hex_center → exit_midpoint`. Adjacent hexes share edge midpoints by
//! construction, so the visual result reads as a continuous river thread
//! across the hex grid.
//!
//! ## Per-instance layout (DD3 §2)
//!
//! Each [`HexRiverInstance`] is exactly **12 bytes**:
//!
//! | Field               | Type    | Offset | Size | Notes                          |
//! |---------------------|---------|--------|------|--------------------------------|
//! | `hex_center_xy`     | `[f32;2]` | 0    | 8    | World-space hex centre         |
//! | `edges_and_width_bits` | `u32` | 8     | 4    | Packed: entry(8) exit(8) width(8) pad(8) |
//!
//! Total: 12 bytes. Byte budget locked by `hex_river_instance_is_12_bytes`.
//!
//! ## Procedural unit mesh
//!
//! 12 vertices (two quads of 6 vertices each = two triangle-list segments).
//! Segment 0 maps to the `entry_midpoint → hex_center` half.
//! Segment 1 maps to the `hex_center → exit_midpoint` half.
//! No index buffer — direct triangle-list draw.
//!
//! ## Shader
//!
//! `shaders/hex_river.wgsl` — embedded via `include_str!`. Contains no RGB
//! literals per CLAUDE.md invariant. River colour comes from the Uniforms
//! struct (`river_color: vec4<f32>`), populated from `PaletteId::River` on
//! the Rust side.
//!
//! ## Depth / blend
//!
//! `(depth_write=false, depth_compare=Always)` — mirrors [`super::hex_surface`]
//! so rivers paint on top of the hex fill pass. Alpha blending is enabled on
//! the colour target so segment overlaps near the hex centre composite cleanly.

use std::mem::size_of;

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt as _;

// ── Depth-state contract ──────────────────────────────────────────────────────

/// Depth behaviour for the hex-river pipeline — `(depth_write, depth_compare)`.
///
/// `(false, Always)` ensures rivers are drawn on top of the hex surface fill
/// (even where the terrain depth would otherwise clip them) and do not write
/// depth themselves (so subsequent overlay passes are unaffected).
///
/// Referenced by both [`HexRiverRenderer::new`] and the
/// `hex_river_pipeline_uses_no_depth_write` contract-lock test so the two
/// can never drift silently.
pub(crate) const HEX_RIVER_DEPTH_STATE: (bool, wgpu::CompareFunction) =
    (false, wgpu::CompareFunction::Always);

// ── HexRiverVertex ────────────────────────────────────────────────────────────

/// Unit-segment vertex for the procedural river polyline mesh.
///
/// Two fields encode position within a segment "quad":
/// - `local_pos.x ∈ {0.0, 1.0}` — start vs end of this segment half
/// - `local_pos.y ∈ {-0.5, 0.5}` — right vs left edge of the centreline
/// - `segment_id ∈ {0, 1}` — entry half (`entry_mid → center`) vs exit half
///   (`center → exit_mid`)
///
/// The vertex shader resolves the actual world-space position from these
/// coordinates combined with the per-instance `hex_center_xy` and edge indices.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct HexRiverVertex {
    /// `(t, n)` in segment-local space:
    ///   `t ∈ {0.0, 1.0}` along the segment (0 = start, 1 = end)
    ///   `n ∈ {-0.5, 0.5}` across the segment (-0.5 = right, 0.5 = left)
    pub local_pos: [f32; 2],
    /// 0 = entry half (`entry_midpoint → hex_center`),
    /// 1 = exit half (`hex_center → exit_midpoint`).
    pub segment_id: u32,
}

impl HexRiverVertex {
    /// Vertex buffer layout: locations 0 and 1, 12-byte stride.
    ///
    /// location 0 — `local_pos`  (Float32x2, offset 0)
    /// location 1 — `segment_id` (Uint32,    offset 8)
    pub fn layout() -> wgpu::VertexBufferLayout<'static> {
        const ATTRIBS: [wgpu::VertexAttribute; 2] =
            wgpu::vertex_attr_array![0 => Float32x2, 1 => Uint32];
        wgpu::VertexBufferLayout {
            array_stride: size_of::<HexRiverVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &ATTRIBS,
        }
    }
}

// ── HexRiverInstance ─────────────────────────────────────────────────────────

/// Per-instance GPU attributes for one hex river thread.  Exactly **12 bytes**.
///
/// Edge indices and width bucket are packed into a single `u32` to avoid the
/// `VertexFormat::Uint8` dance (wgpu 29 does support it but the packing
/// approach avoids per-attribute complexity):
///
/// ```text
/// bits  0..7  = entry_edge  (HexEdge discriminant 0..=5)
/// bits  8..15 = exit_edge   (HexEdge discriminant 0..=5)
/// bits 16..23 = width_bucket (RiverWidth discriminant 0..=2)
/// bits 24..31 = _pad
/// ```
///
/// Use [`HexRiverInstance::pack`] to build the value from typed fields.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct HexRiverInstance {
    /// World-space hex centre (XZ plane), from `axial_to_pixel` + scale.
    pub hex_center_xy: [f32; 2],
    /// Packed: `entry_edge | exit_edge<<8 | width_bucket<<16 | _pad<<24`.
    pub edges_and_width_bits: u32,
}

impl HexRiverInstance {
    /// Construct a packed instance from typed edge + width values.
    ///
    /// # Arguments
    /// - `hex_center_xy`: world-space hex centre
    /// - `entry_edge`: `HexEdge` discriminant 0..=5
    /// - `exit_edge`: `HexEdge` discriminant 0..=5
    /// - `width_bucket`: `RiverWidth` discriminant 0..=2
    #[inline]
    pub fn pack(hex_center_xy: [f32; 2], entry_edge: u8, exit_edge: u8, width_bucket: u8) -> Self {
        let bits = (entry_edge as u32) | ((exit_edge as u32) << 8) | ((width_bucket as u32) << 16);
        Self {
            hex_center_xy,
            edges_and_width_bits: bits,
        }
    }

    /// Vertex buffer layout for the per-instance step.
    ///
    /// location 2 — `hex_center_xy`        (Float32x2, offset 0)
    /// location 3 — `edges_and_width_bits` (Uint32,    offset 8)
    pub fn layout() -> wgpu::VertexBufferLayout<'static> {
        const ATTRIBS: [wgpu::VertexAttribute; 2] =
            wgpu::vertex_attr_array![2 => Float32x2, 3 => Uint32];
        wgpu::VertexBufferLayout {
            array_stride: size_of::<HexRiverInstance>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &ATTRIBS,
        }
    }
}

// ── Uniforms ──────────────────────────────────────────────────────────────────

/// View-projection + hex_size + river_color uniform — 112 bytes.
///
/// Mirrors `struct Uniforms` in `shaders/hex_river.wgsl` byte-for-byte:
///
/// | Field        | Offset | Size | Notes                                  |
/// |--------------|--------|------|----------------------------------------|
/// | `view_proj`  |  0     | 64   | mat4x4<f32>                            |
/// | `hex_size`   | 64     |  4   | world-space centre-to-vertex radius    |
/// | `_pad0`      | 68     | 12   | three f32 scalars (NOT vec3<f32>)      |
/// | `river_color`| 80     | 16   | vec4<f32> RGBA river tint              |
/// | `_pad1`      | 96     | 16   | reserved                               |
///
/// Total: 112 bytes. `#[repr(C)]` + `bytemuck::Pod` guarantees byte layout
/// matches the WGSL struct. Verified by `uniforms_buffer_size_matches_wgsl_layout`.
///
/// **Why three f32 scalars for `_pad0`?** WGSL's `vec3<f32>` has 16-byte
/// alignment, which would silently pad the struct to 128 bytes and diverge
/// from the Rust layout. Three `f32` scalars pack at 4-byte alignment and
/// stay within the 112-byte budget.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct HexRiverUniforms {
    view_proj: [[f32; 4]; 4], //  0..64 bytes
    hex_size: f32,            // 64..68 bytes
    _pad0: [f32; 3],          // 68..80 bytes — three scalars, NOT vec3<f32>
    river_color: [f32; 4],    // 80..96 bytes — RGBA river colour
    _pad1: [f32; 4],          // 96..112 bytes — reserved
}

// ── CPU-side mesh builder ─────────────────────────────────────────────────────

/// Build the 12 unit-segment vertices for the two-quad river mesh.
///
/// The mesh encodes two triangle-list quads (6 vertices each, 12 total):
/// - Vertices 0..6 (`segment_id = 0`): entry half — `entry_midpoint → hex_center`
/// - Vertices 6..12 (`segment_id = 1`): exit half — `hex_center → exit_midpoint`
///
/// Each quad uses the standard two-triangle tessellation:
/// ```text
/// v0(0,-0.5) v1(1,-0.5)
/// v2(0, 0.5) v1(1,-0.5) v3(1, 0.5) v2(0, 0.5)   [CCW triangles]
/// ```
///
/// The vertex shader resolves actual world positions from `local_pos` and the
/// per-instance `hex_center_xy` + edge indices + hex_size.
pub(crate) fn build_hex_river_vertices() -> [HexRiverVertex; 12] {
    let quad = |sid: u32| -> [HexRiverVertex; 6] {
        [
            HexRiverVertex {
                local_pos: [0.0, -0.5],
                segment_id: sid,
            },
            HexRiverVertex {
                local_pos: [1.0, -0.5],
                segment_id: sid,
            },
            HexRiverVertex {
                local_pos: [0.0, 0.5],
                segment_id: sid,
            },
            HexRiverVertex {
                local_pos: [1.0, -0.5],
                segment_id: sid,
            },
            HexRiverVertex {
                local_pos: [1.0, 0.5],
                segment_id: sid,
            },
            HexRiverVertex {
                local_pos: [0.0, 0.5],
                segment_id: sid,
            },
        ]
    };

    let seg0 = quad(0);
    let seg1 = quad(1);

    [
        seg0[0], seg0[1], seg0[2], seg0[3], seg0[4], seg0[5], seg1[0], seg1[1], seg1[2], seg1[3],
        seg1[4], seg1[5],
    ]
}

// ── HexRiverRenderer ─────────────────────────────────────────────────────────

/// Sprint 3.5.B c4 hex river renderer — polyline-with-thickness pass.
///
/// Owns a procedural two-quad unit mesh (12 vertices, shared across all
/// instances) and a resizable per-instance buffer. A single
/// `draw(0..12, 0..instance_count)` call renders all active river hexes.
///
/// Rivers draw on top of the hex surface fill pass via
/// `(depth_write=false, depth_compare=Always)`. Alpha blending composites
/// overlapping segments near hex centres naturally.
pub struct HexRiverRenderer {
    // ── Procedural unit mesh (12 vertices, shared) ────────────────────────────
    /// 12 vertices encoding two CCW quads (segment 0 + segment 1).
    vertex_buffer: wgpu::Buffer,

    // ── Per-instance attribute buffer ─────────────────────────────────────────
    /// Holds [`HexRiverInstance`] structs. Reallocated when capacity is exceeded.
    instance_buffer: wgpu::Buffer,
    /// Number of instances the buffer can hold without reallocation.
    instance_capacity: u32,
    /// Number of instances currently active (uploaded and ready to draw).
    instance_count: u32,

    // ── Pipeline + bind group ─────────────────────────────────────────────────
    pipeline: wgpu::RenderPipeline,
    /// Uniform buffer: view_proj + hex_size + river_color (112 bytes).
    view_proj_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    /// Retained for resize-path bind group reconstruction after buffer realloc.
    #[allow(dead_code)]
    bind_group_layout: wgpu::BindGroupLayout,
}

impl HexRiverRenderer {
    /// Minimum instance buffer capacity (avoids zero-byte allocations on first
    /// construction before any hex grid is available).
    const MIN_INSTANCE_CAPACITY: u32 = 64;

    /// Construct the renderer on the given GPU device.
    ///
    /// `color_format` and `depth_format` must match the render pass targets
    /// that [`draw`] will be called within — identical to
    /// [`super::hex_surface::HexSurfaceRenderer::new`].
    ///
    /// The renderer starts with zero active instances — call [`upload_instances`]
    /// before the first [`draw`].
    pub fn new(
        device: &wgpu::Device,
        color_format: wgpu::TextureFormat,
        depth_format: wgpu::TextureFormat,
    ) -> Self {
        // ── Procedural unit mesh ──────────────────────────────────────────────
        let river_verts = build_hex_river_vertices();
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("hex_river_vbo"),
            contents: bytemuck::cast_slice(&river_verts),
            usage: wgpu::BufferUsages::VERTEX,
        });

        // ── Per-instance buffer (initial minimum capacity) ────────────────────
        let initial_capacity = Self::MIN_INSTANCE_CAPACITY;
        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("hex_river_instance_buf"),
            size: (initial_capacity as u64) * size_of::<HexRiverInstance>() as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // ── Uniforms (identity defaults until first update call) ───────────────
        // River colour initialised from palette::RIVER at construction time.
        let river_rgba = crate::palette::RIVER;
        let identity_uniforms = HexRiverUniforms {
            view_proj: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
            hex_size: 1.0,
            _pad0: [0.0; 3],
            river_color: river_rgba,
            _pad1: [0.0; 4],
        };
        let view_proj_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("hex_river_uniform_buf"),
            contents: bytemuck::cast_slice(&[identity_uniforms]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // ── Bind group layout (single uniform at binding 0) ───────────────────
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("hex_river_bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                // Both VERTEX (view_proj, hex_size) and FRAGMENT (river_color) read
                // from the same uniform binding — must be visible to both stages.
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("hex_river_bg"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: view_proj_buffer.as_entire_binding(),
            }],
        });

        // ── Shader ────────────────────────────────────────────────────────────
        let wgsl_src = include_str!("../../../shaders/hex_river.wgsl");
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("hex_river_shader"),
            source: wgpu::ShaderSource::Wgsl(wgsl_src.into()),
        });

        // ── Pipeline layout ───────────────────────────────────────────────────
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("hex_river_pipeline_layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        // ── Render pipeline ───────────────────────────────────────────────────
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("hex_river_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[HexRiverVertex::layout(), HexRiverInstance::layout()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: color_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: depth_format,
                // Single source of truth for hex-river depth semantics —
                // both the pipeline and `hex_river_pipeline_uses_no_depth_write`
                // read `HEX_RIVER_DEPTH_STATE`.
                depth_write_enabled: Some(HEX_RIVER_DEPTH_STATE.0),
                depth_compare: Some(HEX_RIVER_DEPTH_STATE.1),
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
            vertex_buffer,
            instance_buffer,
            instance_capacity: initial_capacity,
            instance_count: 0,
            pipeline,
            view_proj_buffer,
            bind_group,
            bind_group_layout,
        }
    }

    /// Upload a new instance batch, resizing the GPU buffer if necessary.
    ///
    /// After this call, `instance_count == instances.len()`. If the slice is
    /// empty the renderer skips the draw call on the next [`draw`].
    pub fn upload_instances(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        instances: &[HexRiverInstance],
    ) {
        let needed = instances.len() as u32;
        if needed > self.instance_capacity {
            let new_capacity = needed.max(self.instance_capacity * 2);
            self.instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("hex_river_instance_buf"),
                size: (new_capacity as u64) * size_of::<HexRiverInstance>() as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.instance_capacity = new_capacity;
        }
        if !instances.is_empty() {
            queue.write_buffer(&self.instance_buffer, 0, bytemuck::cast_slice(instances));
        }
        self.instance_count = needed;
    }

    /// Update the view-projection matrix, hex_size, and river colour uniform.
    ///
    /// Call once per frame before [`draw`], passing the same matrix used by the
    /// terrain + hex surface renderers. River colour is always sourced from
    /// `palette::RIVER` and re-uploaded here so the palette can be updated
    /// without recreating the renderer.
    pub fn update_view_projection(
        &self,
        queue: &wgpu::Queue,
        view_proj: &[[f32; 4]; 4],
        hex_size: f32,
    ) {
        let uniforms = HexRiverUniforms {
            view_proj: *view_proj,
            hex_size,
            _pad0: [0.0; 3],
            river_color: crate::palette::RIVER,
            _pad1: [0.0; 4],
        };
        queue.write_buffer(&self.view_proj_buffer, 0, bytemuck::cast_slice(&[uniforms]));
    }

    /// Record a river draw into the given render pass.
    ///
    /// If `instance_count == 0` the call is a no-op — no GPU work is submitted.
    /// The caller must have set up the render pass with the colour + depth
    /// targets matching `color_format`/`depth_format` from [`new`].
    ///
    /// Each instance draws 12 vertices (2 segments × 6 vertices): the vertex
    /// shader reconstructs world positions from `local_pos`, `segment_id`, and
    /// the per-instance packed edge + width data.
    pub fn draw<'rp>(&'rp self, pass: &mut wgpu::RenderPass<'rp>) {
        if self.instance_count == 0 {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
        pass.draw(0..12, 0..self.instance_count);
    }

    /// Returns the depth state values used when constructing the pipeline.
    ///
    /// Reads `HEX_RIVER_DEPTH_STATE` — the same const used by the pipeline —
    /// so the contract lock test has a single source of truth.
    #[cfg(test)]
    pub(crate) fn depth_state_for_test() -> (bool, wgpu::CompareFunction) {
        HEX_RIVER_DEPTH_STATE
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── hex_river_instance_is_12_bytes ────────────────────────────────────────

    /// DD3 §2 budget lock: `HexRiverInstance` must be exactly 12 bytes.
    #[test]
    fn hex_river_instance_is_12_bytes() {
        assert_eq!(
            size_of::<HexRiverInstance>(),
            12,
            "HexRiverInstance must be exactly 12 bytes per DD3 §2"
        );
    }

    // ── hex_river_vertex_layout ───────────────────────────────────────────────

    /// The procedural unit mesh must have exactly 12 vertices (2 quads × 6).
    #[test]
    fn hex_river_vertex_layout() {
        let verts = build_hex_river_vertices();
        assert_eq!(verts.len(), 12, "unit river mesh must have 12 vertices");

        // Segment 0: first 6 vertices all have segment_id == 0.
        for (i, v) in verts[..6].iter().enumerate() {
            assert_eq!(
                v.segment_id, 0,
                "vertex {i}: expected segment_id=0 (entry half)"
            );
        }
        // Segment 1: next 6 vertices all have segment_id == 1.
        for (i, v) in verts[6..].iter().enumerate() {
            assert_eq!(
                v.segment_id,
                1,
                "vertex {}: expected segment_id=1 (exit half)",
                i + 6
            );
        }

        // Check local_pos values are in {0.0, 1.0} × {-0.5, 0.5}.
        for (i, v) in verts.iter().enumerate() {
            let [t, n] = v.local_pos;
            assert!(
                t == 0.0 || t == 1.0,
                "vertex {i}: local_pos.x must be 0.0 or 1.0, got {t}"
            );
            assert!(
                (n - (-0.5)).abs() < 1e-6 || (n - 0.5).abs() < 1e-6,
                "vertex {i}: local_pos.y must be -0.5 or 0.5, got {n}"
            );
        }
    }

    // ── hex_river_pipeline_uses_no_depth_write ────────────────────────────────

    /// Contract lock: hex river pipeline must use depth_write=false +
    /// compare=Always so rivers draw on top of the hex surface fill pass.
    #[test]
    fn hex_river_pipeline_uses_no_depth_write() {
        let (write_enabled, compare) = HexRiverRenderer::depth_state_for_test();
        assert!(
            !write_enabled,
            "hex river must NOT write depth (depth_write_enabled = false); \
             rivers must overlay the hex surface pass"
        );
        assert_eq!(
            compare,
            wgpu::CompareFunction::Always,
            "hex river must use CompareFunction::Always so it paints on top \
             of the hex surface in HexOverlay / HexOnly modes"
        );
    }

    // ── hex_river_instance_pack_roundtrip ─────────────────────────────────────

    /// `HexRiverInstance::pack` must encode entry/exit/width into the bits
    /// field such that the WGSL bit-unpack expressions recover the originals.
    #[test]
    fn hex_river_instance_pack_roundtrip() {
        let inst = HexRiverInstance::pack([1.0, 2.0], 3, 5, 2);
        let bits = inst.edges_and_width_bits;
        let entry = (bits) & 0xFF;
        let exit = (bits >> 8) & 0xFF;
        let width = (bits >> 16) & 0xFF;
        assert_eq!(entry, 3, "entry_edge must round-trip through pack bits");
        assert_eq!(exit, 5, "exit_edge must round-trip through pack bits");
        assert_eq!(width, 2, "width_bucket must round-trip through pack bits");
        assert_eq!(inst.hex_center_xy, [1.0, 2.0]);
    }

    // ── uniforms_buffer_size_matches_wgsl_layout ──────────────────────────────

    /// Two-sided layout lock: both `HexRiverUniforms` (Rust) AND the WGSL
    /// `Uniforms` struct must be exactly 112 bytes. Checking Rust alone misses
    /// the case where WGSL alignment rules silently bump the struct span — e.g.
    /// `vec3<f32>` has 16-byte alignment and pads the struct to 128 bytes.
    ///
    /// This test parses the WGSL via naga and reads the computed struct span,
    /// mirroring the `uniforms_buffer_size_matches_wgsl_layout` pattern in
    /// `hex_surface.rs`.
    #[test]
    fn uniforms_buffer_size_matches_wgsl_layout() {
        // Rust side.
        assert_eq!(
            size_of::<HexRiverUniforms>(),
            112,
            "HexRiverUniforms must be exactly 112 bytes to match shaders/hex_river.wgsl"
        );

        // WGSL side — parse the shader and compute the Uniforms struct span.
        let src = include_str!("../../../shaders/hex_river.wgsl");
        use naga::front::wgsl;
        let module = wgsl::parse_str(src).expect("hex_river.wgsl must parse");
        let uniforms_type = module
            .types
            .iter()
            .find_map(|(_, t)| {
                t.name
                    .as_deref()
                    .filter(|n| *n == "Uniforms")
                    .map(|_| t.inner.clone())
            })
            .expect("WGSL module must declare a `Uniforms` struct");
        let naga::TypeInner::Struct { span, .. } = uniforms_type else {
            panic!("`Uniforms` must be a struct type");
        };
        assert_eq!(
            span, 112,
            "WGSL `Uniforms` struct must be exactly 112 bytes; naga computed \
             {span}. Check that _pad0 uses three f32 scalars (not vec3<f32>); \
             vec3<f32> has 16-byte alignment and would push the struct to 128 bytes."
        );
    }

    // ── hex_river_wgsl_parses_successfully ────────────────────────────────────

    /// `shaders/hex_river.wgsl` must parse and validate through naga.
    #[test]
    fn hex_river_wgsl_parses_successfully() {
        let src = include_str!("../../../shaders/hex_river.wgsl");
        use naga::front::wgsl;
        use naga::valid::{Capabilities, ValidationFlags, Validator};
        let module = wgsl::parse_str(src).expect("hex_river.wgsl WGSL parse failed");
        let mut validator = Validator::new(ValidationFlags::all(), Capabilities::all());
        validator
            .validate(&module)
            .expect("hex_river.wgsl WGSL validation failed");
    }

    // ── hex_river_wgsl_has_no_literal_colors ─────────────────────────────────

    /// Guard: `shaders/hex_river.wgsl` must contain no RGB colour literals.
    ///
    /// River colour is sourced exclusively from the `river_color` uniform field,
    /// which the Rust side populates from `palette::RIVER`. No hardcoded values
    /// in the shader.
    #[test]
    fn hex_river_wgsl_has_no_literal_colors() {
        let src = include_str!("../../../shaders/hex_river.wgsl");
        assert!(
            !src.contains('#'),
            "hex_river.wgsl contains '#' — possible hex colour literal"
        );
        assert!(
            !src.contains("vec3<f32>(0.") && !src.contains("vec3<f32>(1."),
            "hex_river.wgsl contains vec3 colour literal"
        );
        assert!(
            !src.contains("vec4<f32>(0.") && !src.contains("vec4<f32>(1."),
            "hex_river.wgsl contains vec4 colour literal"
        );
    }

    // ── hex_river_instance_step_mode_is_instance ──────────────────────────────

    /// `HexRiverInstance::layout()` must use `Instance` step mode.
    #[test]
    fn hex_river_instance_step_mode_is_instance() {
        let layout = HexRiverInstance::layout();
        assert_eq!(
            layout.step_mode,
            wgpu::VertexStepMode::Instance,
            "HexRiverInstance layout must use Instance step mode"
        );
    }
}
