use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use bk::popup_button::PopUpButton;
use bk::prelude::*;
use bk::views::feedback::Alert;
use bk::views::form::{Form, Row, Section};
use ctappearance::{AppearanceConfig, Toolkit, ini, xdg};

use crate::app::toolkit_text_size;
use crate::localize::{intern, tr};

// GTK 3 compiles these into its own resources, so no folder lists them.
const GTK3_BUILT_IN: [&str; 3] = ["Adwaita", "HighContrast", "HighContrastInverse"];
// The GSettings defaults of org.gnome.desktop.interface, what GTK shows with nothing set.
const GTK_DEFAULT_THEME: &str = "Adwaita";
const GTK_DEFAULT_ICONS: &str = "Adwaita";
// Built into Qt; qt5ct and qt6ct use Fusion when `style` is unset.
const QT_BUILT_IN: [&str; 2] = ["Fusion", "Windows"];
const QT_TOOLS: [&str; 2] = ["qt6ct", "qt5ct"];
const INTERFACE: &str = "org.gnome.desktop.interface";

#[derive(Clone, Debug, PartialEq)]
pub struct Choice {
    names: Vec<String>,
    selected: usize,
}

impl Choice {
    fn new(mut names: Vec<String>, current: &str) -> Self {
        let selected = match names
            .iter()
            .position(|name| name.eq_ignore_ascii_case(current))
        {
            Some(index) => index,
            None => {
                names.push(current.to_string());
                names.len() - 1
            }
        };
        Self { names, selected }
    }

    fn popup(&self, title: &'static str) -> PopUpButton {
        self.names
            .iter()
            .fold(PopUpButton::new(title), |popup, name| {
                popup.item(intern(name.clone()))
            })
            .selected(self.selected)
    }
}

/// What the system has installed and uses now, read again after every change.
#[derive(Clone, Debug, PartialEq)]
pub struct Themes {
    gtk3: Choice,
    icons: Choice,
    gtk4_css: bool,
    qt_style: Choice,
    qt_palettes: Vec<PathBuf>,
    qt_palette: Choice,
    qt_icons: Choice,
    qt_managed: bool,
}

impl Themes {
    pub fn read() -> Self {
        let qt = qt_conf(QT_TOOLS[0]);
        let qt_text = fs::read_to_string(&qt).unwrap_or_default();
        let qt_value = |key| ini::value(&qt_text, "Appearance", key).map(str::to_string);
        let qt_palettes = qt_palettes();
        let palette_names = std::iter::once(tr!("qt-palette-style").to_string())
            .chain(qt_palettes.iter().map(|path| stem(path)))
            .collect();
        let current_palette = match qt_value("custom_palette").as_deref() {
            Some("true") => qt_value("color_scheme_path")
                .map(|path| stem(Path::new(&path)))
                .unwrap_or_default(),
            _ => tr!("qt-palette-style").to_string(),
        };
        let icons = icon_themes();
        Self {
            gtk3: Choice::new(
                gtk3_themes(),
                &gtk_value("gtk-theme", "gtk-theme-name", GTK_DEFAULT_THEME),
            ),
            icons: Choice::new(
                icons.clone(),
                &gtk_value("icon-theme", "gtk-icon-theme-name", GTK_DEFAULT_ICONS),
            ),
            gtk4_css: fs::read_to_string(gtk4_css()).is_ok_and(|css| !css.trim().is_empty()),
            qt_style: Choice::new(
                qt_styles(),
                &qt_value("style").unwrap_or(QT_BUILT_IN[0].to_string()),
            ),
            qt_palette: Choice::new(palette_names, &current_palette),
            qt_palettes,
            qt_icons: Choice::new(
                std::iter::once(tr!("qt-icons-default").to_string())
                    .chain(icons)
                    .collect(),
                &qt_value("icon_theme").unwrap_or_else(|| tr!("qt-icons-default").to_string()),
            ),
            qt_managed: is_symlink(&qt),
        }
    }
}

pub fn own_themes(config: &AppearanceConfig) -> bool {
    !config.gtk_bridge && !config.qt_bridge
}

pub fn freedesktop(
    config: &AppearanceConfig,
    themes: &Themes,
    confirming: State<bool>,
    refresh: State<u32>,
    error: State<Option<String>>,
) -> Form {
    let own = own_themes(config);
    let qt_off = !own || themes.qt_managed;
    let apply = move |result: io::Result<()>| {
        let (mut refresh, mut error) = (refresh, error);
        if let Err(failure) = result {
            error.set(Some(failure.to_string()));
        }
        *refresh.write() += 1;
    };
    let gtk3 = themes.gtk3.names.clone();
    let icons = themes.icons.names.clone();
    let styles = themes.qt_style.names.clone();
    let palettes = themes.qt_palettes.clone();
    let qt_icons = themes.qt_icons.names.clone();
    let gtk =
        Section::titled(tr!("gtk-section"))
            .row(
                Row::new(tr!("gtk3-theme")).control(
                    themes
                        .gtk3
                        .popup(tr!("gtk3-theme"))
                        .disabled(!own)
                        .on_change(move |index: usize| {
                            apply(set_gtk("gtk-theme", "gtk-theme-name", Some(&gtk3[index])))
                        }),
                ),
            )
            .row(
                Row::new(tr!("gtk4-css")).control(
                    rect()
                        .direction(Direction::Horizontal)
                        .spacing(ctdesign::SPACE_2)
                        .maybe_child(themes.gtk4_css.then(|| {
                            Button::new(tr!("remove"))
                                .disabled(!own)
                                .on_press(move |_| apply(remove_gtk4_css()))
                        }))
                        .child(
                            Button::new(tr!("import"))
                                .disabled(!own)
                                .on_press(move |_| {
                                    spawn(async move {
                                        if let Some(file) = pick(tr!("gtk4-css")).await {
                                            apply(import_gtk4_css(&file));
                                        }
                                    });
                                }),
                        ),
                ),
            )
            .row(
                Row::new(tr!("icons")).control(
                    themes.icons.popup(tr!("icons")).disabled(!own).on_change(
                        move |index: usize| {
                            apply(set_gtk(
                                "icon-theme",
                                "gtk-icon-theme-name",
                                Some(&icons[index]),
                            ))
                        },
                    ),
                ),
            )
            .row(
                Row::new(tr!("themes-folder"))
                    .control(Button::new(tr!("open")).on_press(|_| open_themes_folder())),
            );
    let qt_style = Row::new(tr!("qt-style"));
    let qt_style = if themes.qt_managed {
        qt_style.detail(tr!("qt-managed"))
    } else {
        qt_style
    };
    let qt = Section::titled(tr!("qt-section"))
        .row(
            qt_style.control(
                themes
                    .qt_style
                    .popup(tr!("qt-style"))
                    .disabled(qt_off)
                    .on_change(move |index: usize| {
                        apply(set_qt(&[("style", Some(&styles[index]))]))
                    }),
            ),
        )
        .row(
            Row::new(tr!("qt-palette")).control(
                rect()
                    .direction(Direction::Horizontal)
                    .spacing(ctdesign::SPACE_2)
                    .child(
                        themes
                            .qt_palette
                            .popup(tr!("qt-palette"))
                            .disabled(qt_off)
                            .on_change(move |index: usize| match index.checked_sub(1) {
                                None => apply(set_palette(None)),
                                Some(listed) => {
                                    if let Some(path) = palettes.get(listed) {
                                        apply(set_palette(Some(path)))
                                    }
                                }
                            }),
                    )
                    .child(
                        Button::new(tr!("import"))
                            .disabled(qt_off)
                            .on_press(move |_| {
                                spawn(async move {
                                    if let Some(file) = pick(tr!("qt-palette")).await {
                                        apply(import_palette(&file));
                                    }
                                });
                            }),
                    ),
            ),
        )
        .row(
            Row::new(tr!("icons")).control(
                themes
                    .qt_icons
                    .popup(tr!("icons"))
                    .disabled(qt_off)
                    .on_change(move |index: usize| {
                        let theme = index.checked_sub(1).map(|_| qt_icons[index].as_str());
                        apply(set_qt(&[("icon_theme", theme)]))
                    }),
            ),
        )
        .row(
            Row::new(tr!("qt-advanced")).control(
                Button::new(tr!("open-qt6ct"))
                    .disabled(!own || !crate::apps::installed(QT_TOOLS[0]))
                    .on_press(|_| crate::apps::open_app(QT_TOOLS[0])),
            ),
        );
    let text_size = Section::titled(tr!("text-size")).row(toolkit_text_size(
        config,
        error,
        Toolkit::Gtk,
        tr!("text-size-gtk"),
    ));
    // The portal writes the qt6ct font only while it syncs Qt.
    let text_size = if config.qt_bridge {
        text_size.row(toolkit_text_size(
            config,
            error,
            Toolkit::Qt,
            tr!("text-size-qt"),
        ))
    } else {
        text_size
    };
    let mut confirming = confirming;
    let mut form = Form::new()
        .section(
            Section::new().row(
                Row::new(tr!("own-themes")).control(
                    Switch::new(tr!("own-themes"))
                        .on(own)
                        .on_toggle(move |on: bool| {
                            if on {
                                confirming.set(true);
                            } else {
                                edit_bridges(true, error);
                            }
                        }),
                ),
            ),
        )
        .section(gtk)
        .section(qt)
        .section(text_size);
    if own {
        form = form.section(Section::new().row(Row::destructive(
            tr!("back-to-malaysia"),
            move |_| {
                apply(reset());
                edit_bridges(true, error);
            },
        )));
    }
    form
}

pub fn confirm(confirming: State<bool>, error: State<Option<String>>) -> Option<Element> {
    let mut confirming = confirming;
    confirming.read().then(|| {
        Alert::new(tr!("own-themes-confirm-title"), tr!("own-themes-confirm"))
            .confirm(tr!("own-themes-confirm-ok"), move |_| {
                confirming.set(false);
                edit_bridges(false, error);
            })
            .cancel(tr!("cancel"), move |_| confirming.set(false))
            .into_element()
    })
}

fn edit_bridges(on: bool, error: State<Option<String>>) {
    let mut config = ctappearance::load_appearance();
    config.gtk_bridge = on;
    config.qt_bridge = on;
    let mut error = error;
    error.set(crate::state::save_config(&config));
}

async fn pick(title: &'static str) -> Option<PathBuf> {
    bk::portal::open_files(title, false, false)
        .await?
        .into_iter()
        .next()
}

fn stem(path: &Path) -> String {
    path.file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn is_symlink(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_symlink())
}

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

// XDG data folders first, then the legacy dot folder that GTK and icon loaders still read.
fn data_folders(sub: &str, legacy: &str) -> Vec<PathBuf> {
    xdg::data_home()
        .map(|dir| dir.join(sub))
        .into_iter()
        .chain(home().map(|home| home.join(legacy)))
        .chain(xdg::data_dirs().into_iter().map(|dir| dir.join(sub)))
        .collect()
}

fn entries(dirs: &[PathBuf], keep: impl Fn(&Path) -> bool) -> Vec<String> {
    let mut names: Vec<String> = dirs
        .iter()
        .filter_map(|dir| fs::read_dir(dir).ok())
        .flatten()
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| keep(path))
        .map(|path| {
            path.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    sort_unique(&mut names);
    names
}

fn sort_unique(names: &mut Vec<String>) {
    names.sort_by_key(|name| name.to_lowercase());
    names.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
}

fn gtk3_themes() -> Vec<String> {
    // Key themes such as Emacs carry only gtk-keys.css.
    let mut names = entries(&data_folders("themes", ".themes"), |path| {
        path.join("gtk-3.0/gtk.css").is_file()
    });
    names.extend(GTK3_BUILT_IN.map(str::to_string));
    sort_unique(&mut names);
    names
}

// An icon theme lists its folders; a cursor-only theme has none and is left out.
fn icon_themes() -> Vec<String> {
    entries(&data_folders("icons", ".icons"), |path| {
        let name = path.file_name().unwrap_or_default();
        name != "hicolor"
            && name != "default"
            && fs::read_to_string(path.join("index.theme")).is_ok_and(|index| {
                ini::value(&index, "Icon Theme", "Directories").is_some_and(|dirs| !dirs.is_empty())
                    && ini::value(&index, "Icon Theme", "Hidden") != Some("true")
            })
    })
}

fn qt_plugin_folders() -> Vec<PathBuf> {
    std::env::var_os("QT_PLUGIN_PATH")
        .map(|value| std::env::split_paths(&value).collect::<Vec<_>>())
        .unwrap_or_default()
        .into_iter()
        .chain(
            [
                "/run/current-system/sw/lib/qt-6/plugins",
                "/usr/lib/qt6/plugins",
            ]
            .map(PathBuf::from),
        )
        .map(|dir| dir.join("styles"))
        .collect()
}

// A style plugin file is lib<key><Qt major>.so; qt6ct-style only wraps the others.
fn qt_styles() -> Vec<String> {
    let mut names: Vec<String> = entries(&qt_plugin_folders(), |path| {
        path.extension().is_some_and(|extension| extension == "so")
    })
    .into_iter()
    .filter(|file| !file.contains("qt6ct") && !file.contains("qt5ct"))
    .filter_map(|file| {
        let key = file.strip_prefix("lib")?.strip_suffix(".so")?;
        let key = key.trim_end_matches(char::is_numeric);
        let mut letters = key.chars();
        Some(letters.next()?.to_uppercase().chain(letters).collect())
    })
    .collect();
    names.extend(QT_BUILT_IN.map(str::to_string));
    sort_unique(&mut names);
    names
}

fn qt_palettes() -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = xdg::config_home()
        .into_iter()
        .chain(xdg::data_home())
        .chain(xdg::data_dirs())
        .map(|dir| dir.join(QT_TOOLS[0]).join("colors"))
        .filter_map(|dir| fs::read_dir(dir).ok())
        .flatten()
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "conf")
        })
        .collect();
    found.sort_by_key(|path| stem(path).to_lowercase());
    found.dedup_by(|a, b| stem(a) == stem(b));
    found
}

fn config_file(parts: &str) -> PathBuf {
    xdg::config_home().unwrap_or_default().join(parts)
}

fn qt_conf(tool: &str) -> PathBuf {
    config_file(&format!("{tool}/{tool}.conf"))
}

fn gtk4_css() -> PathBuf {
    config_file("gtk-4.0/gtk.css")
}

fn gsettings(args: &[&str]) -> Option<String> {
    let output = Command::new("gsettings").args(args).output().ok()?;
    output.status.success().then(|| {
        String::from_utf8_lossy(&output.stdout)
            .trim()
            .trim_matches('\'')
            .to_string()
    })
}

// GTK on Wayland reads GSettings when its schemas exist and settings.ini otherwise.
fn gtk_value(key: &str, ini_key: &str, default: &str) -> String {
    gsettings(&["get", INTERFACE, key])
        .or_else(|| {
            let ini = fs::read_to_string(config_file("gtk-3.0/settings.ini")).ok()?;
            ini::value(&ini, "Settings", ini_key).map(str::to_string)
        })
        .unwrap_or_else(|| default.to_string())
}

fn set_gtk(key: &str, ini_key: &str, value: Option<&str>) -> io::Result<()> {
    match value {
        Some(value) => gsettings(&["set", INTERFACE, key, value]),
        None => gsettings(&["reset", INTERFACE, key]),
    };
    edit_ini(
        &config_file("gtk-3.0/settings.ini"),
        "Settings",
        &[(ini_key, value)],
    )
}

fn set_qt(keys: &[(&str, Option<&str>)]) -> io::Result<()> {
    QT_TOOLS
        .iter()
        .try_for_each(|tool| edit_ini(&qt_conf(tool), "Appearance", keys))
}

fn set_palette(path: Option<&Path>) -> io::Result<()> {
    let path = path.map(|path| path.display().to_string());
    set_qt(&[
        ("custom_palette", path.as_ref().map(|_| "true")),
        ("color_scheme_path", path.as_deref()),
    ])
}

// A symlinked file belongs to the user's Nix configuration (home-manager) and stays as it is.
fn edit_ini(path: &Path, section: &str, keys: &[(&str, Option<&str>)]) -> io::Result<()> {
    if is_symlink(path) {
        return Err(io::Error::other(tr!(
            "managed-file",
            path = path.display().to_string()
        )));
    }
    let current = fs::read_to_string(path).unwrap_or_default();
    let text = keys.iter().fold(current.clone(), |text, (key, value)| {
        ini::set(&text, section, key, *value)
    });
    if text == current {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, text)
}

fn import_palette(file: &Path) -> io::Result<()> {
    let colors = config_file(&format!("{}/colors", QT_TOOLS[0]));
    fs::create_dir_all(&colors)?;
    let target = colors.join(file.file_name().unwrap_or_default());
    // Copying a file onto itself truncates it.
    if file != target {
        fs::copy(file, &target)?;
    }
    set_palette(Some(&target))
}

// The previous sheet is kept next to it, as gtk.css.bak.
fn import_gtk4_css(file: &Path) -> io::Result<()> {
    let css = gtk4_css();
    if is_symlink(&css) {
        return Err(io::Error::other(tr!(
            "managed-file",
            path = css.display().to_string()
        )));
    }
    let sheet = fs::read(file)?;
    if let Some(parent) = css.parent() {
        fs::create_dir_all(parent)?;
    }
    if css.exists() {
        fs::rename(&css, css.with_extension("css.bak"))?;
    }
    fs::write(&css, sheet)
}

fn remove_gtk4_css() -> io::Result<()> {
    let css = gtk4_css();
    if is_symlink(&css) {
        return Err(io::Error::other(tr!(
            "managed-file",
            path = css.display().to_string()
        )));
    }
    fs::rename(&css, css.with_extension("css.bak"))
}

fn open_themes_folder() {
    let Some(folder) = xdg::data_home().map(|dir| dir.join("themes")) else {
        return;
    };
    if let Err(error) = fs::create_dir_all(&folder) {
        tracing::warn!(%error, "could not create the themes folder");
        return;
    }
    let uri = format!("file://{}", folder.display());
    spawn(async move { bk::portal::open_uri(&uri, false).await });
}

// Back to what GTK and the Qt bridge set with nothing chosen; the user's gtk.css stays.
fn reset() -> io::Result<()> {
    set_gtk("gtk-theme", "gtk-theme-name", None)?;
    set_gtk("icon-theme", "gtk-icon-theme-name", None)?;
    set_qt(&[("style", None), ("icon_theme", None)])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_current_value_is_selected_even_when_no_folder_lists_it() {
        let listed = Choice::new(vec!["Adwaita".into(), "adw-gtk3".into()], "adwaita");
        assert_eq!(listed.selected, 0);
        let unlisted = Choice::new(vec!["Adwaita".into()], "Yaru");
        assert_eq!((unlisted.selected, unlisted.names.len()), (1, 2));
    }
}
