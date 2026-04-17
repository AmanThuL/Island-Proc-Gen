//! `GpuContext` — wgpu Instance / Surface / Adapter / Device / Queue.
//!
//! Owns the low-level wgpu handle set used by both the interactive windowed
//! path ([`GpuContext::new`]) and the Sprint 1C headless offscreen path
//! ([`GpuContext::new_headless`]).
//!
//! Invariants:
//!
//! - `surface` is `Some(..)` iff the context was built from a window via
//!   [`GpuContext::new`]. The headless constructor sets it to `None`.
//! - Interactive callers (`app::Runtime`) reach the surface through
//!   [`GpuContext::surface_expect`], which panics with a descriptive message
//!   rather than silently unwrapping — the windowed path always has a
//!   surface by construction.
//! - Both constructors go through [`request_adapter_and_device`] so the
//!   adapter / device selection logic is not forked (AD6, sprint 1C).
//!
//! See `docs/design/sprints/sprint_1c_headless_validation.md` §AD6 / AD8 /
//! AD10 for the split-path rationale.

use std::sync::Arc;

use anyhow::Context as _;
use tracing::info;
use winit::{dpi::PhysicalSize, window::Window};

/// Depth buffer format used for the main render pass and every offscreen
/// capture. Kept as a single constant so windowed / offscreen render
/// pipelines always agree on the depth attachment.
pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// Texture format used by the Sprint 1C offscreen beauty path.
///
/// Fixed to `Rgba8Unorm` rather than sampled from adapter capabilities: the
/// offscreen path does not involve egui (egui does its own gamma handling on
/// the windowed surface, which is why [`GpuContext::new`] picks a non-sRGB
/// surface format), so the simplest, most portable RGBA8 format is fine.
pub const HEADLESS_COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// Everything wgpu needs to render, either into a window surface or into an
/// offscreen texture.
pub struct GpuContext {
    pub instance: wgpu::Instance,
    /// The window surface. `Some` in the interactive path, `None` in the
    /// headless offscreen path (Sprint 1C).
    pub surface: Option<wgpu::Surface<'static>>,
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    /// Surface configuration. Windowed: reflects current swapchain config.
    /// Headless: reflects the fixed offscreen size + [`HEADLESS_COLOR_FORMAT`].
    pub config: wgpu::SurfaceConfiguration,
    pub size: PhysicalSize<u32>,
    /// Colour format used by render pipelines. Equals the swapchain format on
    /// the windowed path; equals [`HEADLESS_COLOR_FORMAT`] on the headless
    /// path.
    pub surface_format: wgpu::TextureFormat,
    /// Depth buffer texture (`Depth32Float`).
    pub depth_texture: wgpu::Texture,
    /// View into the depth texture for use as a depth attachment.
    pub depth_view: wgpu::TextureView,
    /// Format of the depth texture (always [`DEPTH_FORMAT`]).
    pub depth_format: wgpu::TextureFormat,
}

impl GpuContext {
    /// Construct a fully-initialised GPU context for the given window.
    ///
    /// Blocks the current thread on async adapter / device requests via
    /// `pollster`.
    pub fn new(window: Arc<Window>) -> anyhow::Result<Self> {
        let size = window.inner_size();

        // ── Instance ─────────────────────────────────────────────────────────
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());

        // ── Surface ──────────────────────────────────────────────────────────
        // The window Arc is cloned in so the surface can't outlive it.
        let surface = instance
            .create_surface(window.clone())
            .context("create_surface")?;

        // ── Adapter + Device ────────────────────────────────────────────────
        let (adapter, device, queue) =
            request_adapter_and_device(&instance, Some(&surface), "interactive")?;

        // ── Surface configuration ────────────────────────────────────────────
        let caps = surface.get_capabilities(&adapter);

        // Prefer a non-sRGB format for egui compatibility (egui does its own
        // gamma correction). Fall back to whatever is available.
        let surface_format = caps
            .formats
            .iter()
            .copied()
            .find(|f| !f.is_srgb())
            .unwrap_or(caps.formats[0]);

        let config = make_surface_config(size.width.max(1), size.height.max(1), surface_format);
        surface.configure(&device, &config);

        // ── Depth texture ────────────────────────────────────────────────────
        let (depth_texture, depth_view) =
            create_depth(&device, config.width, config.height, DEPTH_FORMAT);

        info!(
            width = config.width,
            height = config.height,
            format = ?surface_format,
            "Surface configured"
        );

        Ok(Self {
            instance,
            surface: Some(surface),
            adapter,
            device,
            queue,
            config,
            size,
            surface_format,
            depth_texture,
            depth_view,
            depth_format: DEPTH_FORMAT,
        })
    }

    /// Construct a surface-less GPU context for offscreen rendering.
    ///
    /// Used by the Sprint 1C headless capture path (AD2 beauty, AD6, AD8).
    /// Returns `Err` when no suitable adapter is available (e.g. on a CI
    /// runner without a working GPU backend) — callers are expected to
    /// translate that into `BeautyStatus::Skipped` per AD8 rather than
    /// failing the whole run.
    ///
    /// `size` is the `(width, height)` of the offscreen render target the
    /// caller plans to use. It is recorded on `self.size` / `self.config`
    /// so downstream renderers can read it, and it drives the depth
    /// texture dimensions. The actual colour / depth textures for each
    /// capture are created transiently inside
    /// [`GpuContext::capture_offscreen_rgba8`] — this constructor only
    /// sets up the persistent depth buffer matching `size`.
    pub fn new_headless(size: (u32, u32)) -> anyhow::Result<Self> {
        let (width, height) = (size.0.max(1), size.1.max(1));

        // ── Instance ─────────────────────────────────────────────────────────
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());

        // ── Adapter + Device (no compatible_surface) ────────────────────────
        let (adapter, device, queue) = request_adapter_and_device(&instance, None, "headless")?;

        let surface_format = HEADLESS_COLOR_FORMAT;

        // Fabricate a SurfaceConfiguration purely so downstream code that
        // reads `gpu.config.width / height / format` keeps working. It is
        // never handed to an actual `Surface::configure(..)` call because
        // `self.surface` is `None`.
        let config = make_surface_config(width, height, surface_format);

        let (depth_texture, depth_view) = create_depth(&device, width, height, DEPTH_FORMAT);

        info!(
            width = width,
            height = height,
            format = ?surface_format,
            "Headless GPU context initialised"
        );

        Ok(Self {
            instance,
            surface: None,
            adapter,
            device,
            queue,
            config,
            size: PhysicalSize::new(width, height),
            surface_format,
            depth_texture,
            depth_view,
            depth_format: DEPTH_FORMAT,
        })
    }

    /// Borrow the window surface, panicking with a descriptive message when
    /// none is attached.
    ///
    /// Used by the interactive [`crate::GpuContext::new`] path, where the
    /// surface is always present by construction. The headless offscreen
    /// path must never call this — it goes through
    /// [`GpuContext::capture_offscreen_rgba8`] instead.
    pub fn surface_expect(&self) -> &wgpu::Surface<'static> {
        self.surface.as_ref().expect(
            "GpuContext::surface_expect called on a surface-less (headless) \
             context — interactive window path assumed but not initialised",
        )
    }

    /// Called when the window is resized. No-op in the headless path
    /// (guarded by `self.surface.is_some()`).
    pub fn resize(&mut self, new_size: PhysicalSize<u32>) {
        if new_size.width == 0 || new_size.height == 0 {
            return;
        }
        self.size = new_size;
        self.config.width = new_size.width;
        self.config.height = new_size.height;
        if let Some(surface) = self.surface.as_ref() {
            surface.configure(&self.device, &self.config);
        }
        // Recreate depth texture at the new resolution regardless of path —
        // the headless path does resize the depth buffer if the caller ever
        // mutates it (currently unused, but harmless and consistent).
        let (tex, view) = create_depth(
            &self.device,
            self.config.width,
            self.config.height,
            DEPTH_FORMAT,
        );
        self.depth_texture = tex;
        self.depth_view = view;
    }

    /// Render one RGBA8 frame into a transient offscreen texture and return
    /// the mapped bytes with row padding stripped.
    ///
    /// `render_cb` is handed a freshly-created `(color_view, depth_view)`
    /// pair plus the encoder that will be submitted after the closure
    /// returns. It is expected to issue one or more render passes that
    /// target the views.
    ///
    /// Contract:
    ///
    /// - Colour texture format is [`HEADLESS_COLOR_FORMAT`] (`Rgba8Unorm`)
    ///   with `RENDER_ATTACHMENT | COPY_SRC` usage.
    /// - Depth texture format is [`DEPTH_FORMAT`] (`Depth32Float`) with
    ///   `RENDER_ATTACHMENT` usage.
    /// - The returned `Vec<u8>` has exactly `size.0 * size.1 * 4` bytes —
    ///   row padding (wgpu requires `COPY_BYTES_PER_ROW_ALIGNMENT` alignment
    ///   on `bytes_per_row` in `copy_texture_to_buffer`) is stripped before
    ///   returning.
    /// - Any wgpu error during map / submit is bubbled up as `anyhow::Error`;
    ///   the caller never sees raw `wgpu::Error`.
    pub fn capture_offscreen_rgba8(
        &self,
        size: (u32, u32),
        render_cb: impl FnOnce(&wgpu::TextureView, &wgpu::TextureView, &mut wgpu::CommandEncoder),
    ) -> anyhow::Result<Vec<u8>> {
        let (width, height) = size;
        anyhow::ensure!(
            width > 0 && height > 0,
            "capture_offscreen_rgba8 requires non-zero width and height, got ({width}, {height})"
        );

        // ── Transient colour + depth attachments ─────────────────────────────
        let color_texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("headless_color"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: HEADLESS_COLOR_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let color_view = color_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let (_depth_texture, depth_view) = create_depth(&self.device, width, height, DEPTH_FORMAT);

        // ── Encode caller render passes + a single T->B copy ────────────────
        let row_pitch = align_to(
            width
                .checked_mul(4)
                .context("width * 4 overflow in capture_offscreen_rgba8")?,
            wgpu::COPY_BYTES_PER_ROW_ALIGNMENT,
        );
        let readback_size = (row_pitch as u64)
            .checked_mul(height as u64)
            .context("row_pitch * height overflow in capture_offscreen_rgba8")?;

        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("headless_readback"),
            size: readback_size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("headless_capture_encoder"),
            });

        render_cb(&color_view, &depth_view, &mut encoder);

        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &color_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row_pitch),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );

        self.queue.submit(Some(encoder.finish()));

        // ── Map + strip row padding ─────────────────────────────────────────
        let slice = readback.slice(..);
        let (sender, receiver) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            // Safe to ignore send failure: the receiving thread is this one
            // below, and we always drop the receiver on an early return after
            // the poll completes.
            let _ = sender.send(result);
        });
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .context("device.poll during capture_offscreen_rgba8")?;
        receiver
            .recv()
            .context("map_async sender dropped without response")?
            .context("map_async failed in capture_offscreen_rgba8")?;

        let mapped = slice.get_mapped_range();
        let bytes_per_row_tight = (width as usize) * 4;
        let mut out = Vec::with_capacity(bytes_per_row_tight * height as usize);
        for row in 0..height as usize {
            let start = row * row_pitch as usize;
            out.extend_from_slice(&mapped[start..start + bytes_per_row_tight]);
        }
        drop(mapped);
        readback.unmap();

        Ok(out)
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Build a `SurfaceConfiguration` with the fixed defaults shared by the
/// windowed and headless constructors.
///
/// On the headless path the returned value is stored on `GpuContext` purely
/// as a carrier for `width / height / format`; the surface-specific fields
/// (`present_mode`, `desired_maximum_frame_latency`, `alpha_mode`) are
/// inert because `Surface::configure` is never called (`surface == None`).
fn make_surface_config(
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
) -> wgpu::SurfaceConfiguration {
    wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format,
        width,
        height,
        present_mode: wgpu::PresentMode::AutoVsync,
        alpha_mode: wgpu::CompositeAlphaMode::Auto,
        view_formats: vec![],
        desired_maximum_frame_latency: 2,
    }
}

/// Request an adapter + device pair, shared by [`GpuContext::new`] and
/// [`GpuContext::new_headless`] (AD6: shared adapter selection logic).
///
/// `compatible_surface = None` on the headless path — wgpu 29 supports
/// adapter requests without a surface on native backends (Metal / Vulkan /
/// DX12). `label` is purely for the tracing span so we can tell the two
/// paths apart in logs.
pub(crate) fn request_adapter_and_device(
    instance: &wgpu::Instance,
    compatible_surface: Option<&wgpu::Surface<'_>>,
    label: &'static str,
) -> anyhow::Result<(wgpu::Adapter, wgpu::Device, wgpu::Queue)> {
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface,
        force_fallback_adapter: false,
    }))
    .with_context(|| format!("No suitable GPU adapter found (path = {label})"))?;

    info!(
        path = label,
        adapter = %adapter.get_info().name,
        backend = ?adapter.get_info().backend,
        "GPU adapter selected"
    );

    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("island-proc-gen device"),
        required_features: wgpu::Features::empty(),
        required_limits: wgpu::Limits::default(),
        ..Default::default()
    }))
    .with_context(|| format!("request_device failed (path = {label})"))?;

    Ok((adapter, device, queue))
}

fn create_depth(
    device: &wgpu::Device,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("depth_texture"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

/// Round `value` up to the nearest multiple of `alignment`. `alignment` must
/// be non-zero.
#[inline]
fn align_to(value: u32, alignment: u32) -> u32 {
    debug_assert!(alignment > 0, "alignment must be > 0");
    value.div_ceil(alignment) * alignment
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn align_to_matches_manual_formula() {
        assert_eq!(align_to(0, 256), 0);
        assert_eq!(align_to(1, 256), 256);
        assert_eq!(align_to(255, 256), 256);
        assert_eq!(align_to(256, 256), 256);
        assert_eq!(align_to(257, 256), 512);
        // wgpu::COPY_BYTES_PER_ROW_ALIGNMENT is the real-world usage
        assert_eq!(
            align_to(4 * 4, wgpu::COPY_BYTES_PER_ROW_ALIGNMENT),
            wgpu::COPY_BYTES_PER_ROW_ALIGNMENT
        );
        assert_eq!(
            align_to(256 * 4, wgpu::COPY_BYTES_PER_ROW_ALIGNMENT),
            256 * 4
        );
    }

    /// Headless adapter + device bootstrap.
    ///
    /// Marked `#[ignore]` because CI runners without a GPU (e.g. a bare
    /// Linux container) have no adapter, and `new_headless` will return
    /// `Err`. Baseline acceptance host per AD10 is this macOS Metal dev
    /// machine — run with `cargo test -p gpu -- --ignored` to exercise it.
    #[test]
    #[ignore = "requires a working GPU adapter; baseline host = macOS Metal (AD10)"]
    fn new_headless_builds_without_surface() {
        let ctx = GpuContext::new_headless((256, 256))
            .expect("new_headless should succeed on baseline host");
        assert!(
            ctx.surface.is_none(),
            "headless context must have no surface"
        );
        assert_eq!(ctx.surface_format, HEADLESS_COLOR_FORMAT);
        assert_eq!(ctx.depth_format, DEPTH_FORMAT);
        assert_eq!(ctx.size.width, 256);
        assert_eq!(ctx.size.height, 256);
        assert!(
            ctx.device.limits().max_texture_dimension_2d >= 256,
            "device must support at least 256x256 textures"
        );
    }

    /// End-to-end offscreen capture round-trip: clear to red, copy to
    /// readback buffer, verify length + first pixel.
    ///
    /// Marked `#[ignore]` for the same reason as
    /// [`new_headless_builds_without_surface`].
    #[test]
    #[ignore = "requires a working GPU adapter; baseline host = macOS Metal (AD10)"]
    fn capture_offscreen_rgba8_produces_correct_byte_count() {
        let ctx =
            GpuContext::new_headless((4, 4)).expect("new_headless should succeed on baseline host");

        let bytes = ctx
            .capture_offscreen_rgba8((4, 4), |color_view, _depth_view, enc| {
                let _rpass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("headless_clear_red"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: color_view,
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: 1.0,
                                g: 0.0,
                                b: 0.0,
                                a: 1.0,
                            }),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                // Drop the render pass to end it before the encoder is used
                // for the copy step by the surrounding helper.
            })
            .expect("capture_offscreen_rgba8 should succeed on baseline host");

        assert_eq!(
            bytes.len(),
            4 * 4 * 4,
            "expected 64 bytes for 4x4 RGBA8, got {}",
            bytes.len()
        );
        assert_eq!(
            &bytes[..4],
            &[255, 0, 0, 255],
            "first pixel should be opaque red after Clear(1.0, 0.0, 0.0, 1.0) → Rgba8Unorm"
        );
    }
}
