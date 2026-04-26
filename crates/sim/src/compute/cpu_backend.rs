//! CPU compute backend — Sprint 4.C.
//!
//! [`CpuBackend`] implements [`ComputeBackend`] by calling the same free
//! kernel functions that the stage impls call directly. The dispatch path is:
//!
//! ```text
//! ErosionOuterLoop (Arc<dyn ComputeBackend>)
//!   └─ CpuBackend::run_hillslope_diffusion
//!        └─ geomorph::hillslope::hillslope_diffusion_kernel   ← single source of math
//!   └─ CpuBackend::run_stream_power_incision
//!        └─ geomorph::stream_power::stream_power_incision_kernel  ← single source of math
//! ```
//!
//! HillslopeDiffusionStage / StreamPowerIncisionStage (standalone, NOT inside
//! ErosionOuterLoop) continue to call the free functions directly so they are
//! not on the trait-dispatch path — consistent with the spec's "single source
//! of truth for the math" requirement.
//!
//! # Bit-identity contract
//!
//! `CpuBackend` and the direct stage calls produce byte-identical
//! `authoritative.height` / `authoritative.sediment` fields from identical
//! inputs. This is guaranteed by sharing the same free kernel functions.

use std::time::Instant;

use island_core::pipeline::{
    ComputeBackend, ComputeBackendError, ComputeOp, HillslopeParams, StageTiming, StreamPowerParams,
};
use island_core::world::WorldState;

use crate::geomorph::{hillslope_diffusion_kernel, stream_power_incision_kernel};

/// CPU implementation of [`ComputeBackend`].
///
/// Supports all pilot ops ([`ComputeOp::ALL`]) by delegating to the same
/// free kernel functions that the standalone stage impls call. No GPU
/// involvement; `StageTiming::gpu_ms` is always `None`.
#[derive(Debug, Default, Clone, Copy)]
pub struct CpuBackend;

impl ComputeBackend for CpuBackend {
    fn name(&self) -> &'static str {
        "cpu"
    }

    fn supports(&self, op: ComputeOp) -> bool {
        matches!(
            op,
            ComputeOp::HillslopeDiffusion | ComputeOp::StreamPowerIncision
        )
    }

    fn run_hillslope_diffusion(
        &self,
        world: &mut WorldState,
        params: &HillslopeParams,
    ) -> Result<StageTiming, ComputeBackendError> {
        let cpu_start = Instant::now();
        hillslope_diffusion_kernel(world, params);
        Ok(StageTiming {
            cpu_ms: cpu_start.elapsed().as_secs_f64() * 1_000.0,
            gpu_ms: None,
        })
    }

    fn run_stream_power_incision(
        &self,
        world: &mut WorldState,
        params: &StreamPowerParams,
    ) -> Result<StageTiming, ComputeBackendError> {
        let cpu_start = Instant::now();
        stream_power_incision_kernel(world, params)
            .map_err(|e| ComputeBackendError::Other(e.into()))?;
        Ok(StageTiming {
            cpu_ms: cpu_start.elapsed().as_secs_f64() * 1_000.0,
            gpu_ms: None,
        })
    }
}

// ─── tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use island_core::pipeline::ComputeOp;

    /// `CpuBackend` supports both pilot ops.
    #[test]
    fn cpu_backend_supports_pilot_ops() {
        let b = CpuBackend;
        assert!(
            b.supports(ComputeOp::HillslopeDiffusion),
            "CpuBackend must support HillslopeDiffusion"
        );
        assert!(
            b.supports(ComputeOp::StreamPowerIncision),
            "CpuBackend must support StreamPowerIncision"
        );
    }

    /// `CpuBackend::name()` returns `"cpu"`.
    #[test]
    fn cpu_backend_name_is_cpu() {
        assert_eq!(CpuBackend.name(), "cpu");
    }

    /// Verify this file has no wgpu import at the source level — compile-time
    /// anchor for invariant #1. The test body is trivial; the meaningful check
    /// is that this file compiles as part of the `sim` crate without the
    /// `wgpu` dependency in the cargo graph.
    #[test]
    fn compute_backend_trait_in_core_has_no_wgpu_dependency() {
        // If wgpu leaked into `core::pipeline::compute`, `cargo tree -p core`
        // would list it and this comment would be the only surviving clue.
        // The actual enforcement is `cargo tree -p core | grep wgpu` in CI.
        let _ = CpuBackend.name();
    }
}
