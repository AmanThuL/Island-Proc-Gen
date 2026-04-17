//! Island Proc-Gen — Sprint 0 desktop shell.
//!
//! Boots a winit 0.30 `ApplicationHandler` event loop, constructs the
//! `Runtime` (window + wgpu + egui + camera) on the first `resumed` event,
//! and delegates all window events to `Runtime::handle_window_event`.

use anyhow::Result;
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    window::WindowId,
};

use app::runtime::Runtime;

// ── AppHandler ────────────────────────────────────────────────────────────────

struct AppHandler {
    runtime: Option<Runtime>,
}

impl ApplicationHandler for AppHandler {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.runtime.is_none() {
            match Runtime::new(event_loop) {
                Ok(rt) => {
                    self.runtime = Some(rt);
                }
                Err(e) => {
                    tracing::error!("Runtime::new failed: {e:#}");
                    event_loop.exit();
                }
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        if let Some(rt) = self.runtime.as_mut() {
            rt.handle_window_event(event_loop, event);
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(rt) = self.runtime.as_ref() {
            rt.request_redraw();
        }
    }
}

// ── Entry point ───────────────────────────────────────────────────────────────

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("app=debug".parse().unwrap())
                .add_directive("gpu=info".parse().unwrap())
                .add_directive("render=info".parse().unwrap()),
        )
        .init();

    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);

    let mut handler = AppHandler { runtime: None };
    event_loop.run_app(&mut handler)?;

    Ok(())
}
