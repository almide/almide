> Description (fixed history): Stdlib redesign — the standard library as it stands once every adopted proposal lands: every public function with its post-redesign signature, per module, and what was removed
<a id="リデザイン後の標準ライブラリ設計"></a>

# Standard library after redesign (design)

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](target-surface.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](target-surface.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](target-surface.zh-CN.md)

> Non-normative historical design-target snapshot, not a current implementation inventory. The pinned canonical source is authoritative and unchanged; its L2/L3 records take precedence as stated below. No automatic synchronization or independent execution, measurement or renewed official-source retrieval. Source figures (1001/1300 and other counts), (F), compatibility and numerical/determinism claims remain historical author statements. Preserve dated migrations, deferred zlib output equality, deferred map callback design, changed decisions, and stale or conflicting signatures instead of silently reconciling them. R-result-06’s reason is truncated in the original and remains so here. This reader is not input to the original table generator, nor a canonical generated table or normative contract source. Japanese literal placeholders denote module/name/arguments/return type, numeric type/integer, or value/next state. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/target-surface.md)

> Last updated: 2026-10-24
> This list is a historical design target, not evidence that every item is implemented. See the [README guidance](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/README.md#この記録と現行実装の区別) for distinctions from current APIs, migration and unfinished items.
>
> Original records: [README.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/README.md) (proposal index), and the L3 function tables on each L2 page.

This lists the standard library’s public functions after applying every accepted proposal (○). It is mechanically assembled from the L3 function tables on each L2 page. If it disagrees with those tables, the L3 tables and L2 proposals are authoritative.

- On-hold proposals (matching zlib outputs, the shape of `map.map` callbacks, etc.) are not applied; their current forms remain. Rejected and on-hold proposals, with reasons, appear in the final table.
- Signatures use `fn モジュール.名前(引数) -> 戻り型` (module.name(arguments) → return type). `mut` marks an in-place-mutated argument; `T?` is `Option[T]` (canonical R-L0-24 spelling). As in docs/stdlib, `effect fn` return types show the success type (failure is a `String` err, propagated by callers with `!`). Non-effect functions returning failure use `Result[T, String]`.
- “Change” column: blank means unchanged; new means added; renames include old names; semantic changes say what changes. Removed functions appear at each module’s end with migration destinations.
- All migrations follow R-L0-14 (`@deprecated` at epoch 15, removal at epoch 16, migration via `almide fix` or hints).

<a id="一覧に出ない共通の約束"></a>

## Shared conventions not shown in the list

These conventions apply across modules but cannot be read from signatures alone. Their basis is in the individual pages and contract ledger (docs/contracts).

| Convention | Content | Basis |
| --- | --- | --- |
| String positions | Positions and lengths in `string` (`len`/`get`/`slice`/`index_of`) and `regex.index_of` all count code points. Only functions prefixed `byte_` (`byte_len`/`byte_slice`) use bytes. | L2-string、L2-regex |
| Order of `Float` | `float.compare` (also `float32`/`float64`) and the default ordering of `list.sort`/`binary_search`/`heap` use IEEE 754 totalOrder (`-0.0 < 0.0`). NaNs are canonicalized to one value per type width (`Float`/`Float64`: `0x7ff8000000000000`; `Float32`: `0x7fc00000`), so NaN is strictly greater than positive infinity and NaNs compare equal to one another. In exchange, NaN bits do not round-trip. All NaNs also become this value through `to_bits`/`from_bits` and `bytes`'s `get_f32_le`/`get_f64_le`/`append_f32_le`/`append_f64_le`, etc. To round-trip NaN payloads, read/write them as integers using `get_u32_le`/`get_u64_le`. Only the `<` operator compares false against NaN. | R-float-09、R-float-10 |
| Iteration order of `Map`/`Set` | Insertion order (overwriting an existing key does not move it). Identical on native and wasm. | Contract ledger (Map/Set insertion order) |
| Mathematical function values | `math`'s transcendental functions run the same Almide implementation on both targets rather than delegating to libm, returning bit-identical values. NaN has one fixed value. | ADR-0016、L2-math |
| `_checked` conversions | Returns some only when the result equals the original value exactly. Out-of-range values, discarded fractional parts and precision loss all return none (both `float.to_int32_checked(1.5)` and `int.to_float32_checked(16777217)` are none). | R-L0-28 |
| none from `next_` | When the value is none, return the received state (read position/random-number state) unchanged. | R-L0-27 |
| String body | `http.body` reads invalid UTF-8 as replacement characters (requests and responses alike). Use `body_bytes` for exact bytes. | R-http-11、R-http-12 |
| Direction of `heap` | Default (`heap.new`/`from_list`) is smallest first. For largest first, pass an ordering to `new_with` or `new_by`. `heap.to_list` follows repeated `pop` order; equal priorities retain insertion order. | L2-heap、R-L1-02 |
| `Matrix` operators | `+`/`-`/`/`/unary `-` are elementwise; `*` is matrix multiplication (use `matrix.zip_with` for elementwise multiplication). Mismatched shapes abort. | R-matrix-14、L2-matrix |
| `min_value` | Integer `min_value` is the smallest value. Floating-point `min_value` is the smallest finite value (`-max_value()`), like Rust `MIN`, not Java/Kotlin's smallest positive value. | L2-float completeness rule 2 |
| Integer boundaries | `int.bshl`/`bshr` reduce shift counts modulo 64 (`bshl(1, 64)` is 1). `int.abs(int.min_value())` remains `min_value` (wrapping). Unsuffixed conversion `to_<型>`, when targeting an integer, aborts on an unrepresentable value (out of range; also NaN for floating inputs). Integer and floating inputs follow the same rule. Fractional parts truncate toward zero (`float.to_int(1.5)` is 1). Floating destinations do not abort, but round to the nearest value (out of range becomes infinity; NaN remains NaN). Use `_saturating` to clamp to endpoints, `_wrapping` to preserve low bits and wrap between integers, or `_checked` to return none unless exactly representable. `int.wrap(x, bits)` computes within `Int`, not a type conversion. | R-L0-32、R-L0-34、R-L0-35 |
| `map.update` | Absent keys abort (like an out-of-range `list.update`). Use `upsert` to create missing entries, or check `map.contains` when presence is unknown. | R-map-08 |
| Newlines | `io.print`/`io.eprint` do not append a newline. Built-ins `println`/`eprintln` do. | L2-io |
| Protocol hierarchy | Standard types conform to the protocol hierarchy ([protocol-view.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/protocol-view.md), modeled on Swift); module functions are those protocol functions themselves. Required functions have identical names and shapes on every conforming type. Default implementations give types with `compare` the functions `min`/`max`/`clamp`, and every value type `to_string` (this list includes only rows found on the type pages). | R-L0-37、R-L0-38 |
| Out-of-range behavior | Determined by function role. Range extraction (`slice`/`take`/`drop`, etc.) clamps excess length; negative positions/counts abort. Reading one position (`get`/`first`/`bytes.get_*`/`next_*`) yields none. Writing one position (`set`/`update`/`insert`/`remove_at`/`bytes.put_*`) and shape-dependent functions (`matrix` operations, `bytes.xor`, `chunk(0)`) abort. | R-L0-33、R-L0-29 |
| `zlib` compression | Compressed output bytes differ between native and wasm. This remains an unfulfilled part of the same-results-on-both-targets promise; `effect` is only a marker, not a solution. Decide after measuring speed and compression ratio with a shared implementation. Round trips (compress then decompress) are identical on both targets. | R-zlib-03 (on hold, unmet) |
| Codec input | Codec record type `T` derives `T.decode_strict` in addition to `T.decode`. `decode` ignores unknown keys; `decode_strict` rejects the first unknown key with `unknown key '<k>'` before reading fields. `@skip` fields are neither read nor written; `@skip_decode` fields are written but not read (both require defaults). Only these two attributes and `@builtin`/`@location` may annotate fields; others yield E053. | R-L0-48 |

<a id="全体の数"></a>

## Overall counts

| Current functions | After redesign | Unchanged | New | Renamed | Semantic change | Removed |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 1001 | 1300 | 408 | 716 | 88 | 88 | 417 |

Sized numbers (`int8`–`uint64`, `float32`/`float64`) count as one function per type.

<a id="実体の数"></a>

## Counts of distinct entities

This list counts the same protocol function once per type (1,300 rows total). Recounting by the [protocol hierarchy](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/protocol-view.md) greatly reduces the names to learn.

| Counting method | List rows | Names to learn |
| --- | ---: | ---: |
| Protocol requirements (once per protocol) | 796 | 166 |
| Functions exclusive to value types | 297 | 297 |
| Modules without a value type (`fs`/`http`/`math`, etc.) | 207 | 207 |
| Total | 1300 | 670 |

<a id="モジュールごとの数"></a>

## Counts by module

| Module | After redesign | New | Renamed | Semantic change | Removed | Record |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| [`args`](#args) | 3 | 1 | 0 | 0 | 4 | [L2-args.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-args.md) |
| [`base64`](#base64) | 4 | 0 | 0 | 2 | 0 | [L2-base64.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-base64.md) |
| [`bigint`](#bigint) (new module) | 24 | 24 | 0 | 0 | 0 | [L2-bigint.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-bigint.md) |
| [`bool`](#bool) | 3 | 3 | 0 | 0 | 0 | [L2-bool.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-bool.md) |
| [`bytes`](#bytes) | 178 | 64 | 72 | 6 | 34 | [L2-bytes.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-bytes.md) |
| [`compute`](#compute) | 7 | 1 | 0 | 0 | 0 | [L2-compute.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-compute.md) |
| [`datetime`](#datetime) | 18 | 1 | 0 | 1 | 5 | [L2-datetime.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-datetime.md) |
| [`deque`](#deque) (new module) | 60 | 60 | 0 | 0 | 0 | [L2-deque.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-deque.md) |
| [`duration`](#duration) | 7 | 1 | 0 | 0 | 0 | [L2-duration.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-duration.md) |
| [`env`](#env) | 9 | 3 | 0 | 0 | 3 | [L2-env.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-env.md) |
| `error` (entire module removed) | 0 | 0 | 0 | 0 | 3 | [L2-error.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-error.md) |
| [`ffi`](#ffi) (new module) | 7 | 7 | 0 | 0 | 0 | [L2-ffi.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-ffi.md) |
| [`float`](#float) | 26 | 12 | 1 | 0 | 32 | [L2-float.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-float.md) |
| [`float32`](#float32) | 24 | 23 | 0 | 0 | 9 | [L2-sized-numbers.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-sized-numbers.md) |
| [`float64`](#float64) | 24 | 23 | 0 | 0 | 9 | [L2-sized-numbers.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-sized-numbers.md) |
| [`fs`](#fs) | 30 | 1 | 2 | 4 | 5 | [L2-fs.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-fs.md) |
| [`hash`](#hash) | 11 | 8 | 0 | 2 | 1 | [L2-hash.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-hash.md) |
| [`heap`](#heap) (new module) | 16 | 16 | 0 | 0 | 0 | [L2-heap.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-heap.md) |
| [`hex`](#hex) | 3 | 0 | 0 | 0 | 0 | [L2-hex.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-hex.md) |
| [`html`](#html) | 5 | 0 | 0 | 0 | 0 | [L2-html.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-html.md) |
| [`http`](#http) | 34 | 4 | 5 | 10 | 18 | [L2-http.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-http.md) |
| [`int`](#int) | 45 | 14 | 2 | 4 | 41 | [L2-int.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-int.md) |
| [`int16`](#int16) | 37 | 34 | 0 | 0 | 19 | [L2-sized-numbers.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-sized-numbers.md) |
| [`int32`](#int32) | 37 | 34 | 0 | 0 | 21 | [L2-sized-numbers.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-sized-numbers.md) |
| [`int64`](#int64) | 37 | 34 | 0 | 0 | 23 | [L2-sized-numbers.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-sized-numbers.md) |
| [`int8`](#int8) | 37 | 34 | 0 | 0 | 17 | [L2-sized-numbers.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-sized-numbers.md) |
| [`io`](#io) | 12 | 8 | 0 | 2 | 4 | [L2-io.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-io.md) |
| [`json`](#json) | 3 | 0 | 0 | 1 | 12 | [L2-json.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-json.md) |
| [`list`](#list) | 73 | 12 | 0 | 16 | 6 | [L2-list.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-list.md) |
| [`map`](#map) | 37 | 15 | 1 | 7 | 3 | [L2-map.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-map.md) |
| [`math`](#math) | 27 | 12 | 0 | 0 | 8 | [L2-math.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-math.md) |
| [`matrix`](#matrix) | 25 | 3 | 0 | 8 | 24 | [L2-matrix.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-matrix.md) |
| [`net`](#net) | 16 | 8 | 0 | 8 | 18 | [L2-net.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-net.md) |
| [`option`](#option) | 13 | 0 | 0 | 0 | 1 | [L2-option.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-option.md) |
| [`path`](#path) | 10 | 3 | 0 | 0 | 3 | [L2-path.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-path.md) |
| [`process`](#process) | 13 | 8 | 0 | 2 | 12 | [L2-process.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-process.md) |
| [`random`](#random) | 10 | 6 | 0 | 0 | 0 | [L2-random.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-random.md) |
| [`regex`](#regex) | 15 | 7 | 1 | 7 | 0 | [L2-regex.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-regex.md) |
| [`result`](#result) | 14 | 0 | 0 | 0 | 1 | [L2-result.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-result.md) |
| [`secret`](#secret) | 2 | 2 | 0 | 0 | 0 | [L2-secret.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-secret.md) |
| [`set`](#set) | 42 | 23 | 0 | 0 | 0 | [L2-set.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-set.md) |
| [`string`](#string) | 86 | 41 | 3 | 2 | 5 | [L2-string.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-string.md) |
| [`testing`](#testing) | 10 | 0 | 0 | 2 | 0 | [L2-testing.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-testing.md) |
| [`uint16`](#uint16) | 35 | 32 | 0 | 0 | 15 | [L2-sized-numbers.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-sized-numbers.md) |
| [`uint32`](#uint32) | 35 | 32 | 0 | 0 | 19 | [L2-sized-numbers.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-sized-numbers.md) |
| [`uint64`](#uint64) | 35 | 32 | 0 | 0 | 23 | [L2-sized-numbers.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-sized-numbers.md) |
| [`uint8`](#uint8) | 35 | 32 | 0 | 0 | 11 | [L2-sized-numbers.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-sized-numbers.md) |
| [`unix`](#unix) (new module) | 10 | 10 | 0 | 0 | 0 | [L2-unix.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-unix.md) |
| [`url`](#url) | 9 | 4 | 0 | 1 | 1 | [L2-url.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-url.md) |
| [`uuid`](#uuid) (new module) | 7 | 7 | 0 | 0 | 0 | [L2-uuid.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-uuid.md) |
| [`value`](#value) | 29 | 14 | 1 | 0 | 5 | [L2-value.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-value.md) |
| [`zlib`](#zlib) | 11 | 3 | 0 | 3 | 2 | [L2-zlib.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-zlib.md) |

<a id="args"></a>

## args

Record: [L2-args.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-args.md)

| Function | Signature (after redesign) | Change | Proposal |
| --- | --- | --- | --- |
| `has_flag` | `effect fn args.has_flag(name: String) -> Bool` | New | R-args-01 |
| `option` | `effect fn args.option(name: String) -> String?` |  |  |
| `positional` | `effect fn args.positional(options: List[String]) -> List[String]` |  |  |

Removed (4):

| Function | Replacement | Proposal |
| --- | --- | --- |
| `flag` | `has_flag` | R-args-01 |
| `option_or` | `option(n)! ?? d` | R-args-02 |
| `positional_at` | `list.get(args.positional(options)!, i)` | R-args-03 |
| `raw` | `env.args` | R-args-02 |

<a id="base64"></a>

## base64

Record: [L2-base64.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-base64.md)

| Function | Signature (after redesign) | Change | Proposal |
| --- | --- | --- | --- |
| `decode` | `fn base64.decode(s: String) -> Result[Bytes, String]` | Semantic change: accepts only `+/` | R-base64-01 |
| `decode_url` | `fn base64.decode_url(s: String) -> Result[Bytes, String]` | Semantic change: accepts only `-_` | R-base64-01 |
| `encode` | `fn base64.encode(b: Bytes) -> String` |  |  |
| `encode_url` | `fn base64.encode_url(b: Bytes) -> String` |  |  |

<a id="bigint"></a>

## bigint

New module (added at L1).
Record: [L2-bigint.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-bigint.md)

| Function | Signature (after redesign) | Change | Proposal |
| --- | --- | --- | --- |
| `abs` | `fn bigint.abs(x: BigInt) -> BigInt` | New | R-bigint-01 |
| `band` | `fn bigint.band(a: BigInt, b: BigInt) -> BigInt` | New | R-bigint-07 |
| `bit_length` | `fn bigint.bit_length(x: BigInt) -> Int` | New | R-int-11 |
| `bnot` | `fn bigint.bnot(x: BigInt) -> BigInt` | New | R-bigint-07 |
| `bor` | `fn bigint.bor(a: BigInt, b: BigInt) -> BigInt` | New | R-bigint-07 |
| `bshl` | `fn bigint.bshl(a: BigInt, n: Int) -> BigInt` | New | R-bigint-07 |
| `bshr` | `fn bigint.bshr(a: BigInt, n: Int) -> BigInt` | New | R-bigint-07 |
| `bxor` | `fn bigint.bxor(a: BigInt, b: BigInt) -> BigInt` | New | R-bigint-07 |
| `clamp` | `fn bigint.clamp(x: BigInt, lo: BigInt, hi: BigInt) -> BigInt` | New | R-bigint-05 |
| `compare` | `fn bigint.compare(a: BigInt, b: BigInt) -> Int` | New | R-bigint-01 |
| `floor_div` | `fn bigint.floor_div(a: BigInt, b: BigInt) -> BigInt` | New | R-bigint-01 (same spelling as R-int-03) |
| `from` | `fn bigint.from[S: Numeric](x: S) -> BigInt` | New | R-L0-40 |
| `from_checked` | `fn bigint.from_checked[S: Numeric](x: S) -> BigInt?` | New | R-L0-40 |
| `is_even` | `fn bigint.is_even(x: BigInt) -> Bool` | New | R-bigint-01 (same spelling as `int.is_even`) |
| `is_odd` | `fn bigint.is_odd(x: BigInt) -> Bool` | New | R-bigint-01 |
| `max` | `fn bigint.max(a: BigInt, b: BigInt) -> BigInt` | New | R-bigint-01 |
| `min` | `fn bigint.min(a: BigInt, b: BigInt) -> BigInt` | New | R-bigint-01 |
| `modulo` | `fn bigint.modulo(a: BigInt, b: BigInt) -> BigInt` | New | R-bigint-01 (same spelling as R-int-03) |
| `parse` | `fn bigint.parse(s: String) -> Result[BigInt, String]` | New | R-bigint-01 |
| `parse_radix` | `fn bigint.parse_radix(s: String, radix: Int) -> Result[BigInt, String]` | New | R-bigint-01 |
| `pow` | `fn bigint.pow(base: BigInt, exp: Int) -> BigInt` | New | R-bigint-01 |
| `sign` | `fn bigint.sign(x: BigInt) -> Int` | New | R-bigint-01 |
| `to_string` | `fn bigint.to_string(x: BigInt) -> String` | New | R-bigint-01 |
| `to_string_radix` | `fn bigint.to_string_radix(x: BigInt, radix: Int) -> String` | New | R-bigint-01 |

<a id="bool"></a>

## bool

Record: [L2-bool.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-bool.md)

| Function | Signature (after redesign) | Change | Proposal |
| --- | --- | --- | --- |
| `compare` | `fn bool.compare(a: Bool, b: Bool) -> Int` | New | R-bool-02 |
| `parse` | `fn bool.parse(s: String) -> Result[Bool, String]` | New | R-bool-01 |
| `to_string` | `fn bool.to_string(b: Bool) -> String` | New | R-bool-03 |

<a id="bytes"></a>

## bytes

Record: [L2-bytes.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-bytes.md)

| Function | Signature (after redesign) | Change | Proposal |
| --- | --- | --- | --- |
| `all` | `fn bytes.all(b: Bytes, f: (Int) -> Bool) -> Bool` | New | R-bytes-18 |
| `any` | `fn bytes.any(b: Bytes, f: (Int) -> Bool) -> Bool` | New | R-bytes-18 |
| `append_bool` | `fn bytes.append_bool(mut b: Bytes, x: Bool) -> Unit` | Renamed (formerly `write_bool`) | R-bytes-03 |
| `append_bytes` | `fn bytes.append_bytes(mut b: Bytes, src: Bytes) -> Unit` | New | R-bytes-05 |
| `append_f16_be` | `fn bytes.append_f16_be(mut b: Bytes, x: Float) -> Unit` | New | R-bytes-03 |
| `append_f16_le` | `fn bytes.append_f16_le(mut b: Bytes, x: Float) -> Unit` | New | R-bytes-03 |
| `append_f32_be` | `fn bytes.append_f32_be(mut b: Bytes, x: Float) -> Unit` |  |  |
| `append_f32_le` | `fn bytes.append_f32_le(mut b: Bytes, x: Float) -> Unit` |  |  |
| `append_f64_be` | `fn bytes.append_f64_be(mut b: Bytes, x: Float) -> Unit` |  |  |
| `append_f64_le` | `fn bytes.append_f64_le(mut b: Bytes, x: Float) -> Unit` |  |  |
| `append_i16_be` | `fn bytes.append_i16_be(mut b: Bytes, x: Int) -> Unit` |  |  |
| `append_i16_le` | `fn bytes.append_i16_le(mut b: Bytes, x: Int) -> Unit` |  |  |
| `append_i32_be` | `fn bytes.append_i32_be(mut b: Bytes, x: Int) -> Unit` |  |  |
| `append_i32_le` | `fn bytes.append_i32_le(mut b: Bytes, x: Int) -> Unit` |  |  |
| `append_i64_be` | `fn bytes.append_i64_be(mut b: Bytes, x: Int) -> Unit` |  |  |
| `append_i64_le` | `fn bytes.append_i64_le(mut b: Bytes, x: Int) -> Unit` |  |  |
| `append_i8` | `fn bytes.append_i8(mut b: Bytes, x: Int) -> Unit` | New | R-bytes-15 |
| `append_u16_be` | `fn bytes.append_u16_be(mut b: Bytes, x: Int) -> Unit` |  |  |
| `append_u16_le` | `fn bytes.append_u16_le(mut b: Bytes, x: Int) -> Unit` |  |  |
| `append_u32_be` | `fn bytes.append_u32_be(mut b: Bytes, x: Int) -> Unit` |  |  |
| `append_u32_le` | `fn bytes.append_u32_le(mut b: Bytes, x: Int) -> Unit` |  |  |
| `append_u8` | `fn bytes.append_u8(mut b: Bytes, x: Int) -> Unit` |  | R-bytes-25 |
| `chunk` | `fn bytes.chunk(b: Bytes, n: Int) -> List[Bytes]` | Renamed (formerly `chunks`) | R-bytes-08 |
| `clear` | `fn bytes.clear(mut b: Bytes) -> Unit` |  |  |
| `compare` | `fn bytes.compare(a: Bytes, b: Bytes) -> Int` | Renamed (formerly `cmp`) | R-bytes-08 |
| `contains` | `fn bytes.contains(b: Bytes, needle: Bytes) -> Bool` |  |  |
| `copy_from` | `fn bytes.copy_from(mut b: Bytes, src: Bytes, dst_off: Int, src_off: Int, len: Int) -> Unit` |  |  |
| `copy_within` | `fn bytes.copy_within(mut b: Bytes, src_start: Int, src_end: Int, dst: Int) -> Unit` |  |  |
| `count` | `fn bytes.count(b: Bytes, f: (Int) -> Bool) -> Int` | New | R-bytes-18 |
| `count_matches` | `fn bytes.count_matches(b: Bytes, needle: Bytes) -> Int` | New | R-bytes-27 |
| `dedup` | `fn bytes.dedup(b: Bytes) -> Bytes` | New | R-L0-42 |
| `drop` | `fn bytes.drop(b: Bytes, n: Int) -> Bytes` | New. Negative numbers abort. R-L0-29 | R-bytes-08 |
| `drop_end` | `fn bytes.drop_end(b: Bytes, n: Int) -> Bytes` | New. Negative numbers abort. R-L0-29 | R-bytes-08 |
| `drop_while` | `fn bytes.drop_while(b: Bytes, f: (Int) -> Bool) -> Bytes` | New | R-L0-42 |
| `ends_with` | `fn bytes.ends_with(b: Bytes, suffix: Bytes) -> Bool` |  |  |
| `enumerate` | `fn bytes.enumerate(b: Bytes) -> List[(Int, Int)]` | New | R-L0-42 |
| `fill` | `fn bytes.fill(mut b: Bytes, x: Int) -> Unit` |  |  |
| `filter` | `fn bytes.filter(b: Bytes, f: (Int) -> Bool) -> Bytes` | New | R-bytes-18 |
| `filter_map` | `fn bytes.filter_map(b: Bytes, f: (Int) -> B?) -> List[B]` | New | R-L0-42 |
| `find` | `fn bytes.find(b: Bytes, f: (Int) -> Bool) -> Int?` | New | R-bytes-18 |
| `find_index` | `fn bytes.find_index(b: Bytes, f: (Int) -> Bool) -> Int?` | New | R-L0-42 |
| `find_map` | `fn bytes.find_map(b: Bytes, f: (Int) -> B?) -> B?` | New | R-L0-42 |
| `first` | `fn bytes.first(b: Bytes) -> Int?` | New | R-bytes-20 |
| `fold` | `fn bytes.fold(b: Bytes, init: B, f: (B, Int) -> B) -> B` | New | R-bytes-18 |
| `from_list` | `fn bytes.from_list(xs: List[Int]) -> Bytes` |  |  |
| `from_string` | `fn bytes.from_string(s: String) -> Bytes` |  | R-bytes-10 |
| `get` | `fn bytes.get(b: Bytes, i: Int) -> Int?` |  | R-bytes-09 |
| `get_bool` | `fn bytes.get_bool(b: Bytes, pos: Int) -> Bool?` | Renamed (formerly `read_bool`). Out of range returns none, not false | R-bytes-09 |
| `get_f16_be` | `fn bytes.get_f16_be(b: Bytes, pos: Int) -> Float?` | New | R-bytes-03 |
| `get_f16_be_array` | `fn bytes.get_f16_be_array(b: Bytes, pos: Int, count: Int) -> List[Float]?` | New | R-bytes-03 |
| `get_f16_le` | `fn bytes.get_f16_le(b: Bytes, pos: Int) -> Float?` | Renamed (formerly `read_f16_le`). Out of range returns none, not 0 | R-bytes-09 |
| `get_f16_le_array` | `fn bytes.get_f16_le_array(b: Bytes, pos: Int, count: Int) -> List[Float]?` | Renamed (formerly `read_f16_le_array`). Out of range returns none, not 0 | R-bytes-09 |
| `get_f32_be` | `fn bytes.get_f32_be(b: Bytes, pos: Int) -> Float?` | Renamed (formerly `read_f32_be`). Out of range returns none, not 0 | R-bytes-09 |
| `get_f32_be_array` | `fn bytes.get_f32_be_array(b: Bytes, pos: Int, count: Int) -> List[Float]?` | Renamed (formerly `read_f32_be_array`). Out of range returns none, not 0 | R-bytes-09 |
| `get_f32_le` | `fn bytes.get_f32_le(b: Bytes, pos: Int) -> Float?` | Renamed (formerly `read_f32_le`). Out of range returns none, not 0 | R-bytes-09 |
| `get_f32_le_array` | `fn bytes.get_f32_le_array(b: Bytes, pos: Int, count: Int) -> List[Float]?` | Renamed (formerly `read_f32_le_array`). Out of range returns none, not 0 | R-bytes-09 |
| `get_f64_be` | `fn bytes.get_f64_be(b: Bytes, pos: Int) -> Float?` | Renamed (formerly `read_f64_be`). Out of range returns none, not 0 | R-bytes-09 |
| `get_f64_be_array` | `fn bytes.get_f64_be_array(b: Bytes, pos: Int, count: Int) -> List[Float]?` | Renamed (formerly `read_f64_be_array`). Out of range returns none, not 0 | R-bytes-09 |
| `get_f64_le` | `fn bytes.get_f64_le(b: Bytes, pos: Int) -> Float?` | Renamed (formerly `read_f64_le`). Out of range returns none, not 0 | R-bytes-09 |
| `get_f64_le_array` | `fn bytes.get_f64_le_array(b: Bytes, pos: Int, count: Int) -> List[Float]?` | Renamed (formerly `read_f64_le_array`). Out of range returns none, not 0 | R-bytes-09 |
| `get_i16_be` | `fn bytes.get_i16_be(b: Bytes, pos: Int) -> Int?` | Renamed (formerly `read_i16_be`). Out of range returns none, not 0 | R-bytes-09 |
| `get_i16_be_array` | `fn bytes.get_i16_be_array(b: Bytes, pos: Int, count: Int) -> List[Int]?` | Renamed (formerly `read_i16_be_array`). Out of range returns none, not 0 | R-bytes-09 |
| `get_i16_le` | `fn bytes.get_i16_le(b: Bytes, pos: Int) -> Int?` | Renamed (formerly `read_i16_le`). Out of range returns none, not 0 | R-bytes-09 |
| `get_i16_le_array` | `fn bytes.get_i16_le_array(b: Bytes, pos: Int, count: Int) -> List[Int]?` | Renamed (formerly `read_i16_le_array`). Out of range returns none, not 0 | R-bytes-09 |
| `get_i32_be` | `fn bytes.get_i32_be(b: Bytes, pos: Int) -> Int?` | Renamed (formerly `read_i32_be`). Out of range returns none, not 0 | R-bytes-09 |
| `get_i32_be_array` | `fn bytes.get_i32_be_array(b: Bytes, pos: Int, count: Int) -> List[Int]?` | Renamed (formerly `read_i32_be_array`). Out of range returns none, not 0 | R-bytes-09 |
| `get_i32_le` | `fn bytes.get_i32_le(b: Bytes, pos: Int) -> Int?` | Renamed (formerly `read_i32_le`). Out of range returns none, not 0 | R-bytes-09 |
| `get_i32_le_array` | `fn bytes.get_i32_le_array(b: Bytes, pos: Int, count: Int) -> List[Int]?` | Renamed (formerly `read_i32_le_array`). Out of range returns none, not 0 | R-bytes-09 |
| `get_i64_be` | `fn bytes.get_i64_be(b: Bytes, pos: Int) -> Int?` | Renamed (formerly `read_i64_be`). Out of range returns none, not 0 | R-bytes-09 |
| `get_i64_be_array` | `fn bytes.get_i64_be_array(b: Bytes, pos: Int, count: Int) -> List[Int]?` | Renamed (formerly `read_i64_be_array`). Out of range returns none, not 0 | R-bytes-09 |
| `get_i64_le` | `fn bytes.get_i64_le(b: Bytes, pos: Int) -> Int?` | Renamed (formerly `read_i64_le`). Out of range returns none, not 0 | R-bytes-09 |
| `get_i64_le_array` | `fn bytes.get_i64_le_array(b: Bytes, pos: Int, count: Int) -> List[Int]?` | Renamed (formerly `read_i64_le_array`). Out of range returns none, not 0 | R-bytes-09 |
| `get_i8` | `fn bytes.get_i8(b: Bytes, pos: Int) -> Int?` | New | R-bytes-15 |
| `get_u16_be` | `fn bytes.get_u16_be(b: Bytes, pos: Int) -> Int?` | Renamed (formerly `read_u16_be`). Out of range returns none, not 0 | R-bytes-09 |
| `get_u16_be_array` | `fn bytes.get_u16_be_array(b: Bytes, pos: Int, count: Int) -> List[Int]?` | Renamed (formerly `read_u16_be_array`). Out of range returns none, not 0 | R-bytes-09 |
| `get_u16_le` | `fn bytes.get_u16_le(b: Bytes, pos: Int) -> Int?` | Renamed (formerly `read_u16_le`). Out of range returns none, not 0 | R-bytes-09 |
| `get_u16_le_array` | `fn bytes.get_u16_le_array(b: Bytes, pos: Int, count: Int) -> List[Int]?` | Renamed (formerly `read_u16_le_array`). Out of range returns none, not 0 | R-bytes-09 |
| `get_u32_be` | `fn bytes.get_u32_be(b: Bytes, pos: Int) -> Int?` | Renamed (formerly `read_u32_be`). Out of range returns none, not 0 | R-bytes-09 |
| `get_u32_be_array` | `fn bytes.get_u32_be_array(b: Bytes, pos: Int, count: Int) -> List[Int]?` | Renamed (formerly `read_u32_be_array`). Out of range returns none, not 0 | R-bytes-09 |
| `get_u32_le` | `fn bytes.get_u32_le(b: Bytes, pos: Int) -> Int?` | Renamed (formerly `read_u32_le`). Out of range returns none, not 0 | R-bytes-09 |
| `get_u32_le_array` | `fn bytes.get_u32_le_array(b: Bytes, pos: Int, count: Int) -> List[Int]?` | Renamed (formerly `read_u32_le_array`). Out of range returns none, not 0 | R-bytes-09 |
| `get_u8` | `fn bytes.get_u8(b: Bytes, pos: Int) -> Int?` | New | R-bytes-25 |
| `group_by` | `fn bytes.group_by(b: Bytes, f: (Int) -> B) -> Map[B, List[Int]]` | New | R-L0-42 |
| `index_of` | `fn bytes.index_of(b: Bytes, needle: Bytes) -> Int?` |  |  |
| `insert` | `fn bytes.insert(b: Bytes, i: Int, x: Int) -> Bytes` | Semantic change: aborts outside `0..=len` | R-bytes-11 |
| `intersperse` | `fn bytes.intersperse(b: Bytes, sep: Int) -> Bytes` | New | R-L0-42 |
| `is_empty` | `fn bytes.is_empty(b: Bytes) -> Bool` |  |  |
| `is_equal_constant_time` | `fn bytes.is_equal_constant_time(a: Bytes, b: Bytes) -> Bool` | New | R-bytes-29 |
| `is_eof` | `fn bytes.is_eof(b: Bytes, pos: Int) -> Bool` | Renamed (formerly `eof`) | R-bytes-07 |
| `is_valid_utf8` | `fn bytes.is_valid_utf8(b: Bytes) -> Bool` |  |  |
| `last` | `fn bytes.last(b: Bytes) -> Int?` | New | R-bytes-20 |
| `last_index_of` | `fn bytes.last_index_of(b: Bytes, needle: Bytes) -> Int?` | New | R-bytes-27 |
| `len` | `fn bytes.len(b: Bytes) -> Int` |  |  |
| `map` | `fn bytes.map(b: Bytes, f: (Int) -> Int) -> Bytes` | Renamed (formerly `map_each`) | R-bytes-08 |
| `max` | `fn bytes.max(b: Bytes) -> Int?` | New | R-L0-42 |
| `max_by` | `fn bytes.max_by(b: Bytes, f: (Int) -> K) -> Int?` | New | R-L0-42 |
| `max_with` | `fn bytes.max_with(b: Bytes, cmp: (Int, Int) -> Int) -> Int?` | New | R-L0-42 |
| `min` | `fn bytes.min(b: Bytes) -> Int?` | New | R-L0-42 |
| `min_by` | `fn bytes.min_by(b: Bytes, f: (Int) -> K) -> Int?` | New | R-L0-42 |
| `min_with` | `fn bytes.min_with(b: Bytes, cmp: (Int, Int) -> Int) -> Int?` | New | R-L0-42 |
| `new` | `fn bytes.new() -> Bytes` | New | R-bytes-28 |
| `next_bool` | `fn bytes.next_bool(b: Bytes, pos: Int) -> (Bool?, Int)` | Renamed (formerly `read_bool_at`). Returns `(値, 次の位置)` | R-bytes-13 |
| `next_bytes` | `fn bytes.next_bytes(b: Bytes, pos: Int, n: Int) -> (Bytes?, Int)` | Renamed (formerly `take_at`). Returns `(値, 次の位置)` | R-bytes-13 |
| `next_f16_be` | `fn bytes.next_f16_be(b: Bytes, pos: Int) -> (Float?, Int)` | New | R-bytes-03、R-bytes-13 |
| `next_f16_le` | `fn bytes.next_f16_le(b: Bytes, pos: Int) -> (Float?, Int)` | Renamed (formerly `read_f16_le_at`). Returns `(値, 次の位置)` | R-bytes-13 |
| `next_f32_be` | `fn bytes.next_f32_be(b: Bytes, pos: Int) -> (Float?, Int)` | Renamed (formerly `read_f32_be_at`). Returns `(値, 次の位置)` | R-bytes-13 |
| `next_f32_le` | `fn bytes.next_f32_le(b: Bytes, pos: Int) -> (Float?, Int)` | Renamed (formerly `read_f32_le_at`). Returns `(値, 次の位置)` | R-bytes-13 |
| `next_f64_be` | `fn bytes.next_f64_be(b: Bytes, pos: Int) -> (Float?, Int)` | Renamed (formerly `read_f64_be_at`). Returns `(値, 次の位置)` | R-bytes-13 |
| `next_f64_le` | `fn bytes.next_f64_le(b: Bytes, pos: Int) -> (Float?, Int)` | Renamed (formerly `read_f64_le_at`). Returns `(値, 次の位置)` | R-bytes-13 |
| `next_i16_be` | `fn bytes.next_i16_be(b: Bytes, pos: Int) -> (Int?, Int)` | Renamed (formerly `read_i16_be_at`). Returns `(値, 次の位置)` | R-bytes-13 |
| `next_i16_le` | `fn bytes.next_i16_le(b: Bytes, pos: Int) -> (Int?, Int)` | Renamed (formerly `read_i16_le_at`). Returns `(値, 次の位置)` | R-bytes-13 |
| `next_i32_be` | `fn bytes.next_i32_be(b: Bytes, pos: Int) -> (Int?, Int)` | Renamed (formerly `read_i32_be_at`). Returns `(値, 次の位置)` | R-bytes-13 |
| `next_i32_le` | `fn bytes.next_i32_le(b: Bytes, pos: Int) -> (Int?, Int)` | Renamed (formerly `read_i32_le_at`). Returns `(値, 次の位置)` | R-bytes-13 |
| `next_i64_be` | `fn bytes.next_i64_be(b: Bytes, pos: Int) -> (Int?, Int)` | Renamed (formerly `read_i64_be_at`). Returns `(値, 次の位置)` | R-bytes-13 |
| `next_i64_le` | `fn bytes.next_i64_le(b: Bytes, pos: Int) -> (Int?, Int)` | Renamed (formerly `read_i64_le_at`). Returns `(値, 次の位置)` | R-bytes-13 |
| `next_i8` | `fn bytes.next_i8(b: Bytes, pos: Int) -> (Int?, Int)` | New | R-bytes-15、R-bytes-13 |
| `next_u16_be` | `fn bytes.next_u16_be(b: Bytes, pos: Int) -> (Int?, Int)` | Renamed (formerly `read_u16_be_at`). Returns `(値, 次の位置)` | R-bytes-13 |
| `next_u16_le` | `fn bytes.next_u16_le(b: Bytes, pos: Int) -> (Int?, Int)` | Renamed (formerly `read_u16_le_at`). Returns `(値, 次の位置)` | R-bytes-13 |
| `next_u32_be` | `fn bytes.next_u32_be(b: Bytes, pos: Int) -> (Int?, Int)` | Renamed (formerly `read_u32_be_at`). Returns `(値, 次の位置)` | R-bytes-13 |
| `next_u32_le` | `fn bytes.next_u32_le(b: Bytes, pos: Int) -> (Int?, Int)` | Renamed (formerly `read_u32_le_at`). Returns `(値, 次の位置)` | R-bytes-13 |
| `next_u8` | `fn bytes.next_u8(b: Bytes, pos: Int) -> (Int?, Int)` | Renamed (formerly `read_u8_at`). Returns `(値, 次の位置)` | R-bytes-13 |
| `pad_end` | `fn bytes.pad_end(b: Bytes, n: Int, fill: Int) -> Bytes` | Renamed (formerly `pad_right`) | R-bytes-08 |
| `pad_start` | `fn bytes.pad_start(b: Bytes, n: Int, fill: Int) -> Bytes` | Renamed (formerly `pad_left`) | R-bytes-08 |
| `partition` | `fn bytes.partition(b: Bytes, f: (Int) -> Bool) -> (List[Int], List[Int])` | New | R-L0-42 |
| `pop` | `fn bytes.pop(mut b: Bytes) -> Int?` | New | R-bytes-20 |
| `product` | `fn bytes.product(b: Bytes) -> Int` | New | R-L0-42 |
| `push` | `fn bytes.push(mut b: Bytes, x: Int) -> Unit` |  | R-bytes-02 |
| `put_bool` | `fn bytes.put_bool(mut b: Bytes, pos: Int, x: Bool) -> Unit` | New. Out of range aborts | R-bytes-03、R-bytes-11 |
| `put_f16_be` | `fn bytes.put_f16_be(mut b: Bytes, pos: Int, x: Float) -> Unit` | New | R-bytes-03 |
| `put_f16_le` | `fn bytes.put_f16_le(mut b: Bytes, pos: Int, x: Float) -> Unit` | New | R-bytes-03 |
| `put_f32_be` | `fn bytes.put_f32_be(mut b: Bytes, pos: Int, x: Float) -> Unit` | Renamed (formerly `set_f32_be`). Out of range aborts | R-bytes-01、R-bytes-11 |
| `put_f32_le` | `fn bytes.put_f32_le(mut b: Bytes, pos: Int, x: Float) -> Unit` | Renamed (formerly `set_f32_le`). Out of range aborts | R-bytes-01、R-bytes-11 |
| `put_f64_be` | `fn bytes.put_f64_be(mut b: Bytes, pos: Int, x: Float) -> Unit` | Renamed (formerly `set_f64_be`). Out of range aborts | R-bytes-01、R-bytes-11 |
| `put_f64_le` | `fn bytes.put_f64_le(mut b: Bytes, pos: Int, x: Float) -> Unit` | Renamed (formerly `set_f64_le`). Out of range aborts | R-bytes-01、R-bytes-11 |
| `put_i16_be` | `fn bytes.put_i16_be(mut b: Bytes, pos: Int, x: Int) -> Unit` | Renamed (formerly `set_i16_be`). Out of range aborts | R-bytes-01、R-bytes-11 |
| `put_i16_le` | `fn bytes.put_i16_le(mut b: Bytes, pos: Int, x: Int) -> Unit` | Renamed (formerly `set_i16_le`). Out of range aborts | R-bytes-01、R-bytes-11 |
| `put_i32_be` | `fn bytes.put_i32_be(mut b: Bytes, pos: Int, x: Int) -> Unit` | Renamed (formerly `set_i32_be`). Out of range aborts | R-bytes-01、R-bytes-11 |
| `put_i32_le` | `fn bytes.put_i32_le(mut b: Bytes, pos: Int, x: Int) -> Unit` | Renamed (formerly `set_i32_le`). Out of range aborts | R-bytes-01、R-bytes-11 |
| `put_i64_be` | `fn bytes.put_i64_be(mut b: Bytes, pos: Int, x: Int) -> Unit` | Renamed (formerly `set_i64_be`). Out of range aborts | R-bytes-01、R-bytes-11 |
| `put_i64_le` | `fn bytes.put_i64_le(mut b: Bytes, pos: Int, x: Int) -> Unit` | Renamed (formerly `set_i64_le`). Out of range aborts | R-bytes-01、R-bytes-11 |
| `put_i8` | `fn bytes.put_i8(mut b: Bytes, pos: Int, x: Int) -> Unit` | New | R-bytes-15 |
| `put_u16_be` | `fn bytes.put_u16_be(mut b: Bytes, pos: Int, x: Int) -> Unit` | Renamed (formerly `set_u16_be`). Out of range aborts | R-bytes-01、R-bytes-11 |
| `put_u16_le` | `fn bytes.put_u16_le(mut b: Bytes, pos: Int, x: Int) -> Unit` | Renamed (formerly `set_u16_le`). Out of range aborts | R-bytes-01、R-bytes-11 |
| `put_u32_be` | `fn bytes.put_u32_be(mut b: Bytes, pos: Int, x: Int) -> Unit` | Renamed (formerly `set_u32_be`). Out of range aborts | R-bytes-01、R-bytes-11 |
| `put_u32_le` | `fn bytes.put_u32_le(mut b: Bytes, pos: Int, x: Int) -> Unit` | Renamed (formerly `set_u32_le`). Out of range aborts | R-bytes-01、R-bytes-11 |
| `put_u8` | `fn bytes.put_u8(mut b: Bytes, pos: Int, x: Int) -> Unit` | Renamed (formerly `set_u8`). Out of range aborts | R-bytes-01、R-bytes-11 |
| `reduce` | `fn bytes.reduce(b: Bytes, f: (Int, Int) -> Int) -> Int?` | New | R-L0-42 |
| `remove_at` | `fn bytes.remove_at(b: Bytes, i: Int) -> Bytes` | Semantic change: out of range aborts | R-bytes-11 |
| `repeat` | `fn bytes.repeat(b: Bytes, n: Int) -> Bytes` |  |  |
| `replace` | `fn bytes.replace(b: Bytes, from: Bytes, to: Bytes) -> Bytes` | New | R-bytes-27 |
| `replace_first` | `fn bytes.replace_first(b: Bytes, from: Bytes, to: Bytes) -> Bytes` | New | R-bytes-27 |
| `reverse` | `fn bytes.reverse(b: Bytes) -> Bytes` |  |  |
| `scan` | `fn bytes.scan(b: Bytes, init: B, f: (B, Int) -> B) -> List[B]` | New | R-L0-42 |
| `set` | `fn bytes.set(b: Bytes, i: Int, x: Int) -> Bytes` | Semantic change: out of range aborts | R-bytes-11 |
| `skip` | `fn bytes.skip(b: Bytes, pos: Int, n: Int) -> Int` | Semantic change: `n < 0` aborts | R-bytes-13、R-bytes-21 |
| `slice` | `fn bytes.slice(b: Bytes, start: Int, end: Int) -> Bytes` | Semantic change: negative positions abort. R-L0-29 | R-bytes-12、R-L0-29 |
| `sort` | `fn bytes.sort(b: Bytes) -> List[Int]` | New | R-L0-42 |
| `sort_by` | `fn bytes.sort_by(b: Bytes, f: (Int) -> B) -> List[Int]` | New | R-L0-42 |
| `sort_with` | `fn bytes.sort_with(b: Bytes, cmp: (Int, Int) -> Int) -> List[Int]` | New | R-L0-42 |
| `split` | `fn bytes.split(b: Bytes, sep: Bytes) -> List[Bytes]` |  |  |
| `split_at` | `fn bytes.split_at(b: Bytes, n: Int) -> (Bytes, Bytes)` | New | R-L0-42 |
| `starts_with` | `fn bytes.starts_with(b: Bytes, prefix: Bytes) -> Bool` |  |  |
| `strip_prefix` | `fn bytes.strip_prefix(b: Bytes, prefix: Bytes) -> Bytes?` | New | R-bytes-27 |
| `strip_suffix` | `fn bytes.strip_suffix(b: Bytes, suffix: Bytes) -> Bytes?` | New | R-bytes-27 |
| `sum` | `fn bytes.sum(b: Bytes) -> Int` | New | R-L0-42 |
| `take` | `fn bytes.take(b: Bytes, n: Int) -> Bytes` | New. Negative numbers abort. R-L0-29 | R-bytes-08 |
| `take_end` | `fn bytes.take_end(b: Bytes, n: Int) -> Bytes` | New. Negative numbers abort. R-L0-29 | R-bytes-08 |
| `take_while` | `fn bytes.take_while(b: Bytes, f: (Int) -> Bool) -> Bytes` | New | R-L0-42 |
| `to_list` | `fn bytes.to_list(b: Bytes) -> List[Int]` |  |  |
| `to_string` | `fn bytes.to_string(b: Bytes) -> Result[String, String]` |  | R-bytes-10 |
| `to_string_lossy` | `fn bytes.to_string_lossy(b: Bytes) -> String` |  | R-bytes-10 |
| `unique` | `fn bytes.unique(b: Bytes) -> Bytes` | New | R-L0-42 |
| `unique_by` | `fn bytes.unique_by(b: Bytes, f: (Int) -> K) -> Bytes` | New | R-L0-42 |
| `window` | `fn bytes.window(b: Bytes, n: Int) -> List[Bytes]` | New | R-L0-42 |
| `xor` | `fn bytes.xor(a: Bytes, b: Bytes) -> Bytes` | Semantic change: differing lengths abort | R-bytes-23 |
| `zeros` | `fn bytes.zeros(n: Int) -> Bytes` | Renamed (formerly `new`) | R-bytes-28 |
| `zip` | `fn bytes.zip(b: Bytes, ys: List[B]) -> List[(Int, B)]` | New | R-L0-42 |
| `zip_with` | `fn bytes.zip_with(b: Bytes, ys: List[B], f: (Int, B) -> C) -> List[C]` | New | R-L0-42 |

Removed (34):

| Function | Replacement | Proposal |
| --- | --- | --- |
| `as_mut_ptr` | `ffi.alloc` | R-bytes-06 |
| `as_ptr` | `ffi.region` | R-bytes-06 |
| `concat` | `+` | R-bytes-16 |
| `copy_to_ptr` | `ffi.write` | R-bytes-06 |
| `data_ptr` | `ffi.region` | R-bytes-06 |
| `from_raw_ptr` | `ffi.read` | R-bytes-06 |
| `get_or` | `get(b, i) ?? d` | R-bytes-07 |
| `heap_restore` | Make names internal | R-bytes-14 |
| `heap_save` | Make names internal | R-bytes-14 |
| `lines` | `string.lines(bytes.to_string(b)!)` or `bytes.split` | R-bytes-17 |
| `read_float32` | Suffix family `get_`/`append_`/`put_` | R-bytes-04 |
| `read_int32` | Suffix family `get_`/`append_`/`put_` | R-bytes-04 |
| `read_length_prefixed_strings_le` | Combination of `get_u32_le` and `slice` | R-bytes-05 |
| `read_string_at` | `slice` ＋ `to_string_lossy` | R-bytes-05 |
| `read_string_be` | Combination of `get_u32_be` and `slice` | R-bytes-05 |
| `read_string_be_at` | Combination of `next_u32_be` and `next_bytes` | R-bytes-05 |
| `read_u8` | `get` (same role; none out of range) | R-bytes-09 |
| `read_uint16` | Suffix family `get_`/`append_`/`put_` | R-bytes-04 |
| `read_uint32` | Suffix family `get_`/`append_`/`put_` | R-bytes-04 |
| `set_at` | `put_u8` (out of range aborts) | R-bytes-02、R-bytes-11 |
| `set_float32` | Suffix family `get_`/`append_`/`put_` | R-bytes-04 |
| `set_int32` | Suffix family `get_`/`append_`/`put_` | R-bytes-04 |
| `set_uint16` | Suffix family `get_`/`append_`/`put_` | R-bytes-04 |
| `set_uint32` | Suffix family `get_`/`append_`/`put_` | R-bytes-04 |
| `skip_length_prefixed_le` | Combination of `get_u32_le` and `slice` | R-bytes-05 |
| `write_f64_be` | `append_f64_be` | R-bytes-03 |
| `write_float32` | Suffix family `get_`/`append_`/`put_` | R-bytes-04 |
| `write_i64_be` | `append_i64_be` | R-bytes-03 |
| `write_int32` | Suffix family `get_`/`append_`/`put_` | R-bytes-04 |
| `write_string_be` | `append_u32_be` ＋ `append_bytes` | R-bytes-05 |
| `write_u32_be` | `append_u32_be` | R-bytes-03 |
| `write_u8` | `push` | R-bytes-02 |
| `write_uint16` | Suffix family `get_`/`append_`/`put_` | R-bytes-04 |
| `write_uint32` | Suffix family `get_`/`append_`/`put_` | R-bytes-04 |

<a id="compute"></a>

## compute

Record: [L2-compute.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-compute.md)

| Function | Signature (after redesign) | Change | Proposal |
| --- | --- | --- | --- |
| `compare` | `fn compute.compare(a: Compute, b: Compute) -> Int` | New | R-compute-01 |
| `h` | `fn compute.h(n: Int) -> Compute` |  |  |
| `min` | `fn compute.min(n: Int) -> Compute` |  |  |
| `ms` | `fn compute.ms(n: Int) -> Compute` |  |  |
| `ns` | `fn compute.ns(n: Int) -> Compute` |  |  |
| `s` | `fn compute.s(n: Int) -> Compute` |  |  |
| `us` | `fn compute.us(n: Int) -> Compute` |  |  |

<a id="datetime"></a>

## datetime

Record: [L2-datetime.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-datetime.md)

| Function | Signature (after redesign) | Change | Proposal |
| --- | --- | --- | --- |
| `add_days` | `fn datetime.add_days(ts: Int, n: Int) -> Int` |  |  |
| `add_hours` | `fn datetime.add_hours(ts: Int, n: Int) -> Int` |  |  |
| `add_minutes` | `fn datetime.add_minutes(ts: Int, n: Int) -> Int` |  |  |
| `add_seconds` | `fn datetime.add_seconds(ts: Int, n: Int) -> Int` |  |  |
| `day` | `fn datetime.day(ts: Int) -> Int` |  |  |
| `format` | `fn datetime.format(ts: Int, pattern: String) -> String` |  | R-datetime-05 |
| `from_parts` | `fn datetime.from_parts(y: Int, m: Int, d: Int, h: Int, min: Int, s: Int) -> Result[Int, String]` | Semantic change: out of range errors. `-> Result[Int, String]` | R-datetime-07 |
| `hour` | `fn datetime.hour(ts: Int) -> Int` |  |  |
| `minute` | `fn datetime.minute(ts: Int) -> Int` |  |  |
| `monotonic_ns` | `effect fn datetime.monotonic_ns() -> Int` |  |  |
| `month` | `fn datetime.month(ts: Int) -> Int` |  |  |
| `now` | `effect fn datetime.now() -> Int` |  | R-datetime-01 (retained side of `env.unix_timestamp`) |
| `now_ms` | `effect fn datetime.now_ms() -> Int` | New | R-datetime-01 |
| `parse_iso` | `fn datetime.parse_iso(s: String) -> Result[Int, String]` |  |  |
| `second` | `fn datetime.second(ts: Int) -> Int` |  |  |
| `to_iso` | `fn datetime.to_iso(ts: Int) -> String` |  |  |
| `weekday` | `fn datetime.weekday(ts: Int) -> String` |  | R-datetime-04 |
| `year` | `fn datetime.year(ts: Int) -> Int` |  |  |

Removed (5):

| Function | Replacement | Proposal |
| --- | --- | --- |
| `diff_seconds` | `a - b` | R-datetime-03（`a - b`） |
| `from_unix` | The argument itself | R-datetime-03 (the argument itself) |
| `is_after` | `a > b` | R-datetime-03（`a > b`） |
| `is_before` | `a < b` | R-datetime-03（`a < b`） |
| `to_unix` | The argument itself | R-datetime-03 (the argument itself) |

<a id="deque"></a>

## deque

New module (added at L1).
Record: [L2-deque.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-deque.md)

| Function | Signature (after redesign) | Change | Proposal |
| --- | --- | --- | --- |
| `all` | `fn deque.all[A](d: Deque[A], f: (A) -> Bool) -> Bool` | New | R-deque-02 |
| `any` | `fn deque.any[A](d: Deque[A], f: (A) -> Bool) -> Bool` | New | R-deque-02 |
| `chunk` | `fn deque.chunk(d: Deque[A], n: Int) -> List[Deque[A]]` | New | R-L0-42 |
| `clear` | `fn deque.clear[A](mut d: Deque[A]) -> Unit` | New | R-deque-02、R-L0-04 |
| `contains` | `fn deque.contains[A](d: Deque[A], x: A) -> Bool` | New | R-deque-02 |
| `count` | `fn deque.count[A](d: Deque[A], f: (A) -> Bool) -> Int` | New | R-deque-02 |
| `dedup` | `fn deque.dedup(d: Deque[A]) -> Deque[A]` | New | R-L0-42 |
| `drop` | `fn deque.drop[A](d: Deque[A], n: Int) -> Deque[A]` | New | R-deque-04 |
| `drop_end` | `fn deque.drop_end[A](d: Deque[A], n: Int) -> Deque[A]` | New | R-deque-04 |
| `drop_while` | `fn deque.drop_while(d: Deque[A], f: (A) -> Bool) -> Deque[A]` | New | R-L0-42 |
| `enumerate` | `fn deque.enumerate(d: Deque[A]) -> List[(Int, A)]` | New | R-L0-42 |
| `filter` | `fn deque.filter[A](d: Deque[A], f: (A) -> Bool) -> Deque[A]` | New | R-deque-02 |
| `filter_map` | `fn deque.filter_map(d: Deque[A], f: (A) -> B?) -> List[B]` | New | R-L0-42 |
| `find` | `fn deque.find[A](d: Deque[A], f: (A) -> Bool) -> A?` | New | R-deque-02 |
| `find_index` | `fn deque.find_index(d: Deque[A], f: (A) -> Bool) -> Int?` | New | R-L0-42 |
| `find_map` | `fn deque.find_map(d: Deque[A], f: (A) -> B?) -> B?` | New | R-L0-42 |
| `first` | `fn deque.first[A](d: Deque[A]) -> A?` | New | R-deque-01 |
| `fold` | `fn deque.fold[A, B](d: Deque[A], init: B, f: (B, A) -> B) -> B` | New | R-deque-02 |
| `from_list` | `fn deque.from_list[A](xs: List[A]) -> Deque[A]` | New | R-deque-01 |
| `get` | `fn deque.get[A](d: Deque[A], i: Int) -> A?` | New | R-deque-01 |
| `group_by` | `fn deque.group_by(d: Deque[A], f: (A) -> B) -> Map[B, List[A]]` | New | R-L0-42 |
| `insert` | `fn deque.insert[A](d: Deque[A], i: Int, x: A) -> Deque[A]` | New | R-deque-04 |
| `intersperse` | `fn deque.intersperse(d: Deque[A], sep: A) -> Deque[A]` | New | R-L0-42 |
| `is_empty` | `fn deque.is_empty[A](d: Deque[A]) -> Bool` | New | R-deque-01 |
| `last` | `fn deque.last[A](d: Deque[A]) -> A?` | New | R-deque-01 |
| `len` | `fn deque.len[A](d: Deque[A]) -> Int` | New | R-deque-01 |
| `map` | `fn deque.map[A, B](d: Deque[A], f: (A) -> B) -> Deque[B]` | New | R-deque-02 |
| `max` | `fn deque.max(d: Deque[A]) -> A?` | New | R-L0-42 |
| `max_by` | `fn deque.max_by(d: Deque[A], f: (A) -> K) -> A?` | New | R-L0-42 |
| `max_with` | `fn deque.max_with(d: Deque[A], cmp: (A, A) -> Int) -> A?` | New | R-L0-42 |
| `min` | `fn deque.min(d: Deque[A]) -> A?` | New | R-L0-42 |
| `min_by` | `fn deque.min_by(d: Deque[A], f: (A) -> K) -> A?` | New | R-L0-42 |
| `min_with` | `fn deque.min_with(d: Deque[A], cmp: (A, A) -> Int) -> A?` | New | R-L0-42 |
| `new` | `fn deque.new[A]() -> Deque[A]` | New | R-deque-01 |
| `partition` | `fn deque.partition(d: Deque[A], f: (A) -> Bool) -> (List[A], List[A])` | New | R-L0-42 |
| `pop` | `fn deque.pop[A](mut d: Deque[A]) -> A?` | New | R-deque-01、R-deque-03 |
| `pop_front` | `fn deque.pop_front[A](mut d: Deque[A]) -> A?` | New | R-deque-01 |
| `product` | `fn deque.product[A: Numeric](d: Deque[A]) -> A` | New | R-L0-42 |
| `push` | `fn deque.push[A](mut d: Deque[A], x: A) -> Unit` | New | R-deque-01、R-deque-03 |
| `push_front` | `fn deque.push_front[A](mut d: Deque[A], x: A) -> Unit` | New | R-deque-01 |
| `reduce` | `fn deque.reduce(d: Deque[A], f: (A, A) -> A) -> A?` | New | R-L0-42 |
| `remove_at` | `fn deque.remove_at[A](d: Deque[A], i: Int) -> Deque[A]` | New | R-deque-04 |
| `reverse` | `fn deque.reverse[A](d: Deque[A]) -> Deque[A]` | New | R-deque-04 |
| `scan` | `fn deque.scan(d: Deque[A], init: B, f: (B, A) -> B) -> List[B]` | New | R-L0-42 |
| `set` | `fn deque.set[A](d: Deque[A], i: Int, x: A) -> Deque[A]` | New | R-deque-04 |
| `slice` | `fn deque.slice[A](d: Deque[A], start: Int, end: Int) -> Deque[A]` | New | R-deque-04 |
| `sort` | `fn deque.sort(d: Deque[A]) -> List[A]` | New | R-L0-42 |
| `sort_by` | `fn deque.sort_by(d: Deque[A], f: (A) -> B) -> List[A]` | New | R-L0-42 |
| `sort_with` | `fn deque.sort_with(d: Deque[A], cmp: (A, A) -> Int) -> List[A]` | New | R-L0-42 |
| `split_at` | `fn deque.split_at(d: Deque[A], n: Int) -> (Deque[A], Deque[A])` | New | R-L0-42 |
| `sum` | `fn deque.sum[A: Numeric](d: Deque[A]) -> A` | New | R-L0-42 |
| `take` | `fn deque.take[A](d: Deque[A], n: Int) -> Deque[A]` | New | R-deque-04 |
| `take_end` | `fn deque.take_end[A](d: Deque[A], n: Int) -> Deque[A]` | New | R-deque-04 |
| `take_while` | `fn deque.take_while(d: Deque[A], f: (A) -> Bool) -> Deque[A]` | New | R-L0-42 |
| `to_list` | `fn deque.to_list[A](d: Deque[A]) -> List[A]` | New | R-deque-01 |
| `unique` | `fn deque.unique(d: Deque[A]) -> Deque[A]` | New | R-L0-42 |
| `unique_by` | `fn deque.unique_by(d: Deque[A], f: (A) -> K) -> Deque[A]` | New | R-L0-42 |
| `window` | `fn deque.window(d: Deque[A], n: Int) -> List[Deque[A]]` | New | R-L0-42 |
| `zip` | `fn deque.zip(d: Deque[A], ys: List[B]) -> List[(A, B)]` | New | R-L0-42 |
| `zip_with` | `fn deque.zip_with(d: Deque[A], ys: List[B], f: (A, B) -> C) -> List[C]` | New | R-L0-42 |

<a id="duration"></a>

## duration

Record: [L2-duration.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-duration.md)

| Function | Signature (after redesign) | Change | Proposal |
| --- | --- | --- | --- |
| `compare` | `fn duration.compare(a: Duration, b: Duration) -> Int` | New | R-duration-01 |
| `h` | `fn duration.h(n: Int) -> Duration` |  |  |
| `min` | `fn duration.min(n: Int) -> Duration` |  |  |
| `ms` | `fn duration.ms(n: Int) -> Duration` |  |  |
| `ns` | `fn duration.ns(n: Int) -> Duration` |  |  |
| `s` | `fn duration.s(n: Int) -> Duration` |  |  |
| `us` | `fn duration.us(n: Int) -> Duration` |  |  |

<a id="env"></a>

## env

Record: [L2-env.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-env.md)

| Function | Signature (after redesign) | Change | Proposal |
| --- | --- | --- | --- |
| `args` | `effect fn env.args() -> List[String]` |  | R-env-01 (gather here) |
| `cwd` | `effect fn env.cwd() -> String` |  |  |
| `get` | `effect fn env.get(name: String) -> String?` |  | R-env-01、R-env-03 |
| `os` | `fn env.os() -> String` |  |  |
| `remove` | `effect fn env.remove(name: String) -> Unit` | New | R-env-02 |
| `secret` | `effect fn env.secret(name: String) -> Secret[String]?` | New. Values originate from `Secret` | R-env-05 |
| `set` | `effect fn env.set(name: String, value: String) -> Unit` |  |  |
| `sleep_ms` | `effect fn env.sleep_ms(ms: Int) -> Unit` |  | R-env-01 |
| `vars` | `effect fn env.vars() -> Map[String, String]` | New | R-env-02 |

Removed (3):

| Function | Replacement | Proposal |
| --- | --- | --- |
| `millis` | Move to `datetime.now_ms` | R-env-01 |
| `temp_dir` | `fs.temp_dir` | R-env-01 |
| `unix_timestamp` | `datetime.now` | R-env-01 |

<a id="ffi"></a>

## ffi

New module (added at L1).
Record: [L2-ffi.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-ffi.md)

| Function | Signature (after redesign) | Change | Proposal |
| --- | --- | --- | --- |
| `alloc` | `effect fn ffi.alloc(n: Int) -> Region` | New. From `bytes.as_mut_ptr`. Writable region with length | R-ffi-01、ADR-0031 |
| `is_writable` | `fn ffi.is_writable(r: Region) -> Bool` | New | ADR-0031 |
| `len` | `fn ffi.len(r: Region) -> Int` | New | ADR-0031 |
| `read` | `effect fn ffi.read(r: Region, off: Int, n: Int) -> Bytes` | New. From `bytes.from_raw_ptr`. Out of range errors | R-ffi-01、ADR-0031 |
| `region` | `fn ffi.region(b: Bytes) -> Region` | New. From `bytes.as_ptr`/`data_ptr`. Read-only region | R-ffi-01、ADR-0031 |
| `to_bytes` | `effect fn ffi.to_bytes(r: Region) -> Bytes` | New | ADR-0031 |
| `write` | `effect fn ffi.write(r: Region, off: Int, b: Bytes) -> Unit` | New. From `bytes.copy_to_ptr`. Write destination first | R-ffi-01、R-ffi-03、ADR-0031 |

ADR-0031 (the ruling for table row 5) replaced numeric-address `RawPtr` (`ffi.address`/`ffi.to_int`) with length-bearing `Region`.

<a id="float"></a>

## float

Record: [L2-float.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-float.md)

| Function | Signature (after redesign) | Change | Proposal |
| --- | --- | --- | --- |
| `abs` | `fn float.abs(x: Float) -> Float` |  |  |
| `ceil` | `fn float.ceil(x: Float) -> Float` |  |  |
| `clamp` | `fn float.clamp(x: Float, lo: Float, hi: Float) -> Float` |  |  |
| `compare` | `fn float.compare(a: Float, b: Float) -> Int` | New | R-sized-02 |
| `epsilon` | `fn float.epsilon() -> Float` | New | R-float-03 |
| `floor` | `fn float.floor(x: Float) -> Float` |  |  |
| `from` | `fn float.from[S: Numeric](x: S) -> Float` | New | R-L0-40 |
| `from_bits` | `fn float.from_bits(bits: Int) -> Float` | New | R-float-02 |
| `from_checked` | `fn float.from_checked[S: Numeric](x: S) -> Float?` | New | R-L0-40 |
| `infinity` | `fn float.infinity() -> Float` | New | R-float-03 |
| `is_finite` | `fn float.is_finite(x: Float) -> Bool` | New | R-float-04 |
| `is_infinite` | `fn float.is_infinite(x: Float) -> Bool` |  |  |
| `is_nan` | `fn float.is_nan(x: Float) -> Bool` |  |  |
| `max` | `fn float.max(a: Float, b: Float) -> Float` |  | R-math-01 (retained side of `math.fmax`) |
| `max_value` | `fn float.max_value() -> Float` | New | R-float-03 |
| `min` | `fn float.min(a: Float, b: Float) -> Float` |  | R-math-01 (retained side of `math.fmin`) |
| `min_value` | `fn float.min_value() -> Float` | New | R-float-03 |
| `nan` | `fn float.nan() -> Float` | New | R-float-03 |
| `parse` | `fn float.parse(s: String) -> Result[Float, String]` |  |  |
| `pow` | `fn float.pow(base: Float, exp: Float) -> Float` | New | R-math-01 |
| `round` | `fn float.round(x: Float) -> Float` |  |  |
| `signum` | `fn float.signum(x: Float) -> Float` | Renamed (formerly `sign`). Behavior unchanged | R-float-05 |
| `to_bits` | `fn float.to_bits(f: Float) -> Int` |  | R-float-02 (paired with `from_bits`) |
| `to_fixed` | `fn float.to_fixed(x: Float, decimals: Int) -> String` |  |  |
| `to_string` | `fn float.to_string(x: Float) -> String` |  |  |
| `trunc` | `fn float.trunc(x: Float) -> Float` | New | R-float-04 |

Removed (34):

| Function | Replacement | Proposal |
| --- | --- | --- |
| `from_float32` | `float32.to_float` | R-float-01（`float32.to_float`） |
| `from_float64` | `float64.to_float` | R-float-01（`float64.to_float`） |
| `from_int` | Same as `int.to_float` | R-float-01 (same as `int.to_float`; R-L0-07 item 3) |
| `sqrt` | `math.sqrt` | R-math-03 |
| `to_float32` | `float32.from` | R-L0-40 |
| `to_float32_checked` | `float32.from_checked` | R-L0-40 |
| `to_float64` | `float64.from` | R-L0-40 |
| `to_int` | `int.from` | R-L0-40 |
| `to_int_checked` | `int.from_checked` | R-L0-40 |
| `to_int_saturating` | `int.from_saturating` | R-L0-40 |
| `to_int16` | `int16.from` | R-L0-40 |
| `to_int16_checked` | `int16.from_checked` | R-L0-40 |
| `to_int16_saturating` | `int16.from_saturating` | R-L0-40 |
| `to_int32` | `int32.from` | R-L0-40 |
| `to_int32_checked` | `int32.from_checked` | R-L0-40 |
| `to_int32_saturating` | `int32.from_saturating` | R-L0-40 |
| `to_int64` | `int64.from` | R-L0-40 |
| `to_int64_checked` | `int64.from_checked` | R-L0-40 |
| `to_int64_saturating` | `int64.from_saturating` | R-L0-40 |
| `to_int8` | `int8.from` | R-L0-40 |
| `to_int8_checked` | `int8.from_checked` | R-L0-40 |
| `to_int8_saturating` | `int8.from_saturating` | R-L0-40 |
| `to_uint16` | `uint16.from` | R-L0-40 |
| `to_uint16_checked` | `uint16.from_checked` | R-L0-40 |
| `to_uint16_saturating` | `uint16.from_saturating` | R-L0-40 |
| `to_uint32` | `uint32.from` | R-L0-40 |
| `to_uint32_checked` | `uint32.from_checked` | R-L0-40 |
| `to_uint32_saturating` | `uint32.from_saturating` | R-L0-40 |
| `to_uint64` | `uint64.from` | R-L0-40 |
| `to_uint64_checked` | `uint64.from_checked` | R-L0-40 |
| `to_uint64_saturating` | `uint64.from_saturating` | R-L0-40 |
| `to_uint8` | `uint8.from` | R-L0-40 |
| `to_uint8_checked` | `uint8.from_checked` | R-L0-40 |
| `to_uint8_saturating` | `uint8.from_saturating` | R-L0-40 |

<a id="float32"></a>

## float32

Record: [L2-sized-numbers.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-sized-numbers.md)

| Function | Signature (after redesign) | Change | Proposal |
| --- | --- | --- | --- |
| `abs` | `fn float32.abs(x: Float32) -> Float32` | New | R-sized-11 |
| `ceil` | `fn float32.ceil(x: Float32) -> Float32` | New | R-sized-11 |
| `clamp` | `fn float32.clamp(x: Float32, lo: Float32, hi: Float32) -> Float32` | New | R-sized-08 |
| `compare` | `fn float32.compare(a: Float32, b: Float32) -> Int` | New | R-sized-02 |
| `epsilon` | `fn float32.epsilon() -> Float32` | New | R-sized-03 |
| `floor` | `fn float32.floor(x: Float32) -> Float32` | New | R-sized-11 |
| `from` | `fn float32.from[S: Numeric](x: S) -> Float32` | New | R-L0-40 |
| `from_bits` | `fn float32.from_bits(bits: Int) -> Float32` | New | R-sized-03 |
| `from_checked` | `fn float32.from_checked[S: Numeric](x: S) -> Float32?` | New | R-L0-40 |
| `infinity` | `fn float32.infinity() -> Float32` | New | R-sized-03 |
| `is_finite` | `fn float32.is_finite(x: Float32) -> Bool` | New | R-sized-08 |
| `is_infinite` | `fn float32.is_infinite(x: Float32) -> Bool` | New | R-sized-08 |
| `is_nan` | `fn float32.is_nan(x: Float32) -> Bool` | New | R-sized-08 |
| `max` | `fn float32.max(a: Float32, b: Float32) -> Float32` | New | R-sized-08 |
| `max_value` | `fn float32.max_value() -> Float32` | New | R-sized-03 |
| `min` | `fn float32.min(a: Float32, b: Float32) -> Float32` | New | R-sized-08 |
| `min_value` | `fn float32.min_value() -> Float32` | New | R-sized-03 |
| `nan` | `fn float32.nan() -> Float32` | New | R-sized-03 |
| `parse` | `fn float32.parse(s: String) -> Result[Float32, String]` | New | R-sized-02 |
| `round` | `fn float32.round(x: Float32) -> Float32` | New | R-sized-11 |
| `signum` | `fn float32.signum(x: Float32) -> Float32` | New | R-sized-11 |
| `to_bits` | `fn float32.to_bits(x: Float32) -> Int` | New | R-sized-03 |
| `to_string` | `fn float32.to_string(x: Float32) -> String` |  |  |
| `trunc` | `fn float32.trunc(x: Float32) -> Float32` | New | R-sized-11 |

Removed (9):

| Function | Replacement | Proposal |
| --- | --- | --- |
| `to_float64` | `float64.from` | R-L0-40 |
| `to_int16` | `int16.from` | R-L0-40 |
| `to_int32` | `int32.from` | R-L0-40 |
| `to_int64` | `int64.from` | R-L0-40 |
| `to_int8` | `int8.from` | R-L0-40 |
| `to_uint16` | `uint16.from` | R-L0-40 |
| `to_uint32` | `uint32.from` | R-L0-40 |
| `to_uint64` | `uint64.from` | R-L0-40 |
| `to_uint8` | `uint8.from` | R-L0-40 |

<a id="float64"></a>

## float64

Record: [L2-sized-numbers.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-sized-numbers.md)

| Function | Signature (after redesign) | Change | Proposal |
| --- | --- | --- | --- |
| `abs` | `fn float64.abs(x: Float64) -> Float64` | New | R-sized-11 |
| `ceil` | `fn float64.ceil(x: Float64) -> Float64` | New | R-sized-11 |
| `clamp` | `fn float64.clamp(x: Float64, lo: Float64, hi: Float64) -> Float64` | New | R-sized-08 |
| `compare` | `fn float64.compare(a: Float64, b: Float64) -> Int` | New | R-sized-02 |
| `epsilon` | `fn float64.epsilon() -> Float64` | New | R-sized-03 |
| `floor` | `fn float64.floor(x: Float64) -> Float64` | New | R-sized-11 |
| `from` | `fn float64.from[S: Numeric](x: S) -> Float64` | New | R-L0-40 |
| `from_bits` | `fn float64.from_bits(bits: Int) -> Float64` | New | R-sized-03 |
| `from_checked` | `fn float64.from_checked[S: Numeric](x: S) -> Float64?` | New | R-L0-40 |
| `infinity` | `fn float64.infinity() -> Float64` | New | R-sized-03 |
| `is_finite` | `fn float64.is_finite(x: Float64) -> Bool` | New | R-sized-08 |
| `is_infinite` | `fn float64.is_infinite(x: Float64) -> Bool` | New | R-sized-08 |
| `is_nan` | `fn float64.is_nan(x: Float64) -> Bool` | New | R-sized-08 |
| `max` | `fn float64.max(a: Float64, b: Float64) -> Float64` | New | R-sized-08 |
| `max_value` | `fn float64.max_value() -> Float64` | New | R-sized-03 |
| `min` | `fn float64.min(a: Float64, b: Float64) -> Float64` | New | R-sized-08 |
| `min_value` | `fn float64.min_value() -> Float64` | New | R-sized-03 |
| `nan` | `fn float64.nan() -> Float64` | New | R-sized-03 |
| `parse` | `fn float64.parse(s: String) -> Result[Float64, String]` | New | R-sized-02 |
| `round` | `fn float64.round(x: Float64) -> Float64` | New | R-sized-11 |
| `signum` | `fn float64.signum(x: Float64) -> Float64` | New | R-sized-11 |
| `to_bits` | `fn float64.to_bits(x: Float64) -> Int` | New | R-sized-03 |
| `to_string` | `fn float64.to_string(x: Float64) -> String` |  |  |
| `trunc` | `fn float64.trunc(x: Float64) -> Float64` | New | R-sized-11 |

Removed (9):

| Function | Replacement | Proposal |
| --- | --- | --- |
| `to_float32` | `float32.from` | R-L0-40 |
| `to_int16` | `int16.from` | R-L0-40 |
| `to_int32` | `int32.from` | R-L0-40 |
| `to_int64` | `int64.from` | R-L0-40 |
| `to_int8` | `int8.from` | R-L0-40 |
| `to_uint16` | `uint16.from` | R-L0-40 |
| `to_uint32` | `uint32.from` | R-L0-40 |
| `to_uint64` | `uint64.from` | R-L0-40 |
| `to_uint8` | `uint8.from` | R-L0-40 |

<a id="fs"></a>

## fs

Record: [L2-fs.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-fs.md)

| Function | Signature (after redesign) | Change | Proposal |
| --- | --- | --- | --- |
| `append_bytes` | `effect fn fs.append_bytes(path: String, data: Bytes) -> Unit` | New | R-fs-05 |
| `append_text` | `effect fn fs.append_text(path: String, content: String) -> Unit` | Renamed (formerly `append`) | R-fs-04、R-fs-05 |
| `copy` | `effect fn fs.copy(src: String, dst: String) -> Unit` |  |  |
| `create_temp_dir` | `effect fn fs.create_temp_dir(prefix: String) -> String` |  |  |
| `create_temp_file` | `effect fn fs.create_temp_file(prefix: String) -> String` |  |  |
| `exists` | `effect fn fs.exists(path: String) -> Bool` |  |  |
| `fold_lines` | `effect fn fs.fold_lines[A](path: String, init: A, f: (A, String) -> A) -> A` |  |  |
| `fold_lines_chunked` | `effect fn fs.fold_lines_chunked[A](path: String, workers: Int, init: A, f: (A, String) -> A) -> List[A]` |  |  |
| `fold_lines_range` | `effect fn fs.fold_lines_range[A](path: String, start: Int, end: Int, init: A, f: (A, String) -> A) -> A` | Semantic change: negative positions abort | R-L0-29、R-L0-46 |
| `for_each_line` | `effect fn fs.for_each_line(path: String, f: (String) -> Unit) -> Unit` |  |  |
| `glob` | `effect fn fs.glob(pattern: String) -> List[String]` |  |  |
| `is_dir` | `effect fn fs.is_dir(path: String) -> Bool` |  |  |
| `is_file` | `effect fn fs.is_file(path: String) -> Bool` |  |  |
| `is_symlink` | `effect fn fs.is_symlink(path: String) -> Bool` |  |  |
| `list_dir` | `effect fn fs.list_dir(path: String) -> List[String]` |  |  |
| `mkdir_p` | `effect fn fs.mkdir_p(path: String) -> Unit` |  | R-fs-04 |
| `read_bytes` | `effect fn fs.read_bytes(path: String) -> Bytes` | Semantic change: use `-> Bytes`. Sole byte read | R-fs-01 |
| `read_bytes_if_exists` | `effect fn fs.read_bytes_if_exists(path: String) -> Bytes?` | Semantic change: use `-> Bytes?` | R-fs-01・R-fs-03 |
| `read_lines` | `effect fn fs.read_lines(path: String) -> List[String]` |  |  |
| `read_lines_if_exists` | `effect fn fs.read_lines_if_exists(path: String) -> List[String]?` |  | R-fs-03 |
| `read_text` | `effect fn fs.read_text(path: String) -> String` |  |  |
| `read_text_if_exists` | `effect fn fs.read_text_if_exists(path: String) -> String?` |  | R-fs-03 |
| `remove` | `effect fn fs.remove(path: String) -> Unit` |  |  |
| `remove_all` | `effect fn fs.remove_all(path: String) -> Unit` |  |  |
| `rename` | `effect fn fs.rename(src: String, dst: String) -> Unit` |  |  |
| `stat` | `effect fn fs.stat(path: String) -> FileStat` |  | R-fs-02 |
| `temp_dir` | `effect fn fs.temp_dir() -> String` |  | L2-env R-env-01 |
| `walk` | `effect fn fs.walk(dir: String) -> List[String]` |  |  |
| `write_bytes` | `effect fn fs.write_bytes(path: String, data: Bytes) -> Unit` | Semantic change: use `data: Bytes` | R-fs-01 |
| `write_text` | `effect fn fs.write_text(path: String, content: String) -> Unit` | Renamed (formerly `write`) | R-fs-05 |

Removed (5):

| Function | Replacement | Proposal |
| --- | --- | --- |
| `file_size` | `stat(p)!.size` | R-fs-02 |
| `modified_at` | `stat(p)!.modified` | R-fs-02 |
| `read_bytes_raw` | `read_bytes` | R-fs-01 |
| `read_bytes_raw_if_exists` | `read_bytes_if_exists` | R-fs-01・R-fs-03 |
| `write_bytes_raw` | `write_bytes` | R-fs-01 |

<a id="hash"></a>

## hash

Record: [L2-hash.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-hash.md)

| Function | Signature (after redesign) | Change | Proposal |
| --- | --- | --- | --- |
| `adler32` | `fn hash.adler32(b: Bytes) -> Int` | New | R-hash-02 |
| `crc32` | `fn hash.crc32(b: Bytes) -> Int` | New | R-hash-02 |
| `fnv1a32` | `fn hash.fnv1a32(b: Bytes) -> Int` | Semantic change: argument becomes `Bytes`. `fnv1a32(bytes.from_string(s))` | R-hash-01 |
| `hmac_sha256` | `fn hash.hmac_sha256(key: Secret[Bytes], b: Bytes) -> Secret[Bytes]` | New | R-hash-06、R-hash-07 |
| `hmac_sha512` | `fn hash.hmac_sha512(key: Secret[Bytes], b: Bytes) -> Secret[Bytes]` | New | R-hash-06、R-hash-07 |
| `is_valid_hmac_sha256` | `fn hash.is_valid_hmac_sha256(key: Secret[Bytes], b: Bytes, tag: Secret[Bytes]) -> Bool` | New | R-hash-06、R-hash-07 |
| `is_valid_hmac_sha512` | `fn hash.is_valid_hmac_sha512(key: Secret[Bytes], b: Bytes, tag: Secret[Bytes]) -> Bool` | New | R-hash-06、R-hash-07 |
| `sha256` | `fn hash.sha256(b: Bytes) -> Bytes` |  |  |
| `sha256_hex` | `fn hash.sha256_hex(b: Bytes) -> String` | Semantic change: accepts `Bytes` | R-hash-05 |
| `sha512` | `fn hash.sha512(b: Bytes) -> Bytes` | New | R-hash-03 |
| `sha512_hex` | `fn hash.sha512_hex(b: Bytes) -> String` | New. Accepts `Bytes` | R-hash-03、R-hash-05 |

Removed (1):

| Function | Replacement | Proposal |
| --- | --- | --- |
| `fnv1a32_bytes` | `fnv1a32` | R-hash-01 |

Not added (R-hash-07): `md5`/`md5_hex`/`sha1`/`sha1_hex` (digests unsuitable for security; removed at epoch 15, use `sha256`), and HMAC `_hex` cells `hmac_sha256_hex`/`hmac_sha512_hex` (hex uses `hex.encode(secret.reveal(tag))`).

<a id="heap"></a>

## heap

New module (added at L1).
Record: [L2-heap.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-heap.md)

| Function | Signature (after redesign) | Change | Proposal |
| --- | --- | --- | --- |
| `all` | `fn heap.all[A](h: Heap[A], f: (A) -> Bool) -> Bool` | New | R-heap-02 |
| `any` | `fn heap.any[A](h: Heap[A], f: (A) -> Bool) -> Bool` | New | R-heap-02 |
| `clear` | `fn heap.clear[A](mut h: Heap[A]) -> Unit` | New | R-heap-02、R-L0-04 |
| `contains` | `fn heap.contains[A](h: Heap[A], x: A) -> Bool` | New | R-heap-02 |
| `count` | `fn heap.count[A](h: Heap[A], f: (A) -> Bool) -> Int` | New | R-heap-02 |
| `filter` | `fn heap.filter[A](h: Heap[A], f: (A) -> Bool) -> Heap[A]` | New | R-heap-02 |
| `from_list` | `fn heap.from_list[A](xs: List[A]) -> Heap[A]` | New | R-heap-01 |
| `is_empty` | `fn heap.is_empty[A](h: Heap[A]) -> Bool` | New | R-heap-01 |
| `len` | `fn heap.len[A](h: Heap[A]) -> Int` | New | R-heap-01 |
| `new` | `fn heap.new[A]() -> Heap[A]` | New | R-heap-01 |
| `new_by` | `fn heap.new_by[A, K](f: (A) -> K) -> Heap[A]` | New | R-heap-01 |
| `new_with` | `fn heap.new_with[A](cmp: (A, A) -> Int) -> Heap[A]` | New | R-heap-01 |
| `peek` | `fn heap.peek[A](h: Heap[A]) -> A?` | New | R-heap-01 |
| `pop` | `fn heap.pop[A](mut h: Heap[A]) -> A?` | New | R-heap-01 |
| `push` | `fn heap.push[A](mut h: Heap[A], x: A) -> Unit` | New | R-heap-01 |
| `to_list` | `fn heap.to_list[A](h: Heap[A]) -> List[A]` | New | R-heap-01 |

<a id="hex"></a>

## hex

Record: [L2-hex.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-hex.md)

| Function | Signature (after redesign) | Change | Proposal |
| --- | --- | --- | --- |
| `decode` | `fn hex.decode(s: String) -> Result[Bytes, String]` |  | R-hex-01 |
| `encode` | `fn hex.encode(b: Bytes) -> String` |  | R-hex-01 |
| `encode_upper` | `fn hex.encode_upper(b: Bytes) -> String` |  | R-hex-01 |

<a id="html"></a>

## html

Record: [L2-html.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-html.md)

| Function | Signature (after redesign) | Change | Proposal |
| --- | --- | --- | --- |
| `concat` | `fn html.concat(a: SafeHtml, b: SafeHtml) -> SafeHtml` |  | R-html-01 |
| `empty` | `fn html.empty() -> SafeHtml` |  | R-html-01 |
| `escape` | `fn html.escape(s: String) -> SafeHtml` |  | R-html-01 |
| `raw` | `fn html.raw(s: String) -> SafeHtml` |  | R-html-01 |
| `to_string` | `fn html.to_string(h: SafeHtml) -> String` |  | R-html-01 |

<a id="http"></a>

## http

Record: [L2-http.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-http.md)

| Function | Signature (after redesign) | Change | Proposal |
| --- | --- | --- | --- |
| `body` | `fn http.body[M: HttpMessage](m: M) -> String` | Semantic change: also accepts `HttpRequest` | R-http-12 |
| `body_bytes` | `fn http.body_bytes[M: HttpMessage](m: M) -> Bytes` | New. Also accepts `HttpRequest` | R-http-12 |
| `cancel` | `effect fn http.cancel(c: HttpCall) -> Unit` |  | R-http-01 |
| `decode_json` | `fn http.decode_json[A: Decode](req: HttpRequest) -> Result[A, HttpResponse]` | Type argument `A` is the input type (record annotated only with `: Decode`). Types with `Codec` yield E110. Read the body with `A.decode_strict`; keys outside the type give `400 Bad Request: unknown key '<k>'` (decode-function argument removed) | Row 22, C-436 |
| `get` | `effect fn http.get(url: String) -> String` |  | R-http-01 |
| `get_response` | `effect fn http.get_response(url: String) -> HttpResponse` |  | R-http-01 |
| `header` | `fn http.header[M: HttpMessage](m: M, name: String) -> String?` | Renamed (formerly `get_header`). Also accepts `HttpRequest` | R-http-12 |
| `header_values` | `fn http.header_values[M: HttpMessage](m: M, name: String) -> List[String]` | Semantic change: also accepts `HttpRequest` | R-http-12 |
| `headers` | `fn http.headers[M: HttpMessage](m: M) -> List[(String, String)]` | Semantic change: also accepts `HttpRequest` | R-http-12 |
| `json` | `fn http.json(status: Int, body: String) -> HttpResponse` |  |  |
| `method` | `fn http.method(req: HttpRequest) -> String` | Renamed (formerly `req_method`) | R-http-14 |
| `mount` | `fn http.mount(prefix: String, sub: (HttpRequest) -> Result[HttpResponse, String]) -> HttpRoute` |  |  |
| `new_request` | `fn http.new_request(method: String, target: String, body: String, headers: List[(String, String)]) -> HttpRequest` | Semantic change: `headers` becomes `List[(String, String)]` | R-http-09 |
| `param` | `fn http.param(req: HttpRequest, name: String) -> String?` |  |  |
| `path` | `fn http.path(req: HttpRequest) -> String` | Renamed (formerly `req_path`). Document explicitly that it includes the query | R-http-14 |
| `poll` | `effect fn http.poll(c: HttpCall) -> Result[HttpResponse, String]?` |  | R-http-01 |
| `query_params` | `fn http.query_params(req: HttpRequest) -> Result[List[(String, String)], String]` | Semantic change: same single decoder as `url.parse_query` (invalid escapes error; #4117, C-404) | R-http-10 |
| `read_new` | `effect fn http.read_new(c: HttpCall) -> String` |  | R-http-01 |
| `redirect` | `fn http.redirect(url: String) -> HttpResponse` |  |  |
| `request` | `effect fn http.request(method: String, url: String, body: String, headers: List[(String, String)]) -> String` | Semantic change: `headers` becomes `List[(String, String)]` | R-http-09 |
| `response` | `fn http.response[B: Body](status: Int, body: B) -> HttpResponse` | Semantic change: `body` is `Body` (ADR-0032) | ADR-0032 |
| `route` | `fn http.route(pattern: String, handler: HttpHandler) -> HttpRoute` |  |  |
| `router` | `fn http.router(xs: List[HttpRoute]) -> Result[HttpHandler, String]` |  |  |
| `send` | `effect fn http.send(req: HttpRequest) -> HttpResponse` | New | R-http-13 |
| `serve` | `effect fn http.serve(port: Int, handler: HttpHandler) -> Unit` |  |  |
| `set_body` | `fn http.set_body[M: HttpMessage](m: M, body: String) -> M` | New. Also accepts `HttpRequest` | R-http-12 |
| `set_body_bytes` | `fn http.set_body_bytes[M: HttpMessage](m: M, body: Bytes) -> M` | New. Also accepts `HttpRequest` | R-http-12 |
| `set_header` | `fn http.set_header[M: HttpMessage](m: M, name: String, value: String) -> M` | Semantic change: also accepts `HttpRequest` | R-http-12 |
| `set_status` | `fn http.set_status(resp: HttpResponse, code: Int) -> HttpResponse` | Renamed (formerly `status`). `set_status` | R-http-02 |
| `start` | `effect fn http.start(req: HttpRequest, limits: HttpLimits) -> HttpCall` | Semantic change: use `start(req: HttpRequest, limits: HttpLimits)` | R-http-09、R-http-14 |
| `status` | `fn http.status(resp: HttpResponse) -> Int` | Renamed (formerly `status_code`). Epoch 17. Also remove `0` sentinel | R-http-04、R-http-08 |
| `wait` | `effect fn http.wait(c: HttpCall) -> HttpResponse` |  | R-http-01 |
| `with_headers` | `fn http.with_headers[B: Body](status: Int, body: B, headers: List[(String, String)]) -> HttpResponse` | Semantic change: `headers` becomes `List[(String, String)]` | R-http-09 |
| `wrap` | `fn http.wrap(handler: HttpHandler, middleware: List[HttpMiddleware]) -> HttpHandler` |  |  |

Removed (18):

| Function | Replacement | Proposal |
| --- | --- | --- |
| `delete` | `request("DELETE", …)` | R-http-01 |
| `delete_response` | `send(new_request("DELETE", …))` | R-http-01 |
| `get_bytes` | `body_bytes(get_response(u)!)` | R-http-01 |
| `get_status` | `get_response` | R-http-01 |
| `patch` | `request("PATCH", …)` | R-http-01 |
| `patch_response` | `send(new_request("PATCH", …))` | R-http-01 |
| `post` | `request("POST", …)` | R-http-01 |
| `post_response` | `send(new_request("POST", …))` | R-http-01 |
| `put` | `request("PUT", …)` | R-http-01 |
| `put_response` | `send(new_request("PUT", …))` | R-http-01 |
| `req_body` | `http.body` | R-http-12 |
| `req_header` | `http.header` | R-http-12 |
| `request_bytes` | `body_bytes(send(new_request(…))!)` | R-http-01 |
| `request_response` | `send(new_request(method, url, body, headers))` | R-http-14 |
| `request_status` | `send` | R-http-01 |
| `request_stream` | Handle | R-http-01 |
| `request_stream_with_limits` | Handle | R-http-01 |
| `url_decode` | `url.decode_form(s) ?? s` | R-http-04 |

<a id="int"></a>

## int

Record: [L2-int.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-int.md)

| Function | Signature (after redesign) | Change | Proposal |
| --- | --- | --- | --- |
| `abs` | `fn int.abs(x: Int) -> Int` |  | R-math-01 (retained side of `math.abs`) |
| `band` | `fn int.band(a: Int, b: Int) -> Int` |  | R-int-05 × (do not rename) |
| `bit_length` | `fn int.bit_length(x: Int) -> Int` | Renamed (formerly `bit_width`) | R-int-11 |
| `bit_reverse` | `fn int.bit_reverse(x: Int) -> Int` |  |  |
| `bnot` | `fn int.bnot(x: Int) -> Int` |  | R-int-05 × |
| `bor` | `fn int.bor(a: Int, b: Int) -> Int` |  | R-int-05 × |
| `bshl` | `fn int.bshl(a: Int, n: Int) -> Int` |  | R-int-05 × |
| `bshr` | `fn int.bshr(a: Int, n: Int) -> Int` |  | R-int-05 × |
| `bshr_logical` | `fn int.bshr_logical(a: Int, n: Int) -> Int` | New | R-int-06 |
| `bxor` | `fn int.bxor(a: Int, b: Int) -> Int` |  | R-int-05 × |
| `byte_swap` | `fn int.byte_swap(x: Int) -> Int` |  |  |
| `clamp` | `fn int.clamp(x: Int, lo: Int, hi: Int) -> Int` |  |  |
| `compare` | `fn int.compare(a: Int, b: Int) -> Int` | New | R-sized-02 |
| `count_leading_zeros` | `fn int.count_leading_zeros(x: Int) -> Int` |  |  |
| `count_ones` | `fn int.count_ones(x: Int) -> Int` | Renamed (formerly `pop_count`) | R-int-07 |
| `count_trailing_zeros` | `fn int.count_trailing_zeros(x: Int) -> Int` |  |  |
| `floor_div` | `fn int.floor_div(a: Int, b: Int) -> Int` | New | R-int-03 |
| `from` | `fn int.from[S: Numeric](x: S) -> Int` | New | R-L0-40 |
| `from_checked` | `fn int.from_checked[S: Numeric](x: S) -> Int?` | New | R-L0-40 |
| `from_saturating` | `fn int.from_saturating[S: Numeric](x: S) -> Int` | New | R-L0-40 |
| `from_wrapping` | `fn int.from_wrapping[S: Integer](x: S) -> Int` | New | R-L0-40 |
| `is_even` | `fn int.is_even(x: Int) -> Bool` |  |  |
| `is_odd` | `fn int.is_odd(x: Int) -> Bool` |  |  |
| `log2_ceil` | `fn int.log2_ceil(x: Int) -> Int` |  |  |
| `log2_floor` | `fn int.log2_floor(x: Int) -> Int` |  |  |
| `max` | `fn int.max(a: Int, b: Int) -> Int` |  | R-math-01 (retained side of `math.max`) |
| `max_value` | `fn int.max_value() -> Int` |  |  |
| `min` | `fn int.min(a: Int, b: Int) -> Int` |  | R-math-01 (retained side of `math.min`) |
| `min_value` | `fn int.min_value() -> Int` |  |  |
| `modulo` | `fn int.modulo(a: Int, b: Int) -> Int` | New | R-int-03 |
| `next_power_of_two` | `fn int.next_power_of_two(x: Int) -> Int` |  |  |
| `parse` | `fn int.parse(s: String) -> Result[Int, String]` |  |  |
| `parse_radix` | `fn int.parse_radix(s: String, radix: Int) -> Result[Int, String]` | New | R-int-04 |
| `pow` | `fn int.pow(base: Int, exp: Int) -> Int` | New | R-math-01 |
| `prev_power_of_two` | `fn int.prev_power_of_two(x: Int) -> Int` |  |  |
| `rotate_left` | `fn int.rotate_left(a: Int, n: Int) -> Int` | Semantic change: remove `bits`; rotate at 64 bits | R-int-12 |
| `rotate_right` | `fn int.rotate_right(a: Int, n: Int) -> Int` | Semantic change: remove `bits`; rotate at 64 bits | R-int-12 |
| `sign` | `fn int.sign(x: Int) -> Int` | New | R-math-01 |
| `to_hex` | `fn int.to_hex(x: Int) -> String` |  | R-int-04 (hexadecimal 64-bit pattern; signed radix conversion uses `to_string_radix`) |
| `to_string` | `fn int.to_string(x: Int) -> String` |  |  |
| `to_string_radix` | `fn int.to_string_radix(x: Int, radix: Int) -> String` | New | R-int-04 |
| `wrap` | `fn int.wrap(x: Int, bits: Int) -> Int` | New | R-int-02 |
| `wrap_add` | `fn int.wrap_add(a: Int, b: Int) -> Int` | Semantic change: remove `bits`, wrap at 64 bits. Combine with `int.wrap` for N bits | R-int-12 |
| `wrap_mul` | `fn int.wrap_mul(a: Int, b: Int) -> Int` | Semantic change: remove `bits`, wrap at 64 bits. Combine with `int.wrap` for N bits | R-int-12 |
| `wrap_sub` | `fn int.wrap_sub(a: Int, b: Int) -> Int` | New | R-int-02、R-int-12 |

Removed (41):

| Function | Replacement | Proposal |
| --- | --- | --- |
| `bits_to_f32` | `float32.from_bits` → `float32.to_float` | R-float-02（`float32.from_bits` → `float32.to_float`） |
| `bits_to_float` | `float.from_bits` | R-float-02（`float.from_bits`） |
| `from_hex` | Consolidate into `int.parse_radix(s, 16)` | R-int-04 (consolidate into `int.parse_radix(s, 16)`) |
| `from_int16` | Move to `int16.to_int` | R-sized-01 (move to `int16.to_int`; R-L0-07) |
| `from_int32` | Move to `int32.to_int` | R-sized-01 (move to `int32.to_int`; R-L0-07) |
| `from_int64` | Move to `int64.to_int` | R-sized-01 (move to `int64.to_int`; R-L0-07) |
| `from_int8` | Move to `int8.to_int` | R-sized-01 (move to `int8.to_int`; R-L0-07) |
| `from_uint16` | Move to `uint16.to_int` | R-sized-01 (move to `uint16.to_int`; R-L0-07) |
| `from_uint32` | Move to `uint32.to_int` | R-sized-01 (move to `uint32.to_int`; R-L0-07) |
| `from_uint64` | Move to `uint64.to_int` | R-sized-01 (move to `uint64.to_int`; R-L0-07) |
| `from_uint64_checked` | `uint64.to_int_checked` | R-sized-01（`uint64.to_int_checked`） |
| `from_uint64_saturating` | `uint64.to_int_saturating` | R-sized-01（`uint64.to_int_saturating`） |
| `from_uint8` | Move to `uint8.to_int` | R-sized-01 (move to `uint8.to_int`; R-L0-07) |
| `to_float` | `float.from` | R-L0-40 |
| `to_float32` | `float32.from` | R-L0-40 |
| `to_float32_checked` | `float32.from_checked` | R-L0-40 |
| `to_float64` | `float64.from` | R-L0-40 |
| `to_int16` | `int16.from` | R-L0-40 |
| `to_int16_checked` | `int16.from_checked` | R-L0-40 |
| `to_int16_saturating` | `int16.from_saturating` | R-L0-40 |
| `to_int32` | `int32.from` | R-L0-40 |
| `to_int32_checked` | `int32.from_checked` | R-L0-40 |
| `to_int32_saturating` | `int32.from_saturating` | R-L0-40 |
| `to_int64` | `int64.from` | R-L0-40 |
| `to_int8` | `int8.from` | R-L0-40 |
| `to_int8_checked` | `int8.from_checked` | R-L0-40 |
| `to_int8_saturating` | `int8.from_saturating` | R-L0-40 |
| `to_u32` | `int.wrap(x, 32)` | R-int-02（`int.wrap(x, 32)`。#3851 E） |
| `to_u8` | `int.wrap(x, 8)` | R-int-02（`int.wrap(x, 8)`。#3851 E） |
| `to_uint16` | `uint16.from` | R-L0-40 |
| `to_uint16_checked` | `uint16.from_checked` | R-L0-40 |
| `to_uint16_saturating` | `uint16.from_saturating` | R-L0-40 |
| `to_uint32` | `uint32.from` | R-L0-40 |
| `to_uint32_checked` | `uint32.from_checked` | R-L0-40 |
| `to_uint32_saturating` | `uint32.from_saturating` | R-L0-40 |
| `to_uint64` | `uint64.from` | R-L0-40 |
| `to_uint64_checked` | `uint64.from_checked` | R-L0-40 |
| `to_uint64_saturating` | `uint64.from_saturating` | R-L0-40 |
| `to_uint8` | `uint8.from` | R-L0-40 |
| `to_uint8_checked` | `uint8.from_checked` | R-L0-40 |
| `to_uint8_saturating` | `uint8.from_saturating` | R-L0-40 |

<a id="int16"></a>

## int16

Record: [L2-sized-numbers.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-sized-numbers.md)

| Function | Signature (after redesign) | Change | Proposal |
| --- | --- | --- | --- |
| `abs` | `fn int16.abs(x: Int16) -> Int16` | New | R-sized-10 |
| `band` | `fn int16.band(a: Int16, b: Int16) -> Int16` | New | R-sized-10 |
| `bit_length` | `fn int16.bit_length(x: Int16) -> Int` | New | R-int-11 |
| `bnot` | `fn int16.bnot(x: Int16) -> Int16` | New | R-sized-10 |
| `bor` | `fn int16.bor(a: Int16, b: Int16) -> Int16` | New | R-sized-10 |
| `bshl` | `fn int16.bshl(a: Int16, n: Int) -> Int16` | New | R-sized-10 |
| `bshr` | `fn int16.bshr(a: Int16, n: Int) -> Int16` | New | R-sized-10 |
| `bxor` | `fn int16.bxor(a: Int16, b: Int16) -> Int16` | New | R-sized-10 |
| `byte_swap` | `fn int16.byte_swap(x: Int16) -> Int16` | New | R-sized-10 |
| `clamp` | `fn int16.clamp(x: Int16, lo: Int16, hi: Int16) -> Int16` | New | R-sized-08 |
| `compare` | `fn int16.compare(a: Int16, b: Int16) -> Int` | New | R-sized-02 |
| `count_leading_zeros` | `fn int16.count_leading_zeros(x: Int16) -> Int` | New | R-sized-10 |
| `count_ones` | `fn int16.count_ones(x: Int16) -> Int` | New | R-sized-10 |
| `count_trailing_zeros` | `fn int16.count_trailing_zeros(x: Int16) -> Int` | New | R-sized-10 |
| `floor_div` | `fn int16.floor_div(a: Int16, b: Int16) -> Int16` | New | R-sized-10 |
| `from` | `fn int16.from[S: Numeric](x: S) -> Int16` | New | R-L0-40 |
| `from_checked` | `fn int16.from_checked[S: Numeric](x: S) -> Int16?` | New | R-L0-40 |
| `from_saturating` | `fn int16.from_saturating[S: Numeric](x: S) -> Int16` | New | R-L0-40 |
| `from_wrapping` | `fn int16.from_wrapping[S: Integer](x: S) -> Int16` | New | R-L0-40 |
| `is_even` | `fn int16.is_even(x: Int16) -> Bool` | New | R-sized-10 |
| `is_odd` | `fn int16.is_odd(x: Int16) -> Bool` | New | R-sized-10 |
| `max` | `fn int16.max(a: Int16, b: Int16) -> Int16` | New | R-sized-08 |
| `max_value` | `fn int16.max_value() -> Int16` |  |  |
| `min` | `fn int16.min(a: Int16, b: Int16) -> Int16` | New | R-sized-08 |
| `min_value` | `fn int16.min_value() -> Int16` |  |  |
| `modulo` | `fn int16.modulo(a: Int16, b: Int16) -> Int16` | New | R-sized-10 |
| `parse` | `fn int16.parse(s: String) -> Result[Int16, String]` | New | R-sized-02 |
| `parse_radix` | `fn int16.parse_radix(s: String, radix: Int) -> Result[Int16, String]` | New | R-sized-06 |
| `pow` | `fn int16.pow(base: Int16, exp: Int) -> Int16` | New | R-sized-10 |
| `rotate_left` | `fn int16.rotate_left(a: Int16, n: Int) -> Int16` | New | R-int-12 |
| `rotate_right` | `fn int16.rotate_right(a: Int16, n: Int) -> Int16` | New | R-int-12 |
| `sign` | `fn int16.sign(x: Int16) -> Int` | New | R-sized-10 |
| `to_string` | `fn int16.to_string(x: Int16) -> String` |  |  |
| `to_string_radix` | `fn int16.to_string_radix(x: Int16, radix: Int) -> String` | New | R-sized-06 |
| `wrap_add` | `fn int16.wrap_add(a: Int16, b: Int16) -> Int16` | New | R-int-12 |
| `wrap_mul` | `fn int16.wrap_mul(a: Int16, b: Int16) -> Int16` | New | R-int-12 |
| `wrap_sub` | `fn int16.wrap_sub(a: Int16, b: Int16) -> Int16` | New | R-int-12 |

Removed (19):

| Function | Replacement | Proposal |
| --- | --- | --- |
| `to_float32` | `float32.from` | R-L0-40 |
| `to_float64` | `float64.from` | R-L0-40 |
| `to_int32` | `int32.from` | R-L0-40 |
| `to_int64` | `int64.from` | R-L0-40 |
| `to_int8` | `int8.from` | R-L0-40 |
| `to_int8_checked` | `int8.from_checked` | R-L0-40 |
| `to_int8_saturating` | `int8.from_saturating` | R-L0-40 |
| `to_uint16` | `uint16.from` | R-L0-40 |
| `to_uint16_checked` | `uint16.from_checked` | R-L0-40 |
| `to_uint16_saturating` | `uint16.from_saturating` | R-L0-40 |
| `to_uint32` | `uint32.from` | R-L0-40 |
| `to_uint32_checked` | `uint32.from_checked` | R-L0-40 |
| `to_uint32_saturating` | `uint32.from_saturating` | R-L0-40 |
| `to_uint64` | `uint64.from` | R-L0-40 |
| `to_uint64_checked` | `uint64.from_checked` | R-L0-40 |
| `to_uint64_saturating` | `uint64.from_saturating` | R-L0-40 |
| `to_uint8` | `uint8.from` | R-L0-40 |
| `to_uint8_checked` | `uint8.from_checked` | R-L0-40 |
| `to_uint8_saturating` | `uint8.from_saturating` | R-L0-40 |

<a id="int32"></a>

## int32

Record: [L2-sized-numbers.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-sized-numbers.md)

| Function | Signature (after redesign) | Change | Proposal |
| --- | --- | --- | --- |
| `abs` | `fn int32.abs(x: Int32) -> Int32` | New | R-sized-10 |
| `band` | `fn int32.band(a: Int32, b: Int32) -> Int32` | New | R-sized-10 |
| `bit_length` | `fn int32.bit_length(x: Int32) -> Int` | New | R-int-11 |
| `bnot` | `fn int32.bnot(x: Int32) -> Int32` | New | R-sized-10 |
| `bor` | `fn int32.bor(a: Int32, b: Int32) -> Int32` | New | R-sized-10 |
| `bshl` | `fn int32.bshl(a: Int32, n: Int) -> Int32` | New | R-sized-10 |
| `bshr` | `fn int32.bshr(a: Int32, n: Int) -> Int32` | New | R-sized-10 |
| `bxor` | `fn int32.bxor(a: Int32, b: Int32) -> Int32` | New | R-sized-10 |
| `byte_swap` | `fn int32.byte_swap(x: Int32) -> Int32` | New | R-sized-10 |
| `clamp` | `fn int32.clamp(x: Int32, lo: Int32, hi: Int32) -> Int32` | New | R-sized-08 |
| `compare` | `fn int32.compare(a: Int32, b: Int32) -> Int` | New | R-sized-02 |
| `count_leading_zeros` | `fn int32.count_leading_zeros(x: Int32) -> Int` | New | R-sized-10 |
| `count_ones` | `fn int32.count_ones(x: Int32) -> Int` | New | R-sized-10 |
| `count_trailing_zeros` | `fn int32.count_trailing_zeros(x: Int32) -> Int` | New | R-sized-10 |
| `floor_div` | `fn int32.floor_div(a: Int32, b: Int32) -> Int32` | New | R-sized-10 |
| `from` | `fn int32.from[S: Numeric](x: S) -> Int32` | New | R-L0-40 |
| `from_checked` | `fn int32.from_checked[S: Numeric](x: S) -> Int32?` | New | R-L0-40 |
| `from_saturating` | `fn int32.from_saturating[S: Numeric](x: S) -> Int32` | New | R-L0-40 |
| `from_wrapping` | `fn int32.from_wrapping[S: Integer](x: S) -> Int32` | New | R-L0-40 |
| `is_even` | `fn int32.is_even(x: Int32) -> Bool` | New | R-sized-10 |
| `is_odd` | `fn int32.is_odd(x: Int32) -> Bool` | New | R-sized-10 |
| `max` | `fn int32.max(a: Int32, b: Int32) -> Int32` | New | R-sized-08 |
| `max_value` | `fn int32.max_value() -> Int32` |  |  |
| `min` | `fn int32.min(a: Int32, b: Int32) -> Int32` | New | R-sized-08 |
| `min_value` | `fn int32.min_value() -> Int32` |  |  |
| `modulo` | `fn int32.modulo(a: Int32, b: Int32) -> Int32` | New | R-sized-10 |
| `parse` | `fn int32.parse(s: String) -> Result[Int32, String]` | New | R-sized-02 |
| `parse_radix` | `fn int32.parse_radix(s: String, radix: Int) -> Result[Int32, String]` | New | R-sized-06 |
| `pow` | `fn int32.pow(base: Int32, exp: Int) -> Int32` | New | R-sized-10 |
| `rotate_left` | `fn int32.rotate_left(a: Int32, n: Int) -> Int32` | New | R-int-12 |
| `rotate_right` | `fn int32.rotate_right(a: Int32, n: Int) -> Int32` | New | R-int-12 |
| `sign` | `fn int32.sign(x: Int32) -> Int` | New | R-sized-10 |
| `to_string` | `fn int32.to_string(x: Int32) -> String` |  |  |
| `to_string_radix` | `fn int32.to_string_radix(x: Int32, radix: Int) -> String` | New | R-sized-06 |
| `wrap_add` | `fn int32.wrap_add(a: Int32, b: Int32) -> Int32` | New | R-int-12 |
| `wrap_mul` | `fn int32.wrap_mul(a: Int32, b: Int32) -> Int32` | New | R-int-12 |
| `wrap_sub` | `fn int32.wrap_sub(a: Int32, b: Int32) -> Int32` | New | R-int-12 |

Removed (21):

| Function | Replacement | Proposal |
| --- | --- | --- |
| `to_float32` | `float32.from` | R-L0-40 |
| `to_float64` | `float64.from` | R-L0-40 |
| `to_int16` | `int16.from` | R-L0-40 |
| `to_int16_checked` | `int16.from_checked` | R-L0-40 |
| `to_int16_saturating` | `int16.from_saturating` | R-L0-40 |
| `to_int64` | `int64.from` | R-L0-40 |
| `to_int8` | `int8.from` | R-L0-40 |
| `to_int8_checked` | `int8.from_checked` | R-L0-40 |
| `to_int8_saturating` | `int8.from_saturating` | R-L0-40 |
| `to_uint16` | `uint16.from` | R-L0-40 |
| `to_uint16_checked` | `uint16.from_checked` | R-L0-40 |
| `to_uint16_saturating` | `uint16.from_saturating` | R-L0-40 |
| `to_uint32` | `uint32.from` | R-L0-40 |
| `to_uint32_checked` | `uint32.from_checked` | R-L0-40 |
| `to_uint32_saturating` | `uint32.from_saturating` | R-L0-40 |
| `to_uint64` | `uint64.from` | R-L0-40 |
| `to_uint64_checked` | `uint64.from_checked` | R-L0-40 |
| `to_uint64_saturating` | `uint64.from_saturating` | R-L0-40 |
| `to_uint8` | `uint8.from` | R-L0-40 |
| `to_uint8_checked` | `uint8.from_checked` | R-L0-40 |
| `to_uint8_saturating` | `uint8.from_saturating` | R-L0-40 |

<a id="int64"></a>

## int64

Record: [L2-sized-numbers.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-sized-numbers.md)

| Function | Signature (after redesign) | Change | Proposal |
| --- | --- | --- | --- |
| `abs` | `fn int64.abs(x: Int64) -> Int64` | New | R-sized-10 |
| `band` | `fn int64.band(a: Int64, b: Int64) -> Int64` | New | R-sized-10 |
| `bit_length` | `fn int64.bit_length(x: Int64) -> Int` | New | R-int-11 |
| `bnot` | `fn int64.bnot(x: Int64) -> Int64` | New | R-sized-10 |
| `bor` | `fn int64.bor(a: Int64, b: Int64) -> Int64` | New | R-sized-10 |
| `bshl` | `fn int64.bshl(a: Int64, n: Int) -> Int64` | New | R-sized-10 |
| `bshr` | `fn int64.bshr(a: Int64, n: Int) -> Int64` | New | R-sized-10 |
| `bxor` | `fn int64.bxor(a: Int64, b: Int64) -> Int64` | New | R-sized-10 |
| `byte_swap` | `fn int64.byte_swap(x: Int64) -> Int64` | New | R-sized-10 |
| `clamp` | `fn int64.clamp(x: Int64, lo: Int64, hi: Int64) -> Int64` | New | R-sized-08 |
| `compare` | `fn int64.compare(a: Int64, b: Int64) -> Int` | New | R-sized-02 |
| `count_leading_zeros` | `fn int64.count_leading_zeros(x: Int64) -> Int` | New | R-sized-10 |
| `count_ones` | `fn int64.count_ones(x: Int64) -> Int` | New | R-sized-10 |
| `count_trailing_zeros` | `fn int64.count_trailing_zeros(x: Int64) -> Int` | New | R-sized-10 |
| `floor_div` | `fn int64.floor_div(a: Int64, b: Int64) -> Int64` | New | R-sized-10 |
| `from` | `fn int64.from[S: Numeric](x: S) -> Int64` | New | R-L0-40 |
| `from_checked` | `fn int64.from_checked[S: Numeric](x: S) -> Int64?` | New | R-L0-40 |
| `from_saturating` | `fn int64.from_saturating[S: Numeric](x: S) -> Int64` | New | R-L0-40 |
| `from_wrapping` | `fn int64.from_wrapping[S: Integer](x: S) -> Int64` | New | R-L0-40 |
| `is_even` | `fn int64.is_even(x: Int64) -> Bool` | New | R-sized-10 |
| `is_odd` | `fn int64.is_odd(x: Int64) -> Bool` | New | R-sized-10 |
| `max` | `fn int64.max(a: Int64, b: Int64) -> Int64` | New | R-sized-08 |
| `max_value` | `fn int64.max_value() -> Int64` |  |  |
| `min` | `fn int64.min(a: Int64, b: Int64) -> Int64` | New | R-sized-08 |
| `min_value` | `fn int64.min_value() -> Int64` |  |  |
| `modulo` | `fn int64.modulo(a: Int64, b: Int64) -> Int64` | New | R-sized-10 |
| `parse` | `fn int64.parse(s: String) -> Result[Int64, String]` | New | R-sized-02 |
| `parse_radix` | `fn int64.parse_radix(s: String, radix: Int) -> Result[Int64, String]` | New | R-sized-06 |
| `pow` | `fn int64.pow(base: Int64, exp: Int) -> Int64` | New | R-sized-10 |
| `rotate_left` | `fn int64.rotate_left(a: Int64, n: Int) -> Int64` | New | R-int-12 |
| `rotate_right` | `fn int64.rotate_right(a: Int64, n: Int) -> Int64` | New | R-int-12 |
| `sign` | `fn int64.sign(x: Int64) -> Int` | New | R-sized-10 |
| `to_string` | `fn int64.to_string(x: Int64) -> String` |  |  |
| `to_string_radix` | `fn int64.to_string_radix(x: Int64, radix: Int) -> String` | New | R-sized-06 |
| `wrap_add` | `fn int64.wrap_add(a: Int64, b: Int64) -> Int64` | New | R-int-12 |
| `wrap_mul` | `fn int64.wrap_mul(a: Int64, b: Int64) -> Int64` | New | R-int-12 |
| `wrap_sub` | `fn int64.wrap_sub(a: Int64, b: Int64) -> Int64` | New | R-int-12 |

Removed (23):

| Function | Replacement | Proposal |
| --- | --- | --- |
| `to_float32` | `float32.from` | R-L0-40 |
| `to_float64` | `float64.from` | R-L0-40 |
| `to_int16` | `int16.from` | R-L0-40 |
| `to_int16_checked` | `int16.from_checked` | R-L0-40 |
| `to_int16_saturating` | `int16.from_saturating` | R-L0-40 |
| `to_int32` | `int32.from` | R-L0-40 |
| `to_int32_checked` | `int32.from_checked` | R-L0-40 |
| `to_int32_saturating` | `int32.from_saturating` | R-L0-40 |
| `to_int8` | `int8.from` | R-L0-40 |
| `to_int8_checked` | `int8.from_checked` | R-L0-40 |
| `to_int8_saturating` | `int8.from_saturating` | R-L0-40 |
| `to_uint16` | `uint16.from` | R-L0-40 |
| `to_uint16_checked` | `uint16.from_checked` | R-L0-40 |
| `to_uint16_saturating` | `uint16.from_saturating` | R-L0-40 |
| `to_uint32` | `uint32.from` | R-L0-40 |
| `to_uint32_checked` | `uint32.from_checked` | R-L0-40 |
| `to_uint32_saturating` | `uint32.from_saturating` | R-L0-40 |
| `to_uint64` | `uint64.from` | R-L0-40 |
| `to_uint64_checked` | `uint64.from_checked` | R-L0-40 |
| `to_uint64_saturating` | `uint64.from_saturating` | R-L0-40 |
| `to_uint8` | `uint8.from` | R-L0-40 |
| `to_uint8_checked` | `uint8.from_checked` | R-L0-40 |
| `to_uint8_saturating` | `uint8.from_saturating` | R-L0-40 |

<a id="int8"></a>

## int8

Record: [L2-sized-numbers.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-sized-numbers.md)

| Function | Signature (after redesign) | Change | Proposal |
| --- | --- | --- | --- |
| `abs` | `fn int8.abs(x: Int8) -> Int8` | New | R-sized-10 |
| `band` | `fn int8.band(a: Int8, b: Int8) -> Int8` | New | R-sized-10 |
| `bit_length` | `fn int8.bit_length(x: Int8) -> Int` | New | R-int-11 |
| `bnot` | `fn int8.bnot(x: Int8) -> Int8` | New | R-sized-10 |
| `bor` | `fn int8.bor(a: Int8, b: Int8) -> Int8` | New | R-sized-10 |
| `bshl` | `fn int8.bshl(a: Int8, n: Int) -> Int8` | New | R-sized-10 |
| `bshr` | `fn int8.bshr(a: Int8, n: Int) -> Int8` | New | R-sized-10 |
| `bxor` | `fn int8.bxor(a: Int8, b: Int8) -> Int8` | New | R-sized-10 |
| `byte_swap` | `fn int8.byte_swap(x: Int8) -> Int8` | New | R-sized-10 |
| `clamp` | `fn int8.clamp(x: Int8, lo: Int8, hi: Int8) -> Int8` | New | R-sized-08 |
| `compare` | `fn int8.compare(a: Int8, b: Int8) -> Int` | New | R-sized-02 |
| `count_leading_zeros` | `fn int8.count_leading_zeros(x: Int8) -> Int` | New | R-sized-10 |
| `count_ones` | `fn int8.count_ones(x: Int8) -> Int` | New | R-sized-10 |
| `count_trailing_zeros` | `fn int8.count_trailing_zeros(x: Int8) -> Int` | New | R-sized-10 |
| `floor_div` | `fn int8.floor_div(a: Int8, b: Int8) -> Int8` | New | R-sized-10 |
| `from` | `fn int8.from[S: Numeric](x: S) -> Int8` | New | R-L0-40 |
| `from_checked` | `fn int8.from_checked[S: Numeric](x: S) -> Int8?` | New | R-L0-40 |
| `from_saturating` | `fn int8.from_saturating[S: Numeric](x: S) -> Int8` | New | R-L0-40 |
| `from_wrapping` | `fn int8.from_wrapping[S: Integer](x: S) -> Int8` | New | R-L0-40 |
| `is_even` | `fn int8.is_even(x: Int8) -> Bool` | New | R-sized-10 |
| `is_odd` | `fn int8.is_odd(x: Int8) -> Bool` | New | R-sized-10 |
| `max` | `fn int8.max(a: Int8, b: Int8) -> Int8` | New | R-sized-08 |
| `max_value` | `fn int8.max_value() -> Int8` |  |  |
| `min` | `fn int8.min(a: Int8, b: Int8) -> Int8` | New | R-sized-08 |
| `min_value` | `fn int8.min_value() -> Int8` |  |  |
| `modulo` | `fn int8.modulo(a: Int8, b: Int8) -> Int8` | New | R-sized-10 |
| `parse` | `fn int8.parse(s: String) -> Result[Int8, String]` | New | R-sized-02 |
| `parse_radix` | `fn int8.parse_radix(s: String, radix: Int) -> Result[Int8, String]` | New | R-sized-06 |
| `pow` | `fn int8.pow(base: Int8, exp: Int) -> Int8` | New | R-sized-10 |
| `rotate_left` | `fn int8.rotate_left(a: Int8, n: Int) -> Int8` | New | R-int-12 |
| `rotate_right` | `fn int8.rotate_right(a: Int8, n: Int) -> Int8` | New | R-int-12 |
| `sign` | `fn int8.sign(x: Int8) -> Int` | New | R-sized-10 |
| `to_string` | `fn int8.to_string(x: Int8) -> String` |  |  |
| `to_string_radix` | `fn int8.to_string_radix(x: Int8, radix: Int) -> String` | New | R-sized-06 |
| `wrap_add` | `fn int8.wrap_add(a: Int8, b: Int8) -> Int8` | New | R-int-12 |
| `wrap_mul` | `fn int8.wrap_mul(a: Int8, b: Int8) -> Int8` | New | R-int-12 |
| `wrap_sub` | `fn int8.wrap_sub(a: Int8, b: Int8) -> Int8` | New | R-int-12 |

Removed (17):

| Function | Replacement | Proposal |
| --- | --- | --- |
| `to_float32` | `float32.from` | R-L0-40 |
| `to_float64` | `float64.from` | R-L0-40 |
| `to_int16` | `int16.from` | R-L0-40 |
| `to_int32` | `int32.from` | R-L0-40 |
| `to_int64` | `int64.from` | R-L0-40 |
| `to_uint16` | `uint16.from` | R-L0-40 |
| `to_uint16_checked` | `uint16.from_checked` | R-L0-40 |
| `to_uint16_saturating` | `uint16.from_saturating` | R-L0-40 |
| `to_uint32` | `uint32.from` | R-L0-40 |
| `to_uint32_checked` | `uint32.from_checked` | R-L0-40 |
| `to_uint32_saturating` | `uint32.from_saturating` | R-L0-40 |
| `to_uint64` | `uint64.from` | R-L0-40 |
| `to_uint64_checked` | `uint64.from_checked` | R-L0-40 |
| `to_uint64_saturating` | `uint64.from_saturating` | R-L0-40 |
| `to_uint8` | `uint8.from` | R-L0-40 |
| `to_uint8_checked` | `uint8.from_checked` | R-L0-40 |
| `to_uint8_saturating` | `uint8.from_saturating` | R-L0-40 |

<a id="io"></a>

## io

Record: [L2-io.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-io.md)

| Function | Signature (after redesign) | Change | Proposal |
| --- | --- | --- | --- |
| `close` | `effect fn io.close[H: Close](h: H) -> Unit` | New | R-L0-44 |
| `eprint` | `effect fn io.eprint(s: String) -> Unit` | New | R-io-03 |
| `print` | `effect fn io.print(s: String) -> Unit` |  |  |
| `read` | `effect fn io.read[H: Read](h: H, max: Int) -> Bytes` | New | R-L0-44 |
| `read_all` | `effect fn io.read_all() -> String` |  |  |
| `read_exact` | `effect fn io.read_exact[H: Read](h: H, n: Int) -> Bytes` | New | R-L0-44 |
| `read_line` | `effect fn io.read_line() -> String?` | Semantic change: use `-> String?`. Sole single-line read | R-io-01 |
| `read_to_end` | `effect fn io.read_to_end[H: Read](h: H) -> Bytes` | New | R-L0-44 |
| `stderr` | `effect fn io.stderr() -> Stderr` | New | R-io-05 |
| `stdin` | `effect fn io.stdin() -> Stdin` | New | R-io-05 |
| `stdout` | `effect fn io.stdout() -> Stdout` | New | R-io-05 |
| `write` | `effect fn io.write[H: Write](h: H, data: Bytes) -> Unit` | Semantic change: generic function `write(h, data)` over `Write`. Old one-argument calls are type errors | R-L0-44・R-io-05 |

Removed (4):

| Function | Replacement | Proposal |
| --- | --- | --- |
| `read_byte` | `io.read(io.stdin(), 1)` | R-io-01・R-io-05 |
| `read_line_opt` | `read_line` | R-io-01 |
| `read_n_bytes` | `io.read(io.stdin(), n)` | R-io-01・R-io-05 |
| `write_bytes` | `io.write(io.stdout(), data)` | R-io-01・R-io-02・R-io-05 |

<a id="json"></a>

## json

Record: [L2-json.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-json.md)

| Function | Signature (after redesign) | Change | Proposal |
| --- | --- | --- | --- |
| `parse` | `fn json.parse(text: String) -> Result[Value, String]` | Semantic change: trailing extra characters error. Nesting deeper than 1000 levels errors | R-json-02、R-json-04 |
| `stringify` | `fn json.stringify(v: Value) -> String` |  | R-json-03 |
| `stringify_pretty` | `fn json.stringify_pretty(v: Value) -> String` |  | R-json-03 |

Removed (12):

| Function | Replacement | Proposal |
| --- | --- | --- |
| `field` | `value.key` | R-json-01 |
| `get_array` | `value.get_array` | R-json-01 |
| `get_bool` | `value.get_bool` | R-json-01 |
| `get_float` | `value.get_float` | R-json-01 |
| `get_int` | `value.get_int` | R-json-01 |
| `get_path` | `value.get_path` | R-json-01 |
| `get_string` | `value.get_string` | R-json-01 |
| `index` | `value.index` (negative indices are out of range) | R-json-01 |
| `remove_path` | `value.remove_path` | R-json-01 |
| `root` | `value.root` | R-json-01 |
| `set_path` | `value.set_path` | R-json-01 |
| `to_map` | `value.as_object(v) \|> result.map(map.from_list)` | R-json-01 |

<a id="list"></a>

## list

Record: [L2-list.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-list.md)

| Function | Signature (after redesign) | Change | Proposal |
| --- | --- | --- | --- |
| `all` | `fn list.all(xs: List[A], f: (A) -> Bool) -> Bool` |  |  |
| `any` | `fn list.any(xs: List[A], f: (A) -> Bool) -> Bool` |  |  |
| `binary_search` | `fn list.binary_search(xs: List[A], target: A) -> Int?` | Semantic change: broaden element type to all ordered types | R-list-15 |
| `binary_search_by` | `fn list.binary_search_by(xs: List[A], target: K, f: (A) -> K) -> Int?` | New | R-list-17 |
| `binary_search_with` | `fn list.binary_search_with(xs: List[A], target: A, cmp: (A, A) -> Int) -> Int?` | New | R-list-17 |
| `chunk` | `fn list.chunk(xs: List[A], n: Int) -> List[List[A]]` | Semantic change: `n <= 0` aborts | R-list-10 |
| `clear` | `fn list.clear(mut xs: List[A]) -> Unit` |  | R-list-00 |
| `contains` | `fn list.contains(xs: List[A], x: A) -> Bool` |  |  |
| `count` | `fn list.count(xs: List[A], f: (A) -> Bool) -> Int` |  |  |
| `dedup` | `fn list.dedup(xs: List[A]) -> List[A]` |  |  |
| `drop` | `fn list.drop(xs: List[A], n: Int) -> List[A]` | Semantic change: negative numbers abort. R-L0-29 | R-L0-12、R-L0-29 |
| `drop_end` | `fn list.drop_end(xs: List[A], n: Int) -> List[A]` | Semantic change: negative numbers abort. R-L0-29 | R-L0-12、R-L0-29 |
| `drop_while` | `fn list.drop_while(xs: List[A], f: (A) -> Bool) -> List[A]` |  |  |
| `enumerate` | `fn list.enumerate(xs: List[A]) -> List[(Int, A)]` |  |  |
| `filter` | `fn list.filter(xs: List[A], f: (A) -> Bool) -> List[A]` |  |  |
| `filter_map` | `fn list.filter_map(xs: List[A], f: (A) -> B?) -> List[B]` |  |  |
| `find` | `fn list.find(xs: List[A], f: (A) -> Bool) -> A?` |  |  |
| `find_index` | `fn list.find_index(xs: List[A], f: (A) -> Bool) -> Int?` |  |  |
| `find_map` | `fn list.find_map(xs: List[A], f: (A) -> B?) -> B?` |  |  |
| `first` | `fn list.first(xs: List[A]) -> A?` |  |  |
| `flat_map` | `fn list.flat_map(xs: List[A], f: (A) -> List[B]) -> List[B]` |  |  |
| `flatten` | `fn list.flatten(xs: List[List[A]]) -> List[A]` |  |  |
| `fold` | `fn list.fold(xs: List[A], init: B, f: (B, A) -> B) -> B` |  |  |
| `from_list` | `fn list.from_list(xs: List[A]) -> List[A]` | New | R-list-20 |
| `get` | `fn list.get(xs: List[A], i: Int) -> A?` |  |  |
| `group_by` | `fn list.group_by(xs: List[A], f: (A) -> B) -> Map[B, List[A]]` |  |  |
| `index_of` | `fn list.index_of(xs: List[A], x: A) -> Int?` |  |  |
| `insert` | `fn list.insert(xs: List[A], i: Int, x: A) -> List[A]` | Semantic change: aborts outside `0..=len` | R-list-00、R-list-13 |
| `intersperse` | `fn list.intersperse(xs: List[A], sep: A) -> List[A]` |  |  |
| `is_empty` | `fn list.is_empty(xs: List[A]) -> Bool` |  |  |
| `iterate` | `fn list.iterate(seed: A, f: (A) -> A, n: Int) -> List[A]` |  |  |
| `join` | `fn list.join[T: Text](xs: List[T], sep: T) -> T` | Semantic change: combine the two forms `String`/`Bytes` into one generic function over `Text` | R-L0-42 |
| `last` | `fn list.last(xs: List[A]) -> A?` |  |  |
| `len` | `fn list.len(xs: List[A]) -> Int` |  | R-L0-11 |
| `map` | `fn list.map(xs: List[A], f: (A) -> B) -> List[B]` |  |  |
| `max` | `fn list.max(xs: List[A]) -> A?` |  |  |
| `max_by` | `fn list.max_by(xs: List[A], f: (A) -> K) -> A?` | New | R-list-06 |
| `max_with` | `fn list.max_with(xs: List[A], cmp: (A, A) -> Int) -> A?` | New | R-list-19 |
| `min` | `fn list.min(xs: List[A]) -> A?` |  |  |
| `min_by` | `fn list.min_by(xs: List[A], f: (A) -> K) -> A?` | New | R-list-06 |
| `min_with` | `fn list.min_with(xs: List[A], cmp: (A, A) -> Int) -> A?` | New | R-list-19 |
| `new` | `fn list.new() -> List[A]` | New | R-list-20 |
| `partition` | `fn list.partition(xs: List[A], f: (A) -> Bool) -> (List[A], List[A])` |  |  |
| `pop` | `fn list.pop(mut xs: List[A]) -> A?` |  | R-list-00 |
| `product` | `fn list.product[A: Numeric](xs: List[A]) -> A` | Semantic change: combine the two forms `Int`/`Float` into one generic function accepting all numeric types | R-L0-42 |
| `push` | `fn list.push(mut xs: List[A], x: A) -> Unit` |  | R-list-00 |
| `range` | `fn list.range(start: Int, end: Int) -> List[Int]` |  |  |
| `range_step` | `fn list.range_step(start: Int, end: Int, step: Int) -> List[Int]` | New | R-list-09 |
| `reduce` | `fn list.reduce(xs: List[A], f: (A, A) -> A) -> A?` |  |  |
| `remove_at` | `fn list.remove_at(xs: List[A], i: Int) -> List[A]` | Semantic change: out of range aborts | R-list-00、R-list-13 |
| `repeat` | `fn list.repeat(x: A, n: Int) -> List[A]` |  |  |
| `reverse` | `fn list.reverse(xs: List[A]) -> List[A]` |  | R-list-00 |
| `scan` | `fn list.scan(xs: List[A], init: B, f: (B, A) -> B) -> List[B]` |  |  |
| `set` | `fn list.set(xs: List[A], i: Int, x: A) -> List[A]` | Semantic change: out of range aborts | R-list-00、R-list-13 |
| `slice` | `fn list.slice(xs: List[A], start: Int, end: Int) -> List[A]` | Semantic change: negative positions abort. R-L0-29 | R-list-10、R-L0-29 |
| `sort` | `fn list.sort(xs: List[A]) -> List[A]` |  | R-list-00 |
| `sort_by` | `fn list.sort_by(xs: List[A], f: (A) -> B) -> List[A]` |  | R-list-06 |
| `sort_with` | `fn list.sort_with(xs: List[A], cmp: (A, A) -> Int) -> List[A]` | New | R-list-06 |
| `split_at` | `fn list.split_at(xs: List[A], n: Int) -> (List[A], List[A])` |  |  |
| `sum` | `fn list.sum[A: Numeric](xs: List[A]) -> A` | Semantic change: combine the two forms `Int`/`Float` into one generic function accepting all numeric types | R-L0-42 |
| `swap` | `fn list.swap(xs: List[A], i: Int, j: Int) -> List[A]` | Semantic change: out of range aborts | R-list-00、R-list-13 |
| `take` | `fn list.take(xs: List[A], n: Int) -> List[A]` | Semantic change: negative numbers abort. R-L0-29 | R-L0-12、R-L0-29 |
| `take_end` | `fn list.take_end(xs: List[A], n: Int) -> List[A]` | Semantic change: negative numbers abort. R-L0-29 | R-L0-12、R-L0-29 |
| `take_while` | `fn list.take_while(xs: List[A], f: (A) -> Bool) -> List[A]` |  |  |
| `to_list` | `fn list.to_list(xs: List[A]) -> List[A]` | New | R-list-20 |
| `unique` | `fn list.unique(xs: List[A]) -> List[A]` |  |  |
| `unique_by` | `fn list.unique_by(xs: List[A], f: (A) -> K) -> List[A]` |  |  |
| `unzip` | `fn list.unzip(xs: List[(A, B)]) -> (List[A], List[B])` | New | R-list-07 |
| `update` | `fn list.update(xs: List[A], i: Int, f: (A) -> A) -> List[A]` | Semantic change: out of range aborts | R-list-00、R-list-13 |
| `window` | `fn list.window(xs: List[A], n: Int) -> List[List[A]]` | Semantic change: `n <= 0` aborts. Retained side of `windows` | R-list-03、R-list-10 |
| `with_capacity` | `fn list.with_capacity(cap: Int) -> List[A]` |  | R-list-00 |
| `zip` | `fn list.zip(xs: List[A], ys: List[B]) -> List[(A, B)]` |  |  |
| `zip_with` | `fn list.zip_with(xs: List[A], ys: List[B], f: (A, B) -> C) -> List[C]` |  |  |

Removed (6):

| Function | Replacement | Proposal |
| --- | --- | --- |
| `bundled_probe` | `n + 1` | R-list-04 |
| `get_or` | `list.get(xs, i) ?? d` | R-list-01 |
| `length` | `list.len` | R-list-02 |
| `shuffle` | `random.shuffle` | R-list-05 |
| `tail` | `list.drop(xs, 1)` | R-list-14 |
| `windows` | `list.window` | R-list-03 |

<a id="map"></a>

## map

Record: [L2-map.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-map.md)

| Function | Signature (after redesign) | Change | Proposal |
| --- | --- | --- | --- |
| `all` | `fn map.all(m: Map[K, V], f: ((K, V)) -> Bool) -> Bool` | Semantic change: callback takes one tuple. `(k, v) =>` remains writable as is | R-map-09 |
| `any` | `fn map.any(m: Map[K, V], f: ((K, V)) -> Bool) -> Bool` | Semantic change: callback takes one tuple. `(k, v) =>` remains writable as is | R-map-09 |
| `clear` | `fn map.clear(mut m: Map[K, V]) -> Unit` |  | — (R-L0-04 verb) |
| `contains` | `fn map.contains(m: Map[K, V], key: K) -> Bool` |  |  |
| `count` | `fn map.count(m: Map[K, V], f: ((K, V)) -> Bool) -> Int` | Semantic change: callback takes one tuple. `(k, v) =>` remains writable as is | R-map-09 |
| `enumerate` | `fn map.enumerate(m: Map[K, V]) -> List[(Int, (K, V))]` | New | R-L0-42 |
| `filter` | `fn map.filter(m: Map[K, V], f: ((K, V)) -> Bool) -> Map[K, V]` | Semantic change: callback takes one tuple. `(k, v) =>` remains writable as is | R-map-09 |
| `filter_map` | `fn map.filter_map(m: Map[K, V], f: ((K, V)) -> B?) -> List[B]` | New | R-L0-42 |
| `find` | `fn map.find(m: Map[K, V], f: ((K, V)) -> Bool) -> (K, V)?` | Semantic change: callback takes one tuple. `(k, v) =>` remains writable as is | R-map-09 |
| `find_map` | `fn map.find_map(m: Map[K, V], f: ((K, V)) -> B?) -> B?` | New | R-L0-42 |
| `fold` | `fn map.fold(m: Map[K, V], init: A, f: (A, (K, V)) -> A) -> A` | Semantic change: callback's second argument is a tuple. `almide fix` mutates | R-map-09 |
| `from_list` | `fn map.from_list(xs: List[(K, V)]) -> Map[K, V]` |  | — (R-L0-07) |
| `get` | `fn map.get(m: Map[K, V], key: K) -> V?` |  |  |
| `group_by` | `fn map.group_by(m: Map[K, V], f: ((K, V)) -> B) -> Map[B, List[(K, V)]]` | New | R-L0-42 |
| `is_empty` | `fn map.is_empty(m: Map[K, V]) -> Bool` |  |  |
| `keys` | `fn map.keys(m: Map[K, V]) -> List[K]` |  |  |
| `len` | `fn map.len(m: Map[K, V]) -> Int` |  | — (R-L0-11) |
| `map` | `fn map.map(m: Map[K, V], f: (V) -> B) -> Map[K, B]` |  | R-map-04 |
| `max_by` | `fn map.max_by(m: Map[K, V], f: ((K, V)) -> U) -> (K, V)?` | New | R-L0-42 |
| `max_with` | `fn map.max_with(m: Map[K, V], cmp: ((K, V), (K, V)) -> Int) -> (K, V)?` | New | R-L0-42 |
| `merge` | `fn map.merge(a: Map[K, V], b: Map[K, V]) -> Map[K, V]` |  |  |
| `min_by` | `fn map.min_by(m: Map[K, V], f: ((K, V)) -> U) -> (K, V)?` | New | R-L0-42 |
| `min_with` | `fn map.min_with(m: Map[K, V], cmp: ((K, V), (K, V)) -> Int) -> (K, V)?` | New | R-L0-42 |
| `new` | `fn map.new() -> Map[K, V]` |  |  |
| `partition` | `fn map.partition(m: Map[K, V], f: ((K, V)) -> Bool) -> (List[(K, V)], List[(K, V)])` | New | R-L0-42 |
| `reduce` | `fn map.reduce(m: Map[K, V], f: ((K, V), (K, V)) -> (K, V)) -> (K, V)?` | New | R-L0-42 |
| `remove` | `fn map.remove(m: Map[K, V], key: K) -> Map[K, V]` |  | R-map-01 (replacement) |
| `scan` | `fn map.scan(m: Map[K, V], init: B, f: (B, (K, V)) -> B) -> List[B]` | New | R-L0-42 |
| `set` | `fn map.set(m: Map[K, V], key: K, value: V) -> Map[K, V]` |  | R-map-01 (replacement) |
| `sort_by` | `fn map.sort_by(m: Map[K, V], f: ((K, V)) -> B) -> List[(K, V)]` | New | R-L0-42 |
| `sort_with` | `fn map.sort_with(m: Map[K, V], cmp: ((K, V), (K, V)) -> Int) -> List[(K, V)]` | New | R-L0-42 |
| `to_list` | `fn map.to_list(m: Map[K, V]) -> List[(K, V)]` | Renamed (formerly `entries`) | R-map-07 |
| `update` | `fn map.update(m: Map[K, V], key: K, f: (V) -> V) -> Map[K, V]` | Semantic change: absent keys abort | R-map-08 |
| `upsert` | `fn map.upsert(m: Map[K, V], key: K, init: V, f: (V) -> V) -> Map[K, V]` |  |  |
| `values` | `fn map.values(m: Map[K, V]) -> List[V]` |  |  |
| `zip` | `fn map.zip(m: Map[K, V], ys: List[B]) -> List[((K, V), B)]` | New | R-L0-42 |
| `zip_with` | `fn map.zip_with(m: Map[K, V], ys: List[B], f: ((K, V), B) -> C) -> List[C]` | New | R-L0-42 |

Removed (3):

| Function | Replacement | Proposal |
| --- | --- | --- |
| `delete` | `m = map.remove(m, k)` | R-map-01 |
| `get_or` | `map.get(m, k) ?? d` | R-map-02 |
| `insert` | `m[k] = v`, or `m = map.set(m, k, v)` | R-map-01 |

<a id="math"></a>

## math

Record: [L2-math.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-math.md)

| Function | Signature (after redesign) | Change | Proposal |
| --- | --- | --- | --- |
| `acos` | `fn math.acos(x: Float) -> Float` | New | R-math-02 |
| `acosh` | `fn math.acosh(x: Float) -> Float` | New | R-math-05 |
| `asin` | `fn math.asin(x: Float) -> Float` | New | R-math-02 |
| `asinh` | `fn math.asinh(x: Float) -> Float` | New | R-math-05 |
| `atan` | `fn math.atan(x: Float) -> Float` |  |  |
| `atan2` | `fn math.atan2(y: Float, x: Float) -> Float` | New | R-math-02 |
| `atanh` | `fn math.atanh(x: Float) -> Float` | New | R-math-05 |
| `cbrt` | `fn math.cbrt(x: Float) -> Float` | New | R-math-02 |
| `choose` | `fn math.choose(n: Int, k: Int) -> Int` |  |  |
| `cos` | `fn math.cos(x: Float) -> Float` |  |  |
| `cosh` | `fn math.cosh(x: Float) -> Float` | New | R-math-02 |
| `e` | `fn math.e() -> Float` |  |  |
| `exp` | `fn math.exp(x: Float) -> Float` |  |  |
| `expm1` | `fn math.expm1(x: Float) -> Float` | New | R-math-04 |
| `factorial` | `fn math.factorial(n: Int) -> Int` |  | — (large values: [L2-bigint](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-bigint.md)) |
| `hypot` | `fn math.hypot(a: Float, b: Float) -> Float` | New | R-math-02 |
| `log` | `fn math.log(x: Float) -> Float` |  |  |
| `log10` | `fn math.log10(x: Float) -> Float` |  |  |
| `log1p` | `fn math.log1p(x: Float) -> Float` | New | R-math-04 |
| `log2` | `fn math.log2(x: Float) -> Float` |  |  |
| `log_gamma` | `fn math.log_gamma(x: Float) -> Float` |  |  |
| `pi` | `fn math.pi() -> Float` |  |  |
| `sin` | `fn math.sin(x: Float) -> Float` |  |  |
| `sinh` | `fn math.sinh(x: Float) -> Float` | New | R-math-02 |
| `sqrt` | `fn math.sqrt(x: Float) -> Float` |  | R-math-03 (revisits R-math-01) |
| `tan` | `fn math.tan(x: Float) -> Float` |  |  |
| `tanh` | `fn math.tanh(x: Float) -> Float` |  |  |

Removed (8):

| Function | Replacement | Proposal |
| --- | --- | --- |
| `abs` | `int.abs` | R-math-01（`int.abs`） |
| `fmax` | `float.max` | R-math-01（`float.max`） |
| `fmin` | `float.min` | R-math-01（`float.min`） |
| `fpow` | `float.pow` | R-math-01（`float.pow`） |
| `max` | `int.max` | R-math-01（`int.max`） |
| `min` | `int.min` | R-math-01（`int.min`） |
| `pow` | `int.pow` | R-math-01（`int.pow`） |
| `sign` | `int.sign` | R-math-01（`int.sign`） |

<a id="matrix"></a>

## matrix

Record: [L2-matrix.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-matrix.md)

| Function | Signature (after redesign) | Change | Proposal |
| --- | --- | --- | --- |
| `broadcast_add_row` | `fn matrix.broadcast_add_row(m: Matrix, bias: List[Float]) -> Matrix` | Semantic change: range/shape mismatches abort | R-matrix-05 (unequal lengths abort) |
| `cols` | `fn matrix.cols(m: Matrix) -> Int` |  |  |
| `concat_cols` | `fn matrix.concat_cols(xs: List[Matrix]) -> Matrix` |  |  |
| `concat_rows` | `fn matrix.concat_rows(xs: List[Matrix]) -> Matrix` | New | R-matrix-11 |
| `dot_row` | `fn matrix.dot_row(m: Matrix, r: Int, vec: List[Float]) -> Float` | Semantic change: range/shape mismatches abort | R-matrix-05 (out of range aborts) |
| `from_bytes_f16_le` | `fn matrix.from_bytes_f16_le(b: Bytes, offset: Int, rows: Int, cols: Int) -> Matrix?` | Semantic change: negative shapes abort; insufficient data returns none | R-matrix-05、R-matrix-15 |
| `from_bytes_f32_le` | `fn matrix.from_bytes_f32_le(b: Bytes, offset: Int, rows: Int, cols: Int) -> Matrix?` | Semantic change: negative shapes abort; insufficient data returns none | R-matrix-05、R-matrix-15 |
| `from_bytes_f64_le` | `fn matrix.from_bytes_f64_le(b: Bytes, offset: Int, rows: Int, cols: Int) -> Matrix?` | Semantic change: negative shapes abort; insufficient data returns none | R-matrix-05、R-matrix-15 |
| `from_lists` | `fn matrix.from_lists(xs: List[List[Float]]) -> Matrix` |  |  |
| `gather_rows` | `fn matrix.gather_rows(m: Matrix, indices: List[Int]) -> Matrix` | Semantic change: range/shape mismatches abort | R-matrix-04 / R-matrix-05 (out of range aborts) |
| `get` | `fn matrix.get(m: Matrix, row: Int, col: Int) -> Float?` | Semantic change: returns `Option[Float]` | R-matrix-08 |
| `map` | `fn matrix.map(m: Matrix, f: (Float) -> Float) -> Matrix` |  |  |
| `ones` | `fn matrix.ones(rows: Int, cols: Int) -> Matrix` |  |  |
| `rows` | `fn matrix.rows(m: Matrix) -> Int` |  |  |
| `scale` | `fn matrix.scale(m: Matrix, s: Float) -> Matrix` |  |  |
| `shape` | `fn matrix.shape(m: Matrix) -> (Int, Int)` |  |  |
| `slice_rows` | `fn matrix.slice_rows(m: Matrix, start: Int, end: Int) -> Matrix` | Semantic change: negative positions abort; positive side is clamped | R-L0-29、R-L0-46 |
| `split_cols_even` | `fn matrix.split_cols_even(m: Matrix, n: Int) -> List[Matrix]` |  |  |
| `to_bytes_f16_le` | `fn matrix.to_bytes_f16_le(m: Matrix) -> Bytes` | New | R-matrix-12 |
| `to_bytes_f32_le` | `fn matrix.to_bytes_f32_le(m: Matrix) -> Bytes` |  |  |
| `to_bytes_f64_le` | `fn matrix.to_bytes_f64_le(m: Matrix) -> Bytes` |  |  |
| `to_lists` | `fn matrix.to_lists(m: Matrix) -> List[List[Float]]` |  |  |
| `transpose` | `fn matrix.transpose(m: Matrix) -> Matrix` |  |  |
| `zeros` | `fn matrix.zeros(rows: Int, cols: Int) -> Matrix` |  |  |
| `zip_with` | `fn matrix.zip_with(a: Matrix, b: Matrix, f: (Float, Float) -> Float) -> Matrix` | New | R-matrix-09 |

Removed (24):

| Function | Replacement | Proposal |
| --- | --- | --- |
| `add` | `a + b` | R-matrix-14 |
| `append_rows` | `concat_rows([a, b])` | R-matrix-11 |
| `concat_cols_many` | `matrix.concat_cols` | R-matrix-06 |
| `conv1d` | nn.kernels | R-matrix-01（nn.kernels） |
| `div` | `a / b` | R-matrix-14 |
| `fma` | Internal `__fma` | R-matrix-02 (internal `__fma`) |
| `fma3` | Internal `__fma3` | R-matrix-02 (internal `__fma3`) |
| `linear_f32_row_no_bias` | nn.kernels | R-matrix-01（nn.kernels） |
| `linear_row` | nn.kernels | R-matrix-01（nn.kernels） |
| `linear_row_no_bias` | nn.kernels | R-matrix-01（nn.kernels） |
| `mul` | `a * b` | R-matrix-14 |
| `mul_f32` | `a * b` | R-matrix-03 |
| `mul_f32_scaled` | `matrix.scale(a * b, s)` | R-matrix-03 |
| `mul_f32_t` | `a * matrix.transpose(b)` | R-matrix-03 |
| `mul_f32_t_scaled` | `matrix.scale(a * matrix.transpose(b), s)` | R-matrix-03 |
| `mul_scaled` | Internal `__mul_scaled` | R-matrix-02 (internal `__mul_scaled`) |
| `neg` | `-m` | R-matrix-14 |
| `ones_f32` | `matrix.ones` | R-matrix-03 |
| `pow` | `matrix.map(m, (x) => float.pow(x, e))` | R-matrix-13 |
| `row_dot` | `matrix.dot_row` | R-matrix-06 |
| `select_rows` | `matrix.gather_rows` | R-matrix-04 |
| `select_rows_f32` | nn.kernels | R-matrix-01（nn.kernels） |
| `sub` | `a - b` | R-matrix-14 |
| `zeros_f32` | `matrix.zeros` (read f32 data with from_bytes_f32_le) | R-matrix-03 |

<a id="net"></a>

## net

Record: [L2-net.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-net.md)

| Function | Signature (after redesign) | Change | Proposal |
| --- | --- | --- | --- |
| `ip_to_string` | `fn net.ip_to_string(ip: IpAddr) -> String` | New | R-net-03 |
| `is_loopback` | `fn net.is_loopback(ip: IpAddr) -> Bool` | New | R-net-03 |
| `lookup_host` | `effect fn net.lookup_host(host: String) -> List[IpAddr]` | New | R-net-03 |
| `parse_ip` | `fn net.parse_ip(s: String) -> Result[IpAddr, String]` | New | R-net-03 |
| `tcp_accept` | `effect fn net.tcp_accept(listener: TcpListener) -> TcpStream` | Semantic change: handles become `TcpStream`/`TcpListener` | R-net-05 |
| `tcp_available` | `effect fn net.tcp_available(stream: TcpStream) -> Int` | Semantic change: handles become `TcpStream`/`TcpListener` | R-net-05 |
| `tcp_connect` | `effect fn net.tcp_connect(host: String, port: Int) -> TcpStream` | Semantic change: handles become `TcpStream`/`TcpListener` | R-net-05 |
| `tcp_is_open` | `effect fn net.tcp_is_open(stream: TcpStream) -> Bool` | Semantic change: make effect fn; use `stream: TcpStream` | R-net-04、R-net-05 |
| `tcp_listen` | `effect fn net.tcp_listen(host: String, port: Int) -> TcpListener` | Semantic change: handles become `TcpStream`/`TcpListener` | R-net-05 |
| `tcp_local_port` | `effect fn net.tcp_local_port(listener: TcpListener) -> Int` | Semantic change: handles become `TcpStream`/`TcpListener` | R-net-05 |
| `tcp_read_timeout` | `effect fn net.tcp_read_timeout(stream: TcpStream, len: Int, timeout_ms: Int) -> Bytes` | Semantic change: handles become `TcpStream`/`TcpListener` | R-net-05 |
| `tcp_set_timeout` | `effect fn net.tcp_set_timeout(stream: TcpStream, timeout_ms: Int) -> Unit` | Semantic change: handles become `TcpStream`/`TcpListener` | R-net-05 |
| `udp_bind` | `effect fn net.udp_bind(host: String, port: Int) -> UdpSocket` | New | R-net-02 |
| `udp_local_port` | `effect fn net.udp_local_port(sock: UdpSocket) -> Int` | New | R-net-02 |
| `udp_recv_from` | `effect fn net.udp_recv_from(sock: UdpSocket, max: Int) -> UdpPacket` | New. Returns `UdpPacket` | R-net-01、R-net-06 |
| `udp_send_to` | `effect fn net.udp_send_to(sock: UdpSocket, data: Bytes, host: String, port: Int) -> Unit` | New | R-net-02 |

Removed (18):

| Function | Replacement | Proposal |
| --- | --- | --- |
| `shm_create` | `unix.shm_create` | R-net-01 |
| `shm_read` | `unix.shm_read` | R-net-01 |
| `shm_resize` | `unix.shm_resize` | R-net-01 |
| `shm_write` | `unix.shm_write` | R-net-01 |
| `tcp_close` | `io.close` | R-net-07 |
| `tcp_close_listener` | `io.close` | R-net-07 |
| `tcp_read` | `io.read` | R-net-07 |
| `tcp_read_exact` | `io.read_exact` | R-net-07 |
| `tcp_write` | `io.write` | R-net-07 |
| `unix_accept` | `unix.accept` | R-net-01 |
| `unix_close` | `io.close` | R-net-01 |
| `unix_connect` | `unix.connect` | R-net-01 |
| `unix_listen` | `unix.listen` | R-net-01 |
| `unix_poll` | `unix.select([fd], t)` | R-net-01 |
| `unix_recv` | `io.read` | R-net-01 |
| `unix_send` | `unix.write_fds` | R-net-01 |
| `unix_take_fds` | `unix.take_fds` | R-net-01 |
| `unix_wait` | `unix.select` | R-net-01 |

<a id="option"></a>

## option

Record: [L2-option.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-option.md)

| Function | Signature (after redesign) | Change | Proposal |
| --- | --- | --- | --- |
| `collect` | `fn option.collect(xs: List[A?]) -> List[A]?` |  | R-option-02 |
| `collect_map` | `fn option.collect_map(xs: List[A], f: (A) -> B?) -> List[B]?` |  | R-option-02 |
| `filter` | `fn option.filter(o: A?, f: (A) -> Bool) -> A?` |  | R-option-04 |
| `flat_map` | `fn option.flat_map(o: A?, f: (A) -> B?) -> B?` |  | R-L0-10 (`flat_map` spelling fixed by rule) |
| `flatten` | `fn option.flatten(o: A??) -> A?` |  |  |
| `is_none` | `fn option.is_none(o: A?) -> Bool` |  |  |
| `is_some` | `fn option.is_some(o: A?) -> Bool` |  |  |
| `map` | `fn option.map(o: A?, f: (A) -> B) -> B?` |  |  |
| `or_else` | `fn option.or_else(o: A?, f: () -> A?) -> A?` |  | R-option-03 |
| `to_list` | `fn option.to_list(o: A?) -> List[A]` |  |  |
| `to_result` | `fn option.to_result(o: A?, e: E) -> Result[A, E]` |  | Note: E is polymorphic (a custom variant E confirmed in probe-error/p.almd). ADR-0007 D4's text saying E is fixed to String is older |
| `unwrap_or_else` | `fn option.unwrap_or_else(o: A?, f: () -> A) -> A` |  | Desugaring target of `??` (ADR-0005 D1) |
| `zip` | `fn option.zip(a: A?, b: B?) -> (A, B)?` |  | R-option-04 |

Removed (1):

| Function | Replacement | Proposal |
| --- | --- | --- |
| `unwrap_or` | `o ?? d` | R-option-01 |

<a id="path"></a>

## path

Record: [L2-path.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-path.md)

| Function | Signature (after redesign) | Change | Proposal |
| --- | --- | --- | --- |
| `basename` | `fn path.basename(path: String) -> String` |  |  |
| `components` | `fn path.components(path: String) -> List[String]` | New | R-path-01 |
| `dirname` | `fn path.dirname(path: String) -> String` |  |  |
| `extension` | `fn path.extension(path: String) -> String?` |  |  |
| `is_absolute` | `fn path.is_absolute(path: String) -> Bool` |  |  |
| `join` | `fn path.join(base: String, child: String) -> String` |  |  |
| `normalize` | `fn path.normalize(path: String) -> String` |  |  |
| `relative_to` | `fn path.relative_to(path: String, base: String) -> String?` | New | R-path-01 |
| `stem` | `fn path.stem(path: String) -> String` |  |  |
| `with_extension` | `fn path.with_extension(path: String, ext: String) -> String` | New | R-path-03 |

Removed (3):

| Function | Replacement | Proposal |
| --- | --- | --- |
| `from_string` | Per `SafePath` | R-path-02 |
| `to_string` | Per `SafePath` | R-path-02 |
| `trusted` | Per `SafePath` | R-path-02 |

<a id="process"></a>

## process

Record: [L2-process.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-process.md)

| Function | Signature (after redesign) | Change | Proposal |
| --- | --- | --- | --- |
| `child_pid` | `effect fn process.child_pid(c: ProcessChild) -> Int` | New | R-process-06 |
| `exec` | `effect fn process.exec(cmd: String, args: List[String]) -> String` |  | R-process-02 |
| `exit` | `effect fn process.exit(code: Int) -> Never` |  |  |
| `kill` | `effect fn process.kill(pid: Int, signal: Int) -> Unit` |  | R-process-03 |
| `output` | `effect fn process.output(c: ProcessCommand) -> ProcessStatus` | New | R-process-02 |
| `pid` | `effect fn process.pid() -> Int` | Semantic change: make effect fn | R-process-04 |
| `poll` | `effect fn process.poll(c: ProcessChild) -> Result[ProcessStatus, String]?` | New | R-process-03 |
| `read_new_stderr` | `effect fn process.read_new_stderr(c: ProcessChild) -> String` | New | R-process-03 |
| `read_new_stdout` | `effect fn process.read_new_stdout(c: ProcessChild) -> String` | New | R-process-03 |
| `run` | `effect fn process.run(c: ProcessCommand) -> Int` | Semantic change: accepts `ProcessCommand` | R-process-07 |
| `start` | `effect fn process.start(c: ProcessCommand) -> ProcessChild` | New | R-process-03 |
| `stop` | `effect fn process.stop(c: ProcessChild, grace_ms: Int) -> ProcessStatus` | New | R-process-03 |
| `wait` | `effect fn process.wait(c: ProcessChild) -> ProcessStatus` | New | R-process-03 |

Removed (12):

| Function | Replacement | Proposal |
| --- | --- | --- |
| `args` | `env.args` | R-process-01 |
| `env` | `env.get` | R-process-01 |
| `exec_attached` | `run` | R-process-02 |
| `exec_in` | `output` | R-process-02 |
| `exec_status` | `output` | R-process-02 |
| `exec_status_timeout` | `timeout_ms` of `output` | R-process-02 |
| `exec_with_stdin` | `output` | R-process-02 |
| `is_alive` | `poll` | R-process-03 |
| `run_in` | `run(ProcessCommand { dir: … })` | R-process-07 |
| `sleep` | `env.sleep_ms` | R-process-01 |
| `spawn` | `start` | R-process-03 |
| `stdin_lines` | `string.lines(io.read_all()!)` | R-process-01 |

<a id="random"></a>

## random

Record: [L2-random.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-random.md)

| Function | Signature (after redesign) | Change | Proposal |
| --- | --- | --- | --- |
| `bytes` | `effect fn random.bytes(n: Int) -> Bytes` | New | R-random-02 |
| `choice` | `effect fn random.choice(xs: List[A]) -> A?` |  |  |
| `float` | `effect fn random.float() -> Float` |  |  |
| `int` | `effect fn random.int(min: Int, max: Int) -> Int` |  | R-random-04 |
| `next_choice` | `fn random.next_choice[A](rng: Rng, xs: List[A]) -> (A?, Rng)` | New | R-random-01 |
| `next_float` | `fn random.next_float(rng: Rng) -> (Float, Rng)` | New | R-random-01 |
| `next_int` | `fn random.next_int(rng: Rng, min: Int, max: Int) -> (Int, Rng)` | New | R-random-01 |
| `next_shuffle` | `fn random.next_shuffle[A](rng: Rng, xs: List[A]) -> (List[A], Rng)` | New | R-random-01 |
| `seeded` | `fn random.seeded(seed: Int) -> Rng` | New | R-random-01 |
| `shuffle` | `effect fn random.shuffle(xs: List[A]) -> List[A]` |  | R-random-03 |

<a id="regex"></a>

## regex

Record: [L2-regex.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-regex.md)

| Function | Signature (after redesign) | Change | Proposal |
| --- | --- | --- | --- |
| `captures` | `fn regex.captures(re: Regex, s: String) -> List[String?]?` | Semantic change: first argument becomes `Regex`; groups are `String?` | R-regex-08 |
| `captures_all` | `fn regex.captures_all(re: Regex, s: String) -> List[List[String?]]` | New | R-regex-04、R-regex-08 |
| `compile` | `fn regex.compile(pattern: String) -> Result[Regex, String]` | New | R-regex-02 |
| `escape` | `fn regex.escape(s: String) -> String` | New | R-regex-04 |
| `find` | `fn regex.find(re: Regex, s: String) -> String?` | Semantic change: first argument becomes `Regex` | R-regex-02 |
| `find_all` | `fn regex.find_all(re: Regex, s: String) -> List[String]` | Semantic change: first argument becomes `Regex` | R-regex-02 |
| `index_of` | `fn regex.index_of(re: Regex, s: String) -> Int?` | New | R-regex-04 |
| `is_full_match` | `fn regex.is_full_match(re: Regex, s: String) -> Bool` | Renamed (formerly `full_match`). First argument is `Regex` | R-regex-01、R-regex-02 |
| `is_match` | `fn regex.is_match(re: Regex, s: String) -> Bool` | Semantic change: first argument becomes `Regex` | R-regex-02 |
| `new` | `fn regex.new(pattern: String) -> Regex` | New | R-regex-02 |
| `replace` | `fn regex.replace(re: Regex, s: String, rep: String) -> String` | Semantic change: first argument becomes `Regex` | R-regex-02, R-regex-05 (do not interpret `$1`) |
| `replace_first` | `fn regex.replace_first(re: Regex, s: String, rep: String) -> String` | Semantic change: first argument becomes `Regex` | R-regex-02 |
| `replace_first_with` | `fn regex.replace_first_with(re: Regex, s: String, f: (List[String?]) -> String) -> String` | New | R-regex-07、R-regex-08 |
| `replace_with` | `fn regex.replace_with(re: Regex, s: String, f: (List[String?]) -> String) -> String` | New | R-regex-05、R-regex-08 |
| `split` | `fn regex.split(re: Regex, s: String) -> List[String]` | Semantic change: first argument becomes `Regex` | R-regex-02 |

<a id="result"></a>

## result

Record: [L2-result.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-result.md)

| Function | Signature (after redesign) | Change | Proposal |
| --- | --- | --- | --- |
| `filter` | `fn result.filter(r: Result[A, E], f: (A) -> Bool, err_val: E) -> Result[A, E]` |  | R-result-02 |
| `flat_map` | `fn result.flat_map(r: Result[A, E], f: (A) -> Result[B, E]) -> Result[B, E]` |  | R-L0-10 |
| `flatten` | `fn result.flatten(r: Result[Result[A, E], E]) -> Result[A, E]` |  | R-option-04 |
| `is_err` | `fn result.is_err(r: Result[A, E]) -> Bool` |  |  |
| `is_ok` | `fn result.is_ok(r: Result[A, E]) -> Bool` |  |  |
| `map` | `fn result.map(r: Result[A, E], f: (A) -> B) -> Result[B, E]` |  |  |
| `map_err` | `fn result.map_err(r: Result[A, E], f: (E) -> F) -> Result[A, F]` |  | Canonical form of ADR-0004 D2 |
| `or_else` | `fn result.or_else(r: Result[A, E], f: (E) -> Result[A, F]) -> Result[A, F]` |  |  |
| `partition` | `fn result.partition(xs: List[Result[A, E]]) -> (List[A], List[E])` |  | R-result-05 |
| `to_err_option` | `fn result.to_err_option(r: Result[A, E]) -> E?` |  | R-result-03 |
| `to_list` | `fn result.to_list(r: Result[A, E]) -> List[A]` |  | R-option-04 |
| `to_option` | `fn result.to_option(r: Result[A, E]) -> A?` |  | Desugaring target of `?` |
| `unwrap_or_else` | `fn result.unwrap_or_else(r: Result[A, E], f: (E) -> A) -> A` |  | Desugaring target of `??` (ADR-0005 D1) |
| `zip` | `fn result.zip(a: Result[A, E], b: Result[B, E]) -> Result[(A, B), E]` |  | R-option-04 |

Removed (1):

| Function | Replacement | Proposal |
| --- | --- | --- |
| `unwrap_or` | `r ?? d` | R-result-01 |

<a id="secret"></a>

## secret

Record: [L2-secret.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-secret.md)

| Function | Signature (after redesign) | Change | Proposal |
| --- | --- | --- | --- |
| `new` | `fn secret.new[A](x: A) -> Secret[A]` | New | R-secret-01 |
| `reveal` | `fn secret.reveal[A](s: Secret[A]) -> A` | New | R-secret-01 |

<a id="set"></a>

## set

Record: [L2-set.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-set.md)

| Function | Signature (after redesign) | Change | Proposal |
| --- | --- | --- | --- |
| `all` | `fn set.all(s: Set[A], f: (A) -> Bool) -> Bool` |  | R-map-06 |
| `any` | `fn set.any(s: Set[A], f: (A) -> Bool) -> Bool` |  | R-map-06 |
| `clear` | `fn set.clear(mut s: Set[A]) -> Unit` | New | R-L0-04 (`clear` matrix) |
| `contains` | `fn set.contains(s: Set[A], x: A) -> Bool` |  |  |
| `count` | `fn set.count(s: Set[A], f: (A) -> Bool) -> Int` | New | R-set-01 |
| `difference` | `fn set.difference(a: Set[A], b: Set[A]) -> Set[A]` |  |  |
| `enumerate` | `fn set.enumerate(s: Set[A]) -> List[(Int, A)]` | New | R-L0-42 |
| `filter` | `fn set.filter(s: Set[A], f: (A) -> Bool) -> Set[A]` |  | R-map-06 |
| `filter_map` | `fn set.filter_map(s: Set[A], f: (A) -> B?) -> List[B]` | New | R-L0-42 |
| `find` | `fn set.find(s: Set[A], f: (A) -> Bool) -> A?` | New | R-set-01 |
| `find_map` | `fn set.find_map(s: Set[A], f: (A) -> B?) -> B?` | New | R-L0-42 |
| `fold` | `fn set.fold(s: Set[A], init: B, f: (B, A) -> B) -> B` |  | R-map-06 |
| `from_list` | `fn set.from_list(xs: List[A]) -> Set[A]` |  | — (R-L0-07) |
| `group_by` | `fn set.group_by(s: Set[A], f: (A) -> B) -> Map[B, List[A]]` | New | R-L0-42 |
| `insert` | `fn set.insert(s: Set[A], x: A) -> Set[A]` |  | R-set-04 |
| `intersection` | `fn set.intersection(a: Set[A], b: Set[A]) -> Set[A]` |  |  |
| `is_disjoint` | `fn set.is_disjoint(a: Set[A], b: Set[A]) -> Bool` |  |  |
| `is_empty` | `fn set.is_empty(s: Set[A]) -> Bool` |  |  |
| `is_subset` | `fn set.is_subset(a: Set[A], b: Set[A]) -> Bool` |  | R-set-02 |
| `len` | `fn set.len(s: Set[A]) -> Int` |  | — (R-L0-11) |
| `map` | `fn set.map(s: Set[A], f: (A) -> B) -> Set[B]` |  | R-map-06 |
| `max` | `fn set.max(s: Set[A]) -> A?` | New | R-L0-42 |
| `max_by` | `fn set.max_by(s: Set[A], f: (A) -> K) -> A?` | New | R-L0-42 |
| `max_with` | `fn set.max_with(s: Set[A], cmp: (A, A) -> Int) -> A?` | New | R-L0-42 |
| `min` | `fn set.min(s: Set[A]) -> A?` | New | R-L0-42 |
| `min_by` | `fn set.min_by(s: Set[A], f: (A) -> K) -> A?` | New | R-L0-42 |
| `min_with` | `fn set.min_with(s: Set[A], cmp: (A, A) -> Int) -> A?` | New | R-L0-42 |
| `new` | `fn set.new() -> Set[A]` |  |  |
| `partition` | `fn set.partition(s: Set[A], f: (A) -> Bool) -> (List[A], List[A])` | New | R-L0-42 |
| `product` | `fn set.product[A: Numeric](s: Set[A]) -> A` | New | R-L0-42 |
| `reduce` | `fn set.reduce(s: Set[A], f: (A, A) -> A) -> A?` | New | R-L0-42 |
| `remove` | `fn set.remove(s: Set[A], x: A) -> Set[A]` |  |  |
| `scan` | `fn set.scan(s: Set[A], init: B, f: (B, A) -> B) -> List[B]` | New | R-L0-42 |
| `sort` | `fn set.sort(s: Set[A]) -> List[A]` | New | R-L0-42 |
| `sort_by` | `fn set.sort_by(s: Set[A], f: (A) -> B) -> List[A]` | New | R-L0-42 |
| `sort_with` | `fn set.sort_with(s: Set[A], cmp: (A, A) -> Int) -> List[A]` | New | R-L0-42 |
| `sum` | `fn set.sum[A: Numeric](s: Set[A]) -> A` | New | R-L0-42 |
| `symmetric_difference` | `fn set.symmetric_difference(a: Set[A], b: Set[A]) -> Set[A]` |  |  |
| `to_list` | `fn set.to_list(s: Set[A]) -> List[A]` |  |  |
| `union` | `fn set.union(a: Set[A], b: Set[A]) -> Set[A]` |  |  |
| `zip` | `fn set.zip(s: Set[A], ys: List[B]) -> List[(A, B)]` | New | R-L0-42 |
| `zip_with` | `fn set.zip_with(s: Set[A], ys: List[B], f: (A, B) -> C) -> List[C]` | New | R-L0-42 |

<a id="string"></a>

## string

Record: [L2-string.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-string.md)

| Function | Signature (after redesign) | Change | Proposal |
| --- | --- | --- | --- |
| `byte_len` | `fn string.byte_len(s: String) -> Int` | New | R-string-06 |
| `byte_slice` | `fn string.byte_slice(s: String, start: Int, end: Int) -> String?` |  |  |
| `capitalize` | `fn string.capitalize(s: String) -> String` |  |  |
| `chunk` | `fn string.chunk(s: String, n: Int) -> List[String]` | New | R-L0-42 |
| `clear` | `fn string.clear(mut s: String) -> Unit` |  |  |
| `compare` | `fn string.compare(a: String, b: String) -> Int` | New | R-string-05（R-L0-16） |
| `contains` | `fn string.contains(s: String, needle: String) -> Bool` |  |  |
| `count` | `fn string.count(s: String, f: (String) -> Bool) -> Int` | New | R-string-18、R-L0-42 |
| `count_matches` | `fn string.count_matches(s: String, needle: String) -> Int` | Renamed (formerly `count`) | R-string-18 |
| `dedup` | `fn string.dedup(s: String) -> String` | New | R-L0-42 |
| `drop` | `fn string.drop(s: String, n: Int) -> String` |  |  |
| `drop_end` | `fn string.drop_end(s: String, n: Int) -> String` |  |  |
| `drop_while` | `fn string.drop_while(s: String, f: (String) -> Bool) -> String` | New | R-L0-42 |
| `ends_with` | `fn string.ends_with(s: String, suffix: String) -> Bool` |  |  |
| `enumerate` | `fn string.enumerate(s: String) -> List[(Int, String)]` | New | R-L0-42 |
| `filter` | `fn string.filter(s: String, f: (String) -> Bool) -> String` | New | R-string-16 |
| `filter_map` | `fn string.filter_map(s: String, f: (String) -> B?) -> List[B]` | New | R-L0-42 |
| `find_index` | `fn string.find_index(s: String, f: (String) -> Bool) -> Int?` | New | R-L0-42 |
| `find_map` | `fn string.find_map(s: String, f: (String) -> B?) -> B?` | New | R-L0-42 |
| `first` | `fn string.first(s: String) -> String?` |  |  |
| `fold` | `fn string.fold(s: String, init: B, f: (B, String) -> B) -> B` | New | R-string-16 |
| `from_codepoint` | `fn string.from_codepoint(n: Int) -> String?` | Semantic change: returns `Option[String]` | R-string-11 |
| `from_list` | `fn string.from_list(xs: List[String]) -> String` | New | R-string-17 |
| `from_utf16` | `fn string.from_utf16(xs: List[Int]) -> Result[String, String]` | New | R-string-07 |
| `get` | `fn string.get(s: String, i: Int) -> String?` |  |  |
| `group_by` | `fn string.group_by(s: String, f: (String) -> B) -> Map[B, List[String]]` | New | R-L0-42 |
| `index_of` | `fn string.index_of(s: String, needle: String) -> Int?` |  |  |
| `insert` | `fn string.insert(s: String, i: Int, x: String) -> String` | New | R-string-16 |
| `intersperse` | `fn string.intersperse(s: String, sep: String) -> String` | New | R-L0-42 |
| `is_alpha` | `fn string.is_alpha(s: String) -> Bool` |  |  |
| `is_alphanumeric` | `fn string.is_alphanumeric(s: String) -> Bool` |  |  |
| `is_digit` | `fn string.is_digit(s: String) -> Bool` |  |  |
| `is_empty` | `fn string.is_empty(s: String) -> Bool` |  |  |
| `is_equal_constant_time` | `fn string.is_equal_constant_time(a: String, b: String) -> Bool` | New | R-string-19 |
| `is_lower` | `fn string.is_lower(s: String) -> Bool` |  |  |
| `is_upper` | `fn string.is_upper(s: String) -> Bool` |  |  |
| `is_whitespace` | `fn string.is_whitespace(s: String) -> Bool` |  |  |
| `last` | `fn string.last(s: String) -> String?` |  |  |
| `last_index_of` | `fn string.last_index_of(s: String, needle: String) -> Int?` |  |  |
| `len` | `fn string.len(s: String) -> Int` |  |  |
| `lines` | `fn string.lines(s: String) -> List[String]` |  |  |
| `max` | `fn string.max(s: String) -> String?` | New | R-L0-42 |
| `max_by` | `fn string.max_by(s: String, f: (String) -> K) -> String?` | New | R-L0-42 |
| `max_with` | `fn string.max_with(s: String, cmp: (String, String) -> Int) -> String?` | New | R-L0-42 |
| `min` | `fn string.min(s: String) -> String?` | New | R-L0-42 |
| `min_by` | `fn string.min_by(s: String, f: (String) -> K) -> String?` | New | R-L0-42 |
| `min_with` | `fn string.min_with(s: String, cmp: (String, String) -> Int) -> String?` | New | R-L0-42 |
| `new` | `fn string.new() -> String` | New | R-string-17 |
| `pad_end` | `fn string.pad_end(s: String, n: Int, fill: String) -> String` |  |  |
| `pad_start` | `fn string.pad_start(s: String, n: Int, fill: String) -> String` |  |  |
| `partition` | `fn string.partition(s: String, f: (String) -> Bool) -> (List[String], List[String])` | New | R-L0-42 |
| `pop` | `fn string.pop(mut s: String) -> String?` | New | R-string-16 |
| `push` | `fn string.push(mut s: String, x: String) -> Unit` |  |  |
| `reduce` | `fn string.reduce(s: String, f: (String, String) -> String) -> String?` | New | R-L0-42 |
| `remove_at` | `fn string.remove_at(s: String, i: Int) -> String` | New | R-string-16 |
| `repeat` | `fn string.repeat(s: String, n: Int) -> String` |  |  |
| `replace` | `fn string.replace(s: String, from: String, to: String) -> String` |  |  |
| `replace_first` | `fn string.replace_first(s: String, from: String, to: String) -> String` |  |  |
| `reverse` | `fn string.reverse(s: String) -> String` |  |  |
| `scan` | `fn string.scan(s: String, init: B, f: (B, String) -> B) -> List[B]` | New | R-L0-42 |
| `slice` | `fn string.slice(s: String, start: Int, end: Int) -> String` | Semantic change: remove default for `end`. Two-argument form is `drop` | R-string-15 |
| `sort` | `fn string.sort(s: String) -> List[String]` | New | R-L0-42 |
| `sort_by` | `fn string.sort_by(s: String, f: (String) -> B) -> List[String]` | New | R-L0-42 |
| `sort_with` | `fn string.sort_with(s: String, cmp: (String, String) -> Int) -> List[String]` | New | R-L0-42 |
| `split` | `fn string.split(s: String, sep: String) -> List[String]` |  |  |
| `split_at` | `fn string.split_at(s: String, n: Int) -> (String, String)` | New | R-L0-42 |
| `split_once` | `fn string.split_once(s: String, sep: String) -> (String, String)?` |  |  |
| `starts_with` | `fn string.starts_with(s: String, prefix: String) -> Bool` |  |  |
| `strip_prefix` | `fn string.strip_prefix(s: String, prefix: String) -> String?` |  |  |
| `strip_suffix` | `fn string.strip_suffix(s: String, suffix: String) -> String?` |  |  |
| `take` | `fn string.take(s: String, n: Int) -> String` |  |  |
| `take_end` | `fn string.take_end(s: String, n: Int) -> String` |  |  |
| `take_while` | `fn string.take_while(s: String, f: (String) -> Bool) -> String` | New | R-L0-42 |
| `to_codepoint` | `fn string.to_codepoint(s: String) -> Int?` | Renamed (formerly `codepoint`). none unless exactly one code point | R-string-12、R-string-14 |
| `to_list` | `fn string.to_list(s: String) -> List[String]` | Renamed (formerly `chars`) | R-string-17 |
| `to_lower` | `fn string.to_lower(s: String) -> String` |  |  |
| `to_upper` | `fn string.to_upper(s: String) -> String` |  |  |
| `to_utf16` | `fn string.to_utf16(s: String) -> List[Int]` | New | R-string-07 |
| `trim` | `fn string.trim(s: String) -> String` |  |  |
| `trim_end` | `fn string.trim_end(s: String) -> String` |  |  |
| `trim_start` | `fn string.trim_start(s: String) -> String` |  |  |
| `unique` | `fn string.unique(s: String) -> String` | New | R-L0-42 |
| `unique_by` | `fn string.unique_by(s: String, f: (String) -> K) -> String` | New | R-L0-42 |
| `window` | `fn string.window(s: String, n: Int) -> List[String]` | New | R-L0-42 |
| `zip` | `fn string.zip(s: String, ys: List[B]) -> List[(String, B)]` | New | R-L0-42 |
| `zip_with` | `fn string.zip_with(s: String, ys: List[B], f: (String, B) -> C) -> List[C]` | New | R-L0-42 |

Removed (5):

| Function | Replacement | Proposal |
| --- | --- | --- |
| `from_bytes` | `bytes.from_list(xs) \|> bytes.to_string_lossy` | R-string-02 |
| `join` | `list.join` | R-string-01 |
| `length` | `string.len` | R-string-04 |
| `run_length_encode` | `string.chars` and `list.fold` | R-string-03 |
| `to_bytes` | `bytes.from_string` | R-string-02 |

<a id="testing"></a>

## testing

Record: [L2-testing.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-testing.md)

| Function | Signature (after redesign) | Change | Proposal |
| --- | --- | --- | --- |
| `assert_aborts` | `fn testing.assert_aborts[A](expected: String, f: () -> A) -> Unit` |  |  |
| `assert_approx` | `fn testing.assert_approx(a: Float, b: Float, tolerance: Float) -> Unit` |  |  |
| `assert_contains` | `fn testing.assert_contains(haystack: String, needle: String) -> Unit` |  | R-testing-01 |
| `assert_err` | `fn testing.assert_err[A, B](r: Result[A, B]) -> Unit` |  |  |
| `assert_gt` | `fn testing.assert_gt[A](a: A, b: A) -> Unit` | Semantic change: broaden to all ordered types | R-testing-02 |
| `assert_lt` | `fn testing.assert_lt[A](a: A, b: A) -> Unit` | Semantic change: broaden to all ordered types | R-testing-02 |
| `assert_none` | `fn testing.assert_none[A](o: A?) -> Unit` |  |  |
| `assert_ok` | `fn testing.assert_ok[A, B](r: Result[A, B]) -> Unit` |  |  |
| `assert_snapshot` | `fn testing.assert_snapshot(actual: String, expected: String) -> Unit` |  |  |
| `assert_some` | `fn testing.assert_some[A](o: A?) -> Unit` |  |  |

<a id="uint16"></a>

## uint16

Record: [L2-sized-numbers.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-sized-numbers.md)

| Function | Signature (after redesign) | Change | Proposal |
| --- | --- | --- | --- |
| `band` | `fn uint16.band(a: UInt16, b: UInt16) -> UInt16` | New | R-sized-10 |
| `bit_length` | `fn uint16.bit_length(x: UInt16) -> Int` | New | R-int-11 |
| `bnot` | `fn uint16.bnot(x: UInt16) -> UInt16` | New | R-sized-10 |
| `bor` | `fn uint16.bor(a: UInt16, b: UInt16) -> UInt16` | New | R-sized-10 |
| `bshl` | `fn uint16.bshl(a: UInt16, n: Int) -> UInt16` | New | R-sized-10 |
| `bshr` | `fn uint16.bshr(a: UInt16, n: Int) -> UInt16` | New | R-sized-10 |
| `bxor` | `fn uint16.bxor(a: UInt16, b: UInt16) -> UInt16` | New | R-sized-10 |
| `byte_swap` | `fn uint16.byte_swap(x: UInt16) -> UInt16` | New | R-sized-10 |
| `clamp` | `fn uint16.clamp(x: UInt16, lo: UInt16, hi: UInt16) -> UInt16` | New | R-sized-08 |
| `compare` | `fn uint16.compare(a: UInt16, b: UInt16) -> Int` | New | R-sized-02 |
| `count_leading_zeros` | `fn uint16.count_leading_zeros(x: UInt16) -> Int` | New | R-sized-10 |
| `count_ones` | `fn uint16.count_ones(x: UInt16) -> Int` | New | R-sized-10 |
| `count_trailing_zeros` | `fn uint16.count_trailing_zeros(x: UInt16) -> Int` | New | R-sized-10 |
| `floor_div` | `fn uint16.floor_div(a: UInt16, b: UInt16) -> UInt16` | New | R-sized-10 |
| `from` | `fn uint16.from[S: Numeric](x: S) -> UInt16` | New | R-L0-40 |
| `from_checked` | `fn uint16.from_checked[S: Numeric](x: S) -> UInt16?` | New | R-L0-40 |
| `from_saturating` | `fn uint16.from_saturating[S: Numeric](x: S) -> UInt16` | New | R-L0-40 |
| `from_wrapping` | `fn uint16.from_wrapping[S: Integer](x: S) -> UInt16` | New | R-L0-40 |
| `is_even` | `fn uint16.is_even(x: UInt16) -> Bool` | New | R-sized-10 |
| `is_odd` | `fn uint16.is_odd(x: UInt16) -> Bool` | New | R-sized-10 |
| `max` | `fn uint16.max(a: UInt16, b: UInt16) -> UInt16` | New | R-sized-08 |
| `max_value` | `fn uint16.max_value() -> UInt16` |  |  |
| `min` | `fn uint16.min(a: UInt16, b: UInt16) -> UInt16` | New | R-sized-08 |
| `min_value` | `fn uint16.min_value() -> UInt16` |  |  |
| `modulo` | `fn uint16.modulo(a: UInt16, b: UInt16) -> UInt16` | New | R-sized-10 |
| `parse` | `fn uint16.parse(s: String) -> Result[UInt16, String]` | New | R-sized-02 |
| `parse_radix` | `fn uint16.parse_radix(s: String, radix: Int) -> Result[UInt16, String]` | New | R-sized-06 |
| `pow` | `fn uint16.pow(base: UInt16, exp: Int) -> UInt16` | New | R-sized-10 |
| `rotate_left` | `fn uint16.rotate_left(a: UInt16, n: Int) -> UInt16` | New | R-int-12 |
| `rotate_right` | `fn uint16.rotate_right(a: UInt16, n: Int) -> UInt16` | New | R-int-12 |
| `to_string` | `fn uint16.to_string(x: UInt16) -> String` |  |  |
| `to_string_radix` | `fn uint16.to_string_radix(x: UInt16, radix: Int) -> String` | New | R-sized-06 |
| `wrap_add` | `fn uint16.wrap_add(a: UInt16, b: UInt16) -> UInt16` | New | R-int-12 |
| `wrap_mul` | `fn uint16.wrap_mul(a: UInt16, b: UInt16) -> UInt16` | New | R-int-12 |
| `wrap_sub` | `fn uint16.wrap_sub(a: UInt16, b: UInt16) -> UInt16` | New | R-int-12 |

Removed (15):

| Function | Replacement | Proposal |
| --- | --- | --- |
| `to_float32` | `float32.from` | R-L0-40 |
| `to_float64` | `float64.from` | R-L0-40 |
| `to_int16` | `int16.from` | R-L0-40 |
| `to_int16_checked` | `int16.from_checked` | R-L0-40 |
| `to_int16_saturating` | `int16.from_saturating` | R-L0-40 |
| `to_int32` | `int32.from` | R-L0-40 |
| `to_int64` | `int64.from` | R-L0-40 |
| `to_int8` | `int8.from` | R-L0-40 |
| `to_int8_checked` | `int8.from_checked` | R-L0-40 |
| `to_int8_saturating` | `int8.from_saturating` | R-L0-40 |
| `to_uint32` | `uint32.from` | R-L0-40 |
| `to_uint64` | `uint64.from` | R-L0-40 |
| `to_uint8` | `uint8.from` | R-L0-40 |
| `to_uint8_checked` | `uint8.from_checked` | R-L0-40 |
| `to_uint8_saturating` | `uint8.from_saturating` | R-L0-40 |

<a id="uint32"></a>

## uint32

Record: [L2-sized-numbers.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-sized-numbers.md)

| Function | Signature (after redesign) | Change | Proposal |
| --- | --- | --- | --- |
| `band` | `fn uint32.band(a: UInt32, b: UInt32) -> UInt32` | New | R-sized-10 |
| `bit_length` | `fn uint32.bit_length(x: UInt32) -> Int` | New | R-int-11 |
| `bnot` | `fn uint32.bnot(x: UInt32) -> UInt32` | New | R-sized-10 |
| `bor` | `fn uint32.bor(a: UInt32, b: UInt32) -> UInt32` | New | R-sized-10 |
| `bshl` | `fn uint32.bshl(a: UInt32, n: Int) -> UInt32` | New | R-sized-10 |
| `bshr` | `fn uint32.bshr(a: UInt32, n: Int) -> UInt32` | New | R-sized-10 |
| `bxor` | `fn uint32.bxor(a: UInt32, b: UInt32) -> UInt32` | New | R-sized-10 |
| `byte_swap` | `fn uint32.byte_swap(x: UInt32) -> UInt32` | New | R-sized-10 |
| `clamp` | `fn uint32.clamp(x: UInt32, lo: UInt32, hi: UInt32) -> UInt32` | New | R-sized-08 |
| `compare` | `fn uint32.compare(a: UInt32, b: UInt32) -> Int` | New | R-sized-02 |
| `count_leading_zeros` | `fn uint32.count_leading_zeros(x: UInt32) -> Int` | New | R-sized-10 |
| `count_ones` | `fn uint32.count_ones(x: UInt32) -> Int` | New | R-sized-10 |
| `count_trailing_zeros` | `fn uint32.count_trailing_zeros(x: UInt32) -> Int` | New | R-sized-10 |
| `floor_div` | `fn uint32.floor_div(a: UInt32, b: UInt32) -> UInt32` | New | R-sized-10 |
| `from` | `fn uint32.from[S: Numeric](x: S) -> UInt32` | New | R-L0-40 |
| `from_checked` | `fn uint32.from_checked[S: Numeric](x: S) -> UInt32?` | New | R-L0-40 |
| `from_saturating` | `fn uint32.from_saturating[S: Numeric](x: S) -> UInt32` | New | R-L0-40 |
| `from_wrapping` | `fn uint32.from_wrapping[S: Integer](x: S) -> UInt32` | New | R-L0-40 |
| `is_even` | `fn uint32.is_even(x: UInt32) -> Bool` | New | R-sized-10 |
| `is_odd` | `fn uint32.is_odd(x: UInt32) -> Bool` | New | R-sized-10 |
| `max` | `fn uint32.max(a: UInt32, b: UInt32) -> UInt32` | New | R-sized-08 |
| `max_value` | `fn uint32.max_value() -> UInt32` |  |  |
| `min` | `fn uint32.min(a: UInt32, b: UInt32) -> UInt32` | New | R-sized-08 |
| `min_value` | `fn uint32.min_value() -> UInt32` |  |  |
| `modulo` | `fn uint32.modulo(a: UInt32, b: UInt32) -> UInt32` | New | R-sized-10 |
| `parse` | `fn uint32.parse(s: String) -> Result[UInt32, String]` | New | R-sized-02 |
| `parse_radix` | `fn uint32.parse_radix(s: String, radix: Int) -> Result[UInt32, String]` | New | R-sized-06 |
| `pow` | `fn uint32.pow(base: UInt32, exp: Int) -> UInt32` | New | R-sized-10 |
| `rotate_left` | `fn uint32.rotate_left(a: UInt32, n: Int) -> UInt32` | New | R-int-12 |
| `rotate_right` | `fn uint32.rotate_right(a: UInt32, n: Int) -> UInt32` | New | R-int-12 |
| `to_string` | `fn uint32.to_string(x: UInt32) -> String` |  |  |
| `to_string_radix` | `fn uint32.to_string_radix(x: UInt32, radix: Int) -> String` | New | R-sized-06 |
| `wrap_add` | `fn uint32.wrap_add(a: UInt32, b: UInt32) -> UInt32` | New | R-int-12 |
| `wrap_mul` | `fn uint32.wrap_mul(a: UInt32, b: UInt32) -> UInt32` | New | R-int-12 |
| `wrap_sub` | `fn uint32.wrap_sub(a: UInt32, b: UInt32) -> UInt32` | New | R-int-12 |

Removed (19):

| Function | Replacement | Proposal |
| --- | --- | --- |
| `to_float32` | `float32.from` | R-L0-40 |
| `to_float64` | `float64.from` | R-L0-40 |
| `to_int16` | `int16.from` | R-L0-40 |
| `to_int16_checked` | `int16.from_checked` | R-L0-40 |
| `to_int16_saturating` | `int16.from_saturating` | R-L0-40 |
| `to_int32` | `int32.from` | R-L0-40 |
| `to_int32_checked` | `int32.from_checked` | R-L0-40 |
| `to_int32_saturating` | `int32.from_saturating` | R-L0-40 |
| `to_int64` | `int64.from` | R-L0-40 |
| `to_int8` | `int8.from` | R-L0-40 |
| `to_int8_checked` | `int8.from_checked` | R-L0-40 |
| `to_int8_saturating` | `int8.from_saturating` | R-L0-40 |
| `to_uint16` | `uint16.from` | R-L0-40 |
| `to_uint16_checked` | `uint16.from_checked` | R-L0-40 |
| `to_uint16_saturating` | `uint16.from_saturating` | R-L0-40 |
| `to_uint64` | `uint64.from` | R-L0-40 |
| `to_uint8` | `uint8.from` | R-L0-40 |
| `to_uint8_checked` | `uint8.from_checked` | R-L0-40 |
| `to_uint8_saturating` | `uint8.from_saturating` | R-L0-40 |

<a id="uint64"></a>

## uint64

Record: [L2-sized-numbers.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-sized-numbers.md)

| Function | Signature (after redesign) | Change | Proposal |
| --- | --- | --- | --- |
| `band` | `fn uint64.band(a: UInt64, b: UInt64) -> UInt64` | New | R-sized-10 |
| `bit_length` | `fn uint64.bit_length(x: UInt64) -> Int` | New | R-int-11 |
| `bnot` | `fn uint64.bnot(x: UInt64) -> UInt64` | New | R-sized-10 |
| `bor` | `fn uint64.bor(a: UInt64, b: UInt64) -> UInt64` | New | R-sized-10 |
| `bshl` | `fn uint64.bshl(a: UInt64, n: Int) -> UInt64` | New | R-sized-10 |
| `bshr` | `fn uint64.bshr(a: UInt64, n: Int) -> UInt64` | New | R-sized-10 |
| `bxor` | `fn uint64.bxor(a: UInt64, b: UInt64) -> UInt64` | New | R-sized-10 |
| `byte_swap` | `fn uint64.byte_swap(x: UInt64) -> UInt64` | New | R-sized-10 |
| `clamp` | `fn uint64.clamp(x: UInt64, lo: UInt64, hi: UInt64) -> UInt64` | New | R-sized-08 |
| `compare` | `fn uint64.compare(a: UInt64, b: UInt64) -> Int` | New | R-sized-02 |
| `count_leading_zeros` | `fn uint64.count_leading_zeros(x: UInt64) -> Int` | New | R-sized-10 |
| `count_ones` | `fn uint64.count_ones(x: UInt64) -> Int` | New | R-sized-10 |
| `count_trailing_zeros` | `fn uint64.count_trailing_zeros(x: UInt64) -> Int` | New | R-sized-10 |
| `floor_div` | `fn uint64.floor_div(a: UInt64, b: UInt64) -> UInt64` | New | R-sized-10 |
| `from` | `fn uint64.from[S: Numeric](x: S) -> UInt64` | New | R-L0-40 |
| `from_checked` | `fn uint64.from_checked[S: Numeric](x: S) -> UInt64?` | New | R-L0-40 |
| `from_saturating` | `fn uint64.from_saturating[S: Numeric](x: S) -> UInt64` | New | R-L0-40 |
| `from_wrapping` | `fn uint64.from_wrapping[S: Integer](x: S) -> UInt64` | New | R-L0-40 |
| `is_even` | `fn uint64.is_even(x: UInt64) -> Bool` | New | R-sized-10 |
| `is_odd` | `fn uint64.is_odd(x: UInt64) -> Bool` | New | R-sized-10 |
| `max` | `fn uint64.max(a: UInt64, b: UInt64) -> UInt64` | New | R-sized-08 |
| `max_value` | `fn uint64.max_value() -> UInt64` |  |  |
| `min` | `fn uint64.min(a: UInt64, b: UInt64) -> UInt64` | New | R-sized-08 |
| `min_value` | `fn uint64.min_value() -> UInt64` |  |  |
| `modulo` | `fn uint64.modulo(a: UInt64, b: UInt64) -> UInt64` | New | R-sized-10 |
| `parse` | `fn uint64.parse(s: String) -> Result[UInt64, String]` | New | R-sized-02 |
| `parse_radix` | `fn uint64.parse_radix(s: String, radix: Int) -> Result[UInt64, String]` | New | R-sized-06 |
| `pow` | `fn uint64.pow(base: UInt64, exp: Int) -> UInt64` | New | R-sized-10 |
| `rotate_left` | `fn uint64.rotate_left(a: UInt64, n: Int) -> UInt64` | New | R-int-12 |
| `rotate_right` | `fn uint64.rotate_right(a: UInt64, n: Int) -> UInt64` | New | R-int-12 |
| `to_string` | `fn uint64.to_string(x: UInt64) -> String` |  |  |
| `to_string_radix` | `fn uint64.to_string_radix(x: UInt64, radix: Int) -> String` | New | R-sized-06 |
| `wrap_add` | `fn uint64.wrap_add(a: UInt64, b: UInt64) -> UInt64` | New | R-int-12 |
| `wrap_mul` | `fn uint64.wrap_mul(a: UInt64, b: UInt64) -> UInt64` | New | R-int-12 |
| `wrap_sub` | `fn uint64.wrap_sub(a: UInt64, b: UInt64) -> UInt64` | New | R-int-12 |

Removed (23):

| Function | Replacement | Proposal |
| --- | --- | --- |
| `to_float32` | `float32.from` | R-L0-40 |
| `to_float64` | `float64.from` | R-L0-40 |
| `to_int16` | `int16.from` | R-L0-40 |
| `to_int16_checked` | `int16.from_checked` | R-L0-40 |
| `to_int16_saturating` | `int16.from_saturating` | R-L0-40 |
| `to_int32` | `int32.from` | R-L0-40 |
| `to_int32_checked` | `int32.from_checked` | R-L0-40 |
| `to_int32_saturating` | `int32.from_saturating` | R-L0-40 |
| `to_int64` | `int64.from` | R-L0-40 |
| `to_int64_checked` | `int64.from_checked` | R-L0-40 |
| `to_int64_saturating` | `int64.from_saturating` | R-L0-40 |
| `to_int8` | `int8.from` | R-L0-40 |
| `to_int8_checked` | `int8.from_checked` | R-L0-40 |
| `to_int8_saturating` | `int8.from_saturating` | R-L0-40 |
| `to_uint16` | `uint16.from` | R-L0-40 |
| `to_uint16_checked` | `uint16.from_checked` | R-L0-40 |
| `to_uint16_saturating` | `uint16.from_saturating` | R-L0-40 |
| `to_uint32` | `uint32.from` | R-L0-40 |
| `to_uint32_checked` | `uint32.from_checked` | R-L0-40 |
| `to_uint32_saturating` | `uint32.from_saturating` | R-L0-40 |
| `to_uint8` | `uint8.from` | R-L0-40 |
| `to_uint8_checked` | `uint8.from_checked` | R-L0-40 |
| `to_uint8_saturating` | `uint8.from_saturating` | R-L0-40 |

<a id="uint8"></a>

## uint8

Record: [L2-sized-numbers.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-sized-numbers.md)

| Function | Signature (after redesign) | Change | Proposal |
| --- | --- | --- | --- |
| `band` | `fn uint8.band(a: UInt8, b: UInt8) -> UInt8` | New | R-sized-10 |
| `bit_length` | `fn uint8.bit_length(x: UInt8) -> Int` | New | R-int-11 |
| `bnot` | `fn uint8.bnot(x: UInt8) -> UInt8` | New | R-sized-10 |
| `bor` | `fn uint8.bor(a: UInt8, b: UInt8) -> UInt8` | New | R-sized-10 |
| `bshl` | `fn uint8.bshl(a: UInt8, n: Int) -> UInt8` | New | R-sized-10 |
| `bshr` | `fn uint8.bshr(a: UInt8, n: Int) -> UInt8` | New | R-sized-10 |
| `bxor` | `fn uint8.bxor(a: UInt8, b: UInt8) -> UInt8` | New | R-sized-10 |
| `byte_swap` | `fn uint8.byte_swap(x: UInt8) -> UInt8` | New | R-sized-10 |
| `clamp` | `fn uint8.clamp(x: UInt8, lo: UInt8, hi: UInt8) -> UInt8` | New | R-sized-08 |
| `compare` | `fn uint8.compare(a: UInt8, b: UInt8) -> Int` | New | R-sized-02 |
| `count_leading_zeros` | `fn uint8.count_leading_zeros(x: UInt8) -> Int` | New | R-sized-10 |
| `count_ones` | `fn uint8.count_ones(x: UInt8) -> Int` | New | R-sized-10 |
| `count_trailing_zeros` | `fn uint8.count_trailing_zeros(x: UInt8) -> Int` | New | R-sized-10 |
| `floor_div` | `fn uint8.floor_div(a: UInt8, b: UInt8) -> UInt8` | New | R-sized-10 |
| `from` | `fn uint8.from[S: Numeric](x: S) -> UInt8` | New | R-L0-40 |
| `from_checked` | `fn uint8.from_checked[S: Numeric](x: S) -> UInt8?` | New | R-L0-40 |
| `from_saturating` | `fn uint8.from_saturating[S: Numeric](x: S) -> UInt8` | New | R-L0-40 |
| `from_wrapping` | `fn uint8.from_wrapping[S: Integer](x: S) -> UInt8` | New | R-L0-40 |
| `is_even` | `fn uint8.is_even(x: UInt8) -> Bool` | New | R-sized-10 |
| `is_odd` | `fn uint8.is_odd(x: UInt8) -> Bool` | New | R-sized-10 |
| `max` | `fn uint8.max(a: UInt8, b: UInt8) -> UInt8` | New | R-sized-08 |
| `max_value` | `fn uint8.max_value() -> UInt8` |  |  |
| `min` | `fn uint8.min(a: UInt8, b: UInt8) -> UInt8` | New | R-sized-08 |
| `min_value` | `fn uint8.min_value() -> UInt8` |  |  |
| `modulo` | `fn uint8.modulo(a: UInt8, b: UInt8) -> UInt8` | New | R-sized-10 |
| `parse` | `fn uint8.parse(s: String) -> Result[UInt8, String]` | New | R-sized-02 |
| `parse_radix` | `fn uint8.parse_radix(s: String, radix: Int) -> Result[UInt8, String]` | New | R-sized-06 |
| `pow` | `fn uint8.pow(base: UInt8, exp: Int) -> UInt8` | New | R-sized-10 |
| `rotate_left` | `fn uint8.rotate_left(a: UInt8, n: Int) -> UInt8` | New | R-int-12 |
| `rotate_right` | `fn uint8.rotate_right(a: UInt8, n: Int) -> UInt8` | New | R-int-12 |
| `to_string` | `fn uint8.to_string(x: UInt8) -> String` |  |  |
| `to_string_radix` | `fn uint8.to_string_radix(x: UInt8, radix: Int) -> String` | New | R-sized-06 |
| `wrap_add` | `fn uint8.wrap_add(a: UInt8, b: UInt8) -> UInt8` | New | R-int-12 |
| `wrap_mul` | `fn uint8.wrap_mul(a: UInt8, b: UInt8) -> UInt8` | New | R-int-12 |
| `wrap_sub` | `fn uint8.wrap_sub(a: UInt8, b: UInt8) -> UInt8` | New | R-int-12 |

Removed (11):

| Function | Replacement | Proposal |
| --- | --- | --- |
| `to_float32` | `float32.from` | R-L0-40 |
| `to_float64` | `float64.from` | R-L0-40 |
| `to_int16` | `int16.from` | R-L0-40 |
| `to_int32` | `int32.from` | R-L0-40 |
| `to_int64` | `int64.from` | R-L0-40 |
| `to_int8` | `int8.from` | R-L0-40 |
| `to_int8_checked` | `int8.from_checked` | R-L0-40 |
| `to_int8_saturating` | `int8.from_saturating` | R-L0-40 |
| `to_uint16` | `uint16.from` | R-L0-40 |
| `to_uint32` | `uint32.from` | R-L0-40 |
| `to_uint64` | `uint64.from` | R-L0-40 |

<a id="unix"></a>

## unix

New module (added at L1).
Record: [L2-unix.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-unix.md)

| Function | Signature (after redesign) | Change | Proposal |
| --- | --- | --- | --- |
| `accept` | `effect fn unix.accept(listener: Fd) -> Fd` | New. Moved | R-unix-01 |
| `connect` | `effect fn unix.connect(path: String) -> Fd` | New. Moved from `net.unix_connect` | R-unix-01 |
| `listen` | `effect fn unix.listen(path: String) -> Fd` | New. Moved | R-unix-01 |
| `select` | `effect fn unix.select(xs: List[Fd], timeout_ms: Int) -> List[Fd]` | New. Moved; absorbs `unix_poll` | R-unix-01、R-unix-02 |
| `shm_create` | `effect fn unix.shm_create(size: Int) -> Fd` | New. Moved | R-unix-01 |
| `shm_read` | `effect fn unix.shm_read(fd: Fd, offset: Int, len: Int) -> Bytes` | New. Moved | R-unix-01 |
| `shm_resize` | `effect fn unix.shm_resize(fd: Fd, size: Int) -> Unit` | New. Moved | R-unix-01 |
| `shm_write` | `effect fn unix.shm_write(fd: Fd, offset: Int, data: Bytes) -> Unit` | New. Moved | R-unix-01 |
| `take_fds` | `effect fn unix.take_fds(sock: Fd) -> List[Fd]` | New. Moved | R-unix-01 |
| `write_fds` | `effect fn unix.write_fds(sock: Fd, data: Bytes, fds: List[Fd]) -> Unit` | New. Moved | R-unix-01、R-unix-04 |

<a id="url"></a>

## url

Record: [L2-url.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-url.md)

| Function | Signature (after redesign) | Change | Proposal |
| --- | --- | --- | --- |
| `build_query` | `fn url.build_query(xs: List[(String, String)]) -> String` |  | R-url-02 |
| `decode_component` | `fn url.decode_component(s: String) -> Result[String, String]` |  | R-url-01 |
| `decode_form` | `fn url.decode_form(s: String) -> Result[String, String]` | New. Replacement for `http.url_decode` | R-url-01 |
| `encode_component` | `fn url.encode_component(s: String) -> String` |  | R-url-01 |
| `encode_form` | `fn url.encode_form(s: String) -> String` | New | R-url-01 |
| `parse` | `fn url.parse(s: String) -> Result[Url, String]` | Semantic change: accepts userinfo; adds `userinfo` fields to `Url` | R-url-05 |
| `parse_query` | `fn url.parse_query(query: String) -> Result[List[(String, String)], String]` | New | R-url-02 |
| `resolve` | `fn url.resolve(base: Url, ref: String) -> Result[Url, String]` | New | R-url-03 |
| `to_string` | `fn url.to_string(u: Url) -> String` |  |  |

Removed (1):

| Function | Replacement | Proposal |
| --- | --- | --- |
| `query_pairs` | `parse_query` | R-url-02 |

<a id="uuid"></a>

## uuid

New module (added at L1).
Record: [L2-uuid.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-uuid.md)

| Function | Signature (after redesign) | Change | Proposal |
| --- | --- | --- | --- |
| `compare` | `fn uuid.compare(a: Uuid, b: Uuid) -> Int` | New | R-uuid-01 |
| `from_bytes` | `fn uuid.from_bytes(b: Bytes) -> Result[Uuid, String]` | New | R-uuid-01 |
| `parse` | `fn uuid.parse(s: String) -> Result[Uuid, String]` | New | R-uuid-01 |
| `to_bytes` | `fn uuid.to_bytes(u: Uuid) -> Bytes` | New | R-uuid-01 |
| `to_string` | `fn uuid.to_string(u: Uuid) -> String` | New | R-uuid-01 |
| `v4` | `effect fn uuid.v4() -> Uuid` | New | R-uuid-01 |
| `v7` | `effect fn uuid.v7() -> Uuid` | New | R-uuid-01 |

<a id="value"></a>

## value

Record: [L2-value.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-value.md)

| Function | Signature (after redesign) | Change | Proposal |
| --- | --- | --- | --- |
| `array` | `fn value.array(xs: List[Value]) -> Value` |  | R-value-02 |
| `as_array` | `fn value.as_array(v: Value) -> Result[List[Value], String]` |  | R-value-02 |
| `as_bool` | `fn value.as_bool(v: Value) -> Result[Bool, String]` |  | R-value-02 |
| `as_float` | `fn value.as_float(v: Value) -> Result[Float, String]` |  | R-value-02 |
| `as_int` | `fn value.as_int(v: Value) -> Result[Int, String]` |  | R-value-02 |
| `as_object` | `fn value.as_object(v: Value) -> Result[List[(String, Value)], String]` | New | R-value-02 |
| `as_string` | `fn value.as_string(v: Value) -> Result[String, String]` |  | R-value-02 |
| `bool` | `fn value.bool(b: Bool) -> Value` |  | R-value-02 |
| `float` | `fn value.float(f: Float) -> Value` |  | R-value-02 |
| `get` | `fn value.get(v: Value, key: String) -> Value?` | New | R-value-07 |
| `get_array` | `fn value.get_array(v: Value, key: String) -> List[Value]?` | New. Move from `json.get_array` | R-value-02 |
| `get_bool` | `fn value.get_bool(v: Value, key: String) -> Bool?` | New. Move from `json.get_bool` | R-value-02 |
| `get_float` | `fn value.get_float(v: Value, key: String) -> Float?` | New. Move from `json.get_float` | R-value-02 |
| `get_int` | `fn value.get_int(v: Value, key: String) -> Int?` | New. Move from `json.get_int` | R-value-02 |
| `get_object` | `fn value.get_object(v: Value, key: String) -> List[(String, Value)]?` | New | R-value-02 |
| `get_path` | `fn value.get_path(v: Value, p: Path) -> Value?` | New. Move from `json.get_path` | R-value-03 |
| `get_string` | `fn value.get_string(v: Value, key: String) -> String?` | New. Move from `json.get_string` | R-value-02 |
| `index` | `fn value.index(p: Path, i: Int) -> Path` | New. Move from `json.index`. Negative is out of range | R-value-03 |
| `int` | `fn value.int(n: Int) -> Value` |  | R-value-02 |
| `key` | `fn value.key(p: Path, name: String) -> Path` | New. Move from `json.field` | R-value-03 |
| `merge` | `fn value.merge(a: Value, b: Value) -> Value` |  |  |
| `null` | `fn value.null() -> Value` |  |  |
| `object` | `fn value.object(xs: List[(String, Value)]) -> Value` |  | R-value-02 |
| `omit` | `fn value.omit(v: Value, keys: List[String]) -> Value` |  |  |
| `pick` | `fn value.pick(v: Value, keys: List[String]) -> Value` |  |  |
| `remove_path` | `fn value.remove_path(v: Value, p: Path) -> Value` | New. Move from `json.remove_path` | R-value-03 |
| `root` | `fn value.root() -> Path` | New. Move from `json.root` | R-value-03 |
| `set_path` | `fn value.set_path(v: Value, p: Path, x: Value) -> Result[Value, String]` | New. Move from `json.set_path` | R-value-03 |
| `string` | `fn value.string(s: String) -> Value` | Renamed (formerly `str`) | R-value-08 |

Removed (5):

| Function | Replacement | Proposal |
| --- | --- | --- |
| `field` | `value.get`（Option） | R-value-07 |
| `keys` | `as_object` | R-value-04 |
| `stringify` | `json.stringify` | R-value-01 |
| `to_camel_case` | No replacement (users write it) | R-value-05 |
| `to_snake_case` | No replacement (users write it) | R-value-05 |

<a id="zlib"></a>

## zlib

Record: [L2-zlib.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-zlib.md)

| Function | Signature (after redesign) | Change | Proposal |
| --- | --- | --- | --- |
| `compress` | `effect fn zlib.compress(b: Bytes) -> Bytes` |  | R-zlib-03 |
| `compress_level` | `effect fn zlib.compress_level(b: Bytes, level: Int) -> Bytes` |  | R-zlib-03 |
| `decompress` | `fn zlib.decompress(b: Bytes) -> Result[Bytes, String]` | Semantic change: remove effect, use `fn … -> Result[Bytes, String]`. Error if output exceeds 256 MiB | R-zlib-02、R-zlib-04 |
| `decompress_max` | `fn zlib.decompress_max(b: Bytes, max_len: Int) -> Result[Bytes, String]` | New | R-zlib-04 |
| `deflate` | `effect fn zlib.deflate(b: Bytes) -> Bytes` |  | R-zlib-03 |
| `deflate_level` | `effect fn zlib.deflate_level(b: Bytes, level: Int) -> Bytes` |  | R-zlib-03 |
| `gunzip` | `fn zlib.gunzip(b: Bytes) -> Result[Bytes, String]` | Semantic change: remove effect, use `fn … -> Result[Bytes, String]`. Error if output exceeds 256 MiB | R-zlib-02、R-zlib-04 |
| `gunzip_max` | `fn zlib.gunzip_max(b: Bytes, max_len: Int) -> Result[Bytes, String]` | New | R-zlib-04 |
| `gzip` | `effect fn zlib.gzip(b: Bytes) -> Bytes` |  | R-zlib-03 |
| `inflate` | `fn zlib.inflate(b: Bytes) -> Result[Bytes, String]` | Semantic change: remove effect, use `fn … -> Result[Bytes, String]`. Error if output exceeds 256 MiB | R-zlib-02、R-zlib-04 |
| `inflate_max` | `fn zlib.inflate_max(b: Bytes, max_len: Int) -> Result[Bytes, String]` | New | R-zlib-04 |

Removed (2):

| Function | Replacement | Proposal |
| --- | --- | --- |
| `adler32` | `hash.adler32` | R-zlib-01 |
| `crc32` | `hash.crc32` | R-zlib-01 |

<a id="errorモジュールごと撤去"></a>

## error (entire module removed)

| Function | Replacement | Proposal |
| --- | --- | --- |
| `chain` | `"${a}: ${b}"` (string interpolation) | R-error-01 |
| `context` | `result.map_err(r, (e) => "${msg}: ${e}")` | R-error-01、R-error-02 |
| `message` | `match`, or `result.to_err_option(r) ?? ""` | R-error-01 |


<a id="採らなかった案保留の案と理由"></a>

## Rejected and deferred proposals, with reasons

To avoid repeating the same review comments, this lists the still-effective rejected (×) and deferred proposals (54), with the first one or two sentences of each reason. The full text is in the proposal on each page.

| Proposal | Content | Decision | Reason |
|---|---|---|---|
| [R-L0-18](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md#r-l0-18-型を変換する演算子-as-は足さない---不採用) | Do not add the type-conversion operator `as` | × Rejected | Numeric conversion already has named functions under R-L0-08; this would violate R-L0-01 (one spelling per role). The other three uses are unnecessary given Almide's type design, or conflict with its purpose. |
| [R-L1-20](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L1-categories.md#r-l1-20-固定幅の数のモジュールint8uint64float32float64---統合しない) | Fixed-width numeric modules (`int8`–`uint64`, `float32`, `float64`) | × Do not merge | (a) Zero surveyed languages have a module per type. However, the majority attach methods to types. |
| [R-L1-21](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L1-categories.md#r-l1-21-compute-と-duration---統合しないadr-0001) | `compute` and `duration` | × Do not merge (ADR-0001) | ADR-0001 defines deterministic computation time (`Compute`) and real time (`Duration`) as separate, mutually non-convertible types. |
| [R-L1-23](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L1-categories.md#r-l1-23-hex-と-base64-を別のモジュールのまま残す---まとめない) | Keep `hex` and `base64` as separate modules | × Do not combine | Five languages combine them, but their combined modules have no common name (Elixir `Base`, Deno `@std/encoding`, Python's `b16encode` inside `base64`). |
| [R-L1-29](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L1-categories.md#r-l1-29-html-と-testing---動かさない) | `html` and `testing` | × Do not move | `html` has five functions. Giving escaped HTML a separate type (SafeHtml) follows the same idea as Go's `html/template`, preventing mix-ups through typing. |
| [R-base64-02](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-base64.md#r-base64-02--を付けない符号化を足す---不採用) | Add encoding without `=` | × Rejected | Five languages offer it, but all use an argument or setting (`padding: false`, `omitPadding`), not a separate name. |
| [R-base64-03](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-base64.md#r-base64-03-hex-とモジュールをまとめる---不採用r-l1-23) | Combine the module with hex | × Rejected (R-L1-23) | As stated in R-L1-23, there is no majority name for the combined module. |
| [R-bigint-02](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-bigint.md#r-bigint-02-123n-のような-bigint-のリテラルを足す---不採用) | Add BigInt literals such as `123n` | × Rejected | Do not add them. Write `bigint.parse("123…")!` or `int.to_bigint(123)`. |
| [R-bigint-03](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-bigint.md#r-bigint-03-int-と-bigint-を混ぜた演算を自動で広げる---不採用) | Automatically widen mixed `Int` and `BigInt` arithmetic | × Rejected | Do not do so. If one operand of `a + b` is `Int` and the other `BigInt`, report a type error and suggest `int.to_bigint(a)` in the diagnostic hint. |
| [R-bytes-21](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-bytes.md#r-bytes-21-bytesskip-を-next_-の形にする---不採用) | Put `bytes.skip` into the `next_` form | × Rejected | `skip` advances position without reading a value, so it has no value for `(値, 次の状態)` (value, next state). Returning `(Unit, Int)` only adds a value for the caller to discard. |
| [R-bytes-24](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-bytes.md#r-bytes-24-bytesset-と-put_u8-を-1-つにする---不採用) | Unify `bytes.set` and `put_u8` | × Rejected | `set(b, i, v) -> Bytes` returns a copy, whereas `put_u8(mut b, pos, v)` mutates in place. They form the R-L0-04 pair: copying by default, designated verbs for in-place mutation. |
| [R-compute-02](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-compute.md#r-compute-02-duration-と統合するまたは読み出し関数を足す---不採用) | Merge with `duration`, or add readout functions | × Rejected | Do neither, following ADR-0001 (the clocks are separate, mutually non-convertible types; readout omitted in S7) and L1 R-L1-21. |
| [R-datetime-02](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-datetime.md#r-datetime-02-時刻専用の型instant-などを作る---不採用) | Create a dedicated timestamp type, such as `Instant` | × Rejected | R-L0-00. R-L0-14 item 4 now permits changing the type under the same name if old usage becomes a type error. |
| [R-datetime-04](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-datetime.md#r-datetime-04-weekday-を数を返す関数にする---不採用) | Make `weekday` return a number | × Rejected | R-L0-00. Changing the return type to `Int` makes old usage (`== "Monday"`) a type error, so R-L0-14 item 4 allows this change. |
| [R-datetime-05](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-datetime.md#r-datetime-05-format-の--を--にする---不採用) | Change `format`'s `%%` to `%` | × Rejected | R-L0-14 item 4 prohibits same-name changes that let old usage pass type checking but produce a different result. Changing the output of `%%` is precisely that case. |
| [R-duration-02](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-duration.md#r-duration-02-読み出し関数小数の構築子日の単位を足す---不採用) | Add readout functions, fractional constructors and a day unit | × Rejected | Do not add them, as recorded in ADR-0001 S7 and the 2026-10-16 ruling withdrawing the readout-function proposal. |
| [R-duration-04](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-duration.md#r-duration-04-min分を-minutes-などの完全な綴りにする---不採用) | Expand `min` (minutes) to a full spelling such as `minutes` | × Rejected | ADR-0001 S2 fixes the six unit spellings. Writing `duration.min(a, b)` intending a minimum produces E004 for wrong argument count, not a silently different value. |
| [R-error-02](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-error.md#r-error-02-errorcontext-を-resultcontext-として残す---不採用) | Keep `error.context` as `result.context` | × Rejected | R-L0-01. ADR-0004 Alternatives 7 already rejected `result.context` for this reason (#1104 closed as rejected). |
| [R-float-06](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-float.md#r-float-06-floatminmax-の-nan-の扱いを-listminmax-とそろえる--保留) | Align NaN handling in `float.min`/`max` with `list.min`/`max` | Deferred | Neither NaN-propagating languages (Go, JS, Kotlin, OCaml) nor NaN-ignoring languages (Rust, Swift) reach five. This would change behavior fixed by contracts C-049 and C-306. |
| [R-float-08](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-float.md#r-float-08-float-の床方向の剰余-floatmod-を足す---不採用) | Add floor-based Float modulo `float.mod` | × Rejected | Only three languages offer floor-based Float modulo: Python `%`, Kotlin `mod`, Gleam `float.modulo`. This misses the five-language threshold. |
| [R-fs-04](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-fs.md#r-fs-04-59-以上が持つが-almide-に無い役割---不採用いまは足さない) | Roles present in at least 5/9 languages but absent from Almide | × Rejected (do not add now) | For each role, existing functions suffice (`mkdir` via `mkdir_p`; handles via the line-streaming family and `append`), there is no WASI route (chmod), or demand has not appeared. Reasons per role are in the rows below. |
| [R-heap-04](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-heap.md#r-heap-04-new_minnew_max-に分ける---不採用) | Split into `new_min` and `new_max` | × Rejected | The default is minimum-first, as in Python, Go, OCaml and Kotlin (R-L1-02). For maximum-first use `heap.new_with((a, b) => int.compare(b, a))` (not the difference `b - a`, which overflows at `Int` limits), or `new_by`. |
| [R-hex-02](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-hex.md#r-hex-02-decode_upper-を足す---不採用) | Add `decode_upper` | × Rejected | Only Python `b16decode` defaults to decoding that rejects a different letter case (F). Under rule 2, `decode` accepts spelling variants. |
| [R-html-02](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-html.md#r-html-02-unescape-を足す---不採用) | Add `unescape` | × Rejected | Only Go, Python and Deno have it in their standard libraries, three languages, below L1's threshold of at least five. Reading HTML belongs to a package (an HTML parser). |
| [R-html-03](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-html.md#r-html-03-safehtml-の列をまとめる-htmljoin-を足す---不採用) | Add `html.join` to combine a sequence of `SafeHtml` | × Rejected | Only two languages have an escaped-content type, below the five-language threshold. Use `list.fold(xs, html.empty(), html.concat)`. |
| [R-int-05](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-int.md#r-int-05-bandborbxor-などを多数派の綴りに改名する---不採用改名しないr-int-10-で確かめた) | Rename `band`/`bor`/`bxor` and similar functions to majority spellings | × Rejected (no renaming; checked in R-int-10) | R-L0-00. The four languages expressing bit operations as named functions disagree in spelling: Gleam `bitwise_and`, Elixir `band`, OCaml `logand`, Kotlin `and`, all (F). |
| [R-list-12](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-list.md#r-list-12-多数派に届かない役割は足さない---不採用) | Do not add roles below the majority threshold | × Rejected | The same 5/9 line as L1's category-addition criterion, plus R-L0-01. |
| [R-list-18](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-list.md#r-list-18-listrepeat-を列全体の繰り返しにする---不採用) | Make `list.repeat` repeat a whole sequence | × Rejected | `string.repeat` and `bytes.repeat` repeat the entire sequence; `list.repeat(v, n)` places n copies of a single value in sequence. |
| [R-map-04](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-map.md#r-map-04-mapmap-を-map_values-に改名するまたはコールバックを-k-v-にする--保留六巡目に--から変更dojo-で測る) | Rename `map.map` to `map_values`, or change its callback to `(K, V)` | Deferred (changed from × in the sixth review; measure in Dojo) | The previous reason was “mistakes become type errors,” but R-L0-36 says that alone is not a reason for rejection. Three candidates remain: current `map.map` with `(V)`, `map.map` with `(K, V)`, or renaming to `map_values`. Other languages also differ: Swift/Kotlin `mapValues`, OCaml `map` (value) and `mapi` (key and value), Gleam `map_values` (key and value). |
| [R-map-05](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-map.md#r-map-05-取り出して消す-pop合わせ方つきの-merge_withキーを変換する-map_keys-を足す---不採用) | Add removing lookup `pop`, customizable `merge_with` and key-transforming `map_keys` | × Rejected | R-L0-04 (`pop` is the verb for in-place removal from the end), L1's addition threshold (at least 5/9), and R-L0-00. |
| [R-option-03](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-option.md#r-option-03-抜けている役割の関数を足さない---不採用) | Do not add functions for missing roles | × Rejected | R-L0-00, R-L0-01, R-L0-06. |
| [R-random-04](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-random.md#r-random-04-randomint-の-max--min-を-abort-にする---不採用) | Make `random.int` abort for `max < min` | × Rejected | Do not change it. Returning `min` when `max <= min` is not a failure-hiding value but the defined answer for a range collapsed to one point; it is outside R-L0-06 item 4 (fallible construction/conversion must not return sentinels). |
| [R-random-06](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-random.md#r-random-06-randomintmin-max-の上端を含むことを変える---不採用文書だけ) | Change the inclusive upper bound of `random.int(min, max)` | × Rejected (documentation only) | Although many languages exclude the upper bound (Go, Kotlin, OCaml, Gleam, Rust), R-L0-14.4 prohibits changing meaning under the same name. |
| [R-regex-03](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-regex.md#r-regex-03-パイプの取り違えを宣言の印で止める---不採用r-regex-02-の型で止まるため) | Prevent pipe mix-ups with a declaration marker | × Rejected (R-regex-02's types already prevent them) | A marker and separate types both turn mistaken calls into compile errors, with no change in survival rate. There is no reason to add a new language mechanism for this. |
| [R-regex-06](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-regex.md#r-regex-06-名前つきグループを足さない---不採用) | Do not add named groups | × Rejected | Do not add `(?<name>...)` or APIs for lookup by name. |
| [R-regex-09](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-regex.md#r-regex-09-対象の文字列を第-1-引数にする---不採用) | Put the subject string in the first argument | × Rejected | The `regex` module's subject is `Regex`, following R-L0-02 and R-L0-03: place functions in the subject type's module and put the subject first. |
| [R-result-02](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-result.md#r-result-02-resultfilter-の-3-引数を据え置く---不採用撤去も改名もしない) | Keep the three arguments of `result.filter` | × Rejected (neither remove nor rename) | R-L0-00 and R-L0-14 item 4 (no reassignment of meaning). |
| [R-result-03](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-result.md#r-result-03-resultto_err_option-を改名しない---不採用) | Do not rename `result.to_err_option` | × Rejected | R-L0-00, R-L0-07. |
| [R-result-04](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-result.md#r-result-04-抜けている役割の関数を足さない---不採用) | Do not add functions for missing roles | × Rejected | R-L0-00, R-L0-01, R-L0-06. |
| [R-set-02](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-set.md#r-set-02-setis_superset-を足す---不採用) | Add `set.is_superset` | × Rejected | R-L0-01 (do not give two spellings to the same test with only arguments exchanged), and L1's addition threshold (at least 5/9). |
| [R-set-03](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-set.md#r-set-03-setpartition-を足す---不採用) | Add `set.partition` | × Rejected | L1's addition threshold (at least 5/9) and completeness rule 1 (`partition` is not in the seven-member family). |
| [R-set-04](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-set.md#r-set-04-setinsert-を-setadd-に改名する---不採用) | Rename `set.insert` to `set.add` | × Rejected | R-L0-00 (do not move without a majority) and R-L0-04 (`insert` is consistently the copying-side verb across modules). |
| [R-sized-04](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-sized-numbers.md#r-sized-04-floatto_整数_saturating-8-関数を撤去する---取り消しr-l0-32) | Remove the eight `float.to_<整数>_saturating` functions | × Withdrawn (R-L0-32) | Under R-L0-32, unmarked `float.to_int8` and similar conversions abort out of range. `_saturating` is no longer a synonym: it becomes the only cell retaining the existing saturation behavior. |
| [R-string-08](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-string.md#r-string-08-stringsplit_whitespace-を足す---不採用) | Add `string.split_whitespace` | × Rejected | Only 4/9 have dedicated functions: Rust (F), Go `Fields` (F), Python `split()` (F), Elixir `split/1` (F). This misses L1's “at least five languages” threshold. |
| [R-string-10](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-string.md#r-string-10-指定文字の-trimsplitnコードポイントのリスト化を足す---不採用) | Add specified-character trim, `splitn`, and conversion to a code-point list | × Rejected | R-L0-00 and L1's decision criteria. |
| [R-string-13](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-string.md#r-string-13-stringcount-を述語で数える形にする---不採用) | Make `string.count` count by predicate | × Rejected | For strings, the majority meaning of “count” is substring occurrences: Rust `matches().count()`, Go `strings.Count`, Python `str.count`, Elixir `String.count`, etc. |
| [R-url-04](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-url.md#r-url-04-url-の欄を-option-にする---不採用) | Make `Url` fields Option-valued | × Rejected | Few situations need to distinguish “absent” from “empty” for `query` and `fragment`; changing field types breaks every `u.query` read (R-L0-14 item 4). |
| [R-zlib-03](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-zlib.md#r-zlib-03-圧縮の出力を両-target-で同じにしeffect-を外す--保留) | Make compressed output identical on both targets and remove `effect` | Deferred | Replacing native's host zlib with a self-hosted encoder is expected to reduce speed and compression ratio. |
| [R-env-04](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-env.md#r-env-04-待ち時間タイムアウトの引数を-duration-にする---不採用) | Use `Duration` for waiting and timeout arguments | × Rejected | Of nine languages, only four accept a time type for waiting: Go (`time.Duration`), Rust (`Duration`), Swift (`Duration`), and Kotlin (the `Duration` overload of `delay`). This misses the five-language threshold. |
| [R-heap-05](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-heap.md#r-heap-05-heapfoldheapfind-を足す---不採用) | Add `heap.fold` and `heap.find` | × Rejected | Iteration in internal (array) order exposes that order, so implementation changes alter results. Iteration in removal (ascending) order hides a sort not apparent from the name. |
| [R-result-06](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-result.md#r-result-06-resultcollect-を戻す---不採用) | Restore `result.collect` | × Rejected | This function was removed by ADR-0007 D1. For sequences of fallible elements, write `!` in the `list.map` callback (stops at the first err. |
| [R-int-08](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-int.md#r-int-08-intto_hex-を-to_string_radixx-16-にまとめる---不採用) | Consolidate `int.to_hex` into `to_string_radix(x, 16)` | × Rejected | They differ for negative values and are not synonyms. `int.to_hex(-1)` gives the hexadecimal 64-bit pattern (`"ffffffffffffffff"`); `int.to_string_radix(-1, 16)` is signed (`"-1"`). |
| [R-int-09](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-int.md#r-int-09-intsign-と-floatsignum-の綴りを-1-つにする---不採用) | Unify the spellings of `int.sign` and `float.signum` | × Rejected | They handle zero differently; sharing a name reintroduces the mistake avoided in R-float-05. `int.sign(0)` and `bigint.sign(0)` yield 0, whereas `float.signum(0.0)` yields 1.0, as with Rust's `signum`. |
| [R-bytes-26](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-bytes.md#r-bytes-26-bytesget_u8-と-getappend_u8-と-push-を-1-つにする---不採用役割の違いを明記) | Unify `bytes.get_u8` with `get`, and `append_u8` with `push` | × Rejected (clarify differing roles) | The pairs produce the same results but belong to different families. `get`/`push` are container-family operations (shaped like `list.get`/`deque.push`); `get_u8`/`append_u8` belong to the encoding family, the one-byte cells alongside `get_u16_le`/`append_u16_le` … . |

<a id="後の案で判定を変えた案"></a>

## Proposals whose decisions changed in later proposals

These 11 proposals were initially rejected or deferred, then adopted or replaced by later proposals. The right column names the currently effective proposal. They are not included in the table above.

| Proposal | Content | Original decision | Currently effective proposal |
|---|---|---|---|
| [R-L0-20](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md#r-l0-20-critical-プロファイルで黙って折り返す-to_型-を拒む---取り消し-r-l0-32-で解消) | Reject silently wrapping `to_<型>` conversions in the critical profile | × Withdrawn (→ resolved by R-L0-32) | R-L0-32 |
| [R-L0-21](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md#r-l0-21-無印の-to_型-をはみ出したら失敗する版に変える--保留--r-l0-32r-l0-34-で解消) | Change unmarked `to_<型>` to fail on out-of-range values | Deferred → resolved by R-L0-32 and R-L0-34 | R-L0-32・R-L0-34 |
| [R-hash-04](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-hash.md#r-hash-04-_hex-のセルの入力を-bytes-に変える---不採用-r-hash-05-で採用に変更) | Change `_hex` cells to accept Bytes | × Rejected (→ adopted in R-hash-05) | R-hash-05 |
| [R-http-07](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-http.md#r-http-07-応答のヘッダの-get_header-と要求の-req_header-の不揃い---不採用規則を書く-r-http-12-で解消) | Inconsistency between response-header `get_header` and request-header `req_header` | × Rejected (document the rule; → resolved by R-http-12) | R-http-12 |
| [R-io-04](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-io.md#r-io-04-標準入力をバイト列で全部読む-ioread_all_bytes-を足す---取り消し十三巡目ioread_to_endiostdin-で書く--r-io-05) | Add `io.read_all_bytes` to read all standard input as bytes | × Withdrawn (thirteenth review; use `io.read_to_end(io.stdin())` → R-io-05) | R-io-05 |
| [R-map-03](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-map.md#r-map-03-mapto_list-を足す---不採用-r-map-07-で採用に変更) | Add `map.to_list` | × Rejected (→ adopted in R-map-07) | R-map-07 |
| [R-matrix-07](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-matrix.md#r-matrix-07-mul-を-matmul-に改名し要素ごとの積を足す---不採用-r-matrix-14-で--に置き換え) | Rename `mul` to `matmul` and add elementwise multiplication | × Rejected (→ replaced with `*` by R-matrix-14) | R-matrix-14 |
| [R-sized-05](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-sized-numbers.md#r-sized-05-固定幅のモジュールをまとめる算術関数を足す---不採用算術の部分は--r-sized-10-で見直し) | Combine fixed-width modules and add arithmetic functions | × Rejected (arithmetic reconsidered → R-sized-10) | R-sized-10 |
| [R-sized-07](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-sized-numbers.md#r-sized-07-float32float64-に-is_nan-などの判定を足す---不採用-r-sized-08-で見直し) | Add predicates such as `is_nan` to `float32` and `float64` | × Rejected (→ reconsidered by R-sized-08) | R-sized-08 |
| [R-value-06](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-value.md#r-value-06-構築子-str-を-string-に改名する---不採用-r-value-08-で採用に変更) | Rename constructor `str` to `string` | × Rejected (→ adopted by R-value-08) | R-value-08 |
| [R-L0-41](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md#r-l0-41-ビット演算を演算子にし-を関数の合成から外す---取り消し十二巡目記号の意味を入れ替えないため--r-l0-43) | Make bit operations operators and remove `>>` from function composition | × Withdrawn (twelfth review; do not swap symbols' meanings → R-L0-43) | R-L0-43 |

