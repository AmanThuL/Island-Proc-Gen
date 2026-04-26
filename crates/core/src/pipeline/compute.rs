//! `ComputeBackend` trait + associated types — Sprint 4.C DD1.
//!
//! Defines the **stable kernel dispatch boundary** between simulation logic
//! and compute backends (CPU, GPU). The trait is deliberately kept narrow:
//! only the two pilot kernels (`HillslopeDiffusion`, `StreamPowerIncision`)
//! are enumerated in `ComputeOp::ALL`, which is the DD1 snapshot lock.
//!
//! # Architectural rules
//!
//! * `core` stays headless. This file uses **only** `serde + thiserror + std`.
//!   No `wgpu`, `winit`, `egui*`, `png`, `image`, or `tempfile`. Enforced by
//!   the `compute_backend_trait_in_core_has_no_wgpu_dependency` test in
//!   `crates/sim/src/compute/cpu_backend.rs`.
//! * The trait is `Send + Sync` so a `dyn ComputeBackend` can be wrapped in
//!   `Arc` and shared across threads without `unsafe`.
//! * `WorldState` ownership is taken as `&mut` so kernels write
//!   `authoritative.height` / `authoritative.sediment` in place without
//!   opaque buffer handles (a Sprint 4.x concern).
//!
//! # Extension rule (DD1)
//!
//! Adding a third `ComputeOp` variant **must** unlock the snapshot test
//! `compute_op_enum_snapshot` (update the `assert_eq!(…, 2)` assertion) and
//! review DD1's alternatives table. Never add a variant silently.

use crate::pipeline::StageTiming;
use crate::world::WorldState;

// ─── ComputeOp ───────────────────────────────────────────────────────────────

/// Identifies a discrete compute operation that a [`ComputeBackend`] may
/// accelerate.
///
/// The locked snapshot of all pilot ops is [`ComputeOp::ALL`]. A future
/// Sprint 4.x that adds a 3rd kernel must update `ALL` and unlock the
/// `compute_op_enum_snapshot` test.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComputeOp {
    /// Hillslope diffusion: `∂z/∂t = D · ∇²z` explicit-Euler stencil.
    HillslopeDiffusion,
    /// Stream Power Incision: `E_f = K · A^m · S^n` plus SPACE-lite dual eq.
    StreamPowerIncision,
}

impl ComputeOp {
    /// Locked snapshot of every pilot op — Sprint 4.C DD1.
    ///
    /// Adding a variant here MUST be accompanied by unlocking the
    /// `compute_op_enum_snapshot` test and reviewing DD1's alternatives table.
    pub const ALL: &'static [ComputeOp] = &[
        ComputeOp::HillslopeDiffusion,
        ComputeOp::StreamPowerIncision,
    ];
}

// ─── HillslopeParams / StreamPowerParams ─────────────────────────────────────

/// Parameters extracted from `ErosionParams` for the hillslope diffusion
/// kernel.
///
/// Constructed by `HillslopeDiffusionStage::run` from `world.preset.erosion`
/// before dispatching through the backend. No `&IslandArchetypePreset`
/// reference is held — the kernel only sees plain scalars.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct HillslopeParams {
    /// Linear diffusivity `D` in `∂z/∂t = D · ∇²z`.
    pub hillslope_d: f32,
    /// Number of explicit-Euler sub-steps per kernel call.
    pub n_diff_substep: u32,
}

/// Parameters extracted from `ErosionParams` for the stream power incision
/// kernel.
///
/// Covers both `SpimVariant::Plain` and `SpimVariant::SpaceLite` — the kernel
/// branches internally on `spim_variant`. No `&IslandArchetypePreset`
/// reference is held.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct StreamPowerParams {
    /// SPIM erodibility `K` (Plain branch).
    pub spim_k: f32,
    /// Drainage area exponent `m`.
    pub spim_m: f32,
    /// Slope exponent `n`.
    pub spim_n: f32,
    /// SPACE-lite bedrock erodibility `K_bed`.
    pub space_k_bed: f32,
    /// SPACE-lite sediment entrainability `K_sed`.
    pub space_k_sed: f32,
    /// SPACE-lite cover thickness `H*`.
    pub h_star: f32,
    /// Sea level — lower bound for `z` after incision.
    pub sea_level: f32,
    /// Which SPIM variant to run.
    pub spim_variant: crate::preset::SpimVariant,
}

// ─── ComputeBackendError ─────────────────────────────────────────────────────

/// Errors returned by a [`ComputeBackend`] method.
#[derive(Debug, thiserror::Error)]
pub enum ComputeBackendError {
    /// The backend does not support the requested operation.
    ///
    /// Callers should fall back to the CPU kernel when they encounter this.
    #[error("backend '{backend}' does not support op '{op}'")]
    Unsupported {
        /// Name of the backend (e.g. `"noop"`, `"gpu"`).
        backend: &'static str,
        /// Name of the operation (e.g. `"HillslopeDiffusion"`).
        op: &'static str,
    },

    /// The GPU device was lost; recovery requires re-initialisation.
    #[error("compute backend: GPU device lost")]
    DeviceLost,

    /// A GPU readback timed out before the CPU could access the result.
    #[error("compute backend: readback timed out")]
    ReadbackTimeout,

    /// Any other error from the backend (IO, shader compile, etc.).
    #[error("compute backend: {0}")]
    Other(Box<dyn std::error::Error + Send + Sync>),
}

// ─── ComputeBackend trait ─────────────────────────────────────────────────────

/// Dispatch boundary between simulation stages and compute kernels.
///
/// Both CPU and GPU backends implement this trait. The CPU variant
/// (`crates/sim/src/compute/cpu_backend.rs`) calls the same free kernel
/// functions as the stage impls to guarantee bit-identical output on the
/// CPU path. A future GPU backend writes to `authoritative.height` via
/// a staging-buffer readback.
///
/// # Object safety
///
/// The trait is object-safe on purpose: `ErosionOuterLoop` stores
/// `Arc<dyn ComputeBackend>` so the backend can be swapped at construction
/// time without monomorphising `ErosionOuterLoop`.
pub trait ComputeBackend: Send + Sync {
    /// Stable name for tracing / profiler UI.
    fn name(&self) -> &'static str;

    /// Returns `true` if this backend can execute `op` (i.e. `run_*` will
    /// succeed rather than returning [`ComputeBackendError::Unsupported`]).
    fn supports(&self, op: ComputeOp) -> bool;

    /// Run one explicit-Euler hillslope diffusion step in place on
    /// `world.authoritative.height`.
    ///
    /// `world.derived.coast_mask` must be `Some` (prerequisite).
    /// Returns a [`StageTiming`] with `cpu_ms` always populated;
    /// `gpu_ms` is `Some` only when the kernel ran on GPU hardware.
    fn run_hillslope_diffusion(
        &self,
        world: &mut WorldState,
        params: &HillslopeParams,
    ) -> Result<StageTiming, ComputeBackendError>;

    /// Run one SPIM incision step in place on `world.authoritative.height`
    /// (and `authoritative.sediment` for `SpimVariant::SpaceLite`).
    ///
    /// `world.derived.{accumulation, slope, coast_mask}` must be `Some`.
    /// Returns a [`StageTiming`] with `cpu_ms` always populated;
    /// `gpu_ms` is `Some` only when the kernel ran on GPU hardware.
    fn run_stream_power_incision(
        &self,
        world: &mut WorldState,
        params: &StreamPowerParams,
    ) -> Result<StageTiming, ComputeBackendError>;
}

// ─── NoOpBackend ─────────────────────────────────────────────────────────────

/// A backend that supports no operations and immediately returns
/// [`ComputeBackendError::Unsupported`] for every kernel call.
///
/// Used as a sentinel in tests and as the default when no backend has been
/// wired. Not intended for production use.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoOpBackend;

impl ComputeBackend for NoOpBackend {
    fn name(&self) -> &'static str {
        "noop"
    }

    fn supports(&self, _op: ComputeOp) -> bool {
        false
    }

    fn run_hillslope_diffusion(
        &self,
        _world: &mut WorldState,
        _params: &HillslopeParams,
    ) -> Result<StageTiming, ComputeBackendError> {
        Err(ComputeBackendError::Unsupported {
            backend: "noop",
            op: "HillslopeDiffusion",
        })
    }

    fn run_stream_power_incision(
        &self,
        _world: &mut WorldState,
        _params: &StreamPowerParams,
    ) -> Result<StageTiming, ComputeBackendError> {
        Err(ComputeBackendError::Unsupported {
            backend: "noop",
            op: "StreamPowerIncision",
        })
    }
}

// ─── tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// DD1 snapshot lock: `ComputeOp::ALL` must contain exactly 2 pilot ops.
    ///
    /// A future Sprint 4.x that adds a 3rd kernel **must** update this
    /// assertion AND review DD1's alternatives table before landing.
    #[test]
    fn compute_op_enum_snapshot() {
        assert_eq!(ComputeOp::ALL.len(), 2);
        // Verify the exact variants so reordering is also caught.
        assert_eq!(ComputeOp::ALL[0], ComputeOp::HillslopeDiffusion);
        assert_eq!(ComputeOp::ALL[1], ComputeOp::StreamPowerIncision);
    }

    /// `NoOpBackend` must return `false` for every op.
    #[test]
    fn noop_backend_supports_nothing() {
        let b = NoOpBackend;
        for &op in ComputeOp::ALL {
            assert!(!b.supports(op), "NoOpBackend must not support {op:?}");
        }
    }

    /// `NoOpBackend::name()` returns "noop".
    #[test]
    fn noop_backend_name_is_noop() {
        assert_eq!(NoOpBackend.name(), "noop");
    }
}
