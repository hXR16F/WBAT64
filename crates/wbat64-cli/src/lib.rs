//! `wbat64-cli` — argument parsing, command dispatch and result emission.
//!
//! The binary in the `wbat64` crate is a thin wrapper around [`run`].

pub mod argv;
pub mod exec;
pub mod help;
pub mod output;
pub mod source;
pub mod spec;

use std::path::PathBuf;

use wbat64_core::{parse_bool, Config, Encoding, Error, Result};

pub use argv::Token;
pub use output::Output;
pub use source::SourceRef;
pub use spec::{
    BoxCommand, Command, FillCommand, GlobalOptions, Invocation, ListCommand, TextCommand,
};

/// Parse and run the process command line. Returns the process exit code.
pub fn run() -> i32 {
    run_tokens(argv::raw_args())
}

/// Run an explicit token list (used by tests and embedders).
pub fn run_tokens(tokens: Vec<Token>) -> i32 {
    let mut output = Output::capture();

    if tokens.is_empty() {
        let _ = output.write_str(help::USAGE);
        let _ = output.flush();
        return 0;
    }

    let invocation = match spec::parse_all(tokens) {
        Ok(invocation) => invocation,
        Err(error) => {
            eprintln!("wbat64: {error}");
            return 255;
        }
    };
    let GlobalOptions {
        ref help,
        ref version,
        ..
    } = invocation.options;

    if *version {
        let _ = output.write_str(&format!("wbat64 {}\n", env!("CARGO_PKG_VERSION")));
        let _ = output.flush();
        return 0;
    }
    if *help {
        let _ = output.write_str(help::USAGE);
        let _ = output.flush();
        return 0;
    }
    // Internal helper for w.bat: report whether wbat64 itself can create a file
    // in a directory, then exit without opening any UI.
    if let Some(path) = &invocation.options.probe_write {
        let created = std::fs::write(path, b"").is_ok();
        let removed = !created || std::fs::remove_file(path).is_ok();
        return if created && removed { 0 } else { 1 };
    }
    if invocation.options.plain {
        eprintln!(
            "wbat64: --plain (screen-reader mode) is not implemented yet; omit the flag to draw normally"
        );
        return 255;
    }

    let compat = compat_mode(&invocation.options);
    let (config, warnings) = match load_config(&invocation.options, compat) {
        Ok(result) => result,
        Err(error) => {
            eprintln!("wbat64: {error}");
            return 255;
        }
    };
    for warning in warnings {
        eprintln!("wbat64: {warning}");
    }

    match exec::execute(&config, &invocation.options, &invocation.commands, output) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("wbat64: {error}");
            if config.error_pause {
                exec::error_pause();
            }
            255
        }
    }
}

/// Determine compat mode: the executable name, `WBAT64_COMPAT`, or `--compat`.
fn compat_mode(options: &GlobalOptions) -> bool {
    if let Some(compat) = options.compat {
        return compat;
    }
    if let Ok(value) = std::env::var("WBAT64_COMPAT") {
        if parse_bool(&value).unwrap_or(false) {
            return true;
        }
    }
    std::env::current_exe()
        .ok()
        .and_then(|path| path.file_stem().map(|s| s.to_string_lossy().to_lowercase()))
        .map(|stem| stem == "wbat")
        .unwrap_or(false)
}

fn read_optional(path: &std::path::Path) -> Option<String> {
    std::fs::read(path)
        .ok()
        .map(|bytes| wbat64_core::encoding::decode(&bytes, Encoding::Auto))
}

/// Locate `WBAT.INI`: current directory first, then the executable directory.
fn find_ini(cwd: &std::path::Path, exe_dir: Option<&std::path::Path>) -> Option<PathBuf> {
    let mut candidates = vec![cwd.join("WBAT.INI")];
    if let Some(dir) = exe_dir {
        candidates.push(dir.join("WBAT.INI"));
        candidates.push(dir.join("wbat.ini"));
    }
    candidates.push(cwd.join("wbat.ini"));
    candidates.into_iter().find(|path| path.is_file())
}

fn find_toml(options: &GlobalOptions, cwd: &std::path::Path) -> Result<Option<String>> {
    if let Some(path) = &options.config {
        let path = PathBuf::from(path);
        let text = read_optional(&path)
            .ok_or_else(|| Error::msg(format!("cannot read config file '{}'", path.display())))?;
        return Ok(Some(text));
    }
    let mut candidates = vec![cwd.join("wbat64.toml")];
    if let Ok(appdata) = std::env::var("APPDATA") {
        candidates.push(PathBuf::from(appdata).join("wbat64").join("wbat64.toml"));
    }
    if let Ok(home) = std::env::var("HOME") {
        candidates.push(
            PathBuf::from(home)
                .join(".config")
                .join("wbat64")
                .join("wbat64.toml"),
        );
    }
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        candidates.push(PathBuf::from(xdg).join("wbat64").join("wbat64.toml"));
    }
    Ok(candidates
        .into_iter()
        .find(|path| path.is_file())
        .and_then(|path| read_optional(&path)))
}

/// Build the runtime configuration from INI, TOML and `W`-prefixed overrides.
fn load_config(options: &GlobalOptions, compat: bool) -> Result<(Config, Vec<String>)> {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()));
    let ini = find_ini(&cwd, exe_dir.as_deref()).and_then(|path| read_optional(&path));
    let toml = if compat && options.config.is_none() {
        None
    } else {
        find_toml(options, &cwd)?
    };

    let mut env_map: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for (name, value) in std::env::vars() {
        if name.len() > 1 {
            let first = name.chars().next().unwrap();
            if first == 'W' || first == 'w' {
                env_map.insert(name[1..].to_ascii_lowercase(), value);
            }
        }
    }
    let lookup = |key: &str| env_map.get(&key.to_ascii_lowercase()).cloned();

    let (mut config, warnings) = Config::build(compat, ini.as_deref(), toml.as_deref(), &lookup);
    if config.encoding == Encoding::Oem {
        // Compat mode means "the console's OEM code page", which is CP850 on
        // many European systems, not always CP437.
        config.encoding = oem_encoding();
    }
    let no_color_env = std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty());
    if options.no_color || no_color_env {
        config.theme.monochrome();
    }
    Ok((config, warnings))
}

/// The console's OEM code page, falling back to CP437.
fn oem_encoding() -> Encoding {
    #[cfg(windows)]
    {
        #[link(name = "kernel32")]
        extern "system" {
            fn GetConsoleOutputCP() -> u32;
        }
        let codepage = unsafe { GetConsoleOutputCP() };
        if codepage == 850 {
            return Encoding::Cp850;
        }
    }
    Encoding::Cp437
}
