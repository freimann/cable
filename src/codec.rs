//! Character encodings and byte/string conversion helpers.
//!
//! CABLE could treat data as ASCII or EBCDIC.  The EBCDIC code page used here
//! is IBM code page 037, which is the international EBCDIC variant that was
//! current on the IBM systems CABLE was written for.

use anyhow::{Result, bail};

/// IBM code page 037 decode table (byte -> Unicode scalar).
pub const CP037: [char; 256] = [
    '\u{0000}', '\u{0001}', '\u{0002}', '\u{0003}', '\u{009C}', '\u{0009}', '\u{0086}', '\u{007F}',
    '\u{0097}', '\u{008D}', '\u{008E}', '\u{000B}', '\u{000C}', '\u{000D}', '\u{000E}', '\u{000F}',
    '\u{0010}', '\u{0011}', '\u{0012}', '\u{0013}', '\u{009D}', '\u{0085}', '\u{0008}', '\u{0087}',
    '\u{0018}', '\u{0019}', '\u{0092}', '\u{008F}', '\u{001C}', '\u{001D}', '\u{001E}', '\u{001F}',
    '\u{0080}', '\u{0081}', '\u{0082}', '\u{0083}', '\u{0084}', '\u{000A}', '\u{0017}', '\u{001B}',
    '\u{0088}', '\u{0089}', '\u{008A}', '\u{008B}', '\u{008C}', '\u{0005}', '\u{0006}', '\u{0007}',
    '\u{0090}', '\u{0091}', '\u{0016}', '\u{0093}', '\u{0094}', '\u{0095}', '\u{0096}', '\u{0004}',
    '\u{0098}', '\u{0099}', '\u{009A}', '\u{009B}', '\u{0014}', '\u{0015}', '\u{009E}', '\u{001A}',
    '\u{0020}', '\u{00A0}', '\u{00E2}', '\u{00E4}', '\u{00E0}', '\u{00E1}', '\u{00E3}', '\u{00E5}',
    '\u{00E7}', '\u{00F1}', '\u{00A2}', '\u{002E}', '\u{003C}', '\u{0028}', '\u{002B}', '\u{007C}',
    '\u{0026}', '\u{00E9}', '\u{00EA}', '\u{00EB}', '\u{00E8}', '\u{00ED}', '\u{00EE}', '\u{00EF}',
    '\u{00EC}', '\u{00DF}', '\u{0021}', '\u{0024}', '\u{002A}', '\u{0029}', '\u{003B}', '\u{00AC}',
    '\u{002D}', '\u{002F}', '\u{00C2}', '\u{00C4}', '\u{00C0}', '\u{00C1}', '\u{00C3}', '\u{00C5}',
    '\u{00C7}', '\u{00D1}', '\u{00A6}', '\u{002C}', '\u{0025}', '\u{005F}', '\u{003E}', '\u{003F}',
    '\u{00F8}', '\u{00C9}', '\u{00CA}', '\u{00CB}', '\u{00C8}', '\u{00CD}', '\u{00CE}', '\u{00CF}',
    '\u{00CC}', '\u{0060}', '\u{003A}', '\u{0023}', '\u{0040}', '\u{0027}', '\u{003D}', '\u{0022}',
    '\u{00D8}', '\u{0061}', '\u{0062}', '\u{0063}', '\u{0064}', '\u{0065}', '\u{0066}', '\u{0067}',
    '\u{0068}', '\u{0069}', '\u{00AB}', '\u{00BB}', '\u{00F0}', '\u{00FD}', '\u{00FE}', '\u{00B1}',
    '\u{00B0}', '\u{006A}', '\u{006B}', '\u{006C}', '\u{006D}', '\u{006E}', '\u{006F}', '\u{0070}',
    '\u{0071}', '\u{0072}', '\u{00AA}', '\u{00BA}', '\u{00E6}', '\u{00B8}', '\u{00C6}', '\u{00A4}',
    '\u{00B5}', '\u{007E}', '\u{0073}', '\u{0074}', '\u{0075}', '\u{0076}', '\u{0077}', '\u{0078}',
    '\u{0079}', '\u{007A}', '\u{00A1}', '\u{00BF}', '\u{00D0}', '\u{00DD}', '\u{00DE}', '\u{00AE}',
    '\u{005E}', '\u{00A3}', '\u{00A5}', '\u{00B7}', '\u{00A9}', '\u{00A7}', '\u{00B6}', '\u{00BC}',
    '\u{00BD}', '\u{00BE}', '\u{005B}', '\u{005D}', '\u{00AF}', '\u{00A8}', '\u{00B4}', '\u{00D7}',
    '\u{007B}', '\u{0041}', '\u{0042}', '\u{0043}', '\u{0044}', '\u{0045}', '\u{0046}', '\u{0047}',
    '\u{0048}', '\u{0049}', '\u{00AD}', '\u{00F4}', '\u{00F6}', '\u{00F2}', '\u{00F3}', '\u{00F5}',
    '\u{007D}', '\u{004A}', '\u{004B}', '\u{004C}', '\u{004D}', '\u{004E}', '\u{004F}', '\u{0050}',
    '\u{0051}', '\u{0052}', '\u{00B9}', '\u{00FB}', '\u{00FC}', '\u{00F9}', '\u{00FA}', '\u{00FF}',
    '\u{005C}', '\u{00F7}', '\u{0053}', '\u{0054}', '\u{0055}', '\u{0056}', '\u{0057}', '\u{0058}',
    '\u{0059}', '\u{005A}', '\u{00B2}', '\u{00D4}', '\u{00D6}', '\u{00D2}', '\u{00D3}', '\u{00D5}',
    '\u{0030}', '\u{0031}', '\u{0032}', '\u{0033}', '\u{0034}', '\u{0035}', '\u{0036}', '\u{0037}',
    '\u{0038}', '\u{0039}', '\u{00B3}', '\u{00DB}', '\u{00DC}', '\u{00D9}', '\u{00DA}', '\u{009F}',
];

/// Encode one Unicode scalar as CP037.  Returns `None` for characters that
/// have no single-byte representation.
pub fn encode_cp037(c: char) -> Option<u8> {
    CP037.iter().position(|&x| x == c).map(|i| i as u8)
}

/// IBM code page 437 decode table (byte -> Unicode scalar), including the
/// graphic glyphs the original's ROM font shows for the control range.  CABLE
/// writes bytes straight to video memory, so `0xE9` appears as `Θ`, `0xFA` as
/// `·` and `0xFF` as a non-breaking space.
pub const CP437: [char; 256] = [
    ' ', '\u{263A}', '\u{263B}', '\u{2665}', '\u{2666}', '\u{2663}', '\u{2660}', '\u{2022}',
    '\u{25D8}', '\u{25CB}', '\u{25D9}', '\u{2642}', '\u{2640}', '\u{266A}', '\u{266B}', '\u{263C}',
    '\u{25BA}', '\u{25C4}', '\u{2195}', '\u{203C}', '\u{00B6}', '\u{00A7}', '\u{25AC}', '\u{21A8}',
    '\u{2191}', '\u{2193}', '\u{2192}', '\u{2190}', '\u{221F}', '\u{2194}', '\u{25B2}', '\u{25BC}',
    ' ', '!', '"', '#', '$', '%', '&', '\'', '(', ')', '*', '+', ',', '-', '.', '/', '0', '1', '2',
    '3', '4', '5', '6', '7', '8', '9', ':', ';', '<', '=', '>', '?', '@', 'A', 'B', 'C', 'D', 'E',
    'F', 'G', 'H', 'I', 'J', 'K', 'L', 'M', 'N', 'O', 'P', 'Q', 'R', 'S', 'T', 'U', 'V', 'W', 'X',
    'Y', 'Z', '[', '\\', ']', '^', '_', '`', 'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j', 'k',
    'l', 'm', 'n', 'o', 'p', 'q', 'r', 's', 't', 'u', 'v', 'w', 'x', 'y', 'z', '{', '|', '}', '~',
    '\u{2302}', '\u{00C7}', '\u{00FC}', '\u{00E9}', '\u{00E2}', '\u{00E4}', '\u{00E0}', '\u{00E5}',
    '\u{00E7}', '\u{00EA}', '\u{00EB}', '\u{00E8}', '\u{00EF}', '\u{00EE}', '\u{00EC}', '\u{00C4}',
    '\u{00C5}', '\u{00C9}', '\u{00E6}', '\u{00C6}', '\u{00F4}', '\u{00F6}', '\u{00F2}', '\u{00FB}',
    '\u{00F9}', '\u{00FF}', '\u{00D6}', '\u{00DC}', '\u{00A2}', '\u{00A3}', '\u{00A5}', '\u{20A7}',
    '\u{0192}', '\u{00E1}', '\u{00ED}', '\u{00F3}', '\u{00FA}', '\u{00F1}', '\u{00D1}', '\u{00AA}',
    '\u{00BA}', '\u{00BF}', '\u{2310}', '\u{00AC}', '\u{00BD}', '\u{00BC}', '\u{00A1}', '\u{00AB}',
    '\u{00BB}', '\u{2591}', '\u{2592}', '\u{2593}', '\u{2502}', '\u{2524}', '\u{2561}', '\u{2562}',
    '\u{2556}', '\u{2555}', '\u{2563}', '\u{2551}', '\u{2557}', '\u{255D}', '\u{255C}', '\u{255B}',
    '\u{2510}', '\u{2514}', '\u{2534}', '\u{252C}', '\u{251C}', '\u{2500}', '\u{253C}', '\u{255E}',
    '\u{255F}', '\u{255A}', '\u{2554}', '\u{2569}', '\u{2566}', '\u{2560}', '\u{2550}', '\u{256C}',
    '\u{2567}', '\u{2568}', '\u{2564}', '\u{2565}', '\u{2559}', '\u{2558}', '\u{2552}', '\u{2553}',
    '\u{256B}', '\u{256A}', '\u{2518}', '\u{250C}', '\u{2588}', '\u{2584}', '\u{258C}', '\u{2590}',
    '\u{2580}', '\u{03B1}', '\u{00DF}', '\u{0393}', '\u{03C0}', '\u{03A3}', '\u{03C3}', '\u{00B5}',
    '\u{03C4}', '\u{03A6}', '\u{0398}', '\u{03A9}', '\u{03B4}', '\u{221E}', '\u{03C6}', '\u{03B5}',
    '\u{2229}', '\u{2261}', '\u{00B1}', '\u{2265}', '\u{2264}', '\u{2320}', '\u{2321}', '\u{00F7}',
    '\u{2248}', '\u{00B0}', '\u{2219}', '\u{00B7}', '\u{221A}', '\u{207F}', '\u{00B2}', '\u{25A0}',
    '\u{00A0}',
];

/// A safe, printable representation of a byte.
///
/// `conceal` mirrors CABLE's Conceal/Show state.  When concealing (the
/// original's default), control bytes become `.`; when showing, they render as
/// the code page's graphic glyphs.  The original decides this at `CS:0x3bf7`.
pub fn display_byte(b: u8, ebcdic: bool, conceal: bool) -> char {
    if ebcdic {
        let c = CP037[b as usize];
        let u = c as u32;
        if u < 0x20 || u == 0x7f {
            if conceal {
                '.'
            } else {
                CP437[(u as usize) & 0xff]
            }
        } else {
            c
        }
    } else if b < 0x20 || b == 0x7f {
        if conceal { '.' } else { CP437[b as usize] }
    } else {
        CP437[b as usize]
    }
}

/// Fold one data byte for case-insensitive comparison.
pub fn fold_byte(b: u8, ebcdic: bool) -> u8 {
    if ebcdic {
        let c = CP037[b as usize];
        let upper = c.to_uppercase().next().unwrap_or(c);
        encode_cp037(upper).unwrap_or(b)
    } else if b.is_ascii_lowercase() {
        b - 0x20
    } else {
        b
    }
}

/// Convert typed text into the byte representation used for searching.
pub fn encode_text(s: &str, ebcdic: bool) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(s.chars().count());
    for c in s.chars() {
        if ebcdic {
            match encode_cp037(c) {
                Some(b) => out.push(b),
                None => bail!("Character {c:?} cannot be encoded as EBCDIC"),
            }
        } else {
            let u = c as u32;
            if u <= 0xff {
                out.push(u as u8);
            } else {
                bail!("Character {c:?} cannot be encoded as ASCII");
            }
        }
    }
    Ok(out)
}

/// Parse a hexadecimal string, ignoring spaces and tabs.
pub fn parse_hex(s: &str) -> Result<Vec<u8>> {
    let mut digits = Vec::new();
    for c in s.chars() {
        if c.is_whitespace() {
            continue;
        }
        if let Some(v) = c.to_digit(16) {
            digits.push(v as u8);
        } else {
            bail!("Invalid HEX digit {c:?}");
        }
    }
    if digits.len() % 2 != 0 {
        bail!("HEX string must contain an even number of digits");
    }
    Ok(digits.chunks(2).map(|p| (p[0] << 4) | p[1]).collect())
}

/// Parse an offset typed at the `+`, `-` or `=` prompt.
///
/// A leading `H` or `0x` means hexadecimal, as in the original CABLE prompt
/// ("if HEX start with H").  Everything else is decimal.
pub fn parse_offset(s: &str) -> Result<u64> {
    let t = s.trim();
    if t.is_empty() {
        bail!("No value entered");
    }
    let (radix, digits) = if let Some(rest) = t.strip_prefix(['H', 'h']) {
        (16, rest)
    } else if let Some(rest) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        (16, rest)
    } else {
        (10, t)
    };
    let digits: String = digits.chars().filter(|c| !c.is_whitespace()).collect();
    if digits.is_empty() {
        bail!("No value entered");
    }
    u64::from_str_radix(&digits, radix).map_err(|_| anyhow::anyhow!("Invalid number {t:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cp037_round_trip_for_printable_ascii() {
        for b in 0x20u8..=0x7e {
            let c = CP037[b as usize];
            if encode_cp037(c).is_some() {
                assert_eq!(CP037[encode_cp037(c).unwrap() as usize], c);
            }
        }
    }

    #[test]
    fn ebcdic_text_round_trip() {
        let needle = encode_text("Hello, World!", true).unwrap();
        assert_eq!(needle[0], 0xC8); // H
        assert_eq!(needle[5], 0x6B); // ,
        assert_eq!(needle[6], 0x40); // space
        assert_eq!(CP037[needle[7] as usize], 'W');
    }

    #[test]
    fn parse_hex_ignores_blanks() {
        assert_eq!(
            parse_hex("de ad   BE EF").unwrap(),
            vec![0xde, 0xad, 0xbe, 0xef]
        );
        assert!(parse_hex("abc").is_err());
        assert!(parse_hex("zz").is_err());
    }

    #[test]
    fn parse_offsets() {
        assert_eq!(parse_offset("123").unwrap(), 123);
        assert_eq!(parse_offset("H1f").unwrap(), 31);
        assert_eq!(parse_offset("0x20").unwrap(), 32);
        assert_eq!(parse_offset(" H 10 ").unwrap(), 16);
        assert!(parse_offset("").is_err());
        assert!(parse_offset("nope").is_err());
    }

    #[test]
    fn conceal_replaces_controls() {
        assert_eq!(display_byte(0x00, false, true), '.');
        assert_eq!(display_byte(b'A', false, true), 'A');
        assert_eq!(display_byte(0x00, false, false), ' ');
        assert_eq!(display_byte(0x40, true, true), ' ');
    }

    #[test]
    fn ascii_uses_code_page_437_glyphs() {
        // Bytes seen in the supplied screen capture: `Θ·.*.p.x` followed by
        // a non-breaking space.
        assert_eq!(display_byte(0xE9, false, true), '\u{0398}');
        assert_eq!(display_byte(0xFA, false, true), '\u{00B7}');
        assert_eq!(display_byte(0xFF, false, true), '\u{00A0}');
        assert_eq!(display_byte(0x5C, false, true), '\\');
        assert_eq!(display_byte(0x18, false, true), '.');
        assert_eq!(display_byte(0x18, false, false), '\u{2191}');
    }
}
