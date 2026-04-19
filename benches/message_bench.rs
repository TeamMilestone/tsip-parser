use criterion::{black_box, criterion_group, criterion_main, Criterion};
use tsip_parser::Message;

const INVITE_10H: &[u8] = b"INVITE sip:bob@biloxi.example.com SIP/2.0\r\n\
Via: SIP/2.0/UDP pc33.atlanta.example.com;branch=z9hG4bK776asdhds\r\n\
Max-Forwards: 70\r\n\
To: Bob <sip:bob@biloxi.example.com>\r\n\
From: Alice <sip:alice@atlanta.example.com>;tag=1928301774\r\n\
Call-ID: a84b4c76e66710@pc33.atlanta.example.com\r\n\
CSeq: 314159 INVITE\r\n\
Contact: <sip:alice@pc33.atlanta.example.com>\r\n\
User-Agent: ExampleUA/1.0\r\n\
Content-Type: application/sdp\r\n\
Content-Length: 0\r\n\
\r\n";

const RESPONSE_200: &[u8] = b"SIP/2.0 200 OK\r\n\
Via: SIP/2.0/UDP pc33.atlanta.example.com;branch=z9hG4bK776asdhds\r\n\
To: Bob <sip:bob@biloxi.example.com>;tag=a6c85cf\r\n\
From: Alice <sip:alice@atlanta.example.com>;tag=1928301774\r\n\
Call-ID: a84b4c76e66710@pc33.atlanta.example.com\r\n\
CSeq: 314159 INVITE\r\n\
Contact: <sip:bob@192.0.2.4>\r\n\
Content-Length: 0\r\n\
\r\n";

const COMPACT_INVITE: &[u8] = b"INVITE sip:bob@biloxi.example.com SIP/2.0\r\n\
v: SIP/2.0/UDP pc33.atlanta.example.com;branch=z9hG4bK776asdhds\r\n\
f: Alice <sip:alice@atlanta.example.com>;tag=1928301774\r\n\
t: Bob <sip:bob@biloxi.example.com>\r\n\
i: a84b4c76e66710@pc33.atlanta.example.com\r\n\
m: <sip:alice@pc33.atlanta.example.com>\r\n\
c: application/sdp\r\n\
l: 0\r\n\
CSeq: 1 INVITE\r\n\
\r\n";

fn bench_message(c: &mut Criterion) {
    c.bench_function("message_parse_invite_10h", |b| {
        b.iter(|| Message::parse(black_box(INVITE_10H)).unwrap())
    });
    c.bench_function("message_parse_response_200", |b| {
        b.iter(|| Message::parse(black_box(RESPONSE_200)).unwrap())
    });
    c.bench_function("message_parse_compact_invite", |b| {
        b.iter(|| Message::parse(black_box(COMPACT_INVITE)).unwrap())
    });
}

criterion_group!(benches, bench_message);
criterion_main!(benches);
