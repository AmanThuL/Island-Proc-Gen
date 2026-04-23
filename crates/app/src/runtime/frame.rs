use std::time::Instant;

use tracing::warn;
use winit::dpi::PhysicalSize;

use sim::StageId;
use sim::invalidate_from;

use super::Runtime;
use super::tabs::AppTabViewer;
use super::view_mode::{RenderLayer, ViewMode, render_stack_for};

impl Runtime {
    /// Refresh the overlay texture bakes + rebuild the hex-surface instance
    /// buffer after a slider re-run or regen. Shared by the three slider
    /// branches (`wind_dir_changed`, `erosion`/`space`, `climate`) so
    /// derived-view updates land in lockstep instead of drifting across
    /// three copy-paste call sites. Future slider additions also hook here.
    fn refresh_derived_views(&mut self) {
        self.overlay
            .refresh(&self.gpu, &self.world, &self.overlay_registry);
        self.rebuild_hex_surface_instances();
        self.rebuild_hex_river_instances();
    }

    pub(super) fn tick(&mut self) {
        // FPS (exponential moving average)
        let now = Instant::now();
        let dt = now.duration_since(self.last_frame).as_secs_f32();
        self.last_frame = now;
        self.fps = if self.fps == 0.0 {
            1.0 / dt.max(f32::EPSILON)
        } else {
            self.fps * 0.95 + (1.0 / dt.max(f32::EPSILON)) * 0.05
        };

        // ── Camera aspect from viewport rect ──────────────────────────────────
        // We use the rect written by AppTabViewer::ui in the PREVIOUS frame.
        // This produces at most one frame of stale aspect during window
        // resize, which is imperceptible at 60 fps. On the very first frame
        // viewport_rect is None, so the initial aspect set in Runtime::new
        // (from gpu.size) continues to apply.
        if let Some(rect) = self.viewport_rect {
            let aspect = rect.aspect_ratio().max(1e-3);
            self.camera.set_aspect(aspect);
        }

        // ── Sim step (Sprint 0: no-op) ────────────────────────────────────────

        // ── Upload camera ─────────────────────────────────────────────────────
        let vp = self.camera.view_projection();
        let eye = self.camera.eye();
        self.terrain.update_view(&self.gpu.queue, vp, eye);

        // ── Hex-surface uniform update (per-frame, Fix E) ─────────────────────
        // Compute the world-space hex_size from the sim-space value in hex_grid.
        // If hex_grid is not yet populated, fall back to 1.0 (the renderer's
        // initial default); the 0-instance draw in HexSurface is a no-op.
        let world_hex_size = self
            .world
            .derived
            .hex_grid
            .as_ref()
            .map(|g| {
                g.hex_size
                    * hex::geometry::sim_to_world_scale(
                        self.world.resolution.sim_width,
                        self.world_xz_extent,
                    )
            })
            .unwrap_or(1.0);
        self.hex_surface.update_view_projection(
            &self.gpu.queue,
            &vp.to_cols_array_2d(),
            world_hex_size,
        );

        // ── Hex-river uniform update (per-frame) ──────────────────────────────
        // Uses the same world_hex_size as the hex-surface so rivers scale with
        // the grid. River colour is re-uploaded from palette::RIVER each frame.
        self.hex_river.update_view_projection(
            &self.gpu.queue,
            &vp.to_cols_array_2d(),
            world_hex_size,
        );

        // ── Acquire surface ───────────────────────────────────────────────────
        //
        // `surface_expect` panics with a descriptive message if we ever end
        // up here with a headless (surface-less) `GpuContext`. Runtime is
        // the interactive path by construction, so this assumption holds;
        // the panic would flag a programming error (e.g. a future refactor
        // that accidentally plumbs a headless ctx through the window event
        // loop) rather than a runtime concern.
        let surface = self.gpu.surface_expect();
        let frame = match surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f) => f,
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => return,
            wgpu::CurrentSurfaceTexture::Outdated
            | wgpu::CurrentSurfaceTexture::Suboptimal(_)
            | wgpu::CurrentSurfaceTexture::Lost => {
                surface.configure(&self.gpu.device, &self.gpu.config);
                return;
            }
            wgpu::CurrentSurfaceTexture::Validation => return,
        };

        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame_encoder"),
            });

        // ── 3D scene pass — renders into the offscreen viewport texture ───────
        // Colour target is viewport_tex.color_view() (not the window surface).
        // egui composites the result via egui::Image in the CentralPanel below.
        // The clear colour is the fallback background visible under the sky
        // renderer when sky doesn't cover the full quad.
        //
        // The render layer sequence is determined by `render_stack_for(view_mode)`
        // (Sprint 3.5.A c8) so the same pure function controls both the interactive
        // and the headless executor paths (plan §5 tier-1 parity gate).
        {
            let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("scene_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: self.viewport_tex.color_view(),
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
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
                    view: self.viewport_tex.depth_view(),
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
            for layer in render_stack_for(self.view_mode) {
                match layer {
                    RenderLayer::Sky => self.sky.draw(&mut rpass),
                    RenderLayer::Terrain => self.terrain.draw(&mut rpass),
                    RenderLayer::HexSurface => self.hex_surface.draw(&mut rpass),
                    RenderLayer::HexRiver => self.hex_river.draw(&mut rpass),
                    RenderLayer::Overlay => {
                        self.overlay
                            .draw(&mut rpass, &self.overlay_registry, &self.gpu.queue);
                    }
                }
            }
        }

        // ── egui pass ─────────────────────────────────────────────────────────
        // Extract values before the mutable borrows that follow.
        let fps = self.fps;
        let resolution = self.resolution;
        let seed = self.seed;
        let island_radius = self.preset.island_radius;
        let world_xz_extent = self.world_xz_extent;
        let registry = &mut self.overlay_registry;
        let camera = &mut self.camera;
        let preset = &mut self.preset;
        let view_mode = self.view_mode;

        let raw_input = self.egui_state.take_egui_input(&self.window);

        // Use begin_pass / end_pass (the non-deprecated path in egui 0.34).
        self.egui_ctx.begin_pass(raw_input);

        // ── DockArea — six-tab dock layout replacing the old CentralPanel +
        //    four floating windows. TabViewer dispatches each tab variant to
        //    its panel body; the Viewport tab renders the offscreen texture.
        //
        // Extract local temporaries that the TabViewer needs but that would
        // otherwise conflict with the `&mut self.dock.state` borrow below.
        let viewport_tex_id = self.viewport_tex.egui_texture_id();
        let mut new_view_mode: Option<ViewMode> = None;
        let mut params_result = ui::ParamsPanelResult::default();
        let mut world_event = crate::world_panel::WorldPanelEvent::default();
        let stats_data = ui::StatsPanelData {
            fps,
            resolution,
            seed,
        };

        // Reborrow self.dock.state and viewport_rect as locals so the closure
        // below doesn't need to borrow self while self.egui_ctx is also
        // borrowed by show().
        let dock_state = &mut self.dock.state;
        let viewport_rect = &mut self.viewport_rect;
        let world_panel = &mut self.world_panel;
        {
            #[allow(deprecated)]
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE)
                .show(&self.egui_ctx, |ui| {
                    let mut viewer = AppTabViewer {
                        viewport_tex_id,
                        overlay_registry: registry,
                        camera,
                        island_radius,
                        world_xz_extent,
                        view_mode,
                        new_view_mode: &mut new_view_mode,
                        preset,
                        params_result: &mut params_result,
                        stats_data: &stats_data,
                        viewport_rect,
                        world_panel,
                        world_event: &mut world_event,
                    };
                    egui_dock::DockArea::new(dock_state).show_inside(ui, &mut viewer);
                });
        }

        let full_output = self.egui_ctx.end_pass();

        // Apply ViewMode transition if the user changed it in the panel.
        if let Some(mode) = new_view_mode {
            self.set_view_mode(mode);
        }

        // Slider re-run: when a Sprint 1B climate slider is touched,
        // update the world's preset copy and re-run the affected stage
        // plus its downstream neighbours via `run_from`, then re-bake
        // the overlay textures so visible overlays reflect the new
        // fields on the very next draw.
        if params_result.wind_dir_changed {
            self.world.preset = self.preset.clone();
            if let Err(err) = self
                .pipeline
                .run_from(&mut self.world, StageId::Precipitation as usize)
            {
                warn!("slider re-run failed: {err}");
            } else {
                self.refresh_derived_views();
            }
        }

        // Sprint 2.7 / 3.8: erosion + SPACE-lite slider re-run protocol (CLAUDE.md Gotchas):
        //   1. Sync world.preset from self.preset so stages read the new values.
        //   2. Invalidate caches from the ErosionOuterLoop frontier.
        //   3. Re-run from ErosionOuterLoop (includes CoastType + all 1B stages).
        // space_changed shares the same frontier as erosion_changed (ErosionOuterLoop),
        // so both flags drive the identical body — a single combined check avoids a
        // double invalidate + double run_from on frames where both fire together.
        if params_result.erosion_changed || params_result.space_changed {
            self.world.preset = self.preset.clone();
            invalidate_from(&mut self.world, StageId::ErosionOuterLoop);
            if let Err(err) = self
                .pipeline
                .run_from(&mut self.world, StageId::ErosionOuterLoop as usize)
            {
                warn!("erosion/space slider re-run failed: {err}");
            } else {
                self.refresh_derived_views();
            }
        }

        // Sprint 3.8: LFPM climate slider re-run (q_0 / tau_c / tau_f).
        // Frontier: Precipitation — same as the Sprint 1B wind-dir slider.
        if params_result.climate_changed {
            self.world.preset = self.preset.clone();
            if let Err(err) = self
                .pipeline
                .run_from(&mut self.world, StageId::Precipitation as usize)
            {
                warn!("climate slider re-run failed: {err}");
            } else {
                self.refresh_derived_views();
            }
        }

        // Sprint 2.6.C: World panel events — full rebuild or sea_level fast path.
        if world_event.regenerate {
            if let Err(err) = self.regenerate_from_world_panel() {
                warn!("regenerate failed: {err}");
            }
        }
        if world_event.sea_level_released {
            if let Err(err) = self.apply_sea_level_fast_path() {
                warn!("sea_level fast-path failed: {err}");
            }
        }
        // Sprint 2.6.A: World aspect ComboBox — render-only rebuild, no sim re-run.
        if let Some(new_extent) = world_event.aspect_extent_changed {
            self.apply_world_aspect(new_extent);
        }

        // Handle egui platform output (cursor changes, clipboard, etc.)
        self.egui_state
            .handle_platform_output(&self.window, full_output.platform_output);

        let PhysicalSize { width, height } = self.gpu.size;
        let screen_descriptor = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [width, height],
            pixels_per_point: egui_winit::pixels_per_point(&self.egui_ctx, &self.window),
        };

        let paint_jobs = self
            .egui_ctx
            .tessellate(full_output.shapes, full_output.pixels_per_point);

        // Upload textures (fonts etc.) and update vertex/index buffers
        for (id, delta) in &full_output.textures_delta.set {
            self.egui_renderer
                .update_texture(&self.gpu.device, &self.gpu.queue, *id, delta);
        }
        let _extra_cmd_bufs = self.egui_renderer.update_buffers(
            &self.gpu.device,
            &self.gpu.queue,
            &mut encoder,
            &paint_jobs,
            &screen_descriptor,
        );

        {
            let mut rpass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("egui_pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations {
                            // B.1: terrain now renders into the offscreen
                            // viewport texture; the egui pass composites that
                            // image onto the window surface via the CentralPanel
                            // Image widget. Clear to dark slate so stale pixels
                            // are never visible at window edges or between frames.
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: 0.08,
                                g: 0.08,
                                b: 0.12,
                                a: 1.0,
                            }),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                })
                .forget_lifetime(); // egui_wgpu::Renderer::render takes &mut RenderPass<'static>

            self.egui_renderer
                .render(&mut rpass, &paint_jobs, &screen_descriptor);
        }

        // Free textures scheduled for removal
        for id in &full_output.textures_delta.free {
            self.egui_renderer.free_texture(id);
        }

        // ── Submit ────────────────────────────────────────────────────────────
        self.gpu.queue.submit(std::iter::once(encoder.finish()));
        self.window.pre_present_notify();
        frame.present();
    }
}
