> Description (inactive source index metadata): Stdlib redesign layer 1 — the module list against nine languages: categories to add, to keep apart, to move, and the ownership map for environment, time and processes
<a id="l1-カテゴリモジュールの一覧"></a>

# L1: Category (module) list

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](L1-categories.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L1-categories.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](L1-categories.zh-CN.md)

> Non-normative historical redesign snapshot; pinned original authoritative and unchanged. No automatic synchronization, current API/vendor verification or independent measurements. [F] denotes the original author’s source checks, not this translation pass. Preserve dated accepted/rejected/planned decisions, original counts, and the later 2026-10-24 owner exception to the initial three-condition rule. The source’s cross-target, fusion, secret-display/constant-time and SafeHtml claims are not independent or universal security/performance guarantees. Its initial CRC32 placement summary and later Python exception remain unreconciled. In preserved pseudocode, 型 means type. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L1-categories.md)

> Last updated: 2026-10-21
> Prerequisite: [L0](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md)
> Evidence table: [facts/categories.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/facts/categories.md) (about 100 categories × 9 languages; ● = standard, ◐ = conditionally standard, pkg = official package only, [F] = cells verified in official documentation in this pass)

<a id="判定の基準"></a>

## Decision criteria

**Add a category** only when all 3 conditions hold:
1. At least 5 of the 9 languages have it in their standard libraries (total of ● and ◐).
2. It can be pure computation or run on wasm through a standard WASI interface (R-L0-15).
3. Current functions and existing rulings cannot cover it.

**Split, combine or move categories** only when they violate L0 rules (R-L0-01: one spelling for one role; R-L0-02: use the subject type's module). Do not move them merely because their grouping differs from the majority.

---

<a id="1-いまの-44-モジュールと-9-言語"></a>

## 1. The current 44 modules and 9 languages

| Category | Almide | Treatment in 9 languages (summary) | Verdict |
|---|---|---|---|
| Numbers | `int` `float` `math` | 9/9 have it | Unchanged (contents in L2) |
| Fixed-width numbers | `int8`〜`uint64`, `float32` `float64` | 0 languages have per-type modules. Rust/Kotlin/Swift use type methods; Go uses purpose-specific packages | R-L1-20 |
| Strings | `string` `regex` | 9/9 | Unchanged |
| Collections | `list` `map` `set` | 9/9 | Unchanged |
| Option / Result | `option` `result` `error` | Rust/Gleam/OCaml/Swift/Kotlin have them | `option` and `result` remain unchanged. `error` has 3 duplicate or forwarding functions; remove them, leaving an empty module ([L2-error](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-error.md)) |
| Bytes | `bytes` | 9/9 | Contents in L2. Raw pointers: R-L1-26 |
| Encoding | `base64` `hex` `url` `html` | 5 languages combine base64 and hex | R-L1-23 |
| Data formats | `json` `value` | JSON: 8/9. Only Gleam dynamic provides format-independent dynamic values as standard | R-L1-27 |
| Hashing / compression | `hash` `zlib` | Non-cryptographic hashing: 6+2; Go/Python/Erlang/JVM place CRC32 on the hash side | R-L1-05 |
| Time | `datetime` `duration` `compute` | None of the 9 languages has a compute equivalent | R-L1-21 |
| OS / processes | `env` `process` `args` `fs` `path` `io` | The majority put argv and environment variables in one module | R-L1-25 |
| Networking | `net` `http` | TCP: 9/9; UDP/DNS/IP address types: 7 | R-L1-06, R-L1-26 |
| Randomness | `random` | Seeded randomness: 6; cryptographic randomness (CSPRNG): 6 | R-L1-04 |
| Matrices | `matrix` | 0 languages have it as standard | R-L1-22 |
| Testing | `testing` | 5+1 | Unchanged |

---

<a id="2-足すカテゴリ"></a>

## 2. Categories to add

<a id="r-l1-01-両端キュー-deque---採用"></a>

### R-L1-01 Double-ended queue `deque` — ○ Accepted

- (a) 7 languages have it in their standard libraries (Rust `VecDeque`, Python `collections.deque`, Kotlin `ArrayDeque`, OCaml `Queue`, etc.). Removing from the front in current Almide costs O(n) each time via `list.drop(xs, 1)`.
- (b) Pure computation. No host functionality needed.
- (c) For algorithm problems such as breadth-first search, LLMs write assuming another language's deque. Without one, they must write a custom queue each time, increasing mistakes.
- (d) Addition (new module).
- Functions (following L0 verbs): `deque.new`, `from_list`, `to_list`, `len`, `is_empty`, `push`, `push_front`, `pop`, `pop_front`, `first`, `last`. Like `list.push` / `list.pop`, `push_*` / `pop_*` mutate in place with `mut` arguments (R-L0-04 item 3: one verb has the same style across all modules). Details in [L2-deque](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-deque.md).
- Gate: use the same boundary-input matrix tests as list. Add fixtures verifying equal results on native and wasm.
- issue: #3932

<a id="r-l1-02-優先度キュー-heap---採用"></a>

### R-L1-02 Priority queue `heap` — ○ Accepted

- (a) 6 languages have it (Rust `BinaryHeap`, Go `container/heap`, Python `heapq`, OCaml `Pqueue`, etc.).
- (b) Pure computation.
- (c) A standard tool for frequent problems such as Dijkstra's algorithm and top k items.
- (d) Addition. Default to a min-heap (like Python, Go, OCaml). The key-selecting version is `heap.new_by(f)` (R-L0-10).
- issue: #3933

<a id="r-l1-03-任意精度整数-bigint---採用"></a>

### R-L1-03 Arbitrary-precision integer `bigint` — ○ Accepted

- (a) 6 languages have it (Go `math/big`, Python `int`, JS `BigInt`, Elixir integers, JVM `BigInteger`).
- (b) Pure computation. Add results that do not differ by even 1 byte between native and wasm to the ADR-0016 numeric-determinism commitment.
- (c) Problems involving factorials and large Fibonacci numbers currently silently produce wrong answers as `Int` wraps.
- (d) Addition. Use ADR-0005 desugaring for operators (`+` becomes `bigint.add`).
- issue: #3931

<a id="r-l1-04-random-にシード付き乱数と暗号用の乱数を足す---採用"></a>

### R-L1-04 Add seeded and cryptographic randomness to `random` — ○ Accepted

- (a) 6 languages have seeded randomness, and 6 have CSPRNGs (cryptographic randomness). Current `random` has 4 functions with no seed input.
- (b) Seeded randomness is pure computation (write an algorithm such as PCG in Almide; native and wasm produce the same sequence). Cryptographic randomness runs on wasm through `wasi:random`'s `get-random-bytes`.
- (c) When reproducible randomness is needed in tests, LLMs write APIs shaped like `Random(seed)`.
- (d) Addition. Add copying versions such as `random.seeded(seed) -> Rng`, `random.next_int(rng, lo, hi) -> (Int, Rng)`, plus `random.bytes(n)` (effect).
- issue: #3939

<a id="r-l1-05-hash-を鍵を使わない要約の置き場にするcrc32adler-32-を-zlib-から移しmd5sha-1sha-512-を足す---採用"></a>

### R-L1-05 Make `hash` the home of unkeyed digests; move CRC32 / Adler-32 from zlib and add MD5 / SHA-1 / SHA-512 — ○ Accepted

- (a) 7 languages have MD5/SHA-1/SHA-512/SHA-3. CRC32 exists in Go (`hash/crc32`), Python (`zlib.crc32`), Erlang and JVM, but placement is split. Almide gathers digest-returning functions in `hash` (R-L0-02: the same result kind belongs in the same module).
- (b) Pure computation.
- (c) “CRC32 is in zlib” does not work outside Python. In `hash`, it is discoverable by name.
- (d) Add `hash.crc32` and `hash.adler32`, remove `zlib.crc32` and `zlib.adler32` (use the R-L0-14 window; details in [L2-hash](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-hash.md) R-hash-02 and [L2-zlib](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-zlib.md) R-zlib-01). Add `hash.md5`, `sha1`, `sha512` and each `_hex` variant. Keyed operations (HMAC, AEAD, signatures) belong in #2596's `crypto`.
- issue: #3914

<a id="r-l1-06-net-に-udpdnsip-アドレスを足す---採用"></a>

### R-L1-06 Add UDP / DNS / IP addresses to `net` — ○ Accepted

- (a) 7 languages have them.
- (b) Run on wasm using `wasi:sockets`'s `udp` and `ip-name-lookup`. Use the ADR-0023 path (p3 shim), as for TCP. Do not add new host ops.
- (d) Addition.
- issue: #3930

<a id="r-l1-07-uuid---採用"></a>

### R-L1-07 `uuid` — ○ Accepted

- (a) 5 languages have it (Go 1.27's `uuid`, Python, JS `crypto.randomUUID`, Kotlin, Swift).
- (b) v4 uses randomness (`wasi:random`); v7 uses clocks and randomness (`wasi:clocks` and `wasi:random`). Both are effectful. `uuid.parse` and `uuid.to_string` are pure computation.
- (d) Addition (`uuid.v4()`, `uuid.v7()`, `uuid.parse`, `uuid.to_string`).
- issue: #3934

<a id="r-l1-08-utf-16latin-1-との変換---採用新しいモジュールにはしない"></a>

### R-L1-08 Conversion to/from UTF-16 / Latin-1 — ○ Accepted (not a new module)

- (a) 8 languages have it.
- (b) Pure computation.
- (d) Add string functions such as `string.to_utf16`, `string.from_utf16` (decide in L2-string). Do not create a new module. Under R-L0-07, constructing from a container belongs in the destination type's module.
- issue: file under the L2-string proposal.

---
- issue: #3948

<a id="r-l1-30-bool-モジュールを置きboolparse-を足す---採用"></a>

### R-L1-30 Create a `bool` module and add `bool.parse` — ○ Accepted

- Now: there is `int.parse`, `float.parse`, and `parse` for fixed-width types, but no function reading `Bool` from a string. Reading configuration with `s == "true"` silently turns typos such as `"ture"` into `false`.
- Proposal: create `bool` (usable without import like `int` / `float`), and add `fn bool.parse(s: String) -> Result[Bool, String]`. Accept only `"true"` and `"false"` (leading/trailing whitespace or uppercase yields err). Contents in [L2-bool](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-bool.md).
- Rationale: L1 addition criterion (at least 5 languages). R-L0-25 (conversion from string uses the destination type's `parse`).
- (a) Rust `"true".parse::<bool>()`, Go `strconv.ParseBool`, OCaml `bool_of_string_opt`, Kotlin `toBooleanStrictOrNull`, Swift `Bool("true")`. All reject invalid input.
- issue: #3980

<a id="r-l1-31-秘密の値を包む-secret---採用"></a>

### R-L1-31 `secret` wraps secret values — ○ Accepted

- Now: records holding passwords or API keys print all fields unchanged in interpolation, `assert_eq` failure output and `almide test` reports (#4107). Attributes such as `@redact` on fields are not read and silently protect nothing.
- Proposal: create `secret` (not automatically imported; `import secret`), and add type `Secret[A]`, `secret.new` / `secret.reveal`. Every display path emits `Secret(***)`; `==` uses constant-time comparison only for String / Bytes, and Codec derives decode only. Contents in [L2-secret](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-secret.md).
- Rationale: only 2/8 have dedicated types (Rust secrecy, pydantic), below the 5-language threshold. Adopt it nevertheless because only a type can derive all display paths from 1 declaration (a field attribute protects only 1 path of 1 record). Owner ruling 2026-10-24.
- issue: #4107

<a id="3-足さないカテゴリ-と理由"></a>

## 3. Categories not added (× and reasons)

| Category | Languages providing it | Verdict | Reason |
|---|---:|---|---|
| Lazy iterators | 8 | × | `|>` chains become 1 loop through fusion (ruling: “implicit `|>` is fusion”). Adding a separate lazy type gives the same computation 2 spellings (R-L0-01). |
| String builder | 8 | × | `string.push(mut s, x)` already exists for appending in place. |
| printf-style formatting | 8 | × | String interpolation `"${x}"` and `float.to_fixed`, `string.pad_start` can express it. Adding another formatting mini-language invites LLM mistakes. |
| Locks / atomics / threads / channels / async / cancellation | 5〜8 | × | Existing policy limits concurrency to `fan`'s deterministic data parallelism (concurrency-stance). Cancellation and deadlines are handled by `fan.timeout` and `fan.min(duration)` (#3788). |
| Binary serialization | 7 | × | `bytes` already has a read/write function family (`append_<型>`, `read_<型>`). |
| Time zones (IANA) | 6 | × | Keep it outside the standard library, in a package. The tz database is several hundred KB; wasm must read it from the host or embed it. Keep standard `datetime` deterministic, handling only UTC and fixed offsets. Recommend a package bundling tz data (e.g. `almide/tz`) as the location. |
| Sorted maps | 5 | × | `map` is deterministic in insertion order. For ordering, write `map.entries(m) |> list.sort_by(...)`. Adding a second map type adds a choice of which to use. |
| XML | 6 | × | Use a package. JSON is the only standard data format. |
| Cryptography (AES, signatures, TLS) | 6 | ○(existing) | Proceed in #2596 as pure-Almide `crypto` (AEAD, Ed25519, HMAC). TLS is inside `http`. |
| Logging | 5 | × | `eprintln` writes to standard error. Severity routing is an application concern. |
| Terminal / ANSI | 5 | × | Escape sequences can be strings. Use a package. |
| tar / zip, bzip2 / zstd | 5 | × | Use packages. The standard library has only `zlib` (deflate/gzip). |
| Full-fledged flag parsing | 5 | × | `args` remains a minimal reader. Generating help text and similar features are application concerns. |
| Timers | 5 | × | Express with `fan.timeout` and `env.sleep_ms`. |

---

<a id="4-分け方置き場所"></a>

## 4. Grouping and placement

<a id="r-l1-20-固定幅の数のモジュールint8uint64float32float64---統合しない"></a>

### R-L1-20 Fixed-width numeric modules (`int8`–`uint64`, `float32`, `float64`) — × Do not consolidate

- (a) 0 languages have a module for each type, but the majority attach methods to types. Almide has no methods (only UFCS), so applying R-L0-02 (use the subject type's module) yields per-type modules.
- (c) In `x |> int8.to_int16`, the type at the pipeline's start determines the module.
- Internal inconsistencies (`int.to_u32` versus `int.to_uint32`, missing `to_int` from sized int to Int) are decided in L2 (R-L0-07).

<a id="r-l1-21-compute-と-duration---統合しないadr-0001"></a>

### R-L1-21 `compute` and `duration` — × Do not consolidate (ADR-0001)

ADR-0001 defines deterministic computation time (`Compute`) and real time (`Duration`) as distinct, mutually non-convertible types. None of the 9 languages has an equivalent of `compute`. Still, Almide's own reason—its determinism commitment—means not following the majority.

<a id="r-l1-22-matrix-を標準に残し3852-の切り出しを終える---採用"></a>

### R-L1-22 Keep `matrix` standard and finish the #3852 extraction — ○ Accepted

- (a) 0 languages have matrices in their standard libraries.
- Reason to retain: compiler rewrite rules (`@rewrite`, fusion via egg) target `matrix` operations (`crates/almide-codegen/src/target.rs`). Packages cannot currently bring rewrite rules.
- With that in mind, proceed with #3852 to remove inference kernels and fusion-target functions from the table. Keep only general-purpose dense matrix operations.
- issue: #3852 (existing)

<a id="r-l1-23-hex-と-base64-を別のモジュールのまま残す---まとめない"></a>

### R-L1-23 Keep `hex` and `base64` separate — × Do not combine

5 languages group them together, but their combined module names differ (Elixir `Base`, Deno `@std/encoding`, Python `b16encode` inside `base64`). Go separates `encoding/hex` and `encoding/base64`. Under R-L0-00, do not move them without a majority spelling.

<a id="r-l1-25-os-とプロセスまわりの持ち分を-1-つに決める---採用"></a>

### R-L1-25 Assign one owner to OS and process concerns — ○ Accepted

| Responsibility | Module | Duplicates to remove |
|---|---|---|
| argv, environment variables, working directory, OS name | `env` | `args.raw`(= `env.args`), `process.args`(→ `env.args`), `process.env`(a hole allowing a pure fn to read the environment) |
| Clocks (real-time / monotonic) | `datetime` | `env.unix_timestamp`(= `datetime.now`), move `env.millis` to `datetime.now_ms` |
| Waiting | `env.sleep_ms` | `process.sleep`(only negative-value handling differs) |
| Temporary directories / files | `fs` | `env.temp_dir`(= `fs.temp_dir`) |
| Standard input | `io` | `process.stdin_lines`(→ `string.lines(io.read_all()!)`) |
| Child processes, self-exit, pid | `process` | — |
| Flag parsing | `args` (built on `env.args`) | — |

- (a) argv and environment variables share a module in Rust `std::env`, Go `os`, Kotlin `System`, and others.
- (c) If 2 functions return the same value but differ at the edges, whichever one an LLM chooses, half its answers fail.
- (d) Removals, following R-L0-14.
- issue: #3851 B (existing). Add the move of `env.millis` as new work.

<a id="r-l1-26-生ポインタを-ffi-にunix-ソケットと共有メモリを-unix-に移す---採用"></a>

### R-L1-26 Move raw pointers to `ffi`, Unix sockets and shared memory to `unix` — ○ Accepted

Follow #3853. Since `bytes` is available in all files without import, do not put memory-unsafe functions there. Limit `net` to what runs on wasm through `wasi:sockets`. Move `unix.*` with the already-decided provisional host ops (104–116) unchanged.
- issue: #3853 (existing)

<a id="r-l1-27-json-はテキストとの変換value-は動的な値の木---採用"></a>

### R-L1-27 `json` converts text; `value` is a dynamic value tree — ○ Accepted

Assign ownership: conversion to/from text (`parse`, `stringify`) belongs in `json`; constructing and extracting trees belongs in `value`. Remove `value.stringify` as a duplicate of `json.stringify` (R-L0-01). Decide details in L2.
- issue: #3953

<a id="r-l1-28-url-の符号化は-url-に置く---採用"></a>

### R-L1-28 Put URL encoding in `url` — ○ Accepted

`http.url_decode` (form decoding that converts `+` to spaces) has the role of URL encoding, so move it to the new `url.decode_form` in `url` and remove it (R-L0-01, R-L0-02). Rewrite as `url.decode_form(s) ?? s`. Decide details in [L2-url](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-url.md) R-url-01 and [L2-http](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-http.md) R-http-04.
- issue: #3950

<a id="r-l1-29-html-と-testing---動かさない"></a>

### R-L1-29 `html` and `testing` — × Do not move

`html` has 5 functions. Its design of representing escaped HTML as a distinct type (SafeHtml) follows the same idea as Go's `html/template`, preventing mix-ups by type. `testing` is standard in 5 languages.

---

<a id="5-予定されているもの参考"></a>

## 5. Planned items (reference)

- `kv` (key/value storage, #2699) is underway through the ADR-0020 process. Not covered here.
- `crypto` (#2596) pairs with R-L1-05. Unkeyed digests belong in `hash`, keyed operations in `crypto`.
