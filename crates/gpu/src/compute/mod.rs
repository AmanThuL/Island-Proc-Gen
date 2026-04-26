//! GPU compute module — Sprint 4.D scaffold.
//!
//! Provides:
//! - [`GpuBackend`]: implements [`island_core::pipeline::ComputeBackend`].
//!   Both pilot pipeline slots are `None` at 4.D — 4.E / 4.F will populate
//!   them. Every dispatch currently returns
//!   [`island_core::pipeline::ComputeBackendError::Unsupported`].
//! - [`buffers`]: helpers to upload/readback `ScalarField2D<f32>`.
//! - [`timestamp`]: `ComputePassDescriptor::timestamp_writes`-based timer.
//! - [`hillslope_pipeline`]: stub for Sprint 4.E.
//! - [`stream_power_pipeline`]: stub for Sprint 4.F.
//!
//! # Crate-DAG contract
//!
//! `gpu → core` already exists. No new edges are added. `GpuBackend` lives
//! here in `crates/gpu/`; the `ComputeBackend` trait lives in `crates/core/`.

pub mod buffers;
pub mod hillslope_pipeline;
pub mod stream_power_pipeline;
pub mod timestamp;

use std::sync::Arc;

use island_core::pipeline::{
    ComputeBackend, ComputeBackendError, ComputeOp, HillslopeParams, StageTiming, StreamPowerParams,
};
use island_core::world::WorldState;

use crate::GpuContext;
use hillslope_pipeline::HillslopeComputePipeline;
use stream_power_pipeline::StreamPowerComputePipeline;

// ─── GpuBackend ──────────────────────────────────────────────────────────────

/// GPU compute backend — Sprint 4.D skeleton.
///
/// Both pilot pipeline slots (`hillslope`, `stream_power`) are `None` at
/// Sprint 4.D. `supports()` returns `false` for both operations and every
/// `run_*` call returns [`ComputeBackendError::Unsupported`] with a clear
/// message directing the reader to Sprint 4.E / 4.F.
///
/// The struct is `Send + Sync` because [`GpuContext`] holds only `wgpu`
/// handles that are `Send + Sync` on native backends.
pub struct GpuBackend {
    /// Shared wgpu context. `Arc` allows the backend to be wrapped in
    /// `Arc<dyn ComputeBackend>` for use with `ErosionOuterLoop`.
    #[allow(dead_code)] // Used by 4.E / 4.F pilots.
    ctx: Arc<GpuContext>,
    /// Hillslope diffusion pipeline. `None` at 4.D; `Some` at 4.E.
    hillslope: Option<HillslopeComputePipeline>,
    /// Stream power incision pipeline. `None` at 4.D; `Some` at 4.F.
    stream_power: Option<StreamPowerComputePipeline>,
}

impl GpuBackend {
    /// Construct a `GpuBackend` from an existing headless [`GpuContext`].
    ///
    /// Both pilot pipeline slots are left `None` at Sprint 4.D. Sprint 4.E
    /// and 4.F will construct and assign the real pipelines here.
    pub fn new(ctx: Arc<GpuContext>) -> Self {
        Self {
            ctx,
            hillslope: None,    // Sprint 4.E
            stream_power: None, // Sprint 4.F
        }
    }
}

// SAFETY: wgpu native handles are Send + Sync.
// Arc<GpuContext> is Send + Sync because GpuContext's fields are Send + Sync.
unsafe impl Send for GpuBackend {}
unsafe impl Sync for GpuBackend {}

impl ComputeBackend for GpuBackend {
    fn name(&self) -> &'static str {
        "gpu"
    }

    fn supports(&self, op: ComputeOp) -> bool {
        match op {
            ComputeOp::HillslopeDiffusion => self.hillslope.is_some(),
            ComputeOp::StreamPowerIncision => self.stream_power.is_some(),
        }
    }

    fn run_hillslope_diffusion(
        &self,
        _world: &mut WorldState,
        _params: &HillslopeParams,
    ) -> Result<StageTiming, ComputeBackendError> {
        Err(ComputeBackendError::Unsupported {
            backend: "gpu",
            op: "hillslope_diffusion",
        })
    }

    fn run_stream_power_incision(
        &self,
        _world: &mut WorldState,
        _params: &StreamPowerParams,
    ) -> Result<StageTiming, ComputeBackendError> {
        Err(ComputeBackendError::Unsupported {
            backend: "gpu",
            op: "stream_power_incision",
        })
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

    /// Sprint 4.D contract: both pilot op slots are `None`, so `supports`
    /// returns `false` for both operations.
    ///
    /// Marked #[ignore] — requires a real GPU adapter.
    #[test]
    #[ignore = "requires a working GPU adapter; opt in with IPG_RUN_GPU_TESTS=1"]
    fn gpu_backend_supports_returns_false_for_both_pilot_ops_at_4_d() {
        if std::env::var("IPG_RUN_GPU_TESTS").as_deref() != Ok("1") {
            eprintln!("skipped — set IPG_RUN_GPU_TESTS=1 to run GPU tests");
            return;
        }
        let ctx =
            Arc::new(crate::GpuContext::new_headless((64, 64)).expect("headless context required"));
        let backend = GpuBackend::new(ctx);
        assert!(
            !backend.supports(ComputeOp::HillslopeDiffusion),
            "HillslopeDiffusion must not be supported at 4.D"
        );
        assert!(
            !backend.supports(ComputeOp::StreamPowerIncision),
            "StreamPowerIncision must not be supported at 4.D"
        );
    }
}
