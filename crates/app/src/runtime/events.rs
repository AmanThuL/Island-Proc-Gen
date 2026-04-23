use winit::{
    event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::ActiveEventLoop,
};

use tracing::debug;

use super::Runtime;

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
                        self.input.left_pressed = is_press;
                        if !is_press {
                            self.input.last_cursor = None;
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
