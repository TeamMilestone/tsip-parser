//! SIP Address (name-addr / addr-spec) parser per RFC 3261 §25.1.
//!
//! Ported from `tsip-core`'s `lib/tsip_core/sip/address.rb`. Delegates URI
//! parsing to [`Uri::parse_range`] on the same string so the embedded URI is
//! parsed without an extra allocation.

use crate::error::ParseError;
use crate::scan::{self, downcase_str, slice_str, DQUOTE, EQ_, GT, LT, QMARK, SEMI};
use crate::uri::{parse_param_range, Uri};

/// Params that sit at the Address level rather than being folded into the
/// embedded URI. Must stay in sync with `Address::ADDRESS_PARAMS` in
/// `tsip-core/lib/tsip_core/sip/address.rb`.
pub const ADDRESS_PARAMS: &[&str] = &["tag", "q", "expires"];

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Address {
    pub display_name: Option<String>,
    pub uri: Option<Uri>,
    pub params: Vec<(String, String)>,
}

impl Address {
    pub fn parse(input: &str) -> Result<Self, ParseError> {
        let src = input.as_bytes();
        let (from, to) = scan::trim_ws(src, 0, input.len());

        // Count semicolons after a `<...>` for pre-sizing params capacity.
        let mut lt_idx = None;
        let mut j = from;
        while j < to {
            if src[j] == LT {
                lt_idx = Some(j);
                break;
            }
            j += 1;
        }

        let Some(lt_idx) = lt_idx else {
            return Address::parse_bare_range(input, from, to);
        };

        let mut gt_idx = None;
        let mut j = lt_idx + 1;
        while j < to {
            if src[j] == GT {
                gt_idx = Some(j);
                break;
            }
            j += 1;
        }
        let gt_idx = gt_idx.ok_or(ParseError::UnterminatedAngle)?;

        let display = extract_display(input, from, lt_idx);
        let uri = Uri::parse_range(input, lt_idx + 1, gt_idx)?;

        let mut params: Vec<(String, String)> = Vec::new();
        let mut seg_start = gt_idx + 1;
        while seg_start <= to {
            let mut seg_end = to;
            let mut k = seg_start;
            while k < to {
                if src[k] == SEMI {
                    seg_end = k;
                    break;
                }
                k += 1;
            }
            parse_param_range(input, seg_start, seg_end, &mut params)?;
            if seg_end == to {
                break;
            }
            seg_start = seg_end + 1;
        }

        Ok(Address {
            display_name: display,
            uri: Some(uri),
            params,
        })
    }

    fn parse_bare_range(input: &str, from: usize, to: usize) -> Result<Self, ParseError> {
        let src = input.as_bytes();
        let mut uri_end = to;
        let mut j = from;
        while j < to {
            if src[j] == SEMI {
                uri_end = j;
                break;
            }
            j += 1;
        }

        let mut uri = Uri::parse_range(input, from, uri_end)?;
        let mut params: Vec<(String, String)> = Vec::new();
        if uri_end < to {
            let mut seg_start = uri_end + 1;
            while seg_start <= to {
                let mut seg_end = to;
                let mut k = seg_start;
                while k < to {
                    if src[k] == SEMI {
                        seg_end = k;
                        break;
                    }
                    k += 1;
                }
                classify_bare_param(input, seg_start, seg_end, &mut uri, &mut params)?;
                if seg_end == to {
                    break;
                }
                seg_start = seg_end + 1;
            }
        }

        Ok(Address {
            display_name: None,
            uri: Some(uri),
            params,
        })
    }

    pub fn tag(&self) -> Option<&str> {
        self.params
            .iter()
            .find(|(k, _)| k == "tag")
            .map(|(_, v)| v.as_str())
    }

    pub fn set_tag(&mut self, tag: String) {
        for entry in self.params.iter_mut() {
            if entry.0 == "tag" {
                entry.1 = tag;
                return;
            }
        }
        self.params.push(("tag".into(), tag));
    }

    /// Rough serialized size for buffer pre-sizing.
    fn serialized_size_hint(&self) -> usize {
        let mut n = 2;
        if let Some(dn) = &self.display_name {
            n += dn.len() + 4;
        }
        if let Some(u) = &self.uri {
            n += u.serialized_size_hint();
        }
        for (k, v) in &self.params {
            n += k.len() + v.len() + 2;
        }
        n
    }

    pub fn append_to(&self, buf: &mut String) {
        buf.reserve(self.serialized_size_hint());
        if let Some(dn) = &self.display_name {
            buf.push('"');
            buf.push_str(dn);
            buf.push_str("\" ");
        }
        buf.push('<');
        if let Some(uri) = &self.uri {
            uri.append_to(buf);
        }
        buf.push('>');
        for (k, v) in &self.params {
            buf.push(';');
            buf.push_str(k);
            if !v.is_empty() {
                buf.push('=');
                buf.push_str(v);
            }
        }
    }
}

impl std::fmt::Display for Address {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut buf = String::with_capacity(self.serialized_size_hint());
        self.append_to(&mut buf);
        f.write_str(&buf)
    }
}

fn classify_bare_param(
    input: &str,
    from: usize,
    to: usize,
    uri: &mut Uri,
    params: &mut Vec<(String, String)>,
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

    let key = downcase_str(input, k_from, k_to);
    let val = match eq {
        Some(e) => slice_str(input, e + 1, to),
        None => String::new(),
    };

    // Any stored `>` prematurely terminates the Address `<...>` wrapper on
    // re-parse. Applies to both URI-embedded and Address-level params because
    // parse_param_range (used by the angle-path re-parser for both levels)
    // rejects `>`.
    if key.as_bytes().contains(&GT) || val.as_bytes().contains(&GT) {
        return Err(ParseError::InvalidHost);
    }

    if ADDRESS_PARAMS.contains(&key.as_str()) {
        upsert(params, key, val);
    } else {
        // Uri-embedded params also must not contain `?` — on angle-wrap
        // re-parse the URI body is scanned and `?` would split off as headers
        // start. `&` is safe (URI-level scan doesn't split on it).
        if key.as_bytes().contains(&QMARK) || val.as_bytes().contains(&QMARK) {
            return Err(ParseError::InvalidHost);
        }
        upsert(&mut uri.params, key, val);
    }
    Ok(())
}

fn upsert(target: &mut Vec<(String, String)>, key: String, val: String) {
    for entry in target.iter_mut() {
        if entry.0 == key {
            entry.1 = val;
            return;
        }
    }
    target.push((key, val));
}

fn extract_display(input: &str, from: usize, to: usize) -> Option<String> {
    let src = input.as_bytes();
    let (from, to) = scan::trim_sp_tab(src, from, to);
    if from == to {
        return None;
    }
    if to - from >= 2 && src[from] == DQUOTE && src[to - 1] == DQUOTE {
        return Some(slice_str(input, from + 1, to - 1));
    }
    Some(slice_str(input, from, to))
}
