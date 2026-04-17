//! GPU overlay renderer — Sprint 1A Task 1A.10.
//!
//! `OverlayRenderer` bakes each registered [`OverlayDescriptor`] into a
//! per-descriptor RGBA8 texture at construction time, then draws all visible
//! overlays over the terrain in index order during [`draw`].
//!
//! ## Design notes
//!
//! * All `&'static str` field-key dispatch is confined to
//!   [`crate::overlay::resolve_scalar_source`]. This module only sees the
//!   typed [`ResolvedField`] enum.
//! * The terrain's vertex/index buffers are shared by cloning the
//!   [`wgpu::Buffer`] handles (wgpu 29 buffers are `Clone` / Arc-backed).
//! * The terrain's view uniform buffer is also shared: `update_view` writes
//!   once per frame and both passes see the latest value.
//! * Depth: `LessEqual` + `depth_write_enabled = false` so overlays paint on
//!   the terrain surface without occluding each other.

use bytemuck::{Pod, Zeroable};
use gpu::GpuContext;
use island_core::world::WorldState;
use wgpu::util::DeviceExt as _;

use crate::overlay::{
    OverlayDescriptor, OverlayRegistry, ResolvedField, ValueRange, resolve_scalar_source,
};
use crate::palette;
use crate::terrain::TerrainVertex;

// ── OverlayAlphaUniform ───────────────────────────────────────────────────────

/// Per-overlay alpha uniform — 16 bytes (one `vec4<f32>`).
///
/// Only `.x` carries the alpha; `.y/.z/.w` are std140 padding. WebGPU/naga
/// requires minimum 16-byte uniform buffer alignment.
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct OverlayAlphaUniform {
    alpha: [f32; 4], // only [0] used, rest is padding
}

// ── OverlayBake ───────────────────────────────────────────────────────────────

/// GPU resources baked for one [`OverlayDescriptor`].
///
/// Only the bind group is held here — wgpu 29 `BindGroup` internally stores an
/// `Arc<TextureView>` (which in turn holds `Arc<Texture>`), `Arc<Sampler>`, and
/// `Arc<Buffer>`, so every bound resource stays alive for the bind group's
/// lifetime. This mirrors how `sky.rs` drops its local uniform buffer after
/// `create_bind_group`.
struct OverlayBake {
    bind_group: wgpu::BindGroup,
}

// ── OverlayRenderer ───────────────────────────────────────────────────────────

/// Sprint 1A overlay render pass.
///
/// One texture + bind group per registered descriptor. Visible overlays are
/// drawn in index order over the terrain surface in `draw`. Sprint 1B
/// slider re-runs call [`OverlayRenderer::refresh`] to re-bake all
/// descriptors against an updated `WorldState`.
pub struct OverlayRenderer {
    pipeline: wgpu::RenderPipeline,

    /// Group 0 bind group — binds the terrain-owned view uniform buffer.
    group0_bg: wgpu::BindGroup,

    /// Group 1 bind group layout — kept for `refresh` re-baking.
    bgl1: wgpu::BindGroupLayout,

    /// Per-descriptor bake (None if the field was not populated at boot).
    entries: Vec<Option<OverlayBake>>,

    /// Terrain vertex buffer — shared via `Arc` incref, no GPU copy.
    terrain_vbo: wgpu::Buffer,
    /// Terrain index buffer — shared via `Arc` incref, no GPU copy.
    terrain_ibo: wgpu::Buffer,
    /// Number of terrain indices for `draw_indexed`.
    terrain_index_count: u32,
}

impl OverlayRenderer {
    /// Build the overlay renderer.
    ///
    /// Must be called AFTER [`TerrainRenderer::new`] so that `z_filled` and
    /// other `DerivedCaches` fields are populated.
    pub fn new(
        gpu: &GpuContext,
        world: &WorldState,
        registry: &OverlayRegistry,
        view_buf: &wgpu::Buffer,
        terrain_vbo: &wgpu::Buffer,
        terrain_ibo: &wgpu::Buffer,
        terrain_index_count: u32,
    ) -> Self {
        let device = &gpu.device;
        let queue = &gpu.queue;

        // ── Shader ────────────────────────────────────────────────────────────
        let wgsl_src = include_str!("../../../shaders/overlay.wgsl");
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("overlay_shader"),
            source: wgpu::ShaderSource::Wgsl(wgsl_src.into()),
        });

        // ── Blue noise texture (§3.2 B3 dither) ──────────────────────────────
        // Sprint 1A Pass 4.2 — overlay shader samples the same 64×64 R8 blue
        // noise as terrain.wgsl for ±½ LSB perceptual dither. Loaded locally
        // instead of shared from TerrainRenderer: the asset is 4 KB and
        // inter-renderer coupling isn't worth the save.
        let noise = crate::noise::load_blue_noise_2d(64);
        let blue_noise_tex = device.create_texture_with_data(
            &gpu.queue,
            &wgpu::TextureDescriptor {
                label: Some("overlay_blue_noise_tex"),
                size: wgpu::Extent3d {
                    width: noise.width,
                    height: noise.height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::R8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            &noise.data,
        );
        let blue_noise_view = blue_noise_tex.create_view(&wgpu::TextureViewDescriptor::default());
        let blue_noise_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("overlay_blue_noise_sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });

        // ── Group 0 bind group layout — view uniform + blue noise ─────────────
        let bgl0 = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("overlay_bgl0"),
            entries: &[
                // binding 0: view uniform
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // binding 1: blue noise texture
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                // binding 2: blue noise sampler
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        // ── Group 0 bind group — shared view buffer + blue noise ──────────────
        // `create_bind_group` refcounts all bound resources internally.
        // `blue_noise_tex` / `blue_noise_view` / `blue_noise_sampler` drop at
        // the end of `new()` — the bind group keeps them alive via Arc, exactly
        // as `bake_descriptor` drops its local texture/sampler locals.
        let group0_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("overlay_bg0"),
            layout: &bgl0,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: view_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&blue_noise_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&blue_noise_sampler),
                },
            ],
        });

        // ── Group 1 bind group layout — per-descriptor resources ──────────────
        let bgl1 = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("overlay_bgl1"),
            entries: &[
                // binding 0: alpha uniform
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // binding 1: overlay texture
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                // binding 2: sampler
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        // ── Pipeline layout ───────────────────────────────────────────────────
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("overlay_pipeline_layout"),
            bind_group_layouts: &[Some(&bgl0), Some(&bgl1)],
            immediate_size: 0,
        });

        // ── Render pipeline ───────────────────────────────────────────────────
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("overlay_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_overlay"),
                buffers: &[TerrainVertex::layout()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_overlay"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: gpu.surface_format,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::SrcAlpha,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent::OVER,
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            // Read depth but do NOT write it — overlays don't occlude each other.
            // LessEqual lets overlays paint exactly on the terrain surface
            // (depth == terrain depth after the terrain pass wrote it).
            depth_stencil: Some(wgpu::DepthStencilState {
                format: gpu.depth_format,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
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

        // ── Per-descriptor bake ───────────────────────────────────────────────
        let entries: Vec<Option<OverlayBake>> = registry
            .all()
            .iter()
            .enumerate()
            .map(|(idx, desc)| bake_descriptor(device, queue, &bgl1, desc, world, idx))
            .collect();

        Self {
            pipeline,
            group0_bg,
            bgl1,
            entries,
            // `Buffer::clone` is an `Arc` incref — no GPU copy.
            terrain_vbo: terrain_vbo.clone(),
            terrain_ibo: terrain_ibo.clone(),
            terrain_index_count,
        }
    }

    /// Re-bake every descriptor against the current `world`. Slider
    /// re-runs call this after `SimulationPipeline::run_from` finishes
    /// to pick up the new field values. Previous per-descriptor bakes
    /// drop with the replaced `entries` vec — their internal `Arc`
    /// refcounts release the old textures / samplers / uniform buffers.
    ///
    /// Cost is one `bake_descriptor` per entry (one CPU texture bake +
    /// one GPU upload per overlay). For 12 overlays on a 256² field
    /// that's ~3MB of transient CPU allocation and 12 texture uploads
    /// per slider tick — plenty fast on a modern GPU. Sprint 2 can
    /// refine if profiling disagrees.
    pub fn refresh(&mut self, gpu: &GpuContext, world: &WorldState, registry: &OverlayRegistry) {
        let device = &gpu.device;
        let queue = &gpu.queue;
        self.entries = registry
            .all()
            .iter()
            .enumerate()
            .map(|(idx, desc)| bake_descriptor(device, queue, &self.bgl1, desc, world, idx))
            .collect();
    }

    /// Draw all visible overlays into `rpass`.
    ///
    /// Must be called AFTER [`TerrainRenderer::draw`] in the same render pass.
    /// The view uniform is already up-to-date (written by `terrain.update_view`).
    pub fn draw<'rp>(&'rp self, rpass: &mut wgpu::RenderPass<'rp>, registry: &OverlayRegistry) {
        // Set the shared state once before the per-overlay loop.
        rpass.set_pipeline(&self.pipeline);
        rpass.set_bind_group(0, &self.group0_bg, &[]);
        rpass.set_vertex_buffer(0, self.terrain_vbo.slice(..));
        rpass.set_index_buffer(self.terrain_ibo.slice(..), wgpu::IndexFormat::Uint32);

        for (idx, desc) in registry.all().iter().enumerate() {
            if !desc.visible {
                continue;
            }
            let Some(bake) = self.entries[idx].as_ref() else {
                continue;
            };
            rpass.set_bind_group(1, &bake.bind_group, &[]);
            rpass.draw_indexed(0..self.terrain_index_count, 0, 0..1);
        }
    }
}

// ── Bake helpers ─────────────────────────────────────────────────────────────

/// CPU-side bake: resolve + rasterise one descriptor to RGBA8.
///
/// Returns `(rgba8_row_major, width, height)` or `None` if the field is
/// not yet populated in `world`.
pub fn render_overlay_to_gpu(
    desc: &OverlayDescriptor,
    world: &WorldState,
) -> Option<(Vec<u8>, u32, u32)> {
    // Collect per-cell values as f32 plus width/height.
    let (width, height, values) = match resolve_scalar_source(world, desc.source)? {
        ResolvedField::F32(f) => (f.width, f.height, f.data.to_vec()),
        ResolvedField::U32(f) => (
            f.width,
            f.height,
            f.data.iter().map(|&v| v as f32).collect(),
        ),
        ResolvedField::Mask(m) => (
            m.width,
            m.height,
            m.data.iter().map(|&v| v as f32).collect(),
        ),
    };

    // Per-value transform: LogCompressed works in ln(1 + max(v, 0)) space;
    // all other ranges use identity.
    //
    // `ValueRange::LogCompressed::resolve()` applies the same ln transform to
    // the min/max internally — callers pass RAW extents. The per-pixel pass
    // here must apply the same transform to stay consistent (no double-log).
    let transform = |v: f32| -> f32 {
        match desc.value_range {
            ValueRange::LogCompressed => (1.0_f32 + v.max(0.0)).ln(),
            _ => v,
        }
    };

    // RAW field min/max (LogCompressed transform is applied inside `resolve`).
    let (raw_min, raw_max) = values
        .iter()
        .copied()
        .fold((f32::INFINITY, f32::NEG_INFINITY), |(mn, mx), v| {
            (mn.min(v), mx.max(v))
        });
    let (raw_min, raw_max) = if raw_min.is_finite() && raw_max.is_finite() {
        (raw_min, raw_max)
    } else {
        (0.0, 1.0)
    };

    let (lo, hi) = desc.value_range.resolve(raw_min, raw_max);
    let span = (hi - lo).max(1e-6);

    let mut rgba = Vec::with_capacity(width as usize * height as usize * 4);
    for &v in &values {
        let t = ((transform(v) - lo) / span).clamp(0.0, 1.0);
        rgba.extend_from_slice(&palette::sample(desc.palette, t));
    }

    Some((rgba, width, height))
}

/// Build an [`OverlayBake`] for one descriptor, or return `None` if the field
/// is not yet in `world`.
///
/// `texture`, `sampler`, and `alpha_buf` are all dropped at the end of this
/// function. The bind group keeps them alive via its internal `Arc` refs —
/// see [`OverlayBake`] for the full chain.
fn bake_descriptor(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    bgl1: &wgpu::BindGroupLayout,
    desc: &OverlayDescriptor,
    world: &WorldState,
    idx: usize,
) -> Option<OverlayBake> {
    let (rgba_bytes, width, height) = render_overlay_to_gpu(desc, world)?;

    // ── Upload texture ────────────────────────────────────────────────────────
    let texture = device.create_texture_with_data(
        queue,
        &wgpu::TextureDescriptor {
            label: Some(&format!("overlay_texture_{idx}")),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        },
        wgpu::util::TextureDataOrder::LayerMajor,
        &rgba_bytes,
    );
    let tex_view = texture.create_view(&wgpu::TextureViewDescriptor::default());

    // ── Sampler — Nearest / ClampToEdge ──────────────────────────────────────
    // Nearest is correct: the overlay is a cell-exact palette lookup.
    // Linear would bleed colours across mask boundaries.
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some(&format!("overlay_sampler_{idx}")),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Nearest,
        min_filter: wgpu::FilterMode::Nearest,
        mipmap_filter: wgpu::MipmapFilterMode::Nearest,
        ..Default::default()
    });

    // ── Alpha uniform — Sprint 1A default: 0.6 ───────────────────────────────
    let alpha_data = OverlayAlphaUniform {
        alpha: [0.6, 0.0, 0.0, 0.0],
    };
    let alpha_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(&format!("overlay_alpha_buf_{idx}")),
        contents: bytemuck::cast_slice(&[alpha_data]),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });

    // ── Group 1 bind group ────────────────────────────────────────────────────
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(&format!("overlay_bg1_{idx}")),
        layout: bgl1,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: alpha_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&tex_view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
        ],
    });

    Some(OverlayBake { bind_group })
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use island_core::{
        field::{MaskField2D, ScalarField2D},
        preset::{IslandAge, IslandArchetypePreset},
        seed::Seed,
        world::{Resolution, WorldState},
    };

    use crate::overlay::{OverlayDescriptor, OverlaySource, ValueRange};
    use crate::palette::{self, PaletteId};

    fn test_preset() -> IslandArchetypePreset {
        IslandArchetypePreset {
            name: "test".into(),
            island_radius: 0.5,
            max_relief: 0.5,
            volcanic_center_count: 1,
            island_age: IslandAge::Young,
            prevailing_wind_dir: 0.0,
            marine_moisture_strength: 0.5,
            sea_level: 0.3,
        }
    }

    fn test_world() -> WorldState {
        WorldState::new(Seed(0), test_preset(), Resolution::new(4, 4))
    }

    // 1. Elevation bake: known ramp values map to the expected palette colours.
    #[test]
    fn render_overlay_to_gpu_elevation_matches_palette() {
        let mut world = test_world();

        // Build a 4×1 field with values [0.0, 0.25, 0.5, 0.75].
        // Width=4, Height=1 so row_major index == x.
        let mut field = ScalarField2D::<f32>::new(4, 1);
        field.set(0, 0, 0.0);
        field.set(1, 0, 0.25);
        field.set(2, 0, 0.5);
        field.set(3, 0, 0.75);
        world.derived.z_filled = Some(field);

        let desc = OverlayDescriptor {
            id: "test_elev",
            label: "Test elevation",
            source: OverlaySource::ScalarDerived("z_filled"),
            palette: PaletteId::TerrainHeight,
            value_range: ValueRange::Auto,
            visible: true,
        };

        let (rgba, width, height) = render_overlay_to_gpu(&desc, &world)
            .expect("z_filled is populated, should return Some");

        assert_eq!(width, 4);
        assert_eq!(height, 1);
        assert_eq!(rgba.len(), 16); // 4 pixels × 4 bytes

        // With Auto, lo=0.0, hi=0.75. Check pixel 0 (t=0) and pixel 3 (t=1).
        // t=0.0 → min of [0.0,0.25,0.5,0.75] → t=(0.0-0.0)/(0.75)=0.0
        let expected_t0 = palette::sample(PaletteId::TerrainHeight, 0.0);
        assert_eq!(&rgba[0..4], &expected_t0, "pixel 0 should match t=0.0");

        // t=1.0 → max = 0.75 → t=(0.75-0.0)/0.75 = 1.0
        let expected_t1 = palette::sample(PaletteId::TerrainHeight, 1.0);
        assert_eq!(&rgba[12..16], &expected_t1, "pixel 3 should match t=1.0");

        // t=0.5 for pixel 2 (value=0.5): t=(0.5-0.0)/0.75 = 0.6667
        let expected_t_mid = palette::sample(PaletteId::TerrainHeight, 0.5 / 0.75);
        assert_eq!(&rgba[8..12], &expected_t_mid, "pixel 2 mismatch");
    }

    // 2. Binary-blue mask: 0 → transparent, 1 → RIVER colour.
    #[test]
    fn render_overlay_to_gpu_mask_binary_blue() {
        let mut world = test_world();

        let mut mask = MaskField2D::new(2, 1);
        mask.set(0, 0, 0); // should be transparent
        mask.set(1, 0, 1); // should be RIVER colour
        world.derived.river_mask = Some(mask);

        let desc = OverlayDescriptor {
            id: "test_river",
            label: "Test river",
            source: OverlaySource::Mask("river_mask"),
            palette: PaletteId::BinaryBlue,
            value_range: ValueRange::Fixed(0.0, 1.0),
            visible: true,
        };

        let (rgba, width, height) =
            render_overlay_to_gpu(&desc, &world).expect("river_mask is populated");

        assert_eq!(width, 2);
        assert_eq!(height, 1);
        assert_eq!(rgba.len(), 8);

        // Pixel 0 (value=0 → t=0.0 < 0.5): BinaryBlue gives alpha=0.
        assert_eq!(rgba[3], 0, "mask=0 pixel must be transparent");

        // Pixel 1 (value=1 → t=1.0 >= 0.5): BinaryBlue gives RIVER colour opaque.
        let river_px = palette::sample(PaletteId::BinaryBlue, 1.0);
        assert_eq!(
            &rgba[4..8],
            &river_px,
            "mask=1 pixel should be RIVER colour"
        );
        assert_eq!(rgba[7], 255, "mask=1 pixel must be opaque");
    }

    // 3. None when the field is not populated.
    #[test]
    fn render_overlay_to_gpu_none_if_field_missing() {
        let world = test_world(); // no pipeline run, all derived fields are None

        let desc = OverlayDescriptor {
            id: "test_missing",
            label: "Missing",
            source: OverlaySource::ScalarDerived("z_filled"),
            palette: PaletteId::TerrainHeight,
            value_range: ValueRange::Auto,
            visible: true,
        };

        assert!(
            render_overlay_to_gpu(&desc, &world).is_none(),
            "should return None when z_filled is not populated"
        );
    }

    // 4. Guard: no colour literals in overlay.wgsl.
    #[test]
    fn overlay_wgsl_has_no_literal_colors() {
        let src = include_str!("../../../shaders/overlay.wgsl");
        assert!(
            !src.contains('#'),
            "overlay.wgsl contains '#' — possible hex color literal"
        );
        assert!(
            !src.contains("vec3<f32>(0.") && !src.contains("vec3<f32>(1."),
            "overlay.wgsl contains vec3 color literal"
        );
        assert!(
            !src.contains("vec4<f32>(0.") && !src.contains("vec4<f32>(1."),
            "overlay.wgsl contains vec4 color literal"
        );
    }

    // 5. Guard: overlay.wgsl parses and validates cleanly via naga.
    #[test]
    fn overlay_wgsl_parses_successfully() {
        let src = include_str!("../../../shaders/overlay.wgsl");
        use naga::front::wgsl;
        use naga::valid::{Capabilities, ValidationFlags, Validator};
        let module = wgsl::parse_str(src).expect("WGSL parse failed");
        let mut validator = Validator::new(ValidationFlags::all(), Capabilities::all());
        validator.validate(&module).expect("WGSL validation failed");
    }
}
