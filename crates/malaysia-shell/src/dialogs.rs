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
use barrier_kit::shell::{Anchors, KeyboardInteractivity, Layer, LayerSpec, Toplevels, run_layer};
use freya_components::theming::hooks::use_init_theme;
use freya_core::integration::AppComponent;
use freya_core::prelude::*;
use keyboard_types::{Key, NamedKey};
use torin::prelude::{Alignment, Direction, Gaps, Size};
use vow_design::{DIALOG, DIALOG_PADDING, HEADLINE, Rgba, TEXT};

use crate::desktop::{self, DesktopEntry};
use crate::launcher::{IconData, app_icon, load_icon};
use crate::menubar::{find_entry, resolve_app_name};
use crate::power::{self, PowerAction};

/// Dialog surfaces, in logical px. The panel fills the layer.
const CONFIRM_SIZE: (u32, u32) = (380, 160);
const FORCE_QUIT_SIZE: (u32, u32) = (420, 400);
const ABOUT_SIZE: (u32, u32) = (400, 300);
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

/// What «Acerca de este equipo» lists, read from the running system.
#[derive(Clone, Debug, Default, PartialEq)]
struct AboutInfo {
    hostname: String,
    system: String,
    cpu: String,
    gpu: String,
    memory: String,
}

impl AboutInfo {
    fn read() -> Self {
        let read = |path: &str| std::fs::read_to_string(path).unwrap_or_default();
        Self {
            hostname: read("/proc/sys/kernel/hostname").trim().to_string(),
            system: os_name(&read("/etc/os-release")),
            cpu: cpu_name(&read("/proc/cpuinfo")),
            gpu: gpu_name(),
            memory: memory(&read("/proc/meminfo")),
        }
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
        let palette = vow_appearance::palette(barrier_kit::theme::resolved().scheme);
        let rows = [
            ("Sistema", &self.info.system),
            ("Procesador", &self.info.cpu),
            ("Gráficos", &self.info.gpu),
            ("Memoria", &self.info.memory),
        ]
        .into_iter()
        .map(|(name, value)| {
            rect()
                .width(Size::fill())
                .direction(Direction::Horizontal)
                .spacing(12.0)
                .child(
                    label()
                        .text(name)
                        .width(Size::px(96.0))
                        .font_size(TEXT.size as f32)
                        .color(rgba(palette.text.unfocused)),
                )
                .child(
                    label()
                        .text(value.clone())
                        .width(Size::flex(1.0))
                        .font_size(TEXT.size as f32)
                        .color(rgba(palette.text.focused)),
                )
                .into_element()
        });
        panel()
            .on_global_key_down(|event: Event<KeyboardEventData>| {
                if matches!(event.key, Key::Named(NamedKey::Escape | NamedKey::Enter)) {
                    close();
                }
            })
            .cross_align(Alignment::Center)
            .child(title(&self.info.hostname))
            .child(body(&format!("Malaysia {}", env!("CARGO_PKG_VERSION"))))
            .child(
                rect()
                    .width(Size::fill())
                    .height(Size::flex(1.0))
                    .direction(Direction::Vertical)
                    .main_align(Alignment::Center)
                    .spacing(6.0)
                    .children(rows),
            )
    }
}

/// `malaysia-shell about`.
pub fn run_about() -> anyhow::Result<()> {
    let info = AboutInfo::read();
    tracing::info!(?info, "about this computer");
    run_layer(
        AppComponent::new(AboutApp { info }),
        spec("malaysia-dialog", ABOUT_SIZE),
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
