// https://systemd.io/DESKTOP_ENVIRONMENTS/: app[-<launcher>]-<ApplicationID>-<RANDOM>.scope,
// with the ApplicationID escaped by systemd-escape so each `-` splits a field.
const LAUNCHER: &str = "malaysia";

pub(crate) fn scope_name(app_id: &str, random: u64) -> String {
    format!("app-{LAUNCHER}-{}-{random}.scope", escape(app_id))
}

pub(crate) fn app_id_from_cgroup(cgroup: &str) -> Option<String> {
    cgroup.lines().find_map(|line| {
        let unit = line.rsplit('/').next()?;
        let fields: Vec<&str> = unit.strip_suffix(".scope")?.split('-').collect();
        match fields.as_slice() {
            ["app", _, id, _] | ["app", id, _] => unescape(id),
            _ => None,
        }
    })
}

fn escape(app_id: &str) -> String {
    app_id
        .bytes()
        .map(|byte| match byte {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b':' | b'_' | b'.' => {
                (byte as char).to_string()
            }
            _ => format!("\\x{byte:02x}"),
        })
        .collect()
}

fn unescape(field: &str) -> Option<String> {
    let mut bytes = Vec::with_capacity(field.len());
    let mut rest = field.as_bytes();
    while let Some((&byte, tail)) = rest.split_first() {
        if byte == b'\\' {
            let hex = std::str::from_utf8(tail.get(1..3)?).ok()?;
            bytes.push(u8::from_str_radix(hex, 16).ok()?);
            rest = &tail[3..];
        } else {
            bytes.push(byte);
            rest = tail;
        }
    }
    String::from_utf8(bytes).ok().filter(|id| !id.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_scope_named_by_the_shell_gives_back_its_app_id() {
        let unit = scope_name("google-chrome", 42);
        assert_eq!(unit, "app-malaysia-google\\x2dchrome-42.scope");
        let cgroup = format!("0::/user.slice/user-1000.slice/user@1000.service/app.slice/{unit}\n");
        assert_eq!(
            app_id_from_cgroup(&cgroup).as_deref(),
            Some("google-chrome")
        );
    }

    #[test]
    fn scopes_of_other_launchers_and_services_are_read_or_skipped() {
        let gnome = "0::/user.slice/app.slice/app-gnome-org.gnome.Nautilus-4242.scope";
        assert_eq!(
            app_id_from_cgroup(gnome).as_deref(),
            Some("org.gnome.Nautilus")
        );
        let service = "0::/user.slice/user@1000.service/session.slice/malaysia-shell@dock.service";
        assert_eq!(app_id_from_cgroup(service), None);
        let unnamed = "0::/user.slice/user@1000.service/app.slice/run-u12.scope";
        assert_eq!(app_id_from_cgroup(unnamed), None);
    }
}
