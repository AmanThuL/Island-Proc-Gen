//! GPU context — wgpu Instance / Surface / Adapter / Device / Queue.
//!
//! The actual implementation lives in [`context`]; this root module exists
//! to keep `gpu::GpuContext` / `gpu::DEPTH_FORMAT` as the stable public API
//! while Sprint 1C expands the crate with additional siblings
//! (e.g. offscreen helpers).
//!
//! Sprint 4.D adds [`compute`] with [`compute::GpuBackend`],
//! [`compute::buffers`], and [`compute::timestamp`].

pub mod compute;
pub mod context;

pub use compute::GpuBackend;
pub use context::{
    DEPTH_FORMAT, GPU_BOOTSTRAP_SIZE_FOR_COMPUTE, GpuContext, HEADLESS_COLOR_FORMAT,
};
