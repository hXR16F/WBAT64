//! `wbat64-core` — the parts of wbat64 that do not touch a terminal.
//!
//! This crate contains the tokenizer and layout AST, the color expression
//! parser, the configuration readers (WBAT-style INI and the native TOML
//! subset), variable interpolation, screen geometry, the result model and the
//! result emitters (batch, PowerShell, POSIX shell, JSON).
//!
//! Everything here is pure: no file system access, no environment access, no
//! terminal access. Callers pass in the strings, paths and variable lookups.

#![forbid(unsafe_code)]

pub mod color;
pub mod config;
pub mod emit;
pub mod encoding;
pub mod error;
pub mod geometry;
pub mod ini;
pub mod interp;
pub mod layout;
pub mod text;
pub mod toml_lite;

pub use color::{parse_colors, Color, Style};
pub use config::{Config, Frame, LoadOptions, Theme};
pub use emit::{ButtonResult, EmitMode, ListResult, Results};
pub use encoding::{decode, decode_with_detection, Encoding};
pub use error::{Error, Result};
pub use geometry::{parse_pos, parse_size, pos_to_rect, resolve_pos, Pos, Rect, Size};
pub use interp::parse_bool;
pub use layout::{
    classify_control, find_unit, parse_layout, quick_box, span_positions, Control, ControlKind,
    Line, LineKind, Span, Unit,
};
pub use text::{char_width, fit, str_width, truncate, truncate_pad};
