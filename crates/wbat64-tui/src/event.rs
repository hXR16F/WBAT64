//! Terminal-independent input events.
//!
//! The dialog state machines speak this vocabulary so that they can be driven
//! by a real terminal or by a scripted `--keys` list in tests.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum KeyCode {
    Char(char),
    Enter,
    Esc,
    Tab,
    BackTab,
    Backspace,
    Delete,
    Insert,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
    F(u8),
    Unknown,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Mods {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
}

impl Mods {
    pub const NONE: Mods = Mods {
        shift: false,
        ctrl: false,
        alt: false,
    };
    pub const ALT: Mods = Mods {
        shift: false,
        ctrl: false,
        alt: true,
    };
    pub const CTRL: Mods = Mods {
        shift: false,
        ctrl: true,
        alt: false,
    };
    pub const SHIFT: Mods = Mods {
        shift: true,
        ctrl: false,
        alt: false,
    };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Key {
    pub code: KeyCode,
    pub mods: Mods,
}

impl Key {
    pub const fn new(code: KeyCode) -> Key {
        Key {
            code,
            mods: Mods::NONE,
        }
    }

    pub fn with_mods(code: KeyCode, mods: Mods) -> Key {
        Key { code, mods }
    }

    pub fn char(c: char) -> Key {
        Key::new(KeyCode::Char(c))
    }

    /// The character typed, if any.
    pub fn as_char(&self) -> Option<char> {
        match self.code {
            KeyCode::Char(c) => Some(c),
            _ => None,
        }
    }

    /// Ctrl+letter reduced to its lowercase letter (for `Ctrl+C` etc.).
    pub fn ctrl_char(&self) -> Option<char> {
        if !self.mods.ctrl {
            return None;
        }
        match self.code {
            KeyCode::Char(c) => Some(c.to_ascii_lowercase()),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MouseKind {
    Down {
        x: u16,
        y: u16,
        button: MouseButton,
    },
    Up {
        x: u16,
        y: u16,
        button: MouseButton,
    },
    Drag {
        x: u16,
        y: u16,
    },
    Wheel {
        x: u16,
        y: u16,
        up: bool,
    },
    /// Pointer motion without a pressed button (hover).
    Move {
        x: u16,
        y: u16,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    Key(Key),
    Mouse(MouseKind),
    Resize(u16, u16),
    /// One 100 ms timer step.
    Tick,
}

/// Parse a scripted key list such as `"down,down,enter"`.
///
/// Tokens are comma- or whitespace-separated. Modifiers may be prefixed with
/// `+` (`shift+tab`, `alt+x`, `ctrl+c`). A single character token types that
/// character. `+` on its own is a literal plus sign.
pub fn parse_key_script(script: &str) -> Vec<Event> {
    let mut events = Vec::new();
    for raw in script
        .split([',', ' ', '\t', '\n'])
        .filter(|s| !s.is_empty())
    {
        if let Some(event) = parse_mouse_token(raw) {
            events.push(event);
            continue;
        }
        if let Some(event) = parse_token(raw) {
            events.push(event);
        }
    }
    events
}

fn parse_coords(text: &str) -> Option<(u16, u16)> {
    let (x, y) = text.split_once([':', 'x', '@'])?;
    Some((x.trim().parse().ok()?, y.trim().parse().ok()?))
}

fn parse_mouse_token(token: &str) -> Option<Event> {
    let lower = token.to_ascii_lowercase();
    if let Some(rest) = lower.strip_prefix("click:") {
        let (x, y) = parse_coords(rest)?;
        return Some(Event::Mouse(MouseKind::Down {
            x,
            y,
            button: MouseButton::Left,
        }));
    }
    if let Some(rest) = lower.strip_prefix("release:") {
        let (x, y) = parse_coords(rest)?;
        return Some(Event::Mouse(MouseKind::Up {
            x,
            y,
            button: MouseButton::Left,
        }));
    }
    if let Some(rest) = lower.strip_prefix("rclick:") {
        let (x, y) = parse_coords(rest)?;
        return Some(Event::Mouse(MouseKind::Down {
            x,
            y,
            button: MouseButton::Right,
        }));
    }
    if let Some(rest) = lower.strip_prefix("drag:") {
        let (x, y) = parse_coords(rest)?;
        return Some(Event::Mouse(MouseKind::Drag { x, y }));
    }
    if let Some(rest) = lower.strip_prefix("move:") {
        let (x, y) = parse_coords(rest)?;
        return Some(Event::Mouse(MouseKind::Move { x, y }));
    }
    if let Some(rest) = lower.strip_prefix("wheel:") {
        let (x, y) = parse_coords(rest)?;
        return Some(Event::Mouse(MouseKind::Wheel { x, y, up: true }));
    }
    match lower.as_str() {
        "wheelup" => Some(Event::Mouse(MouseKind::Wheel {
            x: 0,
            y: 0,
            up: true,
        })),
        "wheeldown" => Some(Event::Mouse(MouseKind::Wheel {
            x: 0,
            y: 0,
            up: false,
        })),
        _ => None,
    }
}

fn parse_token(token: &str) -> Option<Event> {
    let lower = token.to_ascii_lowercase();
    let mut mods = Mods::NONE;
    let mut rest = lower.as_str();

    loop {
        if let Some(r) = rest.strip_prefix("shift+") {
            mods.shift = true;
            rest = r;
        } else if let Some(r) = rest.strip_prefix("ctrl+") {
            mods.ctrl = true;
            rest = r;
        } else if let Some(r) = rest.strip_prefix("alt+") {
            mods.alt = true;
            rest = r;
        } else {
            break;
        }
    }

    // Preserve the case of a single typed character.
    let typed_char = if token.chars().count() == 1 {
        token.chars().next()
    } else {
        None
    };

    let code = match rest {
        "enter" | "return" | "cr" => KeyCode::Enter,
        "esc" | "escape" => KeyCode::Esc,
        "tab" => KeyCode::Tab,
        "backtab" | "shift-tab" => KeyCode::BackTab,
        "backspace" | "bksp" | "bs" => KeyCode::Backspace,
        "delete" | "del" => KeyCode::Delete,
        "insert" | "ins" => KeyCode::Insert,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "pgup" | "pageup" => KeyCode::PageUp,
        "pgdn" | "pgdown" | "pagedown" => KeyCode::PageDown,
        "space" => KeyCode::Char(' '),
        "plus" => KeyCode::Char('+'),
        other => {
            if let Some(n) = other.strip_prefix('f').and_then(|n| n.parse::<u8>().ok()) {
                KeyCode::F(n)
            } else if let Some(c) = typed_char {
                KeyCode::Char(c)
            } else {
                KeyCode::Char(other.chars().next()?)
            }
        }
    };
    Some(Event::Key(Key { code, mods }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_script() {
        let events = parse_key_script("down,down,enter");
        assert_eq!(events.len(), 3);
        assert_eq!(events[0], Event::Key(Key::new(KeyCode::Down)));
        assert_eq!(events[2], Event::Key(Key::new(KeyCode::Enter)));
    }

    #[test]
    fn modifiers_and_chars() {
        let events = parse_key_script("alt+x, shift+tab, ctrl+c, A, space, plus");
        assert_eq!(
            events[0],
            Event::Key(Key::with_mods(KeyCode::Char('x'), Mods::ALT))
        );
        assert_eq!(
            events[1],
            Event::Key(Key::with_mods(KeyCode::Tab, Mods::SHIFT))
        );
        assert_eq!(
            events[2],
            Event::Key(Key::with_mods(KeyCode::Char('c'), Mods::CTRL))
        );
        assert_eq!(events[3], Event::Key(Key::char('A')));
        assert_eq!(events[4], Event::Key(Key::char(' ')));
        assert_eq!(events[5], Event::Key(Key::char('+')));
    }

    #[test]
    fn function_keys() {
        assert_eq!(
            parse_key_script("f1")[0],
            Event::Key(Key::new(KeyCode::F(1)))
        );
        assert_eq!(
            parse_key_script("F12")[0],
            Event::Key(Key::new(KeyCode::F(12)))
        );
    }

    #[test]
    fn mouse_scripts() {
        let events = parse_key_script("click:10x4,rclick:1x1,drag:12x5,move:3x4,wheeldown");
        assert_eq!(events.len(), 5);
        assert_eq!(
            events[0],
            Event::Mouse(MouseKind::Down {
                x: 10,
                y: 4,
                button: MouseButton::Left
            })
        );
        assert_eq!(events[3], Event::Mouse(MouseKind::Move { x: 3, y: 4 }));
        assert!(matches!(
            events[4],
            Event::Mouse(MouseKind::Wheel { up: false, .. })
        ));
    }

    #[test]
    fn empty_script() {
        assert!(parse_key_script("").is_empty());
        assert!(parse_key_script(" , ").is_empty());
    }
}
