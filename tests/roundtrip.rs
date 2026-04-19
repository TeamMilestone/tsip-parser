use tsip_parser::{Address, Uri};

const URI_CASES: &[&str] = &[
    "sip:alice@atlanta.example.com",
    "sips:bob@secure.example.com:5061",
    "sip:alice:pw@example.com;transport=tcp",
    "sip:alice@[2001:db8::1]:5060",
    "sip:carol@chicago.example.com;transport=tcp;lr",
    "sip:dave@host?subject=meeting&priority=urgent",
    "tel:+15551234",
];

#[test]
fn uri_round_trip_is_byte_identical() {
    for raw in URI_CASES {
        let once = Uri::parse(raw).unwrap().to_string();
        let twice = Uri::parse(&once).unwrap().to_string();
        assert_eq!(
            once, twice,
            "round-trip diverged for {raw:?}: {once:?} vs {twice:?}"
        );
    }
}

#[test]
fn uri_reparse_produces_identical_struct() {
    for raw in URI_CASES {
        let a = Uri::parse(raw).unwrap();
        let b = Uri::parse(&a.to_string()).unwrap();
        assert_eq!(a, b, "struct inequality for {raw:?}");
    }
}

const ADDRESS_CASES: &[&str] = &[
    r#""Alice" <sip:alice@example.com>;tag=abc"#,
    "<sip:bob@example.com>",
    "Alice <sip:alice@example.com>",
    r#""Bob Builder" <sips:bob@secure.example.com:5061>;tag=xyz;expires=300"#,
];

#[test]
fn address_round_trip_struct_stable() {
    for raw in ADDRESS_CASES {
        let a = Address::parse(raw).unwrap();
        let b = Address::parse(&a.to_string()).unwrap();
        assert_eq!(a, b, "address struct diverged for {raw:?}");
    }
}
