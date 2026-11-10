use ctappearance::xdg;

pub(crate) fn installed(entry: &str) -> bool {
    xdg::data_home()
        .into_iter()
        .chain(xdg::data_dirs())
        .any(|dir| dir.join(format!("applications/{entry}.desktop")).is_file())
}

// The shell launches apps in their own scope (ADR 0024); Settings asks it to.
pub(crate) fn open_app(entry: &str) {
    let shell = std::env::current_exe()
        .ok()
        .and_then(|exe| Some(exe.parent()?.join("malaysia-shell")))
        .filter(|path| path.is_file())
        .unwrap_or_else(|| "malaysia-shell".into());
    if let Err(error) = std::process::Command::new(shell)
        .args(["open-app", entry])
        .spawn()
        .map(|mut child| std::thread::spawn(move || child.wait()))
    {
        tracing::warn!(%error, entry, "could not open the app");
    }
}
