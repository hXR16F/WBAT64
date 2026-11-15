//! Cells, the screen buffer and the diff algorithm.

use wbat64_core::{char_width, Color, Rect, Style};

/// One terminal cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell {
    pub ch: char,
    pub fg: Color,
    pub bg: Color,
    /// This is the second half of a double-width character.
    pub cont: bool,
    /// wbat64 has never painted this cell; its current content is unknown.
    pub unknown: bool,
}

impl Cell {
    /// A cell whose content wbat64 does not know.
    pub const UNKNOWN: Cell = Cell {
        ch: ' ',
        fg: Color::Default,
        bg: Color::Default,
        cont: false,
        unknown: true,
    };

    pub fn blank(style: Style) -> Cell {
        Cell {
            ch: ' ',
            fg: style.fg,
            bg: style.bg,
            cont: false,
            unknown: false,
        }
    }

    pub fn new(ch: char, style: Style) -> Cell {
        Cell {
            ch,
            fg: style.fg,
            bg: style.bg,
            cont: false,
            unknown: false,
        }
    }

    pub fn style(&self) -> Style {
        Style::new(self.fg, self.bg)
    }
}

/// A rectangular grid of cells.
#[derive(Clone, Debug)]
pub struct Screen {
    width: u16,
    height: u16,
    cells: Vec<Cell>,
}

impl Screen {
    /// A screen full of unknown cells.
    pub fn unknown(width: u16, height: u16) -> Screen {
        let width = width.max(1);
        let height = height.max(1);
        Screen {
            width,
            height,
            cells: vec![Cell::UNKNOWN; width as usize * height as usize],
        }
    }

    /// A screen full of spaces in `style` (a cleared screen we painted).
    pub fn blank(width: u16, height: u16, style: Style) -> Screen {
        let width = width.max(1);
        let height = height.max(1);
        Screen {
            width,
            height,
            cells: vec![Cell::blank(style); width as usize * height as usize],
        }
    }

    pub fn width(&self) -> u16 {
        self.width
    }

    pub fn height(&self) -> u16 {
        self.height
    }

    pub fn size(&self) -> (u16, u16) {
        (self.width, self.height)
    }

    fn index(&self, x: u16, y: u16) -> Option<usize> {
        if x >= self.width || y >= self.height {
            None
        } else {
            Some(y as usize * self.width as usize + x as usize)
        }
    }

    pub fn cell(&self, x: u16, y: u16) -> Cell {
        match self.index(x, y) {
            Some(i) => self.cells[i],
            None => Cell::UNKNOWN,
        }
    }

    pub fn set_cell(&mut self, x: u16, y: u16, cell: Cell) {
        if let Some(i) = self.index(x, y) {
            self.cells[i] = cell;
        }
    }

    /// True when the cell was painted by wbat64.
    pub fn is_known(&self, x: u16, y: u16) -> bool {
        !self.cell(x, y).unknown
    }

    /// Blank one cell with a known style (used when half of a wide character
    /// is overwritten, so the diff actually repaints the remaining half).
    fn blank_cell(&mut self, x: u16, y: u16, style: Style) {
        self.set_cell(x, y, Cell::blank(style));
    }

    /// Write one character, handling double-width characters.
    pub fn put(&mut self, x: u16, y: u16, ch: char, style: Style) {
        if x >= self.width || y >= self.height {
            return;
        }
        let width = char_width(ch);
        if width == 0 {
            // Combining marks are not supported on a cell grid; ignore.
            return;
        }
        if width == 1 {
            let current = self.cell(x, y);
            if current.cont {
                // Overwriting the right half of a wide character: the left
                // half must become a blank the renderer will repaint.
                self.blank_cell(x.saturating_sub(1), y, style);
            } else if char_width(current.ch) == 2 {
                // Overwriting the left half: its continuation cell is stale.
                self.blank_cell(x + 1, y, style);
            }
            self.set_cell(x, y, Cell::new(ch, style));
            return;
        }
        // Wide character: needs two cells and must not start in the last column.
        if x + 1 >= self.width {
            return;
        }
        if self.cell(x, y).cont {
            self.blank_cell(x.saturating_sub(1), y, style);
        }
        let right = self.cell(x + 1, y);
        if !right.cont && char_width(right.ch) == 2 {
            // The cell to the right starts another wide character; blank its
            // trailing half before taking both cells.
            self.blank_cell(x + 2, y, style);
        }
        self.set_cell(x, y, Cell::new(ch, style));
        self.set_cell(
            x + 1,
            y,
            Cell {
                ch: ' ',
                fg: style.fg,
                bg: style.bg,
                cont: true,
                unknown: false,
            },
        );
    }

    /// Style of a cell, or the terminal default for unknown cells.
    pub fn style_at(&self, x: u16, y: u16) -> Style {
        let cell = self.cell(x, y);
        Style::new(cell.fg, cell.bg)
    }

    /// Write a string using each target cell's existing style, so text can be
    /// overlaid without repainting the background (WBAT's text default).
    pub fn print_preserving(&mut self, x: u16, y: u16, text: &str) -> u16 {
        let mut cx = x;
        for ch in text.chars() {
            let w = char_width(ch);
            if w == 0 {
                continue;
            }
            if cx >= self.width {
                break;
            }
            let style = self.style_at(cx, y);
            self.put(cx, y, ch, style);
            cx = cx.saturating_add(w as u16);
        }
        cx
    }

    /// Write a string; returns the x position after the last cell.
    pub fn print(&mut self, x: u16, y: u16, text: &str, style: Style) -> u16 {
        let mut cx = x;
        for ch in text.chars() {
            let w = char_width(ch);
            if w == 0 {
                continue;
            }
            if cx >= self.width {
                break;
            }
            self.put(cx, y, ch, style);
            cx = cx.saturating_add(w as u16);
        }
        cx
    }

    /// Fill `rect` with `ch` in `style`.
    pub fn fill(&mut self, rect: Rect, ch: char, style: Style) {
        if rect.is_empty() {
            return;
        }
        for y in rect.y..rect.bottom().min(self.height) {
            for x in rect.x..rect.right().min(self.width) {
                self.put(x, y, ch, style);
            }
        }
    }

    /// Change the style of `rect` without changing the characters (WBAT's
    /// `fill` with the string omitted).
    pub fn recolor(&mut self, rect: Rect, style: Style) {
        let rect = rect.intersect(Rect::new(0, 0, self.width, self.height));
        for y in rect.y..rect.bottom() {
            for x in rect.x..rect.right() {
                let cell = self.cell(x, y);
                if cell.unknown {
                    self.set_cell(x, y, Cell::blank(style));
                } else {
                    self.set_cell(
                        x,
                        y,
                        Cell {
                            fg: style.fg,
                            bg: style.bg,
                            ..cell
                        },
                    );
                }
            }
        }
    }

    /// Fill `rect` with spaces in `style`.
    pub fn clear_rect(&mut self, rect: Rect, style: Style) {
        self.fill(rect, ' ', style);
    }

    /// Draw a horizontal line.
    pub fn hline(&mut self, rect: Rect, y: u16, ch: char, style: Style) {
        for x in rect.x..rect.right().min(self.width) {
            self.put(x, y, ch, style);
        }
    }

    /// Draw a vertical line.
    pub fn vline(&mut self, rect: Rect, x: u16, ch: char, style: Style) {
        for y in rect.y..rect.bottom().min(self.height) {
            self.put(x, y, ch, style);
        }
    }

    /// Copy known cells from `src` into `dst_rect`, using `fallback` for cells
    /// `src` does not know (this is how a box is removed again).
    pub fn restore_region(&mut self, src: &Screen, dst_rect: Rect, fallback: Style) {
        let rect = dst_rect.intersect(Rect::new(0, 0, self.width, self.height));
        for y in rect.y..rect.bottom() {
            for x in rect.x..rect.right() {
                let cell = src.cell(x, y);
                if cell.unknown {
                    self.put(x, y, ' ', fallback);
                } else if cell.cont {
                    continue;
                } else {
                    self.put(x, y, cell.ch, cell.style());
                }
            }
        }
    }

    /// Resize, preserving the overlapping top-left area; new cells are unknown.
    pub fn resize(&mut self, width: u16, height: u16) {
        let width = width.max(1);
        let height = height.max(1);
        if width == self.width && height == self.height {
            return;
        }
        let mut next = Screen::unknown(width, height);
        for y in 0..height.min(self.height) {
            for x in 0..width.min(self.width) {
                next.set_cell(x, y, self.cell(x, y));
            }
        }
        *self = next;
    }

    /// Replace every unknown cell with a blank so the whole screen is paintable.
    pub fn materialize(&mut self, style: Style) {
        for cell in &mut self.cells {
            if cell.unknown {
                *cell = Cell::blank(style);
            }
        }
    }

    /// True when `rect` contains any cell wbat64 painted.
    pub fn any_known(&self, rect: Rect) -> bool {
        for y in rect.y..rect.bottom().min(self.height) {
            for x in rect.x..rect.right().min(self.width) {
                if !self.cell(x, y).unknown {
                    return true;
                }
            }
        }
        false
    }

    /// Reset the screen to `base` for a new frame, but materialise the area a
    /// previous frame occupied.
    ///
    /// Without this, moving a box (dragging) would set the old cells back to
    /// "unknown", and the diff skips unknown destinations, so the previous box
    /// would stay on screen and every drag would leave a trail. Cells the base
    /// screen knows are restored; cells it does not know are blanked in
    /// `fallback` so the old frame is erased.
    pub fn reset_to(&mut self, base: &Screen, previous: Option<Rect>, fallback: Style) {
        *self = base.clone();
        if let Some(area) = previous {
            let area = Rect::new(
                area.x,
                area.y,
                area.w.saturating_add(1),
                area.h.saturating_add(1),
            );
            self.restore_region(base, area, fallback);
        }
    }

    /// A textual dump (used by `--dump-screen` in tests).
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        for y in 0..self.height {
            let mut line = String::new();
            let mut x = 0;
            while x < self.width {
                let cell = self.cell(x, y);
                if cell.cont {
                    x += 1;
                    continue;
                }
                line.push(if cell.unknown { ' ' } else { cell.ch });
                x += char_width(cell.ch).max(1) as u16;
            }
            out.push_str(line.trim_end());
            out.push('\n');
        }
        out
    }
}

/// One run of changed cells on one row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Update {
    pub x: u16,
    pub y: u16,
    pub text: String,
    pub style: Style,
}

fn changed(a: &Cell, b: &Cell) -> bool {
    a.ch != b.ch || a.fg != b.fg || a.bg != b.bg || a.unknown != b.unknown
}

/// Compute the minimal set of runs needed to turn `prev` into `next`.
pub fn diff(prev: &Screen, next: &Screen) -> Vec<Update> {
    let mut updates = Vec::new();
    let width = next.width.min(prev.width);
    let height = next.height.min(prev.height);
    for y in 0..height {
        let mut x = 0u16;
        while x < width {
            let before = prev.cell(x, y);
            let after = next.cell(x, y);
            if after.unknown || after.cont || !changed(&before, &after) {
                x += 1;
                continue;
            }
            let style = Style::new(after.fg, after.bg);
            let start = x;
            let mut text = String::new();
            while x < width {
                let b = next.cell(x, y);
                if b.unknown || b.cont {
                    break;
                }
                let a = prev.cell(x, y);
                if !changed(&a, &b) || Style::new(b.fg, b.bg) != style {
                    break;
                }
                text.push(b.ch);
                x += char_width(b.ch).max(1) as u16;
            }
            if !text.is_empty() {
                updates.push(Update {
                    x: start,
                    y,
                    text,
                    style,
                });
            } else {
                x += 1;
            }
        }
    }
    updates
}

#[cfg(test)]
mod tests {
    use super::*;

    fn style() -> Style {
        Style::new(Color::Indexed(7), Color::Indexed(0))
    }

    #[test]
    fn unknown_screen_has_no_content() {
        let s = Screen::unknown(10, 3);
        assert!(s.cell(0, 0).unknown);
        assert_eq!(s.to_text(), "\n\n\n");
    }

    #[test]
    fn printing_and_wide_chars() {
        let mut s = Screen::unknown(10, 2);
        s.print(0, 0, "a日b", style());
        assert_eq!(s.cell(0, 0).ch, 'a');
        assert_eq!(s.cell(1, 0).ch, '日');
        assert!(s.cell(2, 0).cont);
        assert_eq!(s.cell(3, 0).ch, 'b');
        assert_eq!(s.to_text().lines().next().unwrap(), "a日b");
    }

    #[test]
    fn overwriting_half_a_wide_char_clears_it() {
        let mut s = Screen::unknown(10, 1);
        s.print(0, 0, "日", style());
        s.put(1, 0, 'x', style());
        assert_eq!(s.cell(0, 0).ch, ' ');
        assert!(!s.cell(0, 0).unknown, "the stale half must be repainted");
        assert_eq!(s.cell(1, 0).ch, 'x');
        assert!(!s.cell(1, 0).cont);
    }

    #[test]
    fn overwriting_a_wide_lead_blanks_its_tail() {
        let mut s = Screen::unknown(10, 1);
        s.print(0, 0, "日", style());
        s.put(0, 0, 'x', style());
        assert_eq!(s.cell(0, 0).ch, 'x');
        assert_eq!(s.cell(1, 0).ch, ' ');
        assert!(!s.cell(1, 0).unknown);
        assert!(!s.cell(1, 0).cont);
    }

    #[test]
    fn overwriting_a_wide_tail_with_another_wide_char() {
        let mut s = Screen::unknown(10, 1);
        s.print(0, 0, "日", style());
        s.put(1, 0, '本', style());
        assert_eq!(s.cell(0, 0).ch, ' ');
        assert!(!s.cell(0, 0).unknown);
        assert_eq!(s.cell(1, 0).ch, '本');
        assert!(s.cell(2, 0).cont);
    }

    #[test]
    fn overwriting_a_neighbouring_wide_lead_blanks_its_tail() {
        let mut s = Screen::unknown(10, 1);
        s.print(0, 0, "日", style());
        s.print(2, 0, "本", style());
        s.put(1, 0, '本', style());
        assert_eq!(s.cell(0, 0).ch, ' ');
        assert!(!s.cell(0, 0).unknown);
        assert_eq!(s.cell(1, 0).ch, '本');
        assert!(s.cell(2, 0).cont);
        assert_eq!(s.cell(3, 0).ch, ' ');
        assert!(!s.cell(3, 0).unknown);
    }

    #[test]
    fn diff_only_reports_changes() {
        let prev = Screen::blank(20, 2, style());
        let mut next = prev.clone();
        next.print(3, 1, "hi", style());
        let updates = diff(&prev, &next);
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].x, 3);
        assert_eq!(updates[0].y, 1);
        assert_eq!(updates[0].text, "hi");
    }

    #[test]
    fn diff_skips_unknown_destination() {
        let prev = Screen::blank(10, 1, style());
        let next = Screen::unknown(10, 1);
        assert!(diff(&prev, &next).is_empty());
    }

    #[test]
    fn diff_merges_adjacent_same_style() {
        let prev = Screen::blank(20, 1, style());
        let mut next = prev.clone();
        next.print(2, 0, "abcdef", style());
        let updates = diff(&prev, &next);
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].text, "abcdef");
    }

    #[test]
    fn restore_unknown_cells_become_blank() {
        let base = Screen::unknown(10, 2);
        let mut current = Screen::blank(10, 2, style());
        current.print(1, 0, "box", style());
        let fallback = Style::new(Color::Indexed(0), Color::Indexed(2));
        current.restore_region(&base, Rect::new(0, 0, 10, 1), fallback);
        assert_eq!(current.cell(1, 0).ch, ' ');
        assert_eq!(current.cell(1, 0).bg, fallback.bg);
    }

    #[test]
    fn fill_and_clip() {
        let mut s = Screen::blank(10, 5, style());
        s.fill(Rect::new(8, 4, 10, 10), '#', style());
        assert_eq!(s.cell(9, 4).ch, '#');
        assert_eq!(s.cell(0, 0).ch, ' ');
        assert!(s.any_known(Rect::new(0, 0, 10, 5)));
    }

    #[test]
    fn recolor_preserves_characters() {
        let mut s = Screen::blank(10, 2, style());
        s.print(0, 0, "hi", style());
        let recolor = Style::new(Color::Indexed(4), Color::Indexed(2));
        s.recolor(Rect::new(0, 0, 2, 1), recolor);
        assert_eq!(s.cell(0, 0).ch, 'h');
        assert_eq!(s.cell(1, 0).ch, 'i');
        assert_eq!(s.cell(0, 0).fg, Color::Indexed(4));
        assert_eq!(s.cell(1, 0).bg, Color::Indexed(2));
    }

    #[test]
    fn moving_a_box_erases_the_previous_frame() {
        use crate::paint::draw_frame;
        use wbat64_core::Frame;

        let fallback = Style::new(Color::Indexed(0), Color::Indexed(2));
        let base = Screen::unknown(40, 15);
        let old = Rect::new(2, 1, 10, 4);
        let new = Rect::new(12, 6, 10, 4);

        let mut previous = base.clone();
        draw_frame(&mut previous, old, Frame::single(), style());

        let mut next = base.clone();
        next.reset_to(&base, Some(old), fallback);
        draw_frame(&mut next, new, Frame::single(), style());

        // The old corner is now a known blank, so the diff will paint it.
        assert!(!next.cell(old.x, old.y).unknown);
        assert_eq!(next.cell(old.x, old.y).ch, ' ');
        assert_eq!(next.cell(old.x, old.y).bg, fallback.bg);

        let updates = diff(&previous, &next);
        assert!(
            updates.iter().any(|update| update.y == old.y
                && update.x <= old.x
                && old.x < update.x + update.text.chars().count() as u16),
            "old frame was not repainted: {updates:?}"
        );
        // Untouched cells stay unknown and are therefore never painted.
        assert!(next.cell(39, 14).unknown);
        assert!(next.cell(old.x, old.bottom() + 5).unknown);
    }
}
