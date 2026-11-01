//! The system dialogs of the ❄ menu (14b-2): confirm a power action, force
//! quit an app and «Acerca de este equipo».
//!
//! Each one runs as `malaysia-shell <component>`: a centred Overlay layer that
//! owns the keyboard, like the launcher. Esc (or Cancelar) closes it; Enter
//! runs the default button.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;

use barrier_kit::controls::Button;
use barrier_kit::platform::{Decorations, WindowControls, WindowOptions, run_window};
use barrier_kit::shell::{Anchors, KeyboardInteractivity, Layer, LayerSpec, Toplevels, run_layer};
use barrier_kit::titlebar::window_drag_handler;
use barrier_kit::traffic::TrafficLights;
use freya_components::svg_viewer::SvgViewer;
use freya_components::theming::hooks::use_init_theme;
use freya_core::integration::AppComponent;
use freya_core::prelude::*;
use keyboard_types::{Key, NamedKey};
use torin::prelude::{Alignment, Content, Direction, Gaps, Size};
use vow_design::{
    ButtonKind, CALLOUT, CAPTION, DIALOG, DIALOG_PADDING, DISPLAY, HEADLINE, Rgba, TEXT, TITLEBAR,
};

use crate::desktop::{self, DesktopEntry};
use crate::launcher::{IconData, app_icon, load_icon};
use crate::menubar::{find_entry, resolve_app_name};
use crate::power::{self, PowerAction};

/// Dialog surfaces, in logical px. The panel fills the layer.
const CONFIRM_SIZE: (u32, u32) = (380, 160);
const FORCE_QUIT_SIZE: (u32, u32) = (420, 400);
/// «Acerca de este equipo» takes its measurements from eqsh's About window
/// (eq-desktop/eqsh, `About.qml`, Apache-2.0; only the numbers, no code):
/// 335×550 with the titlebar, the drawing in a 175×125 box 75 px from the top,
/// the title 20 px below it, label and value columns of 120 and 160 px with a
/// 5 px gap and 5 px between rows, the button 20 px below the rows and the
/// footer 20 px above the bottom edge.
const ABOUT_SIZE: (u32, u32) = (335, 550);
const ABOUT_DRAWING_TOP: f32 = 75.0;
/// The 175×125 box, keeping the SVG's 1024×680 aspect.
const ABOUT_DRAWING: (f32, f32) = (175.0, 116.0);
const ABOUT_DRAWING_BOX_HEIGHT: f32 = 125.0;
const ABOUT_GAP: f32 = 20.0;
const ABOUT_LABEL_WIDTH: f32 = 120.0;
const ABOUT_VALUE_WIDTH: f32 = 160.0;
const ABOUT_ROW_GAP: f32 = 5.0;
const ABOUT_FOOTER_GAP: f32 = 8.0;
const ROW_HEIGHT: f32 = 32.0;
const ICON_SIZE: f32 = 22.0;

fn rgba(color: Rgba) -> Color {
    let [r, g, b, a] = color.to_rgba8();
    Color::from_argb(a, r, g, b)
}

fn spec(namespace: &str, size: (u32, u32)) -> LayerSpec {
    LayerSpec {
        namespace: namespace.to_string(),
        layer: Layer::Overlay,
        anchors: Anchors::default(),
        exclusive_zone: 0,
        keyboard: KeyboardInteractivity::Exclusive,
        size: Some(size),
    }
}

fn close() {
    std::process::exit(0);
}

/// The Tengen dialog panel: raised surface, dialog radius and padding.
fn panel() -> Rect {
    let resolved = barrier_kit::theme::resolved();
    rect()
        .width(Size::fill())
        .height(Size::fill())
        .background(rgba(resolved.surfaces.raised))
        .corner_radius(DIALOG.corner_radius as f32)
        .padding(Gaps::new(
            DIALOG_PADDING,
            DIALOG_PADDING,
            DIALOG_PADDING,
            DIALOG_PADDING,
        ))
        .direction(Direction::Vertical)
        .spacing(DIALOG_PADDING / 2.0)
}

fn title(text: &str) -> Label {
    let palette = vow_appearance::palette(barrier_kit::theme::resolved().scheme);
    label()
        .text(text.to_string())
        .font_size(HEADLINE.size as f32)
        .font_weight(FontWeight::BOLD)
        .color(rgba(palette.text.focused))
}

fn body(text: &str) -> Label {
    let palette = vow_appearance::palette(barrier_kit::theme::resolved().scheme);
    label()
        .text(text.to_string())
        .font_size(TEXT.size as f32)
        .color(rgba(palette.text.unfocused))
}

/// Cancelar plus the default (filled) button, aligned to the right.
fn buttons(default_label: &str, on_default: impl Fn() + 'static, enabled: bool) -> Rect {
    rect()
        .width(Size::fill())
        .height(Size::auto())
        .direction(Direction::Horizontal)
        .main_align(Alignment::End)
        .spacing(DIALOG_PADDING / 2.0)
        .child(
            Button::new()
                .on_press(|_| close())
                .child(label().text("Cancelar")),
        )
        .child(
            Button::new()
                .filled()
                .enabled(enabled)
                .on_press(move |_| on_default())
                .child(label().text(default_label.to_string())),
        )
}

/// What a confirmation shows for each action.
fn confirm_text(action: PowerAction) -> (&'static str, &'static str, &'static str) {
    match action {
        PowerAction::Restart => (
            "¿Seguro que quieres reiniciar el equipo ahora?",
            "Las apps abiertas se cerrarán.",
            "Reiniciar",
        ),
        PowerAction::ShutDown => (
            "¿Seguro que quieres apagar el equipo ahora?",
            "Las apps abiertas se cerrarán.",
            "Apagar",
        ),
        PowerAction::LogOut => (
            "¿Seguro que quieres cerrar la sesión?",
            "Las apps abiertas se cerrarán.",
            "Cerrar sesión",
        ),
        PowerAction::Suspend => ("¿Suspender el equipo?", "", "Suspender"),
    }
}

struct ConfirmApp {
    action: PowerAction,
}

impl App for ConfirmApp {
    fn render(&self) -> impl IntoElement {
        use_init_theme(barrier_kit::theme::shell);
        let action = self.action;
        let confirm = move || {
            power::run(action);
            close();
        };
        let (question, detail, button) = confirm_text(action);
        panel()
            .on_global_key_down(move |event: Event<KeyboardEventData>| match event.key {
                Key::Named(NamedKey::Escape) => close(),
                Key::Named(NamedKey::Enter) => confirm(),
                _ => {}
            })
            .child(title(question))
            .child(body(detail))
            .child(rect().width(Size::fill()).height(Size::flex(1.0)))
            .child(buttons(button, confirm, true))
    }
}

/// `malaysia-shell power <restart|shutdown|logout>`.
pub fn run_confirm(arg: Option<&str>) -> anyhow::Result<()> {
    let action = arg
        .and_then(PowerAction::from_arg)
        .ok_or_else(|| anyhow::anyhow!("usage: malaysia-shell power <restart|shutdown|logout>"))?;
    run_layer(
        AppComponent::new(ConfirmApp { action }),
        spec("malaysia-dialog", CONFIRM_SIZE),
    )
    .map_err(|error| anyhow::anyhow!("{error}"))
}

/// One app in the Force Quit list: the first toplevel of each `app_id`, since
/// killing the process closes all its windows.
#[derive(Clone, Debug, PartialEq)]
struct RunningApp {
    index: usize,
    app_id: String,
}

fn running_apps(app_ids: &[String]) -> Vec<RunningApp> {
    let mut seen = Vec::new();
    let mut apps = Vec::new();
    for (index, app_id) in app_ids.iter().enumerate() {
        if app_id.is_empty() || seen.contains(app_id) {
            continue;
        }
        seen.push(app_id.clone());
        apps.push(RunningApp {
            index,
            app_id: app_id.clone(),
        });
    }
    apps
}

type IconCache = Rc<RefCell<HashMap<String, Option<IconData>>>>;

struct ForceQuitApp {
    entries: Rc<Vec<DesktopEntry>>,
    icons: IconCache,
    selected: State<usize>,
}

impl ForceQuitApp {
    fn icon(&self, app_id: &str) -> Option<IconData> {
        self.icons
            .borrow_mut()
            .entry(app_id.to_string())
            .or_insert_with(|| {
                find_entry(app_id, &self.entries).and_then(|entry| load_icon(entry.icon.as_deref()))
            })
            .clone()
    }
}

impl App for ForceQuitApp {
    fn render(&self) -> impl IntoElement {
        use_init_theme(barrier_kit::theme::shell);
        let toplevels = Toplevels::get();
        let app_ids: Vec<String> = toplevels
            .list
            .read()
            .iter()
            .map(|window| window.app_id.clone())
            .collect();
        let apps = running_apps(&app_ids);
        let selected = (*self.selected.read()).min(apps.len().saturating_sub(1));
        let resolved = barrier_kit::theme::resolved();
        let text = rgba(vow_appearance::palette(resolved.scheme).text.focused);
        let mut highlight = resolved.accent;
        highlight.a = 0.18;

        let force_quit = {
            let apps = apps.clone();
            let toplevels = toplevels.clone();
            move || {
                if let Some(app) = apps.get(selected) {
                    tracing::info!(app_id = %app.app_id, "force quit requested");
                    toplevels.force_quit(app.index);
                }
            }
        };
        let rows = apps.iter().enumerate().map(|(row, app)| {
            let name = resolve_app_name(Some(&app.app_id), &self.entries);
            let mut selected_state = self.selected;
            rect()
                .width(Size::fill())
                .height(Size::px(ROW_HEIGHT))
                .corner_radius(DIALOG.corner_radius as f32 / 2.0)
                .background(if row == selected {
                    rgba(highlight)
                } else {
                    Color::TRANSPARENT
                })
                .padding(Gaps::new(0.0, 8.0, 0.0, 8.0))
                .direction(Direction::Horizontal)
                .cross_align(Alignment::Center)
                .spacing(8.0)
                .on_press(move |_| selected_state.set(row))
                .child(app_icon(self.icon(&app.app_id).as_ref(), &name, ICON_SIZE))
                .child(label().text(name).font_size(TEXT.size as f32).color(text))
                .into_element()
        });

        let mut selected_keys = self.selected;
        let count = apps.len();
        let force_quit_key = force_quit.clone();
        panel()
            .on_global_key_down(move |event: Event<KeyboardEventData>| match event.key {
                Key::Named(NamedKey::Escape) => close(),
                Key::Named(NamedKey::Enter) => force_quit_key(),
                Key::Named(NamedKey::ArrowDown) if count > 0 => {
                    selected_keys.set((selected + 1) % count);
                }
                Key::Named(NamedKey::ArrowUp) if count > 0 => {
                    selected_keys.set((selected + count - 1) % count);
                }
                _ => {}
            })
            .child(title("Forzar salida de aplicaciones"))
            .child(body(
                "Si una app no responde, selecciónala y pulsa Forzar salida. Se perderán los cambios sin guardar.",
            ))
            .child(
                rect()
                    .width(Size::fill())
                    .height(Size::flex(1.0))
                    .direction(Direction::Vertical)
                    .spacing(2.0)
                    .children(rows),
            )
            .child(buttons("Forzar salida", force_quit, count > 0))
    }
}

/// `malaysia-shell force-quit`, also opened by Alt+Super+Escape.
pub fn run_force_quit() -> anyhow::Result<()> {
    let app = ForceQuitApp {
        entries: Rc::new(desktop::read_entries()),
        icons: Rc::default(),
        selected: State::create_global(0usize),
    };
    run_layer(
        AppComponent::new(app),
        spec("malaysia-dialog", FORCE_QUIT_SIZE),
    )
    .map_err(|error| anyhow::anyhow!("{error}"))
}

/// The laptop drawings of «Acerca de este equipo».
const THINKPAD_SVG: &[u8] = include_bytes!("about/thinkpad.svg");
const GENERIC_SVG: &[u8] = include_bytes!("about/generic.svg");

/// What «Acerca de este equipo» lists, read from the running system. Like
/// macOS it stays short: model, screen size and five rows.
#[derive(Clone, Debug, Default, PartialEq)]
struct AboutInfo {
    model: String,
    thinkpad: bool,
    /// «14 pulgadas» from the built-in panel, or the host name.
    subtitle: String,
    cpu: String,
    memory: String,
    disk: String,
    /// `product_serial` is root-only on most systems: `None` hides the row.
    serial: Option<String>,
}

impl AboutInfo {
    fn read() -> Self {
        let read = |path: &str| std::fs::read_to_string(path).unwrap_or_default();
        let dmi = |key: &str| read(&format!("/sys/class/dmi/id/{key}")).trim().to_string();
        let model = model_name(
            &dmi("sys_vendor"),
            &dmi("product_name"),
            &dmi("product_version"),
            &dmi("product_family"),
        );
        Self {
            thinkpad: model.contains("ThinkPad"),
            model,
            subtitle: built_in_panel_inches()
                .map(|inches| format!("{inches} pulgadas"))
                .unwrap_or_else(|| read("/proc/sys/kernel/hostname").trim().to_string()),
            cpu: cpu_name(&read("/proc/cpuinfo")),
            memory: memory(&read("/proc/meminfo")),
            disk: startup_disk(&read("/proc/mounts")),
            serial: Some(dmi("product_serial")).filter(|serial| is_real(serial)),
        }
    }
}

/// The diagonal of the laptop panel (eDP or LVDS connector) from its EDID.
fn built_in_panel_inches() -> Option<u32> {
    std::fs::read_dir("/sys/class/drm")
        .ok()?
        .flatten()
        .filter(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            name.contains("-eDP-") || name.contains("-LVDS-")
        })
        .find_map(|entry| panel_inches(&std::fs::read(entry.path().join("edid")).ok()?))
}

/// EDID bytes 21 and 22 hold the image size in centimetres.
fn panel_inches(edid: &[u8]) -> Option<u32> {
    let (width, height) = (f64::from(*edid.get(21)?), f64::from(*edid.get(22)?));
    (width > 0.0 && height > 0.0).then(|| (width.hypot(height) / 2.54).round() as u32)
}

/// Firmware placeholders that are not a name.
fn is_real(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    !value.is_empty()
        && ![
            "to be filled",
            "system product",
            "default string",
            "not specified",
            "o.e.m.",
        ]
        .iter()
        .any(|placeholder| lower.contains(placeholder))
}

/// The marketing name of the machine. Lenovo keeps it in `product_version`
/// («ThinkPad L14 Gen 2») and the machine type in `product_name` («20X1…»).
fn model_name(vendor: &str, name: &str, version: &str, family: &str) -> String {
    if vendor.eq_ignore_ascii_case("lenovo")
        && let Some(model) = [version, family].into_iter().find(|value| is_real(value))
    {
        return model.to_string();
    }
    [name, family]
        .into_iter()
        .find(|value| is_real(value))
        .map(str::to_string)
        .unwrap_or_else(|| "Este equipo".to_string())
}

/// The disk that holds `/` as «SSD de 512 GB», following partitions and
/// device-mapper layers (LUKS, LVM) down to the physical disk.
fn startup_disk(mounts: &str) -> String {
    let Some(device) = mounts.lines().find_map(|line| {
        let mut fields = line.split_whitespace();
        let device = fields.next()?;
        (fields.next() == Some("/")).then_some(device)
    }) else {
        return "Desconocido".to_string();
    };
    let Some(name) = std::fs::canonicalize(device)
        .ok()
        .and_then(|path| path.file_name()?.to_str().map(str::to_string))
    else {
        return "Desconocido".to_string();
    };
    let block = Path::new("/sys/class/block").join(physical_disk(&name));
    let read = |file: &str| std::fs::read_to_string(block.join(file)).unwrap_or_default();
    let kind = if read("queue/rotational").trim() == "1" {
        "Disco duro"
    } else {
        "SSD"
    };
    match read("size").trim().parse::<u64>() {
        Ok(sectors) => format!("{kind} de {}", disk_size(sectors * 512)),
        Err(_) => kind.to_string(),
    }
}

/// `nvme0n1p2` → `nvme0n1`, `dm-0` → its first slave, recursively.
fn physical_disk(name: &str) -> String {
    let block = Path::new("/sys/class/block").join(name);
    if let Some(slave) = std::fs::read_dir(block.join("slaves"))
        .ok()
        .and_then(|mut slaves| slaves.next())
        .and_then(|slave| slave.ok()?.file_name().into_string().ok())
    {
        return physical_disk(&slave);
    }
    if block.join("partition").exists()
        && let Some(parent) = std::fs::canonicalize(&block)
            .ok()
            .and_then(|path| path.parent()?.file_name()?.to_str().map(str::to_string))
    {
        return parent;
    }
    name.to_string()
}

/// Bytes in decimal GB or TB, the way disks are sold.
fn disk_size(bytes: u64) -> String {
    let gb = bytes as f64 / 1e9;
    if gb >= 1000.0 {
        format!("{:.1} TB", gb / 1000.0)
    } else {
        format!("{gb:.0} GB")
    }
}

/// The first `model name`, without trademarks, clocks or the integrated GPU:
/// «AMD Ryzen 5 PRO 5650U», «Intel Core i5-1135G7».
fn cpu_name(cpuinfo: &str) -> String {
    cpuinfo
        .lines()
        .find_map(|line| line.strip_prefix("model name"))
        .and_then(|rest| rest.split_once(':'))
        .map(|(_, model)| short_cpu(model))
        .unwrap_or_else(|| "Desconocido".to_string())
}

fn short_cpu(model: &str) -> String {
    let mut model = model
        .replace("(R)", "")
        .replace("(TM)", "")
        .replace("(tm)", "");
    for cut in [" with ", " @ "] {
        if let Some(index) = model.find(cut) {
            model.truncate(index);
        }
    }
    model
        .split_whitespace()
        .filter(|word| {
            !matches!(*word, "CPU" | "Processor")
                && !word.ends_with("-Core")
                && !(word.ends_with("th") && word.trim_end_matches("th").parse::<u32>().is_ok())
                && *word != "Gen"
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Installed memory as sold: `MemTotal` misses what the firmware and GPU
/// reserve, so it is rounded up to an even number of GB («16 GB»).
fn memory(meminfo: &str) -> String {
    meminfo
        .lines()
        .find_map(|line| line.strip_prefix("MemTotal:"))
        .and_then(|rest| rest.split_whitespace().next()?.parse::<f64>().ok())
        .map(|kib| {
            let gib = kib / 1024.0 / 1024.0;
            format!("{} GB", ((gib / 2.0).ceil() * 2.0) as u32)
        })
        .unwrap_or_else(|| "Desconocida".to_string())
}

struct AboutApp {
    info: AboutInfo,
}

impl App for AboutApp {
    fn render(&self) -> impl IntoElement {
        use_init_theme(barrier_kit::theme::shell);
        let controls = WindowControls::get();
        let focused = controls.status.read().activated;
        about_view(&self.info, focused, about_titlebar(focused, controls.fixed))
    }
}

/// A client-side copy of the compositor's titlebar: its 32 px height and its
/// traffic lights at the same place (only close works), but no title, like
/// macOS. The kit's CSD
/// `Titlebar` is the taller client bar (`titlebar-h`), so it is not used here.
fn about_titlebar(focused: bool, fixed: bool) -> Element {
    let (x, ..) = vow_appearance::traffic::group_rect();
    rect()
        .width(Size::fill())
        .height(Size::px(TITLEBAR.thickness as f32))
        .padding(Gaps::new(0.0, 0.0, 0.0, x as f32))
        .direction(Direction::Horizontal)
        .cross_align(Alignment::Center)
        .on_mouse_down(window_drag_handler(fixed))
        .child(
            TrafficLights::new()
                .focused(focused)
                .close_only(true)
                .on_action(|kind| {
                    if kind == ButtonKind::Close {
                        close();
                    }
                }),
        )
        .into_element()
}

/// The window content. The titlebar has no divider, so the whole window takes
/// the titlebar colour: `surfaces.window` with focus, `surfaces.sunken`
/// without, exactly as the compositor's `ChromeTheme` paints its bar.
fn about_view(info: &AboutInfo, focused: bool, titlebar: Element) -> impl IntoElement + use<> {
    let resolved = barrier_kit::theme::resolved();
    let palette = vow_appearance::palette(resolved.scheme);
    let mut rows = vec![
        ("Procesador", info.cpu.clone()),
        ("Memoria", info.memory.clone()),
        ("Disco de arranque", info.disk.clone()),
    ];
    if let Some(serial) = &info.serial {
        rows.push(("Número de serie", serial.clone()));
    }
    rows.push(("Malaysia", env!("CARGO_PKG_VERSION").to_string()));
    let rows = rows.into_iter().map(|(name, value)| {
        rect()
            .direction(Direction::Horizontal)
            .spacing(ABOUT_ROW_GAP)
            .child(
                label()
                    .text(name)
                    .width(Size::px(ABOUT_LABEL_WIDTH))
                    .text_align(TextAlign::Right)
                    .font_size(TEXT.size as f32)
                    .font_weight(FontWeight::SEMI_BOLD)
                    .color(rgba(palette.text.focused)),
            )
            .child(
                label()
                    .text(value)
                    .width(Size::px(ABOUT_VALUE_WIDTH))
                    .font_size(TEXT.size as f32)
                    .color(rgba(palette.text.unfocused)),
            )
            .into_element()
    });
    let drawing = if info.thinkpad {
        THINKPAD_SVG
    } else {
        GENERIC_SVG
    };
    let muted = rgba(palette.text.unfocused);
    let footer = |text: &str| {
        label()
            .text(text.to_string())
            .font_size(CAPTION.size as f32)
            .color(muted)
    };
    rect()
        .width(Size::fill())
        .height(Size::fill())
        .background(rgba(if focused {
            resolved.surfaces.window
        } else {
            resolved.surfaces.sunken
        }))
        .direction(Direction::Vertical)
        .content(Content::flex())
        .cross_align(Alignment::Center)
        .on_global_key_down(|event: Event<KeyboardEventData>| {
            if matches!(event.key, Key::Named(NamedKey::Escape)) {
                close();
            }
        })
        .child(titlebar)
        .child(
            rect()
                .width(Size::px(ABOUT_DRAWING.0))
                .height(Size::px(ABOUT_DRAWING_BOX_HEIGHT))
                .margin(Gaps::new(
                    ABOUT_DRAWING_TOP - TITLEBAR.thickness as f32,
                    0.0,
                    ABOUT_GAP,
                    0.0,
                ))
                .center()
                .child(
                    SvgViewer::new(drawing)
                        .width(Size::px(ABOUT_DRAWING.0))
                        .height(Size::px(ABOUT_DRAWING.1)),
                ),
        )
        .child(
            label()
                .text(info.model.clone())
                .text_align(TextAlign::Center)
                .width(Size::fill())
                .font_size(DISPLAY.size as f32)
                .font_weight(FontWeight::BOLD)
                .color(rgba(palette.text.focused)),
        )
        .child(
            label()
                .text(info.subtitle.clone())
                .margin(Gaps::new(0.0, 0.0, ABOUT_GAP, 0.0))
                .font_size(CALLOUT.size as f32)
                .color(muted),
        )
        .child(
            rect()
                .direction(Direction::Vertical)
                .spacing(ABOUT_ROW_GAP)
                .children(rows),
        )
        .child(
            rect().margin(Gaps::new(ABOUT_GAP, 0.0, 0.0, 0.0)).child(
                Button::new()
                    .on_press(|_| crate::menubar::spawn_app("malaysia-settings", &[]))
                    .child(label().text("Más información…").font_size(TEXT.size as f32)),
            ),
        )
        .child(rect().width(Size::fill()).height(Size::flex(1.0)))
        .child(
            rect()
                .direction(Direction::Vertical)
                .cross_align(Alignment::Center)
                .spacing(ABOUT_FOOTER_GAP)
                .margin(Gaps::new(0.0, 0.0, ABOUT_GAP, 0.0))
                .child(footer("™ y © 2026 MalaysiaDreams"))
                .child(footer("Todos los derechos reservados.")),
        )
}

/// `malaysia-shell about`: a fixed-size window like macOS «Acerca de este Mac».
pub fn run_about() -> anyhow::Result<()> {
    let info = AboutInfo::read();
    tracing::info!(?info, "about this computer");
    run_window(
        AppComponent::new(AboutApp { info }),
        WindowOptions::new("Acerca de este equipo", "about")
            .decorations(Decorations::Client)
            .size(ABOUT_SIZE)
            .min_size(Some(ABOUT_SIZE))
            .max_size(Some(ABOUT_SIZE)),
    )
    .map_err(|error| anyhow::anyhow!("{error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_row_per_app_keeps_the_first_window() {
        let ids = ["kate", "foot", "kate", ""].map(String::from);
        let apps = running_apps(&ids);
        assert_eq!(
            apps,
            vec![
                RunningApp {
                    index: 0,
                    app_id: "kate".into()
                },
                RunningApp {
                    index: 1,
                    app_id: "foot".into()
                },
            ]
        );
    }

    #[test]
    fn system_files_are_parsed_into_short_values() {
        let cpuinfo = "processor\t: 0\nmodel name\t: AMD Ryzen 5 PRO 5650U with Radeon Graphics\nprocessor\t: 1\n";
        assert_eq!(cpu_name(cpuinfo), "AMD Ryzen 5 PRO 5650U");
        assert_eq!(
            short_cpu("11th Gen Intel(R) Core(TM) i5-1135G7 @ 2.40GHz"),
            "Intel Core i5-1135G7"
        );
        assert_eq!(
            short_cpu("AMD Ryzen 7 5800X 8-Core Processor"),
            "AMD Ryzen 7 5800X"
        );
        assert_eq!(memory("MemTotal:       15728640 kB\n"), "16 GB");
        assert_eq!(memory("MemTotal:       7990000 kB\n"), "8 GB");
        assert_eq!(memory(""), "Desconocida");
    }

    #[test]
    fn the_panel_size_comes_from_the_edid() {
        let mut edid = vec![0u8; 128];
        edid[21] = 31;
        edid[22] = 17;
        assert_eq!(panel_inches(&edid), Some(14));
        assert_eq!(panel_inches(&[0u8; 128]), None);
        assert_eq!(panel_inches(&[]), None);
    }

    #[test]
    fn thinkpads_are_named_from_product_version() {
        let model = model_name(
            "LENOVO",
            "20X1S0QE00",
            "ThinkPad L14 Gen 2",
            "ThinkPad L14 Gen 2",
        );
        assert_eq!(model, "ThinkPad L14 Gen 2");
        assert!(model.contains("ThinkPad"));
        assert_eq!(
            model_name("Dell Inc.", "XPS 13 9310", "", "XPS"),
            "XPS 13 9310"
        );
        assert_eq!(
            model_name("ASUS", "System Product Name", "", "To be filled by O.E.M."),
            "Este equipo"
        );
    }

    #[test]
    fn disk_sizes_read_like_a_label() {
        assert_eq!(disk_size(512_110_190_592), "512 GB");
        assert_eq!(disk_size(2_000_398_934_016), "2.0 TB");
    }

    #[test]
    fn every_power_action_has_its_question() {
        for action in [
            PowerAction::Restart,
            PowerAction::ShutDown,
            PowerAction::LogOut,
        ] {
            let (question, _, button) = confirm_text(action);
            assert!(question.ends_with('?'));
            assert!(!button.is_empty());
        }
    }
}
#[cfg(test)]
mod render_tests {
    use super::*;
    use freya_testing::TestingRunner;
    use torin::prelude::Size2D;

    struct Preview(AboutInfo, bool);

    impl App for Preview {
        fn render(&self) -> impl IntoElement {
            let titlebar = rect()
                .width(Size::fill())
                .height(Size::px(TITLEBAR.thickness as f32))
                .into_element();
            about_view(&self.0, self.1, titlebar)
        }
    }

    #[test]
    fn the_about_window_renders_for_both_drawings() {
        for thinkpad in [true, false] {
            let info = AboutInfo {
                model: "ThinkPad L14 Gen 2".into(),
                thinkpad,
                memory: "15.0 GB".into(),
                ..AboutInfo::default()
            };
            let size = Size2D::new(ABOUT_SIZE.0 as f32, ABOUT_SIZE.1 as f32);
            let (mut test, _) = TestingRunner::new(
                AppComponent::new(Preview(info, thinkpad)),
                size,
                |_| {},
                1.0,
            );
            test.sync_and_update();
            assert!(!test.render().as_ref().is_empty());
        }
    }
}
