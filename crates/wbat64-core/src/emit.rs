//! The result model and its emitters.
//!
//! A child process cannot change its parent's environment, so wbat64 follows the
//! WBAT calling convention: the UI is drawn on the console device and the
//! *results* are written to stdout (or to a file) as `set`/`export` lines that
//! the caller evaluates. JSON is also available for native callers.

use serde_json::json;

use crate::error::{Error, Result};

/// One selected button.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ButtonResult {
    /// 1-based button number.
    pub index: usize,
    /// The WBAT result word.
    pub name: String,
}

/// One list selection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListResult {
    /// 1-based item number.
    pub index: usize,
    pub item: String,
    /// Further selections for `--multi`.
    pub extra: Vec<String>,
}

/// Everything the CLI knows when the last command finishes.
#[derive(Clone, Debug, Default)]
pub struct Results {
    pub exit_code: i32,
    pub escaped: bool,
    /// The selected button as one word, or the full selected list line.
    pub wbat: Option<String>,
    pub button: Option<ButtonResult>,
    pub list: Option<ListResult>,
    pub inputs: Vec<(String, String)>,
    pub checkboxes: Vec<bool>,
    pub radio: Option<usize>,
    /// Set variable `?` to the exit code (`ELvar`).
    pub set_exit_var: bool,
    /// Overrides the value of `?` (lists use the item number).
    pub exit_var: Option<String>,
    /// Prefix applied to emitted variable names (`--prefix`).
    pub prefix: String,
    /// The caller uses `EnableDelayedExpansion`.
    pub delayed: bool,
}

/// Output format for results.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum EmitMode {
    /// Exit code only.
    #[default]
    None,
    /// `set "NAME=value"` lines (also used for `--emit-file`).
    Bat,
    /// `$env:NAME = 'value'` lines.
    Ps1,
    /// `export NAME='value'` lines.
    Sh,
    /// One JSON object.
    Json,
}

impl EmitMode {
    pub fn parse(name: &str) -> Option<EmitMode> {
        match name.trim().to_ascii_lowercase().as_str() {
            "none" => Some(EmitMode::None),
            "bat" | "cmd" | "batch" => Some(EmitMode::Bat),
            "ps1" | "powershell" | "pwsh" => Some(EmitMode::Ps1),
            "sh" | "bash" | "shell" => Some(EmitMode::Sh),
            "json" => Some(EmitMode::Json),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            EmitMode::None => "none",
            EmitMode::Bat => "bat",
            EmitMode::Ps1 => "ps1",
            EmitMode::Sh => "sh",
            EmitMode::Json => "json",
        }
    }
}

/// A valid variable name for every supported shell.
pub fn is_valid_var_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '?' | '@' | '$' | '-'))
}

fn check_name(name: &str) -> Result<()> {
    if is_valid_var_name(name) {
        Ok(())
    } else {
        Err(Error::msg(format!("invalid variable name '{name}'")))
    }
}

/// Escape a value for `set "NAME=value"` inside a `.bat` file run via `call`.
///
/// `%` becomes `%%`, CR/LF are dropped, and under `EnableDelayedExpansion` `!`
/// becomes `^^!`. Inside the quotes `& | < > ^` are literal.
pub fn escape_bat(value: &str, delayed: bool) -> String {
    let mut out = String::with_capacity(value.len() + 8);
    for ch in value.chars() {
        match ch {
            '%' => out.push_str("%%"),
            '\r' | '\n' => {}
            '!' if delayed => out.push_str("^^!"),
            c => out.push(c),
        }
    }
    out
}

/// Escape a value for a PowerShell single-quoted string.
pub fn escape_ps1(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    for ch in value.chars() {
        match ch {
            '\'' => out.push_str("''"),
            '\r' | '\n' => {}
            c => out.push(c),
        }
    }
    out
}

/// Escape a value for a POSIX shell single-quoted string.
pub fn escape_sh(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    for ch in value.chars() {
        match ch {
            '\'' => out.push_str("'\\''"),
            '\r' | '\n' => {}
            c => out.push(c),
        }
    }
    out
}

/// Format one `set "NAME=value"` line (CRLF terminated).
pub fn bat_set(name: &str, value: &str, delayed: bool) -> Result<String> {
    check_name(name)?;
    Ok(format!("set \"{name}={}\"\r\n", escape_bat(value, delayed)))
}

/// Format one `$env:NAME = 'value'` line.
pub fn ps1_set(name: &str, value: &str) -> Result<String> {
    check_name(name)?;
    Ok(format!("$env:{name} = '{}'\n", escape_ps1(value)))
}

/// Format one `export NAME='value'` line.
pub fn sh_set(name: &str, value: &str) -> Result<String> {
    check_name(name)?;
    Ok(format!("export {name}='{}'\n", escape_sh(value)))
}

impl Results {
    /// The variables to publish, in a stable order.
    pub fn variables(&self) -> Result<Vec<(String, String)>> {
        let mut vars: Vec<(String, String)> = Vec::new();
        let with_prefix = |name: &str| format!("{}{}", self.prefix, name);

        if let Some(word) = &self.wbat {
            vars.push((with_prefix("WBAT"), word.clone()));
        } else if let Some(button) = &self.button {
            vars.push((with_prefix("WBAT"), button.name.clone()));
        } else if let Some(list) = &self.list {
            vars.push((with_prefix("WBAT"), list.item.clone()));
        } else if self.escaped {
            vars.push((with_prefix("WBAT"), String::new()));
        }
        if self.set_exit_var {
            let value = self
                .exit_var
                .clone()
                .unwrap_or_else(|| self.exit_code.to_string());
            vars.push((with_prefix("?"), value));
        }
        for (name, value) in &self.inputs {
            vars.push((with_prefix(name), value.trim_end().to_string()));
        }
        for (i, state) in self.checkboxes.iter().enumerate() {
            vars.push((
                with_prefix(&format!("WCB{}", i + 1)),
                (*state as u8).to_string(),
            ));
        }
        if let Some(radio) = self.radio {
            vars.push((with_prefix("WRB"), radio.to_string()));
        }
        for (name, _) in &vars {
            check_name(name)?;
        }
        Ok(vars)
    }

    /// Render results in the requested format.
    pub fn emit(&self, mode: EmitMode) -> Result<String> {
        let vars = self.variables()?;
        let mut out = String::new();
        match mode {
            EmitMode::None => return Ok(out),
            EmitMode::Bat => {
                for (name, value) in &vars {
                    out.push_str(&bat_set(name, value, self.delayed)?);
                }
            }
            EmitMode::Ps1 => {
                for (name, value) in &vars {
                    out.push_str(&ps1_set(name, value)?);
                }
            }
            EmitMode::Sh => {
                for (name, value) in &vars {
                    out.push_str(&sh_set(name, value)?);
                }
            }
            EmitMode::Json => out = self.to_json(),
        }
        Ok(out)
    }

    /// The JSON representation from section 7.9.
    pub fn to_json(&self) -> String {
        let mut map = serde_json::Map::new();
        map.insert("exit_code".into(), json!(self.exit_code));
        map.insert("escaped".into(), json!(self.escaped));
        if let Some(button) = &self.button {
            map.insert(
                "button".into(),
                json!({"index": button.index, "name": button.name}),
            );
        }
        if let Some(list) = &self.list {
            map.insert("item".into(), json!(list.item));
            map.insert("index".into(), json!(list.index));
            if !list.extra.is_empty() {
                let mut all = vec![list.item.clone()];
                all.extend(list.extra.iter().cloned());
                map.insert("selected".into(), json!(all));
            }
        }
        if !self.inputs.is_empty() {
            let mut inputs = serde_json::Map::new();
            for (name, value) in &self.inputs {
                inputs.insert(name.clone(), json!(value.trim_end()));
            }
            map.insert("inputs".into(), serde_json::Value::Object(inputs));
        }
        if !self.checkboxes.is_empty() {
            let values: Vec<u8> = self.checkboxes.iter().map(|b| *b as u8).collect();
            map.insert("checkboxes".into(), json!(values));
        }
        if let Some(radio) = self.radio {
            map.insert("radio".into(), json!(radio));
        }
        serde_json::Value::Object(map).to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Results {
        Results {
            exit_code: 1,
            escaped: false,
            button: Some(ButtonResult {
                index: 1,
                name: "OK".into(),
            }),
            inputs: vec![("name".into(), "Ada".into())],
            checkboxes: vec![true, false],
            radio: Some(2),
            set_exit_var: true,
            ..Results::default()
        }
    }

    #[test]
    fn hostile_values_survive_bat_escaping() {
        let hostile = "Ana & Bob%PATH%!x!^<|>\"q\"";
        let escaped = escape_bat(hostile, false);
        assert!(escaped.contains("%%PATH%%"));
        assert!(escaped.contains("&"));
        let delayed = escape_bat("a!b", true);
        assert_eq!(delayed, "a^^!b");
        let line = bat_set("name", hostile, false).unwrap();
        assert!(line.starts_with("set \"name="));
        assert!(line.ends_with("\"\r\n"));
    }

    #[test]
    fn ps1_and_sh_quoting() {
        assert_eq!(ps1_set("x", "it's").unwrap(), "$env:x = 'it''s'\n");
        assert_eq!(sh_set("x", "it's").unwrap(), "export x='it'\\''s'\n");
    }

    #[test]
    fn bat_variable_list_order() {
        let mut r = sample();
        r.prefix = "W".into();
        let vars = r.variables().unwrap();
        let names: Vec<&str> = vars.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(
            names,
            vec!["WWBAT", "W?", "Wname", "WWCB1", "WWCB2", "WWRB"]
        );
        assert_eq!(vars[4].1, "0");
        assert_eq!(vars[5].1, "2");
    }

    #[test]
    fn invalid_names_are_rejected() {
        assert!(bat_set("bad name", "x", false).is_err());
        assert!(bat_set("", "x", false).is_err());
        assert!(bat_set("ok?", "x", false).is_ok());
        assert!(!is_valid_var_name("a%b"));
    }

    #[test]
    fn json_shape() {
        let v: serde_json::Value = serde_json::from_str(&sample().to_json()).unwrap();
        assert_eq!(v["exit_code"], 1);
        assert_eq!(v["button"]["index"], 1);
        assert_eq!(v["button"]["name"], "OK");
        assert_eq!(v["inputs"]["name"], "Ada");
        assert_eq!(v["checkboxes"], json!([1, 0]));
        assert_eq!(v["radio"], 2);
    }

    #[test]
    fn json_list_shape() {
        let r = Results {
            exit_code: 0,
            list: Some(ListResult {
                index: 7,
                item: "report.txt".into(),
                extra: vec![],
            }),
            ..Results::default()
        };
        let v: serde_json::Value = serde_json::from_str(&r.to_json()).unwrap();
        assert_eq!(v["item"], "report.txt");
        assert_eq!(v["index"], 7);
        assert_eq!(v["escaped"], false);
    }

    #[test]
    fn emit_none_is_empty() {
        assert!(sample().emit(EmitMode::None).unwrap().is_empty());
    }

    #[test]
    fn values_with_newlines_are_flattened() {
        let line = bat_set("x", "a\r\nb", false).unwrap();
        assert_eq!(line, "set \"x=ab\"\r\n");
    }
}
