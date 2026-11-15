//! The scrolling list widget, with type-ahead and an optional live filter.

use std::time::Duration;

use wbat64_core::{
    resolve_pos, str_width, truncate, Color, Config, Pos, Rect, Result, Size, Style,
};
use wbat64_render::{draw_frame, draw_shadow, draw_title, Screen};

use crate::event::{Event, KeyCode, MouseButton, MouseKind};
use crate::session::Session;

/// Options for a list dialog.
#[derive(Clone, Debug)]
pub struct ListOptions {
    pub title: Option<String>,
    pub pos: Option<Pos>,
    pub colors: Option<Style>,
    pub width: Option<u16>,
    pub height: Option<u16>,
    pub initial: Option<usize>,
    pub multi: bool,
    pub max_width: Option<usize>,
    pub fix: bool,
    pub mouse: bool,
    /// Ticks of inactivity before type-ahead resets.
    pub hot_ticks: u64,
    /// Type-to-filter instead of type-ahead.
    pub filter: bool,
}

impl Default for ListOptions {
    fn default() -> Self {
        ListOptions {
            title: None,
            pos: None,
            colors: None,
            width: None,
            height: None,
            initial: None,
            multi: false,
            max_width: None,
            fix: false,
            mouse: true,
            hot_ticks: 10,
            filter: false,
        }
    }
}

/// How a list closed.
#[derive(Clone, Debug, Default)]
pub struct ListOutcome {
    /// 1-based selected item, or `None` when cancelled.
    pub index: Option<usize>,
    pub item: String,
    /// All selected lines (for `--multi`).
    pub selected: Vec<String>,
    pub escaped: bool,
}

struct State {
    /// Index into `visible`.
    sel: usize,
    /// Index into `visible` of the first drawn row.
    offset: usize,
    /// Marked items, indexed by item index.
    marked: Vec<bool>,
    typed: String,
    idle_ticks: u64,
    outcome: Option<ListOutcome>,
    filter: String,
    /// Item indexes currently shown.
    visible: Vec<usize>,
}

impl State {
    fn new(len: usize, initial: Option<usize>) -> State {
        let visible: Vec<usize> = (0..len).collect();
        let sel = initial
            .map(|i| i.saturating_sub(1).min(len.saturating_sub(1)))
            .unwrap_or(0);
        State {
            sel,
            offset: 0,
            marked: vec![false; len],
            typed: String::new(),
            idle_ticks: 0,
            outcome: None,
            filter: String::new(),
            visible,
        }
    }

    fn rows(&self) -> usize {
        self.visible.len()
    }

    fn item_at(&self, visible_pos: usize) -> Option<usize> {
        self.visible.get(visible_pos).copied()
    }

    fn update_visible(&mut self, items: &[String]) {
        let needle = self.filter.to_lowercase();
        self.visible = items
            .iter()
            .enumerate()
            .filter(|(_, item)| needle.is_empty() || item.to_lowercase().contains(&needle))
            .map(|(index, _)| index)
            .collect();
        if self.visible.is_empty() {
            self.sel = 0;
            self.offset = 0;
        } else {
            self.sel = self.sel.min(self.visible.len() - 1);
            self.offset = self.offset.min(self.sel);
        }
    }

    fn ensure_visible(&mut self, rows: usize) {
        if rows == 0 {
            return;
        }
        if self.sel < self.offset {
            self.offset = self.sel;
        } else if self.sel >= self.offset + rows {
            self.offset = self.sel + 1 - rows;
        }
    }

    fn move_by(&mut self, delta: isize) {
        let len = self.rows();
        if len == 0 {
            return;
        }
        let next = self.sel as isize + delta;
        self.sel = next.clamp(0, len as isize - 1) as usize;
    }

    /// Item indexes that would be returned: marked ones, else the current row.
    fn selected_indices(&self) -> Vec<usize> {
        let marked: Vec<usize> = self
            .visible
            .iter()
            .copied()
            .filter(|i| self.marked.get(*i).copied().unwrap_or(false))
            .collect();
        if marked.is_empty() {
            self.item_at(self.sel).into_iter().collect()
        } else {
            marked
        }
    }

    #[cfg(test)]
    fn selected_lines(&self, items: &[String]) -> Vec<String> {
        self.selected_indices()
            .into_iter()
            .filter_map(|i| items.get(i).cloned())
            .collect()
    }
}

fn list_geometry(items: &[String], opts: &ListOptions, screen: Size) -> (Rect, Rect) {
    let longest = items.iter().map(|i| str_width(i)).max().unwrap_or(0);
    let content_w = opts
        .width
        .map(|w| w.saturating_sub(2) as usize)
        .unwrap_or_else(|| (longest + 2).max(20));
    let max_w = opts
        .max_width
        .map(|w| w as u16)
        .unwrap_or(screen.cols)
        .min(screen.cols)
        .max(4);
    let outer_w = ((content_w + 2).min(max_w as usize) as u16).max(4);

    let wanted_h = opts.height.unwrap_or_else(|| {
        let visible = items.len().clamp(3, 12);
        (visible + 2) as u16
    });
    let outer_h = wanted_h.min(screen.rows).max(3);
    let pos = resolve_pos(opts.pos, outer_w, outer_h, screen);
    let outer = Rect::new(pos.col - 1, pos.line - 1, outer_w, outer_h);
    let content = Rect::new(outer.x + 1, outer.y + 1, outer_w - 2, outer_h - 2);
    (outer, content)
}

fn draw_list(
    screen: &mut Screen,
    config: &Config,
    items: &[String],
    opts: &ListOptions,
    outer: Rect,
    content: Rect,
    state: &mut State,
) {
    let theme = &config.theme;
    let box_style = opts.colors.unwrap_or(theme.list);
    if let Some(shadow) = theme.shadow {
        draw_shadow(screen, outer, shadow);
    }
    draw_frame(screen, outer, config.frame, box_style);
    let title = if opts.filter {
        let base = opts.title.clone().unwrap_or_default();
        Some(format!("{base} [{}]", state.filter))
    } else {
        opts.title.clone()
    };
    if let Some(title) = title {
        draw_title(
            screen,
            outer,
            &title,
            Style::new(theme.title.fg, box_style.bg),
        );
    }
    let rows = content.h as usize;
    state.ensure_visible(rows);
    for row in 0..rows {
        let visible_pos = state.offset + row;
        let Some(index) = state.item_at(visible_pos) else {
            break;
        };
        let Some(item) = items.get(index) else {
            break;
        };
        let y = content.y + row as u16;
        let selected = visible_pos == state.sel;
        let style = if selected { theme.bar } else { box_style };
        screen.clear_rect(Rect::new(content.x, y, content.w, 1), style);
        let mut text = truncate(item, content.w as usize);
        if opts.multi {
            let mark = if state.marked.get(index).copied().unwrap_or(false) {
                '*'
            } else {
                ' '
            };
            text = truncate(&format!("{mark} {text}"), content.w as usize);
        }
        screen.print(content.x, y, &text, style);
    }
    if state.visible.is_empty() && opts.filter {
        let hint = if state.filter.is_empty() {
            "(type to filter)"
        } else {
            "(no matches)"
        };
        screen.print(content.x, content.y, hint, box_style);
    }
}

fn confirm(state: &mut State, items: &[String]) {
    let chosen = state.selected_indices();
    let Some(&first) = chosen.first() else {
        return;
    };
    let Some(item) = items.get(first) else {
        return;
    };
    state.outcome = Some(ListOutcome {
        index: Some(first + 1),
        item: item.clone(),
        selected: chosen
            .iter()
            .filter_map(|i| items.get(*i).cloned())
            .collect(),
        escaped: false,
    });
}

fn escape(state: &mut State) {
    state.outcome = Some(ListOutcome {
        escaped: true,
        ..ListOutcome::default()
    });
}

/// Run a list until the user picks an item or cancels.
pub fn run_list(
    session: &mut Session,
    config: &Config,
    items: &[String],
    opts: &ListOptions,
) -> Result<ListOutcome> {
    let mut base = session.screen.clone();
    let mut state = State::new(items.len(), opts.initial);
    let mut last_outer: Option<Rect> = None;
    if opts.filter {
        state.update_visible(items);
    }
    let (_, content) = list_geometry(items, opts, session.size);
    state.ensure_visible(content.h as usize);

    let timeout = Duration::from_millis(100);
    loop {
        let (outer, content) = list_geometry(items, opts, session.size);
        session.screen.reset_to(&base, last_outer, Style::default());
        draw_list(
            &mut session.screen,
            config,
            items,
            opts,
            outer,
            content,
            &mut state,
        );
        session.present()?;
        last_outer = Some(outer);

        let event = session.next_event(timeout)?;
        match event {
            Event::Tick => {
                state.idle_ticks += 1;
                if state.idle_ticks > opts.hot_ticks {
                    state.typed.clear();
                }
            }
            Event::Resize(_, _) => {
                session.refresh_size()?;
                base.resize(session.size.cols, session.size.rows);
            }
            Event::Key(key) => {
                state.idle_ticks = 0;
                match key.code {
                    KeyCode::Esc => {
                        if opts.filter && !state.filter.is_empty() {
                            state.filter.clear();
                            state.update_visible(items);
                        } else {
                            escape(&mut state);
                        }
                    }
                    KeyCode::Enter => confirm(&mut state, items),
                    KeyCode::Up => state.move_by(-1),
                    KeyCode::Down => state.move_by(1),
                    KeyCode::PageUp => state.move_by(-(content.h.max(1) as isize)),
                    KeyCode::PageDown => state.move_by(content.h.max(1) as isize),
                    KeyCode::Home => state.sel = 0,
                    KeyCode::End => state.sel = state.rows().saturating_sub(1),
                    KeyCode::Backspace if opts.filter => {
                        state.filter.pop();
                        state.update_visible(items);
                    }
                    KeyCode::Char(' ') if opts.multi && !opts.filter => {
                        if let Some(index) = state.item_at(state.sel) {
                            if let Some(mark) = state.marked.get_mut(index) {
                                *mark = !*mark;
                            }
                            state.move_by(1);
                        }
                    }
                    KeyCode::Char(c) if opts.filter && !key.mods.ctrl && !key.mods.alt => {
                        state.filter.push(c);
                        state.sel = 0;
                        state.offset = 0;
                        state.update_visible(items);
                    }
                    KeyCode::Char(c) => {
                        state.typed.push(c);
                        let needle = state.typed.to_lowercase();
                        let len = state.rows();
                        if len > 0 {
                            for step in 1..=len {
                                let pos = (state.sel + step) % len;
                                if let Some(index) = state.item_at(pos) {
                                    if items[index].to_lowercase().starts_with(&needle) {
                                        state.sel = pos;
                                        break;
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }
                state.ensure_visible(content.h as usize);
            }
            Event::Mouse(kind) => match kind {
                MouseKind::Down {
                    x,
                    y,
                    button: MouseButton::Left,
                } => {
                    if content.contains(x, y) {
                        let row = (y - content.y) as usize;
                        let visible_pos = state.offset + row;
                        if visible_pos == state.sel && !opts.multi {
                            confirm(&mut state, items);
                        } else if state.item_at(visible_pos).is_some() {
                            state.sel = visible_pos;
                        }
                    }
                }
                MouseKind::Down {
                    x,
                    y,
                    button: MouseButton::Right,
                } => {
                    let (outer, _) = list_geometry(items, opts, session.size);
                    if !outer.contains(x, y) && config.right_esc {
                        escape(&mut state);
                    }
                }
                MouseKind::Wheel { up, .. } => state.move_by(if up { -3 } else { 3 }),
                _ => {}
            },
        }

        if state.outcome.is_some() {
            break;
        }
    }

    let outcome = state.outcome.take().unwrap_or_default();
    let (computed_outer, _) = list_geometry(items, opts, session.size);
    let outer = last_outer.unwrap_or(computed_outer);
    if !opts.fix && !session.keep_frame() {
        let area = Rect::new(
            outer.x,
            outer.y,
            outer.w.saturating_add(1),
            outer.h.saturating_add(1),
        );
        session.restore_area(&base, area, Style::default());
    }
    session.present()?;
    session.set_cursor(outer.x, outer.y)?;
    Ok(outcome)
}

/// Render the list into a plain string (used by tests and diagnostics).
pub fn render_list_text(
    items: &[String],
    opts: &ListOptions,
    size: Size,
    config: &Config,
    selected: usize,
) -> String {
    let mut screen = Screen::blank(
        size.cols,
        size.rows,
        Style::new(Color::Default, Color::Default),
    );
    let (outer, content) = list_geometry(items, opts, size);
    let mut state = State::new(items.len(), Some(selected + 1));
    state.update_visible(items);
    draw_list(&mut screen, config, items, opts, outer, content, &mut state);
    screen.to_text()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items() -> Vec<String> {
        vec![
            "alpha".into(),
            "beta".into(),
            "gamma".into(),
            "alphabet".into(),
        ]
    }

    #[test]
    fn filtering_narrows_the_list() {
        let mut state = State::new(4, None);
        state.filter = "alph".into();
        state.update_visible(&items());
        assert_eq!(state.visible, vec![0, 3]);
        assert_eq!(state.sel, 0);
        state.filter = "zzz".into();
        state.update_visible(&items());
        assert!(state.visible.is_empty());
    }

    #[test]
    fn marked_items_win_over_the_current_row() {
        let mut state = State::new(4, None);
        state.marked[1] = true;
        state.marked[2] = true;
        assert_eq!(state.selected_indices(), vec![1, 2]);
        assert_eq!(state.selected_lines(&items()), vec!["beta", "gamma"]);
    }

    #[test]
    fn selection_clamps_after_filtering() {
        let mut state = State::new(4, Some(4));
        state.sel = 3;
        state.filter = "beta".into();
        state.update_visible(&items());
        assert_eq!(state.sel, 0);
        assert_eq!(state.item_at(0), Some(1));
    }

    #[test]
    fn moving_cannot_leave_the_list() {
        let mut state = State::new(3, None);
        state.move_by(-5);
        assert_eq!(state.sel, 0);
        state.move_by(10);
        assert_eq!(state.sel, 2);
    }
}
