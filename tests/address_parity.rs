use sip_uri::Address;

#[test]
fn name_addr_quoted() {
    let a = Address::parse(r#""Alice Liddell" <sip:alice@example.com>;tag=abc"#).unwrap();
    assert_eq!(a.display_name.as_deref(), Some("Alice Liddell"));
    let uri = a.uri.as_ref().unwrap();
    assert_eq!(uri.user.as_deref(), Some("alice"));
    assert_eq!(a.tag(), Some("abc"));
}

#[test]
fn addr_spec_bare() {
    let a = Address::parse("sip:bob@example.com").unwrap();
    assert!(a.display_name.is_none());
    assert_eq!(a.uri.as_ref().unwrap().user.as_deref(), Some("bob"));
    assert!(a.tag().is_none());
}

#[test]
fn display_no_quotes() {
    let a = Address::parse("Alice <sip:alice@example.com>").unwrap();
    assert_eq!(a.display_name.as_deref(), Some("Alice"));
}

#[test]
fn bare_with_tag() {
    let a = Address::parse("sip:alice@example.com;tag=xyz").unwrap();
    assert_eq!(a.tag(), Some("xyz"));
    // tag belongs to address params, not URI params
    assert!(a.uri.as_ref().unwrap().params.is_empty());
}

#[test]
fn bare_classifies_non_address_param_onto_uri() {
    let a = Address::parse("sip:alice@host;transport=tcp;tag=abc").unwrap();
    assert_eq!(a.tag(), Some("abc"));
    let uri = a.uri.as_ref().unwrap();
    assert_eq!(uri.params.len(), 1);
    assert_eq!(uri.params[0].0, "transport");
    assert_eq!(uri.params[0].1, "tcp");
}

#[test]
fn name_addr_params_all_belong_to_address() {
    let a = Address::parse("<sip:alice@host;transport=tcp>;tag=abc;expires=60").unwrap();
    // transport is inside <>, so it lives on the URI
    let uri = a.uri.as_ref().unwrap();
    assert_eq!(uri.params.len(), 1);
    assert_eq!(uri.params[0].0, "transport");
    // tag/expires are after <>, so they live on the Address — and in this mode
    // non-address keys would ALSO land on the Address (no classification).
    assert_eq!(a.params.len(), 2);
    assert_eq!(a.tag(), Some("abc"));
}

#[test]
fn unterminated_angle_errors() {
    let err = Address::parse("<sip:alice@host").unwrap_err();
    assert_eq!(err, sip_uri::ParseError::UnterminatedAngle);
}

#[test]
fn roundtrip_name_addr() {
    let a = Address::parse(r#""Alice" <sip:alice@example.com>;tag=1"#).unwrap();
    let re = Address::parse(&a.to_string()).unwrap();
    assert_eq!(a.display_name, re.display_name);
    assert_eq!(a.tag(), re.tag());
    assert_eq!(a.uri.as_ref().unwrap().user, re.uri.as_ref().unwrap().user);
}

#[test]
fn roundtrip_bare_preserves_classification() {
    let raw = "sip:alice@host;transport=tcp;tag=abc";
    let a = Address::parse(raw).unwrap();
    // to_string of bare Address wraps in <> since our serializer always does
    // (Ruby does the same). The re-parsed round-trip should have identical
    // semantics even though the textual form differs.
    let re = Address::parse(&a.to_string()).unwrap();
    assert_eq!(re.tag(), Some("abc"));
    assert_eq!(
        re.uri.as_ref().unwrap().params[0],
        ("transport".to_string(), "tcp".to_string())
    );
}

#[test]
fn set_tag_replaces_existing() {
    let mut a = Address::parse("<sip:a@h>;tag=old").unwrap();
    a.set_tag("new".into());
    assert_eq!(a.tag(), Some("new"));
    assert_eq!(a.params.len(), 1);
}

#[test]
fn set_tag_appends_when_missing() {
    let mut a = Address::parse("<sip:a@h>").unwrap();
    a.set_tag("fresh".into());
    assert_eq!(a.tag(), Some("fresh"));
}
