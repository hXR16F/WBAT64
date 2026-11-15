//! Color expressions and roles.
//!
//! The expression syntax is WBAT's: `(foreground on background)`, where names
//! follow the DOS palette order (black, blue, green, cyan, red, magenta, brown,
//! white), `+`/`bright`/`light` select the bright half, and `yellow` is bright
//! brown. wbat64 adds `#rrggbb`, `rgb(r,g,b)` and bare `0..255` terminal indexes.
//!
//! Every color resolves to a [`Color`]; ANSI encoding lives here too so that
//! both the renderer and the TUI agree on one representation.

use std::fmt;

use crate::{Error, Result};

/// A terminal color.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum Color {
    /// The terminal's own default.
    #[default]
    Default,
    /// A palette index, 0..=255 (0..=15 are the DOS colors).
    Indexed(u8),
    /// 24-bit truecolor.
    Rgb(u8, u8, u8),
}

impl Color {
    pub const BLACK: Color = Color::Indexed(0);
    pub const BLUE: Color = Color::Indexed(1);
    pub const GREEN: Color = Color::Indexed(2);
    pub const CYAN: Color = Color::Indexed(3);
    pub const RED: Color = Color::Indexed(4);
    pub const MAGENTA: Color = Color::Indexed(5);
    pub const BROWN: Color = Color::Indexed(6);
    pub const WHITE: Color = Color::Indexed(7);
    pub const GREY: Color = Color::Indexed(8);

    pub fn is_default(self) -> bool {
        matches!(self, Color::Default)
    }

    /// Map a DOS palette index to the equivalent ANSI 16-color index.
    ///
    /// DOS order is black, blue, green, cyan, red, magenta, brown, white;
    /// ANSI order is black, red, green, yellow, blue, magenta, cyan, white.
    /// Indexes 16..=255 are terminal palette indexes and pass through.
    pub fn to_ansi16(self) -> Color {
        match self {
            Color::Indexed(i) if i < 16 => {
                const DOS_TO_ANSI: [u8; 8] = [0, 4, 2, 6, 1, 5, 3, 7];
                let base = DOS_TO_ANSI[(i % 8) as usize];
                Color::Indexed(base + (i / 8) * 8)
            }
            other => other,
        }
    }

    /// Foreground SGR parameters (without the leading `ESC[`).
    pub fn sgr_fg(self) -> String {
        match self.to_ansi16() {
            Color::Default => "39".to_string(),
            Color::Indexed(i) if i < 8 => format!("{}", 30 + i),
            Color::Indexed(i) if i < 16 => format!("{}", 90 + i - 8),
            Color::Indexed(i) => format!("38;5;{i}"),
            Color::Rgb(r, g, b) => format!("38;2;{r};{g};{b}"),
        }
    }

    /// Background SGR parameters (without the leading `ESC[`).
    pub fn sgr_bg(self) -> String {
        match self.to_ansi16() {
            Color::Default => "49".to_string(),
            Color::Indexed(i) if i < 8 => format!("{}", 40 + i),
            Color::Indexed(i) if i < 16 => format!("{}", 100 + i - 8),
            Color::Indexed(i) => format!("48;5;{i}"),
            Color::Rgb(r, g, b) => format!("48;2;{r};{g};{b}"),
        }
    }
}

impl fmt::Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Color::Default => f.write_str("default"),
            Color::Indexed(i) => write!(f, "{i}"),
            Color::Rgb(r, g, b) => write!(f, "#{r:02x}{g:02x}{b:02x}"),
        }
    }
}

/// A resolved foreground/background pair.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Style {
    pub fg: Color,
    pub bg: Color,
}

impl Style {
    pub const fn new(fg: Color, bg: Color) -> Self {
        Style { fg, bg }
    }

    pub const fn fg_only(fg: Color) -> Self {
        Style {
            fg,
            bg: Color::Default,
        }
    }

    pub fn is_default(self) -> bool {
        self.fg.is_default() && self.bg.is_default()
    }

    pub fn with_fg(mut self, fg: Color) -> Self {
        self.fg = fg;
        self
    }

    /// Swap foreground and background.
    pub fn reversed(self) -> Self {
        Style {
            fg: self.bg,
            bg: self.fg,
        }
    }

    /// SGR sequence that selects exactly these colors (plus a reset first, so
    /// no stale attribute leaks in).
    pub fn sgr(self) -> String {
        format!("0;{};{}m", self.fg.sgr_fg(), self.bg.sgr_bg())
    }
}

const NAMES: [(&str, u8); 8] = [
    ("black", 0),
    ("blue", 1),
    ("green", 2),
    ("cyan", 3),
    ("red", 4),
    ("magenta", 5),
    ("brown", 6),
    ("white", 7),
];

/// Strip one pair of surrounding parentheses, if balanced.
fn strip_parens(s: &str) -> &str {
    let t = s.trim();
    if t.starts_with('(') && t.ends_with(')') && t.len() >= 2 {
        &t[1..t.len() - 1]
    } else {
        t
    }
}

fn parse_hex_byte(s: &str) -> Option<u8> {
    u8::from_str_radix(s, 16).ok()
}

/// Parse one color term (already lower-cased, without `+`).
fn parse_term(t: &str, bright: bool) -> Result<Color> {
    let t = t.trim();
    if t.is_empty() {
        return Err(Error::msg("empty color term"));
    }
    if let Some(hex) = t.strip_prefix('#') {
        return match hex.len() {
            6 => {
                let r = parse_hex_byte(&hex[0..2]);
                let g = parse_hex_byte(&hex[2..4]);
                let b = parse_hex_byte(&hex[4..6]);
                match (r, g, b) {
                    (Some(r), Some(g), Some(b)) => Ok(Color::Rgb(r, g, b)),
                    _ => Err(Error::msg(format!("invalid rgb color '#{hex}'"))),
                }
            }
            3 => {
                let r = parse_hex_byte(&hex[0..1]);
                let g = parse_hex_byte(&hex[1..2]);
                let b = parse_hex_byte(&hex[2..3]);
                match (r, g, b) {
                    (Some(r), Some(g), Some(b)) => Ok(Color::Rgb(r * 17, g * 17, b * 17)),
                    _ => Err(Error::msg(format!("invalid rgb color '#{hex}'"))),
                }
            }
            _ => Err(Error::msg(format!("invalid rgb color '#{hex}'"))),
        };
    }
    if let Some(inner) = t
        .strip_prefix("rgb(")
        .and_then(|rest| rest.strip_suffix(')'))
    {
        let parts: Vec<&str> = inner.split(',').map(str::trim).collect();
        if parts.len() != 3 {
            return Err(Error::msg(format!("invalid rgb color '{t}'")));
        }
        let mut vals = [0u8; 3];
        for (i, p) in parts.iter().enumerate() {
            vals[i] = p
                .parse::<u8>()
                .map_err(|_| Error::msg(format!("invalid rgb component '{p}'")))?;
        }
        return Ok(Color::Rgb(vals[0], vals[1], vals[2]));
    }
    if t.chars().all(|c| c.is_ascii_digit()) {
        let v: u16 = t
            .parse()
            .map_err(|_| Error::msg(format!("invalid color index '{t}'")))?;
        if v > 255 {
            return Err(Error::msg(format!("color index out of range: {v}")));
        }
        return Ok(Color::Indexed(v as u8));
    }
    if t == "default" || t == "none" {
        return Ok(Color::Default);
    }
    if t == "yellow" {
        return Ok(Color::Indexed(6 + 8));
    }
    if t == "grey" || t == "gray" {
        return Ok(Color::Indexed(7 + if bright { 8 } else { 0 }));
    }
    for (name, idx) in NAMES {
        if t == name {
            return Ok(Color::Indexed(idx + if bright { 8 } else { 0 }));
        }
    }
    Err(Error::msg(format!("unknown color '{t}'")))
}

/// Parse `"(foreground on background)"` (or a bare `foreground`).
///
/// `on` is optional, and a leading `on` leaves the foreground at the terminal
/// default: `(on white)` means "default foreground on a white background".
pub fn parse_colors(s: &str) -> Result<Style> {
    let inner = strip_parens(s);
    let mut fg: Option<Color> = None;
    let mut bg: Option<Color> = None;
    let mut after_on = false;
    let mut bright = false;
    for raw in inner.split_whitespace() {
        let mut t = raw.to_ascii_lowercase();
        if t == "on" {
            if after_on {
                return Err(Error::msg(format!(
                    "duplicate 'on' in color expression '{s}'"
                )));
            }
            after_on = true;
            continue;
        }
        if t == "bright" || t == "light" {
            bright = true;
            continue;
        }
        if let Some(rest) = t.strip_prefix('+') {
            bright = true;
            t = rest.to_string();
            if t.is_empty() {
                continue;
            }
        }
        // "rgb(1, 2, 3)" tokenizes into pieces when written with spaces; the
        // caller normally passes it without spaces, which is what we accept.
        let color = parse_term(&t, bright)?;
        if after_on {
            if bg.is_some() {
                return Err(Error::msg(format!(
                    "too many background colors in expression '{s}'"
                )));
            }
            bg = Some(color);
        } else if fg.is_some() {
            // The word "on" is optional, so `black white` is also fg/bg.
            if bg.is_some() {
                return Err(Error::msg(format!("too many colors in expression '{s}'")));
            }
            bg = Some(color);
        } else {
            fg = Some(color);
        }
        bright = false;
    }
    if after_on && bg.is_none() {
        return Err(Error::msg(format!(
            "'on' needs a background color in expression '{s}'"
        )));
    }
    if fg.is_none() && bg.is_none() {
        return Err(Error::msg("empty color expression"));
    }
    Ok(Style::new(
        fg.unwrap_or(Color::Default),
        bg.unwrap_or(Color::Default),
    ))
}

/// Parse a single color with an explicit background, used for INI/TOML roles
/// that only carry a foreground (for example `HotKey= red`).
pub fn parse_foreground(s: &str) -> Result<Color> {
    Ok(parse_colors(s)?.fg)
}

/// The palette order used by `pal.ini` (DOS order).
pub const DOS_PALETTE_NAMES: [&str; 8] = [
    "black", "blue", "green", "cyan", "red", "magenta", "brown", "white",
];

/// Convert a 6-bit DOS palette component (0..=63) to 8-bit.
pub fn dos_component_to_8bit(v: u8) -> u8 {
    ((v as u16 * 255 + 31) / 63) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dos_basics() {
        assert_eq!(
            parse_colors("white").unwrap(),
            Style::new(Color::Indexed(7), Color::Default)
        );
        assert_eq!(
            parse_colors("+white on blue").unwrap(),
            Style::new(Color::Indexed(15), Color::Indexed(1))
        );
        assert_eq!(
            parse_colors("(+white on blue)").unwrap(),
            Style::new(Color::Indexed(15), Color::Indexed(1))
        );
        assert_eq!(
            parse_colors("black on +green").unwrap(),
            Style::new(Color::BLACK, Color::Indexed(10))
        );
    }

    #[test]
    fn brightness_spellings() {
        let want = Style::new(Color::Indexed(15), Color::Indexed(4));
        assert_eq!(parse_colors("bright white on red").unwrap(), want);
        assert_eq!(parse_colors("light white on red").unwrap(), want);
        assert_eq!(parse_colors("yellow").unwrap().fg, Color::Indexed(14));
        assert_eq!(parse_colors("grey").unwrap().fg, Color::Indexed(7));
        assert_eq!(parse_colors("gray on black").unwrap().fg, Color::Indexed(7));
    }

    #[test]
    fn extended_colors() {
        assert_eq!(
            parse_colors("#ffd700 on #202020").unwrap(),
            Style::new(Color::Rgb(255, 215, 0), Color::Rgb(32, 32, 32))
        );
        assert_eq!(parse_colors("rgb(1,2,3)").unwrap().fg, Color::Rgb(1, 2, 3));
        assert_eq!(
            parse_colors("196 on 17").unwrap(),
            Style::new(Color::Indexed(196), Color::Indexed(17))
        );
    }

    #[test]
    fn bright_prefix_inside_word() {
        assert_eq!(
            parse_colors("+red on black").unwrap().fg,
            Color::Indexed(12)
        );
    }

    #[test]
    fn leading_on_sets_only_the_background() {
        assert_eq!(
            parse_colors("on white").unwrap(),
            Style::new(Color::Default, Color::Indexed(7))
        );
        assert_eq!(
            parse_colors("(on +white)").unwrap(),
            Style::new(Color::Default, Color::Indexed(15))
        );
        assert_eq!(
            parse_colors("black white").unwrap(),
            Style::new(Color::Indexed(0), Color::Indexed(7))
        );
        assert!(parse_colors("on").is_err());
        assert!(parse_colors("on white blue").is_err());
    }

    #[test]
    fn errors() {
        assert!(parse_colors("mauve").is_err());
        assert!(parse_colors("#zzzzzz").is_err());
        assert!(parse_colors("red on blue on green on white").is_err());
        assert!(parse_colors("").is_err());
    }

    #[test]
    fn ansi_encoding() {
        // DOS red is index 4; ANSI red is index 1.
        assert_eq!(Color::Indexed(4).sgr_fg(), "31");
        assert_eq!(Color::Indexed(12).sgr_fg(), "91");
        assert_eq!(Color::Indexed(1).sgr_bg(), "44");
        assert_eq!(Color::Indexed(196).sgr_fg(), "38;5;196");
        assert_eq!(Color::Rgb(1, 2, 3).sgr_bg(), "48;2;1;2;3");
        assert_eq!(Color::Default.sgr_fg(), "39");
        assert_eq!(Color::Indexed(15).to_ansi16(), Color::Indexed(15));
    }
}
