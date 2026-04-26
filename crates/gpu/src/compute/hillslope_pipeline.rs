//! Hillslope diffusion GPU compute pipeline — Sprint 4.E.
//!
//! This module is a **stub** at Sprint 4.D. `HillslopeComputePipeline` is
//! declared but never constructed; `GpuBackend::hillslope` stays `None` and
//! `GpuBackend::run_hillslope_diffusion` returns
//! [`island_core::pipeline::ComputeBackendError::Unsupported`].
//!
//! Sprint 4.E will:
//! - Write `crates/gpu/src/shaders/hillslope_diffusion.wgsl`
//! - Populate `HillslopeComputePipeline` with bind-group layouts, pipeline,
//!   and the dispatch logic.
//! - Construct `Some(HillslopeComputePipeline)` inside `GpuBackend::new`.

/// Hillslope diffusion compute pipeline.
///
/// Empty at Sprint 4.D; populated by Sprint 4.E.
pub struct HillslopeComputePipeline {
    // Sprint 4.E populates:
    //   pipeline: wgpu::ComputePipeline,
    //   bind_group_layout: wgpu::BindGroupLayout,
}
