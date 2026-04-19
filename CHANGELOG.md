# Changelog

## 0.3.0 — 2026-04-19

### Added

- `message` module with `Message::parse(&[u8]) -> Result<Message, ParseError>`
  for RFC 3261 SIP message framing. Returns `StartLine` (`Request` or
  `Response`), `Vec<(canonical_name, raw_value)>` headers preserving insertion
  order and duplicates, and `Vec<u8>` body. Compact header forms
  (`v`, `f`, `t`, `i`, `m`, `c`, `l`, ...) are mapped to canonical names.
  Line folding per §7.3.1 joins continuation lines with a single SP.
  `Content-Length` is validated (negative / non-numeric / oversize rejected)
  and used to truncate an oversized raw body; a short body is kept verbatim
  (transport-layer concern).
- `Message::content_length()` and `Message::header(canonical)` convenience
  accessors.
- `ParseError` variants: `MessageTooLarge`, `EmptyMessage`, `InvalidStartLine`,
  `InvalidStatusCode`, `HeaderMissingColon`, `NegativeContentLength`,
  `OversizeContentLength`, `BadContentLength`.
- Re-exports at the crate root: `Message`, `StartLine`.

### Internal

- `Message::MAX_SIZE = 65_536`. Inputs above the cap are rejected with
  `MessageTooLarge`. `Content-Length` above the cap yields
  `OversizeContentLength`.
- Canonical header lookup is stateless: 1-byte compact map, then an ASCII
  case-insensitive scan of 37 well-known names, then `capitalize_dashed`
  allocation as the fallback.
- Tests: 28 good corpus + 20 malformed corpus (`tests/message_parity.rs`).
- Fuzz target `message`: 4.78M runs / 30s / panic=0 (local smoke).
- Bench (`benches/message_bench.rs`): INVITE 10-header ≈ 1.48 μs;
  response 200 ≈ 1.03 μs; compact-form INVITE ≈ 1.06 μs.

## 0.2.1 — 2026-04-19

### Fixed

- `sip:alice@host;<evil>=1` and other URI-level param key/values containing
  `>`, `;`, `?`, `&`, `=`, or `<` are now accepted. The parse-time
  `InvalidHost` rejection for `>` was removed; `Uri::append_to` escapes these
  bytes with lowercase pct-encoding on render so the stored byte cannot
  re-tokenize the URI body or terminate an Address `<...>` wrapper on
  re-parse. Closes the last xoracle parity case (#13) vs the Ruby tsip-core
  reference.

### Added

- `Uri::parse_range(src, from, to)` was already public; two more class-method
  entry points join it for FFI bindings (tsip-core's `TsipCore::Sip::Uri =
  TsipParser::Uri` class alias):
  - `Uri::parse_param(raw, &mut Vec<(String, String)>)` — parse one
    `key[=value]` segment as produced by splitting a URI body on `;`.
  - `Uri::parse_host_port(&str)` — parse a `host[:port]` fragment
    (including the bracketed-IPv6 form `[::1]:5060`). Returns
    `(String, Option<u16>)`.

### Internal

- Render-side escape set for URI-level params: `; ? & = < >`. `%` and
  whitespace are *not* escaped — params are stored literally (no pct-decode
  on parse), so escaping `%` would break the fixed point and `parse_param_range`
  already preserves leading value whitespace verbatim.
- Lowercase hex (`%3c`, `%3e`, ...) is emitted for param escapes so the
  re-parse (which `downcase_str`s keys) reaches a fixed point in one cycle
  rather than two. Header/userinfo escape continues to use uppercase hex
  because those fields pct-decode on parse.
- Fuzz: `uri` 9.36M runs / 121s / crashes=0; `address` 8.95M runs / 121s /
  crashes=0.
- Bench (vs v0.2.0): `uri_to_string_typical` +13.9% (expected — per-char
  scan added to param render). `uri_parse_*` within ±7%. All within the
  ±20% tolerance set by the handoff.

## 0.2.0 — 2026-04-19

### Breaking

- Relaxed parse-time validation to converge with the Ruby `tsip-core` parser
  (Option B per `docs/V0_2_0_HANDOFF.md`). pct-encoded special characters in
  userinfo (`%40`, `%3C`, `%25`, ...) and literal `<` in param keys are now
  accepted. Leading/trailing whitespace in URI-level param and header values
  is preserved instead of trimmed.
- Added render-side pct-escape for pct-decoded fields (userinfo, URI header
  key and value). Bytes that would re-tokenize on re-parse (`@ : ; ? < > % & =`
  and whitespace) are emitted as `%XX`. This diverges from Ruby's byte-identical
  `to_s` output but guarantees round-trip stability. Non-ASCII UTF-8 bytes are
  preserved verbatim.
- `Uri::parse` still raises `InvalidHost` for `<...>`-wrapped input — use
  `Address::parse` for name-addr form. Narrow rejections remain for bytes that
  literal (non-pct-decoded) storage cannot round-trip through an Address
  wrapper: `>` in any URI-level or Address-level param key/value, and `?` in
  URI-level param key/value (both would retokenize when the URI is re-parsed
  inside `<...>`).

### Deviations from the v0.2.0 handoff

- Handoff §3.1 listed `<evil>`-style keys as accepted under the permissive
  profile, but fuzz found that a literal `>` in any param position terminates
  the Address wrapper on re-parse. Keeping round-trip stability required
  rejecting `>` (and `?` for URI-embedded params in the bare Address path) at
  parse time. The xoracle case `sip:alice@host;<evil>=1` is now rejected;
  `sip:alice@host;<foo=1` (no trailing `>`) is still accepted.
- Handoff §3.4 stated "all 14 xoracle inputs must be RUST_OK via `Uri::parse`".
  In practice `<sip:...>` inputs are only valid through `Address::parse` (per
  §7), so `examples/xoracle.rs` now dispatches on the leading `<`.

### Internal

- Fuzz: `uri` target 12.0M runs / 301s / crashes=0;
  `address` target 14.9M runs / 301s / crashes=0.
- Bench: ≤ +9% regression across all measurements (within the ±10% tolerance
  set by the handoff).
