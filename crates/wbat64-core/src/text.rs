//! Display-width helpers.
//!
//! wbat64 works on a character grid, so every measurement is in terminal cells
//! rather than Rust `char`s. Wide (CJK, emoji) characters occupy two cells and
//! combining marks occupy none.

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// Display width of a string in terminal cells.
pub fn str_width(s: &str) -> usize {
    UnicodeWidthStr::width(s)
}

/// Display width of one character in terminal cells.
pub fn char_width(c: char) -> usize {
    UnicodeWidthChar::width(c).unwrap_or(0)
}

/// Truncate to at most `width` cells, never splitting a wide character.
pub fn truncate(s: &str, width: usize) -> String {
    let mut out = String::new();
    let mut used = 0usize;
    for ch in s.chars() {
        let w = char_width(ch);
        if used + w > width {
            break;
        }
        out.push(ch);
        used += w;
    }
    out
}

/// Pad with spaces on the right to exactly `width` cells (truncating if needed).
pub fn truncate_pad(s: &str, width: usize) -> String {
    let mut out = truncate(s, width);
    let used = str_width(&out);
    for _ in used..width {
        out.push(' ');
    }
    out
}

/// Pad/truncate to exactly `width` cells (the fixed-width `%VAR%___` rule).
pub fn fit(s: &str, width: usize) -> String {
    truncate_pad(s, width)
}

/// Expand tabs to the next multiple of `tabstop`, based on display columns.
pub fn expand_tabs(s: &str, tabstop: usize) -> String {
    if !s.contains('\t') {
        return s.to_string();
    }
    let tabstop = tabstop.max(1);
    let mut out = String::with_capacity(s.len());
    let mut col = 0usize;
    for ch in s.chars() {
        if ch == '\t' {
            let next = ((col / tabstop) + 1) * tabstop;
            for _ in col..next {
                out.push(' ');
            }
            col = next;
        } else {
            out.push(ch);
            col += char_width(ch);
        }
    }
    out
}

/// Split a string into `(char, cell_column)` pairs.
pub fn cells(s: &str) -> Vec<(char, usize)> {
    let mut out = Vec::with_capacity(s.len());
    let mut col = 0usize;
    for ch in s.chars() {
        out.push((ch, col));
        col += char_width(ch);
    }
    out
}

/// Capitalise the first character of a string (used by `U` input fields).
pub fn uppercase_first(s: &mut String) {
    if let Some(first) = s.chars().next() {
        let upper: String = first.to_uppercase().collect();
        if upper != first.to_string() {
            let rest: String = s.chars().skip(1).collect();
            *s = format!("{upper}{rest}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widths() {
        assert_eq!(str_width("abc"), 3);
        assert_eq!(str_width("日本語"), 6);
        assert_eq!(char_width('\u{0301}'), 0);
    }

    #[test]
    fn truncation_does_not_split_wide_chars() {
        assert_eq!(truncate("日本語", 5), "日本");
        assert_eq!(truncate_pad("日本", 5), "日本 ");
    }

    #[test]
    fn tabs_expand_on_columns() {
        assert_eq!(expand_tabs("a\tb", 8), "a       b");
        assert_eq!(expand_tabs("日本語\tx", 8), "日本語  x");
    }
}
