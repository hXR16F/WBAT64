//! Configuration: WBAT-compatible INI, native TOML, and environment overrides.
//!
//! Precedence (lowest to highest): built-in defaults, `WBAT.INI`, `wbat64.toml`,
//! per-key environment overrides (`W` + key), command line. Building a config
//! is pure: the CLI reads the files and hands the text in.

use crate::color::{parse_colors, Color, Style};
use crate::encoding::Encoding;
use crate::ini::Ini;
use crate::interp::parse_bool;
use crate::toml_lite::{Toml, Value};
use crate::Result;

/// A 3x3 box frame pattern. The middle character is unused (it must be blank).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Frame {
    pub top_left: char,
    pub top: char,
    pub top_right: char,
    pub left: char,
    pub middle: char,
    pub right: char,
    pub bottom_left: char,
    pub bottom: char,
    pub bottom_right: char,
}

impl Default for Frame {
    fn default() -> Self {
        Frame::blank()
    }
}

impl Frame {
    /// WBAT's shipped default: a blank 3x3 pattern. It still reserves one
    /// cell on each side, which is the visible one-character margin.
    pub const fn blank() -> Frame {
        Frame {
            top_left: ' ',
            top: ' ',
            top_right: ' ',
            left: ' ',
            middle: ' ',
            right: ' ',
            bottom_left: ' ',
            bottom: ' ',
            bottom_right: ' ',
        }
    }

    pub const fn single() -> Frame {
        Frame {
            top_left: '┌',
            top: '─',
            top_right: '┐',
            left: '│',
            middle: ' ',
            right: '│',
            bottom_left: '└',
            bottom: '─',
            bottom_right: '┘',
        }
    }

    pub const fn double() -> Frame {
        Frame {
            top_left: '╔',
            top: '═',
            top_right: '╗',
            left: '║',
            middle: ' ',
            right: '║',
            bottom_left: '╚',
            bottom: '═',
            bottom_right: '╝',
        }
    }

    pub const fn rounded() -> Frame {
        Frame {
            top_left: '╭',
            top: '─',
            top_right: '╮',
            left: '│',
            middle: ' ',
            right: '│',
            bottom_left: '╰',
            bottom: '─',
            bottom_right: '╯',
        }
    }

    pub const fn ascii() -> Frame {
        Frame {
            top_left: '+',
            top: '-',
            top_right: '+',
            left: '|',
            middle: ' ',
            right: '|',
            bottom_left: '+',
            bottom: '-',
            bottom_right: '+',
        }
    }

    /// Parse a 9-character pattern such as `"┌─┐│ │└─┘"`.
    pub fn parse(pattern: &str) -> Option<Frame> {
        let chars: Vec<char> = pattern.chars().collect();
        if chars.len() != 9 {
            return None;
        }
        Some(Frame {
            top_left: chars[0],
            top: chars[1],
            top_right: chars[2],
            left: chars[3],
            middle: chars[4],
            right: chars[5],
            bottom_left: chars[6],
            bottom: chars[7],
            bottom_right: chars[8],
        })
    }

    /// Resolve a named frame or a custom pattern.
    pub fn from_name(name: &str) -> Option<Frame> {
        match name.trim().to_ascii_lowercase().as_str() {
            "none" | "blank" => Some(Frame::blank()),
            "ascii" => Some(Frame::ascii()),
            "single" | "plain" => Some(Frame::single()),
            "double" => Some(Frame::double()),
            "rounded" => Some(Frame::rounded()),
            _ => Frame::parse(name),
        }
    }

    pub fn as_pattern(self) -> String {
        [
            self.top_left,
            self.top,
            self.top_right,
            self.left,
            self.middle,
            self.right,
            self.bottom_left,
            self.bottom,
            self.bottom_right,
        ]
        .iter()
        .collect()
    }
}

/// The color roles the renderer uses.
#[derive(Clone, Debug)]
pub struct Theme {
    pub screen: Style,
    pub boxed: Style,
    pub box_hi: Style,
    pub text_hi: Style,
    pub text_title: Style,
    pub bar: Style,
    pub button: Style,
    pub hotkey: Style,
    pub shadow: Option<Color>,
    pub divider: Style,
    pub title: Style,
    pub input: Style,
    pub text: Style,
    pub list: Style,
}

impl Default for Theme {
    fn default() -> Self {
        Theme::compat_defaults()
    }
}

fn must(expr: &str) -> Style {
    parse_colors(expr).expect("built-in color expression must parse")
}

impl Theme {
    pub fn compat_defaults() -> Theme {
        Theme {
            screen: must("black on +green"),
            boxed: must("black on +white"),
            box_hi: must("red on +white"),
            text_hi: must("red on +green"),
            text_title: must("+white on magenta"),
            bar: must("+white on +red"),
            button: must("black on white"),
            hotkey: Style::new(Color::Indexed(4), Color::Default),
            shadow: Some(Color::Indexed(8)),
            divider: Style::new(Color::Indexed(3), Color::Default),
            title: must("black on +blue"),
            input: must("black on +white"),
            text: must("black on +white"),
            list: must("black on +white"),
        }
    }

    /// Drop every color, for `--no-color` / `NO_COLOR`.
    pub fn monochrome(&mut self) {
        for style in [
            &mut self.screen,
            &mut self.boxed,
            &mut self.box_hi,
            &mut self.text_hi,
            &mut self.text_title,
            &mut self.bar,
            &mut self.button,
            &mut self.divider,
            &mut self.title,
            &mut self.input,
            &mut self.text,
            &mut self.list,
        ] {
            *style = Style::default();
        }
        self.hotkey = Style::default();
        self.shadow = None;
    }
}

/// Runtime configuration for one invocation.
#[derive(Clone, Debug)]
pub struct Config {
    pub compat: bool,

    // [general]
    pub el_var: bool,
    pub right_esc: bool,
    /// `ESCinput=1`: keep edits when the user cancels.
    pub keep_input_on_esc: bool,
    pub error_pause: bool,
    pub source_file: String,
    pub pal_file: String,
    pub list_hot_ticks: u64,
    pub encoding: Encoding,
    /// `None` means "terminal width" (native mode); 78 in compat mode.
    pub max_width: Option<usize>,
    pub mouse: bool,
    pub truncate: bool,
    pub check_symbol: char,
    pub radio_symbol: char,
    pub hi_symbol: char,
    pub delayed: bool,

    // [colors]
    pub theme: Theme,
    pub frame: Frame,
}

impl Default for Config {
    fn default() -> Self {
        Config::native_defaults()
    }
}

impl Config {
    pub fn compat_defaults() -> Config {
        Config {
            compat: true,
            el_var: true,
            right_esc: true,
            keep_input_on_esc: false,
            error_pause: true,
            source_file: "WBAT.TXT".to_string(),
            pal_file: "+PAL.INI".to_string(),
            list_hot_ticks: 10,
            encoding: Encoding::Oem,
            max_width: Some(78),
            mouse: true,
            truncate: false,
            check_symbol: 'x',
            radio_symbol: '*',
            hi_symbol: '~',
            delayed: false,
            theme: Theme::compat_defaults(),
            frame: Frame::blank(),
        }
    }

    pub fn native_defaults() -> Config {
        Config {
            compat: false,
            encoding: Encoding::Auto,
            max_width: None,
            source_file: "WBAT.TXT".to_string(),
            ..Config::compat_defaults()
        }
    }

    pub fn defaults_for(compat: bool) -> Config {
        if compat {
            Config::compat_defaults()
        } else {
            Config::native_defaults()
        }
    }

    /// Build a config from optional INI/TOML text plus an environment lookup.
    ///
    /// The lookup receives a bare key name (`"Box"`, `"rightESC"`) and returns
    /// the value of the `W`-prefixed environment variable, if set.
    pub fn build(
        compat: bool,
        ini_text: Option<&str>,
        toml_text: Option<&str>,
        env: &dyn Fn(&str) -> Option<String>,
    ) -> (Config, Vec<String>) {
        let mut config = Config::defaults_for(compat);
        let mut warnings = Vec::new();
        if let Some(text) = ini_text {
            config.apply_ini(text, &mut warnings);
        }
        if let Some(text) = toml_text {
            config.apply_toml(text, &mut warnings);
        }
        config.apply_env(env, &mut warnings);
        (config, warnings)
    }

    fn set_style(target: &mut Style, value: &str, what: &str, warnings: &mut Vec<String>) {
        match parse_colors(value) {
            Ok(style) => *target = style,
            Err(e) => warnings.push(format!("{what}: {e}")),
        }
    }

    /// Apply a WBAT.INI document.
    pub fn apply_ini(&mut self, text: &str, warnings: &mut Vec<String>) {
        let ini = Ini::parse(text);
        let general = |key: &str| ini.get("general", key).or_else(|| ini.get_any(key));
        let color = |key: &str| ini.get("color", key).or_else(|| ini.get_any(key));

        if let Some(v) = general("ELvar").and_then(parse_bool) {
            self.el_var = v;
        }
        if let Some(v) = general("rightESC").and_then(parse_bool) {
            self.right_esc = v;
        }
        if let Some(v) = general("ESCinput").and_then(parse_bool) {
            self.keep_input_on_esc = v;
        }
        if let Some(v) = general("ErrorPause").and_then(parse_bool) {
            self.error_pause = v;
        }
        if let Some(v) = general("BoxFrame") {
            match Frame::parse(v).or_else(|| Frame::from_name(v)) {
                Some(frame) => self.frame = frame,
                None => warnings.push(format!("BoxFrame must be 9 characters, got '{v}'")),
            }
        }
        if let Some(v) = general("CheckSymbol").and_then(parse_symbol) {
            self.check_symbol = v;
        }
        if let Some(v) = general("RadioSymbol").and_then(parse_symbol) {
            self.radio_symbol = v;
        }
        if let Some(v) = general("HiSymbol").and_then(parse_symbol) {
            self.hi_symbol = v;
        }
        if let Some(v) = general("SourceFile") {
            self.source_file = v.to_string();
        }
        if let Some(v) = general("PalFile") {
            self.pal_file = v.to_string();
        }
        if let Some(v) = general("ListHotTicks").and_then(|s| s.trim().parse().ok()) {
            self.list_hot_ticks = v;
        }

        if let Some(v) = color("Screen") {
            Self::set_style(&mut self.theme.screen, v, "Screen", warnings);
        }
        if let Some(v) = color("Box") {
            Self::set_style(&mut self.theme.boxed, v, "Box", warnings);
        }
        if let Some(v) = color("BoxHi") {
            Self::set_style(&mut self.theme.box_hi, v, "BoxHi", warnings);
        }
        if let Some(v) = color("TextHi") {
            Self::set_style(&mut self.theme.text_hi, v, "TextHi", warnings);
        }
        if let Some(v) = color("TextTitle") {
            Self::set_style(&mut self.theme.text_title, v, "TextTitle", warnings);
        }
        if let Some(v) = color("Bar") {
            Self::set_style(&mut self.theme.bar, v, "Bar", warnings);
        }
        if let Some(v) = color("Button") {
            Self::set_style(&mut self.theme.button, v, "Button", warnings);
        }
        if let Some(v) = color("HotKey") {
            if v.trim().eq_ignore_ascii_case("none") {
                // NONE disables hotkeys; draw them like the surrounding label.
                self.theme.hotkey = Style::default();
            } else {
                Self::set_style(&mut self.theme.hotkey, v, "HotKey", warnings);
            }
        }
        if let Some(v) = color("Shadow") {
            if v.trim().eq_ignore_ascii_case("none") {
                self.theme.shadow = None;
            } else {
                match parse_colors(v) {
                    Ok(style) => {
                        self.theme.shadow = Some(if !style.fg.is_default() {
                            style.fg
                        } else {
                            style.bg
                        })
                    }
                    Err(e) => warnings.push(format!("Shadow: {e}")),
                }
            }
        }
        if let Some(v) = color("Divider") {
            Self::set_style(&mut self.theme.divider, v, "Divider", warnings);
        }
        if let Some(v) = color("Title").or_else(|| color("BoxTitle")) {
            Self::set_style(&mut self.theme.title, v, "Title", warnings);
        }
        if let Some(v) = color("Input") {
            Self::set_style(&mut self.theme.input, v, "Input", warnings);
        }
        if let Some(v) = color("Text") {
            Self::set_style(&mut self.theme.text, v, "Text", warnings);
        }
        if let Some(v) = color("List") {
            Self::set_style(&mut self.theme.list, v, "List", warnings);
        }
    }

    /// Apply a native `wbat64.toml` document.
    pub fn apply_toml(&mut self, text: &str, warnings: &mut Vec<String>) {
        let toml = match Toml::parse(text) {
            Ok(t) => t,
            Err(e) => {
                warnings.push(format!("wbat64.toml: {e}"));
                return;
            }
        };
        if let Some(v) = toml.get_str("general", "frame") {
            match Frame::from_name(v) {
                Some(frame) => self.frame = frame,
                None => warnings.push(format!("frame: invalid pattern '{v}'")),
            }
        }
        if let Some(v) = toml.get_bool("general", "mouse") {
            self.mouse = v;
        }
        if let Some(v) = toml.get_bool("general", "escape_discards") {
            self.keep_input_on_esc = !v;
        }
        if let Some(v) = toml.get_bool("general", "el_var") {
            self.el_var = v;
        }
        if let Some(v) = toml.get_bool("general", "error_pause") {
            self.error_pause = v;
        }
        if let Some(v) = toml.get_str("general", "source_file") {
            self.source_file = v.to_string();
        }
        if let Some(v) = toml.get_str("general", "pal_file") {
            self.pal_file = v.to_string();
        }
        if let Some(v) = toml.get_str("general", "encoding") {
            if let Some(enc) = Encoding::parse(v) {
                self.encoding = enc;
            } else {
                warnings.push(format!("encoding: unknown value '{v}'"));
            }
        }
        if let Some(v) = toml.get("general", "max_width").and_then(Value::as_i64) {
            self.max_width = if v <= 0 { None } else { Some(v as usize) };
        }
        if let Some(v) = toml
            .get_str("general", "check_symbol")
            .and_then(parse_symbol)
        {
            self.check_symbol = v;
        }
        if let Some(v) = toml
            .get_str("general", "radio_symbol")
            .and_then(parse_symbol)
        {
            self.radio_symbol = v;
        }
        if let Some(v) = toml.get_str("general", "hi_symbol").and_then(parse_symbol) {
            self.hi_symbol = v;
        }
        if let Some(v) = toml.get_bool("general", "truncate") {
            self.truncate = v;
        }

        let colors = |key: &str| toml.get_str("colors", key).map(str::to_string);
        if let Some(v) = colors("screen") {
            Self::set_style(&mut self.theme.screen, &v, "colors.screen", warnings);
        }
        if let Some(v) = colors("box") {
            Self::set_style(&mut self.theme.boxed, &v, "colors.box", warnings);
        }
        if let Some(v) = colors("box_hi") {
            Self::set_style(&mut self.theme.box_hi, &v, "colors.box_hi", warnings);
        }
        if let Some(v) = colors("text_hi") {
            Self::set_style(&mut self.theme.text_hi, &v, "colors.text_hi", warnings);
        }
        if let Some(v) = colors("text_title") {
            Self::set_style(
                &mut self.theme.text_title,
                &v,
                "colors.text_title",
                warnings,
            );
        }
        if let Some(v) = colors("bar") {
            Self::set_style(&mut self.theme.bar, &v, "colors.bar", warnings);
        }
        if let Some(v) = colors("button") {
            Self::set_style(&mut self.theme.button, &v, "colors.button", warnings);
        }
        if let Some(v) = colors("hotkey") {
            if v.trim().eq_ignore_ascii_case("none") {
                self.theme.hotkey = Style::default();
            } else {
                Self::set_style(&mut self.theme.hotkey, &v, "colors.hotkey", warnings);
            }
        }
        if let Some(v) = colors("shadow") {
            if v.trim().eq_ignore_ascii_case("none") {
                self.theme.shadow = None;
            } else {
                match parse_colors(&v) {
                    Ok(style) => {
                        self.theme.shadow = Some(if !style.fg.is_default() {
                            style.fg
                        } else {
                            style.bg
                        })
                    }
                    Err(e) => warnings.push(format!("colors.shadow: {e}")),
                }
            }
        }
        if let Some(v) = colors("divider") {
            Self::set_style(&mut self.theme.divider, &v, "colors.divider", warnings);
        }
        if let Some(v) = colors("title") {
            Self::set_style(&mut self.theme.title, &v, "colors.title", warnings);
        }
        if let Some(v) = colors("input") {
            Self::set_style(&mut self.theme.input, &v, "colors.input", warnings);
        }
        if let Some(v) = colors("text") {
            Self::set_style(&mut self.theme.text, &v, "colors.text", warnings);
        }
        if let Some(v) = colors("list") {
            Self::set_style(&mut self.theme.list, &v, "colors.list", warnings);
        }
    }

    /// Apply `W`-prefixed environment overrides for every known key.
    pub fn apply_env(&mut self, get: &dyn Fn(&str) -> Option<String>, warnings: &mut Vec<String>) {
        let keys = [
            "ELvar",
            "rightESC",
            "ESCinput",
            "ErrorPause",
            "BoxFrame",
            "CheckSymbol",
            "RadioSymbol",
            "HiSymbol",
            "SourceFile",
            "PalFile",
            "ListHotTicks",
            "Screen",
            "Box",
            "BoxHi",
            "TextHi",
            "TextTitle",
            "Bar",
            "Button",
            "HotKey",
            "Shadow",
            "Divider",
            "Title",
            "Input",
            "Text",
            "List",
        ];
        for key in keys {
            if let Some(value) = get(key) {
                // A one-key INI fragment reuses the exact same parsing rules.
                let section = match key {
                    "Screen" | "Box" | "BoxHi" | "TextHi" | "TextTitle" | "Bar" | "Button"
                    | "HotKey" | "Shadow" | "Divider" | "Title" | "Input" | "Text" | "List" => {
                        "color"
                    }
                    _ => "general",
                };
                let fragment = format!("[{section}]\n{key}={value}\n");
                self.apply_ini(&fragment, warnings);
            }
        }
    }
}

/// Parse a symbol setting: either a single character or a decimal code point.
pub fn parse_symbol(value: &str) -> Option<char> {
    let v = value.trim().trim_matches('"').trim_matches('\'');
    if v.is_empty() {
        return None;
    }
    if v.chars().all(|c| c.is_ascii_digit()) {
        let code: u32 = v.parse().ok()?;
        return char::from_u32(code);
    }
    v.chars().next()
}

/// Configuration for one run.
#[derive(Clone, Debug, Default)]
pub struct LoadOptions {
    /// Compat mode: `wbat`-style defaults, INI only.
    pub compat: bool,
    /// Native mode also reads `WBAT.INI` after the TOML file (section 6.3).
    pub read_ini_in_native: bool,
}

/// Convenience wrapper that mirrors `Config::build` with owned inputs.
pub fn build_config(
    compat: bool,
    ini_text: Option<&str>,
    toml_text: Option<&str>,
    env: &dyn Fn(&str) -> Option<String>,
) -> Result<(Config, Vec<String>)> {
    Ok(Config::build(compat, ini_text, toml_text, env))
}

#[cfg(test)]
mod tests {
    use super::*;

    const INI: &str = r#"
[general]
ELvar=0
rightESC=0
ESCinput=1
ErrorPause=0
BoxFrame="┌─┐│ │└─┘"
CheckSymbol="X"
SourceFile=MY.TXT
[color]
Screen=black on +green
Box=black on +white
HotKey=+yellow
Shadow=NONE
"#;

    #[test]
    fn ini_applies() {
        let (c, warnings) = Config::build(true, Some(INI), None, &|_| None);
        assert!(warnings.is_empty());
        assert!(!c.el_var);
        assert!(!c.right_esc);
        assert!(c.keep_input_on_esc);
        assert!(!c.error_pause);
        assert_eq!(c.source_file, "MY.TXT");
        assert_eq!(c.check_symbol, 'X');
        assert_eq!(
            c.theme.hotkey,
            Style::new(Color::Indexed(14), Color::Default)
        );
        assert_eq!(c.theme.shadow, None);
        assert_eq!(c.frame, Frame::single());
    }

    #[test]
    fn toml_applies_and_overrides_ini() {
        let toml = r##"
[general]
frame = "rounded"
mouse = false
escape_discards = false
max_width = 0
[colors]
bar = "+white on #b00020"
"##;
        let (c, warnings) = Config::build(
            false,
            Some("[general]\nBoxFrame=\"+-+| |+-+\"\n"),
            Some(toml),
            &|_| None,
        );
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(c.frame, Frame::rounded());
        assert!(!c.mouse);
        assert!(c.keep_input_on_esc);
        assert_eq!(c.max_width, None);
        assert_eq!(c.theme.bar.bg, Color::Rgb(176, 0, 32));
    }

    #[test]
    fn env_overrides_win() {
        let env = |key: &str| match key {
            "Box" => Some("white on blue".to_string()),
            "rightESC" => Some("0".to_string()),
            _ => None,
        };
        let (c, _) = Config::build(true, None, None, &env);
        assert_eq!(
            c.theme.boxed,
            Style::new(Color::Indexed(7), Color::Indexed(1))
        );
        assert!(!c.right_esc);
    }

    #[test]
    fn bad_values_become_warnings() {
        let (c, warnings) = Config::build(
            true,
            Some("[color]\nBox=mauve\n[general]\nBoxFrame=short\n"),
            None,
            &|_| None,
        );
        assert_eq!(warnings.len(), 2);
        assert_eq!(c.frame, Frame::blank());
    }

    #[test]
    fn compat_defaults_match_wbat_ini() {
        let c = Config::compat_defaults();
        assert_eq!(c.source_file, "WBAT.TXT");
        assert_eq!(c.frame, Frame::blank());
        assert_eq!(
            c.theme.title,
            Style::new(Color::Indexed(0), Color::Indexed(9))
        );
        assert_eq!(
            c.theme.divider,
            Style::new(Color::Indexed(3), Color::Default)
        );
        assert_eq!(
            c.theme.text_hi,
            Style::new(Color::Indexed(4), Color::Indexed(10))
        );
        assert_eq!(
            c.theme.text_title,
            Style::new(Color::Indexed(15), Color::Indexed(5))
        );
        assert_eq!(
            c.theme.hotkey,
            Style::new(Color::Indexed(4), Color::Default)
        );
    }

    #[test]
    fn frame_names() {
        assert_eq!(Frame::from_name("blank"), Some(Frame::blank()));
        assert_eq!(Frame::from_name("none"), Some(Frame::blank()));
        assert_eq!(Frame::from_name("ascii"), Some(Frame::ascii()));
        assert_eq!(Frame::from_name("rounded").unwrap().top_left, '╭');
        assert_eq!(Frame::from_name("┌─┐│ │└─┘"), Some(Frame::single()));
        assert_eq!(Frame::from_name("nope"), None);
        assert_eq!(Frame::single().as_pattern(), "┌─┐│ │└─┘");
    }

    #[test]
    fn symbols() {
        assert_eq!(parse_symbol("x"), Some('x'));
        assert_eq!(parse_symbol("\"*\""), Some('*'));
        assert_eq!(parse_symbol("251"), Some('û'));
        assert_eq!(parse_symbol(""), None);
    }

    #[test]
    fn text_title_role_is_read() {
        let (config, warnings) = Config::build(
            true,
            Some("[color]\nTextTitle=+white on magenta\n"),
            None,
            &|_| None,
        );
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(config.theme.text_title.fg, Color::Indexed(15));
        assert_eq!(config.theme.text_title.bg, Color::Indexed(5));
    }
}
