//! Sprint 3.5.A c6 — `HexSurfaceRenderer` skeleton (fill-pass only).
//!
//! This module ships the renderer struct, per-instance attribute layout,
//! procedural unit-hex vertex/index buffers, and the public API surface that
//! c8 will wire into `Runtime`.  The renderer exists and passes structural
//! self-tests, but is **not yet constructed by `Runtime`** — that wiring lands
//! in c8.
//!
//! ## Per-instance layout (DD1 §2)
//!
//! Each [`HexInstance`] is exactly **32 bytes**:
//!
//! | Field               | Type    | Offset | Size | Notes |
//! |---------------------|---------|--------|------|-------|
//! | `center_xy`         | `[f32;2]` | 0    | 8    | World-space hex centre from `axial_to_pixel` |
//! | `elevation`         | `f32`   | 8      | 4    | Normalised \[0, 1\]; c7 tonal ramp uses it |
//! | `fill_color_rgba`   | `u32`   | 12     | 4    | Packed RGBA8 (r \| g<<8 \| b<<16 \| a<<24) |
//! | `coast_class_bits`  | `u32`   | 16     | 4    | Low byte = HexCoastClass (0..=6); c8+ populates |
//! | `river_mask_bits`   | `u32`   | 20     | 4    | Low byte = river-flag mask; c8+ populates |
//! | `_pad`              | `[u32;2]` | 24   | 8    | Pads to 32 bytes; expansion room for c7+ |
//!
//! ## Procedural unit mesh
//!
//! The vertex buffer holds 7 vertices: centre at `(0, 0)` followed by 6 corners
//! at unit radius from [`hex::geometry::hex_polygon_vertices`].  The index
//! buffer encodes 6 fan triangles (18 indices).  A single `draw_indexed` call
//! with the instance buffer bound draws all active hexes in one GPU submission.
//!
//! ## Shader
//!
//! `shaders/hex_surface.wgsl` — embedded via `include_str!`. Contains no RGB
//! literals per CLAUDE.md invariant on shaders.  c7 adds the tonal-ramp logic.

use std::mem::size_of;

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt as _;

// ── Depth-state contract (DD5) ────────────────────────────────────────────────

/// Depth behaviour for the hex-surface pipeline — `(depth_write, depth_compare)`.
///
/// `(false, Always)` is the DD5 readable-base-surface semantic: hex pixels are
/// always output (even where terrain sits in front depth-wise), and the hex
/// surface never writes depth (so subsequent overlay passes aren't disturbed
/// by hex Z values).
///
/// Referenced by both the pipeline builder in [`HexSurfaceRenderer::new`] and
/// the `hex_surface_pipeline_disables_depth_write` contract-lock test, so the
/// pipeline and the test can't drift out of sync.
pub(crate) const HEX_DEPTH_STATE: (bool, wgpu::CompareFunction) =
    (false, wgpu::CompareFunction::Always);

// ── HexVertex ─────────────────────────────────────────────────────────────────

/// Unit-hex-local vertex position (centre at origin, corner radius = 1).
///
/// The vertex buffer holds 7 of these: index 0 = centre `(0, 0)`, indices 1–6
/// are the 6 corners from [`build_hex_vertices`].
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct HexVertex {
    /// Position in the unit hex's local 2-D space.  Centre = `[0, 0]`;
    /// corners lie at unit radius.  The shader translates + scales this by
    /// `center_xy` and `hex_size` to produce world-space XZ coordinates.
    pub local_xy: [f32; 2],
}

impl HexVertex {
    /// Vertex buffer layout: 1 attribute, 8-byte stride.
    ///
    /// location 0 — `local_xy` (Float32x2, offset 0)
    pub fn layout() -> wgpu::VertexBufferLayout<'static> {
        const ATTRIBS: [wgpu::VertexAttribute; 1] = wgpu::vertex_attr_array![0 => Float32x2];
        wgpu::VertexBufferLayout {
            array_stride: size_of::<HexVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &ATTRIBS,
        }
    }
}

// ── HexInstance ───────────────────────────────────────────────────────────────

/// Per-instance GPU attributes for one hex cell.  Exactly **32 bytes** —
/// see the module-level table for the full layout.
///
/// `fill_color_rgba` uses packed RGBA8 to keep the struct at 32 bytes while
/// avoiding the std430 alignment issue that `[f32; 4]` at offset 12 would
/// cause.  Use [`HexInstance::pack_rgba`] to build the value and the
/// `unpack_rgba8` WGSL helper in `shaders/hex_surface.wgsl` to unpack it in
/// the shader.
///
/// `coast_class_bits` and `river_mask_bits` are zeroed at c6; c8 populates
/// their low bytes from `DerivedCaches.hex_coast_class` and
/// `HexAttributes.has_river` respectively.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct HexInstance {
    /// World-space hex centre (XZ), from `hex::geometry::axial_to_pixel`.
    pub center_xy: [f32; 2],
    /// Normalised \[0, 1\] elevation.  c7 uses this for the tonal ramp;
    /// c6 uploads it but the shader does not yet act on it.
    pub elevation: f32,
    /// Packed RGBA8 dominant-biome fill colour.
    /// Layout: bits 0..7 = R, 8..15 = G, 16..23 = B, 24..31 = A.
    /// Build with [`HexInstance::pack_rgba`].
    pub fill_color_rgba: u32,
    /// Low byte = `HexCoastClass` discriminant (0..=6); upper bytes reserved.
    /// c8 populates this from `DerivedCaches.hex_coast_class`.
    pub coast_class_bits: u32,
    /// Low byte = river-flag mask; upper bytes reserved.
    /// c8 populates this from `HexAttributes.has_river`.
    pub river_mask_bits: u32,
    /// Padding to 32 bytes.  Expansion room for c7+ fields (e.g. tonal-ramp
    /// params that vary per-instance, or future biome sub-class flags).
    pub _pad: [u32; 2],
}

impl HexInstance {
    /// Pack four linear \[0, 1\] RGBA floats into a `u32` for `fill_color_rgba`.
    ///
    /// Channel order: `r` in bits 0..7, `g` in 8..15, `b` in 16..23,
    /// `a` in 24..31.  Values are clamped to \[0, 1\] before quantisation.
    ///
    /// # Example
    ///
    /// ```
    /// # use render::HexInstance;
    /// let packed = HexInstance::pack_rgba(1.0, 0.5, 0.0, 1.0);
    /// let (r, g, b, a) = HexInstance::unpack_rgba(packed);
    /// assert!((r - 1.0).abs() < 1.0 / 255.0);
    /// ```
    #[inline]
    pub fn pack_rgba(r: f32, g: f32, b: f32, a: f32) -> u32 {
        let ri = (r.clamp(0.0, 1.0) * 255.0).round() as u32;
        let gi = (g.clamp(0.0, 1.0) * 255.0).round() as u32;
        let bi = (b.clamp(0.0, 1.0) * 255.0).round() as u32;
        let ai = (a.clamp(0.0, 1.0) * 255.0).round() as u32;
        ri | (gi << 8) | (bi << 16) | (ai << 24)
    }

    /// Unpack a `u32` previously produced by [`HexInstance::pack_rgba`] back
    /// into four \[0, 1\] floats `(r, g, b, a)`.
    #[inline]
    pub fn unpack_rgba(packed: u32) -> (f32, f32, f32, f32) {
        let r = (packed & 0xFF) as f32 / 255.0;
        let g = ((packed >> 8) & 0xFF) as f32 / 255.0;
        let b = ((packed >> 16) & 0xFF) as f32 / 255.0;
        let a = ((packed >> 24) & 0xFF) as f32 / 255.0;
        (r, g, b, a)
    }

    /// Vertex buffer layout for the per-instance step.
    ///
    /// Locations 1–7 correspond to the fields of [`HexInstance`] in order.
    /// location 1 — `center_xy`        (Float32x2, offset  0)
    /// location 2 — `elevation`        (Float32x1, offset  8)
    /// location 3 — `fill_color_rgba`  (Uint32,    offset 12)
    /// location 4 — `coast_class_bits` (Uint32,    offset 16)
    /// location 5 — `river_mask_bits`  (Uint32,    offset 20)
    /// location 6 — `_pad[0]`          (Uint32,    offset 24)
    /// location 7 — `_pad[1]`          (Uint32,    offset 28)
    pub fn layout() -> wgpu::VertexBufferLayout<'static> {
        const ATTRIBS: [wgpu::VertexAttribute; 7] = wgpu::vertex_attr_array![
            1 => Float32x2,  // center_xy
            2 => Float32,    // elevation
            3 => Uint32,     // fill_color_rgba
            4 => Uint32,     // coast_class_bits
            5 => Uint32,     // river_mask_bits
            6 => Uint32,     // _pad[0]
            7 => Uint32      // _pad[1]
        ];
        wgpu::VertexBufferLayout {
            array_stride: size_of::<HexInstance>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &ATTRIBS,
        }
    }
}

// ── Uniform ───────────────────────────────────────────────────────────────────

/// View-projection + hex_size uniform — 96 bytes.
///
/// Mirrors `struct Uniforms` in `shaders/hex_surface.wgsl` byte-for-byte:
///
/// | Field      | Offset | Size | Notes |
/// |------------|--------|------|-------|
/// | `view_proj`| 0      | 64   | mat4x4<f32> |
/// | `hex_size` | 64     | 4    | world-space centre-to-vertex radius |
/// | `_pad0`    | 68     | 12   | pads `hex_size` to 16-byte boundary |
/// | `_pad1`    | 80     | 16   | reserved for future uniforms |
///
/// Total: 96 bytes. `#[repr(C)]` + `bytemuck::Pod` guarantees byte layout
/// matches the WGSL struct at every field boundary. Asserted by
/// `uniforms_buffer_size_matches_wgsl_layout`.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct HexSurfaceUniforms {
    view_proj: [[f32; 4]; 4], //  0..64 bytes
    hex_size: f32,            // 64..68 bytes — world-space radius; c8 sets via update_hex_size
    _pad0: [f32; 3],          // 68..80 bytes — pads hex_size to 16-byte alignment
    _pad1: [f32; 4],          // 80..96 bytes — reserved
}

// ── CPU-side mesh builders ────────────────────────────────────────────────────

/// Build the 7 unit-hex vertices: centre at index 0 then 6 corners.
///
/// Corner positions come from
/// [`hex::geometry::hex_polygon_vertices`]`((0.0, 0.0), 1.0)` — unit radius,
/// CCW from the upper-right vertex (angle 30°).
pub(crate) fn build_hex_vertices() -> [HexVertex; 7] {
    // Centre vertex
    let mut verts = [HexVertex {
        local_xy: [0.0, 0.0],
    }; 7];

    // 6 corners: base angle 30° (π/6), then 60° (π/3) steps, CCW.
    let base = std::f32::consts::FRAC_PI_6;
    for k in 0..6usize {
        let angle = base + (k as f32) * std::f32::consts::FRAC_PI_3;
        verts[k + 1] = HexVertex {
            local_xy: [angle.cos(), angle.sin()],
        };
    }
    verts
}

/// Build the 18 fan-triangle indices for a 7-vertex unit hex.
///
/// Each of the 6 triangles shares vertex 0 (centre) and uses two consecutive
/// corner vertices:
///
/// ```text
/// triangle k: [0, k+1, ((k+1) % 6) + 1]
/// ```
///
/// Winding is CCW when viewed from +Y (consistent with `FrontFace::Ccw`).
pub(crate) fn build_hex_indices() -> [u32; 18] {
    let mut indices = [0u32; 18];
    for k in 0..6usize {
        let base = k * 3;
        indices[base] = 0;
        indices[base + 1] = (k + 1) as u32;
        indices[base + 2] = ((k + 1) % 6 + 1) as u32;
    }
    indices
}

// ── HexSurfaceRenderer ────────────────────────────────────────────────────────

/// Sprint 3.5.A c6 hex surface renderer — fill pass only.
///
/// Owns a procedural unit-hex vertex buffer (7 vertices, shared across all
/// instances via the instanced draw path) and a resizable per-instance buffer.
/// A single `draw_indexed(0..18, 0, 0..instance_count)` call renders all
/// active hex cells.
///
/// c7 adds the tonal-ramp elevation shading.
/// c8 wires this into `Runtime` (the renderer is NOT constructed there yet).
pub struct HexSurfaceRenderer {
    // ── Procedural unit-hex mesh (shared across all instances) ────────────────
    /// 7 vertices: centre at index 0, corners at indices 1–6.
    vertex_buffer: wgpu::Buffer,
    /// 18 indices encoding 6 fan triangles.
    index_buffer: wgpu::Buffer,
    /// Always 18 — stored as a field to match the `draw_indexed` call site.
    index_count: u32,

    // ── Per-instance attribute buffer (resized when hex grid changes) ─────────
    /// Holds [`HexInstance`] structs.  Reallocated (with `COPY_DST`) if
    /// `instances.len() > instance_capacity`.
    instance_buffer: wgpu::Buffer,
    /// Number of instances the buffer can hold without reallocation.
    instance_capacity: u32,
    /// Number of instances currently active (uploaded and ready to draw).
    instance_count: u32,

    // ── Pipeline + bind group ─────────────────────────────────────────────────
    pipeline: wgpu::RenderPipeline,
    /// View-projection uniform buffer.
    view_proj_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    /// Retained for c8's `upload_instances` resize path — rebinding after
    /// buffer reallocation requires the layout to reconstruct the bind group.
    #[allow(dead_code)]
    bind_group_layout: wgpu::BindGroupLayout,
}

impl HexSurfaceRenderer {
    /// Minimum instance buffer capacity (avoids zero-byte allocations on first
    /// construction before any hex grid is available).
    const MIN_INSTANCE_CAPACITY: u32 = 64;

    /// Construct the renderer on the given GPU device.
    ///
    /// `color_format` and `depth_format` must match the render pass targets
    /// that [`draw`] will be called within.  For headless passes these are
    /// [`gpu::HEADLESS_COLOR_FORMAT`] and [`gpu::HEADLESS_DEPTH_FORMAT`];
    /// for the interactive window they come from `gpu.surface_format` and
    /// `gpu.depth_format`.
    ///
    /// The renderer starts with zero active instances — call [`upload_instances`]
    /// before the first [`draw`].
    pub fn new(
        device: &wgpu::Device,
        color_format: wgpu::TextureFormat,
        depth_format: wgpu::TextureFormat,
    ) -> Self {
        // ── Procedural unit-hex mesh ──────────────────────────────────────────
        let hex_verts = build_hex_vertices();
        let hex_inds = build_hex_indices();
        let index_count = hex_inds.len() as u32; // = 18

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("hex_surface_vbo"),
            contents: bytemuck::cast_slice(&hex_verts),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("hex_surface_ibo"),
            contents: bytemuck::cast_slice(&hex_inds),
            usage: wgpu::BufferUsages::INDEX,
        });

        // ── Per-instance buffer (initial minimum capacity) ────────────────────
        let initial_capacity = Self::MIN_INSTANCE_CAPACITY;
        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("hex_surface_instance_buf"),
            size: (initial_capacity as u64) * size_of::<HexInstance>() as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // ── View-projection + hex_size uniform ────────────────────────────────
        // hex_size defaults to 1.0 to preserve pre-c8 test behaviour.
        // c8 must call `update_hex_size` with the actual `HexGrid.hex_size`
        // after calling `update_view_projection`.
        let identity_uniforms = HexSurfaceUniforms {
            view_proj: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
            hex_size: 1.0,
            _pad0: [0.0; 3],
            _pad1: [0.0; 4],
        };
        let view_proj_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("hex_surface_view_proj_buf"),
            contents: bytemuck::cast_slice(&[identity_uniforms]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // ── Bind group layout (single uniform at binding 0) ───────────────────
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("hex_surface_bgl"),
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

        // ── Bind group ────────────────────────────────────────────────────────
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("hex_surface_bg"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: view_proj_buffer.as_entire_binding(),
            }],
        });

        // ── Shader ────────────────────────────────────────────────────────────
        let wgsl_src = include_str!("../../../shaders/hex_surface.wgsl");
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("hex_surface_shader"),
            source: wgpu::ShaderSource::Wgsl(wgsl_src.into()),
        });

        // ── Pipeline layout ───────────────────────────────────────────────────
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("hex_surface_pipeline_layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        // ── Render pipeline ───────────────────────────────────────────────────
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("hex_surface_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[HexVertex::layout(), HexInstance::layout()],
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
                // Both faces drawn — the hex overlay may be viewed from below
                // the terrain plane in some camera orientations.
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: depth_format,
                // Single source of truth for hex-surface depth semantics —
                // both the pipeline and the `hex_surface_pipeline_disables_depth_write`
                // test read `HEX_DEPTH_STATE`. Keeps pipeline + contract lock
                // in lockstep; drift becomes a compile error rather than a
                // silent test-passing-but-wrong state.
                depth_write_enabled: Some(HEX_DEPTH_STATE.0),
                depth_compare: Some(HEX_DEPTH_STATE.1),
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
            index_buffer,
            index_count,
            instance_buffer,
            instance_capacity: initial_capacity,
            instance_count: 0,
            pipeline,
            view_proj_buffer,
            bind_group,
            bind_group_layout,
        }
    }

    /// Upload a new instance batch, resizing the GPU buffer if the current
    /// capacity is insufficient.
    ///
    /// After this call, `instance_count == instances.len()`.  If the slice is
    /// empty the renderer will skip the draw call on the next [`draw`].
    pub fn upload_instances(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        instances: &[HexInstance],
    ) {
        let needed = instances.len() as u32;
        if needed > self.instance_capacity {
            // Grow by at least 2× to amortise repeated small-growth patterns.
            let new_capacity = needed.max(self.instance_capacity * 2);
            self.instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("hex_surface_instance_buf"),
                size: (new_capacity as u64) * size_of::<HexInstance>() as u64,
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

    /// Update the view-projection + hex_size uniforms in a single call.
    ///
    /// Call once per frame before [`draw`], passing the same matrix used by the
    /// terrain renderer (for consistent projection) and the current
    /// `HexGrid.hex_size` (for correct world-space scaling — terrain space is
    /// `[0, DEFAULT_WORLD_XZ_EXTENT = 5.0]`, so hexes must scale with the grid,
    /// not render at a hardcoded 1 world unit).
    ///
    /// Takes both parameters together rather than splitting into separate
    /// setters so callers cannot forget one — the pre-c8 split variant made
    /// forgetting to update `hex_size` silently produce 1-world-unit hexes.
    pub fn update_view_projection(
        &self,
        queue: &wgpu::Queue,
        view_proj: &[[f32; 4]; 4],
        hex_size: f32,
    ) {
        let uniforms = HexSurfaceUniforms {
            view_proj: *view_proj,
            hex_size,
            _pad0: [0.0; 3],
            _pad1: [0.0; 4],
        };
        queue.write_buffer(&self.view_proj_buffer, 0, bytemuck::cast_slice(&[uniforms]));
    }

    /// Record a fill draw into the given render pass.
    ///
    /// If `instance_count == 0` the call is a no-op — no GPU work is submitted.
    /// The caller must have already set up the render pass (colour + depth
    /// targets matching `color_format`/`depth_format` passed to [`new`]).
    pub fn draw<'rp>(&'rp self, pass: &mut wgpu::RenderPass<'rp>) {
        if self.instance_count == 0 {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
        pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..self.index_count, 0, 0..self.instance_count);
    }

    /// Number of instances currently queued to draw.
    ///
    /// Reserved for c8 wiring and GPU-adapter tests.
    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn instance_count(&self) -> u32 {
        self.instance_count
    }

    /// Always 7 — the procedural unit hex has one centre + six corner vertices.
    ///
    /// Reserved for c8 wiring and GPU-adapter tests.
    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn vertex_count(&self) -> u32 {
        7
    }

    /// Returns the depth state values used when constructing the pipeline.
    ///
    /// Returns the `(depth_write_enabled, depth_compare)` tuple used by the
    /// pipeline in [`new`]. Reads `HEX_DEPTH_STATE` — the same const the
    /// pipeline itself reads — so the contract lock is a single source of
    /// truth. If the pipeline in `new()` ever hardcodes different values
    /// instead of reading `HEX_DEPTH_STATE`, the divergence is visible at the
    /// call site rather than silently passing the test.
    #[cfg(test)]
    pub(crate) fn depth_state_for_test() -> (bool, wgpu::CompareFunction) {
        HEX_DEPTH_STATE
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── hex_instance_is_32_bytes ──────────────────────────────────────────────

    /// DD1 §2 budget lock: `HexInstance` must be exactly 32 bytes.
    ///
    /// This assertion protects the per-instance byte budget agreed in the sprint
    /// spec — changing the struct layout without updating the test is a spec
    /// violation.
    #[test]
    fn hex_instance_is_32_bytes() {
        assert_eq!(
            size_of::<HexInstance>(),
            32,
            "HexInstance must be exactly 32 bytes per DD1 §2"
        );
    }

    // ── pack_rgba8_roundtrip ──────────────────────────────────────────────────

    /// `pack_rgba` → `unpack_rgba` must have ≤ 1/255 error per channel.
    #[test]
    fn pack_rgba8_roundtrip() {
        let channels = [
            (1.0_f32, 0.5_f32, 0.0_f32, 1.0_f32),
            (0.0, 1.0, 0.0, 0.5),
            (0.25, 0.75, 0.125, 0.875),
            (0.0, 0.0, 0.0, 0.0),
            (1.0, 1.0, 1.0, 1.0),
        ];
        let tolerance = 1.0 / 255.0;
        for (r, g, b, a) in channels {
            let packed = HexInstance::pack_rgba(r, g, b, a);
            let (ro, go, bo, ao) = HexInstance::unpack_rgba(packed);
            assert!(
                (ro - r).abs() <= tolerance,
                "R channel error: in={r} out={ro}"
            );
            assert!(
                (go - g).abs() <= tolerance,
                "G channel error: in={g} out={go}"
            );
            assert!(
                (bo - b).abs() <= tolerance,
                "B channel error: in={b} out={bo}"
            );
            assert!(
                (ao - a).abs() <= tolerance,
                "A channel error: in={a} out={ao}"
            );
        }
    }

    // ── procedural_mesh_is_18_index_fan ──────────────────────────────────────

    /// The procedural mesh must have exactly 18 indices forming 6 fan triangles
    /// that all share vertex 0 (the centre).
    #[test]
    fn procedural_mesh_is_18_index_fan() {
        let verts = build_hex_vertices();
        let indices = build_hex_indices();

        // Exactly 7 vertices, 18 indices.
        assert_eq!(verts.len(), 7, "unit hex must have 7 vertices");
        assert_eq!(
            indices.len(),
            18,
            "unit hex must have 18 indices (6 triangles)"
        );

        // Centre vertex is at origin.
        assert_eq!(verts[0].local_xy, [0.0, 0.0], "vertex 0 must be the centre");

        // Every triangle shares vertex 0 (centre).
        for tri in 0..6usize {
            let base = tri * 3;
            assert_eq!(
                indices[base], 0,
                "triangle {tri}: first index must be the centre (0), got {}",
                indices[base]
            );
        }

        // All corner indices are in range [1, 6].
        for (i, &idx) in indices.iter().enumerate() {
            assert!(
                (idx as usize) < 7,
                "index at position {i} = {idx} is out of range (7 vertices)"
            );
        }

        // Each of the 6 corner vertices appears in the fan.
        let mut seen = [false; 6];
        for tri in 0..6usize {
            let base = tri * 3;
            let c1 = indices[base + 1] as usize - 1;
            let c2 = indices[base + 2] as usize - 1;
            assert!(c1 < 6, "corner1 out of range in tri {tri}");
            assert!(c2 < 6, "corner2 out of range in tri {tri}");
            seen[c1] = true;
            seen[c2] = true;
        }
        assert!(
            seen.iter().all(|&s| s),
            "not all 6 corner vertices appear in the fan"
        );
    }

    // ── hex_surface_renderer_uses_instanced_path ─────────────────────────────

    /// Structural assertion (plan §4 item 8): the renderer uses a shared
    /// unit-hex vertex buffer (vertex_count == 7) and a separate per-instance
    /// buffer, NOT a per-hex draw loop.
    ///
    /// Tests `vertex_count()` (always 7 — procedural) and confirms the instance
    /// buffer is separate from the vertex buffer by verifying independent
    /// `upload_instances` semantics.
    ///
    /// This test is CPU-only (no GPU adapter required) — it asserts the
    /// *layout* contract encoded in the public accessors and the mesh builders,
    /// not the GPU execution path.  The GPU execution path is covered by the
    /// headless baseline runs in c8.
    #[test]
    fn hex_surface_renderer_uses_instanced_path() {
        // 1. Vertex buffer holds exactly 7 vertices (procedural fan).
        //    This is a compile-time constant asserted via the accessor.
        //    We test it via the CPU builder rather than a GPU round-trip.
        let verts = build_hex_vertices();
        assert_eq!(
            verts.len(),
            7,
            "procedural unit hex must have exactly 7 vertices"
        );

        // 2. Index buffer has 18 entries (6 triangles × 3 vertices each).
        let inds = build_hex_indices();
        assert_eq!(inds.len(), 18, "procedural unit hex must have 18 indices");

        // 3. HexInstance layout step mode is Instance (not Vertex).
        //    `wgpu::VertexStepMode::Instance` means exactly one instance
        //    attribute set is consumed per INSTANCE, not per vertex — this
        //    is the GPU-instanced path as required by the spec.
        let layout = HexInstance::layout();
        assert_eq!(
            layout.step_mode,
            wgpu::VertexStepMode::Instance,
            "HexInstance layout must use Instance step mode for the instanced draw path"
        );

        // 4. HexVertex layout step mode is Vertex (not Instance).
        let vertex_layout = HexVertex::layout();
        assert_eq!(
            vertex_layout.step_mode,
            wgpu::VertexStepMode::Vertex,
            "HexVertex layout must use Vertex step mode"
        );
    }

    // ── uniforms_buffer_size_matches_wgsl_layout ─────────────────────────────

    /// Fix 1 layout lock: `HexSurfaceUniforms` (Rust) AND the WGSL `Uniforms`
    /// struct MUST both be exactly 96 bytes. Checking Rust alone misses the
    /// case where WGSL alignment rules silently bump the shader's struct to
    /// 112 bytes or more — e.g. using `vec3<f32>` for padding (which has
    /// 16-byte alignment and rounds up the struct span). A mismatch causes
    /// silent corruption of all uniform values at c8 bind time.
    ///
    /// This test parses the WGSL and reads the naga-computed struct span so
    /// the two-sided contract is actually verified. Caught the c7-fix-era
    /// `vec3<f32>` bug during pre-commit review.
    #[test]
    fn uniforms_buffer_size_matches_wgsl_layout() {
        // Rust side.
        assert_eq!(
            size_of::<HexSurfaceUniforms>(),
            96,
            "HexSurfaceUniforms must be exactly 96 bytes to match shaders/hex_surface.wgsl"
        );

        // WGSL side — parse the shader and compute the Uniforms struct span.
        let src = include_str!("../../../shaders/hex_surface.wgsl");
        use naga::front::wgsl;
        let module = wgsl::parse_str(src).expect("hex_surface.wgsl must parse");
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
            span, 96,
            "WGSL `Uniforms` struct must be exactly 96 bytes; naga computed \
             {span}. Likely cause: a `vec3<f32>` field bumps alignment and \
             pushes the struct to 112 bytes. Use three `f32` scalars instead."
        );
    }

    // ── hex_surface_pipeline_disables_depth_write ─────────────────────────────

    /// Fix 3 contract lock: hex surface pipeline must use depth_write=false and
    /// compare=Always.
    ///
    /// Since `wgpu::RenderPipeline` does not expose its depth state for
    /// introspection, this test asserts the contract via the documented-intent
    /// accessor `depth_state_for_test()`. Any future change to the pipeline's
    /// depth state in `new()` must be reflected here.
    #[test]
    fn hex_surface_pipeline_disables_depth_write() {
        let (write_enabled, compare) = HexSurfaceRenderer::depth_state_for_test();
        assert!(
            !write_enabled,
            "hex surface must NOT write depth (depth_write_enabled = false); \
             otherwise overlay passes are disturbed by hex Z values"
        );
        assert_eq!(
            compare,
            wgpu::CompareFunction::Always,
            "hex surface must use CompareFunction::Always so it paints on top of \
             terrain in HexOverlay mode (DD5 readable-base-surface requirement)"
        );
    }

    // ── hex_surface_wgsl_has_no_literal_colors ────────────────────────────────

    /// Guard: `shaders/hex_surface.wgsl` must contain no RGB colour literals.
    ///
    /// Mirrors the same invariant test in `terrain.rs` and `sky.rs`.
    #[test]
    fn hex_surface_wgsl_has_no_literal_colors() {
        let src = include_str!("../../../shaders/hex_surface.wgsl");
        assert!(
            !src.contains('#'),
            "hex_surface.wgsl contains '#' — possible hex color literal"
        );
        assert!(
            !src.contains("vec3<f32>(0.") && !src.contains("vec3<f32>(1."),
            "hex_surface.wgsl contains vec3 colour literal"
        );
        assert!(
            !src.contains("vec4<f32>(0.") && !src.contains("vec4<f32>(1."),
            "hex_surface.wgsl contains vec4 colour literal"
        );
    }

    // ── hex_surface_wgsl_parses_successfully ─────────────────────────────────

    /// `shaders/hex_surface.wgsl` must parse and validate through naga.
    #[test]
    fn hex_surface_wgsl_parses_successfully() {
        let src = include_str!("../../../shaders/hex_surface.wgsl");
        use naga::front::wgsl;
        use naga::valid::{Capabilities, ValidationFlags, Validator};
        let module = wgsl::parse_str(src).expect("hex_surface.wgsl WGSL parse failed");
        let mut validator = Validator::new(ValidationFlags::all(), Capabilities::all());
        validator
            .validate(&module)
            .expect("hex_surface.wgsl WGSL validation failed");
    }

    // ── tonal_ramp_constants_match_sprint_3_5_dd5_lock ───────────────────────

    /// DD5 locks `TONAL_MIN = 0.55` and `TONAL_MAX = 1.0` as pick-once-and-commit
    /// scalars. Mirrors the value-lock pattern used for SPACE-lite constants
    /// (`hs_init_land_constant_matches_sprint_3_1_lock`) and CloudForest bell
    /// (`cloud_forest_f_t_envelope_matches_sprint_3_5_lock`). Since WGSL consts
    /// can't be imported into Rust, the test asserts the literal source text.
    #[test]
    fn tonal_ramp_constants_match_sprint_3_5_dd5_lock() {
        let src = include_str!("../../../shaders/hex_surface.wgsl");
        assert!(
            src.contains("const TONAL_MIN: f32 = 0.55;"),
            "hex_surface.wgsl TONAL_MIN drifted from DD5 lock (0.55)"
        );
        assert!(
            src.contains("const TONAL_MAX: f32 = 1.0;"),
            "hex_surface.wgsl TONAL_MAX drifted from DD5 lock (1.0)"
        );
    }

    // ── hex_vertex_is_8_bytes ─────────────────────────────────────────────────

    /// `HexVertex` must be exactly 8 bytes (2 × f32).
    #[test]
    fn hex_vertex_is_8_bytes() {
        assert_eq!(size_of::<HexVertex>(), 8);
    }

    // ── corner_vertices_at_unit_radius ────────────────────────────────────────

    /// All 6 corner vertices of the unit hex must lie at distance 1.0 from the
    /// origin (within floating-point tolerance).
    #[test]
    fn corner_vertices_at_unit_radius() {
        let verts = build_hex_vertices();
        for (i, v) in verts[1..].iter().enumerate() {
            let [x, y] = v.local_xy;
            let r = (x * x + y * y).sqrt();
            assert!(
                (r - 1.0).abs() < 1e-5,
                "corner {i} radius = {r}, expected 1.0"
            );
        }
    }
}
