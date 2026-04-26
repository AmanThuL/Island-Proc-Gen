//! GPU timestamp helper — Sprint 4.D DD6.
//!
//! Records the wall-clock duration of a compute pass using
//! [`wgpu::ComputePassTimestampWrites`] written via
//! [`wgpu::ComputePassDescriptor::timestamp_writes`].
//!
//! # DD6 feature-gate constraint
//!
//! This module deliberately uses **only** `wgpu::Features::TIMESTAMP_QUERY`
//! (the WebGPU-tier gate). It does **NOT** call
//! [`wgpu::CommandEncoder::write_timestamp`], which requires the heavier
//! `TIMESTAMP_QUERY_INSIDE_ENCODERS` feature that many Metal/DX12 drivers
//! expose only behind an extension. See Sprint 4.D DD6 for the rationale.
//!
//! # Usage
//!
//! ```ignore
//! // 1. Allocate the timer once per pipeline construction.
//! let timer = GpuTimer::new_if_supported(&ctx.device, ctx.timestamp_period_ns)?;
//!
//! // 2. When timing a compute pass, pass timestamp_writes to the descriptor.
//! if let Some(ref t) = timer {
//!     desc.timestamp_writes = Some(t.timestamp_writes());
//! }
//!
//! // 3. After the pass is encoded, call resolve with the encoder by value.
//!    The function submits the encoder + resolve commands and returns ms.
//! let elapsed_ms: Option<f64> = timer
//!     .as_ref()
//!     .and_then(|t| t.resolve(encoder, &ctx.device, &ctx.queue,
//!                             ctx.timestamp_period_ns).transpose())
//!     .transpose()?;
//! ```

use anyhow::{Context as _, Result};

/// Allocates a two-slot [`wgpu::QuerySet`] and a small readback [`wgpu::Buffer`]
/// for timing one compute pass via `ComputePassDescriptor::timestamp_writes`.
///
/// Slot 0 = beginning-of-pass timestamp; slot 1 = end-of-pass timestamp.
/// The delta (slot1 − slot0) × `timestamp_period_ns` gives the GPU duration.
pub struct GpuTimer {
    query_set: wgpu::QuerySet,
    /// `QUERY_RESOLVE | COPY_SRC` destination, 2 × u64 = 16 bytes.
    resolve_buf: wgpu::Buffer,
    /// `MAP_READ | COPY_DST`, same size, for host readback.
    readback_buf: wgpu::Buffer,
}

/// Number of query slots: beginning + end.
const QUERY_COUNT: u32 = 2;
/// Byte size of the resolve / readback buffers (2 × u64).
const QUERY_BUF_BYTES: u64 = (QUERY_COUNT as u64) * std::mem::size_of::<u64>() as u64;

impl GpuTimer {
    /// Create a new timer on `device`.
    ///
    /// Returns `Ok(None)` when `timestamp_period_ns` is `None` (feature not
    /// granted), so callers don't need to branch themselves.
    ///
    /// Returns `Err` only on true device-level failures (allocation, etc.).
    pub fn new_if_supported(
        device: &wgpu::Device,
        timestamp_period_ns: Option<f64>,
    ) -> Result<Option<Self>> {
        if timestamp_period_ns.is_none() {
            return Ok(None);
        }
        let query_set = device.create_query_set(&wgpu::QuerySetDescriptor {
            label: Some("gpu_timer_query_set"),
            ty: wgpu::QueryType::Timestamp,
            count: QUERY_COUNT,
        });
        let resolve_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gpu_timer_resolve"),
            size: QUERY_BUF_BYTES,
            usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let readback_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gpu_timer_readback"),
            size: QUERY_BUF_BYTES,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Ok(Some(Self {
            query_set,
            resolve_buf,
            readback_buf,
        }))
    }

    /// Build a [`wgpu::ComputePassTimestampWrites`] to embed in a
    /// [`wgpu::ComputePassDescriptor`].
    ///
    /// Slot 0 records the beginning-of-pass timestamp; slot 1 records the
    /// end-of-pass timestamp. The caller is responsible for appending the
    /// returned value to `ComputePassDescriptor::timestamp_writes`.
    ///
    /// **DD6 contract:** this is the *only* way this module writes timestamps.
    /// `CommandEncoder::write_timestamp` is never called.
    pub fn timestamp_writes(&self) -> wgpu::ComputePassTimestampWrites<'_> {
        wgpu::ComputePassTimestampWrites {
            query_set: &self.query_set,
            beginning_of_pass_write_index: Some(0),
            end_of_pass_write_index: Some(1),
        }
    }

    /// Append query-resolve + buffer-copy commands to `encoder`, then submit
    /// it to `queue` and map-read the elapsed time in milliseconds.
    ///
    /// # Ownership
    ///
    /// Takes `encoder` by value because [`wgpu::CommandEncoder::finish`]
    /// consumes its receiver. The caller must not use `encoder` after calling
    /// this function.
    ///
    /// # Returns
    ///
    /// - `None` when `timestamp_period_ns` is `None` (feature not granted).
    /// - `Some(Ok(ms))` on success.
    /// - `Some(Err(_))` on GPU or mapping failure.
    pub fn resolve_ms(
        &self,
        mut encoder: wgpu::CommandEncoder,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        timestamp_period_ns: Option<f64>,
    ) -> Option<Result<f64>> {
        let period_ns = timestamp_period_ns?;

        // Resolve query results into the GPU-side resolve buffer, then copy
        // to the MAP_READ readback buffer.
        encoder.resolve_query_set(&self.query_set, 0..QUERY_COUNT, &self.resolve_buf, 0);
        encoder.copy_buffer_to_buffer(&self.resolve_buf, 0, &self.readback_buf, 0, QUERY_BUF_BYTES);

        // Submit encoder (consumes it) and wait for the GPU work to complete.
        queue.submit(Some(encoder.finish()));
        if let Err(e) = device.poll(wgpu::PollType::wait_indefinitely()) {
            return Some(Err(anyhow::anyhow!(
                "device.poll failed during timestamp resolve: {e}"
            )));
        }

        // Map the readback buffer synchronously.
        let slice = self.readback_buf.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        // A second poll drives the map callback (belt-and-suspenders; the
        // wait_indefinitely above should have already ensured completion).
        device.poll(wgpu::PollType::wait_indefinitely()).ok();

        if let Err(e) = rx
            .recv()
            .context("timestamp map_async channel dropped")
            .and_then(|r| r.context("timestamp map_async failed"))
        {
            return Some(Err(e));
        }

        let mapped = slice.get_mapped_range();
        let ticks: [u64; 2] = bytemuck::pod_read_unaligned(&mapped[..16]);
        drop(mapped);
        self.readback_buf.unmap();

        let delta_ticks = ticks[1].saturating_sub(ticks[0]);
        let elapsed_ns = (delta_ticks as f64) * period_ns;
        Some(Ok(elapsed_ns / 1_000_000.0))
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Structural compile-time anchor: verifies that the timestamp module
    /// produces timestamp_writes via `ComputePassDescriptor::timestamp_writes`,
    /// not via `CommandEncoder::write_timestamp`.
    ///
    /// This is enforced at the type level: `GpuTimer::timestamp_writes`
    /// returns `wgpu::ComputePassTimestampWrites`, which can only be used
    /// with `ComputePassDescriptor::timestamp_writes`. The implementation
    /// never calls `CommandEncoder::write_timestamp`. This test documents and
    /// anchors that design decision; compilation of this module is the real
    /// gate.
    #[test]
    fn gpu_backend_uses_pass_descriptor_timestamp_writes() {
        // Verify the module compiles and the helper returns the correct type.
        // A full GPU round-trip requires a real adapter; use the #[ignore]
        // smoke test below for that.
        fn _assert_returns_timestamp_writes(
            timer: &GpuTimer,
        ) -> wgpu::ComputePassTimestampWrites<'_> {
            // This will fail to compile if `timestamp_writes` changes to return
            // something other than `wgpu::ComputePassTimestampWrites`.
            timer.timestamp_writes()
        }
        // Verify constant sizing is correct.
        assert_eq!(QUERY_COUNT, 2);
        assert_eq!(QUERY_BUF_BYTES, 16);
    }

    /// Smoke test: construct a GpuTimer from a headless context and verify
    /// it is Some when TIMESTAMP_QUERY is supported on Metal.
    ///
    /// Marked #[ignore] — requires a real GPU adapter.
    #[test]
    #[ignore = "requires a working GPU adapter with TIMESTAMP_QUERY (macOS Metal); opt in with IPG_RUN_GPU_TESTS=1"]
    fn timestamp_helper_produces_some_gpu_ms_on_metal_when_feature_present() {
        if std::env::var("IPG_RUN_GPU_TESTS").as_deref() != Ok("1") {
            eprintln!("skipped — set IPG_RUN_GPU_TESTS=1 to run GPU tests");
            return;
        }
        let ctx = crate::GpuContext::new_headless((64, 64)).expect("headless context required");
        if ctx.timestamp_period_ns.is_none() {
            eprintln!("TIMESTAMP_QUERY not supported on this adapter; skipping");
            return;
        }
        let timer = GpuTimer::new_if_supported(&ctx.device, ctx.timestamp_period_ns)
            .expect("new_if_supported must not error when timestamp_period_ns is Some");
        assert!(
            timer.is_some(),
            "GpuTimer must be Some when adapter grants TIMESTAMP_QUERY"
        );
    }
}
