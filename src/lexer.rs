//! Hand-written, single-pass lexer over raw bytes.

use crate::diag::{err, Pos, Result};

#[derive(Clone, Debug, PartialEq)]
pub enum Tok {
    Ident(String),
    Kw(&'static str),
    Punct(&'static str),
    /// Decimal literals hold their magnitude (may be 2^31 under unary minus);
    /// hex/octal/binary literals hold the sign-extended 32-bit value.
    Int(i64),
    Long(u64),
    Double(f64),
    Char(u16),
    Str(String),
    Eof,
}

#[derive(Clone, Debug)]
pub struct Token {
    pub tok: Tok,
    pub pos: Pos,
    pub end: Pos,
}

const KEYWORDS: &[&str] = &[
    "abstract",
    "assert",
    "boolean",
    "break",
    "byte",
    "case",
    "catch",
    "char",
    "class",
    "const",
    "continue",
    "default",
    "do",
    "double",
    "else",
    "enum",
    "extends",
    "false",
    "final",
    "finally",
    "float",
    "for",
    "goto",
    "if",
    "implements",
    "import",
    "instanceof",
    "int",
    "interface",
    "long",
    "native",
    "new",
    "null",
    "package",
    "private",
    "protected",
    "public",
    "return",
    "short",
    "static",
    "strictfp",
    "super",
    "switch",
    "synchronized",
    "this",
    "throw",
    "throws",
    "transient",
    "true",
    "try",
    "void",
    "volatile",
    "while",
];

/// Longest-match operator/separator recognition, dispatched on the first byte.
fn punct(r: &[u8]) -> Option<&'static str> {
    let at = |i: usize| r.get(i).copied().unwrap_or(0);
    let (c1, c2, c3) = (at(1), at(2), at(3));
    Some(match r[0] {
        b'(' => "(",
        b')' => ")",
        b'{' => "{",
        b'}' => "}",
        b'[' => "[",
        b']' => "]",
        b';' => ";",
        b',' => ",",
        b'~' => "~",
        b'?' => "?",
        b'@' => "@",
        b'.' => {
            if c1 == b'.' && c2 == b'.' {
                "..."
            } else {
                "."
            }
        }
        b':' => {
            if c1 == b':' {
                "::"
            } else {
                ":"
            }
        }
        b'=' => {
            if c1 == b'=' {
                "=="
            } else {
                "="
            }
        }
        b'!' => {
            if c1 == b'=' {
                "!="
            } else {
                "!"
            }
        }
        b'<' => match (c1, c2) {
            (b'<', b'=') => "<<=",
            (b'<', _) => "<<",
            (b'=', _) => "<=",
            _ => "<",
        },
        b'>' => match (c1, c2, c3) {
            (b'>', b'>', b'=') => ">>>=",
            (b'>', b'>', _) => ">>>",
            (b'>', b'=', _) => ">>=",
            (b'>', _, _) => ">>",
            (b'=', _, _) => ">=",
            _ => ">",
        },
        b'+' => match c1 {
            b'+' => "++",
            b'=' => "+=",
            _ => "+",
        },
        b'-' => match c1 {
            b'-' => "--",
            b'=' => "-=",
            b'>' => "->",
            _ => "-",
        },
        b'*' => {
            if c1 == b'=' {
                "*="
            } else {
                "*"
            }
        }
        b'/' => {
            if c1 == b'=' {
                "/="
            } else {
                "/"
            }
        }
        b'%' => {
            if c1 == b'=' {
                "%="
            } else {
                "%"
            }
        }
        b'&' => match c1 {
            b'&' => "&&",
            b'=' => "&=",
            _ => "&",
        },
        b'|' => match c1 {
            b'|' => "||",
            b'=' => "|=",
            _ => "|",
        },
        b'^' => {
            if c1 == b'=' {
                "^="
            } else {
                "^"
            }
        }
        _ => return None,
    })
}

pub fn lex(src: &str, file: usize) -> Result<Vec<Token>> {
    let b = src.as_bytes();
    let mut toks = Vec::with_capacity(b.len() / 4);
    let mut i = 0usize;
    let mut line = 1u32;
    let mut line_start = 0usize;

    macro_rules! pos {
        ($at:expr) => {
            Pos {
                line,
                col: ($at - line_start + 1) as u32,
            }
        };
    }

    while i < b.len() {
        let c = b[i];
        // whitespace
        if c == b'\n' {
            i += 1;
            line += 1;
            line_start = i;
            continue;
        }
        if c == b' ' || c == b'\t' || c == b'\r' || c == 0x0c {
            i += 1;
            continue;
        }
        // comments
        if c == b'/' && i + 1 < b.len() {
            if b[i + 1] == b'/' {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
                continue;
            }
            if b[i + 1] == b'*' {
                let start = pos!(i);
                i += 2;
                loop {
                    if i + 1 >= b.len() {
                        return err(file, start, "unclosed comment");
                    }
                    if b[i] == b'*' && b[i + 1] == b'/' {
                        i += 2;
                        break;
                    }
                    if b[i] == b'\n' {
                        line += 1;
                        line_start = i + 1;
                    }
                    i += 1;
                }
                continue;
            }
        }
        let p = pos!(i);
        // identifiers / keywords
        if c.is_ascii_alphabetic() || c == b'_' || c == b'$' || c >= 0x80 {
            let start = i;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_' || b[i] == b'$' || b[i] >= 0x80) {
                i += 1;
            }
            let word = &src[start..i];
            let kw = if c.is_ascii_lowercase() && word.len() <= 12 {
                KEYWORDS.binary_search(&word).ok().map(|k| KEYWORDS[k])
            } else {
                None
            };
            let tok = match kw {
                Some(k) => Tok::Kw(k),
                None => Tok::Ident(word.to_string()),
            };
            toks.push(Token {
                tok,
                pos: p,
                end: pos!(i),
            });
            continue;
        }
        // numbers
        if c.is_ascii_digit() || (c == b'.' && i + 1 < b.len() && b[i + 1].is_ascii_digit()) {
            let (tok, n) = lex_number(&b[i..], file, p)?;
            i += n;
            toks.push(Token {
                tok,
                pos: p,
                end: pos!(i),
            });
            continue;
        }
        // char literal
        if c == b'\'' {
            i += 1;
            let ch = if i < b.len() && b[i] == b'\\' {
                let (v, n) = lex_escape(&b[i..], file, p)?;
                i += n;
                v
            } else {
                let rest = &src[i..];
                let ch = match rest.chars().next() {
                    Some(ch) if ch != '\'' && ch != '\n' => ch,
                    _ => return err(file, p, "empty or malformed character literal"),
                };
                i += ch.len_utf8();
                let v = ch as u32;
                if v > 0xFFFF {
                    return err(file, p, "character literal outside the Basic Multilingual Plane");
                }
                v as u16
            };
            if i >= b.len() || b[i] != b'\'' {
                return err(file, p, "unclosed character literal");
            }
            i += 1;
            toks.push(Token {
                tok: Tok::Char(ch),
                pos: p,
                end: pos!(i),
            });
            continue;
        }
        // string literal
        if c == b'"' {
            if b[i..].starts_with(b"\"\"\"") {
                return err(file, p, "text blocks are not supported");
            }
            i += 1;
            let mut units: Vec<u16> = Vec::new();
            loop {
                if i >= b.len() || b[i] == b'\n' {
                    return err(file, p, "unclosed string literal");
                }
                if b[i] == b'"' {
                    i += 1;
                    break;
                }
                if b[i] == b'\\' {
                    let (v, n) = lex_escape(&b[i..], file, pos!(i))?;
                    units.push(v);
                    i += n;
                } else {
                    let ch = src[i..].chars().next().unwrap();
                    let mut buf = [0u16; 2];
                    units.extend_from_slice(ch.encode_utf16(&mut buf));
                    i += ch.len_utf8();
                }
            }
            toks.push(Token {
                tok: Tok::Str(String::from_utf16_lossy(&units)),
                pos: p,
                end: pos!(i),
            });
            continue;
        }
        // punctuation
        let rest = &b[i..];
        match punct(rest) {
            Some(pp) => {
                i += pp.len();
                toks.push(Token {
                    tok: Tok::Punct(pp),
                    pos: p,
                    end: pos!(i),
                });
            }
            None => {
                let ch = src[i..].chars().next().unwrap();
                return err(file, p, format!("illegal character '{}'", ch));
            }
        }
    }
    toks.push(Token {
        tok: Tok::Eof,
        pos: pos!(i),
        end: pos!(i),
    });
    Ok(toks)
}

fn lex_escape(b: &[u8], file: usize, p: Pos) -> Result<(u16, usize)> {
    // b[0] == '\\'
    let c = match b.get(1) {
        Some(c) => *c,
        None => return err(file, p, "bad escape sequence"),
    };
    Ok(match c {
        b'n' => (b'\n' as u16, 2),
        b't' => (b'\t' as u16, 2),
        b'r' => (b'\r' as u16, 2),
        b'b' => (8, 2),
        b'f' => (12, 2),
        b's' => (b' ' as u16, 2),
        b'\'' => (b'\'' as u16, 2),
        b'"' => (b'"' as u16, 2),
        b'\\' => (b'\\' as u16, 2),
        b'u' => {
            let mut j = 1;
            while b.get(j) == Some(&b'u') {
                j += 1;
            }
            if b.len() < j + 4 {
                return err(file, p, "bad unicode escape");
            }
            let hex = std::str::from_utf8(&b[j..j + 4]).unwrap_or("");
            match u16::from_str_radix(hex, 16) {
                Ok(v) => (v, j + 4),
                Err(_) => return err(file, p, "bad unicode escape"),
            }
        }
        b'0'..=b'7' => {
            let max = if c <= b'3' { 3 } else { 2 };
            let mut v: u16 = 0;
            let mut j = 1;
            while j <= max && j < b.len() && (b'0'..=b'7').contains(&b[j]) {
                v = v * 8 + (b[j] - b'0') as u16;
                j += 1;
            }
            (v, j)
        }
        _ => return err(file, p, format!("illegal escape character '\\{}'", c as char)),
    })
}

fn lex_number(b: &[u8], file: usize, p: Pos) -> Result<(Tok, usize)> {
    let mut i = 0;
    // Radix-prefixed integers.
    if b[0] == b'0' && b.len() > 1 && matches!(b[1], b'x' | b'X' | b'b' | b'B') {
        let radix = if matches!(b[1], b'x' | b'X') { 16 } else { 2 };
        i = 2;
        let mut digits = String::new();
        while i < b.len() && ((b[i] as char).is_digit(radix) || b[i] == b'_') {
            if b[i] != b'_' {
                digits.push(b[i] as char);
            }
            i += 1;
        }
        if i < b.len() && (b[i] == b'.' || b[i] == b'p' || b[i] == b'P') {
            return err(file, p, "hexadecimal floating-point literals are not supported");
        }
        let is_long = i < b.len() && (b[i] == b'l' || b[i] == b'L');
        if is_long {
            i += 1;
        }
        let v = match u64::from_str_radix(&digits, radix) {
            Ok(v) => v,
            Err(_) => return err(file, p, "malformed or too large integer literal"),
        };
        return if is_long {
            Ok((Tok::Long(v), i))
        } else if v > u32::MAX as u64 {
            err(file, p, "integer number too large")
        } else {
            Ok((Tok::Int(v as u32 as i32 as i64), i))
        };
    }

    let mut text = String::new();
    let mut is_float = false;
    while i < b.len() && (b[i].is_ascii_digit() || b[i] == b'_') {
        if b[i] != b'_' {
            text.push(b[i] as char);
        }
        i += 1;
    }
    if i < b.len()
        && b[i] == b'.'
        && b.get(i + 1).map_or(true, |c| {
            !c.is_ascii_alphabetic() || matches!(c, b'e' | b'E' | b'd' | b'D' | b'f' | b'F')
        })
    {
        is_float = true;
        text.push('.');
        i += 1;
        while i < b.len() && (b[i].is_ascii_digit() || b[i] == b'_') {
            if b[i] != b'_' {
                text.push(b[i] as char);
            }
            i += 1;
        }
    }
    if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
        is_float = true;
        text.push('e');
        i += 1;
        if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
            text.push(b[i] as char);
            i += 1;
        }
        let start = i;
        while i < b.len() && b[i].is_ascii_digit() {
            text.push(b[i] as char);
            i += 1;
        }
        if start == i {
            return err(file, p, "malformed floating-point literal");
        }
    }
    if i < b.len() {
        match b[i] {
            b'f' | b'F' => {
                return err(file, p, "float is not supported (use double)");
            }
            b'd' | b'D' => {
                i += 1;
                is_float = true;
            }
            b'l' | b'L' if !is_float => {
                i += 1;
                let tok = if text.len() > 1 && text.starts_with('0') {
                    u64::from_str_radix(&text[1..], 8).ok().map(Tok::Long)
                } else {
                    text.parse::<u64>().ok().filter(|v| *v <= 1u64 << 63).map(Tok::Long)
                };
                return match tok {
                    Some(t) => Ok((t, i)),
                    None => err(file, p, "long number too large"),
                };
            }
            _ => {}
        }
    }
    if i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
        return err(file, p, "malformed number literal");
    }
    if is_float {
        let t = if text.starts_with('.') {
            format!("0{}", text)
        } else {
            text
        };
        return match t.parse::<f64>() {
            Ok(v) if v.is_finite() => Ok((Tok::Double(v), i)),
            _ => err(file, p, "malformed or too large floating-point literal"),
        };
    }
    // octal int?
    if text.len() > 1 && text.starts_with('0') {
        return match u64::from_str_radix(&text[1..], 8) {
            Ok(v) if v <= u32::MAX as u64 => Ok((Tok::Int(v as u32 as i32 as i64), i)),
            _ => err(file, p, "malformed or too large octal literal"),
        };
    }
    match text.parse::<u64>() {
        Ok(v) if v <= 1u64 << 31 => Ok((Tok::Int(v as i64), i)),
        _ => err(file, p, "integer number too large"),
    }
}
