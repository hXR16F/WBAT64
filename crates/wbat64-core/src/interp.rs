//! Variable interpolation and the WBAT result-word algorithm.
//!
//! Layout text (but never control text) expands `%NAME%` before it is drawn.
//! A run of underscores directly after the closing `%` reserves a fixed width:
//! `%VAR%________` pads with spaces or truncates to exactly that width.

use crate::text::{fit, str_width};

/// Anything that can resolve a variable name.
pub type Lookup<'a> = &'a dyn Fn(&str) -> Option<String>;

/// Expand `%NAME%` references in `text`.
///
/// Unknown names expand to the empty string, mirroring `cmd.exe`. `%%` is a
/// literal percent sign. An unterminated `%` is kept literally.
pub fn interpolate(text: &str, lookup: Lookup<'_>) -> String {
    if !text.contains('%') {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        if ch != '%' {
            out.push(ch);
            i += 1;
            continue;
        }
        if i + 1 < chars.len() && chars[i + 1] == '%' {
            out.push('%');
            i += 2;
            continue;
        }
        // Find the closing percent on the same line.
        let mut j = i + 1;
        while j < chars.len() && chars[j] != '%' && chars[j] != '\n' {
            j += 1;
        }
        if j >= chars.len() || chars[j] != '%' {
            out.push('%');
            i += 1;
            continue;
        }
        let name: String = chars[i + 1..j].iter().collect();
        let mut k = j + 1;
        while k < chars.len() && chars[k] == '_' {
            k += 1;
        }
        let width = if k > j + 1 { Some(k - (j + 1)) } else { None };
        let value = lookup(&name).unwrap_or_default();
        match width {
            Some(w) => out.push_str(&fit(&value, w)),
            None => out.push_str(&value),
        }
        i = k;
    }
    out
}

/// Interpolate a layout line that may contain escapes (`%%`).
pub fn interpolate_line(text: &str, lookup: Lookup<'_>) -> String {
    interpolate(text, lookup)
}

/// The WBAT result word for a button label (section 4.6):
///
/// 1. remove one trailing `&` (it only disables the hotkey);
/// 2. if a word contains `&`, take that word, otherwise the first word;
/// 3. drop every character except letters, digits and `@ $ - _ ?`.
pub fn command_word(label: &str) -> String {
    let trimmed = label.trim();
    let body = trimmed.strip_suffix('&').unwrap_or(trimmed).trim_end();
    let words: Vec<&str> = body.split_whitespace().collect();
    let pick = words
        .iter()
        .find(|w| w.contains('&'))
        .or_else(|| words.first());
    pick.map(|w| {
        w.chars()
            .filter(|c| c.is_alphanumeric() || matches!(c, '@' | '$' | '-' | '_' | '?'))
            .collect()
    })
    .unwrap_or_default()
}

/// The label as displayed: every `&` hotkey marker removed.
pub fn display_label(label: &str) -> String {
    label.chars().filter(|c| *c != '&').collect()
}

/// The hotkey character and its byte index in the *displayed* label.
///
/// Returns `None` when there is no `&`, or when the only `&` is the trailing
/// "disable hotkey" marker.
pub fn hotkey(label: &str) -> Option<(usize, char)> {
    let chars: Vec<char> = label.chars().collect();
    let mut display_index = 0usize;
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '&' {
            // A trailing '&' only disables the hotkey and is not displayed.
            if i + 1 >= chars.len() {
                return None;
            }
            return Some((display_index, chars[i + 1]));
        }
        display_index += 1;
        i += 1;
    }
    None
}

/// Parse a boolean config/CLI value.
pub fn parse_bool(s: &str) -> Option<bool> {
    match s.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" | "y" => Some(true),
        "0" | "false" | "no" | "off" | "n" => Some(false),
        _ => None,
    }
}

/// Evaluate a `#bar` expression: a sum of non-negative integers joined by `+`
/// or `-` (for example `3`, `0+1`, `10-2`). Returns `None` on garbage.
pub fn eval_bar(expr: &str) -> Option<u32> {
    let s: String = expr.chars().filter(|c| !c.is_whitespace()).collect();
    if s.is_empty() {
        return None;
    }
    let bytes = s.as_bytes();
    if matches!(bytes.last(), Some(b'+') | Some(b'-')) {
        return None;
    }
    let mut i = 0usize;
    let mut total: i64 = 0;
    let mut sign: i64 = 1;
    let mut saw_term = false;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                sign = 1;
                i += 1;
            }
            b'-' => {
                sign = -1;
                i += 1;
            }
            b'0'..=b'9' => {
                let start = i;
                while i < bytes.len() && bytes[i].is_ascii_digit() {
                    i += 1;
                }
                let value: i64 = s[start..i].parse().ok()?;
                total += sign * value;
                sign = 1;
                saw_term = true;
            }
            _ => return None,
        }
    }
    if !saw_term || total < 0 || total > u32::MAX as i64 {
        return None;
    }
    Some(total as u32)
}

/// Replace `#?` countdown markers in a plain string with `replacement`.
pub fn replace_countdown(text: &str, replacement: &str) -> String {
    text.replace("#?", replacement)
}

/// Number of cells a string occupies (exposed for callers).
pub fn width_of(s: &str) -> usize {
    str_width(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(name: &str) -> Option<String> {
        match name {
            "USERNAME" => Some("Ada".into()),
            "name" => Some("Ana & Bob".into()),
            "empty" => Some(String::new()),
            _ => None,
        }
    }

    #[test]
    fn basic_interpolation() {
        assert_eq!(interpolate("hi %USERNAME%!", &env), "hi Ada!");
        assert_eq!(interpolate("%missing%", &env), "");
        assert_eq!(interpolate("100%%", &env), "100%");
        assert_eq!(interpolate("a % b", &env), "a % b");
        assert_eq!(interpolate("trailing %", &env), "trailing %");
    }

    #[test]
    fn fixed_width_fields() {
        assert_eq!(interpolate("[%name%____]", &env), "[Ana ]");
        assert_eq!(interpolate("[%name%__________]", &env), "[Ana & Bob ]");
        assert_eq!(interpolate("[%USERNAME%__]", &env), "[Ad]");
    }

    #[test]
    fn command_word_rules() {
        assert_eq!(command_word("OK"), "OK");
        assert_eq!(command_word("E&xit"), "Exit");
        assert_eq!(command_word("Tour"), "Tour");
        assert_eq!(command_word("Save &Exit"), "Exit");
        assert_eq!(command_word("Close&"), "Close");
        assert_eq!(command_word("a b"), "a");
        assert_eq!(command_word("&Report"), "Report");
        assert_eq!(command_word("1 2"), "1");
    }

    #[test]
    fn display_and_hotkey() {
        assert_eq!(display_label("E&xit"), "Exit");
        assert_eq!(display_label("Close&"), "Close");
        assert_eq!(hotkey("E&xit"), Some((1, 'x')));
        assert_eq!(hotkey("&Tour"), Some((0, 'T')));
        assert_eq!(hotkey("Close&"), None);
        assert_eq!(hotkey("plain"), None);
    }

    #[test]
    fn bar_arithmetic() {
        assert_eq!(eval_bar("3"), Some(3));
        assert_eq!(eval_bar("0+1"), Some(1));
        assert_eq!(eval_bar("10-2"), Some(8));
        assert_eq!(eval_bar(""), None);
        assert_eq!(eval_bar("x"), None);
        assert_eq!(eval_bar("3+"), None);
        assert_eq!(eval_bar("2-5"), None);
    }

    #[test]
    fn bools() {
        assert_eq!(parse_bool("1"), Some(true));
        assert_eq!(parse_bool("off"), Some(false));
        assert_eq!(parse_bool("maybe"), None);
    }
}
