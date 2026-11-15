//! `wbat64-tui` — the interactive terminal layer.
//!
//! This crate owns the console device, translates terminal events, and runs the
//! dialog and list state machines. It never writes UI to stdout: results are
//! the CLI's job.

pub mod dialog;
pub mod event;
pub mod input;
pub mod list;
pub mod session;
pub mod term;

pub use dialog::{run_dialog, DialogOptions, DialogResult, Outcome};
pub use event::{parse_key_script, Event, Key, KeyCode, Mods, MouseButton, MouseKind};
pub use input::InputState;
pub use list::{run_list, ListOptions, ListOutcome};
pub use session::{Session, SessionOptions};
pub use term::{Backend, ConsoleBackend, HeadlessBackend};
