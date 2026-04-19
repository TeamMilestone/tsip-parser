# tsip-parser v0.2.0 핸드오프 — Option B (permissive + escape-on-render)

작성일: 2026-04-19
대상 crate: `tsip-parser` (this repo, `sip_uri_crate/`) v0.1.1 → v0.2.0
연관 문서:
- `tsip-core/docs/TSIP_PARSER_CRATE_HANDOVER.md` — v0.1.1 에서 Ruby 대비 갈라진
  지점 및 옵션 A/B/C 분석 (이 문서의 origin).
- `sip_uri_crate/docs/HANDOVER.md` — crate 원 설계 문서.

## 1. 배경 한 줄

v0.1.1 의 엄격 validator 가 Ruby 가 수용하는 실트래픽 입력 (pct-encoded user
특수문자 등) 을 `InvalidHost` 로 거부함. tsip-core 통합 시 회귀 위험이 커서
**crate 쪽이 Ruby 쪽과 동일한 관대함으로 수렴** 하기로 결정 (옵션 B). 대신
round-trip 안정성은 render 시 escape 로 확보.

## 2. 결정 근거 (요약)

교차검증 14 입력 결과 (상세는 `TSIP_PARSER_CRATE_HANDOVER.md` §6 참조):

- **6 건 hard reject diff** (Ruby OK / Rust InvalidHost):
  `sip:%40alice@host`, `sip:%3Calice@host`, `sip:al%25ice@host`,
  `<sip:alice@host>;tag=`, `sip:alice@host;<evil>=1`, `<sip:alice@host>;?=bad`
- **2 건 silent trim diff** (핸드오버에 없던 신규 발견):
  - `sip:alice@host;transport= TCP` — Ruby `" TCP"`, Rust `"TCP"` (공백 손실)
  - `sip:alice@host?key= val` — 동일 패턴 (URI header 쪽)
- **2 건 silent drop** (empty key entry 상실): 영향 낮음, 옵션 B 에서도 유지
  가능
- **4 건 완전 parity**

tsip-core 호출처 감사: `Address.parse` 가 From/To/Contact/Route/Record-Route
/ Registrar Contact 의 **raw header value** 를 처리. pct-encoded userinfo 는
RFC 3261 §19.1.2 에 의해 실트래픽에 등장 가능 → Rust v0.1.1 을 integration
하면 프로덕션 회귀 위험 큼.

결론: **옵션 B 수정판** — 핸드오버 §3 원안 + 신규 발견(#11, #12)까지 포함.

## 3. 수정 작업 목록

### 3.1 `src/uri.rs`

| 변경 | 현재 위치 (v0.1.1) | 동작 |
|------|-------------------|------|
| `validate_token` 호출 제거 | lines 115, 116, 121 (`parse_range` 내부 userinfo 처리) | pct-decode 된 user/password 에 어떤 바이트든 허용 |
| `validate_token` 함수 자체 제거 | line 542 부근 | 데드 코드 정리 |
| `validate_param_key` / `validate_param_value` 호출 제거 | lines 377-378 (`parse_param_range`), 419-420 (`parse_header_range`) | key/value 바이트 제한 제거 |
| `validate_pct_decoded` 호출 제거 | lines 421-422 (`parse_header_range`) | header key/value 의 `%`/edge-ws 제한 제거 |
| `validate_pct_decoded` / `validate_param_key` / `validate_param_value` 함수 본체 제거 | lines 430, 570, 588 부근 | 데드 코드 |
| `validate_host` 는 유지 | line 526 부근 | `<`/`>`/`[`/`]` + edge whitespace 방어용 — angle-bracket-in-Uri 오용 케이스(#9, #14) 대응. tsip-core 호출처에서는 이 입력이 직접 들어오지 않으므로 유지해도 회귀 0 |
| **`parse_param_range` value 쪽 trim 제거** | lines 358-362 (`scan::trim_ws(src, eq + 1, to)`) | value 는 원문 바이트 그대로 slice. key 쪽 trim 은 유지 (Ruby `Uri.parse_param_range` 가 key trim 함) |
| **`parse_header_range` value 쪽 trim 제거** | 동일 패턴 (line 383-420 구간) | URI header value 도 동일 |
| **`append_to` 에 escape 추가** | line 266 부근 | user/password 내 구분자 바이트를 pct-encode 해서 출력. round-trip 안정 회복 |

#### escape 대상 바이트 (append_to userinfo 영역)

user / password 문자열에 아래 바이트가 나오면 `%XX` 로 출력:
- `@` (0x40) — user/host 구분자 충돌
- `:` (0x3A) — user/password 구분자 충돌
- `;` (0x3B) — userinfo 종료 + params 시작 충돌
- `?` (0x3F) — userinfo 종료 + headers 시작 충돌
- `<` (0x3C), `>` (0x3E) — Address wrapping 과 충돌
- `%` (0x25) — pct-encode 자체와 충돌 (리터럴 `%` 는 `%25`)
- `&`, `=` — header 구분자 (userinfo 에서는 불필요하나 보수적)
- edge whitespace (`SP`/`HTAB`/`CR`/`LF`) — 재파싱 시 외부 trim 에 잡혀 손실됨

다른 필드 (host, param key, param value, header key, header value) 는 escape
**불필요** (Ruby 가 안 함, parity 유지).

권장 구현 형태:

```rust
fn append_userinfo_escaped(buf: &mut String, src: &str) {
    for &b in src.as_bytes() {
        match b {
            b'@' | b':' | b';' | b'?' | b'<' | b'>' | b'%' | b'&' | b'='
            | b' ' | b'\t' | b'\r' | b'\n' => {
                buf.push('%');
                buf.push(HEX_UPPER[(b >> 4) as usize] as char);
                buf.push(HEX_UPPER[(b & 0x0F) as usize] as char);
            }
            _ => buf.push(b as char),
        }
    }
}

const HEX_UPPER: &[u8] = b"0123456789ABCDEF";
```

Ruby `Uri#append_to` (lib/tsip_core/sip/uri.rb:416-440) 는 escape 안 함 —
Ruby 구현이 의도적으로 permissive. 하지만 Ruby 쪽도 실운영에서 user 에 `@` 가
있으면 애초에 이상 입력이라 재파싱해도 의미 틀어짐. crate 는 safer 한 방향
(escape) 을 택함 — **이 차이는 핸드오프 §5 에 명시** 해 tsip-core 유지자가
파악할 수 있게 함.

### 3.2 `src/address.rs`

| 변경 | 현재 위치 | 동작 |
|------|-----------|------|
| `validate_param_key` / `validate_param_value` 호출 제거 | lines 227-228 (`classify_bare_param`) | Address-level param (tag/q/expires 포함) 검증 제거 |
| import 문 정리 | line 9 (`use crate::uri::{parse_param_range, validate_param_key, validate_param_value, Uri}`) | 제거된 함수 빼기 |
| `classify_bare_param` value 쪽 trim 제거 | value slice 부분 | bare address 의 tag= 뒤 공백 보존 |

### 3.3 `src/error.rs`

- `ParseError::InvalidHost` variant 는 유지 (host 에 `<>[]` 등 들어간 경우
  #9/#14 케이스에서 여전히 발생). 다만 `InvalidToken` 등 다른 바이트-수준
  에러 variant 가 있다면 쓰지 않게 되므로 제거 검토.

### 3.4 테스트 영향

- `tests/uri_parity.rs`, `tests/address_parity.rs` 에 현재 "reject 기대" 인
  assertion 이 있으면 수정 필요. 대신 새로 "pct-encoded 특수문자 수용" 파리티
  테스트 추가:
  ```rust
  #[test]
  fn accepts_pct_encoded_at_in_user() {
      let u = Uri::parse("sip:%40alice@host").unwrap();
      assert_eq!(u.user.as_deref(), Some("@alice"));
      // round-trip: literal @ must be re-escaped
      assert_eq!(u.to_string(), "sip:%40alice@host");
  }

  #[test]
  fn preserves_leading_ws_in_param_value() {
      let u = Uri::parse("sip:alice@host;transport= TCP").unwrap();
      assert_eq!(u.params, vec![("transport".into(), " TCP".into())]);
      assert_eq!(u.to_string(), "sip:alice@host;transport= TCP");
  }
  ```
- `tests/roundtrip.rs` — 변경 없음 이론상. 단 escape 덕에 새로운 round-trip
  케이스 추가 가능.
- `examples/xoracle.rs` (이미 존재) — v0.2.0 실행하면 14 건 전부 `RUST_OK` 가
  나와야 정상. 하나라도 `RUST_ERR` 면 추가 누락.

### 3.5 Fuzz 재실행

```bash
cd /Users/wonsup-mini/projects/sip_uri_crate
cargo +nightly fuzz run uri -- -max_total_time=300
cargo +nightly fuzz run address -- -max_total_time=300
```

- crashes=0, timeouts=0 확인.
- 기존 corpus 의 모든 케이스가 `parse → to_string → parse` 고정점 통과.
- escape 추가로 round-trip 안정성은 오히려 개선되어야 함.

### 3.6 벤치 재측정

`cargo bench` 실행 후 v0.1.1 대비 회귀 확인. 기대:
- `uri_parse_typical` 165 ns → ≤ 170 ns (validator 제거로 소폭 개선 혹은 동등)
- `uri_to_string_typical` 72 ns → ≤ 90 ns (escape 바이트 스캔 추가로 약간 상승
  가능, 25% 이내면 수용)
- 크게 회귀(>30%)하면 escape 핫루프 재검토.

## 4. 릴리스 체크리스트

1. 위 §3.1 ~ §3.3 코드 수정
2. §3.4 테스트 추가/갱신
3. `cargo test --release` 전부 통과
4. `cargo run --release --example xoracle` → 14건 모두 RUST_OK + rt_stable
5. §3.5 fuzz 5분 × 2 타겟 → crashes=0
6. §3.6 bench 회귀 없음 (±10% 이내)
7. `CHANGELOG.md` 추가:
   ```
   ## 0.2.0 — 2026-MM-DD
   - BREAKING: Relaxed input validation to match Ruby tsip-core parity.
     pct-encoded special chars in userinfo, `<`/`>` in param keys, and
     leading/trailing whitespace in param values are now accepted.
   - Added render-side escape in `Uri::append_to` for user/password bytes
     that would break round-trip (`@`, `:`, `;`, `?`, `<`, `>`, `%`, `&`,
     `=`, whitespace).
   - `InvalidHost` still raised for angle-bracket wrapping passed directly
     to `Uri::parse` (use `Address::parse` for name-addr inputs).
   ```
8. `Cargo.toml` version bump `0.1.1` → `0.2.0`
9. `git tag v0.2.0 && git push --tags`
10. `cargo publish` (crates.io)

## 5. tsip-core 측 검증 (crate 작업 외)

crate 릴리스 후 tsip-core 저장소에서:

```bash
cd /Users/wonsup-mini/projects/tsip-core
bundle exec rake test
# 기대: 197 runs, 470 assertions, 0 failures (v0.1.1 때와 동일)

ITERATIONS=10000 bundle exec ruby tools/fuzz_sip.rb
# 기대: crashes=0
```

crate 는 아직 tsip-core 에 integrate 안 되어 있으므로 위 두 명령은 v0.2.0
내용과 무관하게 통과해야 정상. 이건 옵션 B 의 정의 ("tsip-core 쪽 회귀 0")
확인 과정.

실제 통합은 별도 단계 (`ext/tsip_core/` 스캐폴드). 본 v0.2.0 핸드오프 스코프
**밖**.

## 6. 예상 작업 시간

- §3.1 `src/uri.rs` 수정: 1.5h (validator 제거 + escape 추가 + 단위 테스트)
- §3.2 `src/address.rs` 수정: 30m
- §3.4 테스트 갱신: 1h
- §3.5 fuzz 재실행: 15m (대기 시간)
- §3.6 bench 측정 + 검토: 30m
- CHANGELOG + release: 30m

총 **~4h**. 단일 세션으로 충분.

## 7. 리스크

| 리스크 | 가능성 | 대응 |
|--------|-------|------|
| escape 추가로 Uri#to_s 가 Ruby 와 byte-identical 아님 | **확실** | 문서화. Ruby `to_s` 는 user 에 `@` 있으면 그대로 출력 (`sip:@alice@host`), Rust 는 `sip:%40alice@host` 출력. Ruby 결과는 재파싱 시 의미가 틀어지고, Rust 결과는 안정. 통합 시 Ruby fallback 경로가 Rust 결과와 다를 수 있음 — `ext/tsip_core/` 통합 단계에서 선택적 escape 모드 제공 가능 |
| fuzz 가 새로운 round-trip 실패 경로 발견 | 중간 | 발견 시 escape 대상 바이트 확장. 현재 목록은 방어적이라 확장 여지 충분 |
| v0.2.0 이 crates.io 에 publish 됐는데 회귀 발견 | 낮음 | v0.2.1 로 즉시 yank + 재릴리스 |

## 8. 비목표 (이 핸드오프 스코프 밖)

- `ext/tsip_core/` magnus 바인딩 — integration 세션.
- Ruby `Uri#append_to` 쪽 escape 추가 — tsip-core 유지자 판단.
- 성능 극대화 (SIMD, SSE4.2 등) — 현재 ns 대 μs 차이로 불필요.
- 별도 gem 분리 — 현재 내부 확장 방침 유지.

---

작성자: tsip-core 성능 분석 / crate 교차검증 담당
검토 요청: tsip-parser crate 유지자 (실제 코드 수정 실행자)
