//! The host: a window, and just enough of an event pump to put a frame on
//! screen through the [`Present`] seam.
//!
//! **Deliberately not here.** The frame loop, the clock, the `dt` clamp and
//! its 100 ms ceiling, `LiveScene::tick`, idle-frame skipping off the
//! generation stamp, the `EventLoopProxy` wake path, and the resize path that
//! re-solves the document for a new extent are all story #572. What this file
//! carries is the minimum that makes the seam demonstrably real: a window
//! exists, a presenter is bound to it, and a committed frame reaches it.
//!
//! The resize handler is the one place where that line needs stating. It
//! forwards the new extent to the presenter, because a surface configured for
//! the old extent posts a torn frame or refuses one, and it stops there. The
//! scene is not re-solved for the new size, so a resized window shows the
//! frame the scene was built for. Story #572 owns that.

use std::error::Error;
use std::sync::Arc;

use dashscene_core::Arena;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

use crate::present::{Present, SkiaPresenter};

/// Builds a scene into `arena` for a drawable of `width` x `height` physical
/// pixels.
///
/// The extent is passed in rather than fixed because the window's physical
/// size is only known once the window exists, and on a high-density display it
/// is not the logical size that was asked for.
pub type SceneBuilder = fn(&mut Arena, u32, u32);

/// The window's requested size, in logical pixels.
const WINDOW_SIZE: LogicalSize<u32> = LogicalSize::new(960, 600);

/// Opens a window, binds the Skia presenter to it, and runs until the window
/// is closed.
pub fn run(title: &'static str, scene: SceneBuilder) -> Result<(), Box<dyn Error>> {
    let event_loop = EventLoop::new()?;
    // Wait, not Poll: nothing here animates yet, so waking for a redraw of an
    // unchanged screen would be work with no output. Choosing between the two
    // off the generation stamp is story #572.
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut host = Host {
        title,
        scene,
        window: None,
        presenter: None,
        arena: Arena::new(),
        failure: None,
    };
    event_loop.run_app(&mut host)?;
    match host.failure {
        Some(failure) => Err(failure),
        None => Ok(()),
    }
}

struct Host {
    title: &'static str,
    scene: SceneBuilder,
    window: Option<Arc<Window>>,
    presenter: Option<Box<dyn Present>>,
    arena: Arena,
    /// The first error that stopped the loop. `ApplicationHandler`'s methods
    /// return nothing, so a failure is parked here and reported by [`run`]
    /// rather than printed and forgotten.
    failure: Option<Box<dyn Error>>,
}

impl Host {
    /// Records `failure` and asks the event loop to stop. The first failure
    /// wins: a later one is a consequence of the state the first left behind.
    fn fail(&mut self, event_loop: &ActiveEventLoop, failure: impl Error + 'static) {
        if self.failure.is_none() {
            self.failure = Some(Box::new(failure));
        }
        event_loop.exit();
    }
}

impl ApplicationHandler for Host {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // `resumed` fires again after a suspend on the platforms that suspend.
        // The window and its surface survive that here, so rebuilding them
        // would drop a live surface for no reason.
        if self.window.is_some() {
            return;
        }

        let attributes = Window::default_attributes()
            .with_title(self.title)
            .with_inner_size(WINDOW_SIZE);
        let window = match event_loop.create_window(attributes) {
            Ok(window) => Arc::new(window),
            Err(error) => return self.fail(event_loop, error),
        };
        let presenter = match SkiaPresenter::new(Arc::clone(&window)) {
            Ok(presenter) => presenter,
            Err(error) => return self.fail(event_loop, error),
        };

        // Physical pixels, so the scene fills the drawable on a high-density
        // display instead of occupying a corner of it.
        let size = window.inner_size();
        (self.scene)(&mut self.arena, size.width, size.height);
        eprintln!(
            "demo: {} — {}x{} physical pixels, {} rects",
            presenter.name(),
            size.width,
            size.height,
            self.arena.committed().rects().len()
        );

        window.request_redraw();
        self.window = Some(window);
        self.presenter = Some(Box::new(presenter));
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        let (Some(window), Some(presenter)) = (self.window.as_ref(), self.presenter.as_mut())
        else {
            return;
        };
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Err(error) = presenter.resize(size.width, size.height) {
                    return self.fail(event_loop, error);
                }
                window.request_redraw();
            }
            WindowEvent::RedrawRequested => {
                // Tells the compositor a frame is about to be posted, so it
                // can schedule the next one; winit asks for it immediately
                // before presenting.
                window.pre_present_notify();
                if let Err(error) = presenter.present(self.arena.committed()) {
                    self.fail(event_loop, error);
                }
            }
            _ => {}
        }
    }
}
