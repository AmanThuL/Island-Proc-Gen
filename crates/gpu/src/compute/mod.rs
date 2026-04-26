//! GPU compute module — Sprint 4.D / 4.E.
//!
//! Provides:
//! - [`GpuBackend`]: implements [`island_core::pipeline::ComputeBackend`].
//!   `HillslopeDiffusion` is supported from Sprint 4.E onward.
//!   `StreamPowerIncision` remains `None` until Sprint 4.F.
//! - [`buffers`]: helpers to upload/readback `ScalarField2D<f32>`.
//! - [`timestamp`]: `ComputePassDescriptor::timestamp_writes`-based timer.
//! - [`hillslope_pipeline`]: fully implemented at Sprint 4.E.
//! - [`stream_power_pipeline`]: stub for Sprint 4.F.
//!
//! # Interior-mutability pattern
//!
//! `HillslopeComputePipeline::dispatch` takes `&mut self` because it may
//! reallocate internal ping-pong buffers when the grid dimensions change.
//! The `ComputeBackend` trait takes `&self` so the pipeline is wrapped in
//! `std::sync::Mutex<Option<HillslopeComputePipeline>>`. The mutex is
//! `lock()`-ed on every dispatch; contention is impossible in practice because
//! `ErosionOuterLoop` is single-threaded.
//!
//! # Crate-DAG contract
//!
//! `gpu → core` already exists. No new edges are added. `GpuBackend` lives
//! here in `crates/gpu/`; the `ComputeBackend` trait lives in `crates/core/`.

pub mod buffers;
pub mod hillslope_pipeline;
pub mod stream_power_pipeline;
pub mod timestamp;

use std::sync::{Arc, Mutex};
use std::time::Instant;

use island_core::pipeline::{
    ComputeBackend, ComputeBackendError, ComputeOp, HillslopeParams, StageTiming, StreamPowerParams,
};
use island_core::world::WorldState;

use crate::GpuContext;
use hillslope_pipeline::HillslopeComputePipeline;
use stream_power_pipeline::StreamPowerComputePipeline;

// ─── GpuBackend ──────────────────────────────────────────────────────────────

/// GPU compute backend — Sprint 4.D / 4.E / 4.F.
///
/// From Sprint 4.E, `HillslopeDiffusion` is fully supported.
/// From Sprint 4.F, `StreamPowerIncision` is also fully supported.
///
/// The struct is `Send + Sync` because [`GpuContext`] holds only `wgpu`
/// handles that are `Send + Sync` on native backends. Interior mutability for
/// buffer reallocation is handled via `Mutex`.
pub struct GpuBackend {
    /// Shared wgpu context. `Arc` allows the backend to be wrapped in
    /// `Arc<dyn ComputeBackend>` for use with `ErosionOuterLoop`.
    #[allow(dead_code)] // Used indirectly via Arc<GpuContext> inside the pipelines.
    ctx: Arc<GpuContext>,
    /// Hillslope diffusion pipeline. `Some` from Sprint 4.E.
    ///
    /// Wrapped in `Mutex` because `dispatch` requires `&mut HillslopeComputePipeline`
    /// (may reallocate ping-pong buffers) while `ComputeBackend::run_hillslope_diffusion`
    /// takes `&self`.
    hillslope: Mutex<Option<HillslopeComputePipeline>>,
    /// Stream power incision pipeline. `Some` from Sprint 4.F.
    ///
    /// Same `Mutex` pattern as `hillslope`: `dispatch` takes `&mut`, the
    /// trait method takes `&self`.
    stream_power: Mutex<Option<StreamPowerComputePipeline>>,
}

impl GpuBackend {
    /// Construct a `GpuBackend` from an existing headless [`GpuContext`].
    ///
    /// From Sprint 4.E, `HillslopeComputePipeline` is constructed here.
    /// `StreamPowerComputePipeline` is left `None` until Sprint 4.F.
    pub fn new(ctx: Arc<GpuContext>) -> Self {
        let hillslope = HillslopeComputePipeline::new(Arc::clone(&ctx));
        let stream_power = StreamPowerComputePipeline::new(Arc::clone(&ctx));
        Self {
            ctx,
            hillslope: Mutex::new(Some(hillslope)),
            stream_power: Mutex::new(Some(stream_power)),
        }
    }
}

// SAFETY: wgpu native handles are Send + Sync.
// Arc<GpuContext> is Send + Sync because GpuContext's fields are Send + Sync.
// Mutex<Option<HillslopeComputePipeline>> is Send + Sync if HillslopeComputePipeline
// is Send. wgpu resources (Buffer, ComputePipeline, BindGroupLayout) are Send.
unsafe impl Send for GpuBackend {}
unsafe impl Sync for GpuBackend {}

impl ComputeBackend for GpuBackend {
    fn name(&self) -> &'static str {
        "gpu"
    }

    fn supports(&self, op: ComputeOp) -> bool {
        match op {
            ComputeOp::HillslopeDiffusion => {
                self.hillslope.lock().map(|g| g.is_some()).unwrap_or(false)
            }
            ComputeOp::StreamPowerIncision => self
                .stream_power
                .lock()
                .map(|g| g.is_some())
                .unwrap_or(false),
        }
    }

    fn run_hillslope_diffusion(
        &self,
        world: &mut WorldState,
        params: &HillslopeParams,
    ) -> Result<StageTiming, ComputeBackendError> {
        let cpu_start = Instant::now();

        let mut guard = self
            .hillslope
            .lock()
            .map_err(|_| ComputeBackendError::Other("hillslope Mutex poisoned".into()))?;

        let pipeline = guard.as_mut().ok_or(ComputeBackendError::Unsupported {
            backend: "gpu",
            op: "hillslope_diffusion",
        })?;

        let gpu_ms = pipeline.dispatch(world, params)?;
        let cpu_ms = cpu_start.elapsed().as_secs_f64() * 1000.0;

        Ok(StageTiming { cpu_ms, gpu_ms })
    }

    fn run_stream_power_incision(
        &self,
        world: &mut WorldState,
        params: &StreamPowerParams,
    ) -> Result<StageTiming, ComputeBackendError> {
        let cpu_start = Instant::now();

        let mut guard = self
            .stream_power
            .lock()
            .map_err(|_| ComputeBackendError::Other("stream_power Mutex poisoned".into()))?;

        let pipeline = guard.as_mut().ok_or(ComputeBackendError::Unsupported {
            backend: "gpu",
            op: "stream_power_incision",
        })?;

        let gpu_ms = pipeline.dispatch(world, params)?;
        let cpu_ms = cpu_start.elapsed().as_secs_f64() * 1000.0;

        Ok(StageTiming { cpu_ms, gpu_ms })
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use island_core::pipeline::ComputeOp;

    /// Sprint 4.D smoke test: `GpuBackend` can be constructed from a headless
    /// `GpuContext`.
    ///
    /// Marked #[ignore] — requires a real GPU adapter.
    #[test]
    #[ignore = "requires a working GPU adapter; opt in with IPG_RUN_GPU_TESTS=1"]
    fn gpu_backend_can_be_constructed_from_headless_context() {
        if std::env::var("IPG_RUN_GPU_TESTS").as_deref() != Ok("1") {
            eprintln!("skipped — set IPG_RUN_GPU_TESTS=1 to run GPU tests");
            return;
        }
        let ctx =
            Arc::new(crate::GpuContext::new_headless((64, 64)).expect("headless context required"));
        let backend = GpuBackend::new(ctx);
        assert_eq!(backend.name(), "gpu");
    }

    /// Sprint 4.F contract: both `supports(HillslopeDiffusion)` and
    /// `supports(StreamPowerIncision)` return `true` after 4.F lands.
    ///
    /// Marked #[ignore] — requires a real GPU adapter.
    #[test]
    #[ignore = "requires a working GPU adapter; opt in with IPG_RUN_GPU_TESTS=1"]
    fn gpu_backend_supports_both_ops_at_4_f() {
        if std::env::var("IPG_RUN_GPU_TESTS").as_deref() != Ok("1") {
            eprintln!("skipped — set IPG_RUN_GPU_TESTS=1 to run GPU tests");
            return;
        }
        let ctx =
            Arc::new(crate::GpuContext::new_headless((64, 64)).expect("headless context required"));
        let backend = GpuBackend::new(ctx);
        assert!(
            backend.supports(ComputeOp::HillslopeDiffusion),
            "HillslopeDiffusion must be supported at 4.F"
        );
        assert!(
            backend.supports(ComputeOp::StreamPowerIncision),
            "StreamPowerIncision must be supported at 4.F"
        );
    }
}
