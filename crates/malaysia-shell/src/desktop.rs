//! Desktop entries: reading, localisation, Exec cleanup and launcher ranking.
//!
//! The parser is deliberately local to `malaysia-shell` (ADR 0023): it is shell
//! logic, not part of BarrierKit.

use std::{
    collections::HashMap,
    env,
    path::{Path, PathBuf},
};

/// One application read from a `.desktop` file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopEntry {
    /// XDG desktop-file id (path relative to `applications`, `/` → `-`).
    pub id: String,
    /// Localised `Name`.
    pub name: String,
    /// Localised `GenericName`, when present.
    pub generic_name: Option<String>,
    /// Localised `Keywords`.
    pub keywords: Vec<String>,
    /// `Icon` (a name or an absolute path).
    pub icon: Option<String>,
    /// `Exec`, split and without field codes.
    pub exec: Vec<String>,
    /// `Path`, the working directory.
    pub path: Option<PathBuf>,
    /// Whether the app needs a terminal.
    pub terminal: bool,
    /// `StartupWMClass`, used to match running windows when present.
    pub startup_wm_class: Option<String>,
}

/// Reads every application visible to the Yuki desktop.
///
/// Directories are visited in XDG precedence order and the first file wins for
/// each id. NixOS's `~/.nix-profile/share` and `/run/current-system/sw/share`
/// are ordinary `XDG_DATA_DIRS` entries, so they need no special case.
pub fn read_entries() -> Vec<DesktopEntry> {
    let home = env::var_os("HOME").unwrap_or_else(|| "/".into());
    let data_home = env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| PathBuf::from(&home).join(".local/share"));
    let data_dirs = env::var_os("XDG_DATA_DIRS")
        .map(|value| {
            env::split_paths(&value)
                .filter(|path| path.is_absolute())
                .collect::<Vec<_>>()
        })
        .filter(|paths| !paths.is_empty())
        .unwrap_or_else(|| {
            vec![
                PathBuf::from("/usr/local/share"),
                PathBuf::from("/usr/share"),
            ]
        });

    let locales = locale_candidates();
    let mut seen = HashMap::<String, ()>::new();
    let mut entries = Vec::new();
    for dir in std::iter::once(data_home).chain(data_dirs) {
        let applications = dir.join("applications");
        collect_desktop_files(
            &applications,
            &applications,
            &mut seen,
            &mut entries,
            &locales,
        );
    }
    entries.sort_by_key(|entry| entry.name.to_lowercase());
    entries
}

fn collect_desktop_files(
    root: &Path,
    dir: &Path,
    seen: &mut HashMap<String, ()>,
    out: &mut Vec<DesktopEntry>,
    locales: &[String],
) {
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    for item in read.flatten() {
        let path = item.path();
        if path.is_dir() {
            collect_desktop_files(root, &path, seen, out, locales);
            continue;
        }
        if path.extension().and_then(|ext| ext.to_str()) != Some("desktop") {
            continue;
        }
        let Some(relative) = path.strip_prefix(root).ok() else {
            continue;
        };
        let id = relative
            .to_string_lossy()
            .trim_end_matches(".desktop")
            .replace('/', "-");
        if seen.contains_key(&id) {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        // The first file for an id wins even when it is Hidden: it masks the
        // lower-priority entry, it does not fall through to it.
        seen.insert(id.clone(), ());
        if let Some(entry) = parse_desktop(&text, &id, locales) {
            out.push(entry);
        }
    }
}

/// Parses one `[Desktop Entry]` group. `id` is the desktop-file id.
pub fn parse_desktop(text: &str, id: &str, locales: &[String]) -> Option<DesktopEntry> {
    let mut group = String::new();
    let mut values = HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(header) = line
            .strip_prefix('[')
            .and_then(|line| line.strip_suffix(']'))
        {
            group = header.to_string();
            continue;
        }
        if group != "Desktop Entry" {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        values.insert(key.trim().to_string(), unescape(value.trim()));
    }

    if boolean(&values, "Hidden") || boolean(&values, "NoDisplay") {
        return None;
    }
    if values.get("Type").map(String::as_str) != Some("Application") {
        return None;
    }
    if boolean(&values, "Terminal") && !values.contains_key("Exec") {
        return None;
    }
    let show_in = values.get("OnlyShowIn").map(String::as_str).unwrap_or("");
    if !show_in.is_empty() && !list_contains(show_in, "Yuki") {
        return None;
    }
    let not_show_in = values.get("NotShowIn").map(String::as_str).unwrap_or("");
    if list_contains(not_show_in, "Yuki") {
        return None;
    }
    let try_exec = values.get("TryExec").map(String::as_str).unwrap_or("");
    if !try_exec.is_empty() && !executable_exists(try_exec) {
        return None;
    }

    let exec = parse_exec(values.get("Exec")?);
    if exec.is_empty() {
        return None;
    }
    let name = localized(&values, "Name", locales)
        .or_else(|| values.get("Name").cloned())
        .filter(|name| !name.is_empty())?;
    let generic_name = localized(&values, "GenericName", locales)
        .or_else(|| values.get("GenericName").cloned())
        .filter(|value| !value.is_empty());
    let keywords = localized(&values, "Keywords", locales)
        .map(|value| split_list(&value))
        .unwrap_or_default();
    let icon = values
        .get("Icon")
        .cloned()
        .filter(|value| !value.is_empty());
    let path = values.get("Path").map(PathBuf::from).filter(|p| p.is_dir());
    let terminal = boolean(&values, "Terminal");

    Some(DesktopEntry {
        id: id.to_string(),
        name,
        generic_name,
        keywords,
        icon,
        exec,
        path,
        terminal,
        startup_wm_class: values.get("StartupWMClass").cloned(),
    })
}

/// `$LANG`-style locale candidates, most specific first (`es_ES`, then `es`).
pub fn locale_candidates() -> Vec<String> {
    let source = ["LANGUAGE", "LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .find_map(|name| env::var(name).ok().filter(|value| !value.is_empty()));
    let Some(source) = source else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for candidate in source.split(':') {
        let candidate = candidate
            .split('.')
            .next()
            .unwrap_or(candidate)
            .split('@')
            .next()
            .unwrap_or(candidate);
        if candidate.is_empty() || candidate == "C" || candidate == "POSIX" {
            continue;
        }
        if !out.iter().any(|value| value == candidate) {
            out.push(candidate.to_string());
        }
        if let Some(language) = candidate.split('_').next()
            && language != candidate
            && !out.iter().any(|value| value == language)
        {
            out.push(language.to_string());
        }
    }
    out
}

fn localized(values: &HashMap<String, String>, key: &str, locales: &[String]) -> Option<String> {
    for locale in locales {
        if let Some(value) = values.get(&format!("{key}[{locale}]")) {
            return Some(value.clone());
        }
    }
    None
}

fn boolean(values: &HashMap<String, String>, key: &str) -> bool {
    values
        .get(key)
        .is_some_and(|value| value.eq_ignore_ascii_case("true"))
}

fn split_list(value: &str) -> Vec<String> {
    value
        .split(';')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn list_contains(value: &str, wanted: &str) -> bool {
    split_list(value).iter().any(|item| item == wanted)
}

fn executable_exists(program: &str) -> bool {
    let executable = |path: &Path| {
        path.is_file()
            && path
                .metadata()
                .map(|meta| {
                    use std::os::unix::fs::PermissionsExt;
                    meta.permissions().mode() & 0o111 != 0
                })
                .unwrap_or(false)
    };
    let path = Path::new(program);
    if path.components().count() > 1 {
        return executable(path);
    }
    env::var_os("PATH")
        .map(|paths| env::split_paths(&paths).any(|dir| executable(&dir.join(program))))
        .unwrap_or(false)
}

/// Splits an `Exec` value and removes desktop-entry field codes.
///
/// The tokenizer follows the spec's quoting rules: double quotes protect
/// whitespace, `\\` escapes a quote, backtick, dollar or backslash inside
/// double quotes, and `%%` is a literal percent sign. Field codes (`%f`, `%F`,
/// `%u`, `%U`, `%i`, `%c`, `%k`) are removed, as required by the launcher.
pub fn parse_exec(value: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_double = false;
    let mut in_single = false;
    let mut chars = value.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '"' if !in_single => in_double = !in_double,
            '\'' if !in_double => in_single = !in_single,
            '\\' if !in_single => {
                if let Some(next) = chars.next() {
                    match (in_double, next) {
                        (true, '"' | '`' | '$' | '\\') => current.push(next),
                        (true, _) => {
                            current.push('\\');
                            current.push(next);
                        }
                        (false, _) => current.push(next),
                    }
                } else {
                    current.push('\\');
                }
            }
            ch if ch.is_whitespace() && !in_double && !in_single => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(ch),
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }

    tokens
        .into_iter()
        .filter_map(|token| {
            let cleaned = strip_field_codes(&token);
            (!cleaned.is_empty()).then_some(cleaned)
        })
        .collect()
}

fn strip_field_codes(token: &str) -> String {
    let mut out = String::new();
    let mut chars = token.chars();
    while let Some(ch) = chars.next() {
        if ch != '%' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('%') => out.push('%'),
            Some('f' | 'F' | 'u' | 'U' | 'i' | 'c' | 'k') => {}
            Some(other) => {
                out.push('%');
                out.push(other);
            }
            None => out.push('%'),
        }
    }
    out
}

fn unescape(value: &str) -> String {
    let mut out = String::new();
    let mut chars = value.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('s') => out.push(' '),
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('\\') => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// The ranking used by the launcher.
pub fn rank<'a>(
    entries: &'a [DesktopEntry],
    query: &str,
    running_apps: &[String],
) -> Vec<&'a DesktopEntry> {
    let query = normalize(query.trim());
    let running: Vec<String> = running_apps.iter().map(|app| normalize(app)).collect();
    let is_running = |entry: &DesktopEntry| {
        let id = normalize(&entry.id);
        let wm_class = entry
            .startup_wm_class
            .as_ref()
            .map(|value| normalize(value));
        running
            .iter()
            .any(|app| *app == id || wm_class.as_ref() == Some(app))
    };

    if query.is_empty() {
        let mut out: Vec<&DesktopEntry> = entries.iter().collect();
        out.sort_by_key(|entry| (!is_running(entry), normalize(&entry.name)));
        return out;
    }

    let mut scored: Vec<(u8, &DesktopEntry)> = entries
        .iter()
        .filter_map(|entry| score(entry, &query).map(|score| (score, entry)))
        .collect();
    scored.sort_by_key(|(score, entry)| (*score, normalize(&entry.name)));
    scored.into_iter().map(|(_, entry)| entry).collect()
}

fn score(entry: &DesktopEntry, query: &str) -> Option<u8> {
    let name = normalize(&entry.name);
    if name.starts_with(query) {
        return Some(0);
    }
    if word_start(&name, query) {
        return Some(1);
    }
    if name.contains(query) {
        return Some(2);
    }
    let generic = entry
        .generic_name
        .as_ref()
        .map(|value| normalize(value))
        .unwrap_or_default();
    if entry
        .keywords
        .iter()
        .any(|keyword| normalize(keyword).contains(query))
        || generic.contains(query)
    {
        return Some(3);
    }
    None
}

fn word_start(name: &str, query: &str) -> bool {
    name.char_indices().any(|(index, _)| {
        if index == 0 {
            return false;
        }
        let before = name[..index].chars().next_back();
        let boundary = before.is_none_or(|ch| !ch.is_alphanumeric());
        boundary && name[index..].starts_with(query)
    })
}

/// Lowercases and strips common Latin accents for accent-insensitive search.
pub fn normalize(value: &str) -> String {
    value
        .to_lowercase()
        .chars()
        .map(|ch| match ch {
            'á' | 'à' | 'ä' | 'â' | 'ã' | 'å' => 'a',
            'é' | 'è' | 'ë' | 'ê' => 'e',
            'í' | 'ì' | 'ï' | 'î' => 'i',
            'ó' | 'ò' | 'ö' | 'ô' | 'õ' => 'o',
            'ú' | 'ù' | 'ü' | 'û' => 'u',
            'ñ' => 'n',
            'ç' => 'c',
            other => other,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn locales() -> Vec<String> {
        vec!["es".to_string()]
    }

    fn entry(name: &str, id: &str) -> DesktopEntry {
        DesktopEntry {
            id: id.to_string(),
            name: name.to_string(),
            generic_name: None,
            keywords: Vec::new(),
            icon: None,
            exec: vec![name.to_string()],
            path: None,
            terminal: false,
            startup_wm_class: None,
        }
    }

    #[test]
    fn locale_beats_the_unlocalised_value() {
        let text =
            "[Desktop Entry]\nType=Application\nName=Firefox\nName[es]=Fuego\nExec=firefox %U\n";
        let entry = parse_desktop(text, "firefox", &locales()).unwrap();
        assert_eq!(entry.name, "Fuego");
    }

    #[test]
    fn hidden_nodisplay_and_wrong_desktops_are_skipped() {
        let base = "Type=Application\nName=App\nExec=app\n";
        assert!(
            parse_desktop(&format!("[Desktop Entry]\n{base}Hidden=true\n"), "a", &[]).is_none()
        );
        assert!(
            parse_desktop(
                &format!("[Desktop Entry]\n{base}NoDisplay=true\n"),
                "a",
                &[]
            )
            .is_none()
        );
        assert!(
            parse_desktop(
                &format!("[Desktop Entry]\n{base}OnlyShowIn=GNOME;\n"),
                "a",
                &[]
            )
            .is_none()
        );
        assert!(
            parse_desktop(
                &format!("[Desktop Entry]\n{base}NotShowIn=Yuki;\n"),
                "a",
                &[]
            )
            .is_none()
        );
        assert!(
            parse_desktop(
                &format!("[Desktop Entry]\n{base}OnlyShowIn=Yuki;\n"),
                "a",
                &[]
            )
            .is_some()
        );
    }

    #[test]
    fn try_exec_must_exist() {
        let text = "[Desktop Entry]\nType=Application\nName=Shell\nExec=sh\nTryExec=/bin/sh\n";
        assert!(parse_desktop(text, "sh", &[]).is_some());
        let text = "[Desktop Entry]\nType=Application\nName=No\nExec=no\nTryExec=/no/such/bin\n";
        assert!(parse_desktop(text, "no", &[]).is_none());
    }

    #[test]
    fn exec_drops_field_codes_and_honours_quotes_and_escapes() {
        let exec = parse_exec(r#"env FOO=1 app --flag %U "quoted arg" %% %c "a\\b" 'c d'"#);
        assert_eq!(
            exec,
            vec![
                "env",
                "FOO=1",
                "app",
                "--flag",
                "quoted arg",
                "%",
                "a\\b",
                "c d"
            ]
        );
    }

    #[test]
    fn ranking_orders_prefix_word_substring_and_keywords() {
        let fuego = entry("Fuego", "fuego");
        let mut campfire = entry("Campfire", "campfire");
        campfire.keywords = vec!["fuego".into()];
        let mut files = entry("Archivos", "files");
        files.generic_name = Some("Gestor de fuego".into());
        let mut firefox = entry("Firefox", "firefox");
        firefox.generic_name = Some("Navegador web".into());
        let entries = vec![firefox, files, campfire, fuego];

        let ranked = rank(&entries, "fue", &[]);
        assert_eq!(ranked[0].id, "fuego");
        assert_eq!(
            ranked
                .iter()
                .map(|entry| entry.id.as_str())
                .collect::<Vec<_>>(),
            vec!["fuego", "files", "campfire"]
        );
        let ranked = rank(&entries, "fire", &[]);
        assert_eq!(ranked[0].id, "firefox");
    }

    #[test]
    fn ranking_is_accent_insensitive_and_ties_are_alphabetical() {
        let entries = vec![entry("Árbol", "arbol"), entry("Avión", "avion")];
        let ranked = rank(&entries, "arbol", &[]);
        assert_eq!(ranked[0].id, "arbol");
        let ranked = rank(&entries, "", &[]);
        assert_eq!(
            ranked
                .iter()
                .map(|entry| entry.id.as_str())
                .collect::<Vec<_>>(),
            vec!["arbol", "avion"]
        );
    }

    #[test]
    fn running_apps_come_first_for_an_empty_query() {
        let entries = vec![entry("Bravo", "bravo"), entry("Alfa", "alfa")];
        let ranked = rank(&entries, "", &["bravo".into()]);
        assert_eq!(
            ranked
                .iter()
                .map(|entry| entry.id.as_str())
                .collect::<Vec<_>>(),
            vec!["bravo", "alfa"]
        );
    }
}
