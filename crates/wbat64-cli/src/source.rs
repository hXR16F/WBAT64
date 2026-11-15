//! Resolving layout references and reading layout/list files.

use std::path::{Path, PathBuf};

use wbat64_core::{
    decode_with_detection, find_unit as find_unit_in, parse_layout, Config, Encoding, Error,
    Result, Unit,
};

/// A `@file:label`, `:label`, `@+file:label` or plain path reference.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceRef {
    pub path: String,
    pub label: Option<String>,
    /// Resolve relative to the executable's directory.
    pub plus: bool,
}

impl SourceRef {
    /// Parse a source token. Returns `None` for anything that is not a source.
    pub fn parse(token: &str) -> Option<SourceRef> {
        let text = token.trim();
        if text.is_empty() {
            return None;
        }
        if let Some(rest) = text.strip_prefix('@') {
            let plus = rest.starts_with('+');
            let body = rest.strip_prefix('+').unwrap_or(rest);
            return SourceRef::from_body(body, plus);
        }
        if let Some(rest) = text.strip_prefix('+') {
            if rest.starts_with(|c: char| c.is_ascii_digit()) {
                return None;
            }
            return SourceRef::from_body(rest, true);
        }
        if let Some(rest) = text.strip_prefix(':') {
            let label = rest.trim();
            if label.is_empty() {
                return None;
            }
            return Some(SourceRef {
                path: String::new(),
                label: Some(label.to_string()),
                plus: false,
            });
        }
        None
    }

    fn from_body(body: &str, plus: bool) -> Option<SourceRef> {
        let body = body.trim();
        if body.is_empty() {
            return None;
        }
        if let Some(index) = body.rfind(':') {
            let path = &body[..index];
            let label = &body[index + 1..];
            let label_ok =
                !label.is_empty() && !label.contains(['\\', '/', '.']) && !path.ends_with(':');
            if label_ok {
                return Some(SourceRef {
                    path: path.to_string(),
                    label: Some(label.to_string()),
                    plus,
                });
            }
        }
        Some(SourceRef {
            path: body.to_string(),
            label: None,
            plus,
        })
    }

    pub fn display(&self) -> String {
        if self.path.is_empty() {
            return self.label.clone().unwrap_or_default();
        }
        match &self.label {
            Some(label) => format!("{}:{}", self.path, label),
            None => self.path.clone(),
        }
    }
}

/// A parsed layout document.
pub struct LayoutDocument {
    pub units: Vec<Unit>,
    pub errors: Vec<Error>,
    pub file: String,
}

fn candidate_paths(path: PathBuf) -> Vec<PathBuf> {
    let mut paths = vec![path.clone()];
    if path.extension().is_none() {
        paths.push(path.with_extension("BAT"));
        paths.push(path.with_extension("bat"));
        paths.push(path.with_extension("txt"));
    }
    paths
}

/// Resolve a source reference to an existing path (plus a display name).
pub fn resolve_path(
    reference: &SourceRef,
    config: &Config,
    cwd: &Path,
    exe_dir: Option<&Path>,
) -> Result<(PathBuf, String)> {
    let (raw, plus) = if reference.path.is_empty() {
        (
            config.source_file.clone(),
            config.source_file.starts_with('+'),
        )
    } else {
        (reference.path.clone(), reference.plus)
    };
    let raw = raw.strip_prefix('+').unwrap_or(&raw).to_string();
    if raw.is_empty() {
        return Err(Error::msg("no source file configured (SourceFile)"));
    }
    let mut path = PathBuf::from(&raw);
    if plus {
        if let Some(dir) = exe_dir {
            path = dir.join(&path);
        } else if path.is_relative() {
            path = cwd.join(&path);
        }
    } else if path.is_relative() {
        path = cwd.join(&path);
    }

    let candidates = candidate_paths(path);
    for candidate in &candidates {
        if candidate.is_file() {
            let display = if plus {
                format!("+{}", raw)
            } else {
                raw.clone()
            };
            return Ok((candidate.clone(), display));
        }
    }
    let tried = candidates
        .iter()
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>()
        .join(", ");
    let origin = if reference.path.is_empty() {
        " (from SourceFile in wbat.ini or wbat64.toml)"
    } else {
        ""
    };
    Err(Error::msg(format!(
        "cannot open layout file '{raw}'{origin} (tried {tried})"
    )))
}

fn read_text(
    path: &Path,
    config: &Config,
    encoding: Option<Encoding>,
) -> Result<(String, Encoding)> {
    let bytes =
        std::fs::read(path).map_err(|e| Error::io(format!("reading '{}'", path.display()), e))?;
    Ok(decode_with_detection(
        &bytes,
        encoding.unwrap_or(config.encoding),
    ))
}

/// Read and parse a layout file.
pub fn read_layout_document(
    reference: &SourceRef,
    config: &Config,
    encoding: Option<Encoding>,
    cwd: &Path,
    exe_dir: Option<&Path>,
) -> Result<LayoutDocument> {
    let (path, display) = resolve_path(reference, config, cwd, exe_dir)?;
    let (text, _) = read_text(&path, config, encoding)?;
    let (units, errors) = parse_layout(&text, &display, config.hi_symbol);
    Ok(LayoutDocument {
        units,
        errors,
        file: display,
    })
}

/// Find the requested unit, with WBAT-style diagnostics.
pub fn select_unit<'a>(document: &'a LayoutDocument, label: Option<&str>) -> Result<&'a Unit> {
    match label {
        Some(label) => find_unit_in(&document.units, label).ok_or_else(|| {
            Error::msg(format!("label ':{label}' not found in '{}'", document.file))
        }),
        None => {
            if document.units.len() == 1 {
                Ok(&document.units[0])
            } else {
                Err(Error::msg(format!(
                    "'{}' contains {} units; give a label with @file:label",
                    document.file,
                    document.units.len()
                )))
            }
        }
    }
}

/// Read a list file into lines.
pub fn read_list_lines(
    reference: &SourceRef,
    config: &Config,
    encoding: Option<Encoding>,
    cwd: &Path,
    exe_dir: Option<&Path>,
) -> Result<Vec<String>> {
    let (path, _) = resolve_path(reference, config, cwd, exe_dir)?;
    let (text, _) = read_text(&path, config, encoding)?;
    Ok(split_lines(&text))
}

pub fn split_lines(text: &str) -> Vec<String> {
    let mut lines: Vec<String> = text
        .split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line).to_string())
        .collect();
    if lines.last().map(|l| l.is_empty()).unwrap_or(false) {
        lines.pop();
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_references() {
        let r = SourceRef::parse("@dialogs.txt:signup").unwrap();
        assert_eq!(r.path, "dialogs.txt");
        assert_eq!(r.label.as_deref(), Some("signup"));
        assert!(!r.plus);

        let r = SourceRef::parse(":main").unwrap();
        assert_eq!(r.path, "");
        assert_eq!(r.label.as_deref(), Some("main"));

        let r = SourceRef::parse("@+C:\\dos\\demo.bat:menu").unwrap();
        assert!(r.plus);
        assert_eq!(r.path, "C:\\dos\\demo.bat");
        assert_eq!(r.label.as_deref(), Some("menu"));

        // A Windows drive colon is not a label separator.
        let r = SourceRef::parse("@C:\\dos\\demo.bat").unwrap();
        assert_eq!(r.path, "C:\\dos\\demo.bat");
        assert_eq!(r.label, None);

        assert_eq!(SourceRef::parse("+2"), None);
        assert_eq!(SourceRef::parse("plain.txt"), None);
    }

    #[test]
    fn splits_lines_like_wbat() {
        assert_eq!(split_lines("a\r\nb\n"), vec!["a", "b"]);
        assert_eq!(split_lines("a\n\nb"), vec!["a", "", "b"]);
        assert!(split_lines("").is_empty());
    }
}
