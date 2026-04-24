use winit::{
    event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::ActiveEventLoop,
};

use tracing::debug;

use super::Runtime;

/// Maximum accumulated cursor displacement (in physical pixels) between a
/// left-button press and release that still qualifies as a "click" rather
/// than a drag. 3 px matches the standard UI click tolerance.
pub(crate) const CLICK_DRAG_THRESHOLD_PHYS_PX: f64 = 3.0;

impl Runtime {
    /// Handle a `WindowEvent` from winit.
    pub fn handle_window_event(&mut self, event_loop: &ActiveEventLoop, event: WindowEvent) {
        // Forward to egui first so it can update its internal state (records
        // the click, hover, drag, keyboard input, etc.). The `consumed` flag
        // below only affects whether we skip OUR handler — egui has already
        // recorded what it needs via this call.
        let response = self.egui_state.on_window_event(&self.window, &event);

        // egui claims `consumed = true` for pointer events whenever the
        // cursor is over ANY egui widget — including the viewport `Image`
        // and the dock chrome. Returning early on `consumed` for mouse
        // events would therefore swallow every drag the user starts inside
        // the Viewport tab. The viewport-rect gate inside each mouse arm
        // already routes correctly (no camera drive when the cursor is over
        // a panel), so we only honour `consumed` for non-mouse events like
        // keyboard input into a future text field.
        let is_mouse_event = matches!(
            event,
            WindowEvent::CursorMoved { .. }
                | WindowEvent::MouseInput { .. }
                | WindowEvent::MouseWheel { .. }
        );
        if response.consumed && !is_mouse_event {
            return;
        }

        let ppp = egui_winit::pixels_per_point(&self.egui_ctx, &self.window);

        match event {
            WindowEvent::CloseRequested => {
                debug!("Window close requested");
                // Persist the dock layout before exiting so the user's panel
                // arrangement is restored on the next launch.
                if let Err(err) = self.dock.save(&self.dock_layout_path) {
                    tracing::warn!(
                        path = %self.dock_layout_path.display(),
                        error = %err,
                        "dock_layout.ron save failed"
                    );
                }
                event_loop.exit();
            }

            WindowEvent::Resized(new_size) => {
                self.gpu.resize(new_size);
                // Camera aspect is now set per-frame from viewport_rect in
                // tick() (using the previous frame's tab rect). We no longer
                // drive it directly from window size so that the aspect always
                // matches the actual 3-D viewport area, not the full window.
                self.viewport_tex.resize(
                    &self.gpu,
                    (new_size.width, new_size.height),
                    &mut self.egui_renderer,
                );
            }

            WindowEvent::RedrawRequested => {
                self.tick();
            }

            // ── Camera input ──────────────────────────────────────────────────
            WindowEvent::MouseInput { state, button, .. } => {
                // Gate press events on cursor being inside the viewport tab.
                // Release events always pass through so we never leak
                // left_pressed / right_pressed = true state.
                let is_press = state == ElementState::Pressed;
                if is_press {
                    let inside = self
                        .input
                        .last_cursor
                        .zip(self.viewport_rect)
                        .map(|((cx, cy), rect)| cursor_in_rect_physical((cx, cy), rect, ppp))
                        .unwrap_or(false);
                    if !inside {
                        return;
                    }
                }
                match button {
                    MouseButton::Left => {
                        if is_press {
                            self.input.left_pressed = true;
                            // Record cursor position at press time for
                            // click-vs-drag discrimination on release.
                            self.input.left_press_cursor = self.input.last_cursor;
                        } else {
                            self.input.left_pressed = false;
                            // Capture the last known cursor before clearing it,
                            // because MouseInput::Released does not carry a
                            // cursor position in winit.
                            let release_cursor = self.input.last_cursor;
                            self.input.last_cursor = None;

                            // ── Click-vs-drag discrimination ──────────────────
                            // If the cursor barely moved between press and
                            // release, treat it as a hex-pick click.
                            if let (Some(press), Some(release)) =
                                (self.input.left_press_cursor, release_cursor)
                            {
                                let dx = release.0 - press.0;
                                let dy = release.1 - press.1;
                                let moved = dx.abs() + dy.abs();
                                let is_click = moved < CLICK_DRAG_THRESHOLD_PHYS_PX;

                                if is_click {
                                    if let (Some(rect), Some(hex_grid)) =
                                        (self.viewport_rect, self.world.derived.hex_grid.as_ref())
                                    {
                                        // DD7: "Off-grid clicks → no-op". A miss
                                        // (sky / parallel ray / outside grid)
                                        // leaves `picked_hex` unchanged; only a
                                        // real hit updates the selection.
                                        if let Some(picked) = screen_to_picked_hex(
                                            release,
                                            rect,
                                            ppp,
                                            self.camera.view_projection(),
                                            self.camera.eye(),
                                            self.preset.sea_level,
                                            self.world_xz_extent,
                                            self.resolution.sim_width,
                                            hex_grid,
                                        ) {
                                            debug!(
                                                picked_hex = ?picked,
                                                cursor = ?release,
                                                "hex pick"
                                            );
                                            self.picked_hex = Some(picked);
                                        }
                                    }
                                }
                            }
                            self.input.left_press_cursor = None;
                        }
                    }
                    MouseButton::Right => {
                        self.input.right_pressed = is_press;
                        if !is_press {
                            self.input.last_cursor = None;
                        }
                    }
                    _ => {}
                }
            }

            WindowEvent::CursorMoved { position, .. } => {
                let (cx, cy) = (position.x, position.y);

                // Always update last_cursor first so the next move after
                // re-entering the viewport computes a fresh delta rather than
                // teleporting from the stale pre-exit position.
                let prev_cursor = self.input.last_cursor;
                self.input.last_cursor = Some((cx, cy));

                // Only drive the camera when the cursor is inside the
                // viewport tab rect. The delta denominator is the viewport's
                // physical-pixel size so sensitivity is independent of how
                // large or small the user has resized the tab.
                let in_viewport = self
                    .viewport_rect
                    .map(|rect| cursor_in_rect_physical((cx, cy), rect, ppp))
                    .unwrap_or(false);

                if !in_viewport {
                    return;
                }

                if let Some((lx, ly)) = prev_cursor {
                    // Compute deltas as a fraction of the viewport's physical
                    // size. Using physical pixels for both cursor deltas and
                    // the viewport dimensions keeps the math consistent without
                    // any intermediate conversion.
                    // viewport_rect is guaranteed Some here — in_viewport checked above.
                    let rect = self.viewport_rect.unwrap();
                    let rect_w_phys = (rect.width() * ppp).max(1.0);
                    let rect_h_phys = (rect.height() * ppp).max(1.0);
                    let dx = (cx - lx) as f32 / rect_w_phys;
                    let dy = (cy - ly) as f32 / rect_h_phys;

                    if self.input.right_pressed
                        || (self.input.left_pressed && self.input.shift_held)
                    {
                        self.camera.pan(dx, dy);
                    } else if self.input.left_pressed {
                        self.camera.orbit(dx, dy);
                    }
                }
            }

            WindowEvent::MouseWheel { delta, .. } => {
                // Gate scroll on cursor being inside the viewport tab.
                let in_viewport = self
                    .input
                    .last_cursor
                    .zip(self.viewport_rect)
                    .map(|((cx, cy), rect)| cursor_in_rect_physical((cx, cy), rect, ppp))
                    .unwrap_or(false);
                if !in_viewport {
                    return;
                }

                let scroll = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(pos) => pos.y as f32 * 0.01,
                };
                self.camera.zoom(scroll);
            }

            WindowEvent::ModifiersChanged(mods) => {
                self.input.shift_held = mods.state().shift_key();
            }

            _ => {}
        }
    }
}

// ── Input routing helpers ─────────────────────────────────────────────────────

/// Returns `true` when the physical-pixel cursor position `(cx, cy)` lies
/// inside `rect_logical` after converting to logical points using
/// `pixels_per_point`.
///
/// Coordinate convention:
/// - `cursor_phys` — physical pixels, from `WindowEvent::CursorMoved`.
/// - `rect_logical` — egui logical points, from `ui.available_rect_before_wrap()`.
/// - `ppp` — pixels per logical point, from `egui_winit::pixels_per_point`.
///
/// Separating this as a pure function makes it trivially testable without
/// a live event loop.
pub(crate) fn cursor_in_rect_physical(
    cursor_phys: (f64, f64),
    rect_logical: egui::Rect,
    ppp: f32,
) -> bool {
    let lx = (cursor_phys.0 / ppp as f64) as f32;
    let ly = (cursor_phys.1 / ppp as f64) as f32;
    rect_logical.contains(egui::pos2(lx, ly))
}

// ── Hex pick — ray → sea plane → axial ───────────────────────────────────────

/// Given a cursor position in physical pixels and the viewport rect in logical
/// points, compute which hex the cursor is over (if any) by:
///
/// 1. Converting cursor physical → NDC within the viewport.
/// 2. Casting a ray from `view_projection.inverse()` through the NDC point.
/// 3. Intersecting the ray with the `y = sea_level` plane.
/// 4. Converting world XZ → sim XZ via `sim_to_world_scale`.
/// 5. Calling `pixel_to_offset` to find the hex.
///
/// Returns `None` when the ray misses the plane (parallel or behind camera),
/// when `hex_grid` is absent (partial pipeline run), or when the sim-space
/// point falls outside the hex grid.
#[allow(clippy::too_many_arguments)]
pub(super) fn screen_to_picked_hex(
    cursor_phys: (f64, f64),
    rect_logical: egui::Rect,
    ppp: f32,
    view_projection: glam::Mat4,
    eye: glam::Vec3,
    sea_level: f32,
    world_xz_extent: f32,
    sim_width: u32,
    hex_grid: &island_core::world::HexGrid,
) -> Option<hex::OffsetCoord> {
    const EPS: f32 = 1e-6;

    // ── 1. Cursor physical → NDC ──────────────────────────────────────────────
    let lx = (cursor_phys.0 / ppp as f64) as f32;
    let ly = (cursor_phys.1 / ppp as f64) as f32;
    let u = (lx - rect_logical.min.x) / rect_logical.width().max(EPS);
    let v = (ly - rect_logical.min.y) / rect_logical.height().max(EPS);
    let ndc_x = 2.0 * u - 1.0;
    // Flip Y: egui y points down, NDC y points up.
    let ndc_y = 1.0 - 2.0 * v;

    // ── 2. Ray from inverse VP matrix ────────────────────────────────────────
    let inv = view_projection.inverse();
    // depth 0 ≡ near plane, depth 1 ≡ far plane (Metal/wgpu [0, 1] depth range)
    let p_near = inv.project_point3(glam::Vec3::new(ndc_x, ndc_y, 0.0));
    let p_far = inv.project_point3(glam::Vec3::new(ndc_x, ndc_y, 1.0));
    let dir = (p_far - p_near).normalize();

    // ── 3. Intersect with y = sea_level plane ────────────────────────────────
    if dir.y.abs() < EPS {
        // Ray is (nearly) parallel to the horizontal plane — no intersection.
        return None;
    }
    let t = (sea_level - eye.y) / dir.y;
    if !t.is_finite() || t < 0.0 {
        // Behind camera or NaN (degenerate matrix).
        return None;
    }
    let hit = eye + dir * t;

    // ── 4. World XZ → sim XZ ─────────────────────────────────────────────────
    let scale = hex::geometry::sim_to_world_scale(sim_width, world_xz_extent);
    if scale.abs() < EPS {
        return None;
    }
    let sim_x = hit.x / scale;
    let sim_z = hit.z / scale;

    // ── 5. Sim → offset coord ─────────────────────────────────────────────────
    let origin = hex::geometry::default_grid_origin(hex_grid.hex_size);
    hex::geometry::pixel_to_offset(
        sim_x,
        sim_z,
        hex_grid.hex_size,
        origin,
        hex_grid.cols,
        hex_grid.rows,
    )
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use island_core::{
        field::ScalarField2D,
        world::{HexGrid, HexLayout},
    };

    /// Build a minimal HexGrid for pick tests (hex_id_of_cell filled with 0).
    fn make_hex_grid(cols: u32, rows: u32, hex_size: f32) -> HexGrid {
        HexGrid {
            cols,
            rows,
            hex_size,
            layout: HexLayout::FlatTop,
            hex_id_of_cell: ScalarField2D::new(cols, rows),
        }
    }

    /// Build a view-projection matrix for a camera looking straight down at
    /// the given `target` from directly above (eye at `target + (0, dist, 0)`).
    /// `fov_y` in radians, `aspect` = width/height.
    fn top_down_vp(target: glam::Vec3, dist: f32, fov_y: f32, aspect: f32) -> glam::Mat4 {
        let eye = target + glam::Vec3::new(0.0, dist, 0.0);
        let view = glam::Mat4::look_at_rh(eye, target, glam::Vec3::Z);
        let proj = glam::Mat4::perspective_rh(fov_y, aspect, 0.1, 100.0);
        proj * view
    }

    /// A ray cast from a camera looking horizontally (eye at sea_level height,
    /// ray direction parallel to the XZ plane) must return `None` because it
    /// never intersects the `y = sea_level` plane.
    #[test]
    fn screen_to_picked_hex_returns_none_when_ray_parallel_to_sea_plane() {
        // Camera looking along +X at the same height as sea_level.
        let sea_level = 0.0_f32;
        let eye = glam::Vec3::new(0.0, sea_level, 0.0);
        let target = glam::Vec3::new(1.0, sea_level, 0.0);
        let view = glam::Mat4::look_at_rh(eye, target, glam::Vec3::Y);
        let proj = glam::Mat4::perspective_rh(45_f32.to_radians(), 1.0, 0.1, 100.0);
        let vp = proj * view;

        // Centre of a 100×100 viewport at ppp=1.
        let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(100.0, 100.0));
        let cursor = (50.0_f64, 50.0_f64);
        let hex_grid = make_hex_grid(4, 4, 10.0);

        let result =
            screen_to_picked_hex(cursor, rect, 1.0, vp, eye, sea_level, 5.0, 128, &hex_grid);
        assert_eq!(
            result, None,
            "horizontal ray must return None (misses y = sea_level plane)"
        );
    }

    /// A camera looking straight down at the grid centre should resolve the
    /// centred cursor to a valid hex offset.
    #[test]
    fn screen_to_picked_hex_top_down_centre_returns_some() {
        // Grid: 4×4 hexes, hex_size = 10.0 sim units.
        // The grid occupies approximately [0, 4*hex_width] × [0, 4*row_spacing]
        // in sim space, with hex centres at (col+0.5)*hex_w + offset, etc.
        // Place camera above the centre of the grid in world space.
        let hex_size = 10.0_f32;
        let hex_grid = make_hex_grid(4, 4, hex_size);

        // Use sim_width = 128, world_extent = 5.0 — matches production defaults.
        let sim_width = 128_u32;
        let world_extent = 5.0_f32;
        let scale = hex::geometry::sim_to_world_scale(sim_width, world_extent);

        // Compute the world-space centre of hex (1, 1) and point the camera there.
        let origin = hex::geometry::default_grid_origin(hex_size);
        let (sim_cx, sim_cy) = hex::geometry::offset_to_pixel(1, 1, hex_size, origin);
        let world_cx = sim_cx * scale;
        let world_cy_as_z = sim_cy * scale; // sim Y → world Z

        let sea_level = 0.0_f32;
        let target = glam::Vec3::new(world_cx, sea_level, world_cy_as_z);
        let dist = 5.0_f32;
        let fov_y = 45_f32.to_radians();
        let aspect = 1.0_f32;
        let vp = top_down_vp(target, dist, fov_y, aspect);
        let eye = target + glam::Vec3::new(0.0, dist, 0.0);

        // A 200×200 logical-point viewport at ppp=1.0; cursor at exact centre.
        let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(200.0, 200.0));
        let cursor = (100.0_f64, 100.0_f64);

        let result = screen_to_picked_hex(
            cursor,
            rect,
            1.0,
            vp,
            eye,
            sea_level,
            world_extent,
            sim_width,
            &hex_grid,
        );
        assert_eq!(
            result,
            Some(hex::OffsetCoord::new(1, 1)),
            "top-down camera aimed at hex (1,1) centre must resolve to that hex"
        );
    }
}
