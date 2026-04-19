//! SIP URI parser per RFC 3261 §19.1 (plus tel: per RFC 3966).
//!
//! Ported directly from `tsip-core`'s `lib/tsip_core/sip/uri.rb` byte-scan
//! implementation. Field semantics and serialization order are preserved so
//! the two implementations stay byte-identical on `to_string` output.

use std::fmt;

use crate::error::ParseError;
use crate::scan::{
    self, digits_only, downcase_str, parse_u16, pct_decode, slice_str, AMP, AT, COLON, EQ_,
    LBRACKET, QMARK, RBRACKET, SEMI,
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
                user = Some(pct_decode(input, from, c)?);
                password = Some(pct_decode(input, c + 1, at_idx)?);
            } else {
                user = Some(pct_decode(input, from, at_idx)?);
            }
            at_idx + 1
        } else {
            from
        };

        let (host, port) = parse_host_port_range(input, host_start, uh_end);

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
                parse_param_range(input, seg_start, seg_end, &mut params);
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
            buf.push_str(user);
            if let Some(pw) = &self.password {
                buf.push(':');
                buf.push_str(pw);
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
            buf.push_str(k);
            if !v.is_empty() {
                buf.push('=');
                buf.push_str(v);
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
                buf.push_str(k);
                buf.push('=');
                buf.push_str(v);
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
) {
    let src = input.as_bytes();
    let (from, to) = scan::trim_sp_tab(src, from, to);
    if from == to {
        return;
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
    if let Some(eq) = eq {
        let key = downcase_str(input, from, eq);
        let val = slice_str(input, eq + 1, to);
        upsert(target, key, val);
    } else {
        let key = downcase_str(input, from, to);
        upsert(target, key, String::new());
    }
}

fn parse_header_range(
    input: &str,
    from: usize,
    to: usize,
    target: &mut Vec<(String, String)>,
) -> Result<(), ParseError> {
    let src = input.as_bytes();
    let mut eq = None;
    let mut j = from;
    while j < to {
        if src[j] == EQ_ {
            eq = Some(j);
            break;
        }
        j += 1;
    }
    if let Some(eq) = eq {
        let key = pct_decode(input, from, eq)?;
        let val = pct_decode(input, eq + 1, to)?;
        upsert(target, key, val);
    } else {
        let key = pct_decode(input, from, to)?;
        upsert(target, key, String::new());
    }
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

fn parse_host_port_range(input: &str, from: usize, to: usize) -> (String, Option<u16>) {
    if from == to {
        return (String::new(), None);
    }
    let src = input.as_bytes();
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
            return (slice_str(input, from, to), None);
        };

        let host = slice_str(input, from + 1, bracket);
        let rem_start = bracket + 1;
        if rem_start == to {
            return (host, None);
        }
        if src[rem_start] == COLON && digits_only(src, rem_start + 1, to) {
            let port = parse_u16(src, rem_start + 1, to);
            return (host, port);
        }
        return (host, None);
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
            return (slice_str(input, from, lc), port);
        }
    }
    (slice_str(input, from, to), None)
}
