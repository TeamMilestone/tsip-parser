# tsip-parser

Pure-Rust SIP URI (RFC 3261 §19.1) and SIP Address (§25.1) parser/serializer.

Ported from [`tsip-core`](https://github.com/) Ruby reference implementation —
byte-for-byte round-trip compatible with its `Uri#to_s` / `Address#to_s` output.
No external dependencies, no `unsafe` blocks.

## Features

- **Schemes**: `sip`, `sips`, `tel` (case-insensitive)
- **Userinfo**: `user`, `user:password` with `%XX` pct-decoding
- **Hosts**: FQDN, IPv4, bracketed IPv6 (`[::1]:5060`)
- **URI parameters** (`;k=v`) and **URI headers** (`?k=v&k=v`) — insertion order preserved
- **Address**: name-addr (`"Alice" <sip:…>`) and addr-spec (`sip:…`) forms; classifies `tag`/`q`/`expires` at Address level, remaining params onto the embedded URI
- **Range-based API**: `parse_range(input, from, to)` avoids substring allocation when callers already hold the full input (used internally so `Address::parse` delegates to `Uri::parse_range` without copying)

## Usage

```toml
[dependencies]
tsip-parser = "0.1"
```

```rust
use tsip_parser::{Address, Uri};

let uri = Uri::parse("sip:alice@atlanta.example.com;transport=tcp").unwrap();
assert_eq!(uri.scheme, "sip");
assert_eq!(uri.user.as_deref(), Some("alice"));
assert_eq!(uri.host, "atlanta.example.com");
assert_eq!(uri.params[0], ("transport".to_string(), "tcp".to_string()));

let addr = Address::parse(r#""Alice" <sip:alice@atlanta.example.com>;tag=9fxced76sl"#).unwrap();
assert_eq!(addr.display_name.as_deref(), Some("Alice"));
assert_eq!(addr.tag(), Some("9fxced76sl"));

// Round-trip serialization
assert_eq!(
    uri.to_string(),
    "sip:alice@atlanta.example.com;transport=tcp"
);
```

## Performance

Measured on Apple M1 (release, `lto = "thin"`), for typical INVITE-path inputs:

| Operation                    | Time   |
|------------------------------|--------|
| `Uri::parse` (1 param)       | 142 ns |
| `Uri::parse` (3 params)      | 235 ns |
| `Uri::parse` (complex + IPv6 + header) | 339 ns |
| `Uri::to_string`             | 72 ns  |
| `Address::parse` (name-addr) | 204 ns |
| `Address::parse` (bare)      | 200 ns |
| `Address::to_string`         | 77 ns  |

Roughly **25–35× faster** than the Ruby reference implementation (which runs
at 5–7 μs/parse). The crate uses a single-pass byte scanner with
`std::str`-level slicing; no regex, no intermediate `Vec<u8>` allocation.

## Scope

This crate parses a single SIP URI or Address value. It does **not** parse
full SIP messages (start-line, header list, body), and it is not a generic
RFC 3986 URI parser — SIP URIs have a distinct grammar (no `//` authority,
no path, top-level parameters).

Intended as the native backend for future Ruby / Python / Node FFI bindings
that need SIP URI parsing at line rate.

## Design

- **Zero external dependencies.** Keeps the supply chain small and builds
  fast. Adding `smallvec` / `indexmap` was considered and rejected — linear
  search over a short `Vec<(String, String)>` outperforms hashing at the sizes
  SIP URIs actually use.
- **No `unsafe`.** UTF-8 invariants are upheld by construction (the scanner
  only splits on ASCII delimiters, so byte offsets are always char-aligned)
  rather than by `from_utf8_unchecked`.
- **Byte-for-byte parity with Ruby.** Param/header insertion order is
  preserved so `parse → to_string → parse` is idempotent against the
  reference implementation.

## License

MIT. © 2026 Wonsup Lee (이원섭) <alfonso@team-milestone.io>.
