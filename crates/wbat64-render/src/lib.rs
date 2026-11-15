//! `wbat64-render` — the cell buffer and painters.
//!
//! The renderer keeps two screens: `prev` (what the terminal is believed to
//! show) and `next` (what should be shown). Only changed cells are emitted, so
//! a dialog can repaint cheaply on every key press.
//!
//! Cells drawn by wbat64 are *known*; cells wbat64 has never touched are
//! *unknown*, which lets a box be removed without destroying unrelated output
//! the program did not paint itself.

#![forbid(unsafe_code)]

pub mod ansi;
pub mod cell;
pub mod paint;

pub use ansi::{
    encode_clear, encode_cursor, encode_hide_cursor, encode_show_cursor, encode_style_reset,
    encode_updates,
};
pub use cell::{diff, Cell, Screen, Update};
pub use paint::{draw_close_button, draw_divider, draw_frame, draw_shadow, draw_title, word_wrap};

pub use wbat64_core::{Color, Frame, Style};
