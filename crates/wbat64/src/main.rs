//! Thin `wbat64` binary: everything lives in the library crates.

fn main() {
    std::process::exit(wbat64::run_cli());
}
