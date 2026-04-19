# sip_uri_crate 구현 핸드오버

작성일: 2026-04-19. 목적: tsip-core 의 `lib/tsip_core/sip/uri.rb` +
`lib/tsip_core/sip/address.rb` 파싱/직렬화 로직을 pure-Rust 크레이트로 포팅.
Ruby 바인딩은 향후 별도 단계에서 tsip-core 의 `ext/tsip_core/` 또는 이 크레이트
위에 magnus 래퍼 crate 를 얹어 제공. 이 문서는 **크레이트 자체** 의 설계·구현
지침에 집중.

---

## 0. 배경

tsip-core 성능 병목 분석 결과 (2026-04-19, `tsip-core/docs/PERFORMANCE_HANDOVER.md`
8차 세션 참조), stackprof 의 상위 CPU 점유:

```
 6.8% self  Uri.parse_range        total 12.8%
 6.4% self  Address.parse          total 22.4%
 2.0% self  Uri.parse_host_port_range
 1.7% self  Uri.parse_param_range
 1.3% self  Uri.detect_scheme
 1.1% self  Uri.pct_decode
---
~19% self 합계
```

Ruby pure-byte-scan 구현으로 5차 세션에서 이미 +11.8% cps 개선했으나 더 짜낼
여지는 pure-Ruby 에서 소진. Rust 네이티브 파서로 **추가 +10-12% cps** 기대.
본 크레이트는 그 네이티브 백엔드의 "pure Rust" 레이어 (FFI 없음).

---

## 1. 범위

### 포함

1. **SIP URI 파싱** — RFC 3261 §19.1, RFC 3966 (tel:)
   - schemes: `sip`, `sips`, `tel` (대소문자 무관)
   - `userinfo@host:port;params?headers` 분해
   - IPv6 bracketed host: `[::1]:5060`
   - pct-encoding 디코딩 (user / password / header key+value)
   - URI param (`;key=value`) / URI header (`?key=value&key=value`)

2. **SIP Address 파싱** — RFC 3261 §25.1
   - name-addr: `"Display" <uri>;param=value`
   - addr-spec: `sip:alice@host;tag=x` (bare URI + trailing header params)
   - display-name quoting / dequoting
   - Address-level params (`tag`, `q`, `expires`) vs URI-embedded params 분리

3. **직렬화 (to_string 등)** — Ruby `Uri#to_s`, `Address#to_s` 와 **바이트 단위
   round-trip 동등성** 필수

4. **범위 기반 API (`parse_range`)** — 입력은 `&[u8]` + `from..to` range.
   Address 파서가 Uri 를 내부 호출할 때 substring alloc 없이 위임 가능.

### 제외 (명시적 non-goal)

- SIP 메시지 전체 파싱 (start-line / headers / body) — tsip-core Parser 가 수행
- Via / CSeq / Contact 등 다른 SIP 헤더 타입 — 별도 관심사
- TLS / transport / transaction state — 상위 레이어
- 일반 URI (RFC 3986) / HTTP URL 파싱 — SIP URI 는 구조가 다름
  (`//` 없음, path 없음, params 가 top-level)

---

## 2. 레퍼런스 구현 — Ruby 파일

현재 Ruby 구현은 `byte-scan` 방식이라 Rust 포팅 시 로직을 거의 그대로 옮길 수
있음. 포팅 시 아래 파일을 단일 진실 원천(source of truth)으로 삼고 **동일
입력 → 동일 출력** 을 보장.

| 기능 | Ruby 파일 | 라인 | 비고 |
|------|-----------|------|------|
| `Uri.parse` | `lib/tsip_core/sip/uri.rb` | 42-62 | trim 후 `parse_range` 위임 |
| `Uri.parse_range` | 동 | 66-159 | 메인 single-pass 스캔 |
| `Uri.parse_param_range` | 동 | 166-190 | `k=v` 또는 `k` 단독 |
| `Uri.parse_host_port_range` | 동 | 198-239 | IPv6 bracket + port |
| `Uri.detect_scheme` | 동 | 247-271 | sip/sips/tel 대소문자 무관 |
| `Uri.pct_decode` | 동 | 321-347 | `%XX` 헥사 디코딩 |
| `Uri.downcase_range` | 동 | 300-319 | ASCII 소문자화 |
| `Uri.parse_header_range` | 동 | 357-373 | `?k=v&k=v` URI 헤더 |
| `Uri#to_s` / `append_to` | 동 | 408-440 | 직렬화 |
| `Address.parse` | `lib/tsip_core/sip/address.rb` | 30-92 | name-addr 분기 |
| `Address.parse_bare_range` | 동 | 94-126 | addr-spec 분기 |
| `Address.classify_bare_param` | 동 | 128-154 | bare 모드 param 분류 |
| `Address.extract_display` | 동 | 156-170 | `"..."` 디쿼팅 |
| `Address#to_s` / `append_to` | 동 | 180-199 | 직렬화 |

**테스트 레퍼런스**: `tsip-core/test/sip/test_address.rb` (5 테스트) — Ruby 구현과
동일한 입력 set 으로 Rust 구현이 통과해야 함.

---

## 3. 크레이트 레이아웃 제안

```
sip_uri_crate/
├── Cargo.toml
├── README.md
├── docs/
│   └── HANDOVER.md          ← 이 문서
├── src/
│   ├── lib.rs               ← 공개 API re-export
│   ├── uri.rs               ← Uri 구조체 + parse + to_string
│   ├── address.rs           ← Address 구조체 + parse + to_string
│   ├── scan.rs              ← 공용 byte-scan 헬퍼 (pct_decode, downcase_ascii 등)
│   └── error.rs             ← ParseError
├── tests/
│   ├── uri_parity.rs        ← Ruby 테스트 케이스 포팅
│   ├── address_parity.rs
│   └── roundtrip.rs         ← parse → to_string → parse 불변성
├── benches/
│   └── parse_bench.rs       ← criterion 기반 마이크로 벤치
└── fuzz/
    └── fuzz_targets/
        ├── uri.rs           ← cargo-fuzz (libfuzzer)
        └── address.rs
```

### Cargo.toml 초기값

```toml
[package]
name = "sip_uri"
version = "0.1.0"
edition = "2021"
rust-version = "1.75"
description = "RFC 3261 SIP URI (§19.1) and Address (§25.1) parser/serializer"
license = "MIT"
repository = "<향후 GitHub URL>"
keywords = ["sip", "parser", "telephony"]
categories = ["parser-implementations", "network-programming"]

[dependencies]
# 의도적으로 외부 의존성 없음 — pure Rust stdlib 만 사용.
# 이유: tsip-core Ruby 바인딩 시점에 최소 바이너리 사이즈 유지,
# 보안 공급망 축소, 빌드 시간 단축.

[dev-dependencies]
criterion = { version = "0.5", features = ["html_reports"] }

[[bench]]
name = "parse_bench"
harness = false

[profile.release]
lto = "thin"
codegen-units = 1
opt-level = 3
```

**의존성 정책**: 0 external deps. 파서 로직은 순수 byte 조작으로 충분. 향후
정규식·인코딩 라이브러리 유혹 있어도 거부 — 성능·공급망·바이너리 사이즈 전부
손해.

---

## 4. 공개 API 설계

Ruby 측과 parity 를 유지하려면 아래 surface 필요. **lifetime 과 allocation
전략이 중요** — Ruby 는 GC 가 있어 freely String alloc, Rust 는 명시적.

### 4.1 Uri

```rust
// src/uri.rs

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Uri {
    pub scheme: &'static str,       // "sip" | "sips" | "tel" — static literal
    pub user: Option<String>,        // pct-decoded
    pub password: Option<String>,    // pct-decoded
    pub host: String,                // as-is (IPv6: inner 부분만, brackets 제거)
    pub port: Option<u16>,
    pub params: Vec<(String, String)>,  // insertion-order 보존 (to_string 시 필수)
    pub headers: Vec<(String, String)>, // 동일
}

impl Uri {
    pub fn parse(input: &str) -> Result<Self, ParseError> { ... }
    pub fn parse_range(src: &[u8], from: usize, to: usize) -> Result<Self, ParseError> { ... }
    pub fn append_to(&self, buf: &mut String) { ... }
    // Display trait 자동 구현: impl Display for Uri
}

impl fmt::Display for Uri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut buf = String::new();
        self.append_to(&mut buf);
        f.write_str(&buf)
    }
}
```

**설계 노트**:

- `scheme: &'static str` — parser 가 `"sip"` / `"sips"` / `"tel"` literal 반환.
  Ruby 는 `String` 이지만 Rust 에서는 static slice 로 alloc 완전 제거.
- `params`/`headers` 는 `Vec<(String, String)>` — `HashMap` 이 아님.
  이유: (1) RFC 3261 은 param 순서 보존을 요구하지 않지만, `to_string`
  round-trip 에서 Ruby 와 동일 순서로 나와야 기존 snapshot/fuzz 테스트 통과.
  (2) 대부분 SIP URI 는 params ≤ 3개 — linear search 가 Hash 보다 빠름.
- `host` 에서 IPv6 brackets 제거: Ruby 구현은 `[::1]` 을 `::1` 로 저장
  (uri.rb:213). `append_to` 가 재-bracket 씌움 (uri.rb:400-406).

### 4.2 Address

```rust
// src/address.rs

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Address {
    pub display_name: Option<String>,   // dequoted (double-quote 제거)
    pub uri: Option<Uri>,               // None 은 빈 Address (이론적)
    pub params: Vec<(String, String)>,  // tag / q / expires 등 Address-level params
}

impl Address {
    pub fn parse(input: &str) -> Result<Self, ParseError> { ... }
    pub fn tag(&self) -> Option<&str> { ... }
    pub fn set_tag(&mut self, tag: String) { ... }
    pub fn append_to(&self, buf: &mut String) { ... }
}
```

**Address-level params 판정**: 아래 키들은 Address.params 로, 나머지는
embedded URI 의 params 로 밀어넣음 (bare 모드일 때만 이 판정이 의미 있음,
name-addr 모드는 `<uri>` 가 명시 경계):

```rust
const ADDRESS_PARAMS: &[&str] = &["tag", "q", "expires"];
```

Ruby 쪽 `Address::ADDRESS_PARAMS` 와 동기. 향후 변경 시 양쪽 동시에.

### 4.3 ParseError

```rust
// src/error.rs

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    Empty,
    UnterminatedBracket,      // [.. 가 닫히지 않음
    UnterminatedQuote,         // "..." 미종결
    UnterminatedAngle,         // <..> 미종결 (Address)
    InvalidScheme,             // sip/sips/tel 이 아님
    // 필요 시 확장
}

impl std::error::Error for ParseError {}
impl fmt::Display for ParseError { ... }
```

---

## 5. 구현 주의사항

### 5.1 Ruby 구현과의 1:1 parity

- **ASCII 전용 처리**: SIP URI 의 pct-encoded 이스케이프는 바이트 수준. 파서는
  `&[u8]` 기반으로 동작하고 최종 `String` 리턴 시 UTF-8 검증은 *생략*
  (RFC 3261 은 URI 문자셋을 ASCII 로 제한, 실제 field 는 ASCII subset).
  `String::from_utf8_unchecked` 는 **쓰지 말 것** — 안전한 `from_utf8` 후
  invalid 시 ParseError 반환이 올바름. pct-decoded 결과가 non-UTF-8 이면
  Ruby 는 force_encoding 으로 허용하지만 Rust 는 `String` 이 UTF-8 invariant
  이므로 `Vec<u8>` 리턴 옵션 고려 (추후 결정).

- **param 순서 보존**: Ruby Hash 는 insertion order 보존. `Vec<(String, String)>`
  로 매칭. HashMap 도입 시 to_string 순서가 달라져 fuzz regression 가능.

- **case-insensitive 비교**: scheme 판정(`detect_scheme`), parameter key
  downcase(`downcase_range`) 는 ASCII 전용. `str::to_ascii_lowercase()` 사용.
  UTF-8 `to_lowercase` 금지 (locale-dependent, 느림).

- **IPv6 host 저장**: `[::1]:5060` 입력 시 `host = "::1"`, `port = Some(5060)`.
  `host: "[::1]"` 로 저장하면 `append_to` 가 다시 `[[` wrapping 하므로 틀림.

### 5.2 allocation 최소화 패턴

- **Cow 사용 금지 권고**: 초기 포팅은 `String` 리턴으로 단순화. Cow 는
  measurable gain 확인 후 도입. 현재 Ruby 가 `byteslice` → new String 인 자리에
  Rust 에서 갑자기 `&str` 리턴하면 lifetime 이 호출자로 전파되어 API 복잡도
  급증 — FFI 경계에서 어차피 복사 필요.

- **small-vec 고려**: params 가 0 또는 1개인 케이스가 대부분. 추후 측정 후
  `smallvec` 외부 크레이트 도입 여지 있으나 **초기 버전에서는 표준 `Vec` 유지**
  (의존성 무 원칙).

- **pct_decode fast path**: 입력에 `%` 없으면 `src[from..to]` 를 그대로 slice.
  Ruby `uri.rb:321-328` 동일 패턴. 대부분의 SIP URI 는 pct-encoding 없음.

### 5.3 버퍼링된 to_string

`append_to(&mut String)` 형태로 통일. `Display::fmt` 는 그 위에 얇게 감쌈.
이유: Address.to_string 이 내부 Uri 를 호출할 때 중간 String alloc 을 피해야
성능 유지 (Ruby 6차 세션 최적화와 동일 원리).

---

## 6. 테스트 전략

### 6.1 Parity 테스트 (`tests/uri_parity.rs`, `tests/address_parity.rs`)

Ruby 테스트의 입력·기대치를 Rust 로 직접 포팅:

```rust
// tests/address_parity.rs
#[test]
fn parse_name_addr_with_tag() {
    let addr = Address::parse(r#""Alice" <sip:alice@atlanta.example.com>;tag=9fxced76sl"#).unwrap();
    assert_eq!(addr.display_name.as_deref(), Some("Alice"));
    assert_eq!(addr.uri.as_ref().unwrap().user.as_deref(), Some("alice"));
    assert_eq!(addr.uri.as_ref().unwrap().host, "atlanta.example.com");
    assert_eq!(addr.tag(), Some("9fxced76sl"));
}
```

Ruby 테스트 5개 (tsip-core/test/sip/test_address.rb) 전부 포팅 + Uri 관련
추가 케이스. 최소 20개 테스트 목표.

**round-trip 테스트**: `parse → to_string → parse → 동일 구조체` 확인. 이것이
가장 강력한 regression 보호.

### 6.2 Ruby 구현 동작 캡처 (cross-oracle)

Rust 개발 중 의심스러운 입력은 Ruby REPL 로 동작 확인:

```bash
cd /Users/wonsup-mini/projects/tsip-core
bundle exec ruby -I lib -r tsip_core -e '
  puts TsipCore::Sip::Address.parse("<sip:alice@host>;tag=a;q=0.5").inspect
'
```

이 출력이 Rust 구현의 ground truth.

### 6.3 Fuzz (`fuzz/fuzz_targets/uri.rs`)

`cargo-fuzz` (libfuzzer) 로 패닉 없음 확인:

```rust
#![no_main]
use libfuzzer_sys::fuzz_target;
use sip_uri::Uri;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        let _ = Uri::parse(s);  // no panic
    }
});
```

tsip-core 의 fuzz smoke (10k iter, crashes=0) 기준선 유지. CI 에서 매 PR 마다
5분 fuzz run 권장.

### 6.4 벤치마크 (`benches/parse_bench.rs`)

criterion 기반. 목표:

```
uri_parse_typical     : ≤ 1.0 μs/iter  (Ruby 4-5 μs)
uri_parse_range       : ≤ 0.8 μs/iter
uri_to_string         : ≤ 0.5 μs/iter
address_parse_name_addr: ≤ 1.5 μs/iter (Ruby 7 μs)
address_to_string     : ≤ 0.7 μs/iter
```

측정 머신: macOS M1 로컬 및 Linux x86_64 원격 둘 다 기록. 5× 가속이 최소
목표 (Ruby 대비).

---

## 7. 성능 목표 & 검증

### 단기 (이 크레이트 단독)

- `Uri::parse(typical_invite_from_uri)` ≤ 1 μs (Ruby 5.1 μs → 5×)
- `Address::parse(typical_invite_from_header)` ≤ 1.5 μs (Ruby 6.9 μs → 4.5×)
- 할당: 전형 입력당 ≤ 6 allocations (Ruby ~15)

### 장기 (tsip-core 통합 후)

- tsip-core B2BUA INVITE cps 7,244 → **8,000+ 목표** (+10% 이상)
- stackprof Uri/Address self% 13.2% → ≤ 4%
- 회귀: tsip-core `rake test` 197/470 유지, 10k iter fuzz crashes=0

위 지표는 크레이트 자체 가 아니라 **통합 시점** 의 검증 항목. 크레이트 레벨
에서는 위 단기 목표만 충족하면 됨.

---

## 8. 향후 Ruby 통합 경로 (이 크레이트 scope 밖)

이 크레이트를 tsip-core 에 연결하는 옵션:

### 옵션 A: tsip-core 내부 `ext/tsip_core/` (권장)

```
tsip-core/
├── ext/tsip_core/
│   ├── Cargo.toml          # sip_uri = { path = "../../../sip_uri_crate" }
│   ├── extconf.rb          # rb_sys::create_rust_makefile
│   └── src/
│       ├── lib.rs          # #[magnus::init] → TsipCore::Native 모듈
│       ├── uri_binding.rs  # sip_uri::Uri ↔ Ruby Hash 변환
│       └── address_binding.rs
└── lib/tsip_core/sip/
    ├── uri.rb              # Native 있으면 위임, 없으면 기존 byte-scan
    └── address.rb
```

바인딩 crate 가 magnus 의존성을 가지고 pure-crate 는 magnus 비의존성 유지.
stone_smith / stone-webrtc 패턴과 동일.

### 옵션 B: 별도 `stone-sip-uri` gem

`tsip-core` 외에 쓰는 Ruby SIP 프로젝트가 생기면 그때 분리. 현재 수요 없음.

이 크레이트는 두 옵션 모두 지원하도록 **magnus/rb-sys 의존성을 크레이트에 넣지
않음**. 바인딩은 상위 crate 에서 얇은 래퍼로 처리.

---

## 9. 작업 순서 제안

1. **스캐폴드** (1h)
   - `cargo init --lib`
   - `src/lib.rs` 에 empty re-export, error.rs 스켈레톤
   - CI (.github/workflows/rust.yml) 또는 `just test` Makefile 수준으로

2. **Uri 핵심** (1 일)
   - `detect_scheme`, `pct_decode`, `downcase_ascii`, `parse_host_port_range`
     하위 함수 먼저. 각각 unit test.
   - `parse_range` 메인 루프. Ruby 코드 주석 달며 이식.
   - `append_to` + Display 구현.
   - parity 테스트 ≥ 15 개.

3. **Address 핵심** (0.5 일)
   - `extract_display`, `classify_bare_param`
   - `parse` (name-addr / bare 분기), `parse_bare_range`
   - `append_to`
   - parity 테스트 ≥ 10 개.

4. **Fuzz + bench** (0.5 일)
   - `cargo fuzz init`, fuzz_target 2개.
   - `benches/parse_bench.rs` criterion 기반, typical input 5종.
   - 로컬에서 60초 fuzz 돌려 crashes=0 확인.
   - bench 결과를 `docs/BENCH.md` 에 기록.

5. **문서** (0.5 일)
   - `README.md`: 간단한 사용 예시 + 성능 수치
   - CHANGELOG.md
   - 이 HANDOVER.md 의 구현 완료 체크리스트 업데이트

**총 추정**: 2.5-3 일 (Rust 숙련도에 따라 ±1 일).

---

## 10. 구현 시 참조할 RFC 인용

Rust 코드 주석에 RFC 레퍼런스를 명시. 유지보수 시 왜 이렇게 되어있는지 추적
가능하게.

```rust
/// Parse a SIP URI per RFC 3261 §19.1 (or tel: URI per RFC 3966).
///
/// Grammar (simplified):
///   SIP-URI   = "sip:" [ userinfo "@" ] hostport uri-parameters [ headers ]
///   SIPS-URI  = "sips:" [ userinfo "@" ] hostport uri-parameters [ headers ]
///   tel-URI   = "tel:" telephone-subscriber
///   userinfo  = ( unreserved | escaped | user-unreserved ) [ ":" password ] "@"
///   hostport  = host [ ":" port ]
///   uri-parameters    = *( ";" uri-parameter )
///   headers    = "?" header *( "&" header )
///
/// Note: SIP URIs differ from RFC 3986 generic URIs in that they have
/// no `//` authority prefix and no path component. Parameters appear at
/// the top level, not after a path.
pub fn parse(input: &str) -> Result<Self, ParseError> { ... }
```

---

## 11. Non-goals 재확인 (스코프 크립 방지)

- WebRTC / RTP / media — stone-webrtc 담당
- 크립토 / TLS / X.509 — stone_smith 담당
- SIP 헤더 리스트 전체 파싱 — tsip-core Parser 담당
- Transaction / Dialog / Session — tsip-core SIP 레이어 담당
- HTTP / WebSocket URI 파싱 — 이 크레이트는 SIP URI 문법 전용
- 비동기 I/O — 순수 파서, I/O 없음

이 범위를 넘어가는 요청이 들어오면 거부하고 해당 crate/gem 에서 처리.

---

## 12. 열린 결정 사항 (후속 세션에서 판단)

1. **pct_decode 결과가 non-UTF-8 일 때 정책**
   - 옵션 a: `ParseError::InvalidUtf8` 반환 (엄격)
   - 옵션 b: `Vec<u8>` 필드 사용 (Ruby 와 parity 높음)
   - 옵션 c: `String::from_utf8_lossy` 로 `U+FFFD` 치환 (데이터 손실)
   - 현재 권고: **(a) 엄격**. SIP URI 가 ASCII 범위 외 바이트 가진 건 거의
     항상 버그. tsip-core fuzz 에서 예외 탐지되면 (b) 로 전환.

2. **params/headers 를 `Vec` 로 할지 `IndexMap` 으로 할지**
   - `Vec<(K,V)>`: 외부 의존성 0, 소량이면 빠름
   - `indexmap` crate: 순서 보존 + O(1) 조회, dep 추가
   - 현재 권고: **Vec 유지, 측정 후 재고**.

3. **scheme field 의 타입**
   - 현재 제안: `&'static str`
   - 대안: `enum Scheme { Sip, Sips, Tel }` — 패턴매치 안전, 디스패치 빠름
   - 선택 권고: **enum 이 rust-idiomatic**. `as_str()` 로 literal 반환.

후속 결정자가 판단.

---

## 끝

이 문서는 크레이트 신규 구현 세션의 진입점. 1 section ~ 5 section 을 우선
읽고 6~9 는 작업 진행 중 참조. 구현 완료 후 이 파일 맨 위에 "구현 완료 상태"
요약 블록 추가 (tsip-core 핸드오버 문서와 동일 패턴).

- 구현자: (미정)
- 리뷰어: tsip-core 유지보수자
- 원본 Ruby 구현: `tsip-core` @ `lib/tsip_core/sip/uri.rb`, `address.rb`
- 성능 컨텍스트: `tsip-core/docs/PERFORMANCE_HANDOVER.md`
