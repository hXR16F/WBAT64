//! Screen geometry: sizes, positions and rectangles.
//!
//! Public positions are 1-based `line,column` (as in WBAT); rectangles used by
//! the renderer are 0-based cell coordinates.

/// Terminal size in cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Size {
    pub cols: u16,
    pub rows: u16,
}

impl Size {
    pub const fn new(cols: u16, rows: u16) -> Self {
        Size { cols, rows }
    }

    pub fn valid(&self) -> bool {
        self.cols > 0 && self.rows > 0
    }
}

impl Default for Size {
    fn default() -> Self {
        Size::new(80, 25)
    }
}

/// A 1-based screen position; `line == 0` or `col == 0` means "not specified".
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Pos {
    pub line: u16,
    pub col: u16,
}

impl Pos {
    pub const fn new(line: u16, col: u16) -> Self {
        Pos { line, col }
    }

    pub fn is_empty(self) -> bool {
        self.line == 0 && self.col == 0
    }
}

/// A 0-based cell rectangle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Rect {
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
}

impl Rect {
    pub const fn new(x: u16, y: u16, w: u16, h: u16) -> Self {
        Rect { x, y, w, h }
    }

    pub const fn empty() -> Self {
        Rect {
            x: 0,
            y: 0,
            w: 0,
            h: 0,
        }
    }

    pub fn right(&self) -> u16 {
        self.x.saturating_add(self.w)
    }

    pub fn bottom(&self) -> u16 {
        self.y.saturating_add(self.h)
    }

    pub fn is_empty(&self) -> bool {
        self.w == 0 || self.h == 0
    }

    pub fn contains(&self, x: u16, y: u16) -> bool {
        x >= self.x && y >= self.y && x < self.right() && y < self.bottom()
    }

    pub fn intersect(&self, other: Rect) -> Rect {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let right = self.right().min(other.right());
        let bottom = self.bottom().min(other.bottom());
        if right <= x || bottom <= y {
            Rect::empty()
        } else {
            Rect::new(x, y, right - x, bottom - y)
        }
    }

    /// Grow or shrink by `n` cells on every side, saturating at zero.
    pub fn expand(&self, n: i32) -> Rect {
        let x = (self.x as i32).saturating_sub(n).max(0) as u16;
        let y = (self.y as i32).saturating_sub(n).max(0) as u16;
        let right = (self.right() as i32).saturating_add(n).max(0) as u16;
        let bottom = (self.bottom() as i32).saturating_add(n).max(0) as u16;
        Rect::new(x, y, right.saturating_sub(x), bottom.saturating_sub(y))
    }

    pub fn translated(&self, dx: i32, dy: i32) -> Rect {
        Rect::new(
            (self.x as i32 + dx).max(0) as u16,
            (self.y as i32 + dy).max(0) as u16,
            self.w,
            self.h,
        )
    }
}

/// Parse `line,column` (either part optional) and return a 1-based position.
pub fn parse_pos(s: &str) -> Option<Pos> {
    let t = s.trim();
    if t.is_empty() {
        return None;
    }
    let (line, col) = match t.split_once(',') {
        Some((l, c)) => (l.trim(), c.trim()),
        None => (t, ""),
    };
    let line = if line.is_empty() {
        0
    } else {
        line.parse::<u16>().ok()?
    };
    let col = if col.is_empty() {
        0
    } else {
        col.parse::<u16>().ok()?
    };
    if line == 0 && col == 0 {
        return None;
    }
    Some(Pos::new(line, col))
}

/// Parse a `(height,width)` fill size.
pub fn parse_size(s: &str) -> Option<(u16, u16)> {
    let t = s.trim().trim_start_matches('(').trim_end_matches(')');
    let (h, w) = t.split_once(',')?;
    let h = h.trim().parse::<u16>().ok()?;
    let w = w.trim().parse::<u16>().ok()?;
    if h == 0 || w == 0 {
        return None;
    }
    Some((h, w))
}

/// True when a token looks like a `line,column` (or bare `line`) position.
pub fn looks_like_pos(token: &str) -> bool {
    let t = token.trim();
    if t.is_empty() {
        return false;
    }
    let (line, col) = match t.split_once(',') {
        Some((l, c)) => (l.trim(), c.trim()),
        None => (t, ""),
    };
    (!line.is_empty() && line.chars().all(|c| c.is_ascii_digit()))
        && (col.is_empty() || col.chars().all(|c| c.is_ascii_digit()))
}

/// Resolve a requested position into a valid upper-left corner so that a box of
/// `outer_w` x `outer_h` fits on `screen`. Missing parts are centred.
pub fn resolve_pos(requested: Option<Pos>, outer_w: u16, outer_h: u16, screen: Size) -> Pos {
    let cols = screen.cols.max(1);
    let rows = screen.rows.max(1);
    let outer_w = outer_w.min(cols);
    let outer_h = outer_h.min(rows);
    let req = requested.unwrap_or_default();

    let line = if req.line == 0 {
        ((rows.saturating_sub(outer_h)) / 2) + 1
    } else {
        req.line
    };
    let col = if req.col == 0 {
        ((cols.saturating_sub(outer_w)) / 2) + 1
    } else {
        req.col
    };

    let max_line = rows.saturating_sub(outer_h).saturating_add(1).max(1);
    let max_col = cols.saturating_sub(outer_w).saturating_add(1).max(1);
    Pos::new(line.clamp(1, max_line), col.clamp(1, max_col))
}

/// Convert a 1-based position and size into a 0-based rectangle.
pub fn pos_to_rect(pos: Pos, w: u16, h: u16) -> Rect {
    Rect::new(pos.col.saturating_sub(1), pos.line.saturating_sub(1), w, h)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positions() {
        assert_eq!(parse_pos("21,40"), Some(Pos::new(21, 40)));
        assert_eq!(parse_pos("5"), Some(Pos::new(5, 0)));
        assert_eq!(parse_pos(""), None);
        assert_eq!(parse_pos("x,2"), None);
    }

    #[test]
    fn sizes() {
        assert_eq!(parse_size("(9,28)"), Some((9, 28)));
        assert_eq!(parse_size("9,28"), Some((9, 28)));
        assert_eq!(parse_size("9"), None);
    }

    #[test]
    fn centering_and_correction() {
        let screen = Size::new(80, 25);
        let p = resolve_pos(None, 20, 5, screen);
        assert_eq!(p, Pos::new(11, 31));
        // Only a line: horizontally centred.
        let p = resolve_pos(Some(Pos::new(1, 0)), 20, 5, screen);
        assert_eq!(p, Pos::new(1, 31));
        // Off-screen is corrected so the box fits.
        let p = resolve_pos(Some(Pos::new(24, 78)), 20, 5, screen);
        assert_eq!(p, Pos::new(21, 61));
    }

    #[test]
    fn rect_math() {
        let r = Rect::new(2, 3, 4, 5);
        assert!(r.contains(2, 3));
        assert!(!r.contains(6, 3));
        assert_eq!(r.intersect(Rect::new(0, 0, 10, 4)), Rect::new(2, 3, 4, 1));
        assert_eq!(r.expand(1), Rect::new(1, 2, 6, 7));
    }
}
