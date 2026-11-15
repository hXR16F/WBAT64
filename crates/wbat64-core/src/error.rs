//! Errors shared by every wbat64 crate.

use std::fmt;
use std::io;

/// A wbat64 error.
///
/// `Layout` carries a source position so that `wbat64 check` can print
/// `file:line:col: message` diagnostics. Every error ultimately maps to exit
/// code 255 at the CLI boundary.
#[derive(Debug)]
pub enum Error {
    /// A plain message, printed as `wbat64: <message>`.
    Message(String),
    /// An I/O failure with a human readable context.
    Io { context: String, source: io::Error },
    /// A layout problem with a file/line/column position.
    Layout {
        file: String,
        line: usize,
        col: usize,
        message: String,
    },
}

impl Error {
    pub fn msg(message: impl Into<String>) -> Self {
        Error::Message(message.into())
    }

    pub fn io(context: impl Into<String>, source: io::Error) -> Self {
        Error::Io {
            context: context.into(),
            source,
        }
    }

    pub fn at(
        file: impl Into<String>,
        line: usize,
        col: usize,
        message: impl Into<String>,
    ) -> Self {
        Error::Layout {
            file: file.into(),
            line,
            col,
            message: message.into(),
        }
    }

    /// Contextualize an I/O error without losing the message.
    pub fn context(self, context: impl Into<String>) -> Self {
        match self {
            Error::Io { source, .. } => Error::Io {
                context: context.into(),
                source,
            },
            other => other,
        }
    }

    /// The process exit code for this error (always 255).
    pub fn exit_code(&self) -> i32 {
        255
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Message(m) => f.write_str(m),
            Error::Io { context, source } => write!(f, "{context}: {source}"),
            Error::Layout {
                file,
                line,
                col,
                message,
            } => write!(f, "{file}:{line}:{col}: {message}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl From<io::Error> for Error {
    fn from(value: io::Error) -> Self {
        Error::Io {
            context: "I/O error".to_string(),
            source: value,
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_error_display_matches_check_format() {
        let e = Error::at("broken.txt", 3, 9, "unterminated control '['");
        assert_eq!(e.to_string(), "broken.txt:3:9: unterminated control '['");
    }
}
