//! The dialog box: layout, rendering and the event loop.
//!
//! A dialog draws a frame around a layout unit, places its controls and returns
//! an [`Outcome`]. It supports buttons, hotkeys, inputs, checkboxes, radios, a
//! mouse (click, drag by title, right-click outside), a countdown timer and
//! terminal resizes.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use wbat64_core::{
    interp, resolve_pos, str_width, Control, ControlKind, Error, Frame, LineKind, Pos, Rect,
    Result, Size, Span, Style, Theme, Unit,
};
use wbat64_render::{draw_close_button, draw_divider, draw_frame, draw_shadow, draw_title, Screen};

use crate::event::{Event, Key, KeyCode, MouseButton, MouseKind};
use crate::input::InputState;
use crate::session::Session;

/// Options for one dialog.
#[derive(Clone, Debug)]
pub struct DialogOptions {
    pub title: Option<String>,
    pub pos: Option<Pos>,
    pub colors: Option<Style>,
    pub timeout: Option<u32>,
    pub bar: Option<usize>,
    pub fix: bool,
    pub mouse: bool,
    pub close: bool,
    pub initial: HashMap<String, String>,
    pub max_width: Option<usize>,
    pub strict_width: bool,
    pub menu: bool,
}

impl Default for DialogOptions {
    fn default() -> Self {
        DialogOptions {
            title: None,
            pos: None,
            colors: None,
            timeout: None,
            bar: None,
            fix: false,
            mouse: true,
            close: false,
            initial: HashMap::new(),
            max_width: None,
            strict_width: false,
            menu: false,
        }
    }
}

/// How a dialog ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// A button was activated (1-based number).
    Button { index: usize, name: String },
    /// ESC, `[x]`, right-click outside or an escape button.
    Escape,
    /// The countdown expired; `index` is the button it returns, if any.
    Timeout {
        index: Option<usize>,
        name: Option<String>,
    },
}

impl Outcome {
    pub fn is_escape(&self) -> bool {
        matches!(self, Outcome::Escape | Outcome::Timeout { index: None, .. })
    }
}

/// Dialog results.
#[derive(Clone, Debug)]
pub struct DialogResult {
    pub outcome: Outcome,
    pub button_index: Option<usize>,
    pub button_name: Option<String>,
    pub inputs: Vec<(String, String)>,
    pub checkboxes: Vec<bool>,
    pub radio: Option<usize>,
}

struct Placed {
    control: Control,
    rect: Rect,
    kind: ControlKind,
    /// Button number - 1, or the index within the control's own kind.
    index: usize,
}

struct Layout {
    outer: Rect,
    content: Rect,
    placed: Vec<Placed>,
    close: Option<Rect>,
}

fn title_of(unit: &Unit, opts: &DialogOptions) -> Option<String> {
    opts.title.clone().or_else(|| unit.title.clone())
}

fn box_widths(unit: &Unit, opts: &DialogOptions, screen: Size) -> Result<(u16, u16)> {
    let (mut content_w, content_h) = unit.content_size();
    if let Some(title) = title_of(unit, opts) {
        content_w = content_w.max(str_width(&title) + 2);
    }
    if opts.close || unit.close {
        content_w = content_w.max(6);
    }
    content_w = content_w.max(1);

    let hard_max = screen.cols.max(4);
    let soft_max = opts
        .max_width
        .map(|w| (w as u16).min(hard_max))
        .unwrap_or(hard_max);
    let mut outer_w = (content_w + 2) as u16;
    if outer_w > soft_max {
        if opts.strict_width && opts.max_width.is_some() {
            return Err(Error::msg(format!(
                "box is {outer_w} columns wide (maximum {soft_max})"
            )));
        }
        outer_w = soft_max;
    }
    let mut outer_h = (content_h + 2) as u16;
    if outer_h > screen.rows {
        outer_h = screen.rows.max(3);
    }
    Ok((outer_w, outer_h))
}

fn build_layout(unit: &Unit, screen: Size, opts: &DialogOptions, state: &State) -> Result<Layout> {
    let (outer_w, outer_h) = box_widths(unit, opts, screen)?;
    let requested = state.pos.or(opts.pos);
    let pos = resolve_pos(requested, outer_w, outer_h, screen);
    let outer = Rect::new(pos.col - 1, pos.line - 1, outer_w, outer_h);
    let content = Rect::new(outer.x + 1, outer.y + 1, outer_w - 2, outer_h - 2);

    let mut placed = Vec::new();
    let (mut buttons, mut inputs, mut checks, mut radios) = (0usize, 0usize, 0usize, 0usize);
    for (i, line) in unit.lines.iter().enumerate() {
        let y = content.y + i as u16;
        if y >= content.bottom() {
            break;
        }
        let line_width = line.width();
        let offset = if unit.center && (content.w as usize) > line_width {
            ((content.w as usize - line_width) / 2) as u16
        } else {
            0
        };
        let mut x = content.x + offset;
        for span in &line.spans {
            match span {
                Span::Control(control) => {
                    let width = control.display_width().max(1) as u16;
                    let index = match control.kind() {
                        ControlKind::Button | ControlKind::EscapeButton => {
                            let n = buttons;
                            buttons += 1;
                            n
                        }
                        ControlKind::Input => {
                            let n = inputs;
                            inputs += 1;
                            n
                        }
                        ControlKind::Checkbox => {
                            let n = checks;
                            checks += 1;
                            n
                        }
                        ControlKind::Radio => {
                            let n = radios;
                            radios += 1;
                            n
                        }
                    };
                    // Clip to the content rectangle so a line wider than the
                    // box cannot paint over the frame or the shadow.
                    if x < content.right() {
                        let visible = width.min(content.right() - x);
                        placed.push(Placed {
                            control: control.clone(),
                            rect: Rect::new(x, y, visible, 1),
                            kind: control.kind(),
                            index,
                        });
                    }
                    x = x.saturating_add(width);
                }
                other => x = x.saturating_add(other.width() as u16),
            }
        }
    }

    let close = if (opts.close || unit.close) && outer.w >= 8 {
        Some(Rect::new(outer.right() - 2, outer.y, 1, 1))
    } else {
        None
    };

    Ok(Layout {
        outer,
        content,
        placed,
        close,
    })
}

struct Timer {
    deadline: Instant,
    remaining: u32,
    cancelled: bool,
    button: Option<(usize, String)>,
}

struct Drag {
    start_x: u16,
    start_y: u16,
    origin: Pos,
}

struct State {
    pos: Option<Pos>,
    focus: Option<usize>,
    /// Button under the pointer (hover highlight).
    hover: Option<usize>,
    hover_close: bool,
    inputs: Vec<InputState>,
    checks: Vec<bool>,
    radio: Option<usize>,
    timer: Option<Timer>,
    drag: Option<Drag>,
    outcome: Option<Outcome>,
}

impl State {
    fn new(unit: &Unit, opts: &DialogOptions, _frame: &Frame) -> State {
        let mut inputs = Vec::new();
        let mut checks = Vec::new();
        let mut radio = None;
        for control in unit.controls() {
            match control {
                Control::Input { .. } => {
                    let var = control.var_name().unwrap_or("").to_string();
                    let initial = opts.initial.get(&var).cloned().unwrap_or_default();
                    inputs.push(InputState::new(control, &initial));
                }
                Control::Checkbox { .. } => {
                    let name = format!("WCB{}", checks.len() + 1);
                    let on = opts
                        .initial
                        .get(&name)
                        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                        .unwrap_or(false);
                    checks.push(on);
                }
                Control::Radio { .. } => {
                    if radio.is_none() {
                        radio = opts
                            .initial
                            .get("WRB")
                            .and_then(|v| v.trim().parse::<usize>().ok())
                            .filter(|v| (1..=9).contains(v));
                    }
                }
                Control::Button { .. } => {}
            }
        }

        State {
            pos: opts.pos,
            focus: None,
            hover: None,
            hover_close: false,
            inputs,
            checks,
            radio,
            timer: opts.timeout.filter(|t| *t > 0).map(|secs| {
                let secs = secs.min(3600);
                Timer {
                    deadline: Instant::now() + Duration::from_secs(secs as u64),
                    remaining: secs,
                    cancelled: false,
                    button: None,
                }
            }),
            drag: None,
            outcome: None,
        }
    }

    fn countdown(&self) -> String {
        match &self.timer {
            Some(timer) if !timer.cancelled => format!("{:>2}", timer.remaining.min(99)),
            _ => "--".to_string(),
        }
    }

    fn cancel_timer(&mut self) {
        if let Some(timer) = &mut self.timer {
            timer.cancelled = true;
        }
    }

    fn tick(&mut self) {
        let Some(timer) = &mut self.timer else {
            return;
        };
        if timer.cancelled || self.outcome.is_some() {
            return;
        }
        let now = Instant::now();
        if now >= timer.deadline {
            timer.remaining = 0;
            self.outcome = Some(match &timer.button {
                Some((index, name)) => Outcome::Timeout {
                    index: Some(*index),
                    name: Some(name.clone()),
                },
                None => Outcome::Timeout {
                    index: None,
                    name: None,
                },
            });
            return;
        }
        let left = timer.deadline.saturating_duration_since(now);
        let secs = left.as_secs() + u64::from(left.subsec_millis() > 0);
        timer.remaining = secs.min(u32::MAX as u64) as u32;
    }

    fn required_ok(&self) -> bool {
        self.inputs
            .iter()
            .all(|input| !input.required || !input.is_empty())
    }
}

fn first_of_kind(placed: &[Placed], kind: ControlKind) -> Option<usize> {
    placed.iter().position(|p| p.kind == kind)
}

fn initial_focus(placed: &[Placed], opts: &DialogOptions) -> Option<usize> {
    if let Some(bar) = opts.bar {
        let want = bar.max(1);
        if let Some(index) = placed.iter().position(|p| {
            matches!(p.kind, ControlKind::Button | ControlKind::EscapeButton) && p.index + 1 == want
        }) {
            return Some(index);
        }
        if let Some(index) = first_of_kind(placed, ControlKind::Button)
            .or_else(|| first_of_kind(placed, ControlKind::EscapeButton))
        {
            return Some(index);
        }
    }
    first_of_kind(placed, ControlKind::Input)
        .or_else(|| first_of_kind(placed, ControlKind::Button))
        .or_else(|| first_of_kind(placed, ControlKind::EscapeButton))
        .or_else(|| first_of_kind(placed, ControlKind::Checkbox))
        .or_else(|| first_of_kind(placed, ControlKind::Radio))
        .or_else(|| (!placed.is_empty()).then_some(0))
}

fn next_focus(placed: &[Placed], focus: Option<usize>, delta: isize) -> Option<usize> {
    if placed.is_empty() {
        return None;
    }
    let len = placed.len() as isize;
    let start = focus
        .map(|f| f as isize)
        .unwrap_or(if delta > 0 { -1 } else { 0 });
    let next = (start + delta).rem_euclid(len);
    Some(next as usize)
}

fn same_kind_focus(placed: &[Placed], focus: Option<usize>, delta: isize) -> Option<usize> {
    let current = focus?;
    let kind = placed[current].kind;
    let candidates: Vec<usize> = placed
        .iter()
        .enumerate()
        .filter(|(_, p)| p.kind == kind)
        .map(|(i, _)| i)
        .collect();
    if candidates.len() < 2 {
        return Some(current);
    }
    let pos = candidates.iter().position(|i| *i == current).unwrap_or(0) as isize;
    let next = (pos + delta).rem_euclid(candidates.len() as isize);
    Some(candidates[next as usize])
}

fn button_outcome(placed: &Placed) -> Option<Outcome> {
    match &placed.control {
        Control::Button { label, .. } => Some(Outcome::Button {
            index: placed.index + 1,
            name: interp::command_word(label),
        }),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Drawing
// ---------------------------------------------------------------------------

fn print_clipped(screen: &mut Screen, x: u16, y: u16, text: &str, style: Style, max_x: u16) -> u16 {
    let remaining = max_x.saturating_sub(x) as usize;
    if remaining == 0 {
        return x;
    }
    let shown = wbat64_core::truncate(text, remaining);
    screen.print(x, y, &shown, style)
}

fn hotkey_style(theme: &Theme, base: Style) -> Style {
    if theme.hotkey.fg.is_default() && theme.hotkey.bg.is_default() {
        // `HotKey=NONE`: draw the letter like the rest of the label.
        base
    } else if theme.hotkey.bg.is_default() {
        Style::new(theme.hotkey.fg, base.bg)
    } else {
        theme.hotkey
    }
}

fn print_hotkey_label(
    screen: &mut Screen,
    x: u16,
    y: u16,
    label: &str,
    style: Style,
    hotkey_style: Style,
    max_x: u16,
) {
    let display = interp::display_label(label);
    if let Some((index, _)) = interp::hotkey(label) {
        let before: String = display.chars().take(index).collect();
        let after: String = display.chars().skip(index + 1).collect();
        let mut cx = print_clipped(screen, x, y, &before, style, max_x);
        if cx < max_x {
            if let Some(hot) = display.chars().nth(index) {
                cx = print_clipped(screen, cx, y, &hot.to_string(), hotkey_style, max_x);
            }
        }
        print_clipped(screen, cx, y, &after, style, max_x);
    } else {
        print_clipped(screen, x, y, &display, style, max_x);
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_control(
    screen: &mut Screen,
    theme: &Theme,
    placed: &Placed,
    focused: bool,
    hovered: bool,
    state: &State,
    opts: &DialogOptions,
    box_style: Style,
    check_symbol: char,
    radio_symbol: char,
) {
    let rect = placed.rect;
    match &placed.control {
        Control::Button { label, .. } => {
            let base = if opts.menu { box_style } else { theme.button };
            let style = if focused {
                theme.bar
            } else if hovered {
                // Hover uses inverse video, so keyboard focus (Bar) and the
                // pointer are distinguishable.
                base.reversed()
            } else {
                base
            };
            screen.clear_rect(rect, style);
            let hot_style = if focused {
                Style::new(style.fg, style.bg)
            } else {
                hotkey_style(theme, style)
            };
            print_hotkey_label(
                screen,
                rect.x + 1,
                rect.y,
                label,
                style,
                hot_style,
                rect.right() - 1,
            );
        }
        Control::Input { .. } => {
            let input = &state.inputs[placed.index];
            // WBAT puts the bar on the active element, including input fields.
            let style = if focused { theme.bar } else { theme.input };
            screen.clear_rect(rect, style);
            let visible = wbat64_core::truncate(&input.visible(), rect.w as usize);
            screen.print(rect.x, rect.y, &visible, style);
            if focused {
                let caret_x = rect.x + input.caret_cell();
                if caret_x < rect.right() {
                    let ch = screen.cell(caret_x, rect.y).ch;
                    screen.put(caret_x, rect.y, ch, style.reversed());
                }
            }
        }
        Control::Checkbox { label } => {
            let checked = state.checks.get(placed.index).copied().unwrap_or(false);
            let glyph_style = if focused { theme.bar } else { box_style };
            let max_x = rect.right();
            print_clipped(screen, rect.x, rect.y, "[", glyph_style, max_x);
            let symbol = if checked { check_symbol } else { ' ' };
            print_clipped(
                screen,
                rect.x + 1,
                rect.y,
                &symbol.to_string(),
                glyph_style,
                max_x,
            );
            print_clipped(screen, rect.x + 2, rect.y, "]", glyph_style, max_x);
            if !label.is_empty() {
                print_hotkey_label(
                    screen,
                    rect.x + 4,
                    rect.y,
                    label,
                    box_style,
                    hotkey_style(theme, box_style),
                    max_x,
                );
            }
        }
        Control::Radio { label } => {
            let checked = state.radio == Some(placed.index + 1);
            let glyph_style = if focused { theme.bar } else { box_style };
            let max_x = rect.right();
            print_clipped(screen, rect.x, rect.y, "(", glyph_style, max_x);
            let symbol = if checked { radio_symbol } else { ' ' };
            print_clipped(
                screen,
                rect.x + 1,
                rect.y,
                &symbol.to_string(),
                glyph_style,
                max_x,
            );
            print_clipped(screen, rect.x + 2, rect.y, ")", glyph_style, max_x);
            if !label.is_empty() {
                print_hotkey_label(
                    screen,
                    rect.x + 4,
                    rect.y,
                    label,
                    box_style,
                    hotkey_style(theme, box_style),
                    max_x,
                );
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_dialog(
    screen: &mut Screen,
    config: &wbat64_core::Config,
    unit: &Unit,
    layout: &Layout,
    state: &State,
    opts: &DialogOptions,
) {
    let theme = &config.theme;
    let box_style = opts.colors.unwrap_or(theme.boxed);
    if let Some(shadow) = theme.shadow {
        draw_shadow(screen, layout.outer, shadow);
    }
    draw_frame(screen, layout.outer, config.frame, box_style);
    if let Some(title) = title_of(unit, opts) {
        // WBAT uses only the BoxTitle foreground; the title sits on the box
        // background (the colour's background is ignored).
        draw_title(
            screen,
            layout.outer,
            &title,
            Style::new(theme.title.fg, box_style.bg),
        );
    }
    if layout.close.is_some() {
        // The title text ignores the BoxTitle background, but the close button
        // is drawn as a small title-coloured control (WBAT uses the full style).
        let style = if state.hover_close {
            theme.title.reversed()
        } else {
            theme.title
        };
        draw_close_button(screen, layout.outer, style, unit.close_symbol);
    }

    // Draw text spans first; controls paint their own rectangles.
    for (i, line) in unit.lines.iter().enumerate() {
        let y = layout.content.y + i as u16;
        if y >= layout.content.bottom() {
            break;
        }
        if line.kind == LineKind::Divider {
            draw_divider(
                screen,
                layout.content,
                y,
                Style::new(theme.divider.fg, box_style.bg),
            );
            continue;
        }
        if line.kind == LineKind::TitleBar {
            screen.clear_rect(
                Rect::new(layout.content.x, y, layout.content.w, 1),
                theme.text_title,
            );
        }
        let line_width = line.width();
        let offset = if unit.center && (layout.content.w as usize) > line_width {
            ((layout.content.w as usize - line_width) / 2) as u16
        } else {
            0
        };
        let mut x = layout.content.x + offset;
        let max_x = layout.content.right();
        for span in &line.spans {
            match span {
                Span::Text(text) => {
                    let style = if line.kind == LineKind::TitleBar {
                        theme.text_title
                    } else {
                        box_style
                    };
                    x = print_clipped(screen, x, y, text, style, max_x);
                }
                Span::Highlight(text) => {
                    x = print_clipped(screen, x, y, text, theme.box_hi, max_x);
                }
                Span::Var { width, .. } => {
                    x = x.saturating_add(width.unwrap_or(0) as u16);
                }
                Span::Countdown => {
                    x = print_clipped(screen, x, y, &state.countdown(), box_style, max_x);
                }
                Span::Control(control) => {
                    x = x.saturating_add(control.display_width() as u16);
                }
            }
        }
    }

    for (i, placed) in layout.placed.iter().enumerate() {
        let focused = state.focus == Some(i);
        draw_control(
            screen,
            theme,
            placed,
            focused,
            state.hover == Some(i),
            state,
            opts,
            box_style,
            config.check_symbol,
            config.radio_symbol,
        );
    }
}

// ---------------------------------------------------------------------------
// Event handling
// ---------------------------------------------------------------------------

fn set_radio(state: &mut State, index: usize) {
    state.radio = Some(index + 1);
}

fn activate_control(placed: &Placed, state: &mut State, _config: &wbat64_core::Config) {
    match &placed.control {
        Control::Button { esc: true, .. } => {
            // `[? ...]` is the escape button: activating it always cancels,
            // even when a required field is empty, and returns exit code 100.
            state.outcome = Some(Outcome::Escape);
        }
        Control::Button { esc: false, .. } => {
            if state.required_ok() {
                state.outcome = button_outcome(placed);
            }
        }
        Control::Checkbox { .. } => {
            if let Some(value) = state.checks.get_mut(placed.index) {
                *value = !*value;
            }
        }
        Control::Radio { .. } => set_radio(state, placed.index),
        Control::Input { .. } => {}
    }
}

fn hotkey_target(placed: &[Placed], ch: char) -> Option<usize> {
    placed.iter().position(|p| {
        p.control
            .hotkey()
            .map(|(_, c)| c.eq_ignore_ascii_case(&ch))
            .unwrap_or(false)
    })
}

fn handle_key(
    key: Key,
    _session: &mut Session,
    config: &wbat64_core::Config,
    unit: &Unit,
    layout: &Layout,
    state: &mut State,
) -> Result<()> {
    state.cancel_timer();

    match key.code {
        KeyCode::Esc => {
            state.outcome = Some(Outcome::Escape);
            return Ok(());
        }
        KeyCode::Tab => {
            state.focus = next_focus(&layout.placed, state.focus, 1);
            return Ok(());
        }
        KeyCode::BackTab => {
            state.focus = next_focus(&layout.placed, state.focus, -1);
            return Ok(());
        }
        KeyCode::Up => {
            state.focus = next_focus(&layout.placed, state.focus, -1);
            return Ok(());
        }
        KeyCode::Down => {
            state.focus = next_focus(&layout.placed, state.focus, 1);
            return Ok(());
        }
        KeyCode::Left => {
            if let Some(index) = state.focus {
                if matches!(layout.placed[index].kind, ControlKind::Input) {
                    state.inputs[layout.placed[index].index].left();
                } else {
                    state.focus = same_kind_focus(&layout.placed, state.focus, -1);
                }
            }
            return Ok(());
        }
        KeyCode::Right => {
            if let Some(index) = state.focus {
                if matches!(layout.placed[index].kind, ControlKind::Input) {
                    state.inputs[layout.placed[index].index].right();
                } else {
                    state.focus = same_kind_focus(&layout.placed, state.focus, 1);
                }
            }
            return Ok(());
        }
        KeyCode::Home => {
            if let Some(index) = state.focus {
                if matches!(layout.placed[index].kind, ControlKind::Input) {
                    state.inputs[layout.placed[index].index].home();
                }
            }
            return Ok(());
        }
        KeyCode::End => {
            if let Some(index) = state.focus {
                if matches!(layout.placed[index].kind, ControlKind::Input) {
                    state.inputs[layout.placed[index].index].end();
                }
            }
            return Ok(());
        }
        KeyCode::Backspace => {
            if let Some(index) = state.focus {
                if matches!(layout.placed[index].kind, ControlKind::Input) {
                    state.inputs[layout.placed[index].index].backspace();
                }
            }
            return Ok(());
        }
        KeyCode::Delete => {
            if let Some(index) = state.focus {
                if matches!(layout.placed[index].kind, ControlKind::Input) {
                    state.inputs[layout.placed[index].index].delete();
                }
            }
            return Ok(());
        }
        KeyCode::Enter => {
            if let Some(index) = state.focus {
                let placed = &layout.placed[index];
                match placed.kind {
                    ControlKind::Button | ControlKind::EscapeButton => {
                        activate_control(placed, state, config);
                    }
                    ControlKind::Input => {
                        let only_input = unit.inputs().len() == 1 && !unit.has_buttons();
                        if only_input {
                            if state.required_ok() {
                                let name = state.inputs[placed.index].var.clone();
                                state.outcome = Some(Outcome::Button { index: 1, name });
                            }
                        } else {
                            state.focus = next_focus(&layout.placed, state.focus, 1);
                        }
                    }
                    ControlKind::Checkbox => {
                        if let Some(value) = state.checks.get_mut(placed.index) {
                            *value = !*value;
                        }
                    }
                    ControlKind::Radio => set_radio(state, placed.index),
                }
            } else if state.required_ok() && layout.placed.is_empty() {
                state.outcome = Some(Outcome::Escape);
            }
            return Ok(());
        }
        KeyCode::Char(' ') if !key.mods.alt => {
            if let Some(index) = state.focus {
                match layout.placed[index].kind {
                    ControlKind::Input => {
                        state.inputs[layout.placed[index].index].insert(' ');
                    }
                    _ => activate_control(&layout.placed[index], state, config),
                }
            }
            return Ok(());
        }
        KeyCode::Char(c) => {
            if key.mods.ctrl {
                if c.eq_ignore_ascii_case(&'c') {
                    state.outcome = Some(Outcome::Escape);
                }
                return Ok(());
            }
            let is_input = state
                .focus
                .map(|i| matches!(layout.placed[i].kind, ControlKind::Input))
                .unwrap_or(false);
            if is_input && !key.mods.alt {
                let index = state.focus.unwrap();
                state.inputs[layout.placed[index].index].insert(c);
                return Ok(());
            }
            if let Some(target) = hotkey_target(&layout.placed, c) {
                state.focus = Some(target);
                activate_control(&layout.placed[target], state, config);
            }
            return Ok(());
        }
        _ => {}
    }
    Ok(())
}

fn handle_mouse(
    kind: MouseKind,
    session: &mut Session,
    config: &wbat64_core::Config,
    layout: &Layout,
    state: &mut State,
    opts: &DialogOptions,
) {
    if let MouseKind::Move { x, y } = kind {
        // Hovering must not cancel a countdown timer.
        state.hover = layout.placed.iter().position(|placed| {
            matches!(placed.kind, ControlKind::Button | ControlKind::EscapeButton)
                && placed.rect.contains(x, y)
        });
        state.hover_close = layout.close.is_some_and(|rect| rect.contains(x, y));
        return;
    }
    state.cancel_timer();
    match kind {
        MouseKind::Down {
            x,
            y,
            button: MouseButton::Left,
        } => {
            state.cancel_timer();
            if let Some(close) = layout.close {
                if close.contains(x, y) {
                    state.outcome = Some(Outcome::Escape);
                    return;
                }
            }
            if let Some(index) = layout.placed.iter().position(|p| p.rect.contains(x, y)) {
                state.focus = Some(index);
                let placed = &layout.placed[index];
                match placed.kind {
                    ControlKind::Button | ControlKind::EscapeButton => {
                        activate_control(placed, state, config);
                    }
                    ControlKind::Checkbox => {
                        if let Some(value) = state.checks.get_mut(placed.index) {
                            *value = !*value;
                        }
                    }
                    ControlKind::Radio => set_radio(state, placed.index),
                    ControlKind::Input => {
                        let column = x.saturating_sub(placed.rect.x);
                        state.inputs[placed.index].set_caret_from_column(column);
                    }
                }
                return;
            }
            if y == layout.outer.y && layout.outer.contains(x, y) && !opts.fix {
                let origin = Pos::new(layout.outer.y + 1, layout.outer.x + 1);
                state.drag = Some(Drag {
                    start_x: x,
                    start_y: y,
                    origin,
                });
            }
        }
        MouseKind::Down {
            x,
            y,
            button: MouseButton::Right,
        } => {
            if !layout.outer.contains(x, y) && config.right_esc {
                state.outcome = Some(Outcome::Escape);
            }
        }
        MouseKind::Drag { x, y } => {
            state.hover = None;
            state.hover_close = false;
            if let Some(drag) = &state.drag {
                let line = (drag.origin.line as i32 + y as i32 - drag.start_y as i32)
                    .clamp(1, session.size.rows as i32) as u16;
                let col = (drag.origin.col as i32 + x as i32 - drag.start_x as i32)
                    .clamp(1, session.size.cols as i32) as u16;
                state.pos = Some(Pos::new(line, col));
            }
        }
        MouseKind::Up { .. } => {
            state.drag = None;
        }
        _ => {}
    }
}

/// Run a dialog until it closes.
pub fn run_dialog(
    session: &mut Session,
    config: &wbat64_core::Config,
    unit: &Unit,
    opts: &DialogOptions,
) -> Result<DialogResult> {
    let mut base = session.screen.clone();
    let mut state = State::new(unit, opts, &config.frame);
    let mut last_outer: Option<Rect> = None;
    let mut layout = build_layout(unit, session.size, opts, &state)?;
    state.focus = initial_focus(&layout.placed, opts);

    // Resolve the timeout's button now; an invalid bar closes as ESC.
    if let Some(timer) = &mut state.timer {
        if let Some(bar) = opts.bar {
            let want = bar.max(1);
            if let Some(placed) = layout.placed.iter().find(|p| {
                matches!(p.kind, ControlKind::Button | ControlKind::EscapeButton)
                    && p.index + 1 == want
            }) {
                if let Control::Button { label, .. } = &placed.control {
                    timer.button = Some((want, interp::command_word(label)));
                }
            }
        }
    }

    loop {
        layout = match build_layout(unit, session.size, opts, &state) {
            Ok(layout) => layout,
            Err(e) => {
                if !opts.fix {
                    session.screen.reset_to(&base, last_outer, Style::default());
                    session.present()?;
                }
                return Err(e);
            }
        };
        session.screen.reset_to(&base, last_outer, Style::default());
        draw_dialog(&mut session.screen, config, unit, &layout, &state, opts);
        session.present()?;
        last_outer = Some(layout.outer);

        let event = session.next_event(Duration::from_millis(100))?;
        match event {
            Event::Tick => state.tick(),
            Event::Resize(_, _) => {
                session.refresh_size()?;
                base.resize(session.size.cols, session.size.rows);
                state.hover = None;
                state.hover_close = false;
                if opts.pos.is_none() {
                    state.pos = None;
                }
            }
            Event::Key(key) => handle_key(key, session, config, unit, &layout, &mut state)?,
            Event::Mouse(kind) => handle_mouse(kind, session, config, &layout, &mut state, opts),
        }

        if state.outcome.is_some() {
            break;
        }
    }

    let outcome = state.outcome.take().unwrap();
    let outer = layout.outer;
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
    let cursor_y = if opts.fix {
        outer.bottom().min(session.size.rows.saturating_sub(1))
    } else {
        outer.y
    };
    session.set_cursor(outer.x, cursor_y)?;

    let (button_index, button_name) = match &outcome {
        Outcome::Button { index, name } => (Some(*index), Some(name.clone())),
        Outcome::Timeout {
            index: Some(index),
            name,
        } => (Some(*index), name.clone()),
        _ => (None, None),
    };

    Ok(DialogResult {
        outcome,
        button_index,
        button_name,
        inputs: state
            .inputs
            .iter()
            .map(|input| (input.var.clone(), input.result_value()))
            .collect(),
        checkboxes: state.checks,
        radio: state.radio,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::{Session, SessionOptions};
    use wbat64_core::{parse_layout, Color, Pos};

    fn session_with(keys: &str) -> Session {
        let options = SessionOptions {
            keys: Some(keys.to_string()),
            headless_size: Some((30, 10)),
            ..SessionOptions::default()
        };
        Session::open(&wbat64_core::Config::default(), &options).unwrap()
    }

    fn session() -> Session {
        session_with("esc")
    }

    #[test]
    fn default_box_uses_wbat_roles_and_a_blank_frame_margin() {
        let (units, errors) = parse_layout(":u \"test\" [x]\n Hello\n [ OK ]\n:", "t", '~');
        assert!(errors.is_empty(), "{errors:?}");
        let mut session = session();
        let opts = DialogOptions {
            pos: Some(Pos::new(2, 2)),
            fix: true,
            ..DialogOptions::default()
        };
        run_dialog(
            &mut session,
            &wbat64_core::Config::default(),
            &units[0],
            &opts,
        )
        .unwrap();

        // Outer at 1,1; the blank frame cell is the one-character margin and
        // carries the Box background.
        assert_eq!(session.screen.cell(1, 1).ch, ' ');
        assert_eq!(session.screen.cell(1, 1).bg, Color::Indexed(15));
        // Box text starts one cell inside that margin.
        assert_eq!(session.screen.cell(3, 2).ch, 'H');
        assert_eq!(session.screen.cell(3, 2).bg, Color::Indexed(15));
        // Title uses BoxTitle: black on bright blue.
        let title_x = (2..20)
            .find(|&x| session.screen.cell(x, 1).ch == 't')
            .expect("title");
        assert_eq!(session.screen.cell(title_x, 1).bg, Color::Indexed(15));
        // The focused button uses Bar: bright white on bright red.
        assert_eq!(session.screen.cell(3, 3).ch, ' ');
        assert_eq!(session.screen.cell(3, 3).bg, Color::Indexed(12));
        assert_eq!(session.screen.cell(5, 3).ch, 'O');
        assert_eq!(session.screen.cell(5, 3).bg, Color::Indexed(12));
        assert_eq!(session.screen.cell(5, 3).fg, Color::Indexed(15));
        // The close button is a one-cell title-coloured control.
        assert_eq!(session.screen.cell(8, 1).ch, 'x');
        assert_eq!(session.screen.cell(8, 1).bg, Color::Indexed(9));
    }

    #[test]
    fn hovering_a_button_inverts_it_without_moving_focus() {
        let (units, errors) = parse_layout(":u\n[OK] [NO]\n:", "t", '~');
        assert!(errors.is_empty(), "{errors:?}");
        let mut session = session_with("move:8x2,esc");
        let opts = DialogOptions {
            pos: Some(Pos::new(2, 2)),
            fix: true,
            ..DialogOptions::default()
        };
        run_dialog(
            &mut session,
            &wbat64_core::Config::default(),
            &units[0],
            &opts,
        )
        .unwrap();
        assert_eq!(session.screen.cell(8, 2).ch, 'N');
        assert_eq!(session.screen.cell(8, 2).fg, Color::Indexed(7));
        assert_eq!(session.screen.cell(8, 2).bg, Color::Indexed(0));
        // The keyboard focus stays on the first button (Bar).
        assert_eq!(session.screen.cell(3, 2).bg, Color::Indexed(12));
    }

    #[test]
    fn moving_the_pointer_away_clears_hover() {
        let (units, errors) = parse_layout(":u\n[OK] [NO]\n:", "t", '~');
        assert!(errors.is_empty(), "{errors:?}");
        let mut session = session_with("move:8x2,move:20x2,esc");
        let opts = DialogOptions {
            pos: Some(Pos::new(2, 2)),
            fix: true,
            ..DialogOptions::default()
        };
        run_dialog(
            &mut session,
            &wbat64_core::Config::default(),
            &units[0],
            &opts,
        )
        .unwrap();
        assert_eq!(session.screen.cell(8, 2).fg, Color::Indexed(0));
        assert_eq!(session.screen.cell(8, 2).bg, Color::Indexed(7));
    }

    #[test]
    fn menu_buttons_use_the_box_colour_when_unfocused() {
        let (units, errors) = parse_layout(":u\n [ first ]\n [ second ]\n:", "t", '~');
        assert!(errors.is_empty(), "{errors:?}");
        let mut session = session();
        let opts = DialogOptions {
            pos: Some(Pos::new(2, 2)),
            fix: true,
            menu: true,
            ..DialogOptions::default()
        };
        run_dialog(
            &mut session,
            &wbat64_core::Config::default(),
            &units[0],
            &opts,
        )
        .unwrap();
        let second_y = 3;
        assert_eq!(session.screen.cell(2, second_y).ch, ' ');
        assert_eq!(session.screen.cell(2, second_y).bg, Color::Indexed(15));
        assert_eq!(session.screen.cell(5, second_y).ch, 's');
        assert_eq!(session.screen.cell(5, second_y).bg, Color::Indexed(15));
    }
}
