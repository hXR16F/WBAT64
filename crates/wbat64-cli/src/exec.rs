//! Command execution: from parsed [`Command`]s to exit codes and results.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use wbat64_core::{
    ButtonResult, Config, ControlKind, EmitMode, Error, ListResult, Pos, Rect, Result, Results,
    Style, Unit,
};
use wbat64_tui::{
    run_dialog, run_list, DialogOptions, DialogResult, ListOptions, Outcome, Session,
    SessionOptions,
};

use crate::output::Output;
use crate::source::{self, SourceRef};
use crate::spec::{BoxCommand, Command, FillCommand, GlobalOptions, ListCommand, TextCommand};

/// Execute a fully parsed invocation.
pub fn execute(
    config: &Config,
    options: &GlobalOptions,
    commands: &[Command],
    output: Output,
) -> Result<i32> {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf));
    let mut runtime = Runtime {
        config,
        options: options.clone(),
        output,
        session: None,
        results: Results::default(),
        exit_code: 0,
        cwd,
        exe_dir,
        vars: build_vars(options),
    };
    runtime.run(commands)?;
    runtime.finish()
}

fn build_vars(options: &GlobalOptions) -> HashMap<String, String> {
    let mut vars: HashMap<String, String> = std::env::vars().collect();
    for (name, value) in &options.sets {
        vars.insert(name.clone(), value.clone());
    }
    vars
}

fn lookup_var(vars: &HashMap<String, String>, name: &str) -> Option<String> {
    if let Some(value) = vars.get(name) {
        return Some(value.clone());
    }
    vars.iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.clone())
}

struct Runtime<'a> {
    config: &'a Config,
    options: GlobalOptions,
    output: Output,
    session: Option<Session>,
    results: Results,
    exit_code: i32,
    cwd: PathBuf,
    exe_dir: Option<PathBuf>,
    vars: HashMap<String, String>,
}

impl<'a> Runtime<'a> {
    fn lookup(&self, name: &str) -> Option<String> {
        lookup_var(&self.vars, name)
    }

    fn interpolate(&self, unit: &Unit) -> Unit {
        let vars = &self.vars;
        unit.interpolate(&|name| lookup_var(vars, name))
    }

    fn ensure_session(&mut self) -> Result<()> {
        if self.session.is_none() {
            let options = SessionOptions {
                mouse: self.config.mouse && !self.options.no_mouse,
                keys: self.options.keys.clone(),
                headless_size: self.options.headless_size,
                dump_screen: self.options.dump_screen,
            };
            self.session = Some(Session::open(self.config, &options)?);
        }
        Ok(())
    }

    fn run(&mut self, commands: &[Command]) -> Result<()> {
        for command in commands {
            match command {
                Command::Box(box_command) => self.do_box(box_command, false)?,
                Command::Menu(box_command) => self.do_box(box_command, true)?,
                Command::List(list_command) => self.do_list(list_command)?,
                Command::Text(text_command) => self.do_text(text_command)?,
                Command::Fill(fill_command) => self.do_fill(fill_command)?,
                Command::Cls { colors } => self.do_cls(*colors)?,
                Command::Cursor { line, feeds } => {
                    self.ensure_session()?;
                    let session = self.session.as_mut().unwrap();
                    session.place_cursor(*line, *feeds)?;
                }
                Command::Save { id } => {
                    self.ensure_session()?;
                    self.session.as_mut().unwrap().save_screen(id.clone());
                }
                Command::Restore { id } => {
                    self.ensure_session()?;
                    let session = self.session.as_mut().unwrap();
                    if !session.restore_screen(id.clone())? {
                        return Err(Error::msg(format!(
                            "restore: no saved screen{}",
                            id.as_deref()
                                .map(|i| format!(" with id '{i}'"))
                                .unwrap_or_default()
                        )));
                    }
                    session.present()?;
                }
                Command::Stdout { source } => self.do_stdout(source)?,
                Command::El { code } => {
                    self.exit_code = *code as i32;
                }
                Command::AnsiCheck => {
                    self.exit_code = if wbat64_tui::term::console_supports_ansi() {
                        0
                    } else {
                        1
                    };
                }
                Command::Pal { path } => self.do_pal(path.as_deref())?,
                Command::Font { size } => {
                    let _ = size;
                    eprintln!(
                        "wbat64: font is not supported; wbat64 cannot load DOS bitmap .FON/.FNT fonts"
                    );
                    self.exit_code = 0;
                }
                Command::Check { source } => self.do_check(source.as_ref())?,
                Command::Version => {
                    self.output
                        .write_str(&format!("wbat64 {}\n", env!("CARGO_PKG_VERSION")))?;
                }
                Command::Help => {
                    self.output.write_str(crate::help::USAGE)?;
                }
            }
        }
        Ok(())
    }

    fn load_unit(&self, source: &SourceRef) -> Result<Unit> {
        let document = source::read_layout_document(
            source,
            self.config,
            self.options.encoding,
            &self.cwd,
            self.exe_dir.as_deref(),
        )?;
        if let Some(error) = document.errors.first() {
            // Diagnostics in *other* units do not block this one; `wbat64 check`
            // reports them. A broken line inside the selected unit leaves an
            // empty line behind, which is still usable.
            let _ = error;
        }
        Ok(source::select_unit(&document, source.label.as_deref())?.clone())
    }

    fn do_box(&mut self, command: &BoxCommand, menu: bool) -> Result<()> {
        let base = match &command.source {
            Some(source) => self.load_unit(source)?,
            None => wbat64_core::quick_box(
                command.text.as_deref().unwrap_or(""),
                &command.items,
                command.title.as_deref(),
            ),
        };
        let mut unit = self.interpolate(&base);
        if let Some(error) = unit.validate().first() {
            return Err(Error::msg(error.to_string()));
        }
        if unit.controls().is_empty() {
            let ok = wbat64_core::quick_box("", &["OK".to_string()], None);
            unit.lines.extend(ok.lines);
        }

        let mut initial: HashMap<String, String> = HashMap::new();
        for control in unit.inputs() {
            if let Some(var) = control.var_name() {
                if let Some(value) = self.lookup(var) {
                    initial.insert(var.to_string(), value);
                }
            }
        }
        for i in 1..=9 {
            let name = format!("WCB{i}");
            if let Some(value) = self.lookup(&name) {
                initial.insert(name, value);
            }
        }
        if let Some(value) = self.lookup("WRB") {
            initial.insert("WRB".to_string(), value);
        }

        let dialog_options = DialogOptions {
            title: command.title.clone(),
            pos: command.pos,
            colors: command.colors,
            timeout: command.timeout,
            bar: command.bar.map(|b| b as usize),
            fix: command.fix,
            mouse: self.config.mouse && !self.options.no_mouse,
            close: unit.close,
            initial,
            max_width: self.config.max_width,
            strict_width: self.config.compat && command.source.is_some(),
            menu,
        };

        self.ensure_session()?;
        let config = self.config;
        let session = self.session.as_mut().unwrap();
        let result = run_dialog(session, config, &unit, &dialog_options)?;
        self.apply_box_result(&unit, result);
        Ok(())
    }

    fn apply_box_result(&mut self, unit: &Unit, result: DialogResult) {
        let escaped = result.outcome.is_escape();
        match &result.outcome {
            Outcome::Button { index, name } => {
                self.exit_code = *index as i32;
                self.results.wbat = Some(name.clone());
                self.results.button = Some(ButtonResult {
                    index: *index,
                    name: name.clone(),
                });
            }
            Outcome::Timeout {
                index: Some(index),
                name,
            } => {
                self.exit_code = *index as i32;
                let word = name.clone().unwrap_or_default();
                self.results.wbat = Some(word.clone());
                self.results.button = Some(ButtonResult {
                    index: *index,
                    name: word,
                });
            }
            _ => {
                self.exit_code = 100;
                self.results.wbat = Some(escape_word(unit));
            }
        }
        self.results.escaped = escaped;
        if !escaped || self.config.keep_input_on_esc {
            self.results.inputs = result.inputs;
            self.results.checkboxes = result.checkboxes;
            self.results.radio = result.radio;
        }
    }

    fn do_list(&mut self, command: &ListCommand) -> Result<()> {
        let items = if command.stdin {
            let mut bytes = Vec::new();
            use std::io::Read;
            std::io::stdin()
                .read_to_end(&mut bytes)
                .map_err(|e| Error::io("reading the list from stdin", e))?;
            let encoding = self.options.encoding.unwrap_or(self.config.encoding);
            let text = wbat64_core::decode(&bytes, encoding);
            source::split_lines(&text)
        } else {
            let reference = command.source.clone().unwrap_or(SourceRef {
                path: command.path.clone().unwrap_or_default(),
                label: None,
                plus: false,
            });
            if reference.label.is_some() {
                // `list file:label` lists the text lines of a layout unit.
                let document = source::read_layout_document(
                    &reference,
                    self.config,
                    self.options.encoding,
                    &self.cwd,
                    self.exe_dir.as_deref(),
                )?;
                let unit = source::select_unit(&document, reference.label.as_deref())?;
                unit.lines
                    .iter()
                    .map(|line| line.plain())
                    .filter(|line| !line.trim().is_empty())
                    .collect()
            } else {
                source::read_list_lines(
                    &reference,
                    self.config,
                    self.options.encoding,
                    &self.cwd,
                    self.exe_dir.as_deref(),
                )?
            }
        };

        let list_options = ListOptions {
            title: command.title.clone().or_else(|| {
                command
                    .source
                    .as_ref()
                    .map(|s| s.display())
                    .filter(|s| !s.is_empty())
            }),
            pos: command.pos,
            colors: command.colors,
            width: command.width,
            height: command.height,
            initial: command.bar.map(|b| b as usize),
            multi: command.multi,
            max_width: if self.options.truncate {
                Some(78)
            } else {
                self.config.max_width
            },
            fix: command.fix,
            mouse: self.config.mouse && !self.options.no_mouse,
            hot_ticks: self.config.list_hot_ticks,
            filter: command.filter,
        };

        self.ensure_session()?;
        let config = self.config;
        let session = self.session.as_mut().unwrap();
        let result = run_list(session, config, &items, &list_options)?;

        if result.escaped {
            self.exit_code = 254;
            self.results.escaped = true;
            self.results.wbat = Some(String::new());
            self.results.exit_var = Some("0".to_string());
        } else {
            self.exit_code = 0;
            self.results.escaped = false;
            let index = result.index.unwrap_or(0);
            let mut extra = result.selected.clone();
            if !extra.is_empty() {
                extra.remove(0);
            }
            self.results.list = Some(ListResult {
                index,
                item: result.item.clone(),
                extra,
            });
            self.results.wbat = Some(result.item.clone());
            self.results.exit_var = Some(index.to_string());
        }
        Ok(())
    }

    fn do_text(&mut self, command: &TextCommand) -> Result<()> {
        let unit = match &command.source {
            Some(source) => self.load_unit(source)?,
            None => {
                let text = command.text.clone().unwrap_or_default().replace('^', "\n");
                let document = format!(":text\n{text}\n:");
                let (mut units, _) =
                    wbat64_core::parse_layout(&document, "text", self.config.hi_symbol);
                units.pop().unwrap_or_else(|| Unit {
                    label: "text".to_string(),
                    title: None,
                    close: false,
                    close_symbol: 'x',
                    lines: Vec::new(),
                    file: "text".to_string(),
                    line_no: 0,
                    center: false,
                })
            }
        };
        let mut unit = self.interpolate(&unit);
        if let Some(title) = &command.title {
            unit.title = Some(title.clone());
        }
        self.ensure_session()?;
        let session = self.session.as_mut().unwrap();
        let pos = command.pos.unwrap_or(Pos::new(1, 1));
        session.draw_text(&unit, pos, command.colors)?;
        session.present()?;
        Ok(())
    }

    fn do_fill(&mut self, command: &FillCommand) -> Result<()> {
        self.ensure_session()?;
        let (cols, rows) = (
            self.session.as_ref().unwrap().width(),
            self.session.as_ref().unwrap().height(),
        );
        let pos = command.pos.unwrap_or(Pos::new(1, 1));
        let x = pos.col.saturating_sub(1).min(cols.saturating_sub(1));
        let y = pos.line.saturating_sub(1).min(rows.saturating_sub(1));
        let (h, w) = command
            .size
            .unwrap_or((rows.saturating_sub(y).max(1), cols.saturating_sub(x).max(1)));
        let rect = Rect::new(
            x,
            y,
            w.min(cols.saturating_sub(x)),
            h.min(rows.saturating_sub(y)),
        );
        let style = command.colors.unwrap_or(self.config.theme.screen);
        let pattern = command.pattern.clone().unwrap_or_default();
        let session = self.session.as_mut().unwrap();
        session.fill(rect, &pattern, style, command.shift);
        session.present()?;
        Ok(())
    }

    fn do_cls(&mut self, colors: Option<Style>) -> Result<()> {
        self.ensure_session()?;
        let style = colors.unwrap_or(self.config.theme.screen);
        let session = self.session.as_mut().unwrap();
        session.clear(style)?;
        Ok(())
    }

    fn do_stdout(&mut self, source: &SourceRef) -> Result<()> {
        let unit = self.load_unit(source)?;
        let unit = self.interpolate(&unit);
        let mut text = unit.plain_text();
        text.push('\n');
        self.output.write_str(&text)
    }

    fn do_check(&mut self, reference: Option<&SourceRef>) -> Result<()> {
        let reference = reference.cloned().unwrap_or(SourceRef {
            path: String::new(),
            label: None,
            plus: false,
        });
        let document = source::read_layout_document(
            &reference,
            self.config,
            self.options.encoding,
            &self.cwd,
            self.exe_dir.as_deref(),
        )?;
        let mut out = String::new();
        let mut code = 0;
        match &reference.label {
            Some(label) => match wbat64_core::find_unit(&document.units, label) {
                Some(unit) => {
                    let issues = unit.validate();
                    if issues.is_empty() {
                        out.push_str("OK: 1 unit\n");
                    } else {
                        for error in &issues {
                            out.push_str(&format!("{error}\n"));
                        }
                        code = 1;
                    }
                }
                None => {
                    out.push_str(&format!("{}: label ':{label}' not found\n", document.file));
                    code = 1;
                }
            },
            None => {
                let mut issues: Vec<String> =
                    document.errors.iter().map(ToString::to_string).collect();
                for unit in &document.units {
                    issues.extend(unit.validate().iter().map(ToString::to_string));
                }
                if issues.is_empty() {
                    let count = document.units.len();
                    out.push_str(&format!(
                        "OK: {count} unit{}\n",
                        if count == 1 { "" } else { "s" }
                    ));
                } else {
                    for error in &issues {
                        out.push_str(error);
                        out.push('\n');
                    }
                    code = 1;
                }
            }
        }
        self.output.write_str(&out)?;
        self.exit_code = code;
        Ok(())
    }

    fn do_pal(&mut self, path: Option<&str>) -> Result<()> {
        let raw = path
            .map(str::to_string)
            .unwrap_or_else(|| self.config.pal_file.clone());
        let plus = raw.starts_with('+');
        let cleaned = raw.strip_prefix('+').unwrap_or(&raw);
        let mut file = PathBuf::from(cleaned);
        if plus {
            if let Some(dir) = &self.exe_dir {
                file = dir.join(&file);
            }
        } else if file.is_relative() {
            file = self.cwd.join(&file);
        }
        let text = std::fs::read_to_string(&file)
            .map_err(|e| Error::io(format!("reading '{}'", file.display()), e))?;
        let colors = parse_palette(&text)?;
        self.ensure_session()?;
        let session = self.session.as_mut().unwrap();
        session.apply_palette(&colors)?;
        Ok(())
    }

    /// Encode batch output. Defaults to the console OEM code page unless the
    /// caller asked for UTF-8 or CP850 explicitly.
    fn bat_bytes(&self, text: &str) -> Vec<u8> {
        match self.options.encoding {
            Some(wbat64_core::Encoding::Utf8) => text.as_bytes().to_vec(),
            Some(wbat64_core::Encoding::Cp850) => {
                wbat64_core::encoding::encode_oem(text, wbat64_core::Encoding::Cp850)
            }
            _ => wbat64_core::encoding::encode_oem(text, oem_encoding()),
        }
    }

    fn emit_mode(&self) -> EmitMode {
        if let Some(mode) = self.options.emit {
            return mode;
        }
        if self.options.emit_file.is_some() {
            return EmitMode::None;
        }
        if self.output.stdout_is_bat() {
            return EmitMode::Bat;
        }
        EmitMode::None
    }

    fn finish(mut self) -> Result<i32> {
        let mut dump = None;
        if let Some(session) = self.session.take() {
            if self.options.dump_screen {
                dump = Some(session.dump_text());
            }
            drop(session);
        }

        self.results.exit_code = self.exit_code;
        self.results.set_exit_var = self.config.el_var;
        self.results.prefix = self.options.prefix.clone();
        self.results.delayed = self.options.delayed;

        if let Some(text) = dump {
            self.output.write_str(&text)?;
        }

        if let Some(path) = self.options.emit_file.clone() {
            let mut content = String::new();
            for (name, value) in self.results.variables()? {
                content.push_str(&wbat64_core::emit::bat_set(
                    &name,
                    &value,
                    self.results.delayed,
                )?);
            }
            // cmd.exe reads batch files in the OEM code page; an explicit
            // `--encoding utf8|cp850` selects a different batch encoding.
            let bytes = self.bat_bytes(&content);
            write_result_file(&path, &bytes)
                .map_err(|e| Error::io(format!("writing '{path}'"), e))?;
        }

        let mode = self.emit_mode();
        if mode != EmitMode::None {
            let text = self.results.emit(mode)?;
            if mode == EmitMode::Bat && !self.output.is_console() {
                let bytes = self.bat_bytes(&text);
                self.output.write_bytes(&bytes)?;
            } else {
                self.output.write_str(&text)?;
            }
        }
        self.output.flush()?;
        Ok(self.exit_code)
    }
}

/// The `[? ...]` button's word, else the last button's word, else empty.
fn escape_word(unit: &Unit) -> String {
    let buttons = unit.buttons();
    buttons
        .iter()
        .find(|control| control.kind() == ControlKind::EscapeButton)
        .or_else(|| buttons.last())
        .map(|control| control.command_word())
        .unwrap_or_default()
}

fn parse_palette(text: &str) -> Result<[[u8; 3]; 16]> {
    let mut colors = [[0u8; 3]; 16];
    let mut count = 0usize;
    for line in text.lines() {
        let line = line.split(';').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if count >= 16 {
            break;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 3 {
            return Err(Error::msg(format!(
                "PAL: expected 'R G B' in line '{line}'"
            )));
        }
        let mut rgb = [0u8; 3];
        for (i, part) in parts.iter().take(3).enumerate() {
            let value: u16 = part
                .parse()
                .map_err(|_| Error::msg(format!("PAL: invalid component '{part}'")))?;
            if value > 63 {
                return Err(Error::msg(format!(
                    "PAL: component {value} out of range 0..63"
                )));
            }
            rgb[i] = wbat64_core::color::dos_component_to_8bit(value as u8);
        }
        colors[count] = rgb;
        count += 1;
    }
    if count < 16 {
        return Err(Error::msg(format!(
            "PAL: expected 16 colors, found {count}"
        )));
    }
    Ok(colors)
}

/// Write the temporary result batch.
///
/// A previously created file can make this fail with "access denied" when it
/// still has the read-only attribute, and antivirus/indexers occasionally hold
/// freshly created `.bat` files for a moment. Clear the attribute, then retry
/// briefly before giving up.
fn write_result_file(path: &str, bytes: &[u8]) -> std::io::Result<()> {
    match std::fs::write(path, bytes) {
        Ok(()) => Ok(()),
        Err(first) if first.kind() == std::io::ErrorKind::PermissionDenied => {
            if let Ok(metadata) = std::fs::metadata(path) {
                let mut permissions = metadata.permissions();
                #[allow(clippy::permissions_set_readonly_false)]
                permissions.set_readonly(false);
                let _ = std::fs::set_permissions(path, permissions);
            }
            let _ = std::fs::remove_file(path);
            let mut last = first;
            for _ in 0..8 {
                std::thread::sleep(Duration::from_millis(25));
                match std::fs::write(path, bytes) {
                    Ok(()) => return Ok(()),
                    Err(error) => last = error,
                }
            }
            Err(last)
        }
        Err(other) => Err(other),
    }
}

/// The code page `cmd.exe` will use to read a batch file.
fn oem_encoding() -> wbat64_core::Encoding {
    #[cfg(windows)]
    {
        #[link(name = "kernel32")]
        extern "system" {
            fn GetConsoleOutputCP() -> u32;
        }
        let codepage = unsafe { GetConsoleOutputCP() };
        if codepage == 850 {
            return wbat64_core::Encoding::Cp850;
        }
    }
    wbat64_core::Encoding::Cp437
}

/// Pause after an error when `ErrorPause` is set and a console is available.
pub fn error_pause() {
    use std::io::IsTerminal;
    if !std::io::stderr().is_terminal() {
        return;
    }
    eprintln!();
    eprintln!("Press any key to continue . . .");
    wbat64_tui::term::wait_key(Duration::from_secs(60));
}

#[allow(dead_code)]
fn _unused() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writing_results_replaces_a_read_only_leftover() {
        let path = std::env::temp_dir().join(format!(
            "wbat64_readonly_{}_{}.bat",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::write(&path, b"old").unwrap();
        let mut permissions = std::fs::metadata(&path).unwrap().permissions();
        #[allow(clippy::permissions_set_readonly_false)]
        permissions.set_readonly(true);
        std::fs::set_permissions(&path, permissions).unwrap();

        write_result_file(path.to_str().unwrap(), b"new").unwrap();
        let data = std::fs::read(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!(data, b"new");
    }
}
