use std::iter;
use std::time::{Duration, Instant};

use smithay::backend::input::InputTime;
use smithay::backend::renderer::element::solid::SolidColorRenderElement;
use smithay::backend::renderer::element::surface::WaylandSurfaceRenderElement;
use smithay::backend::renderer::element::texture::TextureRenderElement;
use smithay::backend::renderer::element::utils::CropRenderElement;
use smithay::backend::renderer::gles::{GlesRenderer, GlesTexture};
use smithay::desktop::utils::send_frames_surface_tree;
use smithay::desktop::{PopupManager, Space, Window};
use smithay::input::keyboard::Keycode;
use smithay::input::pointer::CursorImageStatus;
use smithay::input::{Seat, SeatState};
use smithay::output::Output;
use smithay::reexports::calloop::timer::{TimeoutAction, Timer};
use smithay::reexports::calloop::{LoopHandle, LoopSignal};
use smithay::reexports::wayland_server::DisplayHandle;
use smithay::reexports::wayland_server::backend::ClientData;
use smithay::render_elements;
use smithay::utils::{Logical, Point, Rectangle, Size};
use smithay::wayland::compositor::{CompositorClientState, CompositorState};
use smithay::wayland::selection::data_device::DataDeviceState;
use smithay::wayland::shell::xdg::XdgShellState;
use smithay::wayland::shm::ShmState;

use crate::drm::Drm;
use crate::effects::{
    Backdrop, Badge, Clipped, Effects, Lines, Name, Placement, Shadow, Title, round_edges,
};
use crate::winit::Winit;
use crate::workspaces::desktops::{MAX_DESKTOPS, Slide, desktop_x, gap, gap_x, stride};
use crate::workspaces::expansion::strip::{Hit, add_centre, close_centre, pill, split_shares};
use crate::workspaces::expansion::{Expansion, Lifted, of_app};
use crate::workspaces::full_screen::{
    Growth, cover, desktop_number, divider, full_screen_window, split_windows,
};
use crate::workspaces::swipe::Swipe;

render_elements! {
    pub Element<=GlesRenderer>;
    Surface=WaylandSurfaceRenderElement<GlesRenderer>,
    Clipped=Clipped,
    Thumbnail=CropRenderElement<Clipped>,
    Lines=Lines,
    Shadow=Shadow,
    Solid=SolidColorRenderElement,
    Badge=Badge,
    Backdrop=Backdrop,
    Text=CropRenderElement<TextureRenderElement<GlesTexture>>,
}

pub struct Row<'a> {
    pub desktops: &'a [Space<Window>],
    pub current: usize,
    pub slide: Option<&'a Slide>,
    pub growth: Option<&'a Growth>,
    pub expansion: &'a Expansion,
    pub pointer: Point<f64, Logical>,
}

pub fn elements(
    renderer: &mut GlesRenderer,
    effects: &Effects,
    row: Row,
    output: &Output,
    frame: &mut Vec<Element>,
) {
    let Row {
        desktops,
        current,
        slide,
        growth,
        expansion,
        pointer,
    } = row;
    let position = slide.map_or(current as f64, |slide| slide.position);
    let carried = slide.and_then(|slide| slide.carried.as_ref());
    let vacated = slide.and_then(|slide| slide.vacated);
    let scale = output.current_scale().fractional_scale();
    let width = output.current_mode().map_or(0, |mode| mode.size.w);
    let height = output.current_mode().map_or(0, |mode| mode.size.h);
    let stride = stride(width, scale);
    let output_geometry = desktops[current].output_geometry(output);
    let lifted = output_geometry
        .and_then(|geometry| expansion.lifted(desktops, geometry.size.to_f64(), pointer));
    if let Some(Lifted {
        window,
        entry,
        frame: geometry,
        drawn: (top_left, factor),
    }) = lifted
    {
        let main = desktops[current].elements().next_back() == Some(window);
        effects.push_window(
            renderer,
            window,
            Placement {
                geometry: Rectangle::new(top_left.to_i32_round(), geometry.size),
                desktop_offset: (
                    entry.map_or(0, |entry| desktop_x(entry, position, stride)),
                    0,
                )
                    .into(),
                expansion: factor,
                ringed: false,
                title: None,
            },
            scale,
            main,
            frame,
        );
    }
    if let (Some(top), Some(geometry), None) =
        (expansion.strip_top(), output_geometry, &expansion.app)
    {
        push_strip(
            renderer,
            effects,
            &row,
            top,
            geometry.size.to_f64(),
            scale,
            frame,
        );
    }
    let lifted = lifted.map(|lifted| lifted.window);
    let hovered = (expansion.open
        && expansion.progress >= 1.0
        && slide.is_none()
        && expansion.dragged.is_none()
        && !expansion.flying())
    .then(|| expansion.window_at(desktops, pointer))
    .flatten()
    .map(|(window, _)| window);
    let previewed = expansion.previewed.as_ref();
    let mut no_windows_shown = false;
    let others = desktops
        .iter()
        .enumerate()
        .filter(|(index, _)| *index != current);
    for (index, desktop) in iter::once((current, &desktops[current])).chain(others) {
        if desktop.outputs().next().is_none() {
            continue;
        }
        let slot = index + usize::from(vacated.is_some_and(|vacated| index >= vacated));
        let strip_x = desktop_x(slot, position, stride);
        if !no_windows_shown
            && expansion.app.is_none()
            && expansion.open
            && strip_x.abs() < width
            && desktop.elements().next().is_none()
            && let Some(geometry) = desktop.output_geometry(output)
        {
            let mut area = round_edges(expansion.area(geometry).to_physical(scale));
            area.loc.x += strip_x;
            let opacity = expansion.progress.clamp(0.0, 1.0) as f32;
            effects.push_no_windows(renderer, area, opacity, scale, frame);
            no_windows_shown = true;
        }
        let in_front = previewed.filter(|_| index == expansion.desktop);
        let windows = in_front.into_iter().chain(
            desktop
                .elements()
                .rev()
                .filter(|window| Some(*window) != in_front),
        );
        for window in windows {
            if Some(window) == lifted || !of_app(window, expansion.app.as_ref()) {
                continue;
            }
            let grown = growth.filter(|growth| growth.window == *window);
            let x = if Some(window) == carried || grown.is_some() {
                0
            } else {
                strip_x
            };
            if x.abs() >= width {
                continue;
            }
            let Some(geometry) = desktop.element_geometry(window) else {
                continue;
            };
            let main = index == current && desktop.elements().next_back() == Some(window);
            let (geometry, factor) = match grown {
                Some(growth) => cover(growth.drawn(geometry.to_f64()), geometry.size),
                None => {
                    let (loc, factor) = expansion.drawn(window, geometry);
                    (Rectangle::new(loc.to_i32_round(), geometry.size), factor)
                }
            };
            let moving = expansion.preview_moving(window);
            let still = expansion.at_rest() && !moving;
            effects.push_window(
                renderer,
                window,
                Placement {
                    geometry,
                    desktop_offset: (x, 0).into(),
                    expansion: factor,
                    ringed: Some(window) == hovered && !moving,
                    title: if expansion.app.is_some() {
                        still.then_some(Title::Below)
                    } else {
                        (Some(window) == hovered && still).then_some(Title::Centred)
                    },
                },
                scale,
                main,
                frame,
            );
        }
        if expansion.progress <= 0.0
            && strip_x.abs() < width
            && full_screen_window(desktop).is_some()
        {
            if let Some((left, tiles_apart)) = desktop
                .output_geometry(output)
                .and_then(|geometry| divider(desktop, geometry))
            {
                let mut tiles_apart = tiles_apart.to_f64().to_physical(scale);
                tiles_apart.loc.x += f64::from(strip_x);
                let active = desktop.elements().next_back() == Some(left);
                effects.push_split_handle(tiles_apart, active, scale, frame);
            }
            let area = Rectangle::new((strip_x, 0).into(), (width, height).into());
            effects.push_backdrop(index, area, true, frame);
        }
    }
    if let Some(vacated) = vacated
        && expansion.progress <= 0.0
    {
        let area = Rectangle::new(
            (desktop_x(vacated, position, stride), 0).into(),
            (width, height).into(),
        );
        effects.push_backdrop(desktops.len(), area, true, frame);
    }
    if let Some(x) = gap_x(position, width, scale) {
        effects.push_gap(
            Rectangle::new((x, 0).into(), (gap(scale), height).into()),
            frame,
        );
    }
}

fn split_name<'a>(left: &'a Window, right: &'a Window) -> Name<'a> {
    Name::Split(
        left.toplevel().unwrap().wl_surface(),
        right.toplevel().unwrap().wl_surface(),
    )
}

fn push_strip(
    renderer: &mut GlesRenderer,
    effects: &Effects,
    row: &Row,
    top: f64,
    output: Size<f64, Logical>,
    scale: f64,
    frame: &mut Vec<Element>,
) {
    let strip = &row.expansion.strip;
    let count = row.desktops.len();
    let shift = Point::from((0.0, top));
    let on_screen = |rectangle: Rectangle<f64, Logical>| {
        round_edges(Rectangle::new(rectangle.loc + shift, rectangle.size).to_physical(scale))
    };
    let bar = on_screen(Rectangle::from_size(Size::from((output.w, strip.height()))));
    let placed = |index: usize| strip.placed(index, count, output, row.pointer.x);
    let ring = (strip.grown && strip.motion.is_none()).then(|| on_screen(placed(row.current)));
    let close = strip.close().map(|index| {
        let pressed = row.expansion.pressed
            && strip.hit(row.pointer - shift, count, output) == Some(Hit::Close(index));
        let centre = close_centre(strip.thumbnail(index, count, output)) + shift;
        (centre.to_physical(scale), pressed)
    });
    effects.push_strip_controls(ring, close, scale, frame);
    let is_desktop = |index: usize| full_screen_window(&row.desktops[index]).is_none();
    let split_preview = row.expansion.split_preview(row.desktops);
    let split_at = |index: usize| split_preview.filter(|(at, ..)| *at == index);
    let cell = |index: usize| {
        on_screen(if strip.grown {
            placed(index)
        } else {
            pill(index, count, output)
        })
    };
    let ghost = row.expansion.ghost.as_ref().and_then(|ghost| {
        let index = row
            .desktops
            .iter()
            .position(|entry| full_screen_window(entry) == Some(&ghost.app))?;
        let (left, right) = ghost.side.left_right(&ghost.window, &ghost.app);
        Some((
            MAX_DESKTOPS,
            cell(index),
            split_name(left, right),
            ghost.opacity as f32,
        ))
    });
    let cells = (0..count).map(|index| {
        let entry = &row.desktops[index];
        let joined = split_at(index).and_then(|(_, window, side, _)| {
            Some(side.left_right(window, full_screen_window(entry)?))
        });
        let name = joined
            .or_else(|| split_windows(entry))
            .map(|(left, right)| split_name(left, right))
            .or_else(|| {
                full_screen_window(entry)
                    .map(|window| Name::App(window.toplevel().unwrap().wl_surface()))
            })
            .unwrap_or_else(|| Name::Desktop(desktop_number(index, is_desktop)));
        (index, cell(index), name, 1.0)
    });
    effects.push_desktop_names(
        renderer,
        ghost.into_iter().chain(cells),
        strip.grown,
        bar,
        scale,
        frame,
    );
    if strip.grown {
        let moving = strip.moving.map(|(from, _)| from);
        let others = (0..count).filter(|&index| Some(index) != moving);
        for index in moving.into_iter().chain(others) {
            let desktop = &row.desktops[index];
            let thumbnail = placed(index);
            let factor = thumbnail.size.h / output.h;
            let whole = on_screen(thumbnail);
            let mut shown = whole;
            if let Some((_, _, side, left)) = split_at(index) {
                let (left_share, right_share) = split_shares(thumbnail, left);
                let (placeholder, rest) = side.left_right(left_share, right_share);
                shown = on_screen(rest);
                effects.push_split_placeholder(on_screen(placeholder), bar, scale, frame);
            }
            let Some(clip) = shown.intersection(bar) else {
                continue;
            };
            for window in desktop.elements().rev() {
                if let Some(geometry) = desktop.element_geometry(window) {
                    let placement = Placement {
                        geometry,
                        desktop_offset: whole.loc,
                        expansion: factor,
                        ringed: false,
                        title: None,
                    };
                    effects.push_thumbnail(renderer, window, placement, clip, scale, frame);
                }
            }
            effects.push_backdrop(index, clip, !is_desktop(index), frame);
        }
    }
    let preview = strip
        .preview(output)
        .and_then(|preview| on_screen(preview).intersection(bar));
    let add = (add_centre(strip.growth, output) + shift).to_physical(scale);
    let pill = (!strip.grown).then(|| on_screen(pill(row.current, count, output)));
    effects.push_strip_marks(add, preview, pill, scale, frame);
    effects.push_strip_bar(bar, row.expansion.open, scale, frame);
}

pub enum Backend {
    Drm(Box<Drm>),
    Winit(Box<Winit>),
}

pub struct State {
    pub display_handle: DisplayHandle,
    pub loop_handle: LoopHandle<'static, State>,
    pub loop_signal: LoopSignal,
    pub start_time: Instant,
    pub backend: Backend,
    pub output: Output,
    pub desktops: Vec<Space<Window>>,
    pub current_desktop: usize,
    pub slide: Option<Slide>,
    pub swipe: Option<Swipe>,
    pub growth: Option<Growth>,
    pub divider_press: Option<(InputTime, Point<f64, Logical>)>,
    pub expansion: Expansion,
    pub popups: PopupManager,
    pub compositor_state: CompositorState,
    pub xdg_shell_state: XdgShellState,
    pub shm_state: ShmState,
    pub seat_state: SeatState<State>,
    pub data_device_state: DataDeviceState,
    pub seat: Seat<State>,
    pub needs_redraw: bool,
    pub frame_pending: bool,
    pub suppressed_keys: Vec<Keycode>,
    pub cursor_status: CursorImageStatus,
    pub frame_elements: Vec<Element>,
}

#[derive(Default)]
pub struct ClientState {
    pub compositor_state: CompositorClientState,
}

impl ClientData for ClientState {}

impl State {
    pub fn redraw_if_needed(&mut self) {
        if !self.needs_redraw || self.frame_pending {
            return;
        }
        match &self.backend {
            Backend::Drm(drm) if !drm.is_active() => {}
            Backend::Drm(_) => self.render(),
            Backend::Winit(winit) => {
                self.frame_pending = true;
                winit.request_redraw();
            }
        }
    }

    pub fn render(&mut self) {
        self.needs_redraw = false;
        let now = self.start_time.elapsed();
        self.advance_slide(now);
        self.advance_growth(now);
        self.advance_expansion(now);
        self.frame_pending = true;
        let pointer = self.seat.get_pointer().unwrap().current_location();
        let row = Row {
            desktops: &self.desktops,
            current: self.current_desktop,
            slide: self.slide.as_ref(),
            growth: self.growth.as_ref(),
            expansion: &self.expansion,
            pointer,
        };
        let rendered = match &mut self.backend {
            Backend::Drm(drm) => {
                drm.push_cursor(
                    &self.output,
                    &self.cursor_status,
                    pointer,
                    &mut self.frame_elements,
                );
                drm.render(row, &self.output, &mut self.frame_elements)
            }
            Backend::Winit(winit) => winit.render(row, &self.output, &mut self.frame_elements),
        }
        .unwrap_or_else(|error| {
            tracing::warn!(%error, "render failed");
            self.needs_redraw = true;
            false
        });
        self.frame_elements.clear();
        if !rendered {
            self.finish_frame_after_one_refresh();
        } else if let Backend::Winit(_) = self.backend {
            self.frame_done();
        }
    }

    fn finish_frame_after_one_refresh(&self) {
        let frame_interval = self
            .output
            .current_mode()
            .map(|mode| Duration::from_secs_f64(1000.0 / mode.refresh as f64))
            .unwrap_or_default();
        if let Err(error) =
            self.loop_handle
                .insert_source(Timer::from_duration(frame_interval), |_, _, state| {
                    state.frame_done();
                    TimeoutAction::Drop
                })
        {
            tracing::error!(%error, "frame timer failed");
        }
    }

    pub fn frame_done(&mut self) {
        self.frame_pending = false;
        let time = self.start_time.elapsed();
        for window in self
            .desktops
            .iter()
            .filter(|desktop| desktop.outputs().next().is_some())
            .flat_map(Space::elements)
        {
            window.send_frame(&self.output, time, Some(Duration::ZERO), |_, _| {
                Some(self.output.clone())
            });
        }
        if let (Backend::Drm(_), CursorImageStatus::Surface(surface)) =
            (&self.backend, &self.cursor_status)
        {
            send_frames_surface_tree(surface, &self.output, time, Some(Duration::ZERO), |_, _| {
                Some(self.output.clone())
            });
        }
    }
}
