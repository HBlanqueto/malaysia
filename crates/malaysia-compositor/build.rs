fn main() {
    println!("cargo:rerun-if-env-changed=MALAYSIA_DEV_SHELL");
    if std::env::var("MALAYSIA_DEV_SHELL").unwrap_or_default() != "1" {
        eprintln!("error: not inside the dev shell; run 'nix develop' first (see ADR 0007)");
        std::process::exit(1);
    }
}
