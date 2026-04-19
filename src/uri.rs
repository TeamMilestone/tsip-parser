//! SIP URI parser per RFC 3261 §19.1 (plus tel: per RFC 3966).
//!
//! Ported directly from `tsip-core`'s `lib/tsip_core/sip/uri.rb` byte-scan
//! implementation. Field semantics and serialization order are preserved so
//! the two implementations stay byte-identical on `to_string` output.

use std::fmt;

use crate::error::ParseError;
use crate::scan::{
    self, digits_only, downcase_str, parse_u16, pct_decode, slice_str, AMP, AT, COLON, EQ_, GT,
    LBRACKET, LT, QMARK, RBRACKET, SEMI,
};

/// Parsed SIP or tel URI.
///
/// `params`/`headers` are `Vec<(String, String)>` — not a map — to preserve
/// insertion order (required for round-trip parity with the Ruby source) and
/// because typical SIP URIs carry ≤ 3 params, where linear search beats hashing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Uri {
    pub scheme: &'static str,
    pub user: Option<String>,
    pub password: Option<String>,
    pub host: String,
    pub port: Option<u16>,
    pub params: Vec<(String, String)>,
    pub headers: Vec<(String, String)>,
}

impl Default for Uri {
    fn default() -> Self {
        Uri {
            scheme: "sip",
            user: None,
            password: None,
            host: String::new(),
            port: None,
            params: Vec::new(),
            headers: Vec::new(),
        }
    }
}

impl Uri {
    pub fn parse(input: &str) -> Result<Self, ParseError> {
        let (from, to) = scan::trim_ws(input.as_bytes(), 0, input.len());
        Uri::parse_range(input, from, to)
    }

    /// Parse `input[from..to]` as a URI. The caller must have already trimmed
    /// outer whitespace. `from`/`to` must be ASCII-aligned byte offsets (this
    /// is always true when produced by our own scanners — we only break on
    /// ASCII delimiters).
    pub fn parse_range(input: &str, from: usize, to: usize) -> Result<Self, ParseError> {
        let (scheme, from) = scan::detect_scheme(input.as_bytes(), from, to);
        let src = input.as_bytes();

        // Single forward pass: locate `?` (header start), first `;` before
        // it (params start), last `@` before params/headers (userinfo split).
        // Also count `;` separators before `?` and `&` separators after `?`
        // so we can pre-size Vec capacities and skip the first-push grow.
        let mut q_idx: Option<usize> = None;
        let mut uh_end: Option<usize> = None;
        let mut at_idx: Option<usize> = None;
        let mut semi_count: usize = 0;
        let mut amp_count: usize = 0;
        let mut j = from;
        while j < to {
            let b = src[j];
            if b == QMARK {
                q_idx = Some(j);
                j += 1;
                break;
            } else if b == SEMI {
                if uh_end.is_none() {
                    uh_end = Some(j);
                }
                semi_count += 1;
            } else if b == AT && uh_end.is_none() {
                at_idx = Some(j);
            }
            j += 1;
        }
        // Finish the scan if we broke out on `?`, counting `&` for headers.
        while j < to {
            if src[j] == AMP {
                amp_count += 1;
            }
            j += 1;
        }
        let body_end = q_idx.unwrap_or(to);
        let uh_end = uh_end.unwrap_or(body_end);
        // `semi_count` counted every `;` up to `?`; the first one is the
        // params-start marker (not a separator), so param segments = count.
        // When uh_end == body_end (no params), semi_count may also be 0.
        let params_cap = semi_count;
        let headers_cap = if q_idx.is_some() { amp_count + 1 } else { 0 };

        let mut user = None;
        let mut password = None;
        let host_start = if let Some(at_idx) = at_idx {
            let mut colon_idx = None;
            let mut k = from;
            while k < at_idx {
                if src[k] == COLON {
                    colon_idx = Some(k);
                    break;
                }
                k += 1;
            }
            if let Some(c) = colon_idx {
                let u = pct_decode(input, from, c)?;
                let p = pct_decode(input, c + 1, at_idx)?;
                user = Some(u);
                password = Some(p);
            } else {
                let u = pct_decode(input, from, at_idx)?;
                user = Some(u);
            }
            at_idx + 1
        } else {
            from
        };

        let (host, port) = parse_host_port_range(input, host_start, uh_end)?;

        let mut params: Vec<(String, String)> = if params_cap > 0 {
            Vec::with_capacity(params_cap)
        } else {
            Vec::new()
        };
        if uh_end < body_end {
            let mut seg_start = uh_end + 1;
            while seg_start <= body_end {
                let mut seg_end = body_end;
                let mut k = seg_start;
                while k < body_end {
                    if src[k] == SEMI {
                        seg_end = k;
                        break;
                    }
                    k += 1;
                }
                parse_param_range(input, seg_start, seg_end, &mut params)?;
                if seg_end == body_end {
                    break;
                }
                seg_start = seg_end + 1;
            }
        }

        let mut headers: Vec<(String, String)> = if headers_cap > 0 {
            Vec::with_capacity(headers_cap)
        } else {
            Vec::new()
        };
        if let Some(q) = q_idx {
            let mut seg_start = q + 1;
            while seg_start <= to {
                let mut seg_end = to;
                let mut k = seg_start;
                while k < to {
                    if src[k] == AMP {
                        seg_end = k;
                        break;
                    }
                    k += 1;
                }
                parse_header_range(input, seg_start, seg_end, &mut headers)?;
                if seg_end == to {
                    break;
                }
                seg_start = seg_end + 1;
            }
        }

        Ok(Uri {
            scheme,
            user,
            password,
            host,
            port,
            params,
            headers,
        })
    }

    /// Parse a single `key[=value]` param segment (as produced by splitting a
    /// URI body on `;`) into `target`. Public entry point over
    /// [`parse_param_range`] so external bindings can feed one segment at a
    /// time — used by tsip-core's `Via.parse` which has already split the
    /// header into params before reaching the URI crate.
    pub fn parse_param(raw: &str, target: &mut Vec<(String, String)>) -> Result<(), ParseError> {
        parse_param_range(raw, 0, raw.len(), target)
    }

    /// Parse a `host[:port]` fragment (including the bracketed-IPv6 form
    /// `[::1]:5060`). Public entry point over [`parse_host_port_range`] so
    /// external bindings can reuse the scanner without re-implementing the
    /// IPv6 / last-colon heuristics.
    pub fn parse_host_port(hp: &str) -> Result<(String, Option<u16>), ParseError> {
        parse_host_port_range(hp, 0, hp.len())
    }

    pub fn transport(&self) -> String {
        for (k, v) in &self.params {
            if k == "transport" {
                return v.to_ascii_lowercase();
            }
        }
        String::new()
    }

    pub fn aor(&self) -> String {
        let mut out = String::with_capacity(self.scheme.len() + 1 + self.host.len() + 16);
        out.push_str(self.scheme);
        out.push(':');
        if let Some(u) = &self.user {
            out.push_str(u);
            out.push('@');
        }
        out.push_str(&self.host);
        out
    }

    pub fn host_port(&self) -> String {
        let mut out = String::with_capacity(self.host.len() + 8);
        self.append_bracket_host(&mut out);
        if let Some(p) = self.port {
            out.push(':');
            let _ = write_u16(&mut out, p);
        }
        out
    }

    pub fn bracket_host(&self) -> String {
        let mut out = String::with_capacity(self.host.len() + 2);
        self.append_bracket_host(&mut out);
        out
    }

    fn append_bracket_host(&self, buf: &mut String) {
        if self.host.contains(':') && !self.host.starts_with('[') {
            buf.push('[');
            buf.push_str(&self.host);
            buf.push(']');
        } else {
            buf.push_str(&self.host);
        }
    }

    /// Approximate the serialized size to pre-size output buffers and avoid
    /// the grow path in `to_string`/`append_to`.
    pub(crate) fn serialized_size_hint(&self) -> usize {
        let mut n = self.scheme.len() + 1 + self.host.len() + 2;
        if let Some(u) = &self.user {
            n += u.len() + 1;
        }
        if let Some(pw) = &self.password {
            n += pw.len() + 1;
        }
        if self.port.is_some() {
            n += 6;
        }
        for (k, v) in &self.params {
            n += k.len() + v.len() + 2;
        }
        if !self.headers.is_empty() {
            n += 1;
            for (k, v) in &self.headers {
                n += k.len() + v.len() + 2;
            }
        }
        n
    }

    /// Serialize into an external buffer. Used by [`crate::Address::append_to`]
    /// to avoid an intermediate `String` allocation.
    pub fn append_to(&self, buf: &mut String) {
        buf.reserve(self.serialized_size_hint());
        buf.push_str(self.scheme);
        buf.push(':');
        if let Some(user) = &self.user {
            append_pct_escaped(buf, user);
            if let Some(pw) = &self.password {
                buf.push(':');
                append_pct_escaped(buf, pw);
            }
            buf.push('@');
        }
        self.append_bracket_host(buf);
        if let Some(port) = self.port {
            buf.push(':');
            let _ = write_u16(buf, port);
        }
        for (k, v) in &self.params {
            buf.push(';');
            append_param_escaped(buf, k);
            if !v.is_empty() {
                buf.push('=');
                append_param_escaped(buf, v);
            }
        }
        if !self.headers.is_empty() {
            buf.push('?');
            let mut first = true;
            for (k, v) in &self.headers {
                if !first {
                    buf.push('&');
                }
                first = false;
                append_pct_escaped(buf, k);
                buf.push('=');
                append_pct_escaped(buf, v);
            }
        }
    }
}

impl fmt::Display for Uri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut buf = String::with_capacity(self.serialized_size_hint());
        self.append_to(&mut buf);
        f.write_str(&buf)
    }
}

/// Write a u16 decimal into a String without going through the formatter
/// machinery (saves an alloc vs `p.to_string()`).
#[inline]
fn write_u16(out: &mut String, mut n: u16) -> fmt::Result {
    if n == 0 {
        out.push('0');
        return Ok(());
    }
    let mut buf = [0u8; 5];
    let mut i = buf.len();
    while n > 0 {
        i -= 1;
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
    }
    // SAFETY not needed: bytes are ASCII digits.
    out.push_str(std::str::from_utf8(&buf[i..]).unwrap());
    Ok(())
}

pub(crate) fn parse_param_range(
    input: &str,
    from: usize,
    to: usize,
    target: &mut Vec<(String, String)>,
) -> Result<(), ParseError> {
    let src = input.as_bytes();
    // Full ws trim (not just SP/HTAB) at the segment level first, so CR/LF-only
    // segments are skipped. See §13 of HANDOVER.md.
    let (from, to) = scan::trim_ws(src, from, to);
    if from == to {
        return Ok(());
    }
    let mut eq = None;
    let mut j = from;
    while j < to {
        if src[j] == EQ_ {
            eq = Some(j);
            break;
        }
        j += 1;
    }
    // Trim ws from the key and value ranges separately. Trailing ws in a key
    // (e.g. input `;P =;` → key `"P "`) would be stripped on re-parse because
    // the outer `Uri::parse` trim_ws strips trailing ws from the *whole* URI;
    // matching that here keeps the fixed point.
    let (k_from, k_to) = match eq {
        Some(eq) => scan::trim_ws(src, from, eq),
        None => (from, to),
    };
    if k_from == k_to {
        return Ok(());
    }
    let (key, val) = if let Some(eq) = eq {
        (
            downcase_str(input, k_from, k_to),
            slice_str(input, eq + 1, to),
        )
    } else {
        (downcase_str(input, k_from, k_to), String::new())
    };
    // Params are stored literally (no pct-decode). Structural bytes like `>`,
    // `;`, `?`, `=` are not rejected here — `Uri::append_to` escapes them on
    // render so the fixed point still holds after one cycle of case folding.
    upsert(target, key, val);
    Ok(())
}

fn parse_header_range(
    input: &str,
    from: usize,
    to: usize,
    target: &mut Vec<(String, String)>,
) -> Result<(), ParseError> {
    let src = input.as_bytes();
    let (from, to) = scan::trim_ws(src, from, to);
    if from == to {
        return Ok(());
    }
    let mut eq = None;
    let mut j = from;
    while j < to {
        if src[j] == EQ_ {
            eq = Some(j);
            break;
        }
        j += 1;
    }
    let (k_from, k_to) = match eq {
        Some(eq) => scan::trim_ws(src, from, eq),
        None => (from, to),
    };
    if k_from == k_to {
        return Ok(());
    }
    let (key, val) = if let Some(eq) = eq {
        (
            pct_decode(input, k_from, k_to)?,
            pct_decode(input, eq + 1, to)?,
        )
    } else {
        (pct_decode(input, k_from, k_to)?, String::new())
    };
    upsert(target, key, val);
    Ok(())
}

/// Ruby `Hash[k] = v` semantics: overwrite existing entry in place, keeping
/// its insertion position; otherwise append.
fn upsert(target: &mut Vec<(String, String)>, key: String, val: String) {
    for entry in target.iter_mut() {
        if entry.0 == key {
            entry.1 = val;
            return;
        }
    }
    target.push((key, val));
}

fn parse_host_port_range(
    input: &str,
    from: usize,
    to: usize,
) -> Result<(String, Option<u16>), ParseError> {
    let src = input.as_bytes();
    // Normalize host boundary whitespace (SP/HTAB/CR/LF). The outer Uri::parse
    // trim_ws only strips the whole-input edges; without this the parser would
    // accept `"sip:A "` with host=`"A "`, but re-parsing the rendered output
    // `"sip:A "` would trim to `"A"` — round-trip unstable.
    let (from, to) = scan::trim_ws(src, from, to);
    if from == to {
        return Ok((String::new(), None));
    }
    if src[from] == LBRACKET {
        let mut bracket = None;
        let mut j = from + 1;
        while j < to {
            if src[j] == RBRACKET {
                bracket = Some(j);
                break;
            }
            j += 1;
        }
        let Some(bracket) = bracket else {
            let host = slice_str(input, from, to);
            validate_host(&host)?;
            return Ok((host, None));
        };

        let host = slice_str(input, from + 1, bracket);
        validate_host(&host)?;
        let rem_start = bracket + 1;
        if rem_start == to {
            return Ok((host, None));
        }
        if src[rem_start] == COLON && digits_only(src, rem_start + 1, to) {
            let port = parse_u16(src, rem_start + 1, to);
            return Ok((host, port));
        }
        return Ok((host, None));
    }

    let mut last_colon = None;
    let mut j = to as isize - 1;
    while j >= from as isize {
        if src[j as usize] == COLON {
            last_colon = Some(j as usize);
            break;
        }
        j -= 1;
    }

    if let Some(lc) = last_colon {
        if digits_only(src, lc + 1, to) {
            let port = parse_u16(src, lc + 1, to);
            let host = slice_str(input, from, lc);
            validate_host(&host)?;
            return Ok((host, port));
        }
    }
    let host = slice_str(input, from, to);
    validate_host(&host)?;
    Ok((host, None))
}

/// Host must not carry structural bytes used by the Address wrapper (`<`, `>`)
/// or whitespace/control bytes. Also reject `[`/`]` in the stored host — the
/// IPv6-bracket parser strips the outer brackets before arriving here, so any
/// remaining bracket is structural garbage that round-trip cannot preserve.
/// See §13 of `docs/HANDOVER.md`.
fn validate_host(host: &str) -> Result<(), ParseError> {
    for &b in host.as_bytes() {
        if b == LT || b == GT || b == LBRACKET || b == RBRACKET || scan::is_ws(b) {
            return Err(ParseError::InvalidHost);
        }
    }
    Ok(())
}

const HEX_UPPER: &[u8; 16] = b"0123456789ABCDEF";
const HEX_LOWER: &[u8; 16] = b"0123456789abcdef";

/// Escape bytes of a pct-decoded field (userinfo, header key/value) so that
/// render+re-parse reaches a fixed point. Covers Address brackets, URI-level
/// delimiters, literal `%` (re-parse would decode it again), and whitespace
/// (edge ws would be stripped by outer trim_ws on re-parse; interior ws is
/// escaped conservatively).
fn append_pct_escaped(buf: &mut String, src: &str) {
    // Iterate chars so non-ASCII codepoints round-trip as their original UTF-8
    // bytes (writing `b as char` would reinterpret the byte as a Unicode
    // codepoint and re-encode it, corrupting multibyte sequences).
    for ch in src.chars() {
        match ch {
            '@' | ':' | ';' | '?' | '<' | '>' | '%' | '&' | '='
            | ' ' | '\t' | '\r' | '\n' => {
                let b = ch as u8;
                buf.push('%');
                buf.push(HEX_UPPER[(b >> 4) as usize] as char);
                buf.push(HEX_UPPER[(b & 0x0F) as usize] as char);
            }
            _ => buf.push(ch),
        }
    }
}

/// Escape bytes of a param key/value on render. Params are stored literally
/// (no pct-decode on parse), so only the bytes that would re-tokenize the
/// URI body on re-parse need escaping: `;` (param separator), `?` (params →
/// headers boundary), `&` (header separator — conservative), `=` (key/value
/// separator), and `<`/`>` (Address wrapping). `%` is not escaped because
/// params are not pct-decoded; escaping it would turn stored `%3c` into
/// `%253c` on re-render and break the fixed point. Whitespace is not escaped
/// because the param-segment trim_ws does not strip interior ws and
/// `parse_param_range` re-stores leading value ws verbatim.
///
/// Hex digits are emitted lowercase so that re-parse of the rendered output
/// (which `downcase_str`s param keys) round-trips to the same byte pattern
/// in one cycle.
fn append_param_escaped(buf: &mut String, src: &str) {
    for ch in src.chars() {
        match ch {
            ';' | '?' | '&' | '=' | '<' | '>' => {
                let b = ch as u8;
                buf.push('%');
                buf.push(HEX_LOWER[(b >> 4) as usize] as char);
                buf.push(HEX_LOWER[(b & 0x0F) as usize] as char);
            }
            _ => buf.push(ch),
        }
    }
}
