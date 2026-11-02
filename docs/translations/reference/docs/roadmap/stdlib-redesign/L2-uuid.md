<a id="l2l3-uuid新しいモジュール"></a>

# L2/L3: uuid (new module)

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](L2-uuid.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-uuid.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](L2-uuid.zh-CN.md)

> Non-normative source-pinned historical reader snapshot. The original path remains authoritative and unchanged for its consumers. No automatic synchronization or independent execution. Accepted proposals, completed checkboxes and remaining work retain their source-time status, not a newly checked current implementation status. The October 21 proposal and (F) checks are the original author’s dated work, not new source verification by this translation. The source’s LLM-choice, monotonicity and duplicate-check claims do not constitute independent guarantees. Its zero UUID example does not separately explain the all-one value. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-uuid.md)

> Original metadata summary (inactive reader note): Stdlib redesign L2/L3 — uuid (new, R-L1-07): v4 and v7 generation as effect fns over wasi:random and wasi:clocks, pure parse/to_string/bytes conversions, and compare because v7 is ordered.

> Last updated: 2026-10-21
> Prerequisites: [L0](../../../../../roadmap/stdlib-redesign/L0-conventions.md) and [L1](../../../../../roadmap/stdlib-redesign/L1-categories.md) (R-L1-07: `uuid.v4()`, `uuid.v7()`, `uuid.parse`, `uuid.to_string`)
> Sources: Rust https://docs.rs/uuid (not in the standard library; a reference), Go https://pkg.go.dev/uuid, Python https://docs.python.org/3/library/uuid.html, TS/JS https://developer.mozilla.org/en-US/docs/Web/API/Crypto/randomUUID, Gleam — (absent), Elixir — (absent), OCaml — (absent), Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/kotlin.uuid/-uuid/, Swift https://developer.apple.com/documentation/foundation/uuid
> (F) = a cell fetched and checked in this investigation.

<a id="l2-役割の比較"></a>

## L2: Comparison of roles

| Role | Almide (proposal) | Rust(uuid crate) | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| Random v4 | `v4` | `Uuid::new_v4` | `NewV4`(F) | `uuid4`(F) | `crypto.randomUUID` | — | — | — | `Uuid.random` | `UUID()` |
| Time-ordered v7 | `v7` | `Uuid::now_v7` | `NewV7`(F) | `uuid7` (3.14 onward)(F) | — | — | — | — | — | — |
| From string | `parse` | `parse_str` | `Parse`(F) | `UUID(s)`(F) | — | — | — | — | `Uuid.parse` | `UUID(uuidString:)` |
| To string | `to_string` | `to_string` | `String`(F) | `str(u)` | (returns a string) | — | — | — | `toString` | `uuidString` |
| From/to 16 bytes | `from_bytes` / `to_bytes` | `from_bytes` / `as_bytes` | `[16]byte` | `UUID(bytes=)` / `.bytes`(F) | — | — | — | — | `fromByteArray` / `toByteArray` | `uuid` |
| Compare | `compare` | `Ord` | `Compare`(F) | `<` | — | — | — | — | `compareTo` | — |
| Inspect version | — | `get_version` | — | `.version`(F) | — | — | — | — | — | — |
| All 0 / all 1 | — | `nil` / `max` | `Nil` / `Max`(F) | `NIL` / `MAX` | — | — | — | — | `NIL` | — |

<a id="完全性の規則"></a>

### Completeness rules

- Creation (effect): 2 functions, {`v4`, `v7`}. Do not include v1, v3, v5, v6, or v8 (the majority of the 9 languages have v4; v7 is added by the L1 ruling. Namespaced v3/v5 use MD5 and SHA-1, so users can write them on top of `hash` if needed).
- Representation conversions (pure): 4 cells, {string, 16 bytes} × {from, to}. `parse` and `from_bytes` have reason-bearing failures, so return `Result[Uuid, String]` (R-L0-06 item 4: do not return a sentinel value marking failure).
- Ordering: `Uuid` sorts lexicographically over its 16 bytes, most significant first. For v7, this gives time order. Per R-L0-16, provide `uuid.compare(a, b) -> Int`.
- Spelling: use `parse` to construct from a string, following the L1 ruling and `int.parse` / `url.parse`. Use `from_bytes` to construct from bytes, following R-L0-07 item 2.
- Intentionally omitted: a version-inspection function, `nil`, and `max`. It can be expressed through string `parse("00000000-0000-0000-0000-000000000000")`, and there is no majority of 5 or more languages either.

<a id="l2-の判定"></a>

### L2 ruling

- This is a new module.

<a id="案"></a>

## Proposal

<a id="r-uuid-01-uuid-モジュールを足す---採用"></a>

### R-uuid-01 Add the `uuid` module — accepted

- Content: the 7 functions in L3 below and an opaque `Uuid` type (internally 16 bytes).
  - `v4` combines 122 random bits with 4 version bits (`4`) and 2 variant bits (`10`).
  - `v7` places the millisecond Unix time in the first 48 bits and a counter within the same millisecond in the following 12 bits (RFC 9562 §6.2 method 1), producing a strictly increasing sequence within one process. Python's `uuid7` also guarantees monotonicity through counting (F).
  - `to_string` uses lowercase 8-4-4-4-12. `parse` accepts that form, uppercase, and the `{…}` and `urn:uuid:` forms (the same range as Python (F)).
- Supporting L0 rules: R-L0-06 (item 4), R-L0-07, R-L0-16 (`compare`), R-L0-15.
- (a) 5 languages have it (L1).
- (b) `v4` uses `get-random-bytes` from `wasi:random/random` (written in Almide on top of `random.bytes(n)`, added by R-L1-04). `v7` additionally uses `wasi:clocks/wall-clock` (`now_ms`, moved to `datetime` by R-L1-25). Both are effect fns. The other 5 functions are pure computations. No new host op is needed. Inspect with ADR-0027 D3 verdicts: for `v4`, the version and variant bits and no duplicates in m generations; for `v7`, the time lies within `[t₀, t₁]`, and consecutively generated values increase under `compare`.
- (c) LLMs write forms such as `uuid4()` and `randomUUID()` for “assign an ID” tasks. `v4` and `v7` are the version names themselves, so the choice is not confused.
- (d) Addition (new module; requires `import uuid`, handled like `json` and `random`).
- Proposed gate: property tests for `parse(to_string(u)) == ok(u)` and `from_bytes(to_bytes(u)) == ok(u)`, a v7 monotonicity fixture, and a cross fixture whose verdict rows agree on native and wasm.
- issue: #3934

<a id="l3-関数表"></a>

## L3: Function table

| Function | Signature (proposal) | Main spellings in other languages | Ruling | Proposal |
|---|---|---|---|---|
| `uuid.v4` | `effect fn v4() -> Uuid` | Go `NewV4`(F) / Python `uuid4`(F) / JS `randomUUID` / Kotlin `Uuid.random` | Add | R-uuid-01 |
| `uuid.v7` | `effect fn v7() -> Uuid` | Go `NewV7`(F) / Python `uuid7`(F) / Rust `now_v7` | Add | R-uuid-01 |
| `uuid.parse` | `fn parse(s: String) -> Result[Uuid, String]` | Go `Parse`(F) / Kotlin `Uuid.parse` / Python `UUID(s)`(F) | Add | R-uuid-01 |
| `uuid.to_string` | `fn to_string(u: Uuid) -> String` | Go `String`(F) / Kotlin `toString` / Swift `uuidString` | Add | R-uuid-01 |
| `uuid.from_bytes` | `fn from_bytes(b: Bytes) -> Result[Uuid, String]` | Rust `from_bytes` / Kotlin `fromByteArray` / Python `UUID(bytes=)`(F) | Add | R-uuid-01 |
| `uuid.to_bytes` | `fn to_bytes(u: Uuid) -> Bytes` | Rust `as_bytes` / Kotlin `toByteArray` / Python `.bytes`(F) | Add | R-uuid-01 |
| `uuid.compare` | `fn compare(a: Uuid, b: Uuid) -> Int` | Go `Compare`(F) / Kotlin `compareTo` | Add | R-uuid-01 |
