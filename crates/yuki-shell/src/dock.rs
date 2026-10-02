//! The dock component: the macOS-style dock at the screen edge.
//!
//! A layer-shell surface with an exclusive zone (bottom by default, from
//! `dock.domain`). It lists the running apps from
//! `ext-foreign-toplevel-list-v1` and, on click, activates the app through
//! `zwlr_foreign_toplevel_management_v1`. The active app is highlighted from
//! the compositor's `window.domain` store.

use std::time::{Duration, SystemTime};
use std::{ffi::c_void, path::PathBuf};

use sctk::{
    compositor::{CompositorHandler, CompositorState},
    delegate_compositor, delegate_output, delegate_registry, delegate_seat,
    globals::GlobalData,
    output::{OutputHandler, OutputState},
    reexports::{
        calloop,
        calloop_wayland_source::WaylandSource,
        client::{
            Connection, Dispatch, Proxy, QueueHandle, delegate_dispatch, event_created_child,
            globals::registry_queue_init,
            protocol::{
                wl_buffer::WlBuffer, wl_output, wl_pointer, wl_seat, wl_surface::WlSurface,
            },
        },
        protocols::{
            ext::foreign_toplevel_list::v1::client::{
                ext_foreign_toplevel_handle_v1::{
                    Event as FtlHandleEvent, ExtForeignToplevelHandleV1,
                },
                ext_foreign_toplevel_list_v1::{Event as FtlListEvent, ExtForeignToplevelListV1},
            },
            wp::{
                fractional_scale::v1::client::{
                    wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1,
                    wp_fractional_scale_v1::{Event as FractionalScaleEvent, WpFractionalScaleV1},
                },
                viewporter::client::{wp_viewport::WpViewport, wp_viewporter::WpViewporter},
            },
        },
        protocols_wlr::{
            foreign_toplevel::v1::client::{
                zwlr_foreign_toplevel_handle_v1::ZwlrForeignToplevelHandleV1,
                zwlr_foreign_toplevel_manager_v1::{
                    Event as MgmtListEvent, ZwlrForeignToplevelManagerV1,
                },
            },
            layer_shell::v1::client::{zwlr_layer_shell_v1, zwlr_layer_surface_v1},
        },
    },
    registry::{ProvidesRegistryState, RegistryState},
    seat::{
        Capability, SeatHandler, SeatState,
        pointer::{BTN_LEFT, PointerEvent, PointerEventKind, PointerHandler},
    },
    shell::wlr_layer::{
        Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
        LayerSurfaceConfigure, LayerSurfaceData,
    },
};

use celestial_kit::gl::{GlesRenderer, Texture};
use celestial_kit::text::Text;

/// The tile size, in logical pixels (overridable via `dock.domain` `tilesize`).
const DEFAULT_TILESIZE: i32 = 48;
/// The gap between tiles.
const GAP: i32 = 6;
/// The dock padding around the tiles.
const PADDING: i32 = 8;
/// The bar height, in logical pixels.
const BAR_HEIGHT: i32 = DEFAULT_TILESIZE + PADDING * 2;
/// The bar background.
const BAR_BG: [u8; 4] = [0x1c, 0x1c, 0x1f, 0xe0];
/// A running app tile.
const TILE_BG: [u8; 4] = [0x3a, 0x3a, 0x40, 0xff];
/// The active app tile.
const TILE_ACTIVE: [u8; 4] = [0x2f, 0x6f, 0xd8, 0xff];
/// The tile text colour.
const TEXT_COLOR: [u8; 4] = [0xf2, 0xf2, 0xf5, 0xff];

/// One app in the dock.
#[derive(Clone)]
struct DockEntry {
    list_handle: ExtForeignToplevelHandleV1,
    mgmt_handle: Option<ZwlrForeignToplevelHandleV1>,
    title: String,
    /// The tile's left edge, in logical pixels.
    x: i32,
}

struct Dock {
    registry: RegistryState,
    compositor: CompositorState,
    output: OutputState,
    layer_shell: Option<LayerShell>,
    viewporter: Option<WpViewporter>,
    fractional: Option<WpFractionalScaleManagerV1>,
    // The foreign-toplevel proxies are kept alive so their events dispatch.
    _foreign_list: Option<ExtForeignToplevelListV1>,
    _foreign_mgmt: Option<ZwlrForeignToplevelManagerV1>,
    seat: SeatState,

    surface: Option<WlSurface>,
    layer_surface: Option<LayerSurface>,
    viewport: Option<WpViewport>,

    renderer: Option<GlesRenderer>,
    presenter: crate::present::EglPresenter,
    texture: Option<Texture>,
    text: Text,

    scale: f64,
    logical_size: (u32, u32),
    buffer_size: (u32, u32),
    tilesize: i32,

    entries: Vec<DockEntry>,
    active_title: String,
    /// The last active title uploaded, to know which tile changed.
    last_active_title: String,
    /// Set when the tile layout changed (add/remove/resize): full re-upload.
    layout_dirty: bool,
    /// The full frame, kept so a redraw uploads only the damaged tiles.
    pixels: Vec<u8>,

    dock_store: PathBuf,
    last_dock_modified: Option<SystemTime>,
    active_store: PathBuf,
    last_active_modified: Option<SystemTime>,
    needs_redraw: bool,
}

/// Connects to the compositor and runs the dock until closed.
pub fn run() -> anyhow::Result<()> {
    let dock_store = data_dir().join("dock.domain");
    let active_store = data_dir().join("window.domain");

    let conn = Connection::connect_to_env()
        .map_err(|_| anyhow::anyhow!("WAYLAND_DISPLAY is not set to a reachable socket"))?;
    let (globals, event_queue) = registry_queue_init(&conn)?;
    let qh = event_queue.handle();

    let mut state = Dock {
        registry: RegistryState::new(&globals),
        compositor: CompositorState::bind(&globals, &qh)?,
        output: OutputState::new(&globals, &qh),
        layer_shell: LayerShell::bind(&globals, &qh).ok(),
        viewporter: globals.bind(&qh, 1..=1, ()).ok(),
        fractional: globals.bind(&qh, 1..=1, ()).ok(),
        _foreign_list: globals.bind(&qh, 1..=1, ()).ok(),
        _foreign_mgmt: globals.bind(&qh, 1..=1, ()).ok(),
        seat: SeatState::new(&globals, &qh),
        surface: None,
        layer_surface: None,
        viewport: None,
        renderer: None,
        presenter: crate::present::EglPresenter::new(),
        texture: None,
        text: Text::new(11.0),
        scale: 1.0,
        logical_size: (0, 0),
        buffer_size: (0, 0),
        tilesize: DEFAULT_TILESIZE,
        entries: Vec::new(),
        active_title: String::new(),
        last_active_title: String::new(),
        layout_dirty: false,
        pixels: Vec::new(),
        dock_store,
        last_dock_modified: None,
        active_store,
        last_active_modified: None,
        needs_redraw: false,
    };

    if state.layer_shell.is_none() {
        anyhow::bail!("the compositor does not advertise zwlr_layer_shell_v1");
    }

    state.create_surfaces(&qh);

    let mut event_loop: calloop::EventLoop<Dock> = calloop::EventLoop::try_new()?;
    WaylandSource::new(conn.clone(), event_queue).insert(event_loop.handle())?;
    let store_conn = conn.clone();
    let store_qh = qh.clone();
    event_loop
        .handle()
        .insert_source(
            calloop::timer::Timer::from_duration(Duration::from_millis(500)),
            move |_, _, state: &mut Dock| {
                state.check_stores(&store_conn, &store_qh);
                calloop::timer::TimeoutAction::ToDuration(Duration::from_millis(500))
            },
        )
        .expect("insert the dock store timer");

    event_loop.run(None, &mut state, |_| {})?;
    Ok(())
}

impl Dock {
    /// Creates the layer surface (top edge: bottom, exclusive zone).
    fn create_surfaces(&mut self, qh: &QueueHandle<Self>) {
        let surface = self.compositor.create_surface(qh);
        self.surface = Some(surface.clone());

        let layer_shell = self.layer_shell.as_ref().expect("layer-shell present");
        let layer_surface = layer_shell.create_layer_surface(
            qh,
            surface.clone(),
            Layer::Top,
            Some("yuki-dock"),
            None,
        );
        let bar_height = BAR_HEIGHT + (self.tilesize - DEFAULT_TILESIZE);
        layer_surface.set_size(0, bar_height as u32);
        layer_surface.set_anchor(Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);
        layer_surface.set_exclusive_zone(bar_height);
        layer_surface.set_keyboard_interactivity(KeyboardInteractivity::OnDemand);
        self.layer_surface = Some(layer_surface);

        if let Some(viewporter) = &self.viewporter {
            self.viewport =
                Some(viewporter.get_viewport::<WlSurface, Self>(&surface, qh, surface.clone()));
        }
        if let Some(fractional) = &self.fractional {
            let _fractional_scale =
                fractional.get_fractional_scale::<WlSurface, Self>(&surface, qh, surface.clone());
        }

        surface.commit();
    }

    /// Applies a configure from the compositor.
    fn configure(
        &mut self,
        conn: &Connection,
        _qh: &QueueHandle<Self>,
        configure: LayerSurfaceConfigure,
    ) {
        let (width, height) = configure.new_size;
        if width == 0 || height == 0 {
            return;
        }
        self.logical_size = (width, height);

        let scale = self.scale.max(1.0);
        let (bw, bh) = (
            (width as f64 * scale).round().max(1.0) as u32,
            (height as f64 * scale).round().max(1.0) as u32,
        );
        self.buffer_size = (bw, bh);

        if let Some(surface) = &self.surface {
            surface.set_buffer_scale(scale.round().max(1.0) as i32);
        }
        if let Some(viewport) = &self.viewport {
            viewport.set_source(0.0, 0.0, bw as f64, bh as f64);
            viewport.set_destination(width as i32, height as i32);
        }

        self.layout_tiles();
        self.render(conn);
    }

    /// Lays the tiles out left to right.
    fn layout_tiles(&mut self) {
        let mut x = PADDING;
        for entry in &mut self.entries {
            entry.x = x;
            x += self.tilesize + GAP;
        }
    }

    /// Rebuilds the frame and presents it (Bloque 7c).
    ///
    /// A layout change re-uploads the whole frame; a change of the active app
    /// uploads only the affected tile(s) via `glTexSubImage2D`. When nothing
    /// changed no frame is committed.
    fn render(&mut self, conn: &Connection) {
        let (w, h) = self.buffer_size;
        if w == 0 || h == 0 {
            return;
        }
        let stride = (w * 4) as usize;
        if self.pixels.len() != stride * h as usize {
            self.pixels = vec![0u8; stride * h as usize];
        }
        let frame = &mut self.pixels;

        fill_rect(frame, (w, h), (0, 0, w, h), BAR_BG);

        let baseline = PADDING + 8;
        for entry in &self.entries {
            let active = entry.title == self.active_title;
            let bg = if active { TILE_ACTIVE } else { TILE_BG };
            fill_rect(
                frame,
                (w, h),
                (entry.x, PADDING, self.tilesize as u32, self.tilesize as u32),
                bg,
            );
            // The app name, clipped to the tile.
            self.text
                .set_text(&entry.title, 11.0, Some(self.tilesize as f32 - 4.0));
            self.text.draw(
                frame,
                stride,
                w as i32,
                h as i32,
                (entry.x + 2, baseline),
                TEXT_COLOR,
            );
        }

        // The damaged region: the whole bar on a layout change, otherwise the
        // tiles that flipped their active state.
        let damaged = if self.layout_dirty {
            Some((0, 0, w, h))
        } else if self.active_title != self.last_active_title {
            let mut dirty = None;
            for entry in &self.entries {
                if entry.title == self.active_title || entry.title == self.last_active_title {
                    dirty = Some(rect_union(
                        dirty,
                        (
                            entry.x as u32,
                            PADDING as u32,
                            self.tilesize as u32,
                            self.tilesize as u32,
                        ),
                    ));
                }
            }
            dirty
        } else {
            None
        };
        self.layout_dirty = false;
        self.needs_redraw = false;
        self.last_active_title = self.active_title.clone();
        let Some(damaged) = damaged else {
            return; // nothing changed since the last commit
        };

        if self.renderer.is_none() {
            let display_ptr = conn.display().id().as_ptr().cast::<c_void>();
            // Safety: the display handle is the live Wayland connection's.
            self.renderer =
                Some(unsafe { GlesRenderer::new(display_ptr) }.expect("create the GLES renderer"));
        }
        let (Some(renderer), Some(surface)) = (&mut self.renderer, &self.surface) else {
            return;
        };
        // One texture per size; the first upload is the whole frame, later
        // redraws upload only the damaged tiles.
        let created = match &self.texture {
            Some(texture) if texture.size == (w, h) => false,
            _ => match renderer.create_texture_rgba((w, h), frame) {
                Ok(texture) => {
                    self.texture = Some(texture);
                    true
                }
                Err(_) => return,
            },
        };
        let Some(texture) = &self.texture else {
            return;
        };
        if created {
            if renderer.upload_texture_rgba(texture, frame).is_err() {
                return;
            }
        } else if renderer
            .upload_texture_region_rgba(texture, frame, damaged.0, damaged.1, damaged.2, damaged.3)
            .is_err()
        {
            return;
        }
        if let Err(error) = self.presenter.present(renderer, surface, texture, (w, h)) {
            eprintln!("dock: present failed: {error}");
        }
    }

    /// Activates the app whose tile was clicked.
    fn click_tile(&mut self, x: i32, y: i32) {
        let Some(entry) = self.entries.iter().find(|e| {
            x >= e.x && x < e.x + self.tilesize && y >= PADDING && y < PADDING + self.tilesize
        }) else {
            return;
        };
        let Some(handle) = &entry.mgmt_handle else {
            return;
        };
        let Some(seat) = self.seat.seats().next() else {
            return;
        };
        eprintln!("dock: activate '{}'", entry.title);
        handle.activate(&seat);
    }

    /// Handles a foreign-toplevel-list `toplevel` event: track the new window.
    fn add_toplevel(&mut self, handle: ExtForeignToplevelHandleV1) {
        self.entries.push(DockEntry {
            list_handle: handle,
            mgmt_handle: None,
            title: String::new(),
            x: 0,
        });
        self.layout_tiles();
        self.needs_redraw = true;
        self.layout_dirty = true;
    }

    /// Removes a tracked window.
    fn remove_toplevel(&mut self, handle: &ExtForeignToplevelHandleV1) {
        self.entries.retain(|entry| entry.list_handle != *handle);
        self.layout_tiles();
        self.needs_redraw = true;
        self.layout_dirty = true;
    }

    /// Re-reads `dock.domain` (tile size) and `window.domain` (active app).
    fn check_stores(&mut self, conn: &Connection, _qh: &QueueHandle<Self>) {
        let dock_modified = std::fs::metadata(&self.dock_store)
            .and_then(|m| m.modified())
            .ok();
        if dock_modified != self.last_dock_modified {
            self.last_dock_modified = dock_modified;
            if let Ok(text) = std::fs::read_to_string(&self.dock_store)
                && let Ok(store) = parse_dock(&text)
                && store.tilesize != self.tilesize
            {
                self.tilesize = store.tilesize;
                self.layout_tiles();
                self.needs_redraw = true;
                self.layout_dirty = true;
            }
        }

        let active_modified = std::fs::metadata(&self.active_store)
            .and_then(|m| m.modified())
            .ok();
        if active_modified != self.last_active_modified {
            self.last_active_modified = active_modified;
            if let Ok(text) = std::fs::read_to_string(&self.active_store)
                && let Ok(title) = parse_active(&text)
            {
                self.active_title = title;
                self.needs_redraw = true;
            }
        }

        if self.needs_redraw {
            self.render(conn);
            self.needs_redraw = false;
        }
    }
}

impl ProvidesRegistryState for Dock {
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

impl OutputHandler for Dock {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output
    }

    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}

    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}

    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl SeatHandler for Dock {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat
    }

    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}

    fn new_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: wl_seat::WlSeat,
        _: Capability,
    ) {
    }

    fn remove_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: wl_seat::WlSeat,
        _: Capability,
    ) {
    }

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl PointerHandler for Dock {
    fn pointer_frame(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        for event in events {
            if let PointerEventKind::Press {
                button,
                serial: _,
                time: _,
            } = event.kind
                && button == BTN_LEFT
            {
                self.click_tile(event.position.0 as i32, event.position.1 as i32);
            }
        }
    }
}

impl CompositorHandler for Dock {
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

impl LayerShellHandler for Dock {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &LayerSurface) {
        eprintln!("dock: layer surface closed by the compositor");
        std::process::exit(0);
    }

    fn configure(
        &mut self,
        conn: &Connection,
        qh: &QueueHandle<Self>,
        _: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _serial: u32,
    ) {
        self.configure(conn, qh, configure);
    }
}

// --- foreign-toplevel-list (client) ---------------------------------------

impl Dispatch<ExtForeignToplevelListV1, (), Dock> for Dock {
    fn event(
        state: &mut Self,
        _: &ExtForeignToplevelListV1,
        event: <ExtForeignToplevelListV1 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let FtlListEvent::Toplevel { toplevel } = event {
            state.add_toplevel(toplevel);
        }
    }

    event_created_child!(Dock, ExtForeignToplevelListV1, [
        sctk::reexports::protocols::ext::foreign_toplevel_list::v1::client::ext_foreign_toplevel_list_v1::EVT_TOPLEVEL_OPCODE => (ExtForeignToplevelHandleV1, ())
    ]);
}

impl Dispatch<ExtForeignToplevelHandleV1, (), Dock> for Dock {
    fn event(
        state: &mut Self,
        proxy: &ExtForeignToplevelHandleV1,
        event: <ExtForeignToplevelHandleV1 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            FtlHandleEvent::Title { title } => {
                if let Some(entry) = state.entries.iter_mut().find(|e| &e.list_handle == proxy) {
                    entry.title = title;
                    state.needs_redraw = true;
                }
            }
            FtlHandleEvent::AppId { .. } => {}
            FtlHandleEvent::Done => state.needs_redraw = true,
            FtlHandleEvent::Closed => state.remove_toplevel(proxy),
            FtlHandleEvent::Identifier { .. } => {}
            _ => {}
        }
    }
}

// --- zwlr_foreign_toplevel_management (client) ------------------------------

impl Dispatch<ZwlrForeignToplevelManagerV1, (), Dock> for Dock {
    fn event(
        state: &mut Self,
        _: &ZwlrForeignToplevelManagerV1,
        event: <ZwlrForeignToplevelManagerV1 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let MgmtListEvent::Toplevel { toplevel } = event {
            // Pair the management handle to the list entry with the same title
            // position: they arrive in the same order.
            if let Some(entry) = state.entries.last_mut() {
                entry.mgmt_handle = Some(toplevel);
            }
        }
    }

    event_created_child!(Dock, ZwlrForeignToplevelManagerV1, [
        sctk::reexports::protocols_wlr::foreign_toplevel::v1::client::zwlr_foreign_toplevel_manager_v1::EVT_TOPLEVEL_OPCODE => (ZwlrForeignToplevelHandleV1, ())
    ]);
}

impl Dispatch<ZwlrForeignToplevelHandleV1, (), Dock> for Dock {
    fn event(
        _: &mut Self,
        _: &ZwlrForeignToplevelHandleV1,
        _: <ZwlrForeignToplevelHandleV1 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        // The compositor sends title/app_id/done/closed; we already track the
        // window identity via the list.
    }
}

// --- viewport / fractional scale -------------------------------------------

impl Dispatch<WpViewporter, (), Dock> for Dock {
    fn event(
        _: &mut Self,
        _: &WpViewporter,
        _: <WpViewporter as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        unreachable!("wp_viewporter has no events")
    }
}

impl Dispatch<WpViewport, WlSurface, Dock> for Dock {
    fn event(
        _: &mut Self,
        _: &WpViewport,
        _: <WpViewport as Proxy>::Event,
        _: &WlSurface,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        unreachable!("wp_viewport has no events")
    }
}

impl Dispatch<WpFractionalScaleManagerV1, (), Dock> for Dock {
    fn event(
        _: &mut Self,
        _: &WpFractionalScaleManagerV1,
        _: <WpFractionalScaleManagerV1 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        unreachable!("wp_fractional_scale_manager_v1 has no events")
    }
}

impl Dispatch<WpFractionalScaleV1, WlSurface, Dock> for Dock {
    fn event(
        state: &mut Self,
        _: &WpFractionalScaleV1,
        event: <WpFractionalScaleV1 as Proxy>::Event,
        _: &WlSurface,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let FractionalScaleEvent::PreferredScale { scale } = event {
            state.scale = scale as f64 / 120.0;
        }
    }
}

impl Dispatch<WlBuffer, (), Dock> for Dock {
    fn event(
        _: &mut Self,
        _: &WlBuffer,
        _: <WlBuffer as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        // `wl_buffer.release`: the compositor is done with this buffer.
    }
}

delegate_compositor!(Dock);
delegate_output!(Dock);
delegate_registry!(Dock);
delegate_seat!(Dock);

delegate_dispatch!(Dock: [zwlr_layer_shell_v1::ZwlrLayerShellV1: GlobalData] => LayerShell);
delegate_dispatch!(Dock: [zwlr_layer_surface_v1::ZwlrLayerSurfaceV1: LayerSurfaceData] => LayerShell);

// --- helpers ---------------------------------------------------------------

/// The state/data directory: `YUKI_DATA_DIR`, else `$XDG_DATA_HOME/yuki`.
fn data_dir() -> PathBuf {
    std::env::var_os("YUKI_DATA_DIR").map_or_else(
        || {
            let base = std::env::var_os("XDG_DATA_HOME")
                .map(PathBuf::from)
                .or_else(|| {
                    std::env::var_os("HOME")
                        .map(PathBuf::from)
                        .map(|h| h.join(".local/share"))
                })
                .unwrap_or_else(|| PathBuf::from(".local/share"));
            base.join("yuki")
        },
        PathBuf::from,
    )
}

/// Parses the dock options from `dock.domain`.
fn parse_dock(text: &str) -> anyhow::Result<DockOptions> {
    #[derive(serde::Deserialize)]
    struct Store {
        #[serde(default)]
        tilesize: Option<i32>,
    }
    let store: Store = toml::from_str(text)?;
    Ok(DockOptions {
        tilesize: store.tilesize.unwrap_or(DEFAULT_TILESIZE),
    })
}

/// Parses the active window title from `window.domain`.
fn parse_active(text: &str) -> anyhow::Result<String> {
    #[derive(serde::Deserialize)]
    struct Store {
        #[serde(default)]
        active: Active,
    }
    #[derive(Default, serde::Deserialize)]
    struct Active {
        #[serde(default)]
        title: String,
    }
    let store: Store = toml::from_str(text)?;
    Ok(store.active.title)
}

#[derive(Debug, Clone, Copy)]
struct DockOptions {
    tilesize: i32,
}

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

/// Bounding box of `a` and `b` (a may be `None`).
fn rect_union(a: Option<(u32, u32, u32, u32)>, b: (u32, u32, u32, u32)) -> (u32, u32, u32, u32) {
    let Some(a) = a else {
        return b;
    };
    let x1 = a.0.min(b.0);
    let y1 = a.1.min(b.1);
    let x2 = (a.0 + a.2).max(b.0 + b.2);
    let y2 = (a.1 + a.3).max(b.1 + b.3);
    (x1, y1, x2 - x1, y2 - y1)
}
