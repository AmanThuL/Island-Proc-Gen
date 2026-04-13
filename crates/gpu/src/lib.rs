//! GPU context — wgpu Instance / Surface / Adapter / Device / Queue.
//!
//! This crate owns the low-level wgpu handle set. No compute, no headless path
//! in Sprint 0 — only the windowed surface path needed for the main app.

use std::sync::Arc;

use anyhow::Context as _;
use tracing::info;
use winit::{dpi::PhysicalSize, window::Window};

/// Everything wgpu needs to render into a window surface.
pub struct GpuContext {
    pub instance: wgpu::Instance,
    pub surface: wgpu::Surface<'static>,
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub config: wgpu::SurfaceConfiguration,
    pub size: PhysicalSize<u32>,
    /// The surface texture format chosen from adapter capabilities.
    pub surface_format: wgpu::TextureFormat,
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

        // ── Adapter ──────────────────────────────────────────────────────────
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))
        .context("No suitable GPU adapter found")?;

        info!(
            adapter = %adapter.get_info().name,
            backend = ?adapter.get_info().backend,
            "GPU adapter selected"
        );

        // ── Device + Queue ───────────────────────────────────────────────────
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("island-proc-gen device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            ..Default::default()
        }))
        .context("request_device")?;

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

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        info!(
            width = config.width,
            height = config.height,
            format = ?surface_format,
            "Surface configured"
        );

        Ok(Self {
            instance,
            surface,
            adapter,
            device,
            queue,
            config,
            size,
            surface_format,
        })
    }

    /// Called when the window is resized.
    pub fn resize(&mut self, new_size: PhysicalSize<u32>) {
        if new_size.width == 0 || new_size.height == 0 {
            return;
        }
        self.size = new_size;
        self.config.width = new_size.width;
        self.config.height = new_size.height;
        self.surface.configure(&self.device, &self.config);
    }
}
