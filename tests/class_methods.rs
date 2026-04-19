//! Coverage for the class-method-style entry points exposed for FFI bindings
//! (tsip-core's `TsipCore::Sip::Uri = TsipParser::Uri` alias). These mirror
//! the Rust surface that the magnus/rb-sys wrappers call into.

use tsip_parser::Uri;

#[test]
fn parse_range_slices_inner_uri() {
    // Address-wrapping context: tsip-core's `Address.parse_bare_range` hands
    // a substring `[lt+1, gt)` to `Uri.parse_range` so the embedded URI is
    // parsed without allocating a new `String`.
    let full = "<sip:alice@host:5060>";
    let u = Uri::parse_range(full, 1, full.len() - 1).unwrap();
    assert_eq!(u.user.as_deref(), Some("alice"));
    assert_eq!(u.host, "host");
    assert_eq!(u.port, Some(5060));
}

#[test]
fn parse_param_single_segment() {
    let mut target: Vec<(String, String)> = Vec::new();
    Uri::parse_param("transport=tls", &mut target).unwrap();
    assert_eq!(target, vec![("transport".into(), "tls".into())]);
}

#[test]
fn parse_param_flag_style_has_empty_value() {
    let mut target: Vec<(String, String)> = Vec::new();
    Uri::parse_param("lr", &mut target).unwrap();
    assert_eq!(target, vec![("lr".into(), "".into())]);
}

#[test]
fn parse_param_downcases_key_preserves_value_case() {
    let mut target: Vec<(String, String)> = Vec::new();
    Uri::parse_param("Transport=TCP", &mut target).unwrap();
    assert_eq!(target, vec![("transport".into(), "TCP".into())]);
}

#[test]
fn parse_host_port_plain() {
    let (host, port) = Uri::parse_host_port("example.com:5060").unwrap();
    assert_eq!(host, "example.com");
    assert_eq!(port, Some(5060));
}

#[test]
fn parse_host_port_no_port() {
    let (host, port) = Uri::parse_host_port("example.com").unwrap();
    assert_eq!(host, "example.com");
    assert_eq!(port, None);
}

#[test]
fn parse_host_port_ipv6_with_port() {
    let (host, port) = Uri::parse_host_port("[::1]:5060").unwrap();
    assert_eq!(host, "::1");
    assert_eq!(port, Some(5060));
}

#[test]
fn parse_host_port_ipv6_no_port() {
    let (host, port) = Uri::parse_host_port("[2001:db8::1]").unwrap();
    assert_eq!(host, "2001:db8::1");
    assert_eq!(port, None);
}
