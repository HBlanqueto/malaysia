use barrier_kit::views::search::fold;
use std::{
    collections::{HashMap, HashSet},
    env,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopEntry {
    pub id: String,
    pub name: String,
    pub generic_name: Option<String>,
    pub keywords: Vec<String>,
    pub icon: Option<String>,
    pub exec: Vec<String>,
    pub path: Option<PathBuf>,
    pub terminal: bool,
    pub startup_wm_class: Option<String>,
}

pub(crate) fn data_home() -> PathBuf {
    env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| {
            PathBuf::from(env::var_os("HOME").unwrap_or_else(|| "/".into())).join(".local/share")
        })
}

pub(crate) fn data_dirs() -> Vec<PathBuf> {
    let system = env::var_os("XDG_DATA_DIRS")
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
    std::iter::once(data_home()).chain(system).collect()
}

pub fn read_entries() -> Vec<DesktopEntry> {
    let locales = locale_candidates();
    let mut seen = HashSet::new();
    let mut entries = Vec::new();
    for dir in data_dirs() {
        let applications = dir.join("applications");
        collect_desktop_files(
            &applications,
            &applications,
            &mut seen,
            &mut entries,
            &locales,
        );
    }
    entries.sort_by_cached_key(|entry| entry.name.to_lowercase());
    entries
}

fn collect_desktop_files(
    root: &Path,
    dir: &Path,
    seen: &mut HashSet<String>,
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
        if seen.contains(&id) {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        seen.insert(id.clone());
        if let Some(entry) = parse_desktop(&text, &id, locales) {
            out.push(entry);
        }
    }
}

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
        let key = key.trim();
        let value = value.trim();
        let value = if is_list(key) {
            value.to_string()
        } else {
            unescape(value)
        };
        values.insert(key.to_string(), value);
    }

    if boolean(&values, "Hidden") || boolean(&values, "NoDisplay") {
        return None;
    }
    if values.get("Type").map(String::as_str) != Some("Application") {
        return None;
    }
    let show_in = values.get("OnlyShowIn").map(String::as_str).unwrap_or("");
    if !show_in.is_empty() && !list_contains(show_in, "Malaysia") {
        return None;
    }
    let not_show_in = values.get("NotShowIn").map(String::as_str).unwrap_or("");
    if list_contains(not_show_in, "Malaysia") {
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
    let name = localized(&values, "Name", locales).filter(|name| !name.is_empty())?;
    let generic_name = localized(&values, "GenericName", locales).filter(|value| !value.is_empty());
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
    values.get(key).cloned()
}

fn boolean(values: &HashMap<String, String>, key: &str) -> bool {
    values
        .get(key)
        .is_some_and(|value| value.eq_ignore_ascii_case("true"))
}

fn is_list(key: &str) -> bool {
    matches!(
        key.split('[').next(),
        Some("Keywords" | "OnlyShowIn" | "NotShowIn")
    )
}

fn split_list(value: &str) -> Vec<String> {
    let mut items = Vec::new();
    let mut item = String::new();
    let mut chars = value.chars();
    while let Some(ch) = chars.next() {
        match (ch, chars.clone().next()) {
            (';', _) => items.push(std::mem::take(&mut item)),
            ('\\', Some(';')) => {
                chars.next();
                item.push(';');
            }
            ('\\', Some(next)) => {
                chars.next();
                item.push(ch);
                item.push(next);
            }
            _ => item.push(ch),
        }
    }
    items.push(item);
    items
        .iter()
        .map(|item| unescape(item).trim().to_owned())
        .filter(|item| !item.is_empty())
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
            Some('f' | 'F' | 'u' | 'U' | 'i' | 'c' | 'k' | 'd' | 'D' | 'n' | 'N' | 'v' | 'm') => {}
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

pub fn rank<'a>(
    entries: impl IntoIterator<Item = &'a DesktopEntry>,
    query: &str,
    running_apps: &[String],
) -> Vec<usize> {
    let query = fold(query.trim());
    let running: Vec<String> = running_apps.iter().map(|app| fold(app)).collect();
    let is_running = |entry: &DesktopEntry| {
        let id = fold(&entry.id);
        let wm_class = entry.startup_wm_class.as_ref().map(|value| fold(value));
        running
            .iter()
            .any(|app| *app == id || wm_class.as_ref() == Some(app))
    };
    let entries = entries.into_iter().enumerate();
    let mut ranked: Vec<(u8, String, usize)> = if query.is_empty() {
        entries
            .map(|(index, entry)| (u8::from(!is_running(entry)), fold(&entry.name), index))
            .collect()
    } else {
        entries
            .filter_map(|(index, entry)| Some((score(entry, &query)?, fold(&entry.name), index)))
            .collect()
    };
    ranked.sort_unstable();
    ranked.into_iter().map(|(.., index)| index).collect()
}

fn score(entry: &DesktopEntry, query: &str) -> Option<u8> {
    let name = fold(&entry.name);
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
        .map(|value| fold(value))
        .unwrap_or_default();
    if entry
        .keywords
        .iter()
        .any(|keyword| fold(keyword).contains(query))
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

    fn ranked_ids<'a>(
        entries: &'a [DesktopEntry],
        query: &str,
        running: &[String],
    ) -> Vec<&'a str> {
        rank(entries, query, running)
            .into_iter()
            .map(|index| entries[index].id.as_str())
            .collect()
    }

    #[test]
    fn locale_beats_the_unlocalised_value() {
        let text =
            "[Desktop Entry]\nType=Application\nName=Firefox\nName[es]=Fuego\nExec=firefox %U\n";
        let entry = parse_desktop(text, "firefox", &locales()).unwrap();
        assert_eq!(entry.name, "Fuego");
    }

    #[test]
    fn unlocalised_keywords_are_read() {
        let text =
            "[Desktop Entry]\nType=Application\nName=Files\nKeywords=folder;browse;\nExec=files\n";
        let entry = parse_desktop(text, "files", &locales()).unwrap();
        assert_eq!(entry.keywords, ["folder", "browse"]);
    }

    #[test]
    fn an_escaped_semicolon_stays_inside_a_list_item() {
        assert_eq!(split_list(r"a\;b;c\\;d;"), ["a;b", "c\\", "d"]);
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
                &format!("[Desktop Entry]\n{base}NotShowIn=Malaysia;\n"),
                "a",
                &[]
            )
            .is_none()
        );
        assert!(
            parse_desktop(
                &format!("[Desktop Entry]\n{base}OnlyShowIn=Malaysia;\n"),
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
        let exec = parse_exec(r#"env FOO=1 app --flag %U "quoted arg" %% %c %m "a\\b" 'c d'"#);
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

        let ids = |query| ranked_ids(&entries, query, &[]);
        assert_eq!(ids("fue"), ["fuego", "files", "campfire"]);
        assert_eq!(ids("fire")[0], "firefox");
    }

    #[test]
    fn ranking_is_accent_insensitive_and_ties_are_alphabetical() {
        let entries = vec![entry("Árbol", "arbol"), entry("Avión", "avion")];
        assert_eq!(ranked_ids(&entries, "arbol", &[])[0], "arbol");
        assert_eq!(ranked_ids(&entries, "", &[]), ["arbol", "avion"]);
    }

    #[test]
    fn running_apps_come_first_for_an_empty_query() {
        let entries = vec![entry("Bravo", "bravo"), entry("Alfa", "alfa")];
        assert_eq!(
            ranked_ids(&entries, "", &["bravo".into()]),
            ["bravo", "alfa"]
        );
    }
}
