//! Fails the build when it runs outside the dev shell, where the mold linker
//! flag is absent and the resulting binary would be linked differently
//! (ADR 0007). `dev.sh` checks the same marker earlier and more cheaply.

fn main() {
    println!("cargo:rerun-if-env-changed=YUKI_DEV_SHELL");
    if std::env::var("YUKI_DEV_SHELL").unwrap_or_default() != "1" {
        eprintln!("error: not inside the dev shell; run 'nix develop' first (see ADR 0007)");
        std::process::exit(1);
    }
}
