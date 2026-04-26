//! Stream power incision GPU compute pipeline — Sprint 4.F.
//!
//! This module is a **stub** at Sprint 4.D. `StreamPowerComputePipeline` is
//! declared but never constructed; `GpuBackend::stream_power` stays `None`
//! and `GpuBackend::run_stream_power_incision` returns
//! [`island_core::pipeline::ComputeBackendError::Unsupported`].
//!
//! Sprint 4.F will:
//! - Write `crates/gpu/src/shaders/stream_power_incision.wgsl`
//! - Populate `StreamPowerComputePipeline` with bind-group layouts, pipeline,
//!   and the dispatch logic.
//! - Construct `Some(StreamPowerComputePipeline)` inside `GpuBackend::new`.

/// Stream power incision compute pipeline.
///
/// Empty at Sprint 4.D; populated by Sprint 4.F.
pub struct StreamPowerComputePipeline {
    // Sprint 4.F populates:
    //   pipeline: wgpu::ComputePipeline,
    //   bind_group_layout: wgpu::BindGroupLayout,
}
