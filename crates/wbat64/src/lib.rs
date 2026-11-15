//! `wbat64` — dialog boxes for batch files and shell scripts.
//!
//! The crate re-exports the pieces of the engine and offers a small embedding
//! API ([`Layout`], [`Dialog`], [`Outcome`]) on top of the same code the command
//! line tool uses.

use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use wbat64_core::{decode, find_unit, parse_layout, Config, Control, Pos, Style, Unit};
use wbat64_tui::{run_dialog, DialogOptions, Session, SessionOptions};

pub use wbat64_core::{Color, ControlKind, EmitMode, Encoding, Frame, Line, LineKind, Span, Theme};
pub use wbat64_core::{Error, Result};
pub use wbat64_tui::{Key, KeyCode, MouseButton, MouseKind, Outcome};

pub mod cli {
    pub use wbat64_cli::*;
}

/// Run the command-line interface and return the process exit code.
pub fn run_cli() -> i32 {
    wbat64_cli::run()
}

/// A parsed layout unit.
#[derive(Clone, Debug)]
pub struct Layout {
    unit: Unit,
}

impl Layout {
    /// Load a unit from a layout file (`@file:label` without the `@`).
    pub fn from_file(path: impl AsRef<Path>, label: &str) -> Result<Layout> {
        let path = path.as_ref();
        let bytes = std::fs::read(path)
            .map_err(|e| Error::io(format!("reading '{}'", path.display()), e))?;
        let text = decode(&bytes, Encoding::Auto);
        Layout::from_str(&text, label)
    }

    /// Parse a unit from layout text.
    pub fn from_str(text: &str, label: &str) -> Result<Layout> {
        let (units, errors) = parse_layout(text, "<memory>", '~');
        if let Some(error) = errors.first() {
            return Err(Error::msg(error.to_string()));
        }
        let unit = find_unit(&units, label)
            .ok_or_else(|| Error::msg(format!("label ':{label}' not found")))?
            .clone();
        Ok(Layout { unit })
    }

    pub fn unit(&self) -> &Unit {
        &self.unit
    }

    pub fn label(&self) -> &str {
        &self.unit.label
    }
}

/// The result of running a [`Dialog`].
#[derive(Clone, Debug)]
pub struct DialogOutcome {
    pub outcome: Outcome,
    pub button_index: Option<usize>,
    pub button_name: Option<String>,
    /// Input values by variable name.
    pub inputs: HashMap<String, String>,
    pub checkboxes: Vec<bool>,
    pub radio: Option<usize>,
}

/// Builder for an interactive dialog.
pub struct Dialog {
    layout: Layout,
    vars: HashMap<String, String>,
    timeout: Option<Duration>,
    pos: Option<Pos>,
    colors: Option<Style>,
    title: Option<String>,
}

impl Dialog {
    pub fn new(layout: Layout) -> Dialog {
        Dialog {
            layout,
            vars: HashMap::new(),
            timeout: None,
            pos: None,
            colors: None,
            title: None,
        }
    }

    /// Provide an initial value for an input, checkbox (`WCB1`..), or radio (`WRB`).
    pub fn var(mut self, name: &str, value: impl Into<String>) -> Dialog {
        self.vars.insert(name.to_string(), value.into());
        self
    }

    pub fn timeout(mut self, timeout: Duration) -> Dialog {
        self.timeout = Some(timeout);
        self
    }

    pub fn position(mut self, pos: Pos) -> Dialog {
        self.pos = Some(pos);
        self
    }

    pub fn colors(mut self, colors: Style) -> Dialog {
        self.colors = Some(colors);
        self
    }

    pub fn title(mut self, title: impl Into<String>) -> Dialog {
        self.title = Some(title.into());
        self
    }

    /// Draw the dialog on the console device and wait for it to close.
    pub fn run(self) -> Result<DialogOutcome> {
        let Dialog {
            layout,
            vars,
            timeout,
            pos,
            colors,
            title,
        } = self;

        let mut config = Config::native_defaults();
        let mut warnings = Vec::new();
        config.apply_env(&|key| std::env::var(format!("W{key}")).ok(), &mut warnings);

        let lookup = |name: &str| vars.get(name).cloned().or_else(|| std::env::var(name).ok());
        let unit = layout.unit.interpolate(&lookup);
        if let Some(error) = unit.validate().first() {
            return Err(Error::msg(error.to_string()));
        }
        let mut initial = HashMap::new();
        for control in unit.inputs() {
            if let Control::Input { var, .. } = control {
                if let Some(value) = lookup(var) {
                    initial.insert(var.clone(), value);
                }
            }
        }
        for (name, value) in &vars {
            initial.insert(name.clone(), value.clone());
        }

        let options = SessionOptions {
            mouse: config.mouse,
            keys: None,
            headless_size: None,
            dump_screen: false,
        };
        let mut session = Session::open(&config, &options)?;
        let dialog_options = DialogOptions {
            title,
            pos,
            colors,
            timeout: timeout.map(|d| d.as_secs().min(3600) as u32),
            bar: None,
            fix: false,
            mouse: config.mouse,
            close: unit.close,
            initial,
            max_width: None,
            strict_width: false,
            menu: false,
        };
        let result = run_dialog(&mut session, &config, &unit, &dialog_options)?;
        drop(session);

        Ok(DialogOutcome {
            outcome: result.outcome,
            button_index: result.button_index,
            button_name: result.button_name,
            inputs: result.inputs.into_iter().collect(),
            checkboxes: result.checkboxes,
            radio: result.radio,
        })
    }
}
