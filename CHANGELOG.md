# Changelog

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
