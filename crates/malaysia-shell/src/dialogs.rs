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
use barrier_kit::platform::{Decorations, WindowOptions, run_window};
use barrier_kit::shell::{Anchors, KeyboardInteractivity, Layer, LayerSpec, Toplevels, run_layer};
use barrier_kit::titlebar::Titlebar;
use freya_components::svg_viewer::SvgViewer;
use freya_components::theming::hooks::use_init_theme;
use freya_core::integration::AppComponent;
use freya_core::prelude::*;
use keyboard_types::{Key, NamedKey};
use torin::prelude::{Alignment, Content, Direction, Gaps, Size};
use vow_design::{CAPTION, DIALOG, DIALOG_PADDING, DISPLAY, HEADLINE, Rgba, TEXT};

use crate::desktop::{self, DesktopEntry};
use crate::launcher::{IconData, app_icon, load_icon};
use crate::menubar::{find_entry, resolve_app_name};
use crate::power::{self, PowerAction};

/// Dialog surfaces, in logical px. The panel fills the layer.
const CONFIRM_SIZE: (u32, u32) = (380, 160);
const FORCE_QUIT_SIZE: (u32, u32) = (420, 400);
const ABOUT_SIZE: (u32, u32) = (360, 480);
/// The laptop drawing keeps the SVG's 1024×680 aspect.
const ABOUT_DRAWING: (f32, f32) = (220.0, 146.0);
const ABOUT_COLUMN_GAP: f32 = 8.0;
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

/// What «Acerca de este equipo» lists, read from the running system.
#[derive(Clone, Debug, Default, PartialEq)]
struct AboutInfo {
    model: String,
    thinkpad: bool,
    hostname: String,
    system: String,
    cpu: String,
    gpu: String,
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
            hostname: read("/proc/sys/kernel/hostname").trim().to_string(),
            system: os_name(&read("/etc/os-release")),
            cpu: cpu_name(&read("/proc/cpuinfo")),
            gpu: gpu_name(),
            memory: memory(&read("/proc/meminfo")),
            disk: startup_disk(&read("/proc/mounts")),
            serial: Some(dmi("product_serial")).filter(|serial| is_real(serial)),
        }
    }
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

/// The disk that holds `/`: its model and size, following partitions and
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
    let disk = physical_disk(&name);
    let block = Path::new("/sys/class/block").join(&disk);
    let model = std::fs::read_to_string(block.join("device/model"))
        .map(|model| model.trim().to_string())
        .unwrap_or_else(|_| disk.clone());
    let size = std::fs::read_to_string(block.join("size"))
        .ok()
        .and_then(|sectors| sectors.trim().parse::<u64>().ok())
        .map(|sectors| format!(" ({})", disk_size(sectors * 512)))
        .unwrap_or_default();
    format!("{model}{size}")
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

/// `PRETTY_NAME` from `/etc/os-release`.
fn os_name(os_release: &str) -> String {
    os_release
        .lines()
        .find_map(|line| line.strip_prefix("PRETTY_NAME="))
        .map(|value| value.trim_matches('"').to_string())
        .unwrap_or_else(|| "Linux".to_string())
}

/// The first `model name` and the number of logical CPUs.
fn cpu_name(cpuinfo: &str) -> String {
    let model = cpuinfo
        .lines()
        .find_map(|line| line.strip_prefix("model name"))
        .and_then(|rest| rest.split_once(':'))
        .map(|(_, model)| model.trim().to_string())
        .unwrap_or_else(|| "Desconocido".to_string());
    let threads = cpuinfo
        .lines()
        .filter(|line| line.starts_with("processor"))
        .count();
    if threads > 0 {
        format!("{model} ({threads} hilos)")
    } else {
        model
    }
}

/// `MemTotal` in GB, rounded to one decimal.
fn memory(meminfo: &str) -> String {
    meminfo
        .lines()
        .find_map(|line| line.strip_prefix("MemTotal:"))
        .and_then(|rest| rest.split_whitespace().next()?.parse::<f64>().ok())
        .map(|kib| format!("{:.1} GB", kib / 1024.0 / 1024.0))
        .unwrap_or_else(|| "Desconocida".to_string())
}

/// The first DRM card, named from `pci.ids` when the system ships it.
fn gpu_name() -> String {
    let Some(card) = std::fs::read_dir("/sys/class/drm")
        .ok()
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("card") && !name.contains('-'))
        })
        .min()
    else {
        return "Desconocida".to_string();
    };
    let id = |file: &str| {
        std::fs::read_to_string(card.join("device").join(file))
            .ok()
            .and_then(|text| u16::from_str_radix(text.trim().trim_start_matches("0x"), 16).ok())
    };
    let (Some(vendor), Some(device)) = (id("vendor"), id("device")) else {
        return "Desconocida".to_string();
    };
    let ids = [
        "/run/current-system/sw/share/hwdata/pci.ids",
        "/usr/share/hwdata/pci.ids",
        "/usr/share/misc/pci.ids",
    ]
    .iter()
    .find_map(|path| std::fs::read_to_string(Path::new(path)).ok())
    .unwrap_or_default();
    pci_name(&ids, vendor, device)
}

/// `Vendor Device` from a `pci.ids` text; the ids in hex when it is missing.
fn pci_name(ids: &str, vendor: u16, device: u16) -> String {
    let short_vendor = match vendor {
        0x1002 => Some("AMD"),
        0x8086 => Some("Intel"),
        0x10de => Some("NVIDIA"),
        _ => None,
    };
    let vendor_key = format!("{vendor:04x}");
    let device_key = format!("\t{device:04x}");
    let mut lines = ids.lines();
    let mut vendor_name = None;
    let mut device_name = None;
    for line in lines.by_ref() {
        if let Some(name) = line
            .strip_prefix(&vendor_key)
            .filter(|_| !line.starts_with('\t'))
        {
            vendor_name = Some(name.trim().to_string());
            break;
        }
    }
    if vendor_name.is_some() {
        for line in lines {
            if !line.starts_with('\t') && !line.starts_with('#') && !line.is_empty() {
                break;
            }
            if let Some(name) = line.strip_prefix(&device_key) {
                device_name = Some(name.trim().to_string());
                break;
            }
        }
    }
    let vendor = short_vendor
        .map(str::to_string)
        .or(vendor_name)
        .unwrap_or_else(|| format!("{vendor:04x}"));
    let device = device_name.unwrap_or_else(|| format!("{device:04x}"));
    format!("{vendor} {device}")
}

struct AboutApp {
    info: AboutInfo,
}

impl App for AboutApp {
    fn render(&self) -> impl IntoElement {
        use_init_theme(barrier_kit::theme::shell);
        about_view(&self.info, Titlebar::new().on_close(|()| close()))
    }
}

/// The window content; the titlebar is a parameter so tests can pass a
/// preview one that needs no Wayland window.
fn about_view(info: &AboutInfo, titlebar: Titlebar) -> impl IntoElement + use<> {
    {
        let resolved = barrier_kit::theme::resolved();
        let palette = vow_appearance::palette(resolved.scheme);
        let mut rows = vec![
            ("Procesador", info.cpu.clone()),
            ("Gráficos", info.gpu.clone()),
            ("Memoria", info.memory.clone()),
            ("Disco de arranque", info.disk.clone()),
        ];
        if let Some(serial) = &info.serial {
            rows.push(("Número de serie", serial.clone()));
        }
        rows.push(("Sistema", info.system.clone()));
        rows.push(("Malaysia", env!("CARGO_PKG_VERSION").to_string()));
        let rows = rows.into_iter().map(|(name, value)| {
            rect()
                .width(Size::fill())
                .direction(Direction::Horizontal)
                .content(Content::flex())
                .spacing(ABOUT_COLUMN_GAP)
                .child(
                    label()
                        .text(name)
                        .width(Size::flex(1.0))
                        .text_align(TextAlign::Right)
                        .font_size(CAPTION.size as f32)
                        .color(rgba(palette.text.focused)),
                )
                .child(
                    label()
                        .text(value)
                        .width(Size::flex(1.2))
                        .font_size(CAPTION.size as f32)
                        .color(rgba(palette.text.unfocused)),
                )
                .into_element()
        });
        let drawing = if info.thinkpad {
            THINKPAD_SVG
        } else {
            GENERIC_SVG
        };
        rect()
            .width(Size::fill())
            .height(Size::fill())
            .background(rgba(resolved.surfaces.window))
            .direction(Direction::Vertical)
            .on_global_key_down(|event: Event<KeyboardEventData>| {
                if matches!(event.key, Key::Named(NamedKey::Escape)) {
                    close();
                }
            })
            .child(titlebar)
            .child(
                rect()
                    .width(Size::fill())
                    .height(Size::flex(1.0))
                    .padding(Gaps::new(
                        0.0,
                        DIALOG_PADDING,
                        DIALOG_PADDING,
                        DIALOG_PADDING,
                    ))
                    .direction(Direction::Vertical)
                    .cross_align(Alignment::Center)
                    .spacing(DIALOG_PADDING / 2.0)
                    .child(
                        SvgViewer::new(drawing)
                            .width(Size::px(ABOUT_DRAWING.0))
                            .height(Size::px(ABOUT_DRAWING.1)),
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
                            .text(info.hostname.clone())
                            .font_size(CAPTION.size as f32)
                            .color(rgba(palette.text.unfocused)),
                    )
                    .child(
                        rect()
                            .width(Size::fill())
                            .direction(Direction::Vertical)
                            .spacing(4.0)
                            .children(rows),
                    )
                    .child(
                        Button::new()
                            .on_press(|_| crate::menubar::spawn_app("malaysia-settings", &[]))
                            .child(label().text("Más información…").font_size(TEXT.size as f32)),
                    ),
            )
    }
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
    fn system_files_are_parsed() {
        assert_eq!(
            os_name("NAME=NixOS\nPRETTY_NAME=\"NixOS 26.05 (Yarara)\"\n"),
            "NixOS 26.05 (Yarara)"
        );
        let cpuinfo = "processor\t: 0\nmodel name\t: AMD Ryzen 5 PRO 5650U\nprocessor\t: 1\nmodel name\t: AMD Ryzen 5 PRO 5650U\n";
        assert_eq!(cpu_name(cpuinfo), "AMD Ryzen 5 PRO 5650U (2 hilos)");
        assert_eq!(memory("MemTotal:       15728640 kB\n"), "15.0 GB");
        assert_eq!(memory(""), "Desconocida");
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
    fn pci_ids_name_the_gpu() {
        let ids = "# comment\n1002  Advanced Micro Devices, Inc. [AMD/ATI]\n\t1636  Renoir\n\t1638  Cezanne\n1022  Advanced Micro Devices, Inc. [AMD]\n\t1638  Wrong vendor\n";
        assert_eq!(pci_name(ids, 0x1002, 0x1638), "AMD Cezanne");
        assert_eq!(pci_name(ids, 0x1002, 0xffff), "AMD ffff");
        assert_eq!(pci_name("", 0x1234, 0x0001), "1234 0001");
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
    use barrier_kit::platform::{DecorationResponse, WindowStatus};
    use freya_testing::TestingRunner;
    use torin::prelude::Size2D;

    struct Preview(AboutInfo);

    impl App for Preview {
        fn render(&self) -> impl IntoElement {
            let status = WindowStatus {
                activated: true,
                maximized: false,
                decoration: DecorationResponse::Client,
                fixed: true,
            };
            about_view(&self.0, Titlebar::preview(status, "", true))
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
            let (mut test, _) =
                TestingRunner::new(AppComponent::new(Preview(info)), size, |_| {}, 1.0);
            test.sync_and_update();
            assert!(!test.render().as_ref().is_empty());
        }
    }
}
