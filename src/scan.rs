//! Byte-level scanning helpers shared by the Uri and Address parsers.
//!
//! Callers pass the original `&str` plus byte offsets into it. Since every
//! delimiter we split on is an ASCII byte (`;`, `?`, `=`, `&`, `@`, `:`, `<`,
//! `>`, `"`, `[`, `]`, SP/HTAB/CR/LF), the recorded offsets always fall on
//! UTF-8 char boundaries, so `&input[from..to]` is a safe direct slice with
//! no revalidation cost. `pct_decode` is the exception — its output can
//! contain arbitrary bytes, so it validates.

use crate::error::ParseError;

pub const SP: u8 = 0x20;
pub const HTAB: u8 = 0x09;
pub const CR: u8 = 0x0D;
pub const LF: u8 = 0x0A;
pub const COLON: u8 = 0x3A;
pub const SEMI: u8 = 0x3B;
pub const QMARK: u8 = 0x3F;
pub const AMP: u8 = 0x26;
pub const EQ_: u8 = 0x3D;
pub const AT: u8 = 0x40;
pub const PCT: u8 = 0x25;
pub const LBRACKET: u8 = 0x5B;
pub const RBRACKET: u8 = 0x5D;
pub const LT: u8 = 0x3C;
pub const GT: u8 = 0x3E;
pub const DQUOTE: u8 = 0x22;

#[inline]
pub fn is_ws(b: u8) -> bool {
    b == SP || b == HTAB || b == CR || b == LF
}

#[inline]
pub fn is_sp_tab(b: u8) -> bool {
    b == SP || b == HTAB
}

#[inline]
pub fn trim_ws(src: &[u8], mut from: usize, mut to: usize) -> (usize, usize) {
    while from < to && is_ws(src[from]) {
        from += 1;
    }
    while to > from && is_ws(src[to - 1]) {
        to -= 1;
    }
    (from, to)
}

#[inline]
pub fn trim_sp_tab(src: &[u8], mut from: usize, mut to: usize) -> (usize, usize) {
    while from < to && is_sp_tab(src[from]) {
        from += 1;
    }
    while to > from && is_sp_tab(src[to - 1]) {
        to -= 1;
    }
    (from, to)
}

#[inline]
fn ieq_byte(b: u8, lower: u8) -> bool {
    b == lower || b == lower.wrapping_sub(32)
}

/// Detect the URI scheme prefix. Returns the static scheme literal and the
/// byte offset past the `:` separator.
pub fn detect_scheme(src: &[u8], from: usize, to: usize) -> (&'static str, usize) {
    let size = to - from;
    if size >= 5
        && ieq_byte(src[from], b's')
        && ieq_byte(src[from + 1], b'i')
        && ieq_byte(src[from + 2], b'p')
        && ieq_byte(src[from + 3], b's')
        && src[from + 4] == COLON
    {
        ("sips", from + 5)
    } else if size >= 4
        && ieq_byte(src[from], b's')
        && ieq_byte(src[from + 1], b'i')
        && ieq_byte(src[from + 2], b'p')
        && src[from + 3] == COLON
    {
        ("sip", from + 4)
    } else if size >= 4
        && ieq_byte(src[from], b't')
        && ieq_byte(src[from + 1], b'e')
        && ieq_byte(src[from + 2], b'l')
        && src[from + 3] == COLON
    {
        ("tel", from + 4)
    } else {
        ("sip", from)
    }
}

#[inline]
pub fn digits_only(src: &[u8], from: usize, to: usize) -> bool {
    if from >= to {
        return false;
    }
    let mut j = from;
    while j < to {
        let b = src[j];
        if !(0x30..=0x39).contains(&b) {
            return false;
        }
        j += 1;
    }
    true
}

#[inline]
pub fn parse_u16(src: &[u8], from: usize, to: usize) -> Option<u16> {
    let mut n: u32 = 0;
    let mut j = from;
    while j < to {
        n = n * 10 + (src[j] - 0x30) as u32;
        if n > u16::MAX as u32 {
            return None;
        }
        j += 1;
    }
    Some(n as u16)
}

#[inline]
fn hex_value(b: u8) -> i16 {
    match b {
        0x30..=0x39 => (b - 0x30) as i16,
        0x41..=0x46 => (b - 0x41 + 10) as i16,
        0x61..=0x66 => (b - 0x61 + 10) as i16,
        _ => -1,
    }
}

/// Direct `&str` slice copy — zero-validation. Callers must have produced
/// `from`/`to` via our scanner (ASCII delimiter positions) so `&input[from..to]`
/// is a valid char-boundary slice.
#[inline]
pub fn slice_str(input: &str, from: usize, to: usize) -> String {
    input[from..to].to_owned()
}

/// ASCII-lowercase the slice `input[from..to]`. Uses `str::to_ascii_lowercase`
/// which preserves UTF-8 validity without revalidation.
#[inline]
pub fn downcase_str(input: &str, from: usize, to: usize) -> String {
    input[from..to].to_ascii_lowercase()
}

/// pct-decode `input[from..to]`. Result may contain arbitrary bytes, so we
/// validate UTF-8 on the output. Fast path: if no `%` present, delegate to
/// `slice_str` and skip allocation of an intermediate Vec<u8>.
pub fn pct_decode(input: &str, from: usize, to: usize) -> Result<String, ParseError> {
    let src = input.as_bytes();
    let mut j = from;
    while j < to {
        if src[j] == PCT {
            break;
        }
        j += 1;
    }
    if j == to {
        return Ok(slice_str(input, from, to));
    }
    let mut out = Vec::with_capacity(to - from);
    out.extend_from_slice(&src[from..j]);
    while j < to {
        let b = src[j];
        if b == PCT && j + 2 < to {
            let h1 = hex_value(src[j + 1]);
            let h2 = hex_value(src[j + 2]);
            if h1 >= 0 && h2 >= 0 {
                out.push(((h1 << 4) | h2) as u8);
                j += 3;
                continue;
            }
        }
        out.push(b);
        j += 1;
    }
    String::from_utf8(out).map_err(|_| ParseError::InvalidUtf8)
}
