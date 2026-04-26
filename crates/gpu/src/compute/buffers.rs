//! GPU buffer helpers for compute passes — Sprint 4.D.
//!
//! Provides helpers to upload a [`ScalarField2D<f32>`] to a GPU storage buffer
//! and read the result back to host memory after a compute pass.
//!
//! # Design
//!
//! The helpers are `f32`-only for Sprint 4.D (YAGNI). Sprint 4.E / 4.F
//! can introduce additional typed variants when concrete kernels need them.
//! Generic bounds over `FieldDtype` are intentionally avoided — that private
//! trait is sealed inside `core::field` and not exposed for downstream use.

use anyhow::{Context as _, Result};
use island_core::ScalarField2D;

/// Create a GPU `STORAGE | COPY_DST | COPY_SRC` buffer pre-populated with
/// the byte content of `field`.
///
/// The buffer size is `field.width() * field.height() * 4` bytes (one `f32`
/// per cell in row-major order, matching [`ScalarField2D::to_bytes`]).
///
/// The returned buffer is ready to bind as a storage buffer in a compute
/// pipeline. After the compute pass completes, use [`readback_f32`] to copy
/// results back to host memory.
///
/// # Panics
///
/// Panics if `field` is empty (zero width or height), as wgpu does not allow
/// zero-size buffers.
pub fn upload_f32_field(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    field: &ScalarField2D<f32>,
    label: &str,
) -> wgpu::Buffer {
    let bytes = field.to_bytes();
    debug_assert!(!bytes.is_empty(), "upload_f32_field: empty field");

    let buf = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: bytes.len() as u64,
        usage: wgpu::BufferUsages::STORAGE
            | wgpu::BufferUsages::COPY_DST
            | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    queue.write_buffer(&buf, 0, &bytes);
    buf
}

/// Copy the contents of a `COPY_SRC` GPU buffer back to host as `Vec<f32>`.
///
/// `element_count` must equal `width * height` of the original field so the
/// returned `Vec` has the correct length. The function issues a
/// `copy_buffer_to_buffer` → `device.poll(wait_indefinitely)` → `map_async`
/// round-trip and returns once all data is on the host.
///
/// # Errors
///
/// Returns `Err` if the copy, poll, or map fails.
pub fn readback_f32(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    src: &wgpu::Buffer,
    element_count: usize,
) -> Result<Vec<f32>> {
    let byte_size = (element_count * std::mem::size_of::<f32>()) as u64;
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback_f32"),
        size: byte_size,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("readback_f32_encoder"),
    });
    encoder.copy_buffer_to_buffer(src, 0, &readback, 0, byte_size);
    queue.submit(Some(encoder.finish()));

    device
        .poll(wgpu::PollType::wait_indefinitely())
        .context("readback_f32: device.poll failed")?;

    let slice = readback.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    device.poll(wgpu::PollType::wait_indefinitely()).ok();

    rx.recv()
        .context("readback_f32: map_async channel dropped")?
        .context("readback_f32: map_async failed")?;

    let mapped = slice.get_mapped_range();
    let floats: Vec<f32> = bytemuck::cast_slice(&mapped[..]).to_vec();
    drop(mapped);
    readback.unmap();

    Ok(floats)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use island_core::ScalarField2D;

    /// Smoke test: upload a small f32 field to GPU and read it back.
    ///
    /// Marked #[ignore] — requires a real GPU adapter.
    #[test]
    #[ignore = "requires a working GPU adapter; run with IPG_RUN_GPU_TESTS=1"]
    fn upload_and_readback_f32_roundtrips() {
        if std::env::var("IPG_RUN_GPU_TESTS").as_deref() != Ok("1") {
            eprintln!("skipped — set IPG_RUN_GPU_TESTS=1 to run GPU tests");
            return;
        }
        let ctx = crate::GpuContext::new_headless((4, 4)).expect("headless context required");

        let mut field: ScalarField2D<f32> = ScalarField2D::new(4, 4);
        for y in 0..4 {
            for x in 0..4 {
                field.set(x, y, (y * 4 + x) as f32 * 0.5);
            }
        }

        let buf = upload_f32_field(&ctx.device, &ctx.queue, &field, "test_upload");
        let result =
            readback_f32(&ctx.device, &ctx.queue, &buf, 16).expect("readback must succeed");

        assert_eq!(result.len(), 16);
        for (i, &v) in result.iter().enumerate() {
            let expected = i as f32 * 0.5;
            assert!(
                (v - expected).abs() < 1e-6,
                "element {i}: expected {expected}, got {v}"
            );
        }
    }
}
