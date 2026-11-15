//! The layout language: unit extraction, tokenizer and AST.
//!
//! A layout file is a sequence of units. A line whose first character is `:`
//! starts a unit; the next such line ends it. Inside a unit, `[...]` marks a
//! control, `~...~` a highlight, `%NAME%` a variable and `[[`, `%%`, `~~`,
//! `##` escapes.
//!
//! The parser is intentionally independent of the environment: [`Unit`] keeps
//! `%VAR%` as a [`Span::Var`] and [`Unit::interpolate`] resolves it. Control
//! text is never interpolated, which is the rule from section 4.3.

use crate::error::{Error, Result};
use crate::interp::{self, Lookup};
use crate::text::{char_width, str_width};

/// Divider marker (U+2500, formerly CP437 code 196).
pub const DIVIDER: char = '─';

/// One piece of a layout line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Span {
    /// Literal text.
    Text(String),
    /// Text drawn in the highlight role (`BoxHi` / `TextHi`).
    Highlight(String),
    /// An unexpanded `%NAME%` reference with an optional fixed width.
    Var { name: String, width: Option<usize> },
    /// An interactive control.
    Control(Control),
    /// The `#?` countdown placeholder.
    Countdown,
}

impl Span {
    /// Width in cells after interpolation.
    pub fn width(&self) -> usize {
        match self {
            Span::Text(s) | Span::Highlight(s) => str_width(s),
            Span::Var { width, .. } => width.unwrap_or(0),
            Span::Control(c) => c.display_width(),
            Span::Countdown => 2,
        }
    }

    /// Interpolate text and variables, leaving controls alone.
    pub fn expand(&self, lookup: Lookup<'_>) -> Span {
        match self {
            Span::Text(s) => Span::Text(interp::interpolate(s, lookup)),
            Span::Highlight(s) => Span::Highlight(interp::interpolate(s, lookup)),
            Span::Var { name, width } => {
                let value = lookup(name).unwrap_or_default();
                match width {
                    Some(w) => Span::Text(crate::text::fit(&value, *w)),
                    None => Span::Text(value),
                }
            }
            Span::Control(c) => Span::Control(c.clone()),
            Span::Countdown => Span::Countdown,
        }
    }

    /// Plain-text rendering (used by `stdout`).
    pub fn plain(&self) -> String {
        match self {
            Span::Text(s) | Span::Highlight(s) => s.clone(),
            Span::Var { name, .. } => format!("%{name}%"),
            Span::Control(c) => c.plain(),
            Span::Countdown => "--".to_string(),
        }
    }
}

/// A control from a `[...]` token.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Control {
    Button {
        label: String,
        esc: bool,
    },
    Input {
        var: String,
        len: Option<usize>,
        upper: bool,
        password: bool,
        required: bool,
        /// Displayed field width in cells.
        width: usize,
    },
    Checkbox {
        label: String,
    },
    Radio {
        label: String,
    },
}

/// The kind of a control, used for focus ordering and hit testing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlKind {
    Button,
    EscapeButton,
    Input,
    Checkbox,
    Radio,
}

impl Control {
    pub fn kind(&self) -> ControlKind {
        match self {
            Control::Button { esc: false, .. } => ControlKind::Button,
            Control::Button { esc: true, .. } => ControlKind::EscapeButton,
            Control::Input { .. } => ControlKind::Input,
            Control::Checkbox { .. } => ControlKind::Checkbox,
            Control::Radio { .. } => ControlKind::Radio,
        }
    }

    /// The label as displayed, with `&` hotkey markers removed.
    pub fn display_label(&self) -> String {
        match self {
            Control::Button { label, .. } => interp::display_label(label),
            Control::Checkbox { label } | Control::Radio { label } => label.clone(),
            Control::Input { .. } => String::new(),
        }
    }

    /// Width in cells.
    pub fn display_width(&self) -> usize {
        match self {
            // WBAT paints the bracket cells as one blank of padding on each
            // side of a button, even when the layout is written as `[OK]`.
            Control::Button { .. } => str_width(&self.display_label()) + 2,
            Control::Input { width, .. } => *width,
            Control::Checkbox { label } | Control::Radio { label } => {
                3 + if label.trim().is_empty() {
                    0
                } else {
                    1 + str_width(label.trim())
                }
            }
        }
    }

    /// Hotkey character and its index in the displayed label.
    pub fn hotkey(&self) -> Option<(usize, char)> {
        match self {
            Control::Button { label, .. } => interp::hotkey(label),
            Control::Checkbox { label } | Control::Radio { label } => interp::hotkey(label),
            Control::Input { .. } => None,
        }
    }

    /// The WBAT result word (buttons only).
    pub fn command_word(&self) -> String {
        match self {
            Control::Button { label, .. } => interp::command_word(label),
            Control::Checkbox { label } | Control::Radio { label } => interp::command_word(label),
            Control::Input { var, .. } => var.clone(),
        }
    }

    pub fn var_name(&self) -> Option<&str> {
        match self {
            Control::Input { var, .. } => Some(var),
            _ => None,
        }
    }

    /// Plain text for `stdout` output.
    pub fn plain(&self) -> String {
        match self {
            Control::Button { label, .. } => interp::display_label(label),
            Control::Checkbox { label } | Control::Radio { label } => {
                if label.trim().is_empty() {
                    "[ ]".to_string()
                } else {
                    format!("[ ] {}", label.trim())
                }
            }
            Control::Input { width, .. } => " ".repeat(*width),
        }
    }
}

/// Special line kinds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineKind {
    Normal,
    /// A line starting with `─`: draw a full-width divider.
    Divider,
    /// A line starting with `#` in a text unit: draw a title bar.
    TitleBar,
}

/// One line of a unit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    pub spans: Vec<Span>,
    pub line_no: usize,
    pub kind: LineKind,
}

impl Line {
    pub fn empty(line_no: usize) -> Line {
        Line {
            spans: Vec::new(),
            line_no,
            kind: LineKind::Normal,
        }
    }

    pub fn width(&self) -> usize {
        self.spans.iter().map(Span::width).sum()
    }

    pub fn expand(&self, lookup: Lookup<'_>) -> Line {
        Line {
            spans: self.spans.iter().map(|s| s.expand(lookup)).collect(),
            line_no: self.line_no,
            kind: self.kind,
        }
    }

    pub fn plain(&self) -> String {
        self.spans.iter().map(Span::plain).collect()
    }

    pub fn has_controls(&self) -> bool {
        self.spans.iter().any(|s| matches!(s, Span::Control(_)))
    }
}

/// A parsed layout unit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unit {
    pub label: String,
    pub title: Option<String>,
    /// A `[x]` close button was requested in the label line.
    pub close: bool,
    /// The symbol shown in that close button (the character inside brackets).
    pub close_symbol: char,
    pub lines: Vec<Line>,
    /// Source file name for messages.
    pub file: String,
    /// 1-based line number of the label line.
    pub line_no: usize,
    /// Centre each line inside the content rectangle (quick boxes).
    pub center: bool,
}

impl Unit {
    /// Interpolate variables in text and highlights. Controls are untouched.
    pub fn interpolate(&self, lookup: Lookup<'_>) -> Unit {
        Unit {
            lines: self.lines.iter().map(|l| l.expand(lookup)).collect(),
            ..self.clone()
        }
    }

    /// Content size in cells (width, height), excluding frame and shadow.
    pub fn content_size(&self) -> (usize, usize) {
        let width = self.lines.iter().map(Line::width).max().unwrap_or(0);
        (width, self.lines.len())
    }

    /// All buttons in reading order.
    pub fn buttons(&self) -> Vec<&Control> {
        self.lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .filter_map(|s| match s {
                Span::Control(c)
                    if matches!(c.kind(), ControlKind::Button | ControlKind::EscapeButton) =>
                {
                    Some(c)
                }
                _ => None,
            })
            .collect()
    }

    /// The button number (1-based) for each escape button.
    pub fn has_buttons(&self) -> bool {
        !self.buttons().is_empty()
    }

    pub fn inputs(&self) -> Vec<&Control> {
        self.controls_of(ControlKind::Input)
    }

    fn controls_of(&self, kind: ControlKind) -> Vec<&Control> {
        self.lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .filter_map(|s| match s {
                Span::Control(c) if c.kind() == kind => Some(c),
                _ => None,
            })
            .collect()
    }

    /// All controls in reading order.
    pub fn controls(&self) -> Vec<&Control> {
        self.lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .filter_map(|s| match s {
                Span::Control(c) => Some(c),
                _ => None,
            })
            .collect()
    }

    /// Plain text of the unit, one line per layout line.
    pub fn plain_text(&self) -> String {
        self.lines
            .iter()
            .map(Line::plain)
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Structural checks that only make sense for a whole unit: WBAT supports
    /// at most nine checkboxes and nine radio buttons, and input variables must
    /// be valid shell variable names.
    pub fn validate(&self) -> Vec<Error> {
        let mut errors = Vec::new();
        let (mut checkboxes, mut radios) = (0usize, 0usize);
        let mut reported_boxes = false;
        let mut reported_radios = false;
        for line in &self.lines {
            for span in &line.spans {
                let Span::Control(control) = span else {
                    continue;
                };
                match control {
                    Control::Checkbox { .. } => {
                        checkboxes += 1;
                        if checkboxes > 9 && !reported_boxes {
                            reported_boxes = true;
                            errors.push(Error::at(
                                &self.file,
                                line.line_no,
                                1,
                                "more than 9 checkboxes in one box",
                            ));
                        }
                    }
                    Control::Radio { .. } => {
                        radios += 1;
                        if radios > 9 && !reported_radios {
                            reported_radios = true;
                            errors.push(Error::at(
                                &self.file,
                                line.line_no,
                                1,
                                "more than 9 radio buttons in one box",
                            ));
                        }
                    }
                    Control::Input { var, .. } => {
                        if !crate::emit::is_valid_var_name(var) {
                            errors.push(Error::at(
                                &self.file,
                                line.line_no,
                                1,
                                format!("invalid input variable name '{var}'"),
                            ));
                        }
                    }
                    Control::Button { .. } => {}
                }
            }
        }
        errors
    }
}

fn parse_symbol_label(s: &str) -> (&str, &str) {
    let end = s
        .char_indices()
        .find(|(_, c)| c.is_whitespace() || *c == '"' || *c == '[')
        .map(|(i, _)| i)
        .unwrap_or(s.len());
    (&s[..end], &s[end..])
}

fn parse_label(
    line: &str,
    file: &str,
    line_no: usize,
) -> Result<(String, Option<String>, bool, char)> {
    let rest = &line[1..];
    let (name, mut after) = parse_symbol_label(rest);
    let name = name.trim();
    if name.is_empty() {
        return Err(Error::at(
            file,
            line_no,
            1,
            "unit label is empty (write ':' to end a unit)",
        ));
    }
    let mut title = None;
    let mut close = false;
    let mut close_symbol = 'x';
    loop {
        after = after.trim_start();
        if after.is_empty() {
            break;
        }
        let mut chars = after.char_indices();
        match chars.next() {
            Some((_, '"')) => {
                let body = &after[1..];
                if let Some(end) = body.find('"') {
                    title = Some(body[..end].to_string());
                    after = &body[end + 1..];
                } else {
                    return Err(Error::at(
                        file,
                        line_no,
                        line.len() - body.len(),
                        "unterminated title string",
                    ));
                }
            }
            Some((_, '[')) => {
                if let Some(end) = after.find(']') {
                    close = true;
                    close_symbol = after[1..end].chars().next().unwrap_or('x');
                    break;
                }
                return Err(Error::at(
                    file,
                    line_no,
                    line.len() - after.len() + 1,
                    "unterminated '[' in label line",
                ));
            }
            _ => {
                // Unknown trailing text: ignore, WBAT is lenient here.
                break;
            }
        }
    }
    Ok((name.to_ascii_lowercase(), title, close, close_symbol))
}

fn push_text(spans: &mut Vec<Span>, ch: char) {
    if let Some(Span::Text(s)) = spans.last_mut() {
        s.push(ch);
    } else {
        spans.push(Span::Text(ch.to_string()));
    }
}

/// Classify the inside of a `[...]` token.
pub fn classify_control(inner: &str) -> Result<Control> {
    let chars: Vec<char> = inner.chars().collect();
    let first = chars.first().copied();
    match first {
        Some('$') => {
            let body: String = chars[1..].iter().collect();
            let mut parts = body.split(',');
            let var = parts.next().unwrap_or("").trim().to_string();
            if var.is_empty() {
                return Err(Error::msg("input control needs a variable name"));
            }
            let mut len = None;
            let mut upper = false;
            let mut password = false;
            let mut required = false;
            for part in parts {
                let p = part.trim();
                if p.is_empty() {
                    continue;
                }
                if p.chars().all(|c| c.is_ascii_digit()) {
                    len = Some(
                        p.parse::<usize>()
                            .map_err(|_| Error::msg(format!("invalid input width '{p}'")))?,
                    );
                    continue;
                }
                for ch in p.chars() {
                    match ch.to_ascii_uppercase() {
                        'U' => upper = true,
                        'P' => password = true,
                        '!' => required = true,
                        _ => {}
                    }
                }
            }
            let default_width = inner.chars().count().saturating_sub(1).max(1);
            let width = len.unwrap_or(default_width).max(1);
            Ok(Control::Input {
                var,
                len,
                upper,
                password,
                required,
                width,
            })
        }
        Some('!') => {
            let label: String = chars[1..].iter().collect();
            Ok(Control::Checkbox {
                label: label.trim().to_string(),
            })
        }
        Some('.') => {
            let label: String = chars[1..].iter().collect();
            Ok(Control::Radio {
                label: label.trim().to_string(),
            })
        }
        Some('?') => {
            // Keep the padding inside the brackets: `[? cancel ]` displays as
            // " cancel " so the escape button has the same breathing room as
            // the other buttons.
            let label: String = chars[1..].iter().collect();
            Ok(Control::Button { label, esc: true })
        }
        _ => Ok(Control::Button {
            label: inner.to_string(),
            esc: false,
        }),
    }
}

fn parse_body_line(raw: &str, file: &str, line_no: usize, hi: char) -> Result<Line> {
    let expanded = crate::text::expand_tabs(raw, 8);
    let mut text = expanded.as_str();
    if let Some(stripped) = text.strip_suffix('^') {
        text = stripped;
    }
    if text.is_empty() {
        return Ok(Line::empty(line_no));
    }
    if text.starts_with(DIVIDER) {
        return Ok(Line {
            spans: Vec::new(),
            line_no,
            kind: LineKind::Divider,
        });
    }
    let mut kind = LineKind::Normal;
    if let Some(rest) = text.strip_prefix('#') {
        kind = LineKind::TitleBar;
        text = rest.strip_prefix(' ').unwrap_or(rest);
    }

    let chars: Vec<char> = text.chars().collect();
    let mut spans: Vec<Span> = Vec::new();
    let mut i = 0usize;
    while i < chars.len() {
        let ch = chars[i];
        match ch {
            '[' => {
                if i + 1 < chars.len() && chars[i + 1] == '[' {
                    push_text(&mut spans, '[');
                    i += 2;
                    continue;
                }
                // Find the closing bracket.
                let close = (i + 1..chars.len()).find(|&j| chars[j] == ']');
                let Some(close) = close else {
                    return Err(Error::at(
                        file,
                        line_no,
                        i + 1,
                        format!("unterminated control '[' (opened at column {})", i + 1),
                    ));
                };
                let inner: String = chars[i + 1..close].iter().collect();
                let control = classify_control(&inner)
                    .map_err(|e| Error::at(file, line_no, i + 1, e.to_string()))?;
                spans.push(Span::Control(control));
                i = close + 1;
            }
            '%' => {
                if i + 1 < chars.len() && chars[i + 1] == '%' {
                    push_text(&mut spans, '%');
                    i += 2;
                    continue;
                }
                match (i + 1..chars.len()).find(|&j| chars[j] == '%') {
                    Some(close) => {
                        let name: String = chars[i + 1..close].iter().collect();
                        let mut j = close + 1;
                        while j < chars.len() && chars[j] == '_' {
                            j += 1;
                        }
                        let width = if j > close + 1 {
                            Some(j - (close + 1))
                        } else {
                            None
                        };
                        spans.push(Span::Var { name, width });
                        i = j;
                    }
                    None => {
                        push_text(&mut spans, '%');
                        i += 1;
                    }
                }
            }
            c if c == hi => {
                if i + 1 < chars.len() && chars[i + 1] == hi {
                    push_text(&mut spans, hi);
                    i += 2;
                    continue;
                }
                // Find the closing highlight symbol, honouring doubled ones.
                let mut j = i + 1;
                let mut content = String::new();
                let mut closed = false;
                while j < chars.len() {
                    if chars[j] == hi {
                        if j + 1 < chars.len() && chars[j + 1] == hi {
                            content.push(hi);
                            j += 2;
                            continue;
                        }
                        closed = true;
                        j += 1;
                        break;
                    }
                    content.push(chars[j]);
                    j += 1;
                }
                if closed {
                    spans.push(Span::Highlight(content));
                    i = j;
                } else {
                    push_text(&mut spans, hi);
                    i += 1;
                }
            }
            '#' => {
                if i + 1 < chars.len() && chars[i + 1] == '?' {
                    spans.push(Span::Countdown);
                    i += 2;
                } else if i + 1 < chars.len() && chars[i + 1] == '#' {
                    push_text(&mut spans, '#');
                    i += 2;
                } else {
                    push_text(&mut spans, '#');
                    i += 1;
                }
            }
            c => {
                push_text(&mut spans, c);
                i += 1;
            }
        }
    }
    Ok(Line {
        spans,
        line_no,
        kind,
    })
}

/// Parse every unit in a layout document.
///
/// Returns all units that could be parsed plus the diagnostics collected on the
/// way; a broken line does not stop parsing (so `wbat64 check` can list several).
pub fn parse_layout(text: &str, file: &str, hi_symbol: char) -> (Vec<Unit>, Vec<Error>) {
    let mut units: Vec<Unit> = Vec::new();
    let mut errors: Vec<Error> = Vec::new();
    let mut current: Option<Unit> = None;

    for (idx, raw) in text.lines().enumerate() {
        let line_no = idx + 1;
        let raw = raw.strip_suffix('\r').unwrap_or(raw);
        // WBAT layouts commonly end a unit with `::` (also a batch comment).
        // It is a terminator, not an empty-label unit.
        if raw.starts_with("::") {
            if let Some(unit) = current.take() {
                units.push(unit);
            }
            continue;
        }
        if raw.starts_with(':') {
            if let Some(unit) = current.take() {
                units.push(unit);
            }
            if raw.trim() == ":" || raw.trim().is_empty() {
                continue;
            }
            match parse_label(raw, file, line_no) {
                Ok((label, title, close, close_symbol)) => {
                    current = Some(Unit {
                        label,
                        title,
                        close,
                        close_symbol,
                        lines: Vec::new(),
                        file: file.to_string(),
                        line_no,
                        center: false,
                    });
                }
                Err(e) => {
                    errors.push(e);
                    // Keep collecting diagnostics but ignore this unit.
                    current = Some(Unit {
                        label: String::new(),
                        title: None,
                        close: false,
                        close_symbol: 'x',
                        lines: Vec::new(),
                        file: file.to_string(),
                        line_no,
                        center: false,
                    });
                }
            }
            continue;
        }
        if let Some(unit) = current.as_mut() {
            match parse_body_line(raw, file, line_no, hi_symbol) {
                Ok(line) => unit.lines.push(line),
                Err(e) => {
                    errors.push(e);
                    unit.lines.push(Line::empty(line_no));
                }
            }
        }
    }
    if let Some(unit) = current.take() {
        units.push(unit);
    }
    units.retain(|u| !u.label.is_empty());
    (units, errors)
}

/// Find a unit by label (case-insensitive).
pub fn find_unit<'a>(units: &'a [Unit], label: &str) -> Option<&'a Unit> {
    let want = label.trim().to_ascii_lowercase();
    units.iter().find(|u| u.label == want)
}

/// Build a synthetic unit for a quick box.
///
/// `prompt` is split on `^` and newlines; `buttons` are the comma-separated
/// items given on the command line. The result is centred line by line.
pub fn quick_box(prompt: &str, buttons: &[String], title: Option<&str>) -> Unit {
    let mut lines: Vec<Line> = Vec::new();
    let mut line_no = 1usize;
    if !prompt.trim().is_empty() {
        for part in prompt.split(['^', '\n']) {
            let mut spans = Vec::new();
            if !part.is_empty() {
                spans.push(Span::Text(part.to_string()));
            }
            lines.push(Line {
                spans,
                line_no,
                kind: LineKind::Normal,
            });
            line_no += 1;
        }
        lines.push(Line::empty(line_no));
        line_no += 1;
    }
    if !buttons.is_empty() {
        let mut spans = Vec::new();
        for (i, b) in buttons.iter().enumerate() {
            if i > 0 {
                spans.push(Span::Text("  ".to_string()));
            }
            spans.push(Span::Control(Control::Button {
                label: b.clone(),
                esc: false,
            }));
        }
        lines.push(Line {
            spans,
            line_no,
            kind: LineKind::Normal,
        });
    }
    Unit {
        label: String::new(),
        title: title.map(str::to_string),
        close: false,
        close_symbol: 'x',
        lines,
        file: String::new(),
        line_no: 0,
        center: true,
    }
}

/// Compute the display offset of every span on a line, for hit testing and
/// control placement. Returns `(span_index, x_offset, span)`.
pub fn span_positions(line: &Line) -> Vec<(usize, usize)> {
    let mut out = Vec::with_capacity(line.spans.len());
    let mut x = 0usize;
    for (i, span) in line.spans.iter().enumerate() {
        out.push((i, x));
        x += span.width();
    }
    out
}

/// Most characters that fit in a cell width (for truncation diagnostics).
pub fn clamp_width(s: &str, width: usize) -> &str {
    let mut used = 0usize;
    for (i, ch) in s.char_indices() {
        let w = char_width(ch);
        if used + w > width {
            return &s[..i];
        }
        used += w;
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIALOGS: &str = r#":main_menu "Main menu" [x]
 [ &Tour    ]  Take the tour
 [ &Setup   ]  Preferences
 [ &Report  ]  Make a report
 [? E&xit   ]
:

:signup "New user" [x]

 Please enter your name and city ^

 Name: [$ name,40              ]
 City: [$ city                 ]

 [.] male  [.] female

 [!] over 21     [!] send newsletter

        [    OK    ] [? cancel ]
:
"#;

    fn env(name: &str) -> Option<String> {
        match name {
            "USERNAME" => Some("Ada".into()),
            _ => None,
        }
    }

    #[test]
    fn parses_units() {
        let (units, errors) = parse_layout(DIALOGS, "dialogs.txt", '~');
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(units.len(), 2);
        assert_eq!(units[0].label, "main_menu");
        assert_eq!(units[0].title.as_deref(), Some("Main menu"));
        assert!(units[0].close);
        assert_eq!(units[0].buttons().len(), 4);
        assert_eq!(units[0].buttons()[3].command_word(), "Exit");
        assert_eq!(units[1].inputs().len(), 2);
    }

    #[test]
    fn control_classification() {
        match classify_control("$ name,40,U!").unwrap() {
            Control::Input {
                var,
                len,
                upper,
                password,
                required,
                width,
            } => {
                assert_eq!(var, "name");
                assert_eq!(len, Some(40));
                assert!(upper && required && !password);
                assert_eq!(width, 40);
            }
            other => panic!("{other:?}"),
        }
        match classify_control("$ city     ").unwrap() {
            Control::Input { var, width, .. } => {
                assert_eq!(var, "city");
                assert!(width >= 1);
            }
            other => panic!("{other:?}"),
        }
        assert!(matches!(
            classify_control("! over 21").unwrap(),
            Control::Checkbox { .. }
        ));
        assert!(matches!(
            classify_control(".").unwrap(),
            Control::Radio { .. }
        ));
        match classify_control("? E&xit").unwrap() {
            Control::Button { label, esc } => {
                assert!(esc);
                // The space after '?' is padding and is preserved.
                assert_eq!(label, " E&xit");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn buttons_are_numbered_in_reading_order() {
        let (units, _) = parse_layout(DIALOGS, "dialogs.txt", '~');
        let words: Vec<String> = units[1]
            .buttons()
            .iter()
            .map(|b| b.command_word())
            .collect();
        assert_eq!(words, vec!["OK", "cancel"]);
    }

    #[test]
    fn escapes_and_highlights() {
        let text = ":u\nliteral [[ ~hi~ %% ~~ ##\n:";
        let (units, errors) = parse_layout(text, "t.txt", '~');
        assert!(errors.is_empty());
        assert_eq!(units[0].lines[0].plain(), "literal [ hi % ~ #");
        assert!(matches!(units[0].lines[0].spans[1], Span::Highlight(_)));
    }

    #[test]
    fn variables_and_fixed_width() {
        let text = ":u\n%USERNAME%__ %USERNAME%\n:";
        let (units, _) = parse_layout(text, "t.txt", '~');
        let expanded = units[0].interpolate(&env);
        assert_eq!(expanded.lines[0].plain(), "Ad Ada");
    }

    #[test]
    fn divider_and_title_lines() {
        let text = ":u\n# Title\n───\ntext\n:";
        let (units, _) = parse_layout(text, "t.txt", '~');
        assert_eq!(units[0].lines[0].kind, LineKind::TitleBar);
        assert_eq!(units[0].lines[0].plain(), "Title");
        assert_eq!(units[0].lines[1].kind, LineKind::Divider);
    }

    #[test]
    fn unterminated_control_reports_position() {
        let text = ":u\nhello [broken\n:";
        let (_, errors) = parse_layout(text, "broken.txt", '~');
        assert_eq!(errors.len(), 1);
        assert_eq!(
            errors[0].to_string(),
            "broken.txt:2:7: unterminated control '[' (opened at column 7)"
        );
    }

    #[test]
    fn labels_are_case_insensitive() {
        let (units, _) = parse_layout(DIALOGS, "d.txt", '~');
        assert!(find_unit(&units, "MAIN_MENU").is_some());
        assert!(find_unit(&units, "nope").is_none());
    }

    #[test]
    fn quick_box_layout() {
        let unit = quick_box(
            "Continue?^Second line",
            &["&yes".into(), "&no".into()],
            None,
        );
        assert_eq!(unit.lines.len(), 4);
        assert_eq!(unit.content_size().0, 11);
        assert_eq!(unit.buttons().len(), 2);
        assert_eq!(unit.buttons()[0].display_label(), "yes");
        assert!(unit.center);
    }

    #[test]
    fn double_colon_terminates_a_unit() {
        let text = ":first\nhello\n::\n:second\nworld\n:";
        let (units, errors) = parse_layout(text, "t.txt", '~');
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(units.len(), 2);
        assert_eq!(units[0].label, "first");
        assert_eq!(units[0].lines.len(), 1);
        assert_eq!(units[0].lines[0].plain(), "hello");
        assert_eq!(units[1].label, "second");
        assert_eq!(units[1].lines[0].plain(), "world");
    }

    #[test]
    fn button_brackets_become_padding() {
        let ok = classify_control("OK").unwrap();
        assert_eq!(ok.display_width(), 4);
        assert_eq!(ok.display_label(), "OK");
        let esc = classify_control("? cancel ").unwrap();
        assert_eq!(esc.display_width(), 10);
        assert_eq!(esc.display_label(), " cancel ");
    }

    #[test]
    fn unit_validation_catches_limits_and_bad_names() {
        let ten_boxes = ":u\n[!][!][!][!][!][!][!][!][!][!]\n:";
        let (units, _) = parse_layout(ten_boxes, "t.txt", '~');
        let errors = units[0].validate();
        assert_eq!(errors.len(), 1);
        assert!(errors[0].to_string().contains("more than 9 checkboxes"));

        let bad_name = ":u\n[$ bad name,10 ]\n:";
        let (units, _) = parse_layout(bad_name, "t.txt", '~');
        let errors = units[0].validate();
        assert_eq!(errors.len(), 1);
        assert!(errors[0]
            .to_string()
            .contains("invalid input variable name"));
    }
}
