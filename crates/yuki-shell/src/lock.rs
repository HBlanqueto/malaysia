//! The lock component: the session-lock screen (`ext-session-lock-v1`).
//!
//! A full-screen surface over the locked session. It asks the compositor to
//! lock, waits for the `locked` event, creates a lock surface per output,
//! renders the lock screen on it (a solid panel with a message), and keeps the
//! surface alive so the compositor can forward input to it. The first version
//! has no authentication: it is the shell's door to the compositor's lock, and
//! a placeholder for the future passphrase/password UI.
//!
//! The never-unlock guarantee lives on the compositor side (`yuki/src/lock.rs`):
//! if this process dies while locked, the session stays locked.

use sctk::reexports::client::backend::ObjectId;
use std::cell::RefCell;
use std::ffi::c_void;

use sctk::{
    compositor::{CompositorHandler, CompositorState},
    delegate_compositor, delegate_output, delegate_registry, delegate_session_lock,
    output::{OutputHandler, OutputState},
    reexports::{
        calloop,
        calloop_wayland_source::WaylandSource,
        client::{
            Connection, Proxy, QueueHandle,
            globals::registry_queue_init,
            protocol::{wl_output, wl_surface::WlSurface},
        },
    },
    registry::{ProvidesRegistryState, RegistryState},
    session_lock::{
        SessionLock, SessionLockHandler, SessionLockState, SessionLockSurface,
        SessionLockSurfaceConfigure,
    },
};

use celestial_kit::gl::{GlesRenderer, Texture};
use celestial_kit::text::Text;

use crate::present::EglPresenter;

/// The font size, in logical pixels.
const FONT_SIZE: f32 = 24.0;
/// The lock screen background.
const LOCK_BG: [u8; 4] = [0x14, 0x14, 0x18, 0xff];
/// The message colour.
const TEXT_COLOR: [u8; 4] = [0xf2, 0xf2, 0xf5, 0xff];

struct Lock {
    registry: RegistryState,
    compositor: CompositorState,
    output: OutputState,
    /// The bound session-lock manager (missing if not advertised).
    session_lock: Option<SessionLockState>,
    /// The session lock once the compositor accepted it (the `locked` event).
    active: Option<SessionLock>,
    /// The lock surfaces, one per output, kept alive so their events dispatch.
    surfaces: RefCell<Vec<(wl_output::WlOutput, SessionLockSurface)>>,

    renderer: Option<GlesRenderer>,
    texture: Option<Texture>,
    /// One EGL presenter per lock surface (each owns its `wl_egl_window`).
    presenters: Vec<(ObjectId, EglPresenter)>,
    text: Text,

    buffer_size: (u32, u32),
}

/// Connects to the compositor, locks the session and runs the lock screen.
pub fn run() -> anyhow::Result<()> {
    let conn = Connection::connect_to_env()
        .map_err(|_| anyhow::anyhow!("WAYLAND_DISPLAY is not set to a reachable socket"))?;
    let (globals, event_queue) = registry_queue_init(&conn)?;
    let qh = event_queue.handle();

    let mut state = Lock {
        registry: RegistryState::new(&globals),
        compositor: CompositorState::bind(&globals, &qh)?,
        output: OutputState::new(&globals, &qh),
        session_lock: Some(SessionLockState::new(&globals, &qh)),
        active: None,
        surfaces: RefCell::new(Vec::new()),
        renderer: None,
        texture: None,
        presenters: Vec::new(),
        text: Text::new(FONT_SIZE),
        buffer_size: (0, 0),
    };

    // Ask the compositor to lock the session. The response arrives as a
    // `locked` or `finished` event on the new session lock object. The lock is
    // kept in `active` so the `locked` event finds it there and the object is
    // never destroyed behind the compositor's back: dropping a locked
    // `SessionLock` sends `destroy`, which is a protocol error while locked.
    // This client never unlocks on its own; only the (future) authentication
    // path will, via `SessionLock::unlock`. If the process dies, the session
    // stays locked (compositor policy, `yuki/src/lock.rs`).
    let session_lock = state
        .session_lock
        .as_ref()
        .expect("the session lock manager is bound")
        .lock(&qh)?;
    state.active = Some(session_lock);

    let mut event_loop: calloop::EventLoop<Lock> = calloop::EventLoop::try_new()?;
    WaylandSource::new(conn, event_queue).insert(event_loop.handle())?;
    event_loop.run(None, &mut state, |_| {})?;
    Ok(())
}

impl Lock {
    /// Creates lock surfaces for `outputs` that do not have one yet.
    ///
    /// A surface that already holds the lock role cannot be reused (a second
    /// role assignment is a protocol error), so each new output gets a fresh
    /// `wl_surface`. The compositor answers each one with a configure event;
    /// rendering happens there, and sctk sends `ack_configure` for us.
    fn lock_outputs(
        &mut self,
        conn: &Connection,
        qh: &QueueHandle<Self>,
        outputs: impl IntoIterator<Item = wl_output::WlOutput>,
    ) {
        let Some(session_lock) = self.active.as_ref() else {
            return;
        };
        let mut changed = false;
        for output in outputs {
            if self
                .surfaces
                .borrow()
                .iter()
                .any(|(locked, _)| *locked == output)
            {
                continue;
            }
            let surface = self.compositor.create_surface(qh);
            let lock_surface = session_lock.create_lock_surface(surface, &output, qh);
            self.surfaces.borrow_mut().push((output, lock_surface));
            changed = true;
        }
        if changed {
            self.render(conn);
        }
    }

    /// Renders the lock screen frame and presents it to every lock surface.
    fn render(&mut self, conn: &Connection) {
        let (w, h) = self.buffer_size;
        if w == 0 || h == 0 {
            return;
        }
        let stride = (w * 4) as usize;
        let mut frame = vec![0u8; stride * h as usize];

        fill_rect(&mut frame, (w, h), (0, 0, w, h), LOCK_BG);

        // A centered message; the authentication UI replaces this later.
        self.text.set_text("Bloqueado", FONT_SIZE, None);
        let (tw, _) = self.text.size();
        let baseline = (h as i32 - FONT_SIZE as i32) / 2;
        self.text.draw(
            &mut frame,
            stride,
            w as i32,
            h as i32,
            ((w as i32 - tw.round() as i32) / 2, baseline),
            TEXT_COLOR,
        );

        if self.renderer.is_none() {
            let display_ptr = conn.display().id().as_ptr().cast::<c_void>();
            // Safety: the display handle is the live Wayland connection's.
            self.renderer =
                Some(unsafe { GlesRenderer::new(display_ptr) }.expect("create the GLES renderer"));
        }
        let Some(renderer) = &mut self.renderer else {
            return;
        };
        if self.texture.as_ref().is_some_and(|t| t.size == (w, h)) {
            if let Some(texture) = &self.texture
                && renderer.upload_texture_rgba(texture, &frame).is_err()
            {
                return;
            }
        } else {
            match renderer.create_texture_rgba((w, h), &frame) {
                Ok(texture) => self.texture = Some(texture),
                Err(_) => return,
            }
        }
        let Some(texture) = &self.texture else {
            return;
        };

        // Present the same frame to every lock surface, pruning presenters of
        // surfaces that are gone.
        let surfaces = self.surfaces.borrow().clone();
        self.presenters
            .retain(|(id, _)| surfaces.iter().any(|(_, s)| s.wl_surface().id() == *id));
        for (_, surface) in surfaces {
            let wl_surface = surface.wl_surface();
            let id = wl_surface.id();
            let presenter = match self
                .presenters
                .iter_mut()
                .find(|(existing, _)| *existing == id)
            {
                Some((_, presenter)) => presenter,
                None => {
                    self.presenters.push((id, EglPresenter::new()));
                    &mut self.presenters.last_mut().expect("just pushed").1
                }
            };
            if let Err(error) = presenter.present(renderer, wl_surface, texture, (w, h)) {
                eprintln!("lock: present failed: {error}");
            }
        }
    }
}

impl ProvidesRegistryState for Lock {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }

    fn runtime_add_global(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: u32,
        _: &str,
        _: u32,
    ) {
    }

    fn runtime_remove_global(&mut self, _: &Connection, _: &QueueHandle<Self>, _: u32, _: &str) {}
}

impl OutputHandler for Lock {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output
    }

    fn new_output(
        &mut self,
        conn: &Connection,
        qh: &QueueHandle<Self>,
        output: wl_output::WlOutput,
    ) {
        // An output added while the session is locked gets its own lock
        // surface immediately (the protocol expects one per output).
        self.lock_outputs(conn, qh, [output]);
    }

    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}

    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl CompositorHandler for Lock {
    fn scale_factor_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlSurface,
        _: i32,
    ) {
    }

    fn transform_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlSurface,
        _: wl_output::Transform,
    ) {
    }

    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlSurface, _: u32) {}

    fn surface_enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }

    fn surface_leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
}

impl SessionLockHandler for Lock {
    fn locked(&mut self, conn: &Connection, qh: &QueueHandle<Self>, session_lock: SessionLock) {
        // The session is locked; create lock surfaces for every output and
        // draw. This client now decides when the session unlocks.
        self.active = Some(session_lock.clone());
        let outputs: Vec<wl_output::WlOutput> = self.output.outputs().collect();
        self.lock_outputs(conn, qh, outputs);
    }

    fn finished(&mut self, _: &Connection, _: &QueueHandle<Self>, _: SessionLock) {
        // The compositor refused the lock (or a newer lock superseded it).
        eprintln!("lock: the compositor did not lock the session");
        std::process::exit(1);
    }

    fn configure(
        &mut self,
        conn: &Connection,
        _qh: &QueueHandle<Self>,
        _: SessionLockSurface,
        configure: SessionLockSurfaceConfigure,
        _serial: u32,
    ) {
        // The compositor asked for an exact size; render at it. sctk has
        // already sent `ack_configure` for this serial.
        let (w, h) = configure.new_size;
        if w == 0 || h == 0 {
            return;
        }
        self.buffer_size = (w, h);
        self.render(conn);
    }
}

delegate_compositor!(Lock);
delegate_output!(Lock);
delegate_registry!(Lock);
delegate_session_lock!(Lock);

// --- helpers ---------------------------------------------------------------

/// Fills a rectangle in a top-down RGBA frame, src-over.
fn fill_rect(frame: &mut [u8], frame_size: (u32, u32), rect: (i32, i32, u32, u32), color: [u8; 4]) {
    let (w, h) = frame_size;
    let (x, y, rw, rh) = rect;
    let stride = (w * 4) as usize;
    for row in y..(y + rh as i32) {
        if row < 0 || row >= h as i32 {
            continue;
        }
        for col in x..(x + rw as i32) {
            if col < 0 || col >= w as i32 {
                continue;
            }
            let pi = row as usize * stride + col as usize * 4;
            let a = color[3] as f32 / 255.0;
            frame[pi] = (color[0] as f32 * a + frame[pi] as f32 * (1.0 - a)) as u8;
            frame[pi + 1] = (color[1] as f32 * a + frame[pi + 1] as f32 * (1.0 - a)) as u8;
            frame[pi + 2] = (color[2] as f32 * a + frame[pi + 2] as f32 * (1.0 - a)) as u8;
            frame[pi + 3] = (a * 255.0 + frame[pi + 3] as f32 * (1.0 - a)) as u8;
        }
    }
}
