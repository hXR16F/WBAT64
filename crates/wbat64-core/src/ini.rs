//! A small WBAT-compatible INI reader.
//!
//! The original `WBAT.INI` puts a frame pattern in quotes that itself contains
//! semicolons and spaces, so a naive "strip everything after `;`" breaks it.
//! This reader is quote-aware: `;` starts a comment only outside quotes, and a
//! fully quoted value is unquoted.

/// One `key=value` pair, preserving its source line for diagnostics.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IniEntry {
    pub section: String,
    pub key: String,
    pub value: String,
    pub line: usize,
}

/// A parsed INI file. Section and key lookups are case-insensitive.
#[derive(Clone, Debug, Default)]
pub struct Ini {
    entries: Vec<IniEntry>,
}

fn strip_comment(line: &str) -> &str {
    let mut in_double = false;
    let mut in_single = false;
    for (i, ch) in line.char_indices() {
        match ch {
            '"' if !in_single => in_double = !in_double,
            '\'' if !in_double => in_single = !in_single,
            ';' if !in_double && !in_single => return &line[..i],
            _ => {}
        }
    }
    line
}

fn unquote(value: &str) -> String {
    let v = value.trim();
    if v.len() >= 2 {
        let bytes = v.as_bytes();
        let first = bytes[0];
        let last = bytes[v.len() - 1];
        if (first == b'"' && last == b'"') || (first == b'\'' && last == b'\'') {
            let inner = &v[1..v.len() - 1];
            // Allow "" inside a quoted value to mean a single quote.
            let q = first as char;
            let doubled = format!("{q}{q}");
            return inner.replace(&doubled, &q.to_string());
        }
    }
    v.to_string()
}

impl Ini {
    pub fn parse(text: &str) -> Ini {
        let mut entries = Vec::new();
        let mut section = String::new();
        for (idx, raw) in text.lines().enumerate() {
            let line = strip_comment(raw).trim();
            if line.is_empty() {
                continue;
            }
            if line.starts_with('[') {
                if let Some(end) = line.find(']') {
                    section = line[1..end].trim().to_ascii_lowercase();
                }
                continue;
            }
            if let Some((key, value)) = line.split_once('=') {
                let key = key.trim().trim_matches('"').to_ascii_lowercase();
                if key.is_empty() {
                    continue;
                }
                entries.push(IniEntry {
                    section: section.clone(),
                    key,
                    value: unquote(value),
                    line: idx + 1,
                });
            }
        }
        Ini { entries }
    }

    pub fn get(&self, section: &str, key: &str) -> Option<&str> {
        let section = section.to_ascii_lowercase();
        let key = key.to_ascii_lowercase();
        self.entries
            .iter()
            .find(|e| e.section == section && e.key == key)
            .map(|e| e.value.as_str())
    }

    /// Look up a key in any section (used as a fallback for sloppy INI files).
    pub fn get_any(&self, key: &str) -> Option<&str> {
        let key = key.to_ascii_lowercase();
        self.entries
            .iter()
            .find(|e| e.key == key)
            .map(|e| e.value.as_str())
    }

    pub fn entries(&self) -> &[IniEntry] {
        &self.entries
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
[general]
ELvar=1              ; set ? = exit code
rightESC=1           ; right-click outside = ESC
BoxFrame="┌─┐│ │└─┘"
CheckSymbol="x"      ; or a decimal code
SourceFile=WBAT.TXT
[color]
Screen=   black on +green
Box=      black on +white
HotKey=   red
Shadow=   +black     ; NONE = off
"#;

    #[test]
    fn reads_sections_and_keys() {
        let ini = Ini::parse(SAMPLE);
        assert_eq!(ini.get("general", "ELvar"), Some("1"));
        assert_eq!(ini.get("GENERAL", "rightesc"), Some("1"));
        assert_eq!(ini.get("general", "SourceFile"), Some("WBAT.TXT"));
        assert_eq!(ini.get("color", "Screen"), Some("black on +green"));
        assert_eq!(ini.get("color", "Box"), Some("black on +white"));
    }

    #[test]
    fn quoted_frame_keeps_semicolons_and_spaces() {
        let ini = Ini::parse(SAMPLE);
        assert_eq!(ini.get("general", "BoxFrame"), Some("┌─┐│ │└─┘"));
    }

    #[test]
    fn quoted_blank_frame_keeps_its_spaces() {
        let ini = Ini::parse("[general]\nBoxFrame=\"         \"\n");
        assert_eq!(ini.get("general", "BoxFrame"), Some("         "));
    }

    #[test]
    fn quoted_hash_and_comment() {
        let ini = Ini::parse("[x]\na=\"v ; not comment\" ; real comment\n");
        assert_eq!(ini.get("x", "a"), Some("v ; not comment"));
    }

    #[test]
    fn missing_is_none() {
        let ini = Ini::parse(SAMPLE);
        assert_eq!(ini.get("general", "nope"), None);
        assert_eq!(ini.get("nope", "ELvar"), None);
    }
}
