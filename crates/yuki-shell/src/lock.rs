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
    delegate_compositor, delegate_keyboard, delegate_output, delegate_registry, delegate_seat,
    delegate_session_lock,
    output::{OutputHandler, OutputState},
    reexports::{
        calloop,
        calloop_wayland_source::WaylandSource,
        client::{
            Connection, Proxy, QueueHandle,
            globals::registry_queue_init,
            protocol::{wl_keyboard, wl_output, wl_seat, wl_surface::WlSurface},
        },
    },
    registry::{ProvidesRegistryState, RegistryState},
    seat::{
        Capability, SeatHandler, SeatState,
        keyboard::{KeyEvent, KeyboardHandler, Keysym, Modifiers},
    },
    session_lock::{
        SessionLock, SessionLockHandler, SessionLockState, SessionLockSurface,
        SessionLockSurfaceConfigure,
    },
};

use celestial_kit::gl::{GlesRenderer, Texture};
use celestial_kit::password::PasswordPrompt;
use yuki_design::LOCK_FONT_SIZE;

use crate::pam;
use crate::present::EglPresenter;

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
    prompt: PasswordPrompt,
    /// The keyboard the compositor gives focus to while locked.
    _keyboard: Option<wl_keyboard::WlKeyboard>,
    seat: SeatState,
    modifiers: Modifiers,
    /// PAM runs in a thread; these are the two halves of its conversation.
    pam_tx: calloop::channel::Sender<pam::Event>,
    attempt: Option<pam::Attempt>,
    loop_signal: calloop::LoopSignal,
    conn: Connection,

    buffer_size: (u32, u32),
}

/// Connects to the compositor, locks the session and runs the lock screen.
pub fn run() -> anyhow::Result<()> {
    let conn = Connection::connect_to_env()
        .map_err(|_| anyhow::anyhow!("WAYLAND_DISPLAY is not set to a reachable socket"))?;
    let (globals, event_queue) = registry_queue_init(&conn)?;
    let qh = event_queue.handle();
    let (pam_tx, pam_rx) = calloop::channel::channel();
    let mut event_loop: calloop::EventLoop<Lock> = calloop::EventLoop::try_new()?;
    let loop_signal = event_loop.get_signal();

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
        prompt: PasswordPrompt::new(LOCK_FONT_SIZE),
        _keyboard: None,
        seat: SeatState::new(&globals, &qh),
        modifiers: Modifiers::default(),
        pam_tx,
        attempt: None,
        loop_signal: loop_signal.clone(),
        conn: conn.clone(),
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

    event_loop
        .handle()
        .insert_source(pam_rx, |event, _, lock: &mut Lock| match event {
            calloop::channel::Event::Msg(event) => lock.pam_event(event),
            calloop::channel::Event::Closed => {}
        })
        .map_err(|_| anyhow::anyhow!("inserting the PAM channel"))?;
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
        _conn: &Connection,
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
            self.render();
        }
    }

    /// Starts PAM once the compositor has confirmed the lock.
    fn start_auth(&mut self) {
        if self.attempt.is_some() || self.active.is_none() {
            return;
        }
        let Some(user) = pam::current_user() else {
            eprintln!("lock: cannot resolve the current user for PAM");
            return;
        };
        eprintln!("lock: starting PAM for {user}");
        self.attempt = Some(pam::Attempt::start(
            self.pam_tx.clone(),
            pam::Service::LOCK,
            user,
        ));
    }

    /// Handles one event from the PAM thread.
    fn pam_event(&mut self, event: pam::Event) {
        match event {
            pam::Event::Said(pam::Message::Prompt { text, secret }) => {
                let _ = secret;
                self.prompt.set_label(pam::prompt_label(&text));
                self.render();
            }
            pam::Event::Said(pam::Message::Info(text)) => {
                eprintln!("lock: PAM: {text}");
            }
            pam::Event::Said(pam::Message::Error(text)) => {
                self.prompt.set_error(text);
                self.render();
            }
            pam::Event::Ended(pam::Outcome::Authenticated) => {
                if let Some(lock) = &self.active {
                    lock.unlock();
                }
                let _ = self.conn.flush();
                self.loop_signal.stop();
            }
            pam::Event::Ended(pam::Outcome::Denied(message)) => {
                eprintln!("lock: PAM refused: {message}");
                self.attempt = None;
                self.prompt.set_error(message);
                self.render();
            }
        }
    }

    /// Sends the current secret to PAM, if an attempt is waiting for it.
    fn submit(&mut self) {
        if self.attempt.is_none() {
            self.start_auth();
        }
        let Some(secret) = self.prompt.take() else {
            return;
        };
        if let Some(attempt) = &self.attempt {
            attempt.answer(secret);
        }
        self.render();
    }

    /// Renders the lock screen frame and presents it to every lock surface.
    fn render(&mut self) {
        let (w, h) = self.buffer_size;
        if w == 0 || h == 0 {
            return;
        }
        let stride = (w * 4) as usize;
        let mut frame = vec![0u8; stride * h as usize];

        self.prompt.draw(&mut frame, stride, w, h);

        if self.renderer.is_none() {
            let display_ptr = self.conn.display().id().as_ptr().cast::<c_void>();
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

impl SeatHandler for Lock {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat
    }

    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}

    fn new_capability(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Keyboard
            && let Ok(keyboard) = self.seat.get_keyboard::<Self, Self>(qh, &seat, None)
        {
            self._keyboard = Some(keyboard);
        }
    }

    fn remove_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Keyboard {
            self._keyboard = None;
        }
    }

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl KeyboardHandler for Lock {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: &WlSurface,
        _: u32,
        _: &[u32],
        _: &[Keysym],
    ) {
    }

    fn leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: &WlSurface,
        _: u32,
    ) {
    }

    fn press_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        event: KeyEvent,
    ) {
        if event.keysym == Keysym::Return || event.keysym == Keysym::KP_Enter {
            self.submit();
            return;
        }
        if event.keysym == Keysym::BackSpace {
            self.prompt.backspace();
            self.render();
            return;
        }
        if self.modifiers.ctrl || self.modifiers.alt || self.modifiers.logo {
            return;
        }
        if let Some(text) = event.utf8 {
            for ch in text.chars() {
                self.prompt.insert(ch);
            }
            self.render();
        }
    }

    fn release_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        _: KeyEvent,
    ) {
    }

    fn update_modifiers(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        modifiers: Modifiers,
        _: u32,
    ) {
        self.modifiers = modifiers;
    }
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
        self.start_auth();
    }

    fn finished(&mut self, _: &Connection, _: &QueueHandle<Self>, _: SessionLock) {
        // The compositor refused the lock (or a newer lock superseded it).
        eprintln!("lock: the compositor did not lock the session");
        std::process::exit(1);
    }

    fn configure(
        &mut self,
        _conn: &Connection,
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
        self.render();
    }
}

delegate_compositor!(Lock);
delegate_keyboard!(Lock);
delegate_output!(Lock);
delegate_registry!(Lock);
delegate_seat!(Lock);
delegate_session_lock!(Lock);

// --- helpers ---------------------------------------------------------------
