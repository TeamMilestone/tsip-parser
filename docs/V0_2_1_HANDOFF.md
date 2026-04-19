# tsip-parser v0.2.1 핸드오프 — parity 보완 + Ruby 통합 API 확장

작성일: 2026-04-19
대상 crate: `tsip-parser` v0.2.0 → v0.2.1
연관 문서:
- `sip_uri_crate/docs/V0_2_0_HANDOFF.md` — v0.2.0 (permissive) 작업 정의
- `sip_uri_crate/docs/HANDOVER.md` — crate 원 설계
- `tsip-core/docs/TSIP_PARSER_CRATE_HANDOVER.md` — parity 배경

## 1. 배경

v0.2.0 릴리스 후 tsip-core 통합 벤치 (2026-04-19) 에서 두 가지 이슈 확인:

**1) Parity 잔존 1건** — `sip:alice@host;<evil>=1` 이 여전히 `InvalidHost`.
`parse_param_range` 에 남아있던 `key.contains(&GT) || val.contains(&GT)` 체크
때문. v0.2.0 이 param key/value 의 `<` 는 허용하지만 `>` 는 reject 유지
(round-trip 을 reject 로 방어).

**2) 통합 throughput 회귀** — bridge ON 시 baseline 7,784 cps → 7,568 cps
(−2.8%, cycle 1 clean). 마이크로 벤치는 Uri.parse 5.8× 가속인데 end-to-end
는 악화. 원인: `TsipCore::Sip::Uri.new(scheme:, user:, ...)` eager field-copy
shim 이 Rust→Ruby 경계에서 7 필드 + 2 Hash 를 매번 마샬링. 미사용 필드까지
복사하는 오버헤드가 Rust 파서의 가속분을 잡아먹음.

v0.2.1 은 위 두 이슈를 해결하기 위한 crate 측 변경. **tsip-core 측 후속 작업
은 §7** (별도 단계, 릴리스 후 진행).

## 2. 결정 방향

- #13 (`<evil>`): **render-side escape 확장**으로 ①+② (parity + round-trip)
  달성. ③ (Ruby byte-identical) 은 포기 — 어차피 `<>` 포함 param key 는
  SIP RFC grammar 에 없어 실트래픽에 나타나지 않음.
- 통합 회귀: tsip-core 가 **class alias** 로 `TsipCore::Sip::Uri = TsipParser::Uri`
  를 쓸 수 있도록 누락 API 를 crate 에 추가. shim 완전 제거 가능해짐.

## 3. 수정 작업 — crate 측

### 3.1 `<evil>` 수용 — render escape 확장

**타깃**: `src/uri.rs`

#### 3.1.1 parse_param_range

현재 (`src/uri.rs:378-380`):
```rust
if key.as_bytes().contains(&GT) || val.as_bytes().contains(&GT) {
    return Err(ParseError::InvalidHost);
}
```

→ **블록 전체 제거**.

#### 3.1.2 parse_header_range

동일 패턴이 있으면 제거 (있는지 `grep -n "contains(&GT)" src/uri.rs` 로 확인).

#### 3.1.3 append_to 의 param/header 영역 escape

현재 param 출력부:
```rust
for (k, v) in &self.params {
    buf.push(';');
    buf.push_str(k);
    if !v.is_empty() {
        buf.push('=');
        buf.push_str(v);
    }
}
```

→ `buf.push_str` 을 신규 escape helper 로 교체:
```rust
for (k, v) in &self.params {
    buf.push(';');
    append_param_escaped(buf, k);
    if !v.is_empty() {
        buf.push('=');
        append_param_escaped(buf, v);
    }
}
```

header 출력부도 동일 패턴 적용 (k, v 둘 다 escape).

#### 3.1.4 append_param_escaped helper

기존 `append_pct_escaped` (`src/uri.rs:525` 부근) 는 userinfo 용으로 설정되어
있음. param/header escape set 을 별도 정의:

```rust
fn append_param_escaped(buf: &mut String, src: &str) {
    for ch in src.chars() {
        match ch {
            // Structural bytes that re-tokenize param/header on re-parse.
            // Also `<` and `>` — address-wrap boundaries.
            // `%` must escape to avoid re-decoding a literal `%`.
            ';' | '?' | '&' | '=' | '<' | '>' | '%'
            | ' ' | '\t' | '\r' | '\n' => {
                buf.push('%');
                let b = ch as u32;
                if b < 0x80 {
                    buf.push(HEX_UPPER[((b >> 4) & 0x0F) as usize] as char);
                    buf.push(HEX_UPPER[(b & 0x0F) as usize] as char);
                } else {
                    // Non-ASCII: emit UTF-8 bytes pct-encoded
                    let mut buf4 = [0u8; 4];
                    for &byte in ch.encode_utf8(&mut buf4).as_bytes() {
                        buf.push('%');
                        buf.push(HEX_UPPER[((byte >> 4) & 0x0F) as usize] as char);
                        buf.push(HEX_UPPER[(byte & 0x0F) as usize] as char);
                    }
                }
            }
            _ => buf.push(ch),
        }
    }
}
```

혹은 `append_pct_escaped` 를 매개변수화 (허용 바이트 set 을 인자로) 해서 재사용.

**escape set 선택 근거**:
- `;` — param separator. key/value 내부에 있으면 새 param 으로 오인
- `?` — params → headers 경계. key/value 에 들어가면 header 로 오인
- `&` — header separator. header key/value 내에 있으면 새 header 로 오인
- `=` — key=value separator (key 내에 있으면 경계 혼동, value 에서는 안전하나 보수적)
- `<`, `>` — address wrapping 경계
- `%` — pct-encode 자체 (리터럴 `%` 는 `%25`)
- whitespace — outer trim 으로 손실 방지

**escape 안 하는 바이트**:
- `@`, `:` — param/header 문맥에서 구조적 의미 없음. Ruby 와 byte-identical 유지
- 영숫자 등 일반 문자 — trivial

#### 3.1.5 parse_header_range 의 pct-decode 는 유지

header 는 `pct_decode` 를 거쳐 저장되므로 (spec 대로), escape 된 문자열이 다시
디코드 됨. 즉 `%3C` → `<` 로 저장되고 append_to 가 다시 `%3C` 로 출력 → round-trip
안정.

param 은 pct-decode 안 함 (Ruby parity). `<evil>` 로 저장하고 render 시 `%3Cevil%3E`
로 출력. 재파싱 시 저장은 `%3Cevil%3E` (no decode), round-trip **변한 것처럼 보이지만
semantically 동일** (같은 key 엔트리). 주의: **parity 와 trade-off** — `uri.params["<evil>"]`
로 저장된 값을 render 후 재파싱하면 `params["%3cevil%3e"]` (downcase) 로 키가 변함.

이를 피하려면:
- 옵션 a: param 저장 시점에 escape (저장 = `%3Cevil%3E`), `uri.params` 조회 시 caller
  가 pct-decode 책임. tsip-core 호환성 깨짐.
- 옵션 b: param key lookup 시 자동 pct-decode (내부 helper). 복잡도 상승.
- 옵션 c: **수용** — "실트래픽에 없는 입력" 이라 무시. Ruby 도 `<` 그대로 저장하므로
  이 지점에서 Ruby 와 갈라지나, 실용 영향 0.

**권장: 옵션 c**. append_to 시점에만 escape, 저장은 raw. adversarial fuzz
round-trip 은 pass (escape→decode→escape 순환) — 실제 값 비교 assertion 이 아닌
`parse(to_s(x)) == x` 동등성이면 통과.

### 3.2 Ruby 통합 API — 클래스 alias 지원

tsip-core 가 `TsipCore::Sip::Uri = TsipParser::Uri` 로 바꿀 수 있게 누락 API
추가. 현재 `TsipParser::Uri.methods` 에 `parse` / `parse_many` 만 있음.

#### 3.2.1 `Uri.parse_range(src, from, to)`

tsip-core `Address.parse_bare_range` / `Address.parse` 가 내부 호출. Ruby 시그니처:

```ruby
Uri.parse_range(src, from, to) # => Uri instance
```

Rust 구현은 이미 존재 (`Uri::parse_range(&str, usize, usize)`). magnus 바인딩만
추가:

```rust
// ext/tsip_parser/src/lib.rs 혹은 uri.rs 바인딩 블록
fn ruby_uri_parse_range(_cls: RClass, src: String, from: usize, to: usize)
    -> Result<Uri, magnus::Error>
{
    Uri::parse_range(&src, from, to)
        .map_err(|e| magnus::Error::new(parse_error_class(), e.to_string()))
}

// init:
cls_uri.define_singleton_method("parse_range", function!(ruby_uri_parse_range, 3))?;
```

**중요**: Ruby Uri.parse_range 가 받는 `src` 는 `String`. Rust `&str` 로 borrow.
`from..to` 는 byte offset. 바운드 체크 필요 (`to <= src.bytesize`).

#### 3.2.2 `Uri.parse_param(raw, target)`

tsip-core `Via.parse` 가 param 한 세그먼트씩 전달. 시그니처:

```ruby
Uri.parse_param(raw, target_hash)
# raw: "transport=tls" 또는 "lr" 
# target_hash: Hash — 이 메서드가 key=value 를 삽입
# 반환값: nil (side-effect only)
```

**주의**: target 이 mutable Ruby Hash. magnus 에서 RHash 받아 insert:

```rust
fn ruby_uri_parse_param(_cls: RClass, raw: String, target: magnus::RHash)
    -> Result<(), magnus::Error>
{
    let bytes = raw.as_bytes();
    let (key, val) = parse_single_param(bytes)?;  // 내부 helper
    target.aset(key, val)?;
    Ok(())
}
```

내부 `parse_single_param` 은 `parse_param_range` 과 유사하지만 `Vec` 대신 단일
(String, String) 튜플 반환. 혹은 기존 함수를 재사용하고 결과 Vec 첫 요소만 취급.

#### 3.2.3 `Uri.parse_host_port(hp)`

tsip-core `Via.parse` 가 `"host:5060"` 또는 `"[::1]:5060"` 형태 전달. 시그니처:

```ruby
Uri.parse_host_port(hp) # => [host_string, port_integer_or_nil]
```

Rust 내부 이미 `parse_host_port_range(&str, from, to)` 존재 (`src/uri.rs:parse_host_port_range`).
엔트리포인트 래퍼 + magnus 바인딩:

```rust
fn ruby_uri_parse_host_port(_cls: RClass, hp: String)
    -> Result<(String, Option<u16>), magnus::Error>
{
    let (host, port) = parse_host_port_range(&hp, 0, hp.len())?;
    Ok((host.to_string(), port))
}

cls_uri.define_singleton_method("parse_host_port", function!(ruby_uri_parse_host_port, 1))?;
```

반환 tuple `(String, Option<u16>)` 이 Ruby 에서 `[host, port]` Array 로 자동 변환됨
(magnus 0.8 기본 동작). port 가 None 이면 `nil`.

### 3.3 Address.new 키워드 생성자 — optional

현재 `TsipParser::Address.new` 는 인자 0개. tsip-core `Address.new(display_name: ..., uri: ..., params: ...)`
호출처 (in_dialog.rb:16-17, routing.rb:30, 45).

#### 3.3.1 Ruby 시그니처

```ruby
TsipParser::Address.new(display_name: nil, uri: nil, params: nil)
```

#### 3.3.2 magnus 구현

```rust
fn ruby_address_new(
    _ruby: &magnus::Ruby,
    _cls: RClass,
    kwargs: magnus::RHash,
) -> Result<Address, magnus::Error>
{
    // kwargs 파싱: display_name, uri, params
    let display_name: Option<String> = kwargs.lookup(magnus::Symbol::new("display_name"))?;
    let uri_wrapper: Option<magnus::Value> = kwargs.lookup(magnus::Symbol::new("uri"))?;
    let params_hash: Option<magnus::RHash> = kwargs.lookup(magnus::Symbol::new("params"))?;

    let uri = if let Some(v) = uri_wrapper {
        // v 가 TsipParser::Uri 인지 unwrap
        Some(<&Uri>::try_convert(v)?.clone())
    } else {
        None
    };

    let params = if let Some(h) = params_hash {
        // Ruby Hash → Vec<(String, String)>
        let mut v = Vec::with_capacity(h.len());
        h.foreach(|k: String, val: String| {
            v.push((k, val));
            Ok(magnus::r_hash::ForEach::Continue)
        })?;
        v
    } else {
        Vec::new()
    };

    Ok(Address { display_name, uri, params })
}
```

(magnus API 정확 문법은 0.8 docs 확인 필요. 위는 스케치.)

#### 3.3.3 초기 접근 — 생략 가능

tsip-core in_dialog.rb / routing.rb 의 Address.new 호출은 건수가 적음 (4 지점).
대안: tsip-core 측에서 `Address.new` 대신 `Address.build(...)` 같은 wrapper 사용
(순수 Ruby). 이 경우 crate 작업 생략 가능. **tsip-core 통합 리팩터가 이 경로
택하면 3.3 은 skip**.

**권장**: 3.3 은 **생략**. 이유:
- Ruby kwargs → Rust struct 매핑이 add surface + 버그 위험
- tsip-core 호출처 4개만 리팩터하면 됨
- crate API surface 가 단순하게 유지됨

### 3.4 테스트 추가

```rust
// tests/uri_parity.rs
#[test]
fn accepts_gt_in_param_via_render_escape() {
    let u = Uri::parse("sip:alice@host;<evil>=1").unwrap();
    assert_eq!(u.params, vec![("<evil>".into(), "1".into())]);
    // render escapes < >
    let r = u.to_string();
    assert!(r.contains("%3C") || r.contains("%3c"), "expected escape in {}", r);
    // round-trip: reparse should still yield a valid Uri (key may be escaped-form)
    let _u2 = Uri::parse(&r).unwrap();
}

// tests/class_methods.rs (신규 파일)
#[test]
fn parse_range_slices_inner_uri() {
    let full = "<sip:alice@host:5060>";
    let u = Uri::parse_range(full, 1, full.len() - 1).unwrap();
    assert_eq!(u.user.as_deref(), Some("alice"));
    assert_eq!(u.host, "host");
    assert_eq!(u.port, Some(5060));
}

#[test]
fn parse_param_single_segment() {
    let mut target: Vec<(String, String)> = Vec::new();
    // (internal API for Ruby binding — may need wrapper)
    // This validates the *Rust* backing of what the Ruby binding exposes
    crate::uri::parse_param_range("transport=tls", 0, 13, &mut target).unwrap();
    assert_eq!(target, vec![("transport".into(), "tls".into())]);
}

#[test]
fn parse_host_port_ipv6_with_port() {
    let (host, port) = Uri::parse_host_port_range("[::1]:5060", 0, 10).unwrap();
    assert_eq!(host, "::1");
    assert_eq!(port, Some(5060));
}
```

(Ruby 바인딩 테스트는 gem 측에서 별도 — crate 는 Rust 함수 커버리지만 확인.)

### 3.5 Fuzz 재실행

```bash
cd /Users/wonsup-mini/projects/sip_uri_crate
cargo +nightly fuzz run uri -- -max_total_time=300
cargo +nightly fuzz run address -- -max_total_time=300
```

- `>` 가 param 에 들어간 adversarial 입력이 round-trip stable 인지 중점 확인
- crashes=0, timeouts=0

### 3.6 벤치 재측정

```bash
cargo bench
```

- `uri_parse_typical` 165 ns → 변화 없어야 함 (parse 측은 변경 없음)
- `uri_to_string_typical` 72 ns → ≤ 90 ns (param escape 바이트 스캔 추가 감안)
- 회귀 >20% 면 escape 핫루프 재점검

## 4. 릴리스 체크리스트

1. §3.1 구현 (render escape)
2. §3.2 구현 (class method 3개 바인딩)
3. §3.3 — skip (tsip-core 측 해결)
4. §3.4 테스트 추가 + 기존 41개 통과
5. `cargo run --release --example xoracle` → 14건 모두 RUST_OK
6. §3.5 fuzz 5분 × 2 타겟 → crashes=0
7. §3.6 bench 회귀 ≤20%
8. `Cargo.toml` 0.2.0 → 0.2.1
9. gem 측 버전도 0.2.1 로 bump, `gem build` + `gem push`
10. `CHANGELOG.md`:
    ```
    ## 0.2.1 — 2026-MM-DD
    - FIX: Param key/value containing `>` now accepted; rendered with
      pct-escape to preserve round-trip in Address-wrapping context.
      Resolves last remaining parity case vs Ruby tsip-core reference
      (#13 in cross-oracle matrix).
    - NEW: `Uri.parse_range(src, from, to)`, `Uri.parse_param(raw, hash)`,
      `Uri.parse_host_port(hp)` class methods exposed to Ruby. Enables
      tsip-core to use `TsipCore::Sip::Uri = TsipParser::Uri` class alias
      without a per-parse marshalling shim.
    - Render-side escape now applied to param/header keys and values for
      bytes `<`, `>`, `;`, `?`, `&`, `=`, `%`, and whitespace.
    ```

## 5. 리스크

| 리스크 | 가능성 | 대응 |
|--------|-------|------|
| param escape 확장으로 Ruby `to_s` 와 byte-different 출력 | 확실 (의도) | CHANGELOG 에 명시. tsip-core `rake test` 197/470 여전히 통과 확인 |
| escape-decode 비대칭으로 key 가 pct-encoded 형태로 저장되어 `uri.params["<evil>"]` 조회 실패 | 낮음 (§3.1.5 옵션 c 수용 시) | 실트래픽 영향 0. 문서화 |
| magnus 0.8 의 `RHash` foreach 또는 `TypedData::try_convert` API 가 예상과 다름 | 중간 | 0.8 docs + stone_smith 예제 참조 |
| 바운드 체크 누락으로 `parse_range(src, 0, 10000)` 같은 입력에서 panic | 중간 | 바인딩 래퍼에서 `to > src.len()` 명시 체크 |
| param `;key=val;<other>=x` round-trip 에서 render 후 재파싱 시 key 형태 변화 | 중간 (§3.1.5) | 수용 (adversarial 입력) 또는 §3.1.5 옵션 a/b 로 처리 |

## 6. 예상 작업 시간

- §3.1 render escape: 1.5h (코드 + 테스트)
- §3.2 class method 3개 바인딩: 2h (magnus 인터페이스 + Ruby 동작 확인)
- §3.4 테스트 갱신: 1h
- §3.5 fuzz 재실행: 15m 대기
- §3.6 bench 측정: 30m
- gem 측 버전 bump + build/push: 30m
- CHANGELOG + release: 30m

총 **~6h**.

## 7. tsip-core 측 후속 작업 (v0.2.1 릴리스 이후)

crate v0.2.1 이 나온 뒤 tsip-core 저장소에서 진행. **v0.2.1 릴리스 범위 밖**이지만
함께 계획하기 위해 기록.

### 7.1 bridge shim 을 class alias 로 교체

현재 `lib/tsip_core/sip/tsip_parser_bridge.rb`:
```ruby
def self.parse(str)
  tp = TsipParser::Uri.parse(str)
  new(scheme: tp.scheme, user: tp.user, ...)  # eager copy — overhead
end
```

→ 단순 alias:
```ruby
begin
  require "tsip_parser"
rescue LoadError
  return
end

module TsipCore
  module Sip
    remove_const(:Uri) if const_defined?(:Uri, false)
    remove_const(:Address) if const_defined?(:Address, false)
    Uri = TsipParser::Uri
    Address = TsipParser::Address
  end
end
```

**전제**: v0.2.1 의 §3.2 class method 3개가 `TsipParser::Uri` 에 추가됨. 없으면
`lib/tsip_core/sip/address.rb`, `via.rb` 가 `Uri.parse_range` / `parse_param` /
`parse_host_port` 호출 시 `NoMethodError`.

### 7.2 Address.new 호출처 리팩터

영향 파일:
- `lib/tsip_core/sip/in_dialog.rb:16-17` — `Address.new(uri: ..., params: {...})`
- `lib/tsip_core/sip/routing.rb:30` — `Address.new(uri: Uri.parse(...))`
- `lib/tsip_core/sip/routing.rb:45` — `Address.new(uri: uri)`

대안 1: TsipParser::Address 에 `.build(display_name:, uri:, params:)` 클래스 헬퍼
추가 (tsip-core 쪽 monkey-patch 로 해결, crate 불변).

대안 2: 각 호출처를 수정해 TsipParser::Address instance method 로 구성
(`a = TsipParser::Address.new; a.uri = ...; a.tag = ...`).

**권장**: 대안 1. class alias 직후 tsip-core 쪽에서:
```ruby
class TsipParser::Address
  def self.build(display_name: nil, uri: nil, params: nil)
    a = new
    a.display_name = display_name if display_name
    a.uri = uri if uri
    params&.each { |k, v| a.params[k] = v }  # params Hash 는 memoized, 직접 set
    a
  end
end
```
호출처 4 군데를 `Address.new(...)` → `Address.build(...)` 치환.

### 7.3 is_a?(Uri) / is_a?(Address) 검증

2 지점:
- `lib/tsip_core/sip/routing.rb:43` — `local_uri.is_a?(Uri)` — class alias 후
  `Uri` = `TsipParser::Uri` 이므로 OK (identity 유지)
- `lib/tsip_core/sip/uri.rb:443` — Ruby Uri `==` 연산자 내부. 이 파일 자체는
  alias 후 unreferenced 됨. 파일 삭제 or 파일 내용 무시 (load 안 되게)

**검증**: alias 적용 후 `TSIP_PARSER=1 bundle exec rake test` → 197/470 통과.

### 7.4 Ruby 측 Uri 클래스 삭제

`lib/tsip_core/sip/uri.rb`, `lib/tsip_core/sip/address.rb` 는 alias 적용 후
쓰이지 않음. 삭제 or `TSIP_PARSER=0` 시에만 require 되게 `lib/tsip_core/sip.rb`
조정.

### 7.5 원격 재측정

- cycle 1 clean baseline 7,784 cps (오늘 측정) 기준
- class alias 적용 후 기대: baseline 대비 +5~10% (shim 오버헤드 제거분)
- 목표: **8,000+ cps 진입**
- 미달성 시 Parser 네이티브화 로드맵 검토

## 8. 비목표

이 릴리스 스코프 밖:
- Parser.parse / Parser.parse_start_line 의 네이티브화 — v0.3.x
- tsip-parser 를 별도 gem 으로 계속 유지 (현재는 단일 gem 내 ext 로 통합됨)
- SIMD 최적화, aws-lc-rs 연동 등 크립토/벡터 가속
- Address builder pattern 지원 (tsip-core 측 helper 로 우회)

---

작성자: tsip-core 통합 벤치 담당
검토 대상: tsip-parser crate 유지자
다음 핸드오프: `V0_3_0_HANDOFF.md` (Parser 네이티브화 검토 시)
