//! A session ties a backend to a screen model and is shared by all commands in
//! one wbat64 invocation.

use std::time::Duration;

use wbat64_core::{resolve_pos, Config, LineKind, Pos, Rect, Result, Size, Span, Style, Unit};
use wbat64_render::{
    diff, draw_divider, encode_clear, encode_cursor, encode_hide_cursor, encode_show_cursor,
    encode_updates, Screen,
};

use crate::event::{parse_key_script, Event};
use crate::term::{Backend, ConsoleBackend, HeadlessBackend};

/// Options that decide which backend to use.
#[derive(Clone, Debug, Default)]
pub struct SessionOptions {
    pub mouse: bool,
    /// Scripted events; implies the headless backend.
    pub keys: Option<String>,
    pub headless_size: Option<(u16, u16)>,
    pub dump_screen: bool,
}

/// The active terminal plus the cells wbat64 believes are on screen.
pub struct Session {
    backend: Box<dyn Backend>,
    /// The desired screen (drawn by commands).
    pub screen: Screen,
    /// The screen the terminal is believed to show.
    prev: Screen,
    pub size: Size,
    text_highlight_style: Style,
    text_title_style: Style,
    text_divider_style: Style,
    cursor: (u16, u16),
    saved: Vec<(String, Screen)>,
    hide_cursor: bool,
    keep_frame: bool,
}

impl Session {
    pub fn open(config: &Config, options: &SessionOptions) -> Result<Session> {
        let mut backend: Box<dyn Backend> = match &options.keys {
            Some(script) => Box::new(HeadlessBackend::new(
                options.headless_size.unwrap_or((80, 25)),
                parse_key_script(script),
                options.dump_screen,
            )),
            None => Box::new(ConsoleBackend::new(options.mouse)?),
        };
        let (cols, rows) = backend.size()?;
        let cursor = backend.cursor_position().unwrap_or((0, 0));
        // Win32 consoles can be read: use the real screen as the baseline so
        // moving or closing a box restores the background exactly instead of
        // blanking it. Other terminals start with an unknown screen.
        let mut captured = backend
            .capture_screen()
            .unwrap_or_else(|| Screen::unknown(cols, rows));
        captured.resize(cols, rows);
        let mut session = Session {
            backend,
            screen: captured.clone(),
            prev: captured,
            size: Size::new(cols, rows),
            text_highlight_style: config.theme.text_hi,
            text_title_style: config.theme.text_title,
            text_divider_style: config.theme.divider,
            cursor,
            saved: Vec::new(),
            hide_cursor: false,
            keep_frame: options.dump_screen,
        };
        session.hide_cursor = true;
        session.backend.write_str(encode_hide_cursor())?;
        Ok(session)
    }

    pub fn backend(&self) -> &dyn Backend {
        &*self.backend
    }

    pub fn is_headless(&self) -> bool {
        self.backend.is_headless()
    }

    /// Headless runs asked for `--dump-screen`: keep the last frame visible so
    /// the dump shows the dialog rather than the restored background.
    pub fn keep_frame(&self) -> bool {
        self.keep_frame
    }

    pub fn supports_ansi(&self) -> bool {
        self.backend.supports_ansi()
    }

    /// Push the screen model to the terminal.
    pub fn present(&mut self) -> Result<()> {
        let updates = diff(&self.prev, &self.screen);
        if updates.is_empty() {
            return Ok(());
        }
        let encoded = encode_updates(&updates);
        self.backend.write_str(&encoded)?;
        self.backend.flush()?;
        self.prev = self.screen.clone();
        Ok(())
    }

    /// Re-query the terminal size; returns true when it changed.
    pub fn refresh_size(&mut self) -> Result<bool> {
        let (cols, rows) = self.backend.size()?;
        if cols == self.size.cols && rows == self.size.rows {
            return Ok(false);
        }
        self.size = Size::new(cols, rows);
        self.screen.resize(cols, rows);
        self.prev.resize(cols, rows);
        Ok(true)
    }

    pub fn cursor(&self) -> (u16, u16) {
        self.cursor
    }

    pub fn set_cursor(&mut self, x: u16, y: u16) -> Result<()> {
        self.cursor = (x, y);
        self.backend.write_str(&encode_cursor(x, y))
    }

    /// Clear the screen with `style`, which also resets the model.
    pub fn clear(&mut self, style: Style) -> Result<()> {
        self.backend.write_str(&encode_clear(style))?;
        self.backend.flush()?;
        self.screen = Screen::blank(self.size.cols, self.size.rows, style);
        self.prev = self.screen.clone();
        self.cursor = (0, 0);
        Ok(())
    }

    /// Fill a rectangle with a repeating pattern (or just a color).
    pub fn fill(&mut self, rect: Rect, pattern: &str, style: Style, shift: i32) {
        let rect = rect.intersect(Rect::new(0, 0, self.size.cols, self.size.rows));
        if pattern.is_empty() {
            self.screen.recolor(rect, style);
            return;
        }
        let chars: Vec<char> = pattern.chars().collect();
        let len = chars.len();
        for y in rect.y..rect.bottom() {
            let offset = ((y as i32 - rect.y as i32) * shift).rem_euclid(len as i32) as usize;
            let mut x = rect.x;
            let mut idx = offset;
            while x < rect.right() {
                let ch = chars[idx % len];
                self.screen.put(x, y, ch, style);
                x += wbat64_core::char_width(ch).max(1) as u16;
                idx += 1;
            }
        }
    }

    /// Draw a text unit at a position (no frame).
    ///
    /// With explicit colours the whole text rectangle is painted. Without
    /// colours WBAT leaves the screen colours alone: only the characters are
    /// replaced, preserving the cells underneath.
    pub fn draw_text(&mut self, unit: &Unit, pos: Pos, colors: Option<Style>) -> Result<()> {
        let (content_w, content_h) = unit.content_size();
        let pos = resolve_pos(
            Some(Pos::new(pos.line.max(1), pos.col.max(1))),
            content_w.max(1) as u16,
            content_h.max(1) as u16,
            self.size,
        );
        let x0 = pos.col.saturating_sub(1);
        let y0 = pos.line.saturating_sub(1);
        let plain_style = colors.unwrap_or_else(|| self.screen.style_at(x0, y0));
        if colors.is_some() {
            self.screen.clear_rect(
                Rect::new(x0, y0, content_w as u16, content_h as u16),
                plain_style,
            );
        }
        for (i, line) in unit.lines.iter().enumerate() {
            let y = y0 + i as u16;
            if y >= self.size.rows {
                break;
            }
            match line.kind {
                LineKind::Divider => {
                    draw_divider(
                        &mut self.screen,
                        Rect::new(x0, y, content_w as u16, 1),
                        y,
                        Style::new(self.text_divider_style.fg, plain_style.bg),
                    );
                }
                LineKind::TitleBar => {
                    self.screen.fill(
                        Rect::new(x0, y, content_w as u16, 1),
                        ' ',
                        self.text_title_style,
                    );
                    let mut x = x0;
                    for span in &line.spans {
                        x = self.draw_text_span(
                            span,
                            x,
                            y,
                            self.text_title_style,
                            self.text_title_style,
                        );
                    }
                }
                LineKind::Normal => {
                    let mut x = x0;
                    for span in &line.spans {
                        x = if colors.is_some() {
                            self.draw_text_span(span, x, y, plain_style, self.text_highlight_style)
                        } else {
                            self.draw_text_span_preserving(span, x, y, self.text_highlight_style)
                        };
                    }
                }
            }
        }
        self.cursor = (x0, y0.saturating_add(content_h as u16));
        Ok(())
    }

    fn draw_text_span_preserving(&mut self, span: &Span, x: u16, y: u16, hi_style: Style) -> u16 {
        match span {
            Span::Text(s) => self.screen.print_preserving(x, y, s),
            Span::Highlight(s) => self.screen.print(x, y, s, hi_style),
            Span::Var { width, .. } => x + width.unwrap_or(0) as u16,
            Span::Countdown => self.screen.print_preserving(x, y, "--"),
            Span::Control(c) => {
                let text = c.display_label();
                self.screen.print_preserving(x, y, &text)
            }
        }
    }

    fn draw_text_span(
        &mut self,
        span: &Span,
        x: u16,
        y: u16,
        style: Style,
        hi_style: Style,
    ) -> u16 {
        match span {
            Span::Text(s) => self.screen.print(x, y, s, style),
            Span::Highlight(s) => self.screen.print(x, y, s, hi_style),
            Span::Var { width, .. } => x + width.unwrap_or(0) as u16,
            Span::Countdown => self.screen.print(x, y, "--", style),
            Span::Control(c) => {
                let text = c.display_label();
                self.screen.print(x, y, &text, style)
            }
        }
    }

    /// Move the real cursor; `feeds` newlines scroll the screen.
    pub fn place_cursor(&mut self, line: Option<u16>, feeds: u16) -> Result<()> {
        if let Some(line) = line {
            let y = line.saturating_sub(1).min(self.size.rows.saturating_sub(1));
            self.set_cursor(0, y)?;
        }
        if feeds > 0 {
            let mut text = "\r\n".repeat(feeds as usize);
            text.push_str(encode_show_cursor());
            self.backend.write_str(&text)?;
            self.backend.flush()?;
            let (x, y) = self.cursor;
            self.cursor = (x, y.saturating_add(feeds));
        }
        Ok(())
    }

    pub fn save_screen(&mut self, id: Option<String>) {
        self.saved
            .push((id.unwrap_or_default(), self.screen.clone()));
        if self.saved.len() > 20 {
            self.saved.remove(0);
        }
    }

    /// Restore a saved screen (last one when `id` is empty).
    pub fn restore_screen(&mut self, id: Option<String>) -> Result<bool> {
        let want = id.unwrap_or_default();
        let position = self
            .saved
            .iter()
            .rposition(|(sid, _)| want.is_empty() || *sid == want);
        match position {
            Some(index) => {
                let (_, screen) = self.saved.remove(index);
                self.screen = screen;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    /// Apply a 16-color DOS palette with OSC 4.
    pub fn apply_palette(&mut self, colors: &[[u8; 3]; 16]) -> Result<()> {
        let mut out = String::new();
        for (i, rgb) in colors.iter().enumerate() {
            out.push_str(&format!(
                "\x1b]4;{};rgb:{:02x}/{:02x}/{:02x}\x07",
                i, rgb[0], rgb[1], rgb[2]
            ));
        }
        self.backend.write_str(&out)?;
        self.backend.flush()
    }

    pub fn next_event(&mut self, timeout: Duration) -> Result<Event> {
        match self.backend.poll_event(timeout)? {
            Some(event) => Ok(event),
            None => Ok(Event::Tick),
        }
    }

    /// The text dump used by `--dump-screen`.
    pub fn dump_text(&self) -> String {
        self.screen.to_text()
    }

    /// Width of the screen in cells.
    pub fn width(&self) -> u16 {
        self.size.cols
    }

    pub fn height(&self) -> u16 {
        self.size.rows
    }

    /// Restore the area a dialog occupied onto the state before it opened.
    pub fn restore_area(&mut self, base: &Screen, area: Rect, fallback: Style) {
        self.screen.restore_region(base, area, fallback);
    }

    pub fn show_cursor(&mut self) -> Result<()> {
        self.hide_cursor = false;
        self.backend.write_str(encode_show_cursor())?;
        self.backend.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wbat64_core::{parse_layout, Color, Pos, Rect};

    fn session() -> Session {
        let options = SessionOptions {
            keys: Some("esc".to_string()),
            headless_size: Some((20, 8)),
            ..SessionOptions::default()
        };
        Session::open(&Config::native_defaults(), &options).unwrap()
    }

    fn unit(text: &str) -> Unit {
        let (mut units, errors) = parse_layout(text, "test", '~');
        assert!(errors.is_empty(), "{errors:?}");
        units.pop().unwrap()
    }

    #[test]
    fn text_without_colors_keeps_the_screen_colours() {
        let mut s = session();
        let screen_style = Style::new(Color::Indexed(7), Color::Indexed(4));
        s.screen.fill(Rect::new(0, 0, 20, 8), ' ', screen_style);
        s.draw_text(&unit(":u\nhello\n:"), Pos::new(1, 1), None)
            .unwrap();
        assert_eq!(s.screen.cell(0, 0).ch, 'h');
        assert_eq!(s.screen.style_at(0, 0), screen_style);
        // The rest of the line/rectangle was not repainted.
        assert_eq!(s.screen.style_at(10, 0), screen_style);
        assert_eq!(s.screen.style_at(0, 1), screen_style);
    }

    #[test]
    fn text_with_colors_fills_the_rectangle() {
        let mut s = session();
        let screen_style = Style::new(Color::Indexed(7), Color::Indexed(4));
        s.screen.fill(Rect::new(0, 0, 20, 8), ' ', screen_style);
        let text_style = Style::new(Color::Indexed(0), Color::Indexed(2));
        s.draw_text(&unit(":u\nhello\n:"), Pos::new(1, 1), Some(text_style))
            .unwrap();
        assert_eq!(s.screen.style_at(0, 0), text_style);
        assert_eq!(s.screen.style_at(4, 0), text_style);
        // Outside the text rectangle.
        assert_eq!(s.screen.style_at(5, 0), screen_style);
    }

    #[test]
    fn fill_with_only_a_background_recolors_and_keeps_text() {
        use wbat64_core::parse_colors;
        let mut s = session();
        s.screen
            .print(0, 0, "hi", Style::new(Color::Indexed(0), Color::Indexed(4)));
        let style = parse_colors("(on white)").unwrap();
        s.fill(Rect::new(0, 0, 4, 1), "", style, 0);
        assert_eq!(s.screen.cell(0, 0).ch, 'h');
        assert_eq!(s.screen.cell(1, 0).ch, 'i');
        assert_eq!(s.screen.cell(0, 0).bg, Color::Indexed(7));
        assert_eq!(s.screen.cell(3, 0).bg, Color::Indexed(7));
        assert_eq!(s.screen.cell(3, 0).ch, ' ');
    }

    #[test]
    fn text_title_and_divider_use_their_roles() {
        let mut s = session();
        let screen_style = Style::new(Color::Indexed(7), Color::Indexed(4));
        s.screen.fill(Rect::new(0, 0, 20, 8), ' ', screen_style);
        s.draw_text(&unit(":u\n#Title\n─────\n:"), Pos::new(1, 1), None)
            .unwrap();
        assert_eq!(s.screen.style_at(0, 0), s.text_title_style);
        assert_eq!(s.screen.cell(0, 1).ch, '─');
        assert_eq!(s.screen.cell(0, 1).fg, Color::Indexed(3));
        assert_eq!(s.screen.cell(0, 1).bg, Color::Indexed(4));
    }
}
