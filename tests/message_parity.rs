use tsip_parser::{Message, ParseError, StartLine};

fn parse(raw: &[u8]) -> Message {
    Message::parse(raw).expect("parse succeeded")
}

fn req(msg: &Message) -> (&str, &str, &str) {
    match &msg.start_line {
        StartLine::Request {
            method,
            request_uri,
            sip_version,
        } => (method.as_str(), request_uri.as_str(), sip_version.as_str()),
        _ => panic!("expected Request"),
    }
}

fn resp(msg: &Message) -> (&str, u16, &str) {
    match &msg.start_line {
        StartLine::Response {
            sip_version,
            status_code,
            reason_phrase,
        } => (sip_version.as_str(), *status_code, reason_phrase.as_str()),
        _ => panic!("expected Response"),
    }
}

fn header_values<'a>(msg: &'a Message, canonical: &str) -> Vec<&'a str> {
    msg.headers
        .iter()
        .filter(|(n, _)| n == canonical)
        .map(|(_, v)| v.as_str())
        .collect()
}

// ---- good corpus (requests + responses, compact/folded/body) ----

#[test]
fn invite_basic() {
    let raw = b"INVITE sip:bob@biloxi.example.com SIP/2.0\r\n\
                Via: SIP/2.0/UDP pc33.atlanta.example.com;branch=z9hG4bK776asdhds\r\n\
                Max-Forwards: 70\r\n\
                To: Bob <sip:bob@biloxi.example.com>\r\n\
                From: Alice <sip:alice@atlanta.example.com>;tag=1928301774\r\n\
                Call-ID: a84b4c76e66710\r\n\
                CSeq: 314159 INVITE\r\n\
                Contact: <sip:alice@pc33.atlanta.example.com>\r\n\
                Content-Type: application/sdp\r\n\
                Content-Length: 0\r\n\
                \r\n";
    let m = parse(raw);
    let (meth, uri, ver) = req(&m);
    assert_eq!(meth, "INVITE");
    assert_eq!(uri, "sip:bob@biloxi.example.com");
    assert_eq!(ver, "SIP/2.0");
    assert_eq!(m.header("Max-Forwards"), Some("70"));
    assert_eq!(m.header("Call-ID"), Some("a84b4c76e66710"));
    assert_eq!(m.content_length(), Some(0));
    assert!(m.body.is_empty());
}

#[test]
fn register() {
    let raw = b"REGISTER sip:registrar.biloxi.example.com SIP/2.0\r\n\
                Via: SIP/2.0/UDP bobspc.biloxi.example.com:5060;branch=z9hG4bKnashds7\r\n\
                To: Bob <sip:bob@biloxi.example.com>\r\n\
                From: Bob <sip:bob@biloxi.example.com>;tag=456248\r\n\
                Call-ID: 843817637684230@998sdasdh09\r\n\
                CSeq: 1826 REGISTER\r\n\
                Contact: <sip:bob@192.0.2.4>\r\n\
                Expires: 7200\r\n\
                Content-Length: 0\r\n\
                \r\n";
    let m = parse(raw);
    let (meth, _, _) = req(&m);
    assert_eq!(meth, "REGISTER");
    assert_eq!(m.header("Expires"), Some("7200"));
}

#[test]
fn bye() {
    let raw = b"BYE sip:alice@pc33.atlanta.example.com SIP/2.0\r\n\
                Via: SIP/2.0/UDP 192.0.2.4;branch=z9hG4bKnashds10\r\n\
                Max-Forwards: 70\r\n\
                From: Bob <sip:bob@biloxi.example.com>;tag=a6c85cf\r\n\
                To: Alice <sip:alice@atlanta.example.com>;tag=1928301774\r\n\
                Call-ID: a84b4c76e66710\r\n\
                CSeq: 231 BYE\r\n\
                Content-Length: 0\r\n\
                \r\n";
    let m = parse(raw);
    assert_eq!(req(&m).0, "BYE");
}

#[test]
fn cancel() {
    let raw = b"CANCEL sip:bob@biloxi.example.com SIP/2.0\r\n\
                Via: SIP/2.0/UDP host;branch=z9hG4bK1\r\n\
                CSeq: 1 CANCEL\r\n\
                \r\n";
    let m = parse(raw);
    assert_eq!(req(&m).0, "CANCEL");
}

#[test]
fn options() {
    let raw = b"OPTIONS sip:carol@chicago.example.com SIP/2.0\r\n\
                Via: SIP/2.0/UDP pc33.atlanta.example.com;branch=z9hG4bKhjhs8ass877\r\n\
                Max-Forwards: 70\r\n\
                To: <sip:carol@chicago.example.com>\r\n\
                From: Alice <sip:alice@atlanta.example.com>;tag=1928301774\r\n\
                Call-ID: a84b4c76e66710\r\n\
                CSeq: 63104 OPTIONS\r\n\
                Contact: <sip:alice@pc33.atlanta.example.com>\r\n\
                Accept: application/sdp\r\n\
                Content-Length: 0\r\n\
                \r\n";
    let m = parse(raw);
    assert_eq!(req(&m).0, "OPTIONS");
    assert_eq!(m.header("Accept"), Some("application/sdp"));
}

#[test]
fn subscribe() {
    let raw = b"SUBSCRIBE sip:alice@example.com SIP/2.0\r\n\
                Event: presence\r\n\
                Expires: 3600\r\n\
                \r\n";
    assert_eq!(req(&parse(raw)).0, "SUBSCRIBE");
}

#[test]
fn notify() {
    let raw = b"NOTIFY sip:alice@example.com SIP/2.0\r\n\
                Event: presence\r\n\
                Subscription-State: active\r\n\
                \r\n";
    let m = parse(raw);
    assert_eq!(req(&m).0, "NOTIFY");
    assert_eq!(m.header("Subscription-State"), Some("active"));
}

#[test]
fn refer() {
    let raw = b"REFER sip:alice@example.com SIP/2.0\r\n\
                Refer-To: <sip:carol@chicago.example.com>\r\n\
                \r\n";
    let m = parse(raw);
    assert_eq!(req(&m).0, "REFER");
    assert!(m.header("Refer-To").unwrap().contains("carol"));
}

#[test]
fn message_method() {
    let raw = b"MESSAGE sip:bob@biloxi.example.com SIP/2.0\r\n\
                Content-Type: text/plain\r\n\
                Content-Length: 5\r\n\
                \r\n\
                hello";
    let m = parse(raw);
    assert_eq!(req(&m).0, "MESSAGE");
    assert_eq!(m.body, b"hello");
}

#[test]
fn info_prack_update() {
    for meth in ["INFO", "PRACK", "UPDATE"] {
        let raw = format!(
            "{} sip:x@host SIP/2.0\r\nCall-ID: 1\r\nCSeq: 1 {}\r\n\r\n",
            meth, meth
        );
        let m = parse(raw.as_bytes());
        assert_eq!(req(&m).0, meth);
    }
}

#[test]
fn response_100_trying() {
    let m = parse(b"SIP/2.0 100 Trying\r\nCall-ID: x\r\n\r\n");
    let (v, c, r) = resp(&m);
    assert_eq!(v, "SIP/2.0");
    assert_eq!(c, 100);
    assert_eq!(r, "Trying");
}

#[test]
fn response_200_ok() {
    let m = parse(b"SIP/2.0 200 OK\r\nCall-ID: x\r\n\r\n");
    assert_eq!(resp(&m).1, 200);
}

#[test]
fn response_302_moved() {
    let m = parse(b"SIP/2.0 302 Moved Temporarily\r\nCall-ID: x\r\n\r\n");
    let (_, c, r) = resp(&m);
    assert_eq!(c, 302);
    assert_eq!(r, "Moved Temporarily");
}

#[test]
fn response_404_not_found() {
    let m = parse(b"SIP/2.0 404 Not Found\r\nCall-ID: x\r\n\r\n");
    let (_, c, r) = resp(&m);
    assert_eq!(c, 404);
    assert_eq!(r, "Not Found");
}

#[test]
fn response_503_service_unavailable() {
    let m = parse(b"SIP/2.0 503 Service Unavailable\r\nCall-ID: x\r\n\r\n");
    let (_, c, r) = resp(&m);
    assert_eq!(c, 503);
    assert_eq!(r, "Service Unavailable");
}

#[test]
fn response_600_busy_everywhere() {
    let m = parse(b"SIP/2.0 600 Busy Everywhere\r\nCall-ID: x\r\n\r\n");
    let (_, c, r) = resp(&m);
    assert_eq!(c, 600);
    assert_eq!(r, "Busy Everywhere");
}

#[test]
fn response_empty_reason() {
    let m = parse(b"SIP/2.0 200\r\nCall-ID: x\r\n\r\n");
    let (_, c, r) = resp(&m);
    assert_eq!(c, 200);
    assert_eq!(r, "");
}

#[test]
fn compact_form_headers_canonicalised() {
    let raw = b"INVITE sip:bob@biloxi.example.com SIP/2.0\r\n\
                v: SIP/2.0/UDP host;branch=z9hG4bK1\r\n\
                f: Alice <sip:alice@atlanta.example.com>;tag=abc\r\n\
                t: Bob <sip:bob@biloxi.example.com>\r\n\
                i: call-xyz\r\n\
                m: <sip:alice@pc33>\r\n\
                c: application/sdp\r\n\
                l: 3\r\n\
                \r\n\
                abc";
    let m = parse(raw);
    assert_eq!(m.header("Via").unwrap(), "SIP/2.0/UDP host;branch=z9hG4bK1");
    assert!(m.header("From").unwrap().contains("alice"));
    assert!(m.header("To").unwrap().contains("bob"));
    assert_eq!(m.header("Call-ID"), Some("call-xyz"));
    assert_eq!(m.header("Contact"), Some("<sip:alice@pc33>"));
    assert_eq!(m.header("Content-Type"), Some("application/sdp"));
    assert_eq!(m.header("Content-Length"), Some("3"));
    assert_eq!(m.body, b"abc");
}

#[test]
fn folded_header() {
    let raw = b"OPTIONS sip:x@host SIP/2.0\r\n\
                Subject: Longer subject\r\n  continued line\r\n\there too\r\n\
                Call-ID: x\r\n\
                \r\n";
    let m = parse(raw);
    assert_eq!(
        m.header("Subject"),
        Some("Longer subject continued line here too")
    );
}

#[test]
fn multiple_via_preserve_order() {
    let raw = b"INVITE sip:bob@host SIP/2.0\r\n\
                Via: SIP/2.0/UDP a.example.com;branch=z9hG4bK1\r\n\
                Via: SIP/2.0/UDP b.example.com;branch=z9hG4bK2\r\n\
                Via: SIP/2.0/UDP c.example.com;branch=z9hG4bK3\r\n\
                \r\n";
    let m = parse(raw);
    let vias = header_values(&m, "Via");
    assert_eq!(vias.len(), 3);
    assert!(vias[0].contains("a.example.com"));
    assert!(vias[1].contains("b.example.com"));
    assert!(vias[2].contains("c.example.com"));
}

#[test]
fn body_present_with_content_length() {
    let raw = b"INVITE sip:bob@host SIP/2.0\r\n\
                Content-Type: application/sdp\r\n\
                Content-Length: 11\r\n\
                \r\n\
                hello world";
    let m = parse(raw);
    assert_eq!(m.body, b"hello world");
    assert_eq!(m.content_length(), Some(11));
}

#[test]
fn body_absent_no_content_length() {
    let raw = b"BYE sip:x@host SIP/2.0\r\n\
                Call-ID: 1\r\n\
                \r\n";
    let m = parse(raw);
    assert!(m.body.is_empty());
    assert_eq!(m.content_length(), None);
}

#[test]
fn content_length_truncates_oversized_body() {
    let raw = b"MESSAGE sip:x@host SIP/2.0\r\n\
                Content-Length: 5\r\n\
                \r\n\
                helloTRAILING";
    let m = parse(raw);
    assert_eq!(m.body, b"hello");
}

#[test]
fn content_length_mismatch_keeps_short_body() {
    let raw = b"MESSAGE sip:x@host SIP/2.0\r\n\
                Content-Length: 100\r\n\
                \r\n\
                hi";
    let m = parse(raw);
    assert_eq!(m.body, b"hi");
}

#[test]
fn lf_only_line_endings_accepted() {
    let raw = b"OPTIONS sip:x@host SIP/2.0\nCall-ID: 1\n\n";
    let m = parse(raw);
    assert_eq!(req(&m).0, "OPTIONS");
    assert_eq!(m.header("Call-ID"), Some("1"));
}

#[test]
fn method_uppercased() {
    let raw = b"invite sip:bob@host SIP/2.0\r\nCall-ID: 1\r\n\r\n";
    let m = parse(raw);
    assert_eq!(req(&m).0, "INVITE");
}

#[test]
fn unknown_header_capitalize_dashed() {
    let raw = b"OPTIONS sip:x@host SIP/2.0\r\n\
                x-custom-thing: yes\r\n\
                \r\n";
    let m = parse(raw);
    assert_eq!(m.header("X-Custom-Thing"), Some("yes"));
}

// ---- malformed corpus ----

#[test]
fn err_empty_input() {
    assert_eq!(Message::parse(b""), Err(ParseError::EmptyMessage));
}

#[test]
fn err_too_large() {
    let big = vec![b'A'; Message::MAX_SIZE + 1];
    assert_eq!(Message::parse(&big), Err(ParseError::MessageTooLarge));
}

#[test]
fn err_start_line_no_newline() {
    assert_eq!(
        Message::parse(b"INVITE sip:x@host SIP/2.0"),
        Err(ParseError::InvalidStartLine)
    );
}

#[test]
fn err_start_line_one_token() {
    assert_eq!(
        Message::parse(b"INVITE\r\n\r\n"),
        Err(ParseError::InvalidStartLine)
    );
}

#[test]
fn err_start_line_two_tokens_request() {
    assert_eq!(
        Message::parse(b"INVITE sip:x@host\r\n\r\n"),
        Err(ParseError::InvalidStartLine)
    );
}

#[test]
fn err_request_bad_sip_version() {
    assert_eq!(
        Message::parse(b"INVITE sip:x@host HTTP/1.1\r\n\r\n"),
        Err(ParseError::InvalidStartLine)
    );
}

#[test]
fn err_response_non_numeric_status() {
    assert_eq!(
        Message::parse(b"SIP/2.0 OK Fine\r\n\r\n"),
        Err(ParseError::InvalidStatusCode)
    );
}

#[test]
fn err_response_short_status() {
    assert_eq!(
        Message::parse(b"SIP/2.0 20 OK\r\n\r\n"),
        Err(ParseError::InvalidStatusCode)
    );
}

#[test]
fn err_response_long_status() {
    assert_eq!(
        Message::parse(b"SIP/2.0 2000 OK\r\n\r\n"),
        Err(ParseError::InvalidStatusCode)
    );
}

#[test]
fn err_header_missing_colon() {
    let raw = b"OPTIONS sip:x@host SIP/2.0\r\n\
                BrokenHeaderNoColon\r\n\
                \r\n";
    assert_eq!(Message::parse(raw), Err(ParseError::HeaderMissingColon));
}

#[test]
fn err_header_blank_name() {
    let raw = b"OPTIONS sip:x@host SIP/2.0\r\n\
                : value\r\n\
                \r\n";
    assert_eq!(Message::parse(raw), Err(ParseError::HeaderMissingColon));
}

#[test]
fn err_negative_content_length() {
    let raw = b"MESSAGE sip:x@host SIP/2.0\r\n\
                Content-Length: -1\r\n\
                \r\n";
    assert_eq!(Message::parse(raw), Err(ParseError::NegativeContentLength));
}

#[test]
fn err_bad_content_length() {
    let raw = b"MESSAGE sip:x@host SIP/2.0\r\n\
                Content-Length: abc\r\n\
                \r\n";
    assert_eq!(Message::parse(raw), Err(ParseError::BadContentLength));
}

#[test]
fn err_oversize_content_length() {
    let raw = b"MESSAGE sip:x@host SIP/2.0\r\n\
                Content-Length: 999999999\r\n\
                \r\n";
    assert_eq!(Message::parse(raw), Err(ParseError::OversizeContentLength));
}

#[test]
fn err_folded_without_prior_header() {
    let raw = b"OPTIONS sip:x@host SIP/2.0\r\n\
                 orphan continuation\r\n\
                \r\n";
    assert!(Message::parse(raw).is_err());
}

#[test]
fn err_fully_empty_after_first_lf() {
    assert_eq!(Message::parse(b"\r\n"), Err(ParseError::InvalidStartLine));
}

#[test]
fn err_invalid_utf8_in_header_name() {
    let raw: &[u8] = &[
        b'O', b'P', b'T', b'I', b'O', b'N', b'S', b' ', b's', b'i', b'p', b':', b'x', b'@', b'h',
        b' ', b'S', b'I', b'P', b'/', b'2', b'.', b'0', b'\r', b'\n', 0xFF, 0xFE, b':', b' ', b'v',
        b'\r', b'\n', b'\r', b'\n',
    ];
    assert_eq!(Message::parse(raw), Err(ParseError::InvalidUtf8));
}

#[test]
fn err_invalid_utf8_in_method() {
    let raw: &[u8] = &[
        0xFF, 0xFE, b' ', b's', b'i', b'p', b':', b'x', b'@', b'h', b' ', b'S', b'I', b'P', b'/',
        b'2', b'.', b'0', b'\r', b'\n', b'\r', b'\n',
    ];
    assert_eq!(Message::parse(raw), Err(ParseError::InvalidUtf8));
}

#[test]
fn err_start_line_only_spaces() {
    assert_eq!(Message::parse(b"   \r\n\r\n"), Err(ParseError::InvalidStartLine));
}

#[test]
fn err_status_code_with_letters() {
    assert_eq!(
        Message::parse(b"SIP/2.0 2O0 OK\r\n\r\n"),
        Err(ParseError::InvalidStatusCode)
    );
}

#[test]
fn err_sip_version_lowercase_request() {
    assert_eq!(
        Message::parse(b"INVITE sip:x@h sip/2.0\r\n\r\n"),
        Err(ParseError::InvalidStartLine)
    );
}
