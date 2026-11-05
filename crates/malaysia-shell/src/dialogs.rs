use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;

use barrier_kit::controls::{Button, ButtonStyle};
use barrier_kit::menu::measure_text_weight;
use barrier_kit::patterns::about::about_window;
use barrier_kit::platform::{Timers, WindowControls, WindowOptions, run_window};
use barrier_kit::shell::{
    Anchors, KeyboardInteractivity, Layer, LayerSpec, ToplevelInfo, Toplevels, run_layer,
};
use barrier_kit::style::Palette;
use barrier_kit::titlebar::Titlebar;
use freya_components::svg_viewer::SvgViewer;
use freya_core::integration::AppComponent;
use freya_core::prelude::*;
use torin::prelude::{Alignment, Content, Direction, Gaps, Size};
use vow_design::{
    ABOUT_BUTTON_GAP, ABOUT_COLUMN_GAP, ABOUT_DRAWING_HEIGHT, ABOUT_DRAWING_TOP,
    ABOUT_DRAWING_WIDTH, ABOUT_FOOTER_BOTTOM, ABOUT_HEIGHT, ABOUT_SPEC_ROW_GAP, ABOUT_SPECS_GAP,
    ABOUT_TITLE_GAP, ABOUT_TITLE_SIZE, ABOUT_VALUE_WIDTH, ABOUT_WIDTH, CALLOUT, CAPTION, DIALOG,
    DIALOG_PADDING, HEADLINE, SPACE_2, TEXT, TITLEBAR,
};

use crate::desktop::{self, DesktopEntry};
use crate::launcher::{IconData, app_icon, load_icon};
use crate::menubar::{find_entry, resolve_app_name};
use crate::power::{self, PowerAction};
use barrier_kit::theme;

const CONFIRM_SIZE: (u32, u32) = (280, 250);
const CONFIRM_BUTTON_GAP: f32 = 10.0;
const FORCE_QUIT_SIZE: (u32, u32) = (420, 400);
pub const ABOUT_APP_ID: &str = "about";
pub const ABOUT_APP_INFO_ID: &str = "about-app";
const ABOUT_SIZE: (u32, u32) = (ABOUT_WIDTH, ABOUT_HEIGHT);
const ABOUT_APP_SIZE: (u32, u32) = (260, 250);
const ABOUT_APP_ICON_SIZE: f32 = 64.0;
const ROW_HEIGHT: f32 = 32.0;
const ICON_SIZE: f32 = 22.0;

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

fn panel() -> Rect {
    let resolved = barrier_kit::theme::resolved();
    rect()
        .width(Size::fill())
        .height(Size::fill())
        .background(theme::color(resolved.surfaces.raised))
        .corner_radius(DIALOG.corner_radius as f32)
        .padding(DIALOG_PADDING)
        .direction(Direction::Vertical)
        .spacing(DIALOG_PADDING / 2.0)
}

fn title(text: &str) -> Label {
    let palette = Palette::current();
    label()
        .text(text.to_string())
        .font_size(HEADLINE.size as f32)
        .font_weight(FontWeight::BOLD)
        .color(theme::color(palette.label))
}

fn body(text: &str) -> Label {
    let palette = Palette::current();
    label()
        .text(text.to_string())
        .font_size(TEXT.size as f32)
        .color(theme::color(palette.secondary_label))
}

fn buttons(default_label: &str, on_default: impl Fn() + 'static, enabled: bool) -> Rect {
    rect()
        .width(Size::fill())
        .height(Size::auto())
        .direction(Direction::Horizontal)
        .main_align(Alignment::End)
        .spacing(DIALOG_PADDING / 2.0)
        .child(Button::new("Cancelar").on_press(|_| close()))
        .child(
            Button::new(default_label.to_string())
                .style(ButtonStyle::Primary)
                .disabled(!enabled)
                .on_press(move |_| on_default()),
        )
}

const POWER_ICON_SIZE: f32 = 64.0;
const POWER_GLYPH_SIZE: f32 = 34.0;
const COUNTDOWN_SECONDS: u32 = 60;

const POWER_SVG: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"32\" height=\"32\" viewBox=\"0 0 32 32\"><path d=\"M10.2 8.6a10 10 0 1 0 11.6 0\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2.4\" stroke-linecap=\"round\"/><path d=\"M16 4.5v11\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2.4\" stroke-linecap=\"round\"/></svg>";
const RESTART_SVG: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"32\" height=\"32\" viewBox=\"0 0 32 32\"><path d=\"M25.5 16A9.5 9.5 0 1 1 22.7 9.3\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2.4\" stroke-linecap=\"round\"/><path d=\"M23.6 3.8v6.4h-6.4\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2.4\" stroke-linecap=\"round\" stroke-linejoin=\"round\"/></svg>";
const LOGOUT_SVG: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"32\" height=\"32\" viewBox=\"0 0 32 32\"><circle cx=\"16\" cy=\"11\" r=\"5\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2.4\"/><path d=\"M6.5 26.5a9.5 9.5 0 0 1 19 0\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2.4\" stroke-linecap=\"round\"/></svg>";
const SLEEP_SVG: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"32\" height=\"32\" viewBox=\"0 0 32 32\"><path d=\"M20.5 5.5a11 11 0 1 0 6 14.5 9 9 0 0 1-6-14.5z\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2.4\" stroke-linejoin=\"round\"/></svg>";

struct ConfirmText {
    question: &'static str,
    outcome: &'static str,
    button: &'static str,
    glyph: &'static str,
}

fn confirm_text(action: PowerAction) -> ConfirmText {
    match action {
        PowerAction::Restart => ConfirmText {
            question: "¿Quieres reiniciar el equipo?",
            outcome: "el equipo se reiniciará",
            button: "Reiniciar",
            glyph: RESTART_SVG,
        },
        PowerAction::ShutDown => ConfirmText {
            question: "¿Quieres apagar el equipo?",
            outcome: "el equipo se apagará",
            button: "Apagar",
            glyph: POWER_SVG,
        },
        PowerAction::LogOut => ConfirmText {
            question: "¿Seguro que quieres salir de todas las apps y cerrar la sesión?",
            outcome: "la sesión se cerrará",
            button: "Cerrar sesión",
            glyph: LOGOUT_SVG,
        },
        PowerAction::Suspend => ConfirmText {
            question: "¿Quieres suspender el equipo?",
            outcome: "el equipo se suspenderá",
            button: "Suspender",
            glyph: SLEEP_SVG,
        },
    }
}

fn countdown_text(outcome: &str, seconds: u32) -> String {
    let unit = if seconds == 1 { "segundo" } else { "segundos" };
    format!("Si no haces nada, {outcome} automáticamente dentro de {seconds} {unit}.")
}

fn schedule_tick(timers: Timers, remaining: State<u32>, action: PowerAction) {
    let next = timers.clone();
    timers.after(std::time::Duration::from_secs(1), move || {
        let mut remaining = remaining;
        let left = remaining.peek().saturating_sub(1);
        remaining.set(left);
        if left == 0 {
            power::run(action);
            close();
        } else {
            schedule_tick(next.clone(), remaining, action);
        }
    });
}

fn power_badge(glyph: &'static str) -> Rect {
    let mut disc = Palette::current().secondary_label;
    disc.a = 0.55;
    rect()
        .width(Size::px(POWER_ICON_SIZE))
        .height(Size::px(POWER_ICON_SIZE))
        .corner_radius(POWER_ICON_SIZE / 2.0)
        .background(theme::color(disc))
        .center()
        .child(
            SvgViewer::new(Bytes::from_static(glyph.as_bytes()))
                .width(Size::px(POWER_GLYPH_SIZE))
                .height(Size::px(POWER_GLYPH_SIZE))
                .color(Color::WHITE)
                .show_loader(false),
        )
}

fn wide_buttons(default_label: &str, on_default: impl Fn() + 'static) -> Rect {
    rect()
        .width(Size::fill())
        .height(Size::auto())
        .direction(Direction::Horizontal)
        .content(Content::Flex)
        .spacing(CONFIRM_BUTTON_GAP)
        .child(
            Button::new("Cancelar")
                .width(Size::flex(1.0))
                .on_press(|_| close()),
        )
        .child(
            Button::new(default_label.to_string())
                .style(ButtonStyle::Primary)
                .width(Size::flex(1.0))
                .on_press(move |_| on_default()),
        )
}

struct ConfirmApp {
    action: PowerAction,
    remaining: State<u32>,
}

impl App for ConfirmApp {
    fn render(&self) -> impl IntoElement {
        barrier_kit::theme::use_shell_theme();
        let action = self.action;
        let remaining = self.remaining;
        let timers = Timers::get();
        use_hook(move || schedule_tick(timers, remaining, action));
        confirm_view(action, *remaining.read())
    }
}

fn confirm_view(action: PowerAction, seconds: u32) -> Rect {
    let confirm = move || {
        power::run(action);
        close();
    };
    let text = confirm_text(action);
    let palette = Palette::current();
    panel()
        .spacing(0.0)
        .content(Content::Flex)
        .on_global_key_down(move |event: Event<KeyboardEventData>| match event.key {
            Key::Named(NamedKey::Escape) => close(),
            Key::Named(NamedKey::Enter) => confirm(),
            _ => {}
        })
        .child(power_badge(text.glyph))
        .child(
            label()
                .width(Size::fill())
                .padding(Gaps::new(14.0, 0.0, SPACE_2, 0.0))
                .text(text.question)
                .font_size(TEXT.size as f32)
                .font_weight(FontWeight::BOLD)
                .color(theme::color(palette.label)),
        )
        .child(
            label()
                .width(Size::fill())
                .text(countdown_text(text.outcome, seconds))
                .font_size(CALLOUT.size as f32)
                .color(theme::color(palette.label)),
        )
        .child(
            rect()
                .width(Size::fill())
                .height(Size::flex(1.0))
                .min_height(Size::px(18.0)),
        )
        .child(wide_buttons(text.button, confirm))
}

pub fn run_confirm(arg: Option<&str>) -> anyhow::Result<()> {
    let action = arg.and_then(PowerAction::from_arg).ok_or_else(|| {
        anyhow::anyhow!("usage: malaysia-shell power <suspend|restart|shutdown|logout>")
    })?;
    run_layer(
        AppComponent::new(ConfirmApp {
            action,
            remaining: State::create_global(COUNTDOWN_SECONDS),
        }),
        spec("malaysia-dialog", CONFIRM_SIZE),
    )
    .map_err(|error| anyhow::anyhow!("{error}"))
}

#[derive(Clone, Debug, PartialEq)]
struct RunningApp {
    id: u64,
    app_id: String,
}

fn running_apps(windows: &[ToplevelInfo]) -> Vec<RunningApp> {
    let mut apps: Vec<RunningApp> = Vec::new();
    for window in windows {
        if window.app_id.is_empty() || apps.iter().any(|app| app.app_id == window.app_id) {
            continue;
        }
        apps.push(RunningApp {
            id: window.id,
            app_id: window.app_id.clone(),
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
        barrier_kit::theme::use_shell_theme();
        let toplevels = Toplevels::get();
        let apps = running_apps(&toplevels.list.read());
        let selected = (*self.selected.read()).min(apps.len().saturating_sub(1));
        let palette = Palette::current();
        let text = theme::color(palette.label);
        let mut highlight = palette.accent;
        highlight.a = 0.18;

        let force_quit = {
            let apps = apps.clone();
            let toplevels = toplevels.clone();
            move || {
                if let Some(app) = apps.get(selected) {
                    tracing::info!(app_id = %app.app_id, "force quit requested");
                    toplevels.force_quit(app.id);
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
                    theme::color(highlight)
                } else {
                    Color::TRANSPARENT
                })
                .padding(Gaps::new(0.0, SPACE_2, 0.0, SPACE_2))
                .direction(Direction::Horizontal)
                .cross_align(Alignment::Center)
                .spacing(SPACE_2)
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

const THINKPAD_SVG: &[u8] = include_bytes!("about/thinkpad.svg");
const GENERIC_SVG: &[u8] = include_bytes!("about/generic.svg");

#[derive(Clone, Debug, Default, PartialEq)]
struct AboutInfo {
    model: String,
    thinkpad: bool,
    subtitle: String,
    cpu: String,
    graphics: Option<String>,
    memory: String,
    disk: String,
    serial: Option<String>,
    system: (String, String),
}

impl AboutInfo {
    fn read() -> Self {
        let read = |path: &str| std::fs::read_to_string(path).unwrap_or_default();
        let dmi = |key: &str| read(&format!("/sys/class/dmi/id/{key}")).trim().to_string();
        let cpuinfo = read("/proc/cpuinfo");
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
            cpu: cpu_name(&cpuinfo),
            graphics: graphics(&cpuinfo),
            memory: memory(&read("/proc/meminfo")),
            disk: startup_disk(&read("/proc/mounts")),
            serial: Some(dmi("product_serial")).filter(|serial| is_real(serial)),
            system: system_release(&read("/etc/os-release")).unwrap_or_else(|| {
                (
                    "Linux".to_string(),
                    read("/proc/sys/kernel/osrelease").trim().to_string(),
                )
            }),
        }
    }
}

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

fn panel_inches(edid: &[u8]) -> Option<u32> {
    let (width, height) = (f64::from(*edid.get(21)?), f64::from(*edid.get(22)?));
    (width > 0.0 && height > 0.0).then(|| (width.hypot(height) / 2.54).round() as u32)
}

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

fn disk_size(bytes: u64) -> String {
    let gb = bytes as f64 / 1e9;
    if gb >= 1000.0 {
        format!("{:.1} TB", gb / 1000.0)
    } else {
        format!("{gb:.0} GB")
    }
}

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

const PCI_IDS: [&str; 3] = [
    "/run/current-system/sw/share/hwdata/pci.ids",
    "/usr/share/hwdata/pci.ids",
    "/usr/share/misc/pci.ids",
];

fn graphics(cpuinfo: &str) -> Option<String> {
    if let Some(integrated) = integrated_gpu(cpuinfo) {
        return Some(integrated);
    }
    let device = gpu_device()?;
    let read = |file: &str| {
        std::fs::read_to_string(device.join(file))
            .ok()
            .map(|value| value.trim().to_string())
    };
    let ids = PCI_IDS
        .iter()
        .find_map(|path| std::fs::read_to_string(path).ok())?;
    let (vendor, id) = (read("vendor")?, read("device")?);
    let name = pci_name(
        &ids,
        vendor.trim_start_matches("0x"),
        id.trim_start_matches("0x"),
    )?;
    match read("mem_info_vram_total").and_then(|bytes| bytes.parse::<u64>().ok()) {
        Some(bytes) if bytes > 0 => Some(format!("{name} {} MB", bytes / 1024 / 1024)),
        _ => Some(name),
    }
}

fn gpu_device() -> Option<std::path::PathBuf> {
    let mut cards: Vec<_> = std::fs::read_dir("/sys/class/drm")
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("card") && !name.contains('-'))
        })
        .collect();
    cards.sort();
    cards
        .into_iter()
        .map(|card| card.join("device"))
        .find(|device| device.join("vendor").exists())
}

fn integrated_gpu(cpuinfo: &str) -> Option<String> {
    let model = cpuinfo
        .lines()
        .find_map(|line| line.strip_prefix("model name"))?
        .split_once(':')?
        .1;
    let (cpu, gpu) = model.split_once(" with ")?;
    let vendor = cpu.split_whitespace().next()?;
    Some(format!("{vendor} {}", gpu.trim()))
}

fn pci_name(ids: &str, vendor: &str, device: &str) -> Option<String> {
    let vendor_prefix = format!("{vendor}  ");
    let mut lines = ids
        .lines()
        .skip_while(|line| !line.starts_with(&vendor_prefix));
    let vendor_name = lines.next()?.strip_prefix(&vendor_prefix)?;
    let device_prefix = format!("\t{device}  ");
    let device_name = lines
        .take_while(|line| line.starts_with('\t') || line.starts_with('#'))
        .find_map(|line| line.strip_prefix(&device_prefix))?;
    Some(format!(
        "{} {}",
        short_vendor(vendor_name.trim()),
        bracketed(device_name.trim())
    ))
}

fn bracketed(name: &str) -> &str {
    name.split_once('[')
        .and_then(|(_, rest)| rest.strip_suffix(']'))
        .unwrap_or(name)
}

fn short_vendor(name: &str) -> &str {
    match name.split_once('[') {
        Some((_, rest)) => rest.trim_end_matches(']').split('/').next().unwrap_or(rest),
        None => name.split_whitespace().next().unwrap_or(name),
    }
}

fn system_release(os_release: &str) -> Option<(String, String)> {
    let field = |key: &str| {
        os_release
            .lines()
            .find_map(|line| line.strip_prefix(key)?.strip_prefix('='))
            .map(|value| value.trim().trim_matches('"').to_string())
            .filter(|value| !value.is_empty())
    };
    Some((
        field("NAME")?,
        field("VERSION").or_else(|| field("VERSION_ID"))?,
    ))
}

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
        barrier_kit::theme::use_shell_theme();
        about_view(&self.info)
    }
}

fn about_view(info: &AboutInfo) -> impl IntoElement + use<> {
    let resolved = barrier_kit::theme::resolved();
    let palette = Palette::current();
    let mut rows = vec![("Procesador".to_string(), info.cpu.clone())];
    if let Some(graphics) = &info.graphics {
        rows.push(("Gráficos".to_string(), graphics.clone()));
    }
    rows.push(("Memoria".to_string(), info.memory.clone()));
    rows.push(("Disco de arranque".to_string(), info.disk.clone()));
    if let Some(serial) = &info.serial {
        rows.push(("Número de serie".to_string(), serial.clone()));
    }
    rows.push(info.system.clone());
    let label_width = rows
        .iter()
        .map(|(name, _)| measure_text_weight(name, CAPTION.size as f32, FontWeight::SEMI_BOLD))
        .fold(0.0, f32::max)
        .ceil();
    let rows = rows.into_iter().map(|(name, value)| {
        rect()
            .direction(Direction::Horizontal)
            .spacing(ABOUT_COLUMN_GAP)
            .child(
                label()
                    .text(name)
                    .width(Size::px(label_width))
                    .text_align(TextAlign::Right)
                    .font_size(CAPTION.size as f32)
                    .font_weight(FontWeight::SEMI_BOLD)
                    .color(theme::color(palette.label)),
            )
            .child(
                label()
                    .text(value)
                    .width(Size::px(ABOUT_VALUE_WIDTH))
                    .font_size(CAPTION.size as f32)
                    .color(theme::color(palette.secondary_label)),
            )
            .into_element()
    });
    let drawing = if info.thinkpad {
        THINKPAD_SVG
    } else {
        GENERIC_SVG
    };
    let muted = theme::color(palette.secondary_label);
    let footer = |text: &str| {
        label()
            .text(text.to_string())
            .font_size(CAPTION.size as f32)
            .color(muted)
    };
    rect()
        .width(Size::fill())
        .height(Size::fill())
        .background(theme::color(resolved.surfaces.raised))
        .font_family(TEXT.family)
        .direction(Direction::Vertical)
        .content(Content::flex())
        .cross_align(Alignment::Center)
        .on_global_key_down(|event: Event<KeyboardEventData>| {
            if matches!(event.key, Key::Named(NamedKey::Escape)) {
                close();
            }
        })
        .child(Titlebar::new().close_only(true).title("").into_element())
        .child(
            rect()
                .width(Size::px(ABOUT_DRAWING_WIDTH))
                .height(Size::px(ABOUT_DRAWING_HEIGHT))
                .margin(Gaps::new(
                    ABOUT_DRAWING_TOP - TITLEBAR.thickness as f32,
                    0.0,
                    ABOUT_TITLE_GAP,
                    0.0,
                ))
                .center()
                .child(
                    SvgViewer::new(drawing)
                        .width(Size::px(ABOUT_DRAWING_WIDTH))
                        .height(Size::px(ABOUT_DRAWING_HEIGHT)),
                ),
        )
        .child(
            label()
                .text(info.model.clone())
                .text_align(TextAlign::Center)
                .width(Size::fill())
                .font_size(ABOUT_TITLE_SIZE)
                .font_weight(FontWeight::BOLD)
                .color(theme::color(palette.label)),
        )
        .child(
            label()
                .text(info.subtitle.clone())
                .margin(Gaps::new(0.0, 0.0, ABOUT_SPECS_GAP, 0.0))
                .font_size(CALLOUT.size as f32)
                .color(muted),
        )
        .child(
            rect()
                .direction(Direction::Vertical)
                .spacing(ABOUT_SPEC_ROW_GAP)
                .children(rows),
        )
        .child(
            rect()
                .margin(Gaps::new(ABOUT_BUTTON_GAP, 0.0, 0.0, 0.0))
                .child(
                    Button::new("Más información…")
                        .small(true)
                        .on_press(|_| crate::spawn::spawn_settings()),
                ),
        )
        .child(rect().width(Size::fill()).height(Size::flex(1.0)))
        .child(
            rect()
                .direction(Direction::Vertical)
                .cross_align(Alignment::Center)
                .margin(Gaps::new(0.0, 0.0, ABOUT_FOOTER_BOTTOM, 0.0))
                .child(footer("™ y © 2026 Malaysia"))
                .child(footer("Todos los derechos reservados.")),
        )
}

pub fn run_about() -> anyhow::Result<()> {
    let info = AboutInfo::read();
    tracing::info!(?info, "about this computer");
    run_window(
        AppComponent::new(AboutApp { info }),
        WindowOptions::new("Acerca de este PC", ABOUT_APP_ID)
            .size(ABOUT_SIZE)
            .min_size(Some(ABOUT_SIZE))
            .max_size(Some(ABOUT_SIZE)),
    )
    .map_err(|error| anyhow::anyhow!("{error}"))
}

struct AboutAppInfo {
    name: String,
    kind: Option<String>,
    app_id: String,
    icon: Option<IconData>,
}

impl App for AboutAppInfo {
    fn render(&self) -> impl IntoElement {
        barrier_kit::theme::use_shell_theme();
        let resolved = barrier_kit::theme::resolved();
        let focused = WindowControls::get().status.read().activated;
        rect()
            .width(Size::fill())
            .height(Size::fill())
            .background(theme::color(if focused {
                resolved.surfaces.window
            } else {
                resolved.surfaces.titlebar_inactive
            }))
            .direction(Direction::Vertical)
            .content(Content::flex())
            .on_global_key_down(|event: Event<KeyboardEventData>| {
                if matches!(event.key, Key::Named(NamedKey::Escape)) {
                    close();
                }
            })
            .child(Titlebar::new().close_only(true).title("").into_element())
            .child(
                rect()
                    .width(Size::fill())
                    .height(Size::flex(1.0))
                    .child(about_window(
                        app_icon(self.icon.as_ref(), &self.name, ABOUT_APP_ICON_SIZE),
                        self.name.clone(),
                        self.kind.clone(),
                        self.app_id.clone(),
                    )),
            )
    }
}

pub fn run_about_app(app_id: Option<&str>) -> anyhow::Result<()> {
    let app_id = app_id.ok_or_else(|| anyhow::anyhow!("about-app needs an app_id"))?;
    let entries = desktop::read_entries();
    let entry = find_entry(app_id, &entries);
    let info = AboutAppInfo {
        name: resolve_app_name(Some(app_id), &entries),
        kind: entry.and_then(|entry| entry.generic_name.clone()),
        app_id: app_id.to_string(),
        icon: entry.and_then(|entry| load_icon(entry.icon.as_deref())),
    };
    let window_title = format!("Acerca de {}", info.name);
    run_window(
        AppComponent::new(info),
        WindowOptions::new(window_title, ABOUT_APP_INFO_ID)
            .size(ABOUT_APP_SIZE)
            .min_size(Some(ABOUT_APP_SIZE))
            .max_size(Some(ABOUT_APP_SIZE)),
    )
    .map_err(|error| anyhow::anyhow!("{error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_row_per_app_keeps_the_first_window() {
        let windows =
            [(7, "kate"), (3, "foot"), (9, "kate"), (4, "")].map(|(id, app_id)| ToplevelInfo {
                id,
                app_id: app_id.into(),
                ..Default::default()
            });
        assert_eq!(
            running_apps(&windows),
            vec![
                RunningApp {
                    id: 7,
                    app_id: "kate".into()
                },
                RunningApp {
                    id: 3,
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
    fn the_gpu_and_the_system_read_like_macos() {
        let cpuinfo = "model name\t: AMD Ryzen 5 PRO 5650U with Radeon Graphics\n";
        assert_eq!(
            integrated_gpu(cpuinfo).as_deref(),
            Some("AMD Radeon Graphics")
        );
        assert_eq!(
            integrated_gpu("model name\t: Intel(R) Core(TM) i5-8250U\n"),
            None
        );
        let ids = "8086  Intel Corporation\n\t9a49  TigerLake-LP GT2 [Iris Xe Graphics]\n\t\t17aa 22d8  ThinkPad\n10de  NVIDIA Corporation\n\t2560  GA106M [GeForce RTX 3060 Mobile / Max-Q]\n1002  Advanced Micro Devices, Inc. [AMD/ATI]\n\t73bf  Navi 21\n";
        assert_eq!(
            pci_name(ids, "8086", "9a49").as_deref(),
            Some("Intel Iris Xe Graphics")
        );
        assert_eq!(
            pci_name(ids, "10de", "2560").as_deref(),
            Some("NVIDIA GeForce RTX 3060 Mobile / Max-Q")
        );
        assert_eq!(
            pci_name(ids, "1002", "73bf").as_deref(),
            Some("AMD Navi 21")
        );
        assert_eq!(pci_name(ids, "8086", "2560"), None);
        let os_release = "NAME=NixOS\nPRETTY_NAME=\"NixOS 26.05 (Yarara)\"\nVERSION=\"26.05 (Yarara)\"\nVERSION_ID=\"26.05\"\n";
        assert_eq!(
            system_release(os_release),
            Some(("NixOS".to_string(), "26.05 (Yarara)".to_string()))
        );
        assert_eq!(
            system_release("NAME=\"Arch Linux\"\nVERSION_ID=rolling\n"),
            Some(("Arch Linux".to_string(), "rolling".to_string()))
        );
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
            let text = confirm_text(action);
            assert!(text.question.starts_with('¿') && text.question.ends_with('?'));
            assert!(!text.button.is_empty());
        }
    }

    #[test]
    fn the_countdown_reads_like_macos() {
        assert_eq!(
            countdown_text(confirm_text(PowerAction::ShutDown).outcome, 53),
            "Si no haces nada, el equipo se apagará automáticamente dentro de 53 segundos."
        );
        assert!(countdown_text("x", 1).ends_with("1 segundo."));
    }
}
