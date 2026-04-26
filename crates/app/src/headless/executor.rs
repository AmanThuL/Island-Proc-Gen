//! Headless capture executor — glue between request, pipeline, truth bake,
//! and (when a GPU adapter is available) the beauty render.
//!
//! See `docs/design/sprints/sprint_1c_headless_validation.md` §AD6 / AD8 for
//! the architectural decisions this module enforces:
//!
//! - [`run_request`] is the sole entry point from `main.rs` — it loads the
//!   request, attempts to bootstrap [`GpuContext::new_headless`] **once**
//!   (AD8), iterates the shots, and writes `summary.ron`.
//! - [`run_shot`] is called per shot; it never tries to construct a GPU
//!   context. It receives `gpu_opt: Option<&GpuContext>` from the top level
//!   and threads it down into the beauty branch.
//! - The truth (CPU) path is authoritative. A GPU adapter failure downgrades
//!   `overall_status` to [`OverallStatus::PassedWithBeautySkipped`] only when
//!   truth green — it never short-circuits the CPU bake.
//!
//! Exit-code mapping lives on [`OverallStatus::exit_code`] (AD9); `main.rs`
//! uses it to set the process exit code.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context as _, Result, bail};
use tracing::{error, info, warn};

use crate::runtime::view_mode::{RenderLayer, ViewMode, render_stack_for};
use data::golden::SummaryMetrics;
use gpu::GpuContext;
use island_core::{
    seed::Seed,
    world::{Resolution, WorldState},
};
use render::overlay::OverlayRegistry;
use render::overlay_export::bake_overlay_to_rgba8;

use crate::headless::output::{
    BeautyStatus, BeautySummary, InternalErrorKind, OverallStatus, RunLayout, RunSummary,
    ShotSummary, TruthSummary, compute_request_fingerprint, compute_run_id, now_utc_iso8601,
    write_metrics_ron, write_request_ron, write_rgba8_png, write_summary_ron,
};
use crate::headless::request::{CaptureRequest, CaptureShot};

// ─── run_request ──────────────────────────────────────────────────────────────

/// Canonical beauty resolution used to bootstrap the headless GPU context.
/// Individual shots may override their own capture size via
/// [`crate::headless::request::BeautySpec::resolution`]; this constant only
/// sizes the persistent depth buffer that ships with the context.
const GPU_BOOTSTRAP_SIZE: (u32, u32) = (1280, 800);

/// Env var that, when set to `1`, forces [`GpuContext::new_headless`] to be
/// *treated as* failed at the top of [`run_request`] without actually
/// attempting the initialisation. Used by the AD8 fallback tests below to
/// exercise the skip branch on a machine where the adapter would otherwise
/// succeed.
///
/// This is a test-only mock hook — the CLI in `main.rs` does not document it,
/// and production users should never set it.
const FORCE_GPU_FAIL_ENV: &str = "IPG_FORCE_HEADLESS_GPU_FAIL";

/// Execute the [`CaptureRequest`] stored at `request_path`.
///
/// On success the returned [`OverallStatus`] is exactly what was written to
/// `<output_dir>/summary.ron`; `main.rs` then maps it to the AD9 exit code.
///
/// Errors of the "couldn't even load the file" variety are represented as
/// `Ok(OverallStatus::InternalError { .. })` so the exit-code mapping stays a
/// single switch — `Err(_)` is reserved for genuinely unexpected panics that
/// bubble out of `anyhow::Result` from pipelines / GPU submits.
pub fn run_request(request_path: &Path) -> Result<OverallStatus> {
    // ── Load request ─────────────────────────────────────────────────────────
    let request_text = match std::fs::read_to_string(request_path) {
        Ok(s) => s,
        Err(e) => {
            // No output dir exists yet — no summary.ron to write.
            warn!(path = ?request_path, err = %e, "no summary.ron written: could not read request file");
            return Ok(OverallStatus::InternalError {
                reason: format!("read {request_path:?} failed: {e}"),
                kind: InternalErrorKind::Io,
            });
        }
    };

    let req: CaptureRequest = match ron::de::from_str(&request_text) {
        Ok(r) => r,
        Err(e) => {
            // No output dir exists yet — no summary.ron to write.
            warn!(path = ?request_path, err = %e, "no summary.ron written: could not parse request RON");
            return Ok(OverallStatus::InternalError {
                reason: format!("parse {request_path:?} as CaptureRequest failed: {e}"),
                kind: InternalErrorKind::RonParse,
            });
        }
    };

    // ── Resolve effective run_id / output_dir ───────────────────────────────
    let run_id = req.run_id.clone().unwrap_or_else(|| compute_run_id(&req));
    let output_dir = req
        .output_dir
        .clone()
        .unwrap_or_else(|| PathBuf::from("captures/headless").join(&run_id));

    let layout = RunLayout::new(output_dir);
    if let Err(e) = std::fs::create_dir_all(&layout.root) {
        // Output dir could not be created — no summary.ron to write.
        warn!(root = ?layout.root, err = %e, "no summary.ron written: could not create output directory");
        return Ok(OverallStatus::InternalError {
            reason: format!("create_dir_all {:?} failed: {e}", layout.root),
            kind: InternalErrorKind::Io,
        });
    }

    // Self-contained audit: copy the request into the run directory.
    if let Err(e) = write_request_ron(&layout, &req) {
        // Output dir exists but request copy failed — no summary.ron to write.
        warn!(err = %e, "no summary.ron written: could not write request.ron audit copy");
        return Ok(OverallStatus::InternalError {
            reason: format!("write_request_ron failed: {e}"),
            kind: InternalErrorKind::Io,
        });
    }

    // ── AD8 top-level GPU bootstrap — one attempt, reused across all shots ──
    // Test-only hook: FORCE_GPU_FAIL_ENV is a private test sentinel — never
    // document it in CLI help or rely on it in production scripts.
    let force_fail = std::env::var(FORCE_GPU_FAIL_ENV)
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    let (gpu_opt, gpu_unavailable_reason) = if force_fail {
        let reason =
            format!("{FORCE_GPU_FAIL_ENV}=1 — GPU bootstrap deliberately skipped (test-only hook)");
        warn!(reason = %reason, "Headless GPU bootstrap force-skipped via env var");
        (None, reason)
    } else {
        match GpuContext::new_headless(GPU_BOOTSTRAP_SIZE) {
            Ok(ctx) => {
                info!(
                    width = GPU_BOOTSTRAP_SIZE.0,
                    height = GPU_BOOTSTRAP_SIZE.1,
                    "Headless GPU bootstrap succeeded; beauty renders will run"
                );
                (Some(ctx), String::new())
            }
            Err(e) => {
                let reason = format!("GpuContext::new_headless failed: {e:#}");
                warn!(reason = %reason, "Headless GPU bootstrap failed; beauty shots will be Skipped");
                (None, reason)
            }
        }
    };

    // ── Execute shots ────────────────────────────────────────────────────────
    let fingerprint = compute_request_fingerprint(&req);
    let timestamp = now_utc_iso8601();

    let mut shot_summaries: Vec<ShotSummary> = Vec::with_capacity(req.shots.len());
    let mut mid_shot_error: Option<(String, InternalErrorKind)> = None;
    for shot in &req.shots {
        match run_shot(shot, gpu_opt.as_ref(), &layout, &gpu_unavailable_reason) {
            Ok(summary) => shot_summaries.push(summary),
            Err(e) => {
                let kind = classify_shot_error(&e);
                mid_shot_error = Some((format!("shot {:?}: {e:#}", shot.id), kind));
                break;
            }
        }
    }

    // ── AD9 aggregate ────────────────────────────────────────────────────────
    let overall_status = if let Some((reason, kind)) = mid_shot_error {
        // Mid-shot crash: write a partial summary so consumers can distinguish
        // "crashed partway through" from "never started".
        OverallStatus::InternalError { reason, kind }
    } else {
        let skipped_shot_ids: Vec<String> = shot_summaries
            .iter()
            .filter(|s| {
                matches!(
                    s.beauty.as_ref().map(|b| &b.status),
                    Some(BeautyStatus::Skipped { .. })
                )
            })
            .map(|s| s.id.clone())
            .collect();

        if gpu_opt.is_none() && !skipped_shot_ids.is_empty() {
            OverallStatus::PassedWithBeautySkipped {
                skipped_shot_ids,
                reason: gpu_unavailable_reason,
            }
        } else {
            OverallStatus::Passed
        }
    };

    // ── Build summary + write ────────────────────────────────────────────────
    // Sprint 4.A DD2: the v4 binary always writes `schema_version: 4` regardless
    // of the input request's schema. This diverges from the v1–v3 policy of
    // mirroring the request version. v3 baselines on disk will read as v3 until
    // the 4.B chore(data) commit regenerates them; the compare tool's
    // schema_version check is updated to accept run >= expected, so old baselines
    // continue to pass under the v4 binary (see compare.rs).
    let summary = RunSummary {
        schema_version: 4,
        run_id,
        request_fingerprint: fingerprint,
        timestamp_utc: timestamp,
        shots: shot_summaries,
        overall_status,
        warnings: Vec::new(),
    };

    if let Err(e) = write_summary_ron(&layout, &summary) {
        // Writing the summary failed; log to stderr but still return the
        // outcome so the process exit code is correct.
        error!(err = %e, "write_summary_ron failed — summary.ron may be absent or stale");
        return Ok(OverallStatus::InternalError {
            reason: format!("write_summary_ron failed: {e}"),
            kind: InternalErrorKind::Io,
        });
    }

    Ok(summary.overall_status)
}

// ─── run_shot ────────────────────────────────────────────────────────────────

/// Execute a single [`CaptureShot`]: run the canonical 19-stage pipeline
/// (18 `StageId` variants + terminal `ValidationStage`), bake every truth overlay,
/// write metrics, and (when a GPU is available and a [`BeautySpec`](crate::headless::request::BeautySpec)
/// was supplied) render the beauty shot via [`GpuContext::capture_offscreen_rgba8`].
///
/// Returns an [`anyhow::Error`] on unrecoverable per-shot failures;
/// [`run_request`] classifies those via [`classify_shot_error`] and returns
/// `OverallStatus::InternalError`.
pub fn run_shot(
    shot: &CaptureShot,
    gpu_opt: Option<&GpuContext>,
    layout: &RunLayout,
    gpu_unavailable_reason: &str,
) -> Result<ShotSummary> {
    // ── Setup ────────────────────────────────────────────────────────────────
    layout
        .create_shot_dirs(&shot.id)
        .with_context(|| format!("create_shot_dirs({:?}) failed", shot.id))?;

    // ── Preset ───────────────────────────────────────────────────────────────
    let mut preset = data::presets::load_preset(&shot.preset)
        .map_err(|e| ShotError::PresetNotFound(e.to_string()))?;

    // DD5: fold any per-shot preset overrides on top of the loaded preset
    // before the simulation runs. None is a no-op (v1 forward-compat).
    if let Some(override_spec) = &shot.preset_override {
        override_spec.apply_to(&mut preset);
    }

    // ── Build world + run pipeline ──────────────────────────────────────────
    let resolution = Resolution::new(shot.sim_resolution, shot.sim_resolution);
    let mut world = WorldState::new(Seed(shot.seed), preset.clone(), resolution);

    let pipeline_start = Instant::now();
    sim::default_pipeline()
        .run(&mut world)
        .map_err(|e| ShotError::Pipeline(e.to_string()))?;
    let pipeline_ms = elapsed_ms(pipeline_start);

    // Sprint 4.A: harvest per-stage timings captured by run_from.
    let stage_timings = world.derived.last_stage_timings.take().unwrap_or_default();

    // ── Truth path: overlay bakes ───────────────────────────────────────────
    let registry = OverlayRegistry::sprint_3_defaults();
    let bake_start = Instant::now();
    let mut overlay_hashes: BTreeMap<String, String> = BTreeMap::new();
    for overlay_id in &shot.truth.overlays {
        let Some(desc) = registry.by_id(overlay_id) else {
            bail!("overlay id {overlay_id:?} is not registered in the overlay registry");
        };
        let Some((rgba, w, h)) = bake_overlay_to_rgba8(desc, &world) else {
            bail!(
                "field for overlay {overlay_id:?} was not populated — pipeline must have failed silently"
            );
        };

        let png_path = layout.overlay_png(&shot.id, overlay_id);
        write_rgba8_png(&png_path, &rgba, w, h)
            .with_context(|| format!("write_rgba8_png({png_path:?}) failed"))?;

        let hash = blake3::hash(&rgba).to_hex().to_string();
        overlay_hashes.insert(overlay_id.clone(), hash);
    }

    // ── Truth path: metrics ─────────────────────────────────────────────────
    let metrics_hash = if shot.truth.include_metrics {
        let metrics = SummaryMetrics::compute(&world);
        let canonical = write_metrics_ron(layout, &shot.id, &metrics)
            .with_context(|| format!("write_metrics_ron({:?}) failed", shot.id))?;
        Some(blake3::hash(&canonical).to_hex().to_string())
    } else {
        // No metrics file requested. `None` avoids the false-positive trap of
        // two unrelated shots with include_metrics=false comparing as equal via
        // a shared hash-of-empty-bytes sentinel.
        None
    };
    let bake_ms = elapsed_ms(bake_start);

    let truth = TruthSummary {
        overlay_hashes,
        metrics_hash,
    };

    // ── Beauty path ─────────────────────────────────────────────────────────
    let (beauty, gpu_render_ms) = match (&shot.beauty, gpu_opt) {
        // No BeautySpec on this shot → nothing to do.
        (None, _) => (None, None),

        // BeautySpec present but GPU unavailable → AD8 Skipped.
        (Some(beauty_spec), None) => {
            let summary = BeautySummary {
                camera_preset: beauty_spec.camera_preset.clone(),
                status: BeautyStatus::Skipped {
                    reason: gpu_unavailable_reason.to_owned(),
                },
                byte_hash: None,
            };
            (Some(summary), None)
        }

        // BeautySpec present and GPU available → render.
        (Some(beauty_spec), Some(gpu)) => {
            let gpu_start = Instant::now();
            let view_mode = shot.view_mode.unwrap_or(ViewMode::Continuous);
            let summary = render_beauty_shot(
                gpu,
                &world,
                &preset,
                beauty_spec,
                layout,
                &shot.id,
                view_mode,
            )?;
            (Some(summary), Some(elapsed_ms(gpu_start)))
        }
    };

    Ok(ShotSummary {
        id: shot.id.clone(),
        truth,
        beauty,
        pipeline_ms,
        bake_ms,
        gpu_render_ms,
        stage_timings,
    })
}

// ─── Beauty render ───────────────────────────────────────────────────────────

/// Render one beauty scene into `<run>/shots/<shot_id>/beauty/scene.png`.
///
/// Mirrors the construction order used by [`crate::runtime::Runtime::new`]:
/// the GPU context + the populated world drive a [`render::TerrainRenderer`],
/// a [`render::SkyRenderer`], a [`render::HexSurfaceRenderer`], and a
/// [`render::OverlayRenderer`] (the overlay renderer is only drawn when the
/// caller supplied a non-empty `overlay_stack`). The draw sequence is
/// determined by [`render_stack_for`]`(view_mode)` — the same pure function
/// used by `frame.rs::tick` — ensuring interactive ↔ headless render-path
/// parity. All renderers are dropped at the end of this function so they
/// release their wgpu resources before the next shot bootstraps.
fn render_beauty_shot(
    gpu: &GpuContext,
    world: &WorldState,
    preset: &island_core::preset::IslandArchetypePreset,
    beauty: &crate::headless::request::BeautySpec,
    layout: &RunLayout,
    shot_id: &str,
    view_mode: ViewMode,
) -> Result<BeautySummary> {
    use render::camera::{eye_position, preset_by_name, view_projection};

    // ── Resolve camera preset ───────────────────────────────────────────────
    let camera_preset = preset_by_name(&beauty.camera_preset).ok_or_else(|| {
        ShotError::Other(format!(
            "unknown camera preset '{}' — valid values are 'hero', 'top_debug', 'low_oblique'",
            beauty.camera_preset
        ))
    })?;

    // ── Resolve overlay stack into a local, visibility-adjusted registry ────
    //
    // `BeautySpec.overlay_stack` lists the overlays to composite. Rather than
    // mutate the shared registry, clone the defaults and flip visibility so
    // only the requested overlays draw. Unknown ids bail — better to surface a
    // user typo here than to silently produce a beauty PNG with a missing
    // overlay.
    let mut registry = OverlayRegistry::sprint_3_defaults();
    // Snapshot the ids of entries that start visible, then clear them via
    // `set_visibility`. Collect first so the immutable borrow in `all()` ends
    // before the mutable `set_visibility` call.
    let visible_ids: Vec<&'static str> = registry
        .all()
        .iter()
        .filter(|e| e.visible)
        .map(|e| e.id)
        .collect();
    for id in visible_ids {
        registry.set_visibility(id, false);
    }
    for id in &beauty.overlay_stack {
        if registry.by_id(id).is_none() {
            bail!(
                "beauty overlay_stack references unknown overlay id {id:?}; it is not in the overlay registry"
            );
        }
        registry.set_visibility(id, true);
    }

    // ── Construct renderers ─────────────────────────────────────────────────
    //
    // TerrainRenderer::new picks up `gpu.surface_format`, which on a headless
    // context is `HEADLESS_COLOR_FORMAT = Rgba8Unorm` — so all four pipelines
    // target `Rgba8Unorm` automatically. No fork between windowed / headless.
    //
    // Headless always uses DEFAULT_WORLD_XZ_EXTENT so baselines remain
    // truth-identical regardless of any interactive aspect-ComboBox state.
    let terrain = render::TerrainRenderer::new(gpu, world, preset, render::DEFAULT_WORLD_XZ_EXTENT);
    let overlay_renderer = render::OverlayRenderer::new(
        gpu,
        world,
        &registry,
        terrain.view_buf(),
        terrain.terrain_vbo(),
        terrain.terrain_ibo(),
        terrain.terrain_index_count(),
    );
    let sky = render::SkyRenderer::new(gpu);

    // ── Hex-surface renderer (c9) ─────────────────────────────────────────────
    // Constructed with the same format/depth targets as terrain so both can
    // share the same render pass. Instance buffer is populated from the
    // freshly-run pipeline; 0 instances → draw is a no-op for Continuous shots.
    let mut hex_surface =
        render::HexSurfaceRenderer::new(&gpu.device, gpu.surface_format, gpu.depth_format);
    let hex_instances = crate::runtime::build_hex_instances(world, render::DEFAULT_WORLD_XZ_EXTENT);
    hex_surface.upload_instances(&gpu.device, &gpu.queue, &hex_instances);

    // ── Hex-river renderer (Sprint 3.5.B c4) ─────────────────────────────────
    // Drawn after the hex-surface fill pass (RenderLayer::HexRiver position in
    // the stack) so rivers read over the fill colour. Continuous shots have
    // 0 instances (river layer not in Continuous stack); draw is a no-op.
    let mut hex_river =
        render::HexRiverRenderer::new(&gpu.device, gpu.surface_format, gpu.depth_format);
    let river_instances =
        crate::runtime::build_hex_river_instances(world, render::DEFAULT_WORLD_XZ_EXTENT);
    hex_river.upload_instances(&gpu.device, &gpu.queue, &river_instances);

    // ── Upload camera — headless uses DEFAULT_WORLD_XZ_EXTENT explicitly ──────
    // Baselines were captured at DEFAULT_WORLD_XZ_EXTENT = 5.0. The interactive
    // Runtime may use a different extent (aspect ComboBox), but the headless path
    // has no UI state so it always uses the stable default.
    let (width, height) = beauty.resolution;
    let aspect = width as f32 / height.max(1) as f32;
    let vp = view_projection(
        camera_preset,
        preset.island_radius,
        aspect,
        render::DEFAULT_WORLD_XZ_EXTENT,
    );
    let eye = eye_position(
        camera_preset,
        preset.island_radius,
        render::DEFAULT_WORLD_XZ_EXTENT,
    );
    terrain.update_view(&gpu.queue, vp, eye);

    // ── Hex-surface uniform update ────────────────────────────────────────────
    // Compute world-space hex_size from the sim-space value in hex_grid.
    // If hex_grid is not yet populated, fall back to 1.0; draw is a no-op
    // when instance_count == 0 (Continuous shots with empty hex_instances).
    let scale = hex::geometry::sim_to_world_scale(
        world.resolution.sim_width,
        render::DEFAULT_WORLD_XZ_EXTENT,
    );
    let world_hex_size = world
        .derived
        .hex_grid
        .as_ref()
        .map(|g| g.hex_size * scale)
        .unwrap_or(1.0);
    hex_surface.update_view_projection(&gpu.queue, &vp.to_cols_array_2d(), world_hex_size);
    hex_river.update_view_projection(&gpu.queue, &vp.to_cols_array_2d(), world_hex_size);

    // ── Offscreen capture ───────────────────────────────────────────────────
    let rgba = gpu
        .capture_offscreen_rgba8(beauty.resolution, |color_view, depth_view, encoder| {
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("headless_beauty_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: color_view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        // Clear value is never visible: sky.wgsl paints the full-screen
                        // triangle before anything else on the Rgba8Unorm target. Present
                        // here only so wgpu has a defined LoadOp. NOT a palette value.
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.08,
                            g: 0.08,
                            b: 0.12,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            // Dispatch via render_stack_for — the same pure function used by
            // frame.rs::tick. Exhaustive match (no `_` arm) ensures adding a
            // future RenderLayer variant fails to compile here AND in frame.rs,
            // keeping both call sites in sync (plan §5 tier-1 parity gate).
            for layer in render_stack_for(view_mode) {
                match layer {
                    RenderLayer::Sky => sky.draw(&mut rpass),
                    RenderLayer::Terrain => terrain.draw(&mut rpass),
                    RenderLayer::HexSurface => hex_surface.draw(&mut rpass),
                    RenderLayer::HexRiver => hex_river.draw(&mut rpass),
                    RenderLayer::Overlay => {
                        overlay_renderer.draw(&mut rpass, &registry, &gpu.queue);
                    }
                }
            }
        })
        .map_err(|e| ShotError::GpuRuntime(format!("capture_offscreen_rgba8 failed: {e:#}")))?;

    let png_path = layout.beauty_png(shot_id);
    write_rgba8_png(&png_path, &rgba, width, height)
        .with_context(|| format!("write_rgba8_png({png_path:?}) failed"))?;

    let byte_hash = blake3::hash(&rgba).to_hex().to_string();

    Ok(BeautySummary {
        camera_preset: beauty.camera_preset.clone(),
        status: BeautyStatus::Rendered,
        byte_hash: Some(byte_hash),
    })
}

// ─── Error classification ────────────────────────────────────────────────────

/// Typed per-shot error used so [`run_request`] can pick the right
/// [`InternalErrorKind`] without regex-matching error messages.
///
/// Kept private: callers only see `anyhow::Error` (via `.into()`).
#[derive(Debug, thiserror::Error)]
enum ShotError {
    #[error("preset not found: {0}")]
    PresetNotFound(String),
    #[error("pipeline failed: {0}")]
    Pipeline(String),
    #[error("GPU runtime error on beauty path: {0}")]
    GpuRuntime(String),
    #[error("{0}")]
    Other(String),
}

/// Inspect an error chain returned from [`run_shot`] to pick the matching
/// [`InternalErrorKind`].
///
/// Ordered by specificity: the most concrete variants go first so
/// `ShotError::Other` only matches when no more specific cause is in the
/// `anyhow::Error` chain.
fn classify_shot_error(err: &anyhow::Error) -> InternalErrorKind {
    for cause in err.chain() {
        if let Some(shot_err) = cause.downcast_ref::<ShotError>() {
            return match shot_err {
                ShotError::PresetNotFound(_) => InternalErrorKind::PresetNotFound,
                ShotError::Pipeline(_) => InternalErrorKind::PipelineError,
                ShotError::GpuRuntime(_) => InternalErrorKind::GpuRuntimeError,
                ShotError::Other(_) => InternalErrorKind::Other,
            };
        }
        if cause.downcast_ref::<std::io::Error>().is_some() {
            return InternalErrorKind::Io;
        }
    }
    InternalErrorKind::Other
}

// ─── Misc helpers ────────────────────────────────────────────────────────────

#[inline]
fn elapsed_ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1_000.0
}

// Re-export for test assertions — keeps the tests from reaching into crate
// internals.
#[cfg(test)]
pub(crate) fn force_fail_env_name() -> &'static str {
    FORCE_GPU_FAIL_ENV
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::headless::request::{BeautySpec, CaptureRequest, CaptureShot, TruthSpec};
    use std::path::Path;
    use std::sync::{Mutex, MutexGuard};

    // ── helpers ──────────────────────────────────────────────────────────────

    /// Global lock taken by every test that runs the heavy
    /// [`run_request`] path. Reasons it's load-bearing:
    ///
    /// 1. The AD8 skip test toggles `IPG_FORCE_HEADLESS_GPU_FAIL` on the
    ///    process env, which is shared across all threads cargo's parallel
    ///    test harness spawns — without serialising, a sibling test can read
    ///    the var while we're in the middle of the "force fail" block and
    ///    observe the wrong state.
    /// 2. Each run bootstraps its own `GpuContext` (wgpu instance, adapter,
    ///    device). Running two concurrently stresses driver state on macOS
    ///    Metal and has occasionally produced flaky failures in local CI.
    ///
    /// The ignore gate on the heavy tests already limits this to manual
    /// `cargo test -- --ignored` invocations, so the serialisation cost is
    /// irrelevant for the hot path.
    fn pipeline_test_lock() -> MutexGuard<'static, ()> {
        static LOCK: Mutex<()> = Mutex::new(());
        // `lock().unwrap_or_else(..)` recovers from a poisoned mutex so one
        // panicking test doesn't lock out the others on subsequent runs.
        LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn write_request(dir: &Path, req: &CaptureRequest) -> PathBuf {
        let path = dir.join("request.ron");
        let text = ron::ser::to_string_pretty(req, ron::ser::PrettyConfig::default())
            .expect("serialize request");
        std::fs::write(&path, text).expect("write request.ron");
        path
    }

    fn minimal_request(dir: &Path) -> CaptureRequest {
        CaptureRequest {
            schema_version: 1,
            run_id: Some("test_run".into()),
            output_dir: Some(dir.join("run")),
            shots: vec![
                CaptureShot {
                    id: "truth_only".into(),
                    seed: 42,
                    preset: "volcanic_single".into(),
                    sim_resolution: 64,
                    truth: TruthSpec {
                        overlays: vec!["final_elevation".into(), "river_network".into()],
                        include_metrics: true,
                    },
                    beauty: None,
                    preset_override: None,
                    view_mode: None,
                },
                CaptureShot {
                    id: "with_beauty".into(),
                    seed: 42,
                    preset: "volcanic_single".into(),
                    sim_resolution: 64,
                    truth: TruthSpec {
                        overlays: vec!["slope".into()],
                        include_metrics: true,
                    },
                    beauty: Some(BeautySpec {
                        camera_preset: "hero".into(),
                        overlay_stack: vec![],
                        resolution: (320, 200),
                    }),
                    preset_override: None,
                    view_mode: None,
                },
            ],
        }
    }

    // ── Fast (non-`#[ignore]`) tests: argv / parse paths ─────────────────────

    #[test]
    fn run_missing_file_returns_internal_error_io() {
        // Use a clearly-unlikely path; run_request must not panic.
        let status = run_request(Path::new("/definitely/nonexistent/path.ron"))
            .expect("run_request must not return Err on IO failure");
        match status {
            OverallStatus::InternalError {
                kind: InternalErrorKind::Io,
                ..
            } => {}
            other => panic!("expected InternalError(Io), got {other:?}"),
        }
    }

    #[test]
    fn run_invalid_ron_returns_internal_error_ronparse() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("bad.ron");
        std::fs::write(&path, "this is not valid RON {{{").expect("write garbage");
        let status =
            run_request(&path).expect("run_request must not return Err on RON parse failure");
        match status {
            OverallStatus::InternalError {
                kind: InternalErrorKind::RonParse,
                ..
            } => {}
            other => panic!("expected InternalError(RonParse), got {other:?}"),
        }
    }

    #[test]
    fn force_fail_env_name_matches_constant() {
        // Guard: tests below poke this env var — locking the name here
        // catches accidental renames.
        assert_eq!(force_fail_env_name(), "IPG_FORCE_HEADLESS_GPU_FAIL");
    }

    /// A `bogus_preset` name fails preset-load before the pipeline runs — this
    /// is a CPU-only code path, no GPU required.  The test verifies that
    /// `summary.ron` is written even when a mid-shot `InternalError` fires.
    #[test]
    fn run_request_writes_summary_on_mid_shot_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        let run_dir = dir.path().join("run");
        let req = CaptureRequest {
            schema_version: 1,
            run_id: Some("mid_shot_error_test".into()),
            output_dir: Some(run_dir.clone()),
            shots: vec![CaptureShot {
                id: "bad_shot".into(),
                seed: 1,
                preset: "bogus_preset_that_does_not_exist".into(),
                sim_resolution: 64,
                truth: TruthSpec {
                    overlays: vec![],
                    include_metrics: false,
                },
                beauty: None,
                preset_override: None,
                view_mode: None,
            }],
        };
        let req_path = write_request(dir.path(), &req);

        let status = run_request(&req_path).expect("run_request must not return Err");

        // The outcome must be an InternalError with kind PresetNotFound.
        match &status {
            OverallStatus::InternalError {
                kind: InternalErrorKind::PresetNotFound,
                ..
            } => {}
            other => panic!("expected InternalError(PresetNotFound), got {other:?}"),
        }

        // summary.ron must have been written despite the error.
        let summary_path = run_dir.join("summary.ron");
        assert!(
            summary_path.exists(),
            "summary.ron must exist even on mid-shot InternalError"
        );

        // The written summary must parse cleanly.
        let raw = std::fs::read_to_string(&summary_path).expect("read summary.ron");
        let summary: RunSummary = ron::de::from_str(&raw).expect("summary.ron must round-trip");

        // overall_status in the file must match what run_request returned.
        assert_eq!(
            summary.overall_status, status,
            "summary.ron overall_status must match the returned status"
        );
        match summary.overall_status {
            OverallStatus::InternalError {
                kind: InternalErrorKind::PresetNotFound,
                ..
            } => {}
            other => panic!(
                "summary.ron overall_status must be InternalError(PresetNotFound), got {other:?}"
            ),
        }
    }

    // ── Slow (`#[ignore]`) tests: actually run the pipeline ──────────────────

    #[test]
    #[ignore = "runs the full Sprint 1B pipeline + GPU; macOS Metal baseline host only (AD10)"]
    fn run_request_end_to_end_produces_non_empty_artifact_dir() {
        let _lock = pipeline_test_lock();
        let dir = tempfile::tempdir().expect("tempdir");
        let req = minimal_request(dir.path());
        let req_path = write_request(dir.path(), &req);

        // Ensure the deliberate-skip env var is OFF so we exercise the real GPU path.
        // SAFETY: the `pipeline_test_lock` above serialises all tests that touch
        // this env var, so the read is race-free for the duration of this guard.
        unsafe {
            std::env::remove_var(FORCE_GPU_FAIL_ENV);
        }

        let status = run_request(&req_path).expect("run_request must not Err");
        assert!(
            matches!(status, OverallStatus::Passed),
            "expected Passed, got {status:?}"
        );

        let run_dir = req.output_dir.as_ref().unwrap();
        assert!(
            run_dir.join("summary.ron").exists(),
            "summary.ron must exist"
        );
        assert!(
            run_dir.join("request.ron").exists(),
            "request.ron must exist"
        );
        assert!(
            run_dir
                .join("shots/truth_only/overlays/final_elevation.png")
                .exists(),
            "truth overlay PNG must exist"
        );
        assert!(
            run_dir.join("shots/truth_only/metrics.ron").exists(),
            "metrics.ron must exist"
        );
        assert!(
            run_dir.join("shots/with_beauty/beauty/scene.png").exists(),
            "beauty scene PNG must exist"
        );

        // Parse the summary to make sure it round-trips.
        let raw = std::fs::read_to_string(run_dir.join("summary.ron")).unwrap();
        let summary: RunSummary = ron::de::from_str(&raw).unwrap();
        assert_eq!(summary.shots.len(), 2);
    }

    #[test]
    #[ignore = "runs the full Sprint 1B pipeline + GPU; macOS Metal baseline host only (AD10)"]
    fn run_request_determinism_same_inputs_same_summary_hashes() {
        let _lock = pipeline_test_lock();
        let dir = tempfile::tempdir().expect("tempdir");
        let mut req = minimal_request(dir.path());

        // Two different output dirs so the second run doesn't clobber the first.
        let run1 = dir.path().join("run1");
        let run2 = dir.path().join("run2");

        req.output_dir = Some(run1.clone());
        let req_path1 = write_request(dir.path(), &req);

        // Disable the force-fail hook so we actually run beauty paths.
        unsafe {
            std::env::remove_var(FORCE_GPU_FAIL_ENV);
        }

        let status1 = run_request(&req_path1).expect("first run");
        assert!(matches!(status1, OverallStatus::Passed));

        // Second run → different dir, same logical request.
        req.output_dir = Some(run2.clone());
        let req_path2 = write_request(dir.path(), &req);
        let status2 = run_request(&req_path2).expect("second run");
        assert!(matches!(status2, OverallStatus::Passed));

        let s1: RunSummary =
            ron::de::from_str(&std::fs::read_to_string(run1.join("summary.ron")).unwrap()).unwrap();
        let s2: RunSummary =
            ron::de::from_str(&std::fs::read_to_string(run2.join("summary.ron")).unwrap()).unwrap();

        assert_eq!(s1.run_id, s2.run_id, "run_id must be bit-exact");
        assert_eq!(
            s1.request_fingerprint, s2.request_fingerprint,
            "request_fingerprint must be bit-exact"
        );
        assert_eq!(s1.shots.len(), s2.shots.len());
        for (a, b) in s1.shots.iter().zip(s2.shots.iter()) {
            assert_eq!(a.id, b.id);
            assert_eq!(
                a.truth.overlay_hashes, b.truth.overlay_hashes,
                "overlay_hashes must be bit-exact"
            );
            assert_eq!(
                a.truth.metrics_hash, b.truth.metrics_hash,
                "metrics_hash must be bit-exact"
            );
            // Same host + binary + inputs → byte-exact beauty render.
            // AD7 lists shots[*].beauty.byte_hash among bit-exact fields.
            if let (Some(b1), Some(b2)) = (a.beauty.as_ref(), b.beauty.as_ref()) {
                assert_eq!(
                    b1.byte_hash, b2.byte_hash,
                    "beauty byte_hash must be bit-exact across runs on same host"
                );
            }
        }
    }

    #[test]
    #[ignore = "runs the full Sprint 1B pipeline; IPG_FORCE_HEADLESS_GPU_FAIL exercises the AD8 skip branch"]
    fn run_request_skipped_beauty_when_gpu_unavailable() {
        let _lock = pipeline_test_lock();
        let dir = tempfile::tempdir().expect("tempdir");
        let req = minimal_request(dir.path());
        let req_path = write_request(dir.path(), &req);

        // Force-fail the GPU bootstrap for this test only.
        // SAFETY: serialised by `pipeline_test_lock` above.
        unsafe {
            std::env::set_var(FORCE_GPU_FAIL_ENV, "1");
        }

        let status = run_request(&req_path).expect("run_request must not Err");

        // Clean up before any assertion so a panic doesn't poison other tests.
        unsafe {
            std::env::remove_var(FORCE_GPU_FAIL_ENV);
        }

        match status {
            OverallStatus::PassedWithBeautySkipped {
                skipped_shot_ids,
                reason,
            } => {
                assert_eq!(skipped_shot_ids, vec!["with_beauty".to_owned()]);
                assert!(
                    reason.contains(FORCE_GPU_FAIL_ENV),
                    "reason should mention the env var that forced the skip, got {reason:?}"
                );
            }
            other => panic!("expected PassedWithBeautySkipped, got {other:?}"),
        }

        // Truth path must still have run completely.
        let run_dir = req.output_dir.as_ref().unwrap();
        assert!(
            run_dir
                .join("shots/truth_only/overlays/final_elevation.png")
                .exists(),
            "truth overlay PNG must still be written"
        );
        assert!(
            run_dir.join("shots/truth_only/metrics.ron").exists(),
            "metrics.ron must still be written"
        );
        // Beauty PNG must NOT exist when status is Skipped.
        assert!(
            !run_dir.join("shots/with_beauty/beauty/scene.png").exists(),
            "beauty PNG must not be written when GPU was Skipped"
        );
    }

    /// Sprint 4.A DD2: the v4 binary always writes `schema_version: 4`
    /// regardless of the input request's schema version. This supersedes the
    /// Sprint 3.5 "mirror-the-request" policy.
    ///
    /// For each of the 3 previously-shipped schema versions (1, 2, 3), this
    /// test writes a minimal request, runs the headless executor with
    /// GPU bootstrap force-failed (truth path still runs; beauty skipped),
    /// reads the generated `summary.ron`, and asserts
    /// `summary.schema_version >= input.schema_version` (the upgrade direction
    /// is always forward — never backward).
    ///
    /// Also asserts that the output is exactly 4 (the current binary's version)
    /// regardless of the input schema.
    #[test]
    fn run_summary_always_stamps_schema_version_4_for_v4_binary() {
        let _lock = pipeline_test_lock();

        for input_schema_version in [1_u32, 2_u32, 3_u32] {
            let dir = tempfile::tempdir().expect("tempdir");

            // Build a minimal truth-only request (no beauty → no GPU need).
            let req = CaptureRequest {
                schema_version: input_schema_version,
                run_id: Some(format!("schema_v{input_schema_version}_v4binary_test")),
                output_dir: Some(dir.path().join("run")),
                shots: vec![CaptureShot {
                    id: "truth_only".into(),
                    seed: 42,
                    preset: "volcanic_single".into(),
                    sim_resolution: 64,
                    truth: TruthSpec {
                        overlays: vec!["final_elevation".into()],
                        include_metrics: true,
                    },
                    beauty: None,
                    preset_override: None,
                    view_mode: None,
                }],
            };
            let req_path = write_request(dir.path(), &req);

            // Force-skip GPU so beauty doesn't gate the test on adapter
            // availability; truth path runs to completion.
            // SAFETY: serialised by `pipeline_test_lock` above.
            unsafe {
                std::env::set_var(FORCE_GPU_FAIL_ENV, "1");
            }

            let run_status = run_request(&req_path);

            unsafe {
                std::env::remove_var(FORCE_GPU_FAIL_ENV);
            }

            let status = run_status.expect("run_request must not Err");
            match status {
                OverallStatus::Passed | OverallStatus::PassedWithBeautySkipped { .. } => {}
                other => panic!(
                    "input_schema_version {input_schema_version}: expected Passed/PassedWithBeautySkipped, got {other:?}"
                ),
            }

            // Read the written summary.ron and confirm the stamped version.
            let summary_path = req.output_dir.as_ref().unwrap().join("summary.ron");
            let summary_text =
                std::fs::read_to_string(&summary_path).expect("summary.ron must exist");
            let summary: RunSummary =
                ron::de::from_str(&summary_text).expect("summary.ron must parse");

            // Sprint 4.A DD2: binary always stamps v4, not the input version.
            assert_eq!(
                summary.schema_version, 4,
                "input_schema_version {input_schema_version}: v4 binary must stamp 4, got {}",
                summary.schema_version
            );
            // Upgrade direction is always forward.
            assert!(
                summary.schema_version >= input_schema_version,
                "summary schema_version {} must be >= input {}",
                summary.schema_version,
                input_schema_version
            );
        }
    }
}
