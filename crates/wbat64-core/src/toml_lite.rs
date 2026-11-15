//! A deliberately small TOML subset for `wbat64.toml`.
//!
//! Supporting the whole TOML grammar would mean pulling in a dependency for
//! five keys. This reader understands tables (`[general]`, `[colors]`),
//! `key = value` pairs, basic and literal strings, booleans, integers, floats,
//! comments and one-dimensional arrays of scalars. Anything else is reported
//! as an error with a line number.

use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Str(String),
    Bool(bool),
    Int(i64),
    Float(f64),
    Array(Vec<Value>),
}

impl Value {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            Value::Str(s) => crate::interp::parse_bool(s),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Value::Int(i) => Some(*i),
            Value::Str(s) => s.parse().ok(),
            _ => None,
        }
    }
}

/// Parsed tables: `section -> key -> value` (section/key lower-cased).
#[derive(Clone, Debug, Default)]
pub struct Toml {
    tables: BTreeMap<String, BTreeMap<String, Value>>,
}

/// Strip a `#` comment that is outside quotes.
fn strip_comment(line: &str) -> &str {
    let mut in_double = false;
    let mut in_single = false;
    let mut escaped = false;
    for (i, ch) in line.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match ch {
            '\\' if in_double => escaped = true,
            '"' if !in_single => in_double = !in_double,
            '\'' if !in_double => in_single = !in_single,
            '#' if !in_double && !in_single => return &line[..i],
            _ => {}
        }
    }
    line
}

fn unescape_basic(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('"') => out.push('"'),
            Some('\\') => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

fn parse_string(input: &str) -> Result<(String, &str), String> {
    let s = input.trim_start();
    let mut chars = s.char_indices();
    let Some((_, quote)) = chars.next() else {
        return Err("expected a value".to_string());
    };
    if quote != '"' && quote != '\'' {
        return Err(format!("expected a quoted string, found '{quote}'"));
    }
    let escaped = quote == '"';
    let mut out = String::new();
    let mut escaped_char = false;
    let mut end = None;
    for (i, ch) in s[1..].char_indices() {
        if escaped_char {
            escaped_char = false;
            continue;
        }
        if escaped && ch == '\\' {
            escaped_char = true;
            continue;
        }
        if ch == quote {
            end = Some(1 + i);
            break;
        }
    }
    let Some(end_idx) = end else {
        return Err("unterminated string".to_string());
    };
    let body = &s[1..end_idx];
    if escaped {
        out.push_str(&unescape_basic(body));
    } else {
        out.push_str(body);
    }
    Ok((out, &s[end_idx + 1..]))
}

fn parse_scalar(input: &str) -> Result<Value, String> {
    let t = input.trim();
    if t.is_empty() {
        return Err("missing value".to_string());
    }
    if t.starts_with('"') || t.starts_with('\'') {
        let (s, rest) = parse_string(t)?;
        if !rest.trim().is_empty() {
            return Err(format!(
                "trailing characters after string: '{}'",
                rest.trim()
            ));
        }
        return Ok(Value::Str(s));
    }
    match t {
        "true" => return Ok(Value::Bool(true)),
        "false" => return Ok(Value::Bool(false)),
        _ => {}
    }
    if let Ok(i) = t.replace('_', "").parse::<i64>() {
        return Ok(Value::Int(i));
    }
    if let Ok(f) = t.parse::<f64>() {
        return Ok(Value::Float(f));
    }
    // Bare words are accepted as strings (TOML would reject them, but being
    // lenient here makes hand-written configs friendlier).
    if t.chars()
        .all(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | '.' | '/' | '#'))
    {
        return Ok(Value::Str(t.to_string()));
    }
    Err(format!("cannot parse value '{t}'"))
}

fn parse_value(input: &str) -> Result<Value, String> {
    let t = input.trim();
    if t.starts_with('[') {
        if !t.ends_with(']') {
            return Err("unterminated array".to_string());
        }
        let inner = &t[1..t.len() - 1];
        let mut items = Vec::new();
        let mut current = String::new();
        let mut in_string: Option<char> = None;
        let mut escaped = false;
        for ch in inner.chars() {
            if let Some(q) = in_string {
                current.push(ch);
                if escaped {
                    escaped = false;
                } else if ch == '\\' && q == '"' {
                    escaped = true;
                } else if ch == q {
                    in_string = None;
                }
                continue;
            }
            match ch {
                '"' | '\'' => {
                    in_string = Some(ch);
                    current.push(ch);
                }
                ',' => {
                    if !current.trim().is_empty() {
                        items.push(parse_scalar(&current)?);
                    }
                    current.clear();
                }
                _ => current.push(ch),
            }
        }
        if !current.trim().is_empty() {
            items.push(parse_scalar(&current)?);
        }
        return Ok(Value::Array(items));
    }
    parse_scalar(t)
}

impl Toml {
    pub fn parse(text: &str) -> Result<Toml, String> {
        let mut tables: BTreeMap<String, BTreeMap<String, Value>> = BTreeMap::new();
        let mut section = String::new();
        tables.insert(section.clone(), BTreeMap::new());
        for (idx, raw) in text.lines().enumerate() {
            let line_no = idx + 1;
            let line = strip_comment(raw).trim();
            if line.is_empty() {
                continue;
            }
            if line.starts_with('[') {
                if !line.ends_with(']') {
                    return Err(format!("line {line_no}: unterminated table header"));
                }
                section = line[1..line.len() - 1].trim().to_ascii_lowercase();
                tables.entry(section.clone()).or_default();
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                return Err(format!("line {line_no}: expected `key = value`"));
            };
            let key = key.trim().trim_matches('"').to_ascii_lowercase();
            if key.is_empty() {
                return Err(format!("line {line_no}: empty key"));
            }
            let value = parse_value(value).map_err(|e| format!("line {line_no}: {e}"))?;
            tables
                .entry(section.clone())
                .or_default()
                .insert(key, value);
        }
        Ok(Toml { tables })
    }

    pub fn get(&self, section: &str, key: &str) -> Option<&Value> {
        self.tables
            .get(&section.to_ascii_lowercase())
            .and_then(|t| t.get(&key.to_ascii_lowercase()))
    }

    pub fn get_str(&self, section: &str, key: &str) -> Option<&str> {
        self.get(section, key).and_then(Value::as_str)
    }

    pub fn get_bool(&self, section: &str, key: &str) -> Option<bool> {
        self.get(section, key).and_then(Value::as_bool)
    }

    pub fn is_empty(&self) -> bool {
        self.tables.values().all(|t| t.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r##"
# wbat64.toml
[general]
frame = "rounded"          # ascii | single | double | rounded
mouse = true
escape_discards = true
max_width = 100

[colors]
screen = "#c8e8c0 on #1e1e1e"
box    = "black on +white"
bar    = "+white on #b00020"
shadow = "none"

[extra]
list = ["a", "b", 'c']
"##;

    #[test]
    fn reads_tables() {
        let t = Toml::parse(SAMPLE).unwrap();
        assert_eq!(t.get_str("general", "frame"), Some("rounded"));
        assert_eq!(t.get_bool("general", "mouse"), Some(true));
        assert_eq!(
            t.get("general", "max_width").and_then(Value::as_i64),
            Some(100)
        );
        assert_eq!(t.get_str("colors", "bar"), Some("+white on #b00020"));
        assert_eq!(t.get_str("colors", "shadow"), Some("none"));
        assert_eq!(
            t.get("extra", "list"),
            Some(&Value::Array(vec![
                Value::Str("a".into()),
                Value::Str("b".into()),
                Value::Str("c".into())
            ]))
        );
    }

    #[test]
    fn rejects_broken_syntax() {
        assert!(Toml::parse("[general]\nframe\n").is_err());
        assert!(Toml::parse("[general]\nframe = \"x\n").is_err());
        assert!(Toml::parse("[general\n").is_err());
    }

    #[test]
    fn hash_inside_string_is_not_a_comment() {
        let t = Toml::parse("[colors]\nscreen = \"#ff0000 on #00ff00\"\n").unwrap();
        assert_eq!(t.get_str("colors", "screen"), Some("#ff0000 on #00ff00"));
    }
}
