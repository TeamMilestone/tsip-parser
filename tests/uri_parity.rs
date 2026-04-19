use tsip_parser::{ParseError, Uri};

fn get_param<'a>(uri: &'a Uri, key: &str) -> Option<&'a str> {
    uri.params
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
}

fn get_header<'a>(uri: &'a Uri, key: &str) -> Option<&'a str> {
    uri.headers
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
}

#[test]
fn parses_basic_sip_uri() {
    let u = Uri::parse("sip:alice@atlanta.example.com").unwrap();
    assert_eq!(u.scheme, "sip");
    assert_eq!(u.user.as_deref(), Some("alice"));
    assert_eq!(u.host, "atlanta.example.com");
    assert_eq!(u.port, None);
    assert!(u.password.is_none());
    assert!(u.params.is_empty());
    assert!(u.headers.is_empty());
}

#[test]
fn parses_sips_scheme_case_insensitive() {
    let u = Uri::parse("SIPS:alice@host").unwrap();
    assert_eq!(u.scheme, "sips");
    assert_eq!(u.user.as_deref(), Some("alice"));
}

#[test]
fn parses_tel_scheme() {
    let u = Uri::parse("tel:+15551234").unwrap();
    assert_eq!(u.scheme, "tel");
    assert_eq!(u.host, "+15551234");
    assert!(u.user.is_none());
}

#[test]
fn parses_port() {
    let u = Uri::parse("sip:alice@example.com:5060").unwrap();
    assert_eq!(u.host, "example.com");
    assert_eq!(u.port, Some(5060));
}

#[test]
fn parses_password() {
    let u = Uri::parse("sip:alice:secret@example.com").unwrap();
    assert_eq!(u.user.as_deref(), Some("alice"));
    assert_eq!(u.password.as_deref(), Some("secret"));
}

#[test]
fn parses_ipv6_host_with_port() {
    let u = Uri::parse("sip:alice@[2001:db8::1]:5060").unwrap();
    assert_eq!(u.host, "2001:db8::1");
    assert_eq!(u.port, Some(5060));
}

#[test]
fn parses_ipv6_host_no_port() {
    let u = Uri::parse("sip:[::1]").unwrap();
    assert_eq!(u.host, "::1");
    assert_eq!(u.port, None);
}

#[test]
fn parses_uri_params_preserving_order() {
    let u = Uri::parse("sip:alice@host;transport=tcp;lr;method=INVITE").unwrap();
    assert_eq!(u.params.len(), 3);
    assert_eq!(u.params[0].0, "transport");
    assert_eq!(u.params[0].1, "tcp");
    assert_eq!(u.params[1].0, "lr");
    assert_eq!(u.params[1].1, "");
    assert_eq!(u.params[2].0, "method");
    assert_eq!(u.params[2].1, "INVITE");
}

#[test]
fn downcases_param_keys_only() {
    let u = Uri::parse("sip:alice@host;Transport=TCP").unwrap();
    assert_eq!(u.params[0].0, "transport");
    assert_eq!(u.params[0].1, "TCP");
}

#[test]
fn parses_uri_headers() {
    let u = Uri::parse("sip:alice@host?subject=meeting&priority=urgent").unwrap();
    assert_eq!(get_header(&u, "subject"), Some("meeting"));
    assert_eq!(get_header(&u, "priority"), Some("urgent"));
}

#[test]
fn pct_decodes_user() {
    let u = Uri::parse("sip:%61lice@host").unwrap();
    assert_eq!(u.user.as_deref(), Some("alice"));
}

#[test]
fn pct_decodes_header_key_and_value() {
    let u = Uri::parse("sip:alice@host?sub%6Aect=hi%20there").unwrap();
    assert_eq!(get_header(&u, "subject"), Some("hi there"));
}

#[test]
fn trims_outer_whitespace() {
    let u = Uri::parse("   sip:alice@host   ").unwrap();
    assert_eq!(u.host, "host");
}

#[test]
fn params_come_before_headers() {
    let u = Uri::parse("sip:alice@host;transport=udp?subject=hi").unwrap();
    assert_eq!(get_param(&u, "transport"), Some("udp"));
    assert_eq!(get_header(&u, "subject"), Some("hi"));
}

#[test]
fn trailing_semicolon_in_params_is_tolerated() {
    // Ruby parse treats a trailing semi as an empty segment which becomes a
    // zero-length key after trim; the param loop skips empty segments.
    let u = Uri::parse("sip:alice@host;transport=udp;").unwrap();
    assert_eq!(u.params.len(), 1);
    assert_eq!(u.params[0].0, "transport");
}

#[test]
fn display_matches_parsed_input() {
    let raw = "sip:alice@atlanta.example.com;transport=tcp";
    assert_eq!(Uri::parse(raw).unwrap().to_string(), raw);
}

#[test]
fn display_reapplies_ipv6_brackets() {
    let raw = "sip:alice@[::1]:5060";
    assert_eq!(Uri::parse(raw).unwrap().to_string(), raw);
}

#[test]
fn scheme_detection_falls_back_to_sip() {
    // No recognized scheme prefix — Ruby falls back to "sip" and leaves the
    // whole body as user+host. Mirror that behavior.
    let u = Uri::parse("alice@host").unwrap();
    assert_eq!(u.scheme, "sip");
    assert_eq!(u.user.as_deref(), Some("alice"));
    assert_eq!(u.host, "host");
}

#[test]
fn empty_input_is_empty_sip_uri() {
    // Ruby allows an empty string: parse_range with from==to returns scheme=sip,
    // empty host. We match that rather than erroring, since tsip-core relies on it.
    let u = Uri::parse("").unwrap();
    assert_eq!(u.scheme, "sip");
    assert_eq!(u.host, "");
}

#[test]
fn nil_like_parse_error_variant_exists() {
    // Sanity: ParseError has the expected variants (compile-time check).
    let _ = ParseError::Empty;
}

#[test]
fn accepts_pct_encoded_at_in_user() {
    let u = Uri::parse("sip:%40alice@host").unwrap();
    assert_eq!(u.user.as_deref(), Some("@alice"));
    // literal @ in stored user must be re-escaped on render so the round-trip
    // reaches a fixed point.
    assert_eq!(u.to_string(), "sip:%40alice@host");
}

#[test]
fn accepts_literal_pct_in_user() {
    let u = Uri::parse("sip:al%25ice@host").unwrap();
    assert_eq!(u.user.as_deref(), Some("al%ice"));
    assert_eq!(u.to_string(), "sip:al%25ice@host");
}

#[test]
fn preserves_leading_ws_in_param_value() {
    let u = Uri::parse("sip:alice@host;transport= TCP").unwrap();
    assert_eq!(u.params, vec![("transport".into(), " TCP".into())]);
    assert_eq!(u.to_string(), "sip:alice@host;transport= TCP");
}

#[test]
fn preserves_leading_ws_in_header_value() {
    let u = Uri::parse("sip:alice@host?key= val").unwrap();
    assert_eq!(u.headers, vec![("key".into(), " val".into())]);
    // Header values are pct-decoded on parse, so the stored leading space must
    // be escaped on render to survive round-trip (outer segment trim_ws would
    // strip it otherwise). Re-parsing %20 restores the leading space.
    assert_eq!(u.to_string(), "sip:alice@host?key=%20val");
    let re = Uri::parse(&u.to_string()).unwrap();
    assert_eq!(re.headers, u.headers);
}

#[test]
fn escapes_pct_decoded_cr_in_header_key() {
    // Fuzz regression: `?%0D` decoded to key "\r", which lost round-trip
    // before header-side escape was added. The renderer must escape the CR.
    let u = Uri::parse("?%0D").unwrap();
    assert_eq!(u.headers, vec![("\r".into(), "".into())]);
    let rendered = u.to_string();
    let re = Uri::parse(&rendered).unwrap();
    assert_eq!(re.to_string(), rendered);
}

#[test]
fn accepts_lt_but_rejects_gt_in_param_key() {
    // `<` in a param key is permissive — it survives round-trip even when the
    // URI is wrapped in `<...>` by an Address, because the scanner still finds
    // the true closing `>` afterwards.
    let u = Uri::parse("sip:alice@host;<foo=1").unwrap();
    assert_eq!(u.params, vec![("<foo".into(), "1".into())]);

    // `>` in a param key/value is rejected: a stored `>` would terminate the
    // `<...>` wrapper prematurely on Address re-parse, breaking round-trip.
    assert_eq!(
        Uri::parse("sip:alice@host;foo>=1").unwrap_err(),
        ParseError::InvalidHost
    );
    assert_eq!(
        Uri::parse("sip:alice@host;foo=>").unwrap_err(),
        ParseError::InvalidHost
    );
}

#[test]
fn duplicate_param_keys_overwrite_in_place() {
    // Ruby Hash assignment overwrites without reordering — mirror that.
    let u = Uri::parse("sip:a@h;tag=one;x=y;tag=two").unwrap();
    assert_eq!(u.params.len(), 2);
    assert_eq!(u.params[0].0, "tag");
    assert_eq!(u.params[0].1, "two");
    assert_eq!(u.params[1].0, "x");
}
