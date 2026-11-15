//! Command-line tokenization.
//!
//! Batch files pass quoted arguments through `%*`, and `cmd.exe` hands the
//! whole command line to the process. To support both `wbat64 box 5,12 "Hi" ok`
//! and `call w.bat "menu @dialogs.txt:main_menu #%mpos%+1"`, wbat64 re-tokenizes
//! the raw command line on Windows (where quotes are still present), and falls
//! back to `std::env::args()` elsewhere.

/// One command-line token plus whether it came from a quoted region.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Token {
    pub text: String,
    pub quoted: bool,
}

impl Token {
    pub fn new(text: impl Into<String>, quoted: bool) -> Token {
        Token {
            text: text.into(),
            quoted,
        }
    }

    pub fn plain(text: impl Into<String>) -> Token {
        Token::new(text, false)
    }
}

impl std::fmt::Display for Token {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.quoted {
            write!(f, "\"{}\"", self.text)
        } else {
            f.write_str(&self.text)
        }
    }
}

/// Tokenize a raw command line, optionally dropping the first token (argv[0]).
pub fn tokenize(line: &str, skip_first: bool) -> Vec<Token> {
    let chars: Vec<char> = line.chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0usize;
    let mut first = true;
    while i < chars.len() {
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        if i >= chars.len() {
            break;
        }
        let mut text = String::new();
        let mut quoted = false;
        while i < chars.len() && !chars[i].is_whitespace() {
            match chars[i] {
                '"' => {
                    quoted = true;
                    i += 1;
                    while i < chars.len() {
                        if chars[i] == '"' {
                            if i + 1 < chars.len() && chars[i + 1] == '"' {
                                text.push('"');
                                i += 2;
                                continue;
                            }
                            i += 1;
                            break;
                        }
                        text.push(chars[i]);
                        i += 1;
                    }
                }
                '\\' if i + 1 < chars.len() && chars[i + 1] == '"' => {
                    // cmd.exe keeps backslashes literal; \" is an escaped quote.
                    text.push('"');
                    quoted = true;
                    i += 2;
                }
                ch => {
                    text.push(ch);
                    i += 1;
                }
            }
        }
        if first && skip_first {
            first = false;
            continue;
        }
        first = false;
        tokens.push(Token { text, quoted });
    }
    tokens
}

/// The process arguments with quoting information where available.
pub fn raw_args() -> Vec<Token> {
    #[cfg(windows)]
    {
        let line = windows_command_line();
        if let Some(line) = line {
            let tokens = tokenize(&line, true);
            if !tokens.is_empty() {
                return tokens;
            }
        }
    }
    std::env::args()
        .skip(1)
        .map(|arg| {
            // A single argument containing spaces almost certainly came from a
            // quoted string (or a quoted parameter string passed to w.bat).
            let quoted = arg.chars().any(char::is_whitespace);
            Token::new(arg, quoted)
        })
        .collect()
}

#[cfg(windows)]
fn windows_command_line() -> Option<String> {
    extern "system" {
        fn GetCommandLineW() -> *const u16;
    }
    unsafe {
        let ptr = GetCommandLineW();
        if ptr.is_null() {
            return None;
        }
        let mut len = 0usize;
        while *ptr.add(len) != 0 {
            len += 1;
        }
        let slice = std::slice::from_raw_parts(ptr, len);
        Some(String::from_utf16_lossy(slice))
    }
}

/// Split tokens into command segments on `!` or `--and` (when unquoted).
pub fn split_commands(tokens: &[Token]) -> Vec<Vec<Token>> {
    let mut segments: Vec<Vec<Token>> = Vec::new();
    let mut current: Vec<Token> = Vec::new();
    for token in tokens {
        if !token.quoted && (token.text == "!" || token.text.eq_ignore_ascii_case("--and")) {
            if !current.is_empty() {
                segments.push(std::mem::take(&mut current));
            }
        } else {
            current.push(token.clone());
        }
    }
    if !current.is_empty() {
        segments.push(current);
    }
    segments
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(tokens: &[Token]) -> Vec<&str> {
        tokens.iter().map(|t| t.text.as_str()).collect()
    }

    #[test]
    fn simple_quoting() {
        let tokens = tokenize(r#"wbat64 box 5,12 "Hello there" ok,back"#, true);
        assert_eq!(
            texts(&tokens),
            vec!["box", "5,12", "Hello there", "ok,back"]
        );
        assert!(tokens[2].quoted);
        assert!(!tokens[3].quoted);
    }

    #[test]
    fn doubled_quotes() {
        let tokens = tokenize(r#"wbat64 box "say ""hi""" ok"#, true);
        assert_eq!(tokens[1].text, r#"say "hi""#);
    }

    #[test]
    fn quoted_parameter_string_is_one_token() {
        let tokens = tokenize(r#"wbat64 "menu @dialogs.txt:main_menu #3+1""#, true);
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].text, "menu @dialogs.txt:main_menu #3+1");
        assert!(tokens[0].quoted);
    }

    #[test]
    fn split_on_bang_and_and() {
        let tokens = tokenize("wbat64 cls ! text 1,1 hi --and box ok", true);
        let segments = split_commands(&tokens);
        assert_eq!(segments.len(), 3);
        assert_eq!(texts(&segments[0]), vec!["cls"]);
        assert_eq!(texts(&segments[1]), vec!["text", "1,1", "hi"]);
        assert_eq!(texts(&segments[2]), vec!["box", "ok"]);
    }

    #[test]
    fn quoted_bang_is_literal() {
        let tokens = tokenize(r#"box "a ! b" ok"#, true);
        assert_eq!(split_commands(&tokens).len(), 1);
    }
}
