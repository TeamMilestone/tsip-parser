//! RFC 3261 SIP message framing parser.
//!
//! Performs start-line + header canonicalisation + Content-Length validation
//! without touching structured header values (Via/CSeq/Contact/... remain raw
//! strings). Line folding per §7.3.1 is expanded into a single joined value.

use crate::error::ParseError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartLine {
    Request {
        method: String,
        request_uri: String,
        sip_version: String,
    },
    Response {
        sip_version: String,
        status_code: u16,
        reason_phrase: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub start_line: StartLine,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Message {
    pub const MAX_SIZE: usize = 65_536;

    pub fn parse(raw: &[u8]) -> Result<Message, ParseError> {
        parse_message(raw)
    }

    pub fn content_length(&self) -> Option<usize> {
        for (name, value) in &self.headers {
            if name == "Content-Length" {
                return value.trim().parse().ok();
            }
        }
        None
    }

    pub fn header(&self, canonical: &str) -> Option<&str> {
        for (name, value) in &self.headers {
            if name == canonical {
                return Some(value.as_str());
            }
        }
        None
    }
}

const SP: u8 = 0x20;
const HTAB: u8 = 0x09;
const CR: u8 = 0x0D;
const LF: u8 = 0x0A;

const COMPACT_MAP: &[(u8, &str)] = &[
    (b'i', "Call-ID"),
    (b'm', "Contact"),
    (b'e', "Content-Encoding"),
    (b'l', "Content-Length"),
    (b'c', "Content-Type"),
    (b'f', "From"),
    (b's', "Subject"),
    (b'k', "Supported"),
    (b't', "To"),
    (b'v', "Via"),
    (b'r', "Refer-To"),
    (b'b', "Referred-By"),
    (b'o', "Event"),
    (b'u', "Allow-Events"),
    (b'a', "Accept-Contact"),
    (b'j', "Reject-Contact"),
    (b'd', "Request-Disposition"),
    (b'x', "Session-Expires"),
    (b'y', "Identity"),
    (b'n', "Identity-Info"),
];

const CANONICAL_LIST: &[&str] = &[
    "Via",
    "From",
    "To",
    "Call-ID",
    "CSeq",
    "Contact",
    "Max-Forwards",
    "Expires",
    "Record-Route",
    "Route",
    "Authorization",
    "WWW-Authenticate",
    "Proxy-Authorization",
    "Proxy-Authenticate",
    "User-Agent",
    "Server",
    "Content-Type",
    "Content-Length",
    "Content-Encoding",
    "Content-Disposition",
    "Allow",
    "Supported",
    "Require",
    "Accept",
    "Accept-Encoding",
    "Accept-Language",
    "Subject",
    "Event",
    "Refer-To",
    "Referred-By",
    "Session-Expires",
    "Min-SE",
    "Reason",
    "Date",
    "Timestamp",
    "Warning",
    "Organization",
    "Priority",
];

fn parse_message(raw: &[u8]) -> Result<Message, ParseError> {
    if raw.is_empty() {
        return Err(ParseError::EmptyMessage);
    }
    if raw.len() > Message::MAX_SIZE {
        return Err(ParseError::MessageTooLarge);
    }

    let (headers_end, body_start) = find_headers_end(raw);
    let headers_section = &raw[..headers_end];

    let first_lf = memchr(headers_section, LF).ok_or(ParseError::InvalidStartLine)?;
    let start_line_bytes = strip_trailing_cr(&headers_section[..first_lf]);
    let start_line = parse_start_line(start_line_bytes)?;

    let mut headers: Vec<(String, String)> = Vec::new();
    let mut cursor = first_lf + 1;
    let mut current_name: Option<String> = None;
    let mut current_value: String = String::new();

    while cursor < headers_end {
        let lf = match memchr(&headers_section[cursor..], LF) {
            Some(rel) => cursor + rel,
            None => headers_end,
        };
        let line = strip_trailing_cr(&headers_section[cursor..lf]);
        cursor = lf + 1;

        if line.is_empty() {
            continue;
        }

        if is_sp_tab(line[0]) {
            if current_name.is_none() {
                return Err(ParseError::InvalidStartLine);
            }
            let trimmed = trim_sp_tab(line);
            let trimmed_str =
                std::str::from_utf8(trimmed).map_err(|_| ParseError::InvalidUtf8)?;
            if !current_value.is_empty() {
                current_value.push(' ');
            }
            current_value.push_str(trimmed_str);
            continue;
        }

        if let Some(name) = current_name.take() {
            headers.push((name, std::mem::take(&mut current_value)));
        }

        let colon = line
            .iter()
            .position(|&b| b == b':')
            .ok_or(ParseError::HeaderMissingColon)?;
        let name_bytes = trim_sp_tab(&line[..colon]);
        let value_bytes = trim_sp_tab(&line[colon + 1..]);
        if name_bytes.is_empty() {
            return Err(ParseError::HeaderMissingColon);
        }
        let name_str =
            std::str::from_utf8(name_bytes).map_err(|_| ParseError::InvalidUtf8)?;
        let value_str =
            std::str::from_utf8(value_bytes).map_err(|_| ParseError::InvalidUtf8)?;
        current_name = Some(canonical_of(name_str));
        current_value = value_str.to_owned();
    }

    if let Some(name) = current_name.take() {
        headers.push((name, current_value));
    }

    let body_bytes = &raw[body_start..];
    let mut body = body_bytes.to_vec();

    if let Some(raw_cl) = find_header_value(&headers, "Content-Length") {
        let cl = parse_content_length(raw_cl)?;
        if cl <= body.len() {
            body.truncate(cl);
        }
    }

    Ok(Message {
        start_line,
        headers,
        body,
    })
}

fn find_headers_end(raw: &[u8]) -> (usize, usize) {
    // Returns (headers_section_end_exclusive, body_start). The section
    // end includes the terminating CRLF of the final header line so
    // start-line / header iteration can always locate an LF boundary.
    if let Some(p) = find_sequence(raw, b"\r\n\r\n") {
        return (p + 2, p + 4);
    }
    if let Some(p) = find_sequence(raw, b"\n\n") {
        return (p + 1, p + 2);
    }
    (raw.len(), raw.len())
}

fn find_sequence(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || needle.len() > hay.len() {
        return None;
    }
    let limit = hay.len() - needle.len();
    for i in 0..=limit {
        if &hay[i..i + needle.len()] == needle {
            return Some(i);
        }
    }
    None
}

fn memchr(hay: &[u8], b: u8) -> Option<usize> {
    hay.iter().position(|&x| x == b)
}

fn strip_trailing_cr(line: &[u8]) -> &[u8] {
    if let Some(&last) = line.last() {
        if last == CR {
            return &line[..line.len() - 1];
        }
    }
    line
}

fn is_sp_tab(b: u8) -> bool {
    b == SP || b == HTAB
}

fn trim_sp_tab(s: &[u8]) -> &[u8] {
    let mut from = 0;
    let mut to = s.len();
    while from < to && is_sp_tab(s[from]) {
        from += 1;
    }
    while to > from && is_sp_tab(s[to - 1]) {
        to -= 1;
    }
    &s[from..to]
}

fn parse_start_line(line: &[u8]) -> Result<StartLine, ParseError> {
    let t = trim_sp_tab(line);
    if t.is_empty() {
        return Err(ParseError::InvalidStartLine);
    }
    let first_sp = t
        .iter()
        .position(|&b| b == SP || b == HTAB)
        .ok_or(ParseError::InvalidStartLine)?;
    let tok1 = &t[..first_sp];
    let mut p = first_sp;
    while p < t.len() && is_sp_tab(t[p]) {
        p += 1;
    }
    let rest2 = &t[p..];
    let (tok2, tok3): (&[u8], &[u8]) = match rest2.iter().position(|&b| b == SP || b == HTAB) {
        Some(i) => {
            let mut q = i;
            while q < rest2.len() && is_sp_tab(rest2[q]) {
                q += 1;
            }
            (&rest2[..i], trim_sp_tab(&rest2[q..]))
        }
        None => (rest2, &[][..]),
    };

    if tok1.is_empty() || tok2.is_empty() {
        return Err(ParseError::InvalidStartLine);
    }

    if starts_with_sip_slash(tok1) {
        let sip_version =
            std::str::from_utf8(tok1).map_err(|_| ParseError::InvalidUtf8)?.to_owned();
        let status_str =
            std::str::from_utf8(tok2).map_err(|_| ParseError::InvalidUtf8)?;
        if status_str.len() != 3 || !status_str.bytes().all(|b| b.is_ascii_digit()) {
            return Err(ParseError::InvalidStatusCode);
        }
        let status_code: u16 = status_str
            .parse()
            .map_err(|_| ParseError::InvalidStatusCode)?;
        let reason_phrase =
            std::str::from_utf8(tok3).map_err(|_| ParseError::InvalidUtf8)?.to_owned();
        Ok(StartLine::Response {
            sip_version,
            status_code,
            reason_phrase,
        })
    } else {
        if tok3.is_empty() {
            return Err(ParseError::InvalidStartLine);
        }
        let method = std::str::from_utf8(tok1)
            .map_err(|_| ParseError::InvalidUtf8)?
            .to_ascii_uppercase();
        let request_uri = std::str::from_utf8(tok2)
            .map_err(|_| ParseError::InvalidUtf8)?
            .to_owned();
        let sip_version = std::str::from_utf8(tok3)
            .map_err(|_| ParseError::InvalidUtf8)?
            .to_owned();
        if !sip_version.starts_with("SIP/") {
            return Err(ParseError::InvalidStartLine);
        }
        Ok(StartLine::Request {
            method,
            request_uri,
            sip_version,
        })
    }
}

fn starts_with_sip_slash(s: &[u8]) -> bool {
    s.len() >= 4
        && s[0].eq_ignore_ascii_case(&b'S')
        && s[1].eq_ignore_ascii_case(&b'I')
        && s[2].eq_ignore_ascii_case(&b'P')
        && s[3] == b'/'
}

fn canonical_of(name: &str) -> String {
    let bytes = name.as_bytes();
    if bytes.len() == 1 {
        let b = bytes[0].to_ascii_lowercase();
        for (ch, full) in COMPACT_MAP {
            if *ch == b {
                return (*full).to_owned();
            }
        }
    }
    for full in CANONICAL_LIST {
        if full.len() == bytes.len()
            && full
                .as_bytes()
                .iter()
                .zip(bytes.iter())
                .all(|(a, b)| a.eq_ignore_ascii_case(b))
        {
            return (*full).to_owned();
        }
    }
    capitalize_dashed(name)
}

fn capitalize_dashed(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut next_upper = true;
    for ch in s.chars() {
        if ch == '-' {
            out.push('-');
            next_upper = true;
        } else if next_upper {
            out.push(ch.to_ascii_uppercase());
            next_upper = false;
        } else {
            out.push(ch.to_ascii_lowercase());
        }
    }
    out
}

fn find_header_value<'a>(headers: &'a [(String, String)], canonical: &str) -> Option<&'a str> {
    for (name, value) in headers {
        if name == canonical {
            return Some(value.as_str());
        }
    }
    None
}

fn parse_content_length(s: &str) -> Result<usize, ParseError> {
    let t = s.trim();
    if t.is_empty() {
        return Err(ParseError::BadContentLength);
    }
    if t.starts_with('-') {
        return Err(ParseError::NegativeContentLength);
    }
    if !t.bytes().all(|b| b.is_ascii_digit()) {
        return Err(ParseError::BadContentLength);
    }
    let n: usize = t.parse().map_err(|_| ParseError::OversizeContentLength)?;
    if n > Message::MAX_SIZE {
        return Err(ParseError::OversizeContentLength);
    }
    Ok(n)
}
