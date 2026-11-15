//! The wbat64 command grammar.
//!
//! The syntax mixes WBAT positional tokens (`5,12`, `(white on blue)`, `#3,15`,
//! `@file:label`) with long options (`--pos 5,12`). Options are extracted first
//! (they are global), then each `!`/`--and` segment is parsed into a command.

use wbat64_core::{
    parse_colors, parse_pos, parse_size, EmitMode, Encoding, Error, Pos, Result, Style,
};

use crate::argv::{split_commands, Token};
use crate::source::SourceRef;

/// Global options, valid anywhere on the command line.
#[derive(Clone, Debug, Default)]
pub struct GlobalOptions {
    pub help: bool,
    pub version: bool,
    pub emit: Option<EmitMode>,
    pub emit_file: Option<String>,
    pub prefix: String,
    pub encoding: Option<Encoding>,
    pub sets: Vec<(String, String)>,
    pub keys: Option<String>,
    pub dump_screen: bool,
    pub delayed: bool,
    pub no_mouse: bool,
    pub no_color: bool,
    pub truncate: bool,
    pub compat: Option<bool>,
    pub config: Option<String>,
    pub plain: bool,
    pub headless_size: Option<(u16, u16)>,
    pub pos: Option<Pos>,
    pub colors: Option<Style>,
    pub bar: Option<u32>,
    pub timeout: Option<u32>,
    pub title: Option<String>,
    pub width: Option<u16>,
    pub height: Option<u16>,
    pub multi: bool,
    pub filter: bool,
    pub fix: bool,
    /// Internal: test whether wbat64 can create this file, then exit.
    pub probe_write: Option<String>,
}

/// One `box`/`menu` invocation.
#[derive(Clone, Debug, Default)]
pub struct BoxCommand {
    pub fix: bool,
    pub source: Option<SourceRef>,
    pub pos: Option<Pos>,
    pub colors: Option<Style>,
    pub title: Option<String>,
    pub text: Option<String>,
    pub items: Vec<String>,
    pub bar: Option<u32>,
    pub timeout: Option<u32>,
}

/// One `list` invocation.
#[derive(Clone, Debug, Default)]
pub struct ListCommand {
    pub fix: bool,
    pub source: Option<SourceRef>,
    pub path: Option<String>,
    pub stdin: bool,
    pub pos: Option<Pos>,
    pub colors: Option<Style>,
    pub title: Option<String>,
    pub width: Option<u16>,
    pub height: Option<u16>,
    pub bar: Option<u32>,
    pub multi: bool,
    pub filter: bool,
}

/// One `text` invocation.
#[derive(Clone, Debug, Default)]
pub struct TextCommand {
    pub fix: bool,
    pub source: Option<SourceRef>,
    pub pos: Option<Pos>,
    pub colors: Option<Style>,
    pub title: Option<String>,
    pub text: Option<String>,
}

/// One `fill` invocation.
#[derive(Clone, Debug, Default)]
pub struct FillCommand {
    pub pos: Option<Pos>,
    pub size: Option<(u16, u16)>,
    pub colors: Option<Style>,
    pub pattern: Option<String>,
    pub shift: i32,
}

/// A parsed command.
#[derive(Clone, Debug)]
pub enum Command {
    Box(BoxCommand),
    Menu(BoxCommand),
    List(ListCommand),
    Text(TextCommand),
    Fill(FillCommand),
    Cls { colors: Option<Style> },
    Cursor { line: Option<u16>, feeds: u16 },
    Save { id: Option<String> },
    Restore { id: Option<String> },
    Stdout { source: SourceRef },
    El { code: u8 },
    AnsiCheck,
    Pal { path: Option<String> },
    Font { size: Option<u32> },
    Check { source: Option<SourceRef> },
    Version,
    Help,
}

impl Command {
    pub fn name(&self) -> &'static str {
        match self {
            Command::Box(_) => "box",
            Command::Menu(_) => "menu",
            Command::List(_) => "list",
            Command::Text(_) => "text",
            Command::Fill(_) => "fill",
            Command::Cls { .. } => "cls",
            Command::Cursor { .. } => "cursor",
            Command::Save { .. } => "save",
            Command::Restore { .. } => "restore",
            Command::Stdout { .. } => "stdout",
            Command::El { .. } => "el",
            Command::AnsiCheck => "ansi?",
            Command::Pal { .. } => "pal",
            Command::Font { .. } => "font",
            Command::Check { .. } => "check",
            Command::Version => "version",
            Command::Help => "help",
        }
    }

    /// Box, menu and list are interactive; at most one may appear, and last.
    pub fn is_interactive(&self) -> bool {
        matches!(self, Command::Box(_) | Command::Menu(_) | Command::List(_))
    }
}

/// A fully parsed invocation.
#[derive(Clone, Debug)]
pub struct Invocation {
    pub commands: Vec<Command>,
    pub options: GlobalOptions,
}

fn parse_u16(value: &str, what: &str) -> Result<u16> {
    value
        .trim()
        .parse::<u16>()
        .map_err(|_| Error::msg(format!("invalid {what} '{value}'")))
}

fn expand_env(text: &str) -> String {
    wbat64_core::interp::interpolate(text, &|name| std::env::var(name).ok())
}

/// `#bar[,timeout]`
fn parse_bar_token(text: &str) -> Result<(Option<u32>, Option<u32>)> {
    let body = text.trim_start_matches('#');
    let (bar_part, timeout_part) = match body.split_once(',') {
        Some((bar, timeout)) => (bar, Some(timeout)),
        None => (body, None),
    };
    let bar = if bar_part.trim().is_empty() {
        None
    } else {
        let expanded = expand_env(bar_part);
        Some(
            wbat64_core::interp::eval_bar(&expanded)
                .ok_or_else(|| Error::msg(format!("invalid bar selection '#{bar_part}'")))?,
        )
    };
    let timeout = match timeout_part {
        Some(t) if t.trim().is_empty() => None,
        Some(t) => Some(parse_u16(t, "timeout")?.min(3600) as u32),
        None => None,
    };
    Ok((bar, timeout))
}

fn is_paren(text: &str) -> bool {
    text.trim_start().starts_with('(')
}

/// Collect a parenthesised group that was split by the shell (`(blue on +brown)`).
fn collect_paren(tokens: &[Token], index: usize) -> (String, usize) {
    let mut group = tokens[index].text.trim().to_string();
    let mut i = index;
    while !group.trim_end().ends_with(')') && i + 1 < tokens.len() {
        i += 1;
        group.push(' ');
        group.push_str(tokens[i].text.trim());
    }
    (group, i)
}

fn looks_like_shift(text: &str) -> Option<i32> {
    let t = text.trim();
    if t.len() >= 2
        && matches!(t.as_bytes()[0], b'+' | b'-')
        && t[1..].chars().all(|c| c.is_ascii_digit())
    {
        return t.parse::<i32>().ok();
    }
    None
}

fn source_token(text: &str) -> Option<SourceRef> {
    let t = text.trim();
    if t.starts_with('@') || t.starts_with(':') {
        return SourceRef::parse(t);
    }
    if let Some(rest) = t.strip_prefix('+') {
        if !rest
            .chars()
            .next()
            .map(|c| c.is_ascii_digit())
            .unwrap_or(false)
        {
            return SourceRef::parse(t);
        }
    }
    None
}

fn split_items(tokens: &[String]) -> Vec<String> {
    let joined = tokens.join(" ");
    joined
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// Extract `--options` from anywhere on the command line.
pub fn extract_options(tokens: Vec<Token>) -> Result<(Vec<Token>, GlobalOptions)> {
    let mut positional = Vec::new();
    let mut options = GlobalOptions::default();
    let mut i = 0usize;
    while i < tokens.len() {
        let token = tokens[i].clone();
        if token.quoted || !token.text.starts_with("--") || token.text.len() <= 2 {
            positional.push(token);
            i += 1;
            continue;
        }
        let raw = token.text.clone();
        let (name, inline) = match raw.split_once('=') {
            Some((name, value)) => (name.to_string(), Some(value.to_string())),
            None => (raw.clone(), None),
        };
        let lower = name.to_ascii_lowercase();
        if lower == "--and" {
            positional.push(token);
            i += 1;
            continue;
        }

        let take_value = |i: &mut usize| -> Result<String> {
            if let Some(value) = &inline {
                return Ok(value.clone());
            }
            *i += 1;
            tokens
                .get(*i)
                .map(|t| t.text.clone())
                .ok_or_else(|| Error::msg(format!("option '{name}' needs a value")))
        };

        match lower.as_str() {
            "--help" | "-h" | "/?" => options.help = true,
            "--version" | "-v" => options.version = true,
            "--emit" => {
                let value = take_value(&mut i)?;
                options.emit = Some(
                    EmitMode::parse(&value)
                        .ok_or_else(|| Error::msg(format!("unknown --emit mode '{value}'")))?,
                );
            }
            "--emit-file" => options.emit_file = Some(take_value(&mut i)?),
            "--prefix" => options.prefix = take_value(&mut i)?,
            "--encoding" => {
                let value = take_value(&mut i)?;
                options.encoding = Some(
                    Encoding::parse(&value)
                        .ok_or_else(|| Error::msg(format!("unknown encoding '{value}'")))?,
                );
            }
            "--set" => {
                let value = take_value(&mut i)?;
                let (name, val) = value.split_once('=').ok_or_else(|| {
                    Error::msg(format!("--set expects NAME=VALUE, got '{value}'"))
                })?;
                options.sets.push((name.to_string(), val.to_string()));
            }
            "--keys" => options.keys = Some(take_value(&mut i)?),
            "--dump-screen" => options.dump_screen = true,
            "--delayed" => options.delayed = true,
            "--no-mouse" => options.no_mouse = true,
            "--no-color" | "--mono" => options.no_color = true,
            "--truncate" => options.truncate = true,
            "--compat" => options.compat = Some(true),
            "--native" => options.compat = Some(false),
            "--config" => options.config = Some(take_value(&mut i)?),
            "--plain" => options.plain = true,
            "--size" => {
                let value = take_value(&mut i)?;
                let (w, h) = value.split_once(['x', 'X']).ok_or_else(|| {
                    Error::msg(format!("--size expects WIDTHxHEIGHT, got '{value}'"))
                })?;
                options.headless_size = Some((parse_u16(w, "width")?, parse_u16(h, "height")?));
            }
            "--pos" => {
                let value = take_value(&mut i)?;
                options.pos = Some(
                    parse_pos(&value)
                        .ok_or_else(|| Error::msg(format!("invalid position '{value}'")))?,
                );
            }
            "--colors" | "--color" => {
                let value = take_value(&mut i)?;
                options.colors = Some(parse_colors(&value)?);
            }
            "--bar" => {
                let value = take_value(&mut i)?;
                options.bar = Some(
                    wbat64_core::interp::eval_bar(&expand_env(&value))
                        .ok_or_else(|| Error::msg(format!("invalid --bar '{value}'")))?,
                );
            }
            "--timeout" => {
                let value = take_value(&mut i)?;
                options.timeout = Some(parse_u16(&value, "timeout")?.min(3600) as u32);
            }
            "--title" => options.title = Some(take_value(&mut i)?),
            "--width" => options.width = Some(parse_u16(&take_value(&mut i)?, "width")?),
            "--height" => options.height = Some(parse_u16(&take_value(&mut i)?, "height")?),
            "--multi" => options.multi = true,
            "--filter" => options.filter = true,
            "--fix" => options.fix = true,
            "--probe-write" => options.probe_write = Some(take_value(&mut i)?),
            _ => return Err(Error::msg(format!("unknown option '{name}'"))),
        }
        i += 1;
    }
    Ok((positional, options))
}

fn parse_box(tokens: &[Token], fix: bool, options: &GlobalOptions, menu: bool) -> Result<Command> {
    let mut command = BoxCommand {
        fix: fix || options.fix,
        pos: options.pos,
        colors: options.colors,
        title: options.title.clone(),
        bar: options.bar,
        timeout: options.timeout,
        ..BoxCommand::default()
    };
    let mut item_tokens: Vec<String> = Vec::new();
    let mut i = 0usize;
    while i < tokens.len() {
        let token = &tokens[i];
        let text = token.text.trim();
        if text.is_empty() {
            i += 1;
            continue;
        }
        if is_paren(text) {
            let (group, consumed) = collect_paren(tokens, i);
            i = consumed;
            if let Some((h, w)) = parse_size(&group) {
                let _ = (h, w);
                // Quick boxes do not take a size; ignore it gracefully.
            } else {
                command.colors = Some(parse_colors(&group)?);
            }
        } else if text.starts_with('#') {
            let (bar, timeout) = parse_bar_token(text)?;
            if bar.is_some() {
                command.bar = bar;
            }
            if timeout.is_some() {
                command.timeout = timeout;
            }
        } else if let Some(source) = source_token(text) {
            command.source = Some(source);
        } else if command.pos.is_none()
            && !token.quoted
            && wbat64_core::geometry::looks_like_pos(text)
        {
            command.pos = parse_pos(text);
        } else if token.quoted {
            match &mut command.text {
                Some(existing) => {
                    existing.push(' ');
                    existing.push_str(text);
                }
                None => command.text = Some(text.to_string()),
            }
        } else {
            item_tokens.push(text.to_string());
        }
        i += 1;
    }
    if !item_tokens.is_empty() {
        command.items = split_items(&item_tokens);
    }
    if menu {
        Ok(Command::Menu(command))
    } else {
        Ok(Command::Box(command))
    }
}

fn parse_list(tokens: &[Token], fix: bool, options: &GlobalOptions) -> Result<Command> {
    let mut command = ListCommand {
        fix: fix || options.fix,
        pos: options.pos,
        colors: options.colors,
        title: options.title.clone(),
        width: options.width,
        height: options.height,
        bar: options.bar,
        multi: options.multi,
        filter: options.filter,
        ..ListCommand::default()
    };
    let mut i = 0usize;
    while i < tokens.len() {
        let token = &tokens[i];
        let text = token.text.trim();
        if text.is_empty() {
            i += 1;
            continue;
        }
        if is_paren(text) {
            let (group, consumed) = collect_paren(tokens, i);
            i = consumed;
            if !parse_size(&group).is_some() {
                command.colors = Some(parse_colors(&group)?);
            }
        } else if text.starts_with('#') {
            let (bar, _) = parse_bar_token(text)?;
            if bar.is_some() {
                command.bar = bar;
            }
        } else if text == "-" && !token.quoted {
            command.stdin = true;
        } else if let Some(source) = source_token(text) {
            command.source = Some(source);
        } else if command.pos.is_none()
            && !token.quoted
            && wbat64_core::geometry::looks_like_pos(text)
        {
            command.pos = parse_pos(text);
        } else if command.path.is_none() {
            command.path = Some(text.to_string());
        }
        i += 1;
    }
    if command.source.is_none() && command.path.is_none() && !command.stdin {
        return Err(Error::msg("list needs a file, '-', or @file:label"));
    }
    Ok(Command::List(command))
}

fn parse_text(tokens: &[Token], fix: bool, options: &GlobalOptions) -> Result<Command> {
    let mut command = TextCommand {
        fix: fix || options.fix,
        pos: options.pos,
        colors: options.colors,
        title: options.title.clone(),
        ..TextCommand::default()
    };
    let mut i = 0usize;
    while i < tokens.len() {
        let token = &tokens[i];
        let text = token.text.trim();
        if text.is_empty() {
            i += 1;
            continue;
        }
        if is_paren(text) {
            let (group, consumed) = collect_paren(tokens, i);
            i = consumed;
            command.colors = Some(parse_colors(&group)?);
        } else if let Some(source) = source_token(text) {
            command.source = Some(source);
        } else if command.pos.is_none()
            && !token.quoted
            && wbat64_core::geometry::looks_like_pos(text)
        {
            command.pos = parse_pos(text);
        } else {
            match &mut command.text {
                Some(existing) => {
                    existing.push(' ');
                    existing.push_str(text);
                }
                None => command.text = Some(text.to_string()),
            }
        }
        i += 1;
    }
    if command.source.is_none() && command.text.is_none() {
        return Err(Error::msg("text needs a layout reference or a string"));
    }
    Ok(Command::Text(command))
}

fn parse_fill(tokens: &[Token], options: &GlobalOptions) -> Result<Command> {
    let mut command = FillCommand {
        pos: options.pos,
        colors: options.colors,
        ..FillCommand::default()
    };
    let mut i = 0usize;
    while i < tokens.len() {
        let token = &tokens[i];
        let text = token.text.trim();
        if text.is_empty() {
            i += 1;
            continue;
        }
        if is_paren(text) {
            let (group, consumed) = collect_paren(tokens, i);
            i = consumed;
            if let Some((h, w)) = parse_size(&group) {
                command.size = Some((h, w));
            } else {
                command.colors = Some(parse_colors(&group)?);
            }
        } else if let Some(shift) = looks_like_shift(text) {
            command.shift = shift;
        } else if command.pos.is_none()
            && !token.quoted
            && wbat64_core::geometry::looks_like_pos(text)
        {
            command.pos = parse_pos(text);
        } else if command.pattern.is_none() {
            command.pattern = Some(token.text.clone());
        }
        i += 1;
    }
    Ok(Command::Fill(command))
}

fn parse_cls(tokens: &[Token], options: &GlobalOptions) -> Result<Command> {
    let mut colors = options.colors;
    let mut i = 0usize;
    while i < tokens.len() {
        let text = tokens[i].text.trim();
        if is_paren(text) {
            let (group, consumed) = collect_paren(tokens, i);
            i = consumed;
            colors = Some(parse_colors(&group)?);
        }
        i += 1;
    }
    Ok(Command::Cls { colors })
}

fn parse_cursor(tokens: &[Token]) -> Result<Command> {
    let mut line = None;
    let mut feeds = 0u16;
    for token in tokens {
        let text = token.text.trim();
        if text.is_empty() {
            continue;
        }
        if let Some((l, f)) = text.split_once('+') {
            if !l.trim().is_empty() {
                line = Some(parse_u16(l, "cursor line")?);
            }
            if !f.trim().is_empty() {
                feeds = parse_u16(f, "cursor feeds")?;
            }
        } else if let Some(stripped) = text.strip_prefix('+') {
            feeds = parse_u16(stripped, "cursor feeds")?;
        } else {
            line = Some(parse_u16(text, "cursor line")?);
        }
    }
    Ok(Command::Cursor { line, feeds })
}

fn parse_check(tokens: &[Token]) -> Result<Command> {
    let mut source = None;
    for token in tokens {
        if let Some(reference) = source_token(&token.text) {
            source = Some(reference);
        }
    }
    Ok(Command::Check { source })
}

fn is_command_keyword(word: &str) -> bool {
    matches!(
        word.to_ascii_lowercase().as_str(),
        "box"
            | "menu"
            | "list"
            | "text"
            | "fill"
            | "cls"
            | "cursor"
            | "save"
            | "restore"
            | "stdout"
            | "el"
            | "ansi?"
            | "ansi"
            | "pal"
            | "font"
            | "check"
            | "version"
            | "help"
            | "fix"
    )
}

fn option_takes_value(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "--emit"
            | "--emit-file"
            | "--prefix"
            | "--encoding"
            | "--set"
            | "--keys"
            | "--size"
            | "--pos"
            | "--colors"
            | "--color"
            | "--bar"
            | "--timeout"
            | "--title"
            | "--width"
            | "--height"
            | "--config"
    )
}

/// `call w.bat "menu @dialogs.txt:main_menu #%mpos%+1"` arrives as one quoted
/// token, possibly after w.bat's own `--emit-file PATH`. When a quoted token at
/// the command position starts with a command keyword, expand it so both forms
/// behave identically.
pub fn expand_command_strings(tokens: Vec<Token>) -> Vec<Token> {
    let mut out: Vec<Token> = Vec::with_capacity(tokens.len());
    let mut seen_command = false;
    let mut expect_value = false;

    for token in tokens {
        if !token.quoted {
            if token.text == "!" || token.text.eq_ignore_ascii_case("--and") {
                seen_command = false;
                expect_value = false;
            } else if expect_value {
                expect_value = false;
            } else if token.text.starts_with("--") {
                if !token.text.contains('=') && option_takes_value(&token.text) {
                    expect_value = true;
                }
            } else if !seen_command {
                seen_command = true;
            }
            out.push(token);
            continue;
        }

        if expect_value {
            expect_value = false;
            out.push(token);
            continue;
        }
        if !seen_command {
            let word = token.text.split_whitespace().next().unwrap_or("");
            if is_command_keyword(word) {
                let expanded = crate::argv::tokenize(&token.text, false);
                if !expanded.is_empty() {
                    seen_command = true;
                    out.extend(expanded);
                    continue;
                }
            }
        }
        out.push(token);
    }
    out
}

fn parse_command(tokens: &[Token], options: &GlobalOptions) -> Result<Command> {
    let mut index = 0usize;
    let mut fix = false;
    if tokens
        .first()
        .map(|t| t.text.eq_ignore_ascii_case("fix"))
        .unwrap_or(false)
    {
        fix = true;
        index = 1;
    }
    let keyword = tokens
        .get(index)
        .map(|t| t.text.to_ascii_lowercase())
        .ok_or_else(|| Error::msg("empty command"))?;
    index += 1;
    let rest = &tokens[index..];
    match keyword.as_str() {
        "box" => parse_box(rest, fix, options, false),
        "menu" => parse_box(rest, fix, options, true),
        "list" => parse_list(rest, fix, options),
        "text" => parse_text(rest, fix, options),
        "fill" => parse_fill(rest, options),
        "cls" => parse_cls(rest, options),
        "cursor" => parse_cursor(rest),
        "save" => Ok(Command::Save {
            id: rest.first().map(|t| t.text.clone()),
        }),
        "restore" => Ok(Command::Restore {
            id: rest.first().map(|t| t.text.clone()),
        }),
        "stdout" => {
            let source = rest
                .iter()
                .find_map(|t| source_token(&t.text))
                .ok_or_else(|| Error::msg("stdout needs @file:label"))?;
            Ok(Command::Stdout { source })
        }
        "el" => {
            let value = rest
                .first()
                .ok_or_else(|| Error::msg("el needs an exit code"))?;
            let code = value
                .text
                .trim()
                .parse::<u16>()
                .map_err(|_| Error::msg(format!("invalid exit code '{}'", value.text)))?;
            if code > 255 {
                return Err(Error::msg(format!("exit code {code} is out of range")));
            }
            Ok(Command::El { code: code as u8 })
        }
        "ansi?" | "ansi" => Ok(Command::AnsiCheck),
        "pal" => Ok(Command::Pal {
            path: rest.first().map(|t| t.text.clone()),
        }),
        "font" => Ok(Command::Font {
            size: rest.first().and_then(|t| t.text.trim().parse::<u32>().ok()),
        }),
        "check" => parse_check(rest),
        "version" => Ok(Command::Version),
        "help" | "?" => Ok(Command::Help),
        other => Err(Error::msg(format!("unknown command '{other}'"))),
    }
}

/// Split the token stream into command segments and parse each one.
pub fn parse(tokens: &[Token]) -> Result<Invocation> {
    let expanded = expand_command_strings(tokens.to_vec());
    let mut segments: Vec<Command> = Vec::new();
    for segment in split_commands(&expanded) {
        segments.push(parse_command(&segment, &GlobalOptions::default())?);
    }
    if segments.is_empty() {
        return Err(Error::msg("no command given"));
    }
    Ok(Invocation {
        commands: segments,
        options: GlobalOptions::default(),
    })
}

/// Validate the command sequence: exactly one interactive command, and last.
pub fn validate(commands: &[Command]) -> Result<()> {
    let interactive: Vec<usize> = commands
        .iter()
        .enumerate()
        .filter(|(_, c)| c.is_interactive())
        .map(|(i, _)| i)
        .collect();
    if interactive.len() > 1 {
        return Err(Error::msg(
            "only one box, menu or list may appear in a chain",
        ));
    }
    if let Some(&index) = interactive.first() {
        if index + 1 != commands.len() {
            return Err(Error::msg(
                "a box, menu or list must be the last command in a chain",
            ));
        }
    }
    Ok(())
}

/// Convenience: parse options and commands together.
pub fn parse_all(tokens: Vec<Token>) -> Result<Invocation> {
    let tokens = expand_command_strings(tokens);
    let (positional, options) = extract_options(tokens)?;
    let mut commands = Vec::new();
    for segment in split_commands(&positional) {
        commands.push(parse_command(&segment, &options)?);
    }
    if commands.is_empty() {
        if options.help {
            return Ok(Invocation {
                commands: vec![Command::Help],
                options,
            });
        }
        if options.version {
            return Ok(Invocation {
                commands: vec![Command::Version],
                options,
            });
        }
        if options.probe_write.is_some() {
            return Ok(Invocation {
                commands: Vec::new(),
                options,
            });
        }
        return Err(Error::msg("no command given"));
    }
    validate(&commands)?;
    Ok(Invocation { commands, options })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::argv::tokenize;
    use wbat64_core::Color;

    fn parse_line(line: &str) -> Invocation {
        let tokens = tokenize(&format!("wbat64 {line}"), true);
        parse_all(tokens).unwrap()
    }

    #[test]
    fn quick_box() {
        let inv = parse_line(r#"box 19,55 (blue on +brown) "Continue?^Details" &yes,&no,cancel"#);
        match &inv.commands[0] {
            Command::Box(b) => {
                assert_eq!(b.pos, Some(Pos::new(19, 55)));
                assert_eq!(b.colors.unwrap().fg, Color::Indexed(1));
                assert_eq!(b.colors.unwrap().bg, Color::Indexed(14));
                assert_eq!(b.text.as_deref(), Some("Continue?^Details"));
                assert_eq!(b.items, vec!["&yes", "&no", "cancel"]);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn layout_reference_with_bar_and_timeout() {
        let inv = parse_line("box @dialogs.txt:signup #3,15");
        match &inv.commands[0] {
            Command::Box(b) => {
                let source = b.source.as_ref().unwrap();
                assert_eq!(source.path, "dialogs.txt");
                assert_eq!(source.label.as_deref(), Some("signup"));
                assert_eq!(b.bar, Some(3));
                assert_eq!(b.timeout, Some(15));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn bar_arithmetic() {
        let inv = parse_line("menu @m.txt:main #0+1");
        match &inv.commands[0] {
            Command::Menu(b) => assert_eq!(b.bar, Some(1)),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn chain_and_options() {
        let inv = parse_line(r#"cls --and text 2,4 "hi" --and box ok --emit json"#);
        assert_eq!(inv.commands.len(), 3);
        assert_eq!(inv.options.emit, Some(EmitMode::Json));
        assert!(matches!(inv.commands[1], Command::Text(_)));
    }

    #[test]
    fn fill_parsing() {
        let inv = parse_line(r##"fill 13,32 (9,28) (+magenta on blue) "└┐┌┘" +2"##);
        match &inv.commands[0] {
            Command::Fill(f) => {
                assert_eq!(f.pos, Some(Pos::new(13, 32)));
                assert_eq!(f.size, Some((9, 28)));
                assert_eq!(f.pattern.as_deref(), Some("└┐┌┘"));
                assert_eq!(f.shift, 2);
                assert_eq!(f.colors.unwrap().bg, Color::Indexed(1));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn cursor_syntax() {
        let inv = parse_line("cursor 25+5");
        assert!(matches!(
            inv.commands[0],
            Command::Cursor {
                line: Some(25),
                feeds: 5
            }
        ));
    }

    #[test]
    fn list_sources() {
        let inv = parse_line("list 3,60 files.txt");
        match &inv.commands[0] {
            Command::List(l) => {
                assert_eq!(l.path.as_deref(), Some("files.txt"));
                assert!(!l.stdin);
            }
            other => panic!("{other:?}"),
        }
        let inv = parse_line("list -");
        assert!(matches!(&inv.commands[0], Command::List(l) if l.stdin));
    }

    #[test]
    fn quoted_parameter_string_behaves_identically() {
        let plain = parse_line("menu @dialogs.txt:main_menu #3");
        let quoted = parse_line(r#""menu @dialogs.txt:main_menu #3""#);
        match (&plain.commands[0], &quoted.commands[0]) {
            (Command::Menu(a), Command::Menu(b)) => {
                assert_eq!(a.source, b.source);
                assert_eq!(a.bar, b.bar);
                assert_eq!(b.bar, Some(3));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn quoted_parameter_string_after_shim_options() {
        // w.bat expands to: wbat64 --emit-file out.bat "menu @f:main #3"
        let tokens = vec![
            Token::plain("--emit-file"),
            Token::new("out.bat", true),
            Token::new("menu @dialogs.txt:main_menu #3", true),
        ];
        let inv = parse_all(tokens).unwrap();
        assert_eq!(inv.options.emit_file.as_deref(), Some("out.bat"));
        match &inv.commands[0] {
            Command::Menu(b) => assert_eq!(b.bar, Some(3)),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn quoted_prompt_is_not_treated_as_a_command() {
        let inv = parse_line(r#"box "boxing is fun" ok"#);
        match &inv.commands[0] {
            Command::Box(b) => assert_eq!(b.text.as_deref(), Some("boxing is fun")),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn interactive_must_be_last() {
        let tokens = tokenize("wbat64 box ok ! cls", true);
        let err = parse_all(tokens).unwrap_err();
        assert!(err.to_string().contains("last"));
    }

    #[test]
    fn two_interactive_rejected() {
        let tokens = tokenize("wbat64 box ok ! list -", true);
        assert!(parse_all(tokens).is_err());
    }

    #[test]
    fn unknown_option_and_command() {
        assert!(parse_all(tokenize("wbat64 box --wat", true)).is_err());
        assert!(parse_all(tokenize("wbat64 frobnicate", true)).is_err());
    }

    #[test]
    fn sets_and_prefix() {
        let inv = parse_line("box ok --set name=Ada --prefix W_ --delayed");
        assert_eq!(
            inv.options.sets,
            vec![("name".to_string(), "Ada".to_string())]
        );
        assert_eq!(inv.options.prefix, "W_");
        assert!(inv.options.delayed);
    }
}
