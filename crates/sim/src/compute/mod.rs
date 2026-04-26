//! Compute backend implementations for `sim`.
//!
//! Sprint 4.C introduces the `ComputeBackend` trait (in `core::pipeline::compute`)
//! and the first concrete implementation, [`CpuBackend`], here.
//!
//! Sprint 4.D+ will add `GpuBackend` in this module once the WGPU infrastructure
//! lands in `crates/gpu/`.

pub mod cpu_backend;
pub use cpu_backend::CpuBackend;
