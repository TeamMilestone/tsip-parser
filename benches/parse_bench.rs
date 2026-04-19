use criterion::{black_box, criterion_group, criterion_main, Criterion};
use tsip_parser::{Address, Uri};

const TYPICAL_URI: &str = "sip:alice@atlanta.example.com;transport=tcp";
const COMPLEX_URI: &str = "sip:alice:pw@[2001:db8::1]:5060;transport=tcp;lr?subject=hi&priority=u";
const TYPICAL_ADDR: &str = "\"Alice\" <sip:alice@atlanta.example.com>;tag=9fxced76sl";
const BARE_ADDR: &str = "sip:alice@atlanta.example.com;tag=9fxced76sl";

fn bench_uri(c: &mut Criterion) {
    c.bench_function("uri_parse_typical", |b| {
        b.iter(|| Uri::parse(black_box(TYPICAL_URI)).unwrap())
    });
    c.bench_function("uri_parse_complex", |b| {
        b.iter(|| Uri::parse(black_box(COMPLEX_URI)).unwrap())
    });

    // Breakdown — isolate sources of allocation.
    c.bench_function("uri_parse_host_only", |b| {
        b.iter(|| Uri::parse(black_box("sip:atlanta.example.com")).unwrap())
    });
    c.bench_function("uri_parse_user_host", |b| {
        b.iter(|| Uri::parse(black_box("sip:alice@atlanta.example.com")).unwrap())
    });
    c.bench_function("uri_parse_one_param", |b| {
        b.iter(|| Uri::parse(black_box("sip:alice@host;transport=tcp")).unwrap())
    });
    c.bench_function("uri_parse_three_params", |b| {
        b.iter(|| Uri::parse(black_box("sip:alice@host;transport=tcp;lr;method=INVITE")).unwrap())
    });
    c.bench_function("uri_parse_one_header", |b| {
        b.iter(|| Uri::parse(black_box("sip:alice@host?subject=hi")).unwrap())
    });
    c.bench_function("uri_parse_pct_user", |b| {
        b.iter(|| Uri::parse(black_box("sip:%61lice%20x@host")).unwrap())
    });

    let uri = Uri::parse(TYPICAL_URI).unwrap();
    c.bench_function("uri_to_string_typical", |b| {
        b.iter(|| black_box(&uri).to_string())
    });
}

fn bench_address(c: &mut Criterion) {
    c.bench_function("address_parse_name_addr", |b| {
        b.iter(|| Address::parse(black_box(TYPICAL_ADDR)).unwrap())
    });
    c.bench_function("address_parse_bare", |b| {
        b.iter(|| Address::parse(black_box(BARE_ADDR)).unwrap())
    });

    let addr = Address::parse(TYPICAL_ADDR).unwrap();
    c.bench_function("address_to_string", |b| {
        b.iter(|| black_box(&addr).to_string())
    });
}

criterion_group!(benches, bench_uri, bench_address);
criterion_main!(benches);
