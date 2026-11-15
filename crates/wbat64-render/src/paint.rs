//! Higher level painters: frame, shadow, title, close button, divider.

use crate::cell::Screen;
use wbat64_core::{str_width, truncate, Frame, Rect, Style};

/// Paint a box frame and fill the whole box with the box style.
pub fn draw_frame(screen: &mut Screen, outer: Rect, frame: Frame, style: Style) {
    if outer.w < 2 || outer.h < 2 {
        return;
    }
    screen.clear_rect(outer, style);
    let right = outer.right() - 1;
    let bottom = outer.bottom() - 1;
    for x in outer.x + 1..right {
        screen.put(x, outer.y, frame.top, style);
        screen.put(x, bottom, frame.bottom, style);
    }
    for y in outer.y + 1..bottom {
        screen.put(outer.x, y, frame.left, style);
        screen.put(right, y, frame.right, style);
    }
    screen.put(outer.x, outer.y, frame.top_left, style);
    screen.put(right, outer.y, frame.top_right, style);
    screen.put(outer.x, bottom, frame.bottom_left, style);
    screen.put(right, bottom, frame.bottom_right, style);
}

/// Paint the drop shadow: one column on the right, one row at the bottom.
pub fn draw_shadow(screen: &mut Screen, outer: Rect, color: wbat64_core::Color) {
    let style = Style::new(color, color);
    let right = outer.right();
    let bottom = outer.bottom();
    for y in outer.y.saturating_add(1)..=bottom {
        screen.put(right, y, ' ', style);
    }
    for x in outer.x.saturating_add(1)..=right {
        screen.put(x, bottom, ' ', style);
    }
}

/// Draw a centred title in the top border.
pub fn draw_title(screen: &mut Screen, outer: Rect, title: &str, style: Style) {
    if outer.w < 4 || title.is_empty() {
        return;
    }
    let available = (outer.w - 2) as usize;
    let padded = format!(" {title} ");
    let text = truncate(&padded, available);
    let width = str_width(&text);
    let start = outer.x + 1 + ((available.saturating_sub(width)) / 2) as u16;
    screen.print(start, outer.y, &text, style);
}

/// Draw the close button symbol in the top-right corner.
pub fn draw_close_button(screen: &mut Screen, outer: Rect, style: Style, symbol: char) {
    if outer.w < 8 {
        return;
    }
    screen.put(outer.right() - 2, outer.y, symbol, style);
}

/// Draw a divider line across the content rectangle.
///
/// WBAT always uses U+2500 here, independent of `BoxFrame`; only the
/// foreground of the `Divider` colour is used (the background comes from the
/// box).
pub fn draw_divider(screen: &mut Screen, content: Rect, y: u16, style: Style) {
    if y < content.y || y >= content.bottom() {
        return;
    }
    for x in content.x..content.right() {
        screen.put(x, y, '─', style);
    }
}

/// Greedy word wrap; words longer than `width` are hard-broken.
pub fn word_wrap(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        if paragraph.is_empty() {
            lines.push(String::new());
            continue;
        }
        let mut current = String::new();
        for word in paragraph.split_whitespace() {
            let mut word = word.to_string();
            loop {
                let candidate = if current.is_empty() {
                    word.clone()
                } else {
                    format!("{current} {word}")
                };
                if str_width(&candidate) <= width {
                    current = candidate;
                    break;
                }
                if current.is_empty() {
                    // Break the word itself.
                    let head = truncate(&word, width);
                    let used = str_width(&head);
                    if used == 0 {
                        word = word.chars().skip(1).collect();
                        continue;
                    }
                    lines.push(head.clone());
                    word = word[head.len()..].to_string();
                    if word.is_empty() {
                        break;
                    }
                } else {
                    lines.push(std::mem::take(&mut current));
                }
            }
        }
        if !current.is_empty() {
            lines.push(current);
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use wbat64_core::Color;

    fn style() -> Style {
        Style::new(Color::Indexed(0), Color::Indexed(7))
    }

    #[test]
    fn frame_draws_all_borders() {
        let mut s = Screen::blank(20, 10, style());
        draw_frame(&mut s, Rect::new(2, 1, 10, 5), Frame::single(), style());
        assert_eq!(s.cell(2, 1).ch, '┌');
        assert_eq!(s.cell(11, 1).ch, '┐');
        assert_eq!(s.cell(2, 5).ch, '└');
        assert_eq!(s.cell(11, 5).ch, '┘');
        assert_eq!(s.cell(2, 3).ch, '│');
        assert_eq!(s.cell(6, 1).ch, '─');
        assert_eq!(s.cell(5, 3).ch, ' ');
    }

    #[test]
    fn shadow_is_offset() {
        let mut s = Screen::blank(20, 10, style());
        draw_shadow(&mut s, Rect::new(2, 1, 4, 3), Color::Indexed(8));
        assert_eq!(s.cell(6, 2).bg, Color::Indexed(8));
        assert_eq!(s.cell(3, 4).bg, Color::Indexed(8));
        assert_eq!(s.cell(2, 1).bg, Color::Indexed(7));
    }

    #[test]
    fn title_is_centred() {
        let mut s = Screen::blank(30, 5, style());
        let outer = Rect::new(0, 0, 24, 3);
        draw_title(&mut s, outer, "Hi", style());
        let row: String = (0..24).map(|x| s.cell(x, 0).ch).collect();
        assert!(row.contains(" Hi "));
        let start = row.find('H').unwrap();
        assert!((8..=11).contains(&start), "start {start}");
    }

    #[test]
    fn divider_is_a_line_regardless_of_the_frame() {
        let mut s = Screen::blank(20, 5, style());
        let divider_style = Style::new(Color::Indexed(3), Color::Indexed(7));
        draw_divider(&mut s, Rect::new(2, 1, 10, 3), 2, divider_style);
        assert_eq!(s.cell(2, 2).ch, '─');
        assert_eq!(s.cell(11, 2).ch, '─');
        assert_eq!(s.cell(2, 2).fg, Color::Indexed(3));
        assert_eq!(s.cell(2, 2).bg, Color::Indexed(7));
    }

    #[test]
    fn close_button_at_top_right() {
        let mut s = Screen::blank(30, 5, style());
        let outer = Rect::new(0, 0, 16, 3);
        draw_close_button(&mut s, outer, style(), 'X');
        assert_eq!(s.cell(14, 0).ch, 'X');
        assert_eq!(s.cell(14, 0).fg, style().fg);
    }

    #[test]
    fn wrapping() {
        assert_eq!(word_wrap("a b c", 3), vec!["a b", "c"]);
        assert_eq!(word_wrap("abcdef", 3), vec!["abc", "def"]);
        assert_eq!(word_wrap("", 5), vec![""]);
    }
}
