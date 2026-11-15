//! ANSI escape encoding for the renderer updates.
//!
//! Output goes to the console device, never to stdout, so plain ANSI sequences
//! are written directly. Windows 10+ (and every Unix terminal) understands
//! them once virtual-terminal processing is enabled by the TUI layer.

use wbat64_core::Style;

use crate::{Screen, Update};

/// Reset all attributes.
pub const RESET: &str = "\x1b[0m";
/// Hide the text cursor.
pub const HIDE_CURSOR: &str = "\x1b[?25l";
/// Show the text cursor.
pub const SHOW_CURSOR: &str = "\x1b[?25h";
/// Save the cursor position (DECSC).
pub const SAVE_CURSOR: &str = "\x1b7";
/// Restore the cursor position (DECRC).
pub const RESTORE_CURSOR: &str = "\x1b8";

pub fn encode_style_reset() -> &'static str {
    RESET
}

pub fn encode_hide_cursor() -> &'static str {
    HIDE_CURSOR
}

pub fn encode_show_cursor() -> &'static str {
    SHOW_CURSOR
}

/// Move the cursor to a 0-based cell.
pub fn encode_cursor(x: u16, y: u16) -> String {
    format!("\x1b[{};{}H", y.saturating_add(1), x.saturating_add(1))
}

/// Set a color pair without moving the cursor.
pub fn encode_style(style: Style) -> String {
    format!("\x1b[{}", style.sgr())
}

/// Clear the screen, filling it with `style`.
pub fn encode_clear(style: Style) -> String {
    format!("\x1b[{} \x1b[2J\x1b[H", style.sgr())
}

/// Encode a list of changed runs. Runs are normally in row-major order, so the
/// encoder only emits cursor moves when the run does not continue the previous
/// position.
pub fn encode_updates(updates: &[Update]) -> String {
    let mut out = String::new();
    let mut cursor: Option<(u16, u16)> = None;
    let mut style: Option<Style> = None;
    for update in updates {
        if cursor != Some((update.x, update.y)) {
            out.push_str(&encode_cursor(update.x, update.y));
        }
        if style != Some(update.style) {
            out.push_str(&format!("\x1b[{}", update.style.sgr()));
            style = Some(update.style);
        }
        out.push_str(&update.text);
        let advance: u16 = update
            .text
            .chars()
            .map(|c| wbat64_core::char_width(c) as u16)
            .sum();
        cursor = Some((update.x + advance, update.y));
    }
    if !out.is_empty() {
        out.push_str(RESET);
    }
    out
}

/// Write a whole screen (used on resize or after `cls`).
pub fn encode_screen(screen: &Screen) -> String {
    let mut out = String::new();
    for y in 0..screen.height() {
        for x in 0..screen.width() {
            let cell = screen.cell(x, y);
            if cell.unknown || cell.cont {
                continue;
            }
            out.push_str(&encode_cursor(x, y));
            out.push_str(&format!("\x1b[{}", cell.style().sgr()));
            out.push(cell.ch);
        }
    }
    if !out.is_empty() {
        out.push_str(RESET);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn style() -> Style {
        Style::new(
            wbat64_core::Color::Indexed(7),
            wbat64_core::Color::Indexed(0),
        )
    }

    #[test]
    fn cursor_is_one_based() {
        assert_eq!(encode_cursor(0, 0), "\x1b[1;1H");
        assert_eq!(encode_cursor(9, 4), "\x1b[5;10H");
    }

    #[test]
    fn adjacent_runs_do_not_move_the_cursor() {
        let updates = vec![
            Update {
                x: 2,
                y: 1,
                text: "ab".into(),
                style: style(),
            },
            Update {
                x: 4,
                y: 1,
                text: "c".into(),
                style: style(),
            },
        ];
        let encoded = encode_updates(&updates);
        assert_eq!(encoded.matches("\x1b[2;3H").count(), 1);
        assert!(encoded.contains("abc"));
        assert!(encoded.ends_with(RESET));
    }

    #[test]
    fn style_changes_are_emitted() {
        let other = Style::new(
            wbat64_core::Color::Indexed(1),
            wbat64_core::Color::Indexed(6),
        );
        let updates = vec![
            Update {
                x: 0,
                y: 0,
                text: "a".into(),
                style: style(),
            },
            Update {
                x: 1,
                y: 0,
                text: "b".into(),
                style: other,
            },
        ];
        let encoded = encode_updates(&updates);
        assert!(encoded.contains("\x1b[0;37;40m"));
        // Blue is DOS 1 -> ANSI 4 (34), brown is DOS 6 -> ANSI 3 (43).
        assert!(encoded.contains("\x1b[0;34;43m"));
    }

    #[test]
    fn empty_updates_are_empty() {
        assert_eq!(encode_updates(&[]), "");
    }
}
