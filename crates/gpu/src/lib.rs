//! GPU context — wgpu Instance / Surface / Adapter / Device / Queue.
//!
//! The actual implementation lives in [`context`]; this root module exists
//! to keep `gpu::GpuContext` / `gpu::DEPTH_FORMAT` as the stable public API
//! while Sprint 1C expands the crate with additional siblings
//! (e.g. offscreen helpers).

pub mod context;

pub use context::{DEPTH_FORMAT, GpuContext, HEADLESS_COLOR_FORMAT};
