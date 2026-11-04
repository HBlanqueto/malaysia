//! Processes the shell launches: no zombies, and apps outside the component's cgroup.

use std::ffi::OsStr;
use std::io;
use std::os::unix::process::CommandExt;
use std::process::Command;

pub(crate) fn app_command(program: impl AsRef<OsStr>) -> Command {
    // A component restart stops its whole cgroup; apps get their own scope under app.slice.
    if std::env::var_os("INVOCATION_ID").is_none() {
        return Command::new(program);
    }
    let mut command = Command::new("systemd-run");
    command
        .args([
            "--user",
            "--scope",
            "--slice=app.slice",
            "--collect",
            "--quiet",
            "--",
        ])
        .arg(program);
    command
}

pub(crate) fn detached(mut command: Command) -> io::Result<()> {
    // SAFETY: setsid, fork and _exit are async-signal-safe, as pre_exec requires.
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() < 0 {
                return Err(io::Error::last_os_error());
            }
            match libc::fork() {
                -1 => Err(io::Error::last_os_error()),
                0 => Ok(()),
                _ => libc::_exit(0),
            }
        });
    }
    command.spawn()?.wait()?;
    Ok(())
}
