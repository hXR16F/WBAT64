//! Editable input-field state (text, caret and horizontal scroll).

use wbat64_core::{char_width, Control};

/// One `[$ var,width,flags]` field.
#[derive(Clone, Debug)]
pub struct InputState {
    pub var: String,
    pub chars: Vec<char>,
    /// Caret position as a character index, 0..=len.
    pub cursor: usize,
    /// First visible character index.
    pub scroll: usize,
    pub width: usize,
    pub upper: bool,
    pub password: bool,
    pub required: bool,
}

impl InputState {
    pub fn new(control: &Control, initial: &str) -> InputState {
        let (var, width, upper, password, required) = match control {
            Control::Input {
                var,
                width,
                upper,
                password,
                required,
                ..
            } => (var.clone(), *width, *upper, *password, *required),
            _ => (String::new(), 1, false, false, false),
        };
        let text = if upper {
            initial.to_uppercase()
        } else {
            initial.to_string()
        };
        let chars: Vec<char> = text.chars().collect();
        let cursor = chars.len();
        InputState {
            var,
            chars,
            cursor,
            scroll: 0,
            width: width.max(1),
            upper,
            password,
            required,
        }
    }

    pub fn value(&self) -> String {
        self.chars.iter().collect()
    }

    pub fn is_empty(&self) -> bool {
        self.chars.iter().all(|c| c.is_whitespace())
    }

    fn mask(&self, ch: char) -> char {
        if self.password {
            '*'
        } else {
            ch
        }
    }

    /// Characters from `scroll` that fit in the field, padded with spaces.
    pub fn visible(&self) -> String {
        let mut out = String::new();
        let mut used = 0usize;
        for ch in self.chars.iter().skip(self.scroll) {
            let shown = self.mask(*ch);
            let w = char_width(shown);
            if used + w > self.width {
                break;
            }
            out.push(shown);
            used += w;
        }
        while used < self.width {
            out.push(' ');
            used += 1;
        }
        out
    }

    /// Cell offset of the caret inside the field.
    pub fn caret_cell(&self) -> u16 {
        let mut used = 0usize;
        for ch in self
            .chars
            .iter()
            .skip(self.scroll)
            .take(self.cursor.saturating_sub(self.scroll))
        {
            used += char_width(self.mask(*ch));
        }
        used.min(self.width.saturating_sub(1)) as u16
    }

    fn raw_caret(&self) -> usize {
        let mut used = 0usize;
        for ch in self
            .chars
            .iter()
            .skip(self.scroll)
            .take(self.cursor.saturating_sub(self.scroll))
        {
            used += char_width(self.mask(*ch));
        }
        used
    }

    /// Adjust `scroll` so the caret stays visible.
    pub fn ensure_visible(&mut self) {
        if self.cursor < self.scroll {
            self.scroll = self.cursor;
        }
        while self.raw_caret() >= self.width && self.scroll < self.cursor {
            self.scroll += 1;
        }
    }

    pub fn insert(&mut self, ch: char) {
        if ch == '\r' || ch == '\n' {
            return;
        }
        if self.upper {
            for upper in ch.to_uppercase() {
                self.chars.insert(self.cursor, upper);
                self.cursor += 1;
            }
        } else {
            self.chars.insert(self.cursor, ch);
            self.cursor += 1;
        }
        self.ensure_visible();
    }

    pub fn insert_str(&mut self, text: &str) {
        for ch in text.chars() {
            self.insert(ch);
        }
    }

    pub fn backspace(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
            self.chars.remove(self.cursor);
            self.ensure_visible();
        }
    }

    pub fn delete(&mut self) {
        if self.cursor < self.chars.len() {
            self.chars.remove(self.cursor);
            self.ensure_visible();
        }
    }

    pub fn left(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
            self.ensure_visible();
        }
    }

    pub fn right(&mut self) {
        if self.cursor < self.chars.len() {
            self.cursor += 1;
            self.ensure_visible();
        }
    }

    pub fn home(&mut self) {
        self.cursor = 0;
        self.scroll = 0;
    }

    pub fn end(&mut self) {
        self.cursor = self.chars.len();
        self.ensure_visible();
    }

    /// Move the caret to the clicked column inside the field.
    pub fn set_caret_from_column(&mut self, column: u16) {
        let mut used = 0usize;
        let target = column as usize;
        for (i, ch) in self.chars.iter().enumerate().skip(self.scroll) {
            let w = char_width(self.mask(*ch));
            if used + w > target {
                self.cursor = i;
                return;
            }
            used += w;
            if used >= target {
                self.cursor = i + 1;
                return;
            }
        }
        self.cursor = self.chars.len();
        self.ensure_visible();
    }

    /// Trim trailing spaces, matching WBAT's result rule.
    pub fn result_value(&self) -> String {
        self.value().trim_end().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn control(width: usize, upper: bool, password: bool) -> Control {
        Control::Input {
            var: "name".into(),
            len: Some(width),
            upper,
            password,
            required: false,
            width,
        }
    }

    #[test]
    fn typing_and_editing() {
        let mut input = InputState::new(&control(10, false, false), "");
        input.insert_str("abc");
        assert_eq!(input.value(), "abc");
        assert_eq!(input.cursor, 3);
        input.left();
        input.insert('X');
        assert_eq!(input.value(), "abXc");
        input.backspace();
        assert_eq!(input.value(), "abc");
        input.home();
        input.delete();
        assert_eq!(input.value(), "bc");
        input.end();
        assert_eq!(input.cursor, 2);
    }

    #[test]
    fn uppercase_flag() {
        let mut input = InputState::new(&control(10, true, false), "ada");
        assert_eq!(input.value(), "ADA");
        input.insert('x');
        assert_eq!(input.value(), "ADAx".replace('x', "X"));
    }

    #[test]
    fn password_masks_display_but_not_value() {
        let input = InputState::new(&control(6, false, true), "secret");
        assert_eq!(input.visible(), "******");
        assert_eq!(input.value(), "secret");
    }

    #[test]
    fn scrolling_keeps_caret_visible() {
        let mut input = InputState::new(&control(5, false, false), "abcdefgh");
        input.end();
        assert!(input.scroll > 0);
        assert!(input.raw_caret() < 5);
        assert_eq!(input.visible().chars().count(), 5);
    }

    #[test]
    fn caret_from_click() {
        let mut input = InputState::new(&control(10, false, false), "abc");
        input.set_caret_from_column(1);
        assert_eq!(input.cursor, 1);
        input.set_caret_from_column(9);
        assert_eq!(input.cursor, 3);
    }

    #[test]
    fn result_trims_trailing_spaces_only() {
        let mut input = InputState::new(&control(10, false, false), "");
        input.insert_str("a b  ");
        assert_eq!(input.result_value(), "a b");
    }
}
