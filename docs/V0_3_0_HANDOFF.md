# tsip-parser v0.3.0 핸드오프 — SIP Message parser 추가

작성일: 2026-04-19
대상 crate: `tsip-parser` v0.2.1 → v0.3.0
연관 문서:
- `sip_uri_crate/docs/V0_2_0_HANDOFF.md` / `V0_2_1_HANDOFF.md` — 이전 릴리스
- `sip_uri_crate/docs/HANDOVER.md` — crate 원 설계
- `tsip-core/docs/PERFORMANCE_HANDOVER.md` — 10차 세션 (bridge v0.2.3 integration)
- `tsip_parser_gem/docs/V0_2_3_HANDOFF.md` — gem 측 최신 릴리스

## 1. 배경

tsip-core 10차 세션(2026-04-19) 결과 bridge ON 시 cps **7,239** (목표 8,000~9,500
의 ~90%). Uri/Address 네이티브화 효과는 이미 수확했고, 남은 pure-Ruby 핫패스는
`TsipCore::Sip::Parser.parse`(메시지 레벨 파서) 단 하나.

11차 세션 원격 stackprof(v0.2.3 ON, INVITE 1000c × 60s, 19,010 CPU 샘플) 기준:

| self% | frame | 비고 |
|------:|-------|------|
| 9.8%  | OpenSSL::SSL::SSLSocket#sysread | 건드릴 수 없음 |
| 8.0%  | OpenSSL::SSL::SSLSocket#syswrite | 건드릴 수 없음 |
| **5.3%** | **TsipCore::Sip::Parser.parse** | total 17.9% |
| 5.2%  | (sweeping) | GC |
| **3.1%** | **Message#add_header** | Parser가 반복 호출 |
| **2.7%** | **Parser.parse_start_line** | |
| 2.7%  | TsipParser::Address.parse | 이미 네이티브 |
| **2.4%** | **Headers.canonical** | add_header 내부 |
| 2.1%  | Parser.trim_ws | |
| 1.9%  | String#byteslice | Parser에서 대부분 |
| 1.7%  | String#index | Parser에서 |

**Parser 관련 self% 합산 ≈ 15~18%**. OpenSSL/GC는 손댈 수 없고, Parser 계열이
유일한 큰 덩어리. v0.3.0 에서 메시지 파서를 네이티브화해서 이 덩어리를 제거한다.

## 2. 스코프

### In scope
- crate 에 `message` 모듈 추가 — `Message::parse(&[u8]) -> Result<Message, ParseError>`
- canonical header name 매핑 (compact form + 대소문자 정규화) 을 crate 내부로 이동
- RFC 3261 §7.3.1 line folding 지원 (`\r\n SP/HTAB` continuation)
- Content-Length 검증 (negative / oversize 는 parse error)
- `ParseError` variant 확장 (아래 §3.3)
- 기존 `Address` / `Uri` API 는 그대로 (breaking change 없음)

### Out of scope
- Via/CSeq/Contact 같은 **구조화 헤더 파싱** — 헤더 값은 생 String 으로 보존
  (gem / tsip-core 에서 기존 `Via.parse` / `CSeq.parse` / `Address.parse` 를 호출).
  이 단계에선 순수 **framing + canonical + Content-Length** 만.
- Body decoding (multipart 등) — raw bytes 그대로 반환.
- 네트워크 레벨 streaming (여러 메시지 pipeline) — 입력은 단일 완결 메시지.

## 3. Crate 측 변경

### 3.1 새 모듈: `src/message.rs`

공개 API (minimal, breaking 없음):

```rust
/// SIP 시작 라인 — Request 또는 Response.
pub enum StartLine {
    Request {
        method: String,      // 대문자 정규화 ("INVITE", "REGISTER", ...)
        request_uri: String, // raw string (파싱은 caller 책임)
        sip_version: String, // "SIP/2.0"
    },
    Response {
        sip_version: String,
        status_code: u16,
        reason_phrase: String,
    },
}

/// 파싱된 SIP 메시지 — headers 순서 보존 Vec, body 는 raw bytes.
pub struct Message {
    pub start_line: StartLine,
    pub headers: Vec<(String, String)>, // (canonical_name, raw_value)
    pub body: Vec<u8>,
}

impl Message {
    pub const MAX_SIZE: usize = 65_536;

    pub fn parse(raw: &[u8]) -> Result<Message, ParseError>;

    /// 편의: Content-Length 값이 선언되어 있으면 파싱해 반환. 없으면 None.
    /// parse() 내부에서 이미 검증된 상태이므로 여기선 단순 lookup.
    pub fn content_length(&self) -> Option<usize>;

    /// 편의: canonical name 으로 첫 헤더 값 조회.
    pub fn header(&self, canonical: &str) -> Option<&str>;
}
```

`lib.rs` 에 `pub mod message;` 추가하고 `pub use message::{Message, StartLine};` 재export.

### 3.2 Canonical 헤더 테이블

tsip-core `lib/tsip_core/sip/headers.rb` 의 `COMPACT` 및 canonical 리스트를
Rust 상수로 포팅:

```rust
// src/message.rs 내부
const COMPACT_MAP: &[(u8, &str)] = &[
    (b'i', "Call-ID"),
    (b'm', "Contact"),
    (b'e', "Content-Encoding"),
    (b'l', "Content-Length"),
    (b'c', "Content-Type"),
    (b'f', "From"),
    (b's', "Subject"),
    (b'k', "Supported"),
    (b't', "To"),
    (b'v', "Via"),
    (b'r', "Refer-To"),
    (b'b', "Referred-By"),
    (b'o', "Event"),
    (b'u', "Allow-Events"),
    (b'a', "Accept-Contact"),
    (b'j', "Reject-Contact"),
    (b'd', "Request-Disposition"),
    (b'x', "Session-Expires"),
    (b'y', "Identity"),
    (b'n', "Identity-Info"),
];

const CANONICAL_LIST: &[&str] = &[
    "Via", "From", "To", "Call-ID", "CSeq", "Contact", "Max-Forwards",
    "Expires", "Record-Route", "Route", "Authorization", "WWW-Authenticate",
    "Proxy-Authorization", "Proxy-Authenticate", "User-Agent", "Server",
    "Content-Type", "Content-Length", "Content-Encoding", "Content-Disposition",
    "Allow", "Supported", "Require", "Accept", "Accept-Encoding",
    "Accept-Language", "Subject", "Event", "Refer-To", "Referred-By",
    "Session-Expires", "Min-SE", "Reason", "Date", "Timestamp", "Warning",
    "Organization", "Priority",
];
```

Lookup 전략:
1. 1 바이트 compact form (case-insensitive) 이면 `COMPACT_MAP` 직접 매칭 → 정적 `&str` 반환 (alloc 0)
2. 길이 > 1 이면 `CANONICAL_LIST` 를 ASCII case-insensitive 비교로 스캔 (소수·짧은 리스트라 selective hash 보다 빠름)
3. miss 시 `capitalize_dashed` 로 allocation 후 반환

첫 2 케이스가 hot path 의 ~95% 를 차지할 것으로 예상 (8차 benchmark 기준).

Ruby `Headers.canonical` 의 CANONICAL_CACHE 처럼 **런타임 learning cache 는 두지
않음** — Rust 파서는 stateless 로 유지. 프로세스 단위 cache 이득보다 thread-safety
+ 테스트 가능성 우선.

### 3.3 `ParseError` 확장

`src/error.rs` 에 variant 추가:

```rust
pub enum ParseError {
    // (기존)
    Empty,
    UnterminatedBracket,
    UnterminatedQuote,
    UnterminatedAngle,
    InvalidScheme,
    InvalidUtf8,
    InvalidHost,
    // (신규 v0.3.0)
    MessageTooLarge,          // raw.len() > Message::MAX_SIZE
    EmptyMessage,              // headers_end == 0
    InvalidStartLine,          // 토큰 개수 부족 / SIP-Version 형식 불량
    InvalidStatusCode,         // 3-digit 아님 / parse 실패
    HeaderMissingColon,        // ":" 없는 헤더 라인
    NegativeContentLength,     // "-1" 등
    OversizeContentLength,     // > MAX_SIZE
    BadContentLength,          // 숫자 파싱 실패
}
```

기존 variant 는 그대로 — Address/Uri 에서는 새 variant 들이 절대 생성되지 않음.

### 3.4 파싱 알고리즘 (요약)

tsip-core `lib/tsip_core/sip/parser.rb` 의 byte-scan 을 그대로 Rust 포팅. 입력이
`&[u8]` 이라 Ruby 의 `raw.b` / `raw.getbyte` / `raw.byteslice` 변환 없이 바로 슬라이스.

1. `raw.windows(4).position(|w| w == b"\r\n\r\n")` 로 header/body 경계. 없으면
   `\n\n` fallback, 그것도 없으면 whole message = headers only (body empty).
2. `raw[0..first_nl]` 를 start-line 으로 SP/HTAB 로 3 토큰 분할.
   - 첫 토큰이 `SIP/` 로 시작하면 Response, 아니면 Request.
   - Request: `method` 는 `to_ascii_uppercase`, `sip_version` 은 `SIP/` prefix 체크.
   - Response: `status_code` 는 3-digit, `u16::parse`.
3. 각 헤더 라인:
   - 뒤이은 라인이 SP/HTAB 시작이면 folding — 현재 라인에 space 한 개 + trimmed
     continuation 이어붙임.
   - `":"` 위치 찾아 name/value 분리. 양쪽 trim_sp_tab.
   - `canonical_of(name)` 로 정규화 후 `headers.push((canonical, value))`.
4. Body = raw[body_start..].
   - `Content-Length` 선언이 있으면 parse / 검증. `len < raw.len() - body_start`
     이면 `body = raw[body_start..body_start+len]`.

Folding lazy path: Ruby 구현처럼 folding 없는 흔한 케이스는 추가 allocation 없이
raw 에서 바로 `str::from_utf8_unchecked` 없는 안전 슬라이스 (헤더 값 저장은
`String::from_utf8_lossy` 대신 `str::from_utf8` + 명시적 에러).

### 3.5 테스트 (`tests/message_parity.rs`)

정적 corpus 최소 20 건 — INVITE, REGISTER, BYE, CANCEL, OPTIONS, SUBSCRIBE,
NOTIFY, REFER, MESSAGE, INFO, PRACK, UPDATE, 1XX/2XX/3XX/4XX/5XX/6XX 응답, 
compact-form header, folded header, body 있는/없는 케이스.

정확도 검증:
- start_line 필드 일치 (tsip-core Ruby Parser 출력과 field-by-field)
- headers 순서 + canonical name 일치
- body bytes 완전 일치
- error 변환 — malformed corpus 20 건 에 대해 Rust / Ruby 모두 ParseError (variant 종류는 달라도 무방)

퍼저 (`fuzz/fuzz_targets/fuzz_message.rs`) 추가. `cargo fuzz run fuzz_message -- -max_total_time=600` 로 panic=0 보장.

벤치 (`benches/message_bench.rs`) — INVITE 10-header 기준 criterion.
기대치: 5~8 μs/parse (Ruby 현재 ~15.5 μs/parse 대비 2~3×).

## 4. Gem 측 변경 (`tsip_parser_gem`, v0.2.3 → v0.3.0)

### 4.1 `ext/tsip_parser/src/message.rs` 신규

```rust
use magnus::{function, prelude::*, Error, RArray, RHash, RString, Ruby, Symbol};
use tsip_parser::{Message, StartLine};

fn parse(input: RString) -> Result<RHash, Error> {
    let ruby = unsafe { Ruby::get_unchecked() };
    let bytes = unsafe { input.as_slice() };
    let m = Message::parse(bytes).map_err(|e| crate::error::to_ruby(&ruby, e))?;

    let result = ruby.hash_new_capa(6);
    let headers = build_headers_hash(&ruby, &m.headers)?;

    match m.start_line {
        StartLine::Request { method, request_uri, sip_version } => {
            result.aset(Symbol::new("kind"), Symbol::new("request"))?;
            result.aset(Symbol::new("method"), method)?;
            result.aset(Symbol::new("request_uri"), request_uri)?;
            result.aset(Symbol::new("sip_version"), sip_version)?;
        }
        StartLine::Response { sip_version, status_code, reason_phrase } => {
            result.aset(Symbol::new("kind"), Symbol::new("response"))?;
            result.aset(Symbol::new("sip_version"), sip_version)?;
            result.aset(Symbol::new("status_code"), status_code)?;
            result.aset(Symbol::new("reason_phrase"), reason_phrase)?;
        }
    }
    result.aset(Symbol::new("headers"), headers)?;
    result.aset(Symbol::new("body"), ruby.str_from_slice(&m.body))?;
    Ok(result)
}

fn build_headers_hash(ruby: &Ruby, pairs: &[(String, String)]) -> Result<RHash, Error> {
    // canonical name -> [values] (Ruby Array<String>) 로 그룹핑
    let hash = ruby.hash_new_capa(pairs.len().min(16));
    for (name, value) in pairs {
        let key = RString::new(name);
        let existing: Option<RArray> = hash.aref(key).ok();
        match existing {
            Some(arr) => { arr.push(value.as_str())?; }
            None => {
                let arr = ruby.ary_new_capa(1);
                arr.push(value.as_str())?;
                hash.aset(RString::new(name), arr)?;
            }
        }
    }
    Ok(hash)
}
```

`init` 에서 `TsipParser::Message` 클래스 정의 + `define_singleton_method("parse", ..., 1)`.

Ruby 상에서의 계약:

```ruby
TsipParser::Message.parse("INVITE sip:...\r\n...") # => Hash
# => { kind: :request, method: "INVITE", request_uri: "sip:...",
#      sip_version: "SIP/2.0",
#      headers: { "Via" => ["SIP/2.0/TLS ..."], "From" => ["..."], ... },
#      body: "" }
```

### 4.2 인코딩 주의

`RString::new(&m.body)` 는 encoding 이 UTF-8 로 잡히는데 SIP body 는 임의 bytes
일 수 있음. Ruby 측에서 `body.force_encoding(Encoding::ASCII_8BIT)` 하거나 rust
에서 `ruby.enc_associate(body, rb_ascii8bit_encindex())` — 후자를 권장.

### 4.3 에러 매핑

`crate::error::to_ruby` 가 `ParseError` variant 전부 핸들링하도록 확장.
v0.3.0 신규 variant 는 전부 `TsipParser::ParseError` (ArgumentError sub) 로 raise.

### 4.4 Gemfile / Cargo

- gem `tsip_parser.gemspec` version bump 0.2.3 → 0.3.0
- `ext/tsip_parser/Cargo.toml` `tsip-parser = "0.3"` (path = "../../sip_uri_crate" 로 로컬 개발, 릴리스 시 버전 올림)
- CHANGELOG 갱신

## 5. tsip-core 측 통합

### 5.1 `lib/tsip_core/sip/tsip_parser_bridge.rb` 확장

기존 bridge 는 Uri/Address class-alias 만 담당. v0.3.0 부터 Parser 도 override.

```ruby
if defined?(TsipParser::Message)
  module TsipCore
    module Sip
      module Parser
        def self.parse(raw)
          h = TsipParser::Message.parse(raw.is_a?(String) ? raw : raw.to_s)
          msg = if h[:kind] == :request
            Request.new(
              method: h[:method],
              request_uri: h[:request_uri],
              sip_version: h[:sip_version],
            )
          else
            Response.new(
              sip_version: h[:sip_version],
              status_code: h[:status_code],
              reason_phrase: h[:reason_phrase],
            )
          end
          msg.instance_variable_set(:@headers, h[:headers])
          msg.body = h[:body]
          msg
        rescue TsipParser::ParseError => e
          raise ParseError, e.message
        end
      end
    end
  end
end
```

포인트:
- `@headers = h[:headers]` 로 직접 할당 — `add_header` 루프 skip (Parser.parse /
  Message#add_header / Headers.canonical 세 프레임 동시 증발).
- canonical name 은 Rust 에서 이미 적용되어 있음 → Ruby 측 재검증 불필요.
- error message 문자열은 Ruby `ParseError` 로 승격해 기존 rescue 경로 호환.

### 5.2 회귀

- `bundle exec rake test` (OFF) → 197 / 470 / 0 — 유지
- `TSIP_PARSER=1 bundle exec rake test` (ON) → 동일 목표. Rust/Ruby 의 error
  메시지 문자열 차이로 `assert_raises` 가 실패할 수 있음 → 메시지 기대값을
  `ParseError` 클래스만 체크하도록 완화 필요할 수 있음.
- `ITERATIONS=10000 bundle exec ruby tools/fuzz_sip.rb` (OFF / ON) → crashes=0
- `tools/parity_check.rb` — Message-level comparison 추가. 40+ 정적 corpus + 2k
  mutation iter. bridge ON 과 OFF 출력이 필드별 일치해야 함.

### 5.3 Bench

`tools/bench_message.rb` 는 이미 있음 — Parser.parse 마이크로 벤치 용. v0.3.0
환경에서 OFF / ON 비교 돌려서 per-parse 개선치 기록.

원격 INVITE 1000c × 60s × 3 run 기대:
- baseline (v0.2.3 ON): ~7,239 cps
- v0.3.0 ON: **~7,700~8,000 cps** (Parser 관련 self 15% 중 절반 실이득)
- 목표 8,000 돌파 여부는 박스 load 변동 때문에 3-run 평균으로 판정.

## 6. 릴리스 순서

1. **crate v0.3.0** (sip_uri_crate)
   - `src/message.rs` 작성 + `src/error.rs` variant 추가 + `lib.rs` re-export
   - tests / bench / fuzz 추가
   - Cargo version 0.2.1 → 0.3.0, CHANGELOG
   - 로컬 `cargo test && cargo bench --bench message_bench -- --quick`
2. **gem v0.3.0** (tsip_parser_gem)
   - `ext/tsip_parser/Cargo.toml` `tsip-parser = { version = "0.3", path = "../../sip_uri_crate" }` (로컬), 릴리스 시 path 제거
   - `ext/tsip_parser/src/message.rs` 추가 + `lib.rs` init 에 등록
   - `lib/tsip_parser.rb` 에 Ruby 측 상수 / 문서
   - `rake compile && rake test`, CHANGELOG
3. **tsip-core bridge**
   - `lib/tsip_core/sip/tsip_parser_bridge.rb` Parser override 추가
   - `tools/parity_check.rb` Message corpus 확장
   - OFF / ON 양쪽 rake test 197/470
   - 원격 rsync → 8-worker cluster 기동 → INVITE 1000c × 60s × 3 run 측정
   - `docs/PERFORMANCE_HANDOVER.md` 12차 세션 섹션 추가

## 7. 리스크 / 주의

### 7.1 Headers 순서 보존

`TsipCore::Sip::Message#@headers` 는 현재 Ruby Hash — Ruby 3.1+ 부터 insertion
order 보존됨. Rust 측에서 canonical name 기준 group 시, 같은 이름 반복 헤더의
**값 순서** 는 원본 그대로 유지해야 Via 다중 등에서 라우팅이 깨지지 않음.
`build_headers_hash` 루프가 `pairs` 순서대로 push 하는지 테스트로 명시.

### 7.2 %-encoded Request-URI

start-line 의 `request_uri` 는 현재 Ruby Parser 에서도 **String 그대로 저장**
(파싱은 `Uri.parse` 호출 시). Rust 파서도 동일해야 함 — start-line 쪼개기만
하고 URI 자체는 Uri::parse 로 넘기지 않음. Ruby Request#request_uri 가 String 기대.

### 7.3 Content-Length 미스매치

Ruby 현 구현: `Content-Length` 가 body 실제 길이보다 **짧으면** 잘라서 저장,
**길면** 에러 아니라 raw body 그대로. v0.3.0 도 같은 semantic 유지 (스펙상 진짜
validation 은 transport layer 의 StreamFramer 책임).

### 7.4 Body encoding

Ruby Parser 가 body 를 `force_encoding("UTF-8")` 하고 invalid 면 `.b` 로 되돌림.
Rust 에서 `RString::new(&m.body)` 가 UTF-8 태그만 붙이면 OK — Ruby 측에서
bridge 가 `body.force_encoding(Encoding::ASCII_8BIT) unless body.valid_encoding?`
패턴 유지.

### 7.5 Benchmark 노이즈

10차 세션에서도 박스 load 변동으로 cycle 간 1,000 cps 편차 발생. 측정 시:
- INVITE run 3 회 + median 사용
- `uptime` 1m load < 0.3 확인 후 시작
- stackprof 없는 clean run (오버헤드 1~2% 제거)

## 8. 파일 목록

### crate
- 신규: `src/message.rs`, `tests/message_parity.rs`, `benches/message_bench.rs`, `fuzz/fuzz_targets/fuzz_message.rs`
- 수정: `src/lib.rs` (`pub mod message` + re-export), `src/error.rs` (variants),
  `Cargo.toml` (version), `CHANGELOG.md`

### gem
- 신규: `ext/tsip_parser/src/message.rs`
- 수정: `ext/tsip_parser/src/lib.rs` (init 에 등록), `ext/tsip_parser/src/error.rs`
  (매핑 확장), `ext/tsip_parser/Cargo.toml` (crate 버전 bump), `tsip_parser.gemspec`
  (version), `CHANGELOG.md`

### tsip-core
- 수정: `lib/tsip_core/sip/tsip_parser_bridge.rb` (Parser override 추가),
  `tools/parity_check.rb` (Message corpus), `Gemfile.lock`
- 문서: `docs/PERFORMANCE_HANDOVER.md` 12차 세션 섹션

## 9. 다음 세션 체크리스트

- [ ] crate `src/message.rs` + error variants 작성
- [ ] crate unit tests — 20+ 정상 corpus, 20+ malformed corpus
- [ ] crate fuzz target — 10 분 panic=0
- [ ] crate bench — per-parse μs 기록
- [ ] crate v0.3.0 tag
- [ ] gem `message.rs` + init 등록, `rake compile`
- [ ] gem `TsipParser::Message.parse` Ruby 레벨 smoke test
- [ ] gem v0.3.0 빌드
- [ ] tsip-core bridge Parser override
- [ ] tsip-core `rake test` OFF / ON 197/470
- [ ] tsip-core fuzz 10k OFF / ON crashes=0
- [ ] parity_check.rb Message-level 확장, 55/56+ 유지
- [ ] 원격 INVITE 1000c × 60s × 3 run, cps 기록 → PERFORMANCE_HANDOVER.md 12차 세션
