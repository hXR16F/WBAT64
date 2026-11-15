//! Layout and console text encodings.
//!
//! Compat mode reads layouts in the console's OEM code page, native mode reads
//! UTF-8 with an optional BOM. `encoding_rs` is not used: wbat64 only needs the
//! two DOS code pages, so their high halves are embedded as tables, and UTF-8
//! is handled by the standard library.

#[path = "cp_tables.rs"]
mod cp_tables;

use std::fmt;

/// A supported text encoding for layout and list files.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Encoding {
    /// UTF-8 (invalid sequences become U+FFFD).
    Utf8,
    /// Code page 437 (the original PC/DOS code page).
    Cp437,
    /// Code page 850 (Western European DOS).
    Cp850,
    /// The console OEM code page. Windows ANSI/OEM distinctions are handled by
    /// the caller; here it means CP437 unless `WBAT64_OEM_CP=850` is set.
    Oem,
    /// Try UTF-8, fall back to the OEM code page when it is not valid UTF-8.
    #[default]
    Auto,
}

impl Encoding {
    pub fn parse(name: &str) -> Option<Encoding> {
        match name
            .trim()
            .to_ascii_lowercase()
            .replace(['-', '_'], "")
            .as_str()
        {
            "utf8" | "utf" => Some(Encoding::Utf8),
            "cp437" | "437" | "pc8" | "dos" | "oem437" => Some(Encoding::Cp437),
            "cp850" | "850" | "oem850" => Some(Encoding::Cp850),
            "oem" | "console" => Some(Encoding::Oem),
            "auto" => Some(Encoding::Auto),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Encoding::Utf8 => "utf8",
            Encoding::Cp437 => "cp437",
            Encoding::Cp850 => "cp850",
            Encoding::Oem => "oem",
            Encoding::Auto => "auto",
        }
    }

    fn table(self) -> &'static str {
        match self {
            Encoding::Cp850 => cp_tables::CP850_HIGH,
            _ => cp_tables::CP437_HIGH,
        }
    }
}

impl fmt::Display for Encoding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

fn decode_single_byte(bytes: &[u8], table: &str) -> String {
    let high: Vec<char> = table.chars().collect();
    let mut out = String::with_capacity(bytes.len());
    for &b in bytes {
        if b < 0x80 {
            out.push(b as char);
        } else {
            let idx = (b - 0x80) as usize;
            out.push(high.get(idx).copied().unwrap_or('\u{FFFD}'));
        }
    }
    out
}

/// True when `bytes` is valid UTF-8, optionally after a BOM.
pub fn is_utf8(bytes: &[u8]) -> bool {
    std::str::from_utf8(bytes).is_ok()
}

/// Decode bytes with the given encoding, stripping a UTF-8 BOM.
pub fn decode(bytes: &[u8], encoding: Encoding) -> String {
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    match encoding {
        Encoding::Utf8 | Encoding::Auto => {
            if encoding == Encoding::Utf8 || is_utf8(bytes) {
                String::from_utf8_lossy(bytes).into_owned()
            } else {
                decode_single_byte(bytes, Encoding::Cp437.table())
            }
        }
        Encoding::Cp437 | Encoding::Oem => decode_single_byte(bytes, Encoding::Cp437.table()),
        Encoding::Cp850 => decode_single_byte(bytes, Encoding::Cp850.table()),
    }
}

/// Decode and report the encoding that was actually used.
pub fn decode_with_detection(bytes: &[u8], requested: Encoding) -> (String, Encoding) {
    let stripped = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    match requested {
        Encoding::Auto => {
            if is_utf8(stripped) {
                (decode(stripped, Encoding::Utf8), Encoding::Utf8)
            } else {
                (
                    decode_single_byte(stripped, Encoding::Cp437.table()),
                    Encoding::Cp437,
                )
            }
        }
        other => (decode(stripped, other), other),
    }
}

/// Encode a string for output. Only `sh`/`bat` emitters need ASCII-safe text;
/// this is used by the screen dumper.
pub fn encode_lossy(text: &str) -> Vec<u8> {
    text.as_bytes().to_vec()
}

/// Encode text in a single-byte code page.
///
/// This is what `.bat` result files need: `cmd.exe` reads batch files in the
/// console OEM code page, so writing UTF-8 there would mangle non-ASCII values.
/// Characters that the code page cannot represent become `?`.
pub fn encode_single_byte(text: &str, encoding: Encoding) -> Vec<u8> {
    let table = encoding.table();
    let high: Vec<char> = table.chars().collect();
    let mut out = Vec::with_capacity(text.len());
    for ch in text.chars() {
        let code = ch as u32;
        if code < 0x80 {
            out.push(code as u8);
            continue;
        }
        match high.iter().position(|candidate| *candidate == ch) {
            Some(index) => out.push(0x80 + index as u8),
            None => out.push(b'?'),
        }
    }
    out
}

/// Encode text for a Windows batch file (OEM/CP437 by default).
///
/// `Auto`/`Utf8` describe layout encodings; batch files are always OEM.
pub fn encode_oem(text: &str, encoding: Encoding) -> Vec<u8> {
    match encoding {
        Encoding::Cp850 => encode_single_byte(text, Encoding::Cp850),
        _ => encode_single_byte(text, Encoding::Cp437),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf8_with_and_without_bom() {
        assert_eq!(decode("héllo".as_bytes(), Encoding::Utf8), "héllo");
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice("x".as_bytes());
        assert_eq!(decode(&bytes, Encoding::Utf8), "x");
    }

    #[test]
    fn cp437_high_half() {
        // 0xC4 is the box-drawing horizontal line in CP437.
        assert_eq!(decode(&[0xC4], Encoding::Cp437), "─");
        assert_eq!(decode(&[0xDB], Encoding::Cp437), "█");
        // 0x82 is 'é' in both CP437 and CP850.
        assert_eq!(decode(&[0x82], Encoding::Cp437), "é");
    }

    #[test]
    fn cp850_differs_from_cp437() {
        // 0xD5 is 'ı' in CP850 but '╒' in CP437.
        assert_eq!(decode(&[0xD5], Encoding::Cp850), "\u{0131}");
        assert_eq!(decode(&[0xD5], Encoding::Cp437), "╒");
    }

    #[test]
    fn auto_falls_back_to_cp437() {
        let (text, used) = decode_with_detection(&[0xC4, 0xC4], Encoding::Auto);
        assert_eq!(text, "──");
        assert_eq!(used, Encoding::Cp437);
    }

    #[test]
    fn encoding_names() {
        assert_eq!(Encoding::parse("UTF-8"), Some(Encoding::Utf8));
        assert_eq!(Encoding::parse("cp850"), Some(Encoding::Cp850));
        assert_eq!(Encoding::parse("klingon"), None);
    }

    #[test]
    fn oem_encoding_round_trips_representable_text() {
        // 0x82 is 'é' in CP437; '─' is 0xC4.
        assert_eq!(encode_single_byte("é", Encoding::Cp437), vec![0x82]);
        assert_eq!(encode_single_byte("─", Encoding::Cp437), vec![0xC4]);
        assert_eq!(encode_single_byte("abc", Encoding::Cp437), b"abc".to_vec());
        // Unrepresentable characters degrade to '?'.
        assert_eq!(encode_single_byte("日", Encoding::Cp437), b"?".to_vec());
        // CP850 maps 0x82 to 'é' as well but differs at 0xD5.
        assert_eq!(encode_single_byte("\u{0131}", Encoding::Cp850), vec![0xD5]);
        assert_eq!(encode_single_byte("─", Encoding::Cp850), vec![0xC4]);
    }
}
