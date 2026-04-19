//! Offscreen viewport render target.
//!
//! Holds a colour + depth texture pair sized to the egui `Viewport` tab's
//! `available_size()` (in B.1: filled to window; in B.2 onwards: the dock tab
//! rect). The 3D render pass (sky + terrain + overlay) targets these
//! textures instead of the window surface, and egui displays the colour
//! texture via `egui::Image` — see `crates/app/src/runtime.rs` for the
//! integration.
//!
//! Format rule: colour matches `gpu.surface_format` (Metal `Bgra8Unorm`,
//! headless `Rgba8Unorm`) so the same `TerrainRenderer` pipeline works on
//! both paths. Depth matches `gpu.depth_format`.

use gpu::GpuContext;

/// A colour + depth texture pair that serves as the offscreen render target
/// for the 3D scene.
///
/// The colour texture is registered with `egui_wgpu::Renderer` as a
/// `TextureId` so it can be displayed inside an `egui::Image` widget. The
/// same `TextureId` is reused across resizes — only the underlying texture
/// and view are reallocated.
pub struct ViewportTextureSet {
    color: wgpu::Texture,
    color_view: wgpu::TextureView,
    depth: wgpu::Texture,
    depth_view: wgpu::TextureView,
    size: (u32, u32),
    egui_texture_id: egui::TextureId,
}

impl ViewportTextureSet {
    /// Create a viewport texture pair and register the colour view with the
    /// given egui renderer.
    ///
    /// `size` is clamped to at least `(1, 1)` — wgpu rejects zero-dimension
    /// textures, and egui can ask for a tiny size on the first frame.
    ///
    /// The returned [`egui::TextureId`] is stable across resizes; pass it to
    /// `egui::load::SizedTexture::new(id, size)` inside a `CentralPanel`.
    pub fn new(
        gpu: &GpuContext,
        size: (u32, u32),
        egui_renderer: &mut egui_wgpu::Renderer,
    ) -> Self {
        let size = (size.0.max(1), size.1.max(1));
        let (color, color_view) = create_viewport_texture(
            &gpu.device,
            size,
            "viewport_color",
            gpu.surface_format,
            COLOR_USAGE,
        );
        let (depth, depth_view) = create_viewport_texture(
            &gpu.device,
            size,
            "viewport_depth",
            gpu.depth_format,
            DEPTH_USAGE,
        );

        let egui_texture_id = egui_renderer.register_native_texture(
            &gpu.device,
            &color_view,
            wgpu::FilterMode::Linear,
        );

        Self {
            color,
            color_view,
            depth,
            depth_view,
            size,
            egui_texture_id,
        }
    }

    /// Resize to a new `(width, height)`.
    ///
    /// Returns `true` when the textures were reallocated (i.e. the size
    /// changed), `false` when the size was already correct (no-op).
    ///
    /// The [`egui::TextureId`] is preserved across resizes; the egui renderer
    /// registration is updated to point at the new colour view via
    /// `update_egui_texture_from_wgpu_texture`.
    pub fn resize(
        &mut self,
        gpu: &GpuContext,
        new_size: (u32, u32),
        egui_renderer: &mut egui_wgpu::Renderer,
    ) -> bool {
        let new_size = (new_size.0.max(1), new_size.1.max(1));
        if self.size == new_size {
            return false;
        }

        let (color, color_view) = create_viewport_texture(
            &gpu.device,
            new_size,
            "viewport_color",
            gpu.surface_format,
            COLOR_USAGE,
        );
        let (depth, depth_view) = create_viewport_texture(
            &gpu.device,
            new_size,
            "viewport_depth",
            gpu.depth_format,
            DEPTH_USAGE,
        );

        // Update the egui registration to point at the new view.
        // The TextureId is unchanged — existing Image calls continue to work.
        egui_renderer.update_egui_texture_from_wgpu_texture(
            &gpu.device,
            &color_view,
            wgpu::FilterMode::Linear,
            self.egui_texture_id,
        );

        self.color = color;
        self.color_view = color_view;
        self.depth = depth;
        self.depth_view = depth_view;
        self.size = new_size;

        true
    }

    /// Borrow the colour texture view for use as the render pass colour
    /// attachment.
    pub fn color_view(&self) -> &wgpu::TextureView {
        &self.color_view
    }

    /// Borrow the depth texture view for use as the render pass depth
    /// attachment.
    pub fn depth_view(&self) -> &wgpu::TextureView {
        &self.depth_view
    }

    /// The stable `TextureId` registered with egui. Pass to
    /// `egui::load::SizedTexture::new(id, size)` inside a `CentralPanel`.
    pub fn egui_texture_id(&self) -> egui::TextureId {
        self.egui_texture_id
    }

    /// Current texture dimensions as `(width, height)`.
    pub fn size(&self) -> (u32, u32) {
        self.size
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn create_viewport_texture(
    device: &wgpu::Device,
    size: (u32, u32),
    label: &'static str,
    format: wgpu::TextureFormat,
    usage: wgpu::TextureUsages,
) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: size.0,
            height: size.1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

// RENDER_ATTACHMENT: the terrain pass writes into it.
// TEXTURE_BINDING: egui samples it via register_native_texture.
const COLOR_USAGE: wgpu::TextureUsages =
    wgpu::TextureUsages::RENDER_ATTACHMENT.union(wgpu::TextureUsages::TEXTURE_BINDING);

// RENDER_ATTACHMENT only — depth is never sampled by egui.
const DEPTH_USAGE: wgpu::TextureUsages = wgpu::TextureUsages::RENDER_ATTACHMENT;

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires a working GPU adapter; baseline host = macOS Metal (AD10)"]
    fn new_creates_viewport_matching_requested_size() {
        let gpu = gpu::GpuContext::new_headless((800, 600))
            .expect("GpuContext::new_headless should succeed on baseline host");
        let mut egui_renderer = egui_wgpu::Renderer::new(
            &gpu.device,
            gpu.surface_format,
            egui_wgpu::RendererOptions::default(),
        );
        let vtex = ViewportTextureSet::new(&gpu, (800, 600), &mut egui_renderer);
        assert_eq!(vtex.size(), (800, 600));
    }

    #[test]
    #[ignore = "requires a working GPU adapter; baseline host = macOS Metal (AD10)"]
    fn resize_reallocates_on_size_change_noop_on_same() {
        let gpu = gpu::GpuContext::new_headless((800, 600))
            .expect("GpuContext::new_headless should succeed on baseline host");
        let mut egui_renderer = egui_wgpu::Renderer::new(
            &gpu.device,
            gpu.surface_format,
            egui_wgpu::RendererOptions::default(),
        );
        let mut vtex = ViewportTextureSet::new(&gpu, (800, 600), &mut egui_renderer);

        // same size → noop
        let changed = vtex.resize(&gpu, (800, 600), &mut egui_renderer);
        assert!(!changed, "resize to same size should be noop");
        assert_eq!(vtex.size(), (800, 600));

        // different size → reallocate
        let changed = vtex.resize(&gpu, (1024, 768), &mut egui_renderer);
        assert!(changed, "resize to different size should reallocate");
        assert_eq!(vtex.size(), (1024, 768));
    }
}
