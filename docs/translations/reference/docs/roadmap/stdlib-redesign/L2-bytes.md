> Description (fixed history): Stdlib redesign L2/L3 — bytes: in-place verbs (push/put_/append_), one byte-I/O family, raw pointers moved to ffi, and sequence names shared with list/string.
<a id="l2l3-bytes"></a>

# L2/L3: bytes

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](L2-bytes.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-bytes.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](L2-bytes.zh-CN.md)

> Non-normative historical redesign snapshot. Pinned original authoritative and unchanged; no automatic synchronization, fresh official retrieval or independent execution. (F), timing, measurements and implementation claims are original-author reports. Preserve early omitted i8 and u8 cells versus later additions/restoration, old _at tuple order versus next_ reversal, negative slice clamping versus later abort, and old L3 counts versus appended rows. RawPtr migration/import is an old proposal, not an enforced safety boundary or current FFI contract. Constant-time comparison expressly exposes length; source structural checks do not constitute independent timing verification or universal side-channel immunity. Japanese literal placeholders denote type/value/next position or state/width/same name. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-bytes.md)

> Last updated: 2026-10-24
> Prerequisites: [L0](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L0-conventions.md) / [L1](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L1-categories.md)
> Sources: Rust https://doc.rust-lang.org/std/vec/struct.Vec.html / https://doc.rust-lang.org/std/primitive.u32.html / Go https://pkg.go.dev/encoding/binary (F) / https://pkg.go.dev/bytes / Python https://docs.python.org/3/library/struct.html / https://docs.python.org/3/library/stdtypes.html#bytearray / TS/JS https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/DataView (F) / Gleam https://gleam-stdlib.hexdocs.pm/gleam/bit_array.html (F) / Elixir https://elixir.hexdocs.pm/Kernel.html#binary_part/3 / OCaml https://ocaml.org/manual/latest/api/Bytes.html (F) / Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/kotlin/-byte-array/ / Swift https://developer.apple.com/documentation/foundation/data
> Measurements: `almide 0.68.0 (dev, 437b8f127)`, both native and `--target wasm`; probes in scratchpad `probe-bytes/`.
> Counts: `/usr/bin/grep -rEo '\bbytes\.<fn>\b'` counted separately in spec/tests/docs/stdlib/research (upper bounds including definitions and generated indexes).

Reading: `mut` (“mute”) = argument modifying the caller's `var` in place. `??` (null coalescing) = use right side when left is none/err. `b[i] = v` (subscript assignment). LE/BE = little/big endian (byte order).

<a id="l2-役割の比較"></a>

## L2: Role comparison

| Role | Almide | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| Construct zero-filled | `new(n)` | `vec![0; n]` | `make([]byte, n)` | `bytes(n)` | `new Uint8Array(n)` | — | `:binary.copy(<<0>>, n)` | `Bytes.make` (F) | `ByteArray(n)` | `Data(count:)` |
| String ⇄ bytes | `from_string` / `to_string`（Result）/ `to_string_lossy` | `into_bytes` / `String::from_utf8` | `[]byte(s)` / `string(b)` | `encode` / `decode` | `TextEncoder` / `TextDecoder` | `from_string` / `to_string`→Result (F) | binary is a string | `of_string` / `to_string` (F) | `encodeToByteArray` / `decodeToString` | `Data(s.utf8)` / `String(decoding:)` |
| Read one byte (safe out of range) | `get`→Option | `get`→Option | — | — | — | — | — | — | `getOrNull` | — |
| Write one byte (in-place) | `b[i] = v`、`set_at`、`set_u8` | `b[i] = v` | `b[i] = v` | `b[i] = v` | `b[i] = v` | — | — | `set_uint8` (F) | `b[i] = v` | `b[i] = v` |
| Append one byte (in-place) | `push`、`append_u8`、`write_u8` | `push` | `append` | `bytearray.append` | — | — | — | `Buffer.add_uint8` | — | `append` |
| Append bytes (in-place) | **—**（`concat` returns a copy） | `extend_from_slice` | `append(b, s...)` | `extend` / `+=` | `set` | `append` (copy)(F) | `<>` | `Buffer.add_bytes` | `+=` | `append(contentsOf:)` |
| Read sized integers/floats | `read_<型>_<le/be>` (zero out of range)+ `read_uint16` family (two families) | `u32::from_le_bytes` | `binary.LittleEndian.Uint32` (F) | `struct.unpack_from` | `DataView.getUint32(o, le)` (F) | Bit syntax | `<<x::little-32>>` | `get_int32_le` (F) | — | `load(as:)` |
| Positional overwrite | `set_<型>_<le/be>` + `set_uint16` family | `copy_from_slice(&x.to_le_bytes())` | `PutUint32` (F) | `struct.pack_into` | `DataView.setUint32` (F) | — | — | `set_int32_le` (F) | — | — |
| Append at end | `append_<型>_<le/be>` + `write_*_be` + `write_uint16` family | `extend_from_slice` | `AppendUint32` (F) | `+= struct.pack` | — | Bit syntax | `<<b::binary, x::32>>` | `Buffer.add_int32_le` | — | — |
| Read while advancing position | `next_<型>`、`next_bytes`、`skip`、`is_eof` (currently `read_<型>_at`、`take_at`、`eof`） | `Cursor` | `bytes.Reader` + `binary.Read` | `io.BytesIO` | — | Pattern matching | Pattern matching | — | `ByteBuffer`(JVM) | — |
| Subsequence/concatenation/search | `slice`、`concat`、`index_of`、`contains`、`starts_with`、`ends_with`、`split`、`lines` | ● | `bytes.*` | ● | ● | `slice`、`starts_with` (F) | `binary_part`、`:binary.match` | `sub`、`cat` (F) | ● | ● |
| n elements from start/end | **—**（`slice` only） | `&b[..n]` | `b[:n]` | `b[:n]` | `subarray` | `slice` (F) | `binary_part` | `sub` | `take` / `takeLast` | `prefix` / `suffix` |
| Raw pointers | `as_ptr`、`as_mut_ptr`、`data_ptr`、`from_raw_ptr`、`copy_to_ptr` | `as_ptr` etc. | `unsafe` | `ctypes` | — | — | — | — | — | `withUnsafeBytes` |

<a id="完全性の規則"></a>

### Completeness rules

1. **Binary reads/writes form one family matrix.** Axes are type × byte order × operation.
   - Types: `u8`, `u16`, `i16`, `u32`, `i32`, `i64`, `f16`, `f32`, `f64`, `bool`.
   - Order: types at least two bytes wide have both `_le`/`_be`. `u8`/`bool` have no byte order.
   - Operations: `get_<型>` (positional read, none out of range), `next_<型>` (advances position, returns `(Option[値], 次の位置)`; R-bytes-13/R-L0-23), `get_<型>_array` (only types at least two bytes wide; none if any element out of range), `put_<型>` (positional overwrite, in place), `append_<型>` (append at end, in place).
   - **Intentional omissions**: positional `u8` read belongs to `get` (like `list.get`); `u8` `append` belongs to `push` (same “append one element” verb as `list.push`/`string.push`, R-bytes-02). No `u8`/`bool` `_array` (expressible as `bytes.slice(...) |> bytes.to_list`). No `i8`/`u64`. Read `i8` with `bytes.read_u8(b, p) |> int.to_int8`. `u64` does not fit `Int` (i64), so `i64` carries the same bits.
   - **Write cells with identical signed/unsigned bodies** (such as `append_i16_le`/`append_u16_le`) both remain for symmetry with read names. Prioritize inference from names. R-L0-01 does not count declared, gated type-matrix cells as synonyms (“Referrals back to L0 (resolved)” item 1).
2. **In-place verbs only from R-L0-04**: `push`, `clear`, `fill`, `append_<型>`, `put_<型>`, `copy_from`, `copy_within`. `set`, `insert`, `remove_at` are copy-side.
3. **Sequence names match list/string**: `compare` (all ordered types have `<mod>.compare(a, b) -> Int`), `slice`, `take`, `drop`, `take_end`, `drop_end`, `chunk`, `map`, `pad_start`, `pad_end`, `index_of`, `contains`, `starts_with`, `ends_with`, `split`, `lines`, `repeat`, `reverse`, `insert`, `remove_at`.
4. **Fallible reads return no sentinels (zero/empty string)** (R-L0-06 item 4). Option if no reason needed, otherwise `Result[_, String]`.
5. **No memory-unsafe functions** (R-L1-26).

<a id="l2-の判定"></a>

### L2 verdict

- Duplicates: `push` ≡ `append_u8` ≡ `write_u8`, `set_at` ≡ `set_u8` (measured: all three map 300 → `[44]`; `set_at`/`set_u8` share a body) → R-bytes-02. `write_u32_be` ≡ `append_u32_be` and three other pairs → R-bytes-03.
- Two families: suffix family (`read_u32_le`) versus `Endian`-argument family (`read_uint32(b, o, .le)`, 12 functions, only four widths) → R-bytes-04 (#3851 D).
- Verb style: `set_<型>` takes `mut`, while `bytes.set`/`list.set`/`map.set` copy → R-bytes-01.
- Holes: `f16` has only LE reads, no BE/writes; `bool` has no positional overwrite; no in-place byte-sequence append → R-bytes-03/R-bytes-05.
- Naming drift: `cmp` (ordered types use `compare`), `chunks` (list `chunk`), `map_each` (six other modules `map`), `pad_left`/`pad_right` (string `pad_start`/`pad_end`), missing `take`/`drop` family → R-bytes-08.
- Two meanings of `_at`: `read_u32_le_at` etc. advance position, but `read_string_at(b, pos, len)` only reads at a specified position → R-bytes-05.
- General L0 rules: `get_or` (R-L0-06), `eof` (R-L0-09) → R-bytes-07.
- Raw pointers → R-bytes-06 (#3853). Length-prefixed helpers → R-bytes-05 (#3852).
- Sentinels: positional `read_<型>`, `read_bool`, `read_<型>_array` return zero/false out of range (measured: `read_u8(h, 99)` → 0, `read_u32_le(h, 2)` with length 4 → 0), violating R-L0-06 item 4 → R-bytes-09.
- Out-of-range writes: `put_<型>` (current `set_<型>`), `set_at`, `set`, `remove_at` do nothing; `insert` clamps position to `0..len`. `b[i] = v` aborts (measured `Error: index out of bounds`). Violates L0 positional-write rule (abort out of range) → R-bytes-11.
- Boundary matrix (R-L0-12): `bytes.slice(b, -1, 2)` and `list.slice` return `[]`/`[]`, only `string.slice("abc", -1, 2)` returns `"ab"` (measured native/wasm). R-L0-12 clamps negative out-of-range indices to zero, so string is correct; align bytes/list → R-bytes-12 (list: L2-list R-list-10).

<a id="案"></a>

## Proposals

<a id="r-bytes-01-位置を指定した上書きを-put_型-にする---採用"></a>

### R-bytes-01 Name positional overwrites `put_<型>` — ○ Accepted

- Content: rename 15 functions `set_u8` and `set_<型>_<le/be>` (`u16`/`i16`/`u32`/`i32`/`i64`/`f32`/`f64` × LE/BE) to `put_u8`, `put_u16_le` … `put_f64_be`. Arguments unchanged. Separate R-bytes-11 makes out-of-range positions abort in the same window.
- Supporting L0 rules: R-L0-04 (`set` is a copy verb, in-place positional write uses `put_<型>`), R-L0-14.
- (a) Go uses `PutUint32` (F); OCaml/JS `set_int32_le`/`setUint32` (F). No majority, but internal `bytes.set` (copy) versus `bytes.set_u8` (in-place) share a verb, so rename under R-L0-00 condition 1. Matches Go's `Put`/`Append` pair.
- (b) Name only; bodies (native intrinsic/wasm self-host) unchanged.
- (c) Statement `bytes.set(b, i, v)` silently does nothing (measured: after `bytes.set(b, 0, 9)`, still `[1, 2, 3]`). Distinct verbs reveal `set` copy versus `put_` in-place.
- (d) Rename: `@deprecated(since=15, use="bytes.put_<型>")` (`set_u32_le` uses `use="bytes.put_u32_le"`), `removal_epoch = 16`. `almide fix`: one-to-one `bytes.set_u32_le(b, p, v)` → `bytes.put_u32_le(b, p, v)`. Total counts across 15 functions: spec 178 / tests 10 / docs 78 / stdlib 1.
- Gate: R-L0-G `mut`-verb check (public functions taking `mut` use allowed verbs). Matrix gate (R-bytes-03) checks every `put_` cell.
- Issue: #3898.

<a id="r-bytes-02-1-バイトの追記は-push1-バイトの上書きは-put_u8-だけにする---採用"></a>

### R-bytes-02 Single-byte append only `push`, overwrite only `put_u8` — ○ Accepted

- Content: remove `append_u8`/`write_u8` in favor of `push`; remove `set_at` in favor of `put_u8`.
- Supporting L0 rules: R-L0-01 (one spelling per role, across modules too), R-L0-04, R-L0-14. Settle #3851 C here.
- (a) Append-one verbs differ: Rust `push`, Swift/Python `append`/`append`. Almide already has `list.push(mut xs, x)` and `string.push(mut s, x)` as `push`; retain `push` under same-role/same-spelling R-L0-01.
- (b) All three share `b.push(val as u8)`; no impact on either target.
- (c) Replace `set_at` with `put_u8`; `put_u8` is already retained as the type matrix's `u8` overwrite cell (R-bytes-03). R-bytes-11 aborts out of range, so `put_u8` and subscript assignment `b[i] = v` behave identically. Either spelling works, but only `put_u8` lets `almide fix` retain call shape for one-to-one rewriting.
- (d) Remove: `@deprecated(since=15, use="bytes.push")` (`append_u8`/`write_u8`), `use="bytes.put_u8"` (`set_at`), `removal_epoch = 16`. `almide fix`: `bytes.append_u8(b, v)` → `bytes.push(b, v)`; `bytes.set_at(b, i, v)` → `bytes.put_u8(b, i, v)`. Counts: `append_u8` 69/5/14/0, `write_u8` 8/1/3/0, `set_at` 44/8/5/0 (spec/tests/docs/stdlib). `push` remains with spec 45/stdlib 40.
- Gate: #3851 F synonym gate rule 6 (distinct same-module intrinsics with identical bodies). Matrix gate declares “`u8` append cell = `push`”.
- Issue: #3851 C (existing).

<a id="r-bytes-03-書き足しを-append_-に揃え型--向き--操作の行列を閉じる---採用"></a>

### R-bytes-03 Standardize append as `append_`, close type × order × operation matrix — ○ Accepted

- Content:
  - Remove `write_u32_be`/`write_i64_be`/`write_f64_be` in favor of `append_u32_be`/`append_i64_be`/`append_f64_be`.
  - Rename `write_bool` to `append_bool`, add `put_bool(mut b, pos, val: Bool)`.
  - Fill `f16` holes: `get_f16_be`, `read_f16_be_at`, `get_f16_be_array`, `put_f16_le`, `put_f16_be`, `append_f16_le`, `append_f16_be`.
- Supporting L0 rules: R-L0-01, R-L0-04 (append verb `append_<型>`), CLAUDE.md “expand API families as matrices”.
- (a) JS `getFloat16`/`setFloat16` (F) and Python `struct`'s `e` have both byte orders for `f16` reading/writing. Current LE-only reads force BE file-format writers to implement their own.
- (b) Pure computation. One self-host implementation rounds `f16` (`Float` → half precision), yielding identical bits on both targets.
- (c) LLMs knowing `get_f16_le` infer `put_f16_le` by rule; matrix holes make that inference wrong.
- (d) Remove 3/rename 1/add 8. `@deprecated(since=15, use="bytes.append_u32_be")`, etc. (`write_bool` uses `use="bytes.append_bool"`), `removal_epoch = 16`. `almide fix`: one-to-one `bytes.write_u32_be(b, v)` → `bytes.append_u32_be(b, v)`. Counts (four `write_*` functions): spec 16 / tests 8 / docs 16 / stdlib 0.
- Gate: `tests/stdlib_bytes_matrix_test.rs`. Generate every cell name/signature from declared types/orders/operations and require exact interface JSON equality (both extra/missing fail). One fixture compares every cell's round-trip (`append` → `read`, `put` → `read`) across native/wasm.
- Issue: #3851 C (existing `write_*` portion) + new (matrix holes).

<a id="r-bytes-04-endian-引数の系統12-関数を撤去し接尾辞の系統を唯一にする---採用"></a>

### R-bytes-04 Remove the `Endian`-argument family (12 functions), keep only suffixes — ○ Accepted

- Content: remove `read_uint16`, `read_uint32`, `read_int32`, `read_float32`, `write_uint16`, `write_uint32`, `write_int32`, `write_float32`, `set_uint16`, `set_uint32`, `set_int32`, `set_float32` and `type Endian`. Delete “the new preferred surface” comment at `stdlib/bytes.almd:552-569`. This is #3851 D option (a).
- Supporting L0 rules: R-L0-01 (two families for one operation), R-L0-04 (`write_`/`set_` are not allowed verbs; option (b) would add them or rename the whole matrix), R-L0-00 (minimal renaming).
- (a) Languages split between order in names (OCaml `get_int32_le` (F), Rust `from_le_bytes`) and values (Go `binary.LittleEndian.Uint32` (F), JS `getUint32(o, true)` (F)). No majority: keep the more documented/used suffix family.
- (b) Both lower to the same bodies; no impact.
- (c) Same read currently has two spellings and two return types (`Int`/`UInt32`), with source comments/docs disagreeing on the preferred form. L0 verbs `append_`/`put_` apply only to suffixes.
- (d) Remove. Hints such as `@deprecated(since=15, use="bytes.get_u32_le")` name per-order suffix forms (`read_uint32(b, o, LittleEndian)` → `bytes.get_u32_le(b, o) |> option.map(int.to_uint32)`). `removal_epoch = 16`. `almide fix` rewrites literal `Endian` calls; runtime `Endian` values cannot be mechanically rewritten, so hint only: choose `bytes.get_u32_le` / `bytes.get_u32_be` by order. Counts (12 functions): spec 103 / tests 15 / docs 46 / stdlib 1.
- Gate: R-bytes-03 matrix gate rejects binary read/write functions outside the matrix.
- Issue: #3851 D (existing).

<a id="r-bytes-05-長さ前置きの補助関数と-read_string_at-を撤去しappend_bytes-を足す---採用"></a>

### R-bytes-05 Remove length-prefixed helpers and `read_string_at`, add `append_bytes` — ○ Accepted

- Content:
  - Remove `read_string_be`, `read_string_be_at`, `write_string_be` (u32 BE prefix), `read_length_prefixed_strings_le`, `skip_length_prefixed_le` (u32 LE prefix), `read_string_at(b, pos, len)`.
  - Add `append_bytes(mut b: Bytes, src: Bytes) -> Unit` (append bytes in place).
- Supporting L0 rules: R-L0-01, R-L0-04 (include `bytes` as type in `append_<型>`), R-L0-14. Settle #3852's bytes section here.
- (a) Standard length-prefixed strings occur mainly in Java `DataOutput.writeUTF` (u16 BE); prefix width/order depend on format. Almide currently differs between single u32 BE and batch u32 LE. Eight languages provide in-place byte append (Rust `extend_from_slice`, Go `append(b, s...)`, Python `extend`, etc.).
- (b) Pure computation.
- (c) `read_string_at`'s `_at` differs from other `_at` forms (advance position and return `(位置, Option)`); names cannot predict shape, prefix width or order. Replace using combinations:
  - Read: `let n = bytes.get_u32_be(b, p)?`, then `bytes.slice(b, p + 4, p + 4 + n) |> bytes.to_string`.
  - Write: `bytes.append_u32_be(b, bytes.len(sb))`, then `bytes.append_bytes(b, sb)`.
  - `read_string_at(b, p, n)` → `bytes.slice(b, p, p + n) |> bytes.to_string_lossy`. Partially out-of-range results differ (old empty string, new in-range portion); state this in hints.
- (d) Remove 6/add 1. `@deprecated(since=15, use="…")`: `read_string_at` uses `use="bytes.slice(b, p, p + n) |> bytes.to_string_lossy"`; five prefix helpers name combinations such as `use="bytes.get_u32_be + bytes.slice"`; `removal_epoch = 16`. Changed shape means no `almide fix` rewrite; show (c) patterns in hints. Counts (six functions): spec 35 / tests 14 / docs 33 / stdlib 5. Replace the five stdlib uses in the same removal change.
- Gate: matrix gate (R-bytes-03). Add R-L0-G type check requiring public names ending `_at` to return `(Int, Option[T])`.
- Issue: #3852 (existing).

<a id="r-bytes-06-生ポインタを-ffi-に移す---採用"></a>

### R-bytes-06 Move raw pointers to `ffi` — ○ Accepted

- Content: follow #3853: `as_ptr`/`as_mut_ptr`/`data_ptr` → `ffi.address(b) -> RawPtr` (null if empty), `from_raw_ptr` → `ffi.read(ptr, len) -> Bytes`, `copy_to_ptr` → `ffi.write(b, ptr, cap) -> Int`. Do not auto-import `ffi`.
- Supporting L0 rules: R-L1-26, R-L0-01 (three spellings for data address).
- (a) Rust also has `as_ptr`, but unsafe operations require `unsafe`. Almide lacks `unsafe`, so separate modules and require import.
- (b) Empty Bytes currently differ: `as_ptr` null, `data_ptr` handle + 12. `ffi.address` returns null on both targets. Before removal, verify wasm-to-JS-host address passage (#907, `spec/wasm_host_js/extern_data_ptr_lifetime.almd`) works through `ffi.address`.
- (d) Remove (add on `ffi` side). `@deprecated(since=15, use="ffi.address")` (`as_ptr`/`as_mut_ptr`/`data_ptr`), `use="ffi.read"` (`from_raw_ptr`), `use="ffi.write"` (`copy_to_ptr`), `removal_epoch = 16`. `almide fix` rewrites one-to-one and adds `import ffi`. `data_ptr` return changes `Int` → `RawPtr`; hint at resulting type errors. Counts (five functions): spec 48 / tests 1 / docs 36 / stdlib 2.
- Gate: no `RawPtr` in public `bytes` function types.
- Issue: #3853 (existing).

<a id="r-bytes-07-get_or-を撤去しeof-を-is_eof-に改名する---採用"></a>

### R-bytes-07 Remove `get_or`, rename `eof` to `is_eof` — ○ Accepted

- Content: `get_or(b, i, d)` → `bytes.get(b, i) ?? d`; `eof(b, pos)` → `is_eof(b, pos)` (same behavior: true for `pos >= len` or negative).
- Supporting L0 rules: R-L0-06 (default `_or` forms unified as `??`), R-L0-09 (Bool names `is_`; L0 explicitly names exception `bytes.eof`).
- (b) `get_or` occurs 33 times in stdlib lexer. Before removal, use `research/benchmark/perf/` lexer to verify no slowdown from `bytes.get(b, i) ?? -1` (native `Option<i64>` is unboxed, so no difference expected).
- (d) Remove 1/rename 1. `@deprecated(since=15, use="??")` (`get_or`), `@deprecated(since=15, use="bytes.is_eof")` (`eof`), `removal_epoch = 16`. `almide fix`: `bytes.get_or(b, i, d)` → `bytes.get(b, i) ?? d`; `bytes.eof(b, p)` → `bytes.is_eof(b, p)`. Counts: `get_or` 69/7/11/33, `eof` 9/1/6/1 (spec/tests/docs/stdlib).
- Gate: R-L0-G rules R-L0-06/R-L0-09.
- Issue: #3913.

<a id="r-bytes-08-列の関数を-liststring-と同じ綴りにする---採用"></a>

### R-bytes-08 Align sequence names with list/string — ○ Accepted

- Content:
  - Rename `cmp` → `compare` (L0 revision: ordered types all use `<mod>.compare(a, b) -> Int`), `chunks` → `chunk`, `map_each` → `map`, `pad_left` → `pad_start`, `pad_right` → `pad_end`.
  - Add copy-returning `take(b, n)`, `drop(b, n)`, `take_end(b, n)`, `drop_end(b, n)`, matching list/string boundary answers (`n` exceeding length gives all; negative `n` gives all for `take`, empty for `drop`; L2-string completeness rule 1).
  - `chunk` aborts for size ≤0 like `list.chunk` (measured `list.chunk(xs, 0)` → `Error: chunk size must be positive`, `bytes.chunks(b, 0)` → `[]`). New name avoids R-L0-14's same-name semantic-change prohibition.
- Supporting L0 rules: R-L0-01 (same role/same spelling across modules), R-L0-12 (bytes included among types with `take`/`drop`/`take_end`/`drop_end`), R-L0-06 item 3 (only contract violations abort; zero-size chunking is one).
- (a) Prefix/suffix sequence cuts include Kotlin `take`/`takeLast`, Swift `prefix`/`suffix`; every language shares names between bytes and lists.
- (b) Pure computation.
- (c) LLMs knowing `list.map` write `bytes.map`, currently undefined.
- (d) Rename 5/add 4. `@deprecated(since=15, use="bytes.compare")` (`cmp`), `use="bytes.chunk"` (`chunks`), `use="bytes.map"` (`map_each`), `use="bytes.pad_start"`/`use="bytes.pad_end"`, `removal_epoch = 16`. `almide fix` one-to-one. Hints state `chunks(b, 0)` returned `[]`, while `chunk(b, 0)` aborts. Counts: four renames excluding `cmp`: spec 30 / tests 0 / docs 17 / stdlib 4; `cmp` 20/1/6/1.
- Gate: add five bytes functions to R-L0-12 boundary matrix tests comparing same inputs across list/string/bytes. Extending cross-module synonym gate (#3851 F rule 5) to differing spellings for one role is L0 scope, so only matrix tests here.
- Issue: #3899.

<a id="r-bytes-09-位置指定の読みを番兵-0-を返す-read_型-から-option-を返す-get_型-に移す---採用"></a>

### R-bytes-09 Move positional reads from sentinel-zero `read_<型>` to Option-returning `get_<型>` — ○ Accepted

- Content:
  - Rename `read_<型>_<le/be>` (14) and `read_f16_le` to `get_<型>_<le/be>(b, pos) -> Option[T]`, none out of range.
  - `read_bool(b, pos)` → `get_bool(b, pos) -> Option[Bool]`.
  - `read_<型>_array` (15) → `get_<型>_array(b, pos, count) -> Option[List[T]]`, none if any element out of range.
  - Remove `read_u8(b, pos)` in favor of `bytes.get(b, pos)` (same role, already Option).
  - Retain position-advancing `read_<型>_at`, already Option.
- Supporting L0 rule: R-L0-06 item 4 (fallible construction/decoding returns no sentinel; Option if no reason needed). R-L0-14 item 4 permits changing same-name return `Int` to `Option[Int]`; still rename because the verb mismatches. `read_` belongs to advancing `read_<型>_at`, while positional Option extraction is `get`. `get_` matches R-L0-06 “unsuffixed extraction is Option” (`list.get`, `bytes.get`).
- (a) No language returns zero out of range. Go/JS/Python/Rust/Kotlin throw/panic (JS DataView RangeError (F)). Names match OCaml `get_int32_le`/`set_int32_le` (F).
- (b) Pure computation. One self-host boundary-check implementation gives identical answers on both targets.
- (c) Forgetting length checks currently silently reads corrupt files as zero. Option requires `?` or `??` to extract values.
- (d) Rename 31/remove 1. `@deprecated(since=15, use="bytes.get_u32_le")`, etc. (`read_u8` uses `use="bytes.get"`), `removal_epoch = 16`. `almide fix` changes shape: `bytes.read_u32_le(b, p)` → `bytes.get_u32_le(b, p) ?? 0` (preserve current answer); hint recommends propagating failure with `?`. Counts for 31 excluding `read_u8`: spec 194 / tests 3 / docs 159 / stdlib 59 (`read_u8`: 98/6/6/2). Rewrite stdlib and `matrix.from_bytes_*` internals in the same change.
- Gate: matrix gate (R-bytes-03) checks `get_*` returns Option. In R-L0-G list public functions returning `Int`/`Float`/`Bool`/`String` whose names start `read_`/`get_`/`decode`/`parse`; test boundary inputs for no sentinels.
- Issue: #3897.

<a id="r-bytes-10-utf-8-の両方向は-bytes-が持つ---採用据え置き"></a>

### R-bytes-10 bytes owns both UTF-8 directions — ○ Accepted (keep)

- Content: retain `bytes.from_string`, `bytes.to_string` (Result), `bytes.to_string_lossy` as sole string/bytes conversion spellings. L2-string removes `string.to_bytes`/`string.from_bytes` (`List[Int]`) per L0 revision.
- Supporting L0 rules: R-L0-07 item 4 (paired directions align types), R-L0-01, R-L0-06 item 4 (`to_string` returns reason-bearing Result; `to_string_lossy` always succeeds with U+FFFD replacement).
- (a) Languages split between decoding on source (Gleam `bit_array.to_string`→Result (F), Python `decode`, Kotlin `decodeToString`, OCaml `Bytes.to_string` (F)) and destination (Rust `String::from_utf8`, Swift `String(decoding:)`). Keep Almide's both-directions-on-encoded-side pattern (like base64/hex).
- (b) `bytes.from_string` uses native `Vec<u8>`, lighter than `string.to_bytes`'s `Vec<i64>` (8× size, #2078).
- (d) No bytes-side change.
- Issue: #2078 (related).

<a id="r-bytes-11-範囲外の位置への書き込みを-abort-にする---採用"></a>

### R-bytes-11 Abort writes to out-of-range positions — ○ Accepted

- Content: positional writes abort on out-of-range positions as caller contract violations.
  - In-place overwrite `put_<型>(mut b, pos, v)` (current `set_<型>`/`set_u8`/`set_at`) currently does nothing if width overruns. Abort for `pos < 0` or `pos + 幅 > len`.
  - Copy positional writes `bytes.set(b, i, v)`/`bytes.remove_at(b, i)` currently return unchanged out of range. Abort outside `0 <= i < len`.
  - `bytes.insert(b, pos, v)` currently clamps to `0..len`; accept `0..=len` like `list.insert`, otherwise abort.
- Supporting L0 rules: “positional writes” (after R-L0-12, before R-L0-13), R-L0-06 item 3.
- (a) Go `PutUint32` panics; JS `DataView.setUint32` RangeError (F); OCaml `set_int32_le` throws (F); Rust `copy_from_slice` panics. No language silently discards out-of-range writes.
- (b) Same abort path on both targets, already used by subscript assignment `b[i] = v`; one self-host boundary implementation.
- (c) Currently `put_` and `b[i] = v` differ out of range, silently losing writes, so corrupt output files go unnoticed.
- (d) Semantic change (domain narrowing). Valid writes unchanged, so not dialect-epoch `breaks` (existing policy that narrowing does not trigger an epoch). Names change in R-bytes-01's window, but this proposal itself needs no `@deprecated`. Calls relying on out-of-range no-ops can write `if pos + 4 <= bytes.len(b) then bytes.put_u32_le(b, pos, v) else ()`. Count affected out-of-range calls in spec/tests/stdlib and record in PR.
- Gate: for every matrix `put_` cell (R-bytes-03) and `set`/`insert`/`remove_at`, add native/wasm boundary fixtures {-1, 0, len-width, len-width+1, len}, verifying identical abort positions (same table as L2-list R-list-13). Observable behavior changes require contracts ledger in als first.
- Issue: #3937.

<a id="r-bytes-12-slice-の負の開始位置を-0-に丸める---採用-r-l0-29-で-abort-に変更"></a>

### R-bytes-12 Clamp negative `slice` starts to zero — ○ Accepted (→ changed to abort by R-L0-29)

- Sixth-round note: R-L0-29 changed negative-start clamping to abort. Positive overruns still clamp to length as proposed here.
- Content: clamp negative `start` in `bytes.slice(b, start, end)` to zero. Currently `bytes.slice(b, -1, 2)` returns length zero; make it return the first two bytes like `string.slice("abc", -1, 2)`.
- Supporting L0 rule: R-L0-12 (out-of-range indices clamp to endpoints; negative indices are out of range).
- (a) Python/JS count negative indices from the end; R-L0-12 does not adopt this. Clamping matches Gleam/Elixir `slice`.
- (b) Pure computation; same behavior on both targets.
- (c) Different answers for identical arguments across list/string/bytes defeat boundary behavior learned on another type.
- (d) Semantic change only at boundary inputs, aligning with documented “capped”. No name/type change, so no `@deprecated`. Count negative-start calls in spec/tests and record in PR.
- Gate: R-L0-12 boundary matrix (list/string/bytes × `slice`/`take`/`drop`/`take_end`/`drop_end` × {-1, 0, len, len+1}).
- Issue: #3944.

<a id="r-bytes-13-読み位置を進める族を-next_型-にし値-次の位置-を返す---採用"></a>

### R-bytes-13 Advance position with `next_<型>`, return `(値, 次の位置)` — ○ Accepted

- Current: `read_<型>_at(b, pos) -> (Int, Option[T])` (18) and `take_at(b, pos, n) -> (Int, Option[Bytes])`; next position first, value second.
- Proposal: `next_<型>(b, pos) -> (Option[T], Int)`, `next_bytes(b, pos, n) -> (Option[Bytes], Int)`. Keep names/shapes of `skip(b, pos, n) -> Int` and `is_eof(b, pos) -> Bool` (state only, no value, outside R-L0-23).
- Supporting L0 rule: R-L0-23 (state-advancing functions use `next_`, `(値, 次の状態)`), same shape as `random.next_int(rng, …) -> (Int, Rng)`.
- Failed reads do not advance position (same as current `read_u32_le_at` returning `(0, none)` for short input); retain after renaming. `skip(b, pos, n)` returns negative positions for `n < 0` (measured `skip(a, 1, -2)` → `-1`), so abort for `n < 0` as contract violation (R-L0-12/R-L0-06 item 3).
- (a) Popular Rust bytes library's `get_i32_le` advances position, opposite Almide positional-only `get_<型>` (R-bytes-09). Using `next_` for advancing leaves one `get_` meaning.
- (c) `let (v, pos) = bytes.next_u32_le(b, pos)` matches random's `let (n, rng) = random.next_int(rng, 0, 9)`.
- (d) Rename/semantic change (tuple order). Old forms type-error, satisfying R-L0-14.4. Mark old names `@deprecated` epoch 15, remove epoch 16. `almide fix` renames; hints guide destructuring order.
- Completeness: four columns `get_<型>` (positional read), `next_<型>` (advance), `put_<型>` (positional write), `append_<型>` (append) have all identical type rows (`u16`–`i64`, `f16`–`f64` le/be, `bool`). Only `u8` uses `bytes.get` in `get` column and `bytes.push` in `append` column (same-role functions already exist, R-bytes-02/R-bytes-09). `get_<型>_array` only for types at least two bytes.
- Gate: check four columns × type rows from interface JSON (extend R-bytes-03 matrix gate).
- Issue: #3962.

<a id="r-bytes-14-bytesheap_savebytesheap_restore-を公開の面から外す---採用"></a>

### R-bytes-14 Remove `bytes.heap_save`/`bytes.heap_restore` from public surface — ○ Accepted

- Current: `stdlib/bytes.almd` publicly defines `fn heap_save() -> Int` and `fn heap_restore(checkpoint: Int) -> Unit`, omitted even from this record's L3 table (recording omission).
- Proposal: internal names `__heap_save`/`__heap_restore`, used only by runtime fixtures.
- Supporting L0 rules: R-L1-26 (no memory-unsafe standard functions), R-L0-02 (subject is not Bytes), R-L0-15 (target-dependent result: native zero, wasm heap address).
- (a) No language exposes heap rewind as a standard public API.
- (b) Using values after wasm rewind breaks them.
- (c) Auto-imported `bytes` exposes these in completion; calling them to “free memory” breaks only wasm.
- (d) Remove 2 (keep internal). `@deprecated` epoch 15, remove public access epoch 16. Uses: spec four files (all runtime tests), stdlib 2.
- Gate: R-bytes-03 matrix gate rejects public bytes functions absent from matrix/sequence tables, also catching this omission.
- Issue: #3970.

<a id="r-bytes-15-i8-の行を読み書きの行列に足す---採用"></a>

### R-bytes-15 Add `i8` row to read/write matrix — ○ Accepted

- Current: types u8/u16/i16/u32/i32/i64/f16/f32/f64/bool; i8 intentionally omitted because `bytes.read_u8(b, p) |> int.to_int8` can read it. But R-bytes-09 removes `read_u8`, and `int.to_int8` returns `Int8` (other cells return `Int`).
- Proposal: add `get_i8(b, pos) -> Int?`, `next_i8(b, pos) -> (Int?, Int)`, `put_i8(mut b, pos, val: Int)`, `append_i8(mut b, val: Int)`.
- Supporting L0 rules: R-L0-01 matrix exception (retain signed/unsigned cells even with same bodies), CLAUDE.md “families as matrices”.
- (a) Signed 8-bit readers: OCaml `Bytes.get_int8` (F), JS `DataView.getInt8`, Python `struct`'s `b`, Java `readByte`.
- (c) Having `get_i16_le` without `get_i8` defeats inferred names; handwritten sign conversion `if x >= 128 then x - 256 else x` is error-prone.
- (d) Add 4.
- Completeness: type axis {u8, i8, u16, i16, u32, i32, i64, f16, f32, f64, bool}. Only u64 omitted: does not fit Int, `get_i64_*` carries the same bits. This name is covered by R-L0-19 guidance.
- Issue: #3970.

<a id="r-bytes-16-bytes-も--で連結しbytesconcat-を撤去する---採用"></a>

### R-bytes-16 Concatenate `Bytes` with `+`, remove `bytes.concat` — ○ Accepted

- Current: `fn bytes.concat(a: Bytes, b: Bytes) -> Bytes`; `a + b` rejected with “operator '+' requires numeric, String, or List types”. List/string concatenate with `+`, no `concat` function (R-list-12).
- Proposal: checker and both targets' lowering accept `Bytes + Bytes -> Bytes`, remove `bytes.concat`. Expand CHEATSHEET's “`+` concatenates strings/lists” to sequences (String/List/Bytes).
- Supporting L0 rules: R-L0-00 item 1 (only Bytes among three sequence types spells concatenation differently), R-L0-01.
- (a) Python `b1 + b2`, Kotlin `ByteArray.plus`, Swift `Data + Data`, Elixir `<>`; function forms Go `append`, OCaml `Bytes.cat`, Gleam `bit_array.append`.
- (b) Native concatenates `Vec<u8>`; wasm just calls self-host `bytes_concat`.
- (c) Measured `a + b` type-errors. LLMs learning `+` for list/string write it for Bytes too.
- (d) Language change (only wider acceptance) and removal 1. `@deprecated(since=15, use="+")`, remove epoch 16. `almide fix`: `bytes.concat(a, b)` → `a + b`. `html.concat` uses `SafeHtml`, outside scope.
- Gate: `+` type matrix (String/List/Bytes × both targets) fixtures; contract in als first.
- Issue: #3970.

<a id="r-bytes-17-byteslines-を撤去するstringlines-と-r-の扱いが違う---採用"></a>

### R-bytes-17 Remove `bytes.lines` (different `\r` behavior from `string.lines`) — ○ Accepted

- Current: first line of `bytes.lines(bytes.from_string("a\r\nb\n"))` is `[97, 13]` (retains `\r`); first line of `string.lines("a\r\nb\n")` is `"a"` (removes `\r`). Same verb, different behavior.
- Proposal: remove `bytes.lines`. Text lines use `string.lines(bytes.to_string(b)!)`; byte-preserving splitting on `\n` uses `bytes.split(b, bytes.from_string("\n"))`.
- Supporting L0 rules: R-L0-00 item 1 (same verb differs by module), R-L0-14.4 (cannot change behavior under the same name). No rename because no majority spelling.
- (a) Byte-line splitting strips `\r` in Go `bufio.ScanLines`, Python `bytes.splitlines`, Rust `BufRead::lines`; no standard retaining function found.
- (c) Splitting Windows-origin files from `fs.read_bytes` silently retains trailing `\r`, breaking comparisons.
- (d) Remove: `@deprecated` epoch 15, removal epoch 16. Changed shape means no `almide fix`; hint both alternatives. Counts spec 9 / docs 1.
- Issue: #3970.

<a id="r-bytes-18-bytes-に高階関数の族を置くfilterfoldanyallcountfind---採用"></a>

### R-bytes-18 Give bytes the higher-order family (`filter`/`fold`/`any`/`all`/`count`/`find`) — ○ Accepted

- Current: only higher-order bytes function is `map`.
- Proposal: same D3 family as list/deque. `fn bytes.filter(b: Bytes, f: (Int) -> Bool) -> Bytes`, `fold(b, init: B, f: (B, Int) -> B) -> B`, `any`/`all` (`(Int) -> Bool`), `count(b, f) -> Int`, `find(b, f) -> Int?`. `contains` already exists (subsequence meaning; differs from list `contains` according to subject type under R-L0-01).
- Supporting L0 rule: R-map-06 (D3 family in all ordered collections). Bytes with only `map` was incomplete.
- (a) Rust `[u8]`, Kotlin `ByteArray`, Swift `[UInt8]`/`Data`, Python `bytes` use the whole family as sequences.
- (b) Pure computation. Native `bytes.to_list |> list.fold` builds an 8×-size `List[Int]`; direct `fold` speeds checksum-like work.
- (c) LLMs seeing `bytes.map` also write `bytes.fold`/`bytes.filter`; currently E002.
- (d) Add 6.
- Completeness: ordered collections list/deque/bytes and map/set all have D3's seven functions. String has no character value type (`chars` returns `List[String]`), so excluded.
- Gate: add bytes column to R-map-06 matrix. Property test: every cell equals `bytes.to_list(b) |> list.<同名>(f)`.
- Issue: #3970.

<a id="r-bytes-19-_array-の読みだけがある理由を規則に書く---採用文書だけ"></a>

### R-bytes-19 Document why only `_array` reads exist — ○ Accepted (docs only)

- Proposal: add to intentional omissions: “no `_array` for writes (`put_`/`append_`) or advancing reads (`next_`); use `for` one element at a time”, and declare this in matrix gate.
- (a) Batch array writes exist in Python `struct.pack`, Go `binary.Write`, not a majority.
- (c) Declared omissions let R-L0-19 guidance redirect an LLM writing `append_f32_le_array` correctly.
- Issue: #3970.

<a id="r-bytes-20-firstlastpop-を足す---採用"></a>

### R-bytes-20 Add `first`/`last`/`pop` — ○ Accepted

- Current: bytes has `get`/`push` but lacks `first`/`last`/`pop`. Analogy with list (`first`/`last`/`pop`) and deque (`first`/`last`/`pop`) produces E002.
- Proposal: `fn bytes.first(b: Bytes) -> Int?`, `fn bytes.last(b: Bytes) -> Int?`, `fn bytes.pop(mut b: Bytes) -> Int?`.
- Supporting L0 rules: R-L0-12 (sequence types share names), R-L0-06 (fallible unsuffixed extraction returns Option).
- (a) Rust `Vec<u8>`'s `first`/`last`/`pop`, Python `bytearray.pop`, Kotlin `ByteArray.first`/`lastOrNull`, Swift `Data.first`/`last`/`popLast`.
- (b) Pure computation; `pop` mutates in place like `push`.
- (d) Add 3.
- Completeness: sequence types with `push` (list/bytes) also have `pop`/`first`/`last`.
- Issue: #3980.

<a id="r-bytes-21-bytesskip-を-next_-の形にする---不採用"></a>

### R-bytes-21 Make `bytes.skip` use `next_` form — × Rejected

- Reason: `skip` advances position without reading a value, so there is no value for `(値, 次の状態)`. Returning `(Unit, Int)` only adds a discarded value. R-L0-27 now says position movement without reading returns only next state.
- Issue: none.

<a id="r-bytes-22-byteschunk-は-size--0-で-abort---採用"></a>

### R-bytes-22 `bytes.chunk` aborts for `size <= 0` — ○ Accepted

- Current: R-list-10 makes `list.chunk` abort for `n <= 0`; `bytes.chunk` has no rule.
- Proposal: abort like `list.chunk`.
- Supporting L0 rule: R-L0-12 (same boundary matrix for list/string/bytes).
- (d) Semantic change (abort rejected inputs).
- Issue: #3985.

<a id="r-bytes-23-bytesxor-は長さが違うと-abort---採用"></a>

### R-bytes-23 `bytes.xor` aborts for unequal lengths — ○ Accepted

- Current: truncates to shorter length (measured `almide 0.68.0 dev 437b8f127`, same native/wasm: `[1, 2, 3]` and `[1]` yield `[0]`), returning short results without stopping.
- Proposal: abort unequal lengths, like matrix shape mismatch; callers can check `bytes.len` beforehand.
- Supporting L0 rule: R-L0-06 item 3 (violations of caller-checkable preconditions abort).
- (d) Semantic change: currently truncating calls abort, so include in epoch 15 `breaks`.
- Issue: #3985.

<a id="r-bytes-24-bytesset-と-put_u8-を-1-つにする---不採用"></a>

### R-bytes-24 Unify `bytes.set` and `put_u8` — × Rejected

- Reason: `set(b, i, v) -> Bytes` returns a copy; `put_u8(mut b, pos, v)` mutates. They are R-L0-04's pair (copy default; fixed verbs for mutation). `list.set` also copies.
- Issue: none.

<a id="r-bytes-25-型つきの読み書きの行列に-get_u8append_u8-を戻す---採用r-bytes-02-の一部を見直し"></a>

### R-bytes-25 Restore `get_u8`/`append_u8` to typed read/write matrix — ○ Accepted (revisits part of R-bytes-02)

- Current: typed read/write family {`get_`, `put_`, `next_`, `append_`} × {type}, but only `u8` lacks `get_u8`/`append_u8` (`get_i8`/`put_u8`/`next_u8` exist), because R-bytes-02 consolidated `append_u8` into `push`, R-bytes-09 `get_u8` into `get`. Three external reviews successively flagged this hole.
- Proposal: add `fn bytes.get_u8(b: Bytes, pos: Int) -> Int?`, retain `append_u8` without removal. Keep `bytes.get`/`bytes.push` as container read/write (same forms as `list.get`/`list.push`).
  - Identical computation has two names, but each is a cell in a different family (container versus typed read/write). Apply CLAUDE.md “families as matrices” before R-L0-01 no-synonyms.
- (a) Go `binary.LittleEndian`, JS `DataView`'s `getUint8`, OCaml `Bytes.get_uint8`.
- (d) Add 1/cancel removal 1.
- Completeness: all four typed read/write verbs cover all type columns including `u8`.
- Issue: #3991.

<a id="r-bytes-26-bytesget_u8-と-getappend_u8-と-push-を-1-つにする---不採用役割の違いを明記"></a>

### R-bytes-26 Unify `bytes.get_u8`/`get` and `append_u8`/`push` — × Rejected (state distinct roles)

- Reason: same results, different families. `get`/`push` belong to containers (same as `list.get`/`deque.push`); `get_u8`/`append_u8` to encoding (one-byte cells alongside `get_u16_le`/`append_u16_le` …). Omitting only `u8` leaves a `get_<型>` matrix hole and invalidates inferred `get_u8` (R-bytes-25).
- Promise: identical by definition (`get_u8(b, i) == get(b, i)`; `append_u8`/`push` append identically). Matrix gate also checks this equality.
- Issue: none.

<a id="r-bytes-27-bytes-に-text-の要件をそろえる---採用"></a>

### R-bytes-27 Complete `Text` requirements for `bytes` — ○ Accepted

- Content: add `last_index_of`, `count_matches`, `replace`, `replace_first`, `strip_prefix`, `strip_suffix`. Same shape/meaning as counterparts in `string`, positions in bytes.
- Reason: in protocol hierarchy, `bytes` is the same `Text` as `string` (search/replace subsequences). Supported by Go `bytes` (`LastIndex`/`ReplaceAll`/`CutPrefix`), Python `bytes` (`rfind`/`replace`/`removeprefix`), Rust slices (`strip_prefix`), Swift `Collection` (`trimmingPrefix`/`replacing`).
- (d) Add 6.
- Issue: #4012.

<a id="r-bytes-28-bytesnewlen-を-byteszerosn-に改名しbytesnew-を空のバイト列にする---採用"></a>

### R-bytes-28 Rename `bytes.new(len)` to `bytes.zeros(n)`, make `bytes.new()` empty — ○ Accepted

- Current: container `new()` constructs empty (`list`/`string`/`deque`/`set`/`heap`/`map`). Only `bytes.new(len)` constructs `len` zero bytes, with different arity, incompatible with protocol `Container.new`.
- Proposal: rename current function to `bytes.zeros(n: Int) -> Bytes` (same spelling as `matrix.zeros`), make `bytes.new() -> Bytes` empty.
- (a) All languages pass length for n-byte zero buffers (Rust `vec![0; n]`, Go `make([]byte, n)`, Python `bytes(n)`, JS `new Uint8Array(n)`, Swift `Data(count: n)`). Empty buffers use other forms (Rust `Vec::new()`, Swift `Data()`).
- Name reuse (R-L0-43 item 2): old `bytes.new(10)` arity-mismatches and type-errors, not silently changes meaning.
- (d) Rename/add. `almide fix` rewrites `bytes.new(n)` to `bytes.zeros(n)`.
- Issue: #4012.

<a id="r-bytes-29-定数時間の比較-bytesis_equal_constant_time-を置く---採用"></a>

### R-bytes-29 Add constant-time comparison `bytes.is_equal_constant_time` — ○ Accepted

- Current: `==` returns at first differing byte. Comparing MACs/tokens with `==` lets timing reveal values byte by byte.
- Proposal: `bytes.is_equal_constant_time(a: Bytes, b: Bytes) -> Bool`. Unequal lengths return false immediately (length public). Equal lengths read all bytes, OR `a[i] xor b[i]`, compare once to zero at end. Alongside `string.is_equal_constant_time` (R-string-19), a {Bytes, String} type-matrix cell (R-L0-01 exception).
- Native uses `@intrinsic` (runtime `core::hint::black_box` each time); wasm uses a fixed loop whose only branch compares indices. `tests/constant_time_compare_structure_test.rs` inspects generated code.
- Supporting L0 rules: R-L0-09 (Bool `is_`), R-L0-30 (equal-status operands `a`/`b`).
- (a) Go `subtle.ConstantTimeCompare`, Python `hmac.compare_digest`, Node `crypto.timingSafeEqual`, Java `MessageDigest.isEqual`.
- Protocol (R-L0-37): same role in `String`/`Bytes`, declare once as `Text` requirement ([protocol-view](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/protocol-view.md) `Text` row; module functions are the requirement itself).
- Spelling (R-L0-00): no majority (Go `ConstantTimeCompare`/`hmac.Equal`, Python `compare_digest`, Node `timingSafeEqual`, Erlang `hash_equals`, Rust `subtle`'s `ct_eq`, Java `isEqual`). Avoid `compare`, which means `Ord`'s three-way result (R-L0-16); use Bool `is_` (R-L0-09) and spell equality/constant time directly.
- (d) Add. Contract C-423. Issue: #4106 (R-hash-06).

<a id="l3-関数表"></a>

## L3: Function table

(146 functions + 13 additions. Verdicts: keep 58, rename 53, remove 31, semantic changes 4, add 13. The 15 renamed `put_` functions also change out-of-range behavior in R-bytes-11)

| Function | Signature (current, mut shown) | Main spellings in other languages | Verdict | Proposal |
|---|---|---|---|---|
| `append_f32_be` | `bytes.append_f32_be(mut b: Bytes, val: Float) -> Unit` | Go AppendUint32 (F) / Rust extend_from_slice(&x.to_be_bytes()) | Keep | — |
| `append_f32_le` | `bytes.append_f32_le(mut b: Bytes, val: Float) -> Unit` | Go AppendUint32 (F) / Rust extend_from_slice(&x.to_le_bytes()) | Keep | — |
| `append_f64_be` | `bytes.append_f64_be(mut b: Bytes, val: Float) -> Unit` | Go AppendUint64 (F) / Rust extend_from_slice(&x.to_be_bytes()) | Keep | — |
| `append_f64_le` | `bytes.append_f64_le(mut b: Bytes, val: Float) -> Unit` | Go AppendUint64 (F) / Rust extend_from_slice(&x.to_le_bytes()) | Keep | — |
| `append_i16_be` | `bytes.append_i16_be(mut b: Bytes, val: Int) -> Unit` | Go AppendUint16 (F) / Rust extend_from_slice(&x.to_be_bytes()) | Keep | — |
| `append_i16_le` | `bytes.append_i16_le(mut b: Bytes, val: Int) -> Unit` | Go AppendUint16 (F) / Rust extend_from_slice(&x.to_le_bytes()) | Keep | — |
| `append_i32_be` | `bytes.append_i32_be(mut b: Bytes, val: Int) -> Unit` | Go AppendUint32 (F) / Rust extend_from_slice(&x.to_be_bytes()) | Keep | — |
| `append_i32_le` | `bytes.append_i32_le(mut b: Bytes, val: Int) -> Unit` | Go AppendUint32 (F) / Rust extend_from_slice(&x.to_le_bytes()) | Keep | — |
| `append_i64_be` | `bytes.append_i64_be(mut b: Bytes, val: Int) -> Unit` | Go AppendUint64 (F) / Rust extend_from_slice(&x.to_be_bytes()) | Keep | — |
| `append_i64_le` | `bytes.append_i64_le(mut b: Bytes, val: Int) -> Unit` | Go AppendUint64 (F) / Rust extend_from_slice(&x.to_le_bytes()) | Keep | — |
| `append_u16_be` | `bytes.append_u16_be(mut b: Bytes, val: Int) -> Unit` | Go AppendUint16 (F) / Rust extend_from_slice(&x.to_be_bytes()) | Keep | — |
| `append_u16_le` | `bytes.append_u16_le(mut b: Bytes, val: Int) -> Unit` | Go AppendUint16 (F) / Rust extend_from_slice(&x.to_le_bytes()) | Keep | — |
| `append_u32_be` | `bytes.append_u32_be(mut b: Bytes, val: Int) -> Unit` | Go AppendUint32 (F) / Rust extend_from_slice(&x.to_be_bytes()) | Keep | — |
| `append_u32_le` | `bytes.append_u32_le(mut b: Bytes, val: Int) -> Unit` | Go AppendUint32 (F) / Rust extend_from_slice(&x.to_le_bytes()) | Keep | — |
| `append_u8` | `bytes.append_u8(mut b: Bytes, val: Int) -> Unit` | Rust push / Python append / Go append(b, x) | Keep | R-bytes-25 |
| `as_mut_ptr` | `bytes.as_mut_ptr(b: Bytes) -> RawPtr` | Rust as_mut_ptr | Remove → `ffi.address` | R-bytes-06 |
| `as_ptr` | `bytes.as_ptr(b: Bytes) -> RawPtr` | Rust as_ptr / Go unsafe.SliceData | Remove → `ffi.address` | R-bytes-06 |
| `chunks` | `bytes.chunks(b: Bytes, size: Int) -> List[Bytes]` | Rust chunks / Kotlin chunked / Almide list.chunk | Rename → `chunk` | R-bytes-08 |
| `clear` | `bytes.clear(mut b: Bytes) -> Unit` | Rust clear / Python bytearray.clear | Keep | — |
| `cmp` | `bytes.cmp(a: Bytes, b: Bytes) -> Int` | Rust cmp / Go bytes.Compare / Gleam compare (F) / OCaml Bytes.compare (F) | Rename → `compare` | R-bytes-08 |
| `concat` | `bytes.concat(a: Bytes, b: Bytes) -> Bytes` | Python a + b / Gleam bit_array.append (F) / OCaml Bytes.cat (F) / Go append(a, b...) | Remove → `+` | R-bytes-16 |
| `contains` | `bytes.contains(b: Bytes, pattern: Bytes) -> Bool` | Go bytes.Contains / Python in / Kotlin contains | Keep | — |
| `copy_from` | `bytes.copy_from(mut dst: Bytes, src: Bytes, dst_off: Int, src_off: Int, len: Int) -> Unit` | Rust copy_from_slice / Go copy / OCaml Bytes.blit (F) | Keep | — |
| `copy_to_ptr` | `bytes.copy_to_ptr(b: Bytes, ptr: RawPtr, cap: Int) -> Int` | Rust ptr::copy_nonoverlapping | Remove → `ffi.write` | R-bytes-06 |
| `data_ptr` | `bytes.data_ptr(b: Bytes) -> Int` | Go uintptr(unsafe.Pointer) | Remove → `ffi.address` | R-bytes-06 |
| `ends_with` | `bytes.ends_with(b: Bytes, suffix: Bytes) -> Bool` | Go bytes.HasSuffix / Python endswith | Keep | — |
| `eof` | `bytes.eof(b: Bytes, pos: Int) -> Bool` | Go io.EOF (error value) / Python — / Most: compare length | Rename → `is_eof` | R-bytes-07 |
| `fill` | `bytes.fill(mut b: Bytes, val: Int) -> Unit` | Rust fill / JS fill / OCaml Bytes.fill (F) | Keep | — |
| `from_list` | `bytes.from_list(xs: List[Int]) -> Bytes` | Rust Vec::from / Python bytes(list) / JS Uint8Array.from / Kotlin byteArrayOf | Keep | — |
| `from_raw_ptr` | `bytes.from_raw_ptr(ptr: RawPtr, len: Int) -> Bytes` | Rust slice::from_raw_parts / Go unsafe.Slice | Remove → `ffi.read` | R-bytes-06 |
| `from_string` | `bytes.from_string(s: String) -> Bytes` | Rust String::into_bytes / Go []byte(s) / Python s.encode() / JS TextEncoder / Gleam bit_array.from_string (F) / OCaml Bytes.of_string (F) | Keep | R-bytes-10 |
| `get` | `bytes.get(b: Bytes, i: Int) -> Option[Int]` | Rust get → Option / Kotlin getOrNull / Swift — | Keep (also serves as positional u8 read cell) | R-bytes-09 |
| `get_or` | `bytes.get_or(b: Bytes, i: Int, default: Int) -> Int` | Python get none / Kotlin getOrElse / Most: operators/general functions | Remove → `get(b, i) ?? d` | R-bytes-07 |
| `index_of` | `bytes.index_of(b: Bytes, pattern: Bytes) -> Option[Int]` | Go bytes.Index / Python b.find / JS indexOf | Keep | — |
| `insert` | `bytes.insert(b: Bytes, pos: Int, val: Int) -> Bytes` | Python bytearray.insert / Rust Vec::insert (in-place) | Semantic change (`0..=len` aborts outside) | R-bytes-11 |
| `is_empty` | `bytes.is_empty(b: Bytes) -> Bool` | Rust is_empty / Kotlin isEmpty / Swift isEmpty | Keep | — |
| `is_valid_utf8` | `bytes.is_valid_utf8(b: Bytes) -> Bool` | Go utf8.Valid / Rust str::from_utf8(..).is_ok() | Keep | — |
| `len` | `bytes.len(b: Bytes) -> Int` | Rust len / Go len / Python len / JS length / Gleam byte_size / OCaml length | Keep | — |
| `lines` | `bytes.lines(b: Bytes) -> List[Bytes]` | Go bufio.ScanLines / Python splitlines | Remove (→ `string.lines(bytes.to_string(b)!)` or `bytes.split`) | R-bytes-17 |
| `map_each` | `bytes.map_each(b: Bytes, f: (Int) -> Int) -> Bytes` | Kotlin map / Python bytes(map(f, b)) / Almide list.map | Rename → `map` | R-bytes-08 |
| `new` | `bytes.new(len: Int) -> Bytes` | Rust vec![0; n] / Go make([]byte, n) / Python bytes(n) / JS new Uint8Array(n) / OCaml Bytes.make (F) / Kotlin ByteArray(n) | Rename → `zeros` | R-bytes-28 |
| `pad_left` | `bytes.pad_left(b: Bytes, target_len: Int, val: Int) -> Bytes` | JS padStart / Almide string.pad_start | Rename → `pad_start` | R-bytes-08 |
| `pad_right` | `bytes.pad_right(b: Bytes, target_len: Int, val: Int) -> Bytes` | JS padEnd / Almide string.pad_end | Rename → `pad_end` | R-bytes-08 |
| `push` | `bytes.push(mut b: Bytes, val: Int) -> Unit` | Rust Vec::push / Python bytearray.append / Almide list.push | Keep | R-bytes-02 |
| `set_at` | `bytes.set_at(mut b: Bytes, i: Int, val: Int) -> Unit` | OCaml set_uint8 (F) / JS setUint8 (F) / Most: b[i] = v | Remove → `put_u8` (out of range aborts) | R-bytes-02, R-bytes-11 |
| `copy_within` | `bytes.copy_within(mut b: Bytes, src_start: Int, src_end: Int, dst: Int) -> Unit` | Rust copy_within / JS copyWithin | Keep | — |
| `read_bool` | `bytes.read_bool(b: Bytes, pos: Int) -> Bool` | Python struct '?' / Most: — (read_u8 != 0) | Rename → `get_bool` (none, not false, out of range) | R-bytes-09 |
| `read_bool_at` | `bytes.read_bool_at(b: Bytes, pos: Int) -> (Int, Option[Bool])` | — | Rename → `next_bool` (`(値, 次の位置)` returned) | R-bytes-13 |
| `read_f16_le` | `bytes.read_f16_le(b: Bytes, pos: Int) -> Float` | Go binary.LittleEndian.Uint16 (F) / JS getFloat16(o, true) (F) / Python struct '<e' | Rename → `get_f16_le` (none, not zero, out of range) | R-bytes-09 |
| `read_f16_le_at` | `bytes.read_f16_le_at(b: Bytes, pos: Int) -> (Int, Option[Float])` | — (Most: position-bearing reader type; Go bytes.Reader + binary.Read) | Rename → `next_f16_le` (`(値, 次の位置)` returned) | R-bytes-13 |
| `read_f16_le_array` | `bytes.read_f16_le_array(b: Bytes, pos: Int, count: Int) -> List[Float]` | Python struct.unpack_from('<Ne') / Go binary.Read(r, LittleEndian, []T) | Rename → `get_f16_le_array` (none, not zero, out of range) | R-bytes-09 |
| `read_f32_be` | `bytes.read_f32_be(b: Bytes, pos: Int) -> Float` | Go binary.BigEndian.Uint32 (F) / JS getFloat32(o, false) (F) / Python struct '>f' | Rename → `get_f32_be` (none, not zero, out of range) | R-bytes-09 |
| `read_f32_be_array` | `bytes.read_f32_be_array(b: Bytes, pos: Int, count: Int) -> List[Float]` | Python struct.unpack_from('>Nf') / Go binary.Read(r, BigEndian, []T) | Rename → `get_f32_be_array` (none, not zero, out of range) | R-bytes-09 |
| `read_f32_be_at` | `bytes.read_f32_be_at(b: Bytes, pos: Int) -> (Int, Option[Float])` | — (Most: position-bearing reader type; Go bytes.Reader + binary.Read) | Rename → `next_f32_be` (`(値, 次の位置)` returned) | R-bytes-13 |
| `read_f32_le` | `bytes.read_f32_le(b: Bytes, pos: Int) -> Float` | Go binary.LittleEndian.Uint32 (F) / JS getFloat32(o, true) (F) / Python struct '<f' | Rename → `get_f32_le` (none, not zero, out of range) | R-bytes-09 |
| `read_f32_le_array` | `bytes.read_f32_le_array(b: Bytes, pos: Int, count: Int) -> List[Float]` | Python struct.unpack_from('<Nf') / Go binary.Read(r, LittleEndian, []T) | Rename → `get_f32_le_array` (none, not zero, out of range) | R-bytes-09 |
| `read_f32_le_at` | `bytes.read_f32_le_at(b: Bytes, pos: Int) -> (Int, Option[Float])` | — (Most: position-bearing reader type; Go bytes.Reader + binary.Read) | Rename → `next_f32_le` (`(値, 次の位置)` returned) | R-bytes-13 |
| `read_f64_be` | `bytes.read_f64_be(b: Bytes, pos: Int) -> Float` | Go binary.BigEndian.Uint64 (F) / JS getFloat64(o, false) (F) / Python struct '>d' | Rename → `get_f64_be` (none, not zero, out of range) | R-bytes-09 |
| `read_f64_be_array` | `bytes.read_f64_be_array(b: Bytes, pos: Int, count: Int) -> List[Float]` | Python struct.unpack_from('>Nd') / Go binary.Read(r, BigEndian, []T) | Rename → `get_f64_be_array` (none, not zero, out of range) | R-bytes-09 |
| `read_f64_be_at` | `bytes.read_f64_be_at(b: Bytes, pos: Int) -> (Int, Option[Float])` | — (Most: position-bearing reader type; Go bytes.Reader + binary.Read) | Rename → `next_f64_be` (`(値, 次の位置)` returned) | R-bytes-13 |
| `read_f64_le` | `bytes.read_f64_le(b: Bytes, pos: Int) -> Float` | Go binary.LittleEndian.Uint64 (F) / JS getFloat64(o, true) (F) / Python struct '<d' | Rename → `get_f64_le` (none, not zero, out of range) | R-bytes-09 |
| `read_f64_le_array` | `bytes.read_f64_le_array(b: Bytes, pos: Int, count: Int) -> List[Float]` | Python struct.unpack_from('<Nd') / Go binary.Read(r, LittleEndian, []T) | Rename → `get_f64_le_array` (none, not zero, out of range) | R-bytes-09 |
| `read_f64_le_at` | `bytes.read_f64_le_at(b: Bytes, pos: Int) -> (Int, Option[Float])` | — (Most: position-bearing reader type; Go bytes.Reader + binary.Read) | Rename → `next_f64_le` (`(値, 次の位置)` returned) | R-bytes-13 |
| `read_i16_be` | `bytes.read_i16_be(b: Bytes, pos: Int) -> Int` | Go binary.BigEndian.Uint16 (F) / OCaml get_int16_be (F) / JS getInt16(o, false) (F) / Python struct '>h' | Rename → `get_i16_be` (none, not zero, out of range) | R-bytes-09 |
| `read_i16_be_at` | `bytes.read_i16_be_at(b: Bytes, pos: Int) -> (Int, Option[Int])` | — (Most: position-bearing reader type; Go bytes.Reader + binary.Read) | Rename → `next_i16_be` (`(値, 次の位置)` returned) | R-bytes-13 |
| `read_i16_be_array` | `bytes.read_i16_be_array(b: Bytes, pos: Int, count: Int) -> List[Int]` | Python struct.unpack_from('>Nh') / Go binary.Read(r, BigEndian, []T) | Rename → `get_i16_be_array` (none, not zero, out of range) | R-bytes-09 |
| `read_i16_le` | `bytes.read_i16_le(b: Bytes, pos: Int) -> Int` | Go binary.LittleEndian.Uint16 (F) / OCaml get_int16_le (F) / JS getInt16(o, true) (F) / Python struct '<h' | Rename → `get_i16_le` (none, not zero, out of range) | R-bytes-09 |
| `read_i16_le_at` | `bytes.read_i16_le_at(b: Bytes, pos: Int) -> (Int, Option[Int])` | — (Most: position-bearing reader type; Go bytes.Reader + binary.Read) | Rename → `next_i16_le` (`(値, 次の位置)` returned) | R-bytes-13 |
| `read_i16_le_array` | `bytes.read_i16_le_array(b: Bytes, pos: Int, count: Int) -> List[Int]` | Python struct.unpack_from('<Nh') / Go binary.Read(r, LittleEndian, []T) | Rename → `get_i16_le_array` (none, not zero, out of range) | R-bytes-09 |
| `read_i32_be` | `bytes.read_i32_be(b: Bytes, pos: Int) -> Int` | Go binary.BigEndian.Uint32 (F) / OCaml get_int32_be (F) / JS getInt32(o, false) (F) / Python struct '>i' | Rename → `get_i32_be` (none, not zero, out of range) | R-bytes-09 |
| `read_i32_be_array` | `bytes.read_i32_be_array(b: Bytes, pos: Int, count: Int) -> List[Int]` | Python struct.unpack_from('>Ni') / Go binary.Read(r, BigEndian, []T) | Rename → `get_i32_be_array` (none, not zero, out of range) | R-bytes-09 |
| `read_i32_be_at` | `bytes.read_i32_be_at(b: Bytes, pos: Int) -> (Int, Option[Int])` | — (Most: position-bearing reader type; Go bytes.Reader + binary.Read) | Rename → `next_i32_be` (`(値, 次の位置)` returned) | R-bytes-13 |
| `read_i32_le` | `bytes.read_i32_le(b: Bytes, pos: Int) -> Int` | Go binary.LittleEndian.Uint32 (F) / OCaml get_int32_le (F) / JS getInt32(o, true) (F) / Python struct '<i' | Rename → `get_i32_le` (none, not zero, out of range) | R-bytes-09 |
| `read_i32_le_array` | `bytes.read_i32_le_array(b: Bytes, pos: Int, count: Int) -> List[Int]` | Python struct.unpack_from('<Ni') / Go binary.Read(r, LittleEndian, []T) | Rename → `get_i32_le_array` (none, not zero, out of range) | R-bytes-09 |
| `read_i32_le_at` | `bytes.read_i32_le_at(b: Bytes, pos: Int) -> (Int, Option[Int])` | — (Most: position-bearing reader type; Go bytes.Reader + binary.Read) | Rename → `next_i32_le` (`(値, 次の位置)` returned) | R-bytes-13 |
| `read_i64_be` | `bytes.read_i64_be(b: Bytes, pos: Int) -> Int` | Go binary.BigEndian.Uint64 (F) / OCaml get_int64_be (F) / JS getBigInt64(o, false) (F) / Python struct '>q' | Rename → `get_i64_be` (none, not zero, out of range) | R-bytes-09 |
| `read_i64_be_array` | `bytes.read_i64_be_array(b: Bytes, pos: Int, count: Int) -> List[Int]` | Python struct.unpack_from('>Nq') / Go binary.Read(r, BigEndian, []T) | Rename → `get_i64_be_array` (none, not zero, out of range) | R-bytes-09 |
| `read_i64_be_at` | `bytes.read_i64_be_at(b: Bytes, pos: Int) -> (Int, Option[Int])` | — (Most: position-bearing reader type; Go bytes.Reader + binary.Read) | Rename → `next_i64_be` (`(値, 次の位置)` returned) | R-bytes-13 |
| `read_i64_le` | `bytes.read_i64_le(b: Bytes, pos: Int) -> Int` | Go binary.LittleEndian.Uint64 (F) / OCaml get_int64_le (F) / JS getBigInt64(o, true) (F) / Python struct '<q' | Rename → `get_i64_le` (none, not zero, out of range) | R-bytes-09 |
| `read_i64_le_array` | `bytes.read_i64_le_array(b: Bytes, pos: Int, count: Int) -> List[Int]` | Python struct.unpack_from('<Nq') / Go binary.Read(r, LittleEndian, []T) | Rename → `get_i64_le_array` (none, not zero, out of range) | R-bytes-09 |
| `read_i64_le_at` | `bytes.read_i64_le_at(b: Bytes, pos: Int) -> (Int, Option[Int])` | — (Most: position-bearing reader type; Go bytes.Reader + binary.Read) | Rename → `next_i64_le` (`(値, 次の位置)` returned) | R-bytes-13 |
| `read_length_prefixed_strings_le` | `bytes.read_length_prefixed_strings_le(b: Bytes, pos: Int, count: Int) -> List[String]` | — (format-specific) | Remove (→ `get_u32_le` and `slice` combination) | R-bytes-05 |
| `read_string_at` | `bytes.read_string_at(b: Bytes, pos: Int, len: Int) -> String` | Python b[p:p+n].decode / Rust from_utf8_lossy(&b[p..p+n]) | Remove (→ `slice` ＋ `to_string_lossy`) | R-bytes-05 |
| `read_string_be` | `bytes.read_string_be(b: Bytes, pos: Int) -> String` | Java DataInput.readUTF (u16 BE) / Most: — | Remove (→ `get_u32_be` and `slice` combination) | R-bytes-05 |
| `read_string_be_at` | `bytes.read_string_be_at(b: Bytes, pos: Int) -> (Int, Option[String])` | — | Remove (→ `next_u32_be` and `next_bytes` combination) | R-bytes-05 |
| `read_u16_be` | `bytes.read_u16_be(b: Bytes, pos: Int) -> Int` | Go binary.BigEndian.Uint16 (F) / OCaml get_uint16_be (F) / JS getUint16(o, false) (F) / Python struct '>H' | Rename → `get_u16_be` (none, not zero, out of range) | R-bytes-09 |
| `read_u16_be_at` | `bytes.read_u16_be_at(b: Bytes, pos: Int) -> (Int, Option[Int])` | — (Most: position-bearing reader type; Go bytes.Reader + binary.Read) | Rename → `next_u16_be` (`(値, 次の位置)` returned) | R-bytes-13 |
| `read_u16_be_array` | `bytes.read_u16_be_array(b: Bytes, pos: Int, count: Int) -> List[Int]` | Python struct.unpack_from('>NH') / Go binary.Read(r, BigEndian, []T) | Rename → `get_u16_be_array` (none, not zero, out of range) | R-bytes-09 |
| `read_u16_le` | `bytes.read_u16_le(b: Bytes, pos: Int) -> Int` | Go binary.LittleEndian.Uint16 (F) / OCaml get_uint16_le (F) / JS getUint16(o, true) (F) / Python struct '<H' | Rename → `get_u16_le` (none, not zero, out of range) | R-bytes-09 |
| `read_u16_le_array` | `bytes.read_u16_le_array(b: Bytes, pos: Int, count: Int) -> List[Int]` | Python struct.unpack_from('<NH') / Go binary.Read(r, LittleEndian, []T) | Rename → `get_u16_le_array` (none, not zero, out of range) | R-bytes-09 |
| `read_u16_le_at` | `bytes.read_u16_le_at(b: Bytes, pos: Int) -> (Int, Option[Int])` | — (Most: position-bearing reader type; Go bytes.Reader + binary.Read) | Rename → `next_u16_le` (`(値, 次の位置)` returned) | R-bytes-13 |
| `read_u32_be` | `bytes.read_u32_be(b: Bytes, pos: Int) -> Int` | Go binary.BigEndian.Uint32 (F) / OCaml get_int32_be (F) / JS getUint32(o, false) (F) / Python struct '>I' | Rename → `get_u32_be` (none, not zero, out of range) | R-bytes-09 |
| `read_u32_be_array` | `bytes.read_u32_be_array(b: Bytes, pos: Int, count: Int) -> List[Int]` | Python struct.unpack_from('>NI') / Go binary.Read(r, BigEndian, []T) | Rename → `get_u32_be_array` (none, not zero, out of range) | R-bytes-09 |
| `read_u32_be_at` | `bytes.read_u32_be_at(b: Bytes, pos: Int) -> (Int, Option[Int])` | — (Most: position-bearing reader type; Go bytes.Reader + binary.Read) | Rename → `next_u32_be` (`(値, 次の位置)` returned) | R-bytes-13 |
| `read_u32_le` | `bytes.read_u32_le(b: Bytes, pos: Int) -> Int` | Go binary.LittleEndian.Uint32 (F) / OCaml get_int32_le (F) / JS getUint32(o, true) (F) / Python struct '<I' | Rename → `get_u32_le` (none, not zero, out of range) | R-bytes-09 |
| `read_u32_le_array` | `bytes.read_u32_le_array(b: Bytes, pos: Int, count: Int) -> List[Int]` | Python struct.unpack_from('<NI') / Go binary.Read(r, LittleEndian, []T) | Rename → `get_u32_le_array` (none, not zero, out of range) | R-bytes-09 |
| `read_u32_le_at` | `bytes.read_u32_le_at(b: Bytes, pos: Int) -> (Int, Option[Int])` | — (Most: position-bearing reader type; Go bytes.Reader + binary.Read) | Rename → `next_u32_le` (`(値, 次の位置)` returned) | R-bytes-13 |
| `read_u8` | `bytes.read_u8(b: Bytes, pos: Int) -> Int` | OCaml get_uint8 (F) / JS getUint8 (F) / Most: b[i] | Remove → `get` (same role; none out of range) | R-bytes-09 |
| `read_u8_at` | `bytes.read_u8_at(b: Bytes, pos: Int) -> (Int, Option[Int])` | — (Most: position-bearing reader type) | Rename → `next_u8` (`(値, 次の位置)` returned) | R-bytes-13 |
| `remove_at` | `bytes.remove_at(b: Bytes, pos: Int) -> Bytes` | Rust Vec::remove / Python del b[i] (in-place) | Semantic change (out of range aborts) | R-bytes-11 |
| `repeat` | `bytes.repeat(b: Bytes, n: Int) -> Bytes` | Go bytes.Repeat / Python b * n | Keep | — |
| `reverse` | `bytes.reverse(b: Bytes) -> Bytes` | Rust reverse / Kotlin reversed | Keep | — |
| `set` | `bytes.set(b: Bytes, i: Int, val: Int) -> Bytes` | Kotlin copyOf + set / Most: subscript assignment (in-place) | Semantic change (out of range aborts) | R-bytes-11 |
| `set_f32_be` | `bytes.set_f32_be(mut b: Bytes, pos: Int, val: Float) -> Unit` | Go PutUint32 (F) / OCaml set_*_le・_be (F) / JS DataView setFloat32 (F) | Rename → `put_f32_be` (out of range aborts) | R-bytes-01, R-bytes-11 |
| `set_f32_le` | `bytes.set_f32_le(mut b: Bytes, pos: Int, val: Float) -> Unit` | Go PutUint32 (F) / OCaml set_*_le・_be (F) / JS DataView setFloat32 (F) | Rename → `put_f32_le` (out of range aborts) | R-bytes-01, R-bytes-11 |
| `set_f64_be` | `bytes.set_f64_be(mut b: Bytes, pos: Int, val: Float) -> Unit` | Go PutUint64 (F) / OCaml set_*_le・_be (F) / JS DataView setFloat64 (F) | Rename → `put_f64_be` (out of range aborts) | R-bytes-01, R-bytes-11 |
| `set_f64_le` | `bytes.set_f64_le(mut b: Bytes, pos: Int, val: Float) -> Unit` | Go PutUint64 (F) / OCaml set_*_le・_be (F) / JS DataView setFloat64 (F) | Rename → `put_f64_le` (out of range aborts) | R-bytes-01, R-bytes-11 |
| `set_i16_be` | `bytes.set_i16_be(mut b: Bytes, pos: Int, val: Int) -> Unit` | Go PutUint16 (F) / OCaml set_*_le・_be (F) / JS DataView setInt16 (F) | Rename → `put_i16_be` (out of range aborts) | R-bytes-01, R-bytes-11 |
| `set_i16_le` | `bytes.set_i16_le(mut b: Bytes, pos: Int, val: Int) -> Unit` | Go PutUint16 (F) / OCaml set_*_le・_be (F) / JS DataView setInt16 (F) | Rename → `put_i16_le` (out of range aborts) | R-bytes-01, R-bytes-11 |
| `set_i32_be` | `bytes.set_i32_be(mut b: Bytes, pos: Int, val: Int) -> Unit` | Go PutUint32 (F) / OCaml set_*_le・_be (F) / JS DataView setInt32 (F) | Rename → `put_i32_be` (out of range aborts) | R-bytes-01, R-bytes-11 |
| `set_i32_le` | `bytes.set_i32_le(mut b: Bytes, pos: Int, val: Int) -> Unit` | Go PutUint32 (F) / OCaml set_*_le・_be (F) / JS DataView setInt32 (F) | Rename → `put_i32_le` (out of range aborts) | R-bytes-01, R-bytes-11 |
| `set_i64_be` | `bytes.set_i64_be(mut b: Bytes, pos: Int, val: Int) -> Unit` | Go PutUint64 (F) / OCaml set_*_le・_be (F) / JS DataView setBigInt64 (F) | Rename → `put_i64_be` (out of range aborts) | R-bytes-01, R-bytes-11 |
| `set_i64_le` | `bytes.set_i64_le(mut b: Bytes, pos: Int, val: Int) -> Unit` | Go PutUint64 (F) / OCaml set_*_le・_be (F) / JS DataView setBigInt64 (F) | Rename → `put_i64_le` (out of range aborts) | R-bytes-01, R-bytes-11 |
| `set_u16_be` | `bytes.set_u16_be(mut b: Bytes, pos: Int, val: Int) -> Unit` | Go PutUint16 (F) / OCaml set_*_le・_be (F) / JS DataView setUint16 (F) | Rename → `put_u16_be` (out of range aborts) | R-bytes-01, R-bytes-11 |
| `set_u16_le` | `bytes.set_u16_le(mut b: Bytes, pos: Int, val: Int) -> Unit` | Go PutUint16 (F) / OCaml set_*_le・_be (F) / JS DataView setUint16 (F) | Rename → `put_u16_le` (out of range aborts) | R-bytes-01, R-bytes-11 |
| `set_u32_be` | `bytes.set_u32_be(mut b: Bytes, pos: Int, val: Int) -> Unit` | Go PutUint32 (F) / OCaml set_*_le・_be (F) / JS DataView setUint32 (F) | Rename → `put_u32_be` (out of range aborts) | R-bytes-01, R-bytes-11 |
| `set_u32_le` | `bytes.set_u32_le(mut b: Bytes, pos: Int, val: Int) -> Unit` | Go PutUint32 (F) / OCaml set_*_le・_be (F) / JS DataView setUint32 (F) | Rename → `put_u32_le` (out of range aborts) | R-bytes-01, R-bytes-11 |
| `set_u8` | `bytes.set_u8(mut b: Bytes, pos: Int, val: Int) -> Unit` | Go b[i] = x / OCaml set_*_le・_be (F) / JS DataView setUint8 (F) | Rename → `put_u8` (out of range aborts) | R-bytes-01, R-bytes-11 |
| `skip` | `bytes.skip(b: Bytes, pos: Int, n: Int) -> Int` | Go bytes.Reader.Seek / Python — | Semantic change (`n < 0` aborts) | R-bytes-13, R-bytes-21 |
| `skip_length_prefixed_le` | `bytes.skip_length_prefixed_le(b: Bytes, pos: Int, count: Int) -> Int` | — (format-specific) | Remove (→ `get_u32_le` and `slice` combination) | R-bytes-05 |
| `slice` | `bytes.slice(b: Bytes, start: Int, end: Int) -> Bytes` | Rust &b[a..b] / Go b[a:b] / Python b[a:b] / JS subarray・slice / OCaml Bytes.sub (F) | Semantic change (negative positions abort; R-L0-29) | R-bytes-12, R-L0-29 |
| `split` | `bytes.split(b: Bytes, sep: Bytes) -> List[Bytes]` | Go bytes.Split / Python b.split | Keep | — |
| `starts_with` | `bytes.starts_with(b: Bytes, prefix: Bytes) -> Bool` | Go bytes.HasPrefix / Python startswith / Gleam starts_with (F) | Keep | — |
| `take_at` | `bytes.take_at(b: Bytes, pos: Int, n: Int) -> (Int, Option[Bytes])` | Go bytes.Reader.Read / Python b[p:p+n] | Rename → `next_bytes` (`(値, 次の位置)` returned) | R-bytes-13 |
| `to_list` | `bytes.to_list(b: Bytes) -> List[Int]` | Python list(b) / JS Array.from / Kotlin toList | Keep | — |
| `to_string` | `bytes.to_string(b: Bytes) -> Result[String, String]` | Rust String::from_utf8 → Result / Gleam bit_array.to_string → Result (F) / Python b.decode() / Kotlin decodeToString | Keep | R-bytes-10 |
| `to_string_lossy` | `bytes.to_string_lossy(b: Bytes) -> String` | Rust String::from_utf8_lossy / Python decode(errors='replace') | Keep | R-bytes-10 |
| `write_bool` | `bytes.write_bool(mut b: Bytes, val: Bool) -> Unit` | Python struct.pack('?') / Most: — | Rename → `append_bool` | R-bytes-03 |
| `write_f64_be` | `bytes.write_f64_be(mut b: Bytes, val: Float) -> Unit` | Go AppendUint64(math.Float64bits) (F) | Remove → `append_f64_be` | R-bytes-03 |
| `write_i64_be` | `bytes.write_i64_be(mut b: Bytes, val: Int) -> Unit` | Go AppendUint64 (F) | Remove → `append_i64_be` | R-bytes-03 |
| `write_string_be` | `bytes.write_string_be(mut b: Bytes, s: String) -> Unit` | Java DataOutput.writeUTF (u16 BE) / Most: — | Remove (→ `append_u32_be` ＋ `append_bytes`) | R-bytes-05 |
| `write_u32_be` | `bytes.write_u32_be(mut b: Bytes, val: Int) -> Unit` | Go AppendUint32 (F) | Remove → `append_u32_be` | R-bytes-03 |
| `write_u8` | `bytes.write_u8(mut b: Bytes, val: Int) -> Unit` | Same as above | Remove → `push` | R-bytes-02 |
| `xor` | `bytes.xor(a: Bytes, b: Bytes) -> Bytes` | — (Most: write a loop) | Semantic change (unequal lengths abort) | R-bytes-23 |
| `read_uint16` | `bytes.read_uint16(b: Bytes, offset: Int, endian: Endian) -> UInt16` | Go binary.LittleEndian.Uint32 (F) / JS DataView getUint32(o, le) (F) | Remove (→ suffix family `get_`/`append_`/`put_`) | R-bytes-04 |
| `read_uint32` | `bytes.read_uint32(b: Bytes, offset: Int, endian: Endian) -> UInt32` | Go binary.LittleEndian.Uint32 (F) / JS DataView getUint32(o, le) (F) | Remove (→ suffix family `get_`/`append_`/`put_`) | R-bytes-04 |
| `read_int32` | `bytes.read_int32(b: Bytes, offset: Int, endian: Endian) -> Int32` | Go binary.LittleEndian.Uint32 (F) / JS DataView getUint32(o, le) (F) | Remove (→ suffix family `get_`/`append_`/`put_`) | R-bytes-04 |
| `read_float32` | `bytes.read_float32(b: Bytes, offset: Int, endian: Endian) -> Float32` | Go binary.LittleEndian.Uint32 (F) / JS DataView getUint32(o, le) (F) | Remove (→ suffix family `get_`/`append_`/`put_`) | R-bytes-04 |
| `write_uint16` | `bytes.write_uint16(mut b: Bytes, value: UInt16, endian: Endian) -> Unit` | Go binary.LittleEndian.Uint32 (F) / JS DataView getUint32(o, le) (F) | Remove (→ suffix family `get_`/`append_`/`put_`) | R-bytes-04 |
| `write_uint32` | `bytes.write_uint32(mut b: Bytes, value: UInt32, endian: Endian) -> Unit` | Go binary.LittleEndian.Uint32 (F) / JS DataView getUint32(o, le) (F) | Remove (→ suffix family `get_`/`append_`/`put_`) | R-bytes-04 |
| `write_int32` | `bytes.write_int32(mut b: Bytes, value: Int32, endian: Endian) -> Unit` | Go binary.LittleEndian.Uint32 (F) / JS DataView getUint32(o, le) (F) | Remove (→ suffix family `get_`/`append_`/`put_`) | R-bytes-04 |
| `write_float32` | `bytes.write_float32(mut b: Bytes, value: Float32, endian: Endian) -> Unit` | Go binary.LittleEndian.Uint32 (F) / JS DataView getUint32(o, le) (F) | Remove (→ suffix family `get_`/`append_`/`put_`) | R-bytes-04 |
| `set_uint16` | `bytes.set_uint16(mut b: Bytes, offset: Int, value: UInt16, endian: Endian) -> Unit` | Go binary.LittleEndian.Uint32 (F) / JS DataView getUint32(o, le) (F) | Remove (→ suffix family `get_`/`append_`/`put_`) | R-bytes-04 |
| `set_uint32` | `bytes.set_uint32(mut b: Bytes, offset: Int, value: UInt32, endian: Endian) -> Unit` | Go binary.LittleEndian.Uint32 (F) / JS DataView getUint32(o, le) (F) | Remove (→ suffix family `get_`/`append_`/`put_`) | R-bytes-04 |
| `set_int32` | `bytes.set_int32(mut b: Bytes, offset: Int, value: Int32, endian: Endian) -> Unit` | Go binary.LittleEndian.Uint32 (F) / JS DataView getUint32(o, le) (F) | Remove (→ suffix family `get_`/`append_`/`put_`) | R-bytes-04 |
| `set_float32` | `bytes.set_float32(mut b: Bytes, offset: Int, value: Float32, endian: Endian) -> Unit` | Go binary.LittleEndian.Uint32 (F) / JS DataView getUint32(o, le) (F) | Remove (→ suffix family `get_`/`append_`/`put_`) | R-bytes-04 |
| `put_f16_le` | `bytes.put_f16_le(mut b: Bytes, pos: Int, val: Float) -> Unit` (Proposal) | JS setFloat16 (F) / Python struct '<e' | Add | R-bytes-03 |
| `put_f16_be` | `bytes.put_f16_be(mut b: Bytes, pos: Int, val: Float) -> Unit` (Proposal) | JS setFloat16 (F) / Python struct '>e' | Add | R-bytes-03 |
| `append_f16_le` | `bytes.append_f16_le(mut b: Bytes, val: Float) -> Unit` (Proposal) | Python struct.pack('<e') | Add | R-bytes-03 |
| `append_f16_be` | `bytes.append_f16_be(mut b: Bytes, val: Float) -> Unit` (Proposal) | Python struct.pack('>e') | Add | R-bytes-03 |
| `get_f16_be` | `bytes.get_f16_be(b: Bytes, pos: Int) -> Option[Float]` (Proposal) | JS getFloat16(o, false) (F) / Python struct '>e' | Add | R-bytes-03 |
| `next_f16_be` | `bytes.next_f16_be(b: Bytes, pos: Int) -> (Option[Float], Int)` (Proposal) | — | Add | R-bytes-03, R-bytes-13 |
| `get_f16_be_array` | `bytes.get_f16_be_array(b: Bytes, pos: Int, count: Int) -> Option[List[Float]]` (Proposal) | Python struct.unpack_from('>Ne') | Add | R-bytes-03 |
| `put_bool` | `bytes.put_bool(mut b: Bytes, pos: Int, val: Bool) -> Unit` (Proposal) | Python struct.pack_into('?') | Add (out of range aborts) | R-bytes-03, R-bytes-11 |
| `append_bytes` | `bytes.append_bytes(mut b: Bytes, src: Bytes) -> Unit` (Proposal) | Rust extend_from_slice / Go append(b, src...) / Python bytearray.extend / OCaml Buffer.add_bytes | Add | R-bytes-05 |
| `take` | `bytes.take(b: Bytes, n: Int) -> Bytes` (Proposal) | Rust &b[..n] / Python b[:n] / Kotlin take / Almide list.take | Add (negative numbers abort; R-L0-29) | R-bytes-08 |
| `drop` | `bytes.drop(b: Bytes, n: Int) -> Bytes` (Proposal) | Rust &b[n..] / Python b[n:] / Kotlin drop / Almide list.drop | Add (negative numbers abort; R-L0-29) | R-bytes-08 |
| `take_end` | `bytes.take_end(b: Bytes, n: Int) -> Bytes` (Proposal) | Python b[-n:] / Kotlin takeLast / Almide list.take_end | Add (negative numbers abort; R-L0-29) | R-bytes-08 |
| `drop_end` | `bytes.drop_end(b: Bytes, n: Int) -> Bytes` (Proposal) | Python b[:-n] / Kotlin dropLast / Almide list.drop_end | Add (negative numbers abort; R-L0-29) | R-bytes-08 |
| `heap_save` | `bytes.heap_save() -> Int` | — | Remove (make names internal) | R-bytes-14 |
| `heap_restore` | `bytes.heap_restore(checkpoint: Int) -> Unit` | — | Remove (make names internal) | R-bytes-14 |
| `get_i8` | `bytes.get_i8(b: Bytes, pos: Int) -> Option[Int]` (Proposal) | OCaml `get_int8` (F) / JS `getInt8` | Add | R-bytes-15 |
| `next_i8` | `bytes.next_i8(b: Bytes, pos: Int) -> (Option[Int], Int)` (Proposal) | — | Add | R-bytes-15, R-bytes-13 |
| `put_i8` | `bytes.put_i8(mut b: Bytes, pos: Int, val: Int) -> Unit` (Proposal) | OCaml `set_int8` / JS `setInt8` | Add | R-bytes-15 |
| `append_i8` | `bytes.append_i8(mut b: Bytes, val: Int) -> Unit` (Proposal) | — | Add | R-bytes-15 |
| `filter` | `bytes.filter(b: Bytes, f: (Int) -> Bool) -> Bytes` (Proposal) | Kotlin `filter` / Rust `iter().filter` | Add | R-bytes-18 |
| `fold` | `bytes.fold(b: Bytes, init: B, f: (B, Int) -> B) -> B` (Proposal) | Kotlin `fold` / Rust `iter().fold` | Add | R-bytes-18 |
| `any` | `bytes.any(b: Bytes, f: (Int) -> Bool) -> Bool` (Proposal) | Kotlin `any` | Add | R-bytes-18 |
| `all` | `bytes.all(b: Bytes, f: (Int) -> Bool) -> Bool` (Proposal) | Kotlin `all` | Add | R-bytes-18 |
| `count` | `bytes.count(b: Bytes, f: (Int) -> Bool) -> Int` (Proposal) | Kotlin `count {}` | Add | R-bytes-18 |
| `find` | `bytes.find(b: Bytes, f: (Int) -> Bool) -> Option[Int]` (Proposal) | Kotlin `find` | Add | R-bytes-18 |
| `first` |  (Add)`bytes.first(b: Bytes) -> Option[Int]` | Rust first / Swift first | Add | R-bytes-20 |
| `last` |  (Add)`bytes.last(b: Bytes) -> Option[Int]` | Rust last / Swift last | Add | R-bytes-20 |
| `pop` |  (Add)`bytes.pop(mut b: Bytes) -> Option[Int]` | Rust Vec::pop / Python bytearray.pop | Add | R-bytes-20 |
| `get_u8` |  (Add)`bytes.get_u8(b: Bytes, pos: Int) -> Option[Int]` | Go `LittleEndian.Uint8` / JS `getUint8` / OCaml `get_uint8` | Add | R-bytes-25 |
| `enumerate` |  (Add)`bytes.enumerate(b: Bytes) -> List[(Int, Int)]` | Swift `enumerated` / Rust `enumerate` | Add | R-L0-42 |
| `zip` |  (Add)`bytes.zip(b: Bytes, ys: List[B]) -> List[(Int, B)]` | Swift `zip` / Rust `zip` | Add | R-L0-42 |
| `zip_with` |  (Add)`bytes.zip_with(b: Bytes, ys: List[B], f: (Int, B) -> C) -> List[C]` | Kotlin `zip(other, f)` | Add | R-L0-42 |
| `filter_map` |  (Add)`bytes.filter_map(b: Bytes, f: (Int) -> B?) -> List[B]` | Swift `compactMap` / Rust `filter_map` | Add | R-L0-42 |
| `find_map` |  (Add)`bytes.find_map(b: Bytes, f: (Int) -> B?) -> B?` | Rust `find_map` | Add | R-L0-42 |
| `reduce` |  (Add)`bytes.reduce(b: Bytes, f: (Int, Int) -> Int) -> Int?` | Swift `reduce` / Rust `reduce` | Add | R-L0-42 |
| `scan` |  (Add)`bytes.scan(b: Bytes, init: B, f: (B, Int) -> B) -> List[B]` | Rust `scan` / Kotlin `runningFold` | Add | R-L0-42 |
| `partition` |  (Add)`bytes.partition(b: Bytes, f: (Int) -> Bool) -> (List[Int], List[Int])` | Kotlin `partition` | Add | R-L0-42 |
| `group_by` |  (Add)`bytes.group_by(b: Bytes, f: (Int) -> B) -> Map[B, List[Int]]` | Kotlin `groupBy` / Swift `Dictionary(grouping:)` | Add | R-L0-42 |
| `sort` |  (Add)`bytes.sort(b: Bytes) -> List[Int]` | Swift `sorted()` | Add | R-L0-42 |
| `sort_by` |  (Add)`bytes.sort_by(b: Bytes, f: (Int) -> B) -> List[Int]` | Kotlin `sortedBy` | Add | R-L0-42 |
| `sort_with` |  (Add)`bytes.sort_with(b: Bytes, cmp: (Int, Int) -> Int) -> List[Int]` | Swift `sorted(by:)` | Add | R-L0-42 |
| `min` |  (Add)`bytes.min(b: Bytes) -> Int?` | Swift `min()` / Rust `min` | Add | R-L0-42 |
| `max` |  (Add)`bytes.max(b: Bytes) -> Int?` | Swift `max()` / Rust `max` | Add | R-L0-42 |
| `min_by` |  (Add)`bytes.min_by(b: Bytes, f: (Int) -> K) -> Int?` | Kotlin `minByOrNull` | Add | R-L0-42 |
| `max_by` |  (Add)`bytes.max_by(b: Bytes, f: (Int) -> K) -> Int?` | Kotlin `maxByOrNull` | Add | R-L0-42 |
| `min_with` |  (Add)`bytes.min_with(b: Bytes, cmp: (Int, Int) -> Int) -> Int?` | Swift `min(by:)` | Add | R-L0-42 |
| `max_with` |  (Add)`bytes.max_with(b: Bytes, cmp: (Int, Int) -> Int) -> Int?` | Swift `max(by:)` | Add | R-L0-42 |
| `sum` |  (Add)`bytes.sum(b: Bytes) -> Int` | Rust `sum` / Kotlin `sum` | Add | R-L0-42 |
| `product` |  (Add)`bytes.product(b: Bytes) -> Int` | Rust `product` | Add | R-L0-42 |
| `find_index` |  (Add)`bytes.find_index(b: Bytes, f: (Int) -> Bool) -> Int?` | Swift `firstIndex(where:)` | Add | R-L0-42 |
| `take_while` |  (Add)`bytes.take_while(b: Bytes, f: (Int) -> Bool) -> Bytes` | Swift `prefix(while:)` | Add | R-L0-42 |
| `drop_while` |  (Add)`bytes.drop_while(b: Bytes, f: (Int) -> Bool) -> Bytes` | Swift `drop(while:)` | Add | R-L0-42 |
| `window` |  (Add)`bytes.window(b: Bytes, n: Int) -> List[Bytes]` | Kotlin `windowed` | Add | R-L0-42 |
| `split_at` |  (Add)`bytes.split_at(b: Bytes, n: Int) -> (Bytes, Bytes)` | Rust `split_at` | Add | R-L0-42 |
| `dedup` |  (Add)`bytes.dedup(b: Bytes) -> Bytes` | Rust `dedup` | Add | R-L0-42 |
| `intersperse` |  (Add)`bytes.intersperse(b: Bytes, sep: Int) -> Bytes` | Rust `intersperse` | Add | R-L0-42 |
| `unique` |  (Add)`bytes.unique(b: Bytes) -> Bytes` | Kotlin `distinct` | Add | R-L0-42 |
| `unique_by` |  (Add)`bytes.unique_by(b: Bytes, f: (Int) -> K) -> Bytes` | Kotlin `distinctBy` | Add | R-L0-42 |
| `last_index_of` |  (Add)`bytes.last_index_of(b: Bytes, needle: Bytes) -> Int?` | Go `bytes.LastIndex` / Python `rfind` | Add | R-bytes-27 |
| `replace` |  (Add)`bytes.replace(b: Bytes, from: Bytes, to: Bytes) -> Bytes` | Go `bytes.ReplaceAll` / Python `replace` | Add | R-bytes-27 |
| `replace_first` |  (Add)`bytes.replace_first(b: Bytes, from: Bytes, to: Bytes) -> Bytes` | Go `bytes.Replace(…, 1)` / Python `replace(…, 1)` | Add | R-bytes-27 |
| `strip_prefix` |  (Add)`bytes.strip_prefix(b: Bytes, prefix: Bytes) -> Bytes?` | Rust `strip_prefix` / Go `CutPrefix` | Add | R-bytes-27 |
| `strip_suffix` |  (Add)`bytes.strip_suffix(b: Bytes, suffix: Bytes) -> Bytes?` | Rust `strip_suffix` / Go `CutSuffix` | Add | R-bytes-27 |
| `count_matches` |  (Add)`bytes.count_matches(b: Bytes, needle: Bytes) -> Int` | Go `bytes.Count` / Python `count` | Add | R-bytes-27 |
| `new` |  (Add)`bytes.new() -> Bytes` | Rust `Vec::new` / Swift `Data()` / Go `[]byte{}` | Add | R-bytes-28 |
| `is_equal_constant_time` | `bytes.is_equal_constant_time(a: Bytes, b: Bytes) -> Bool` (Proposal) | Go `subtle.ConstantTimeCompare` / Python `hmac.compare_digest` / Node `crypto.timingSafeEqual` | Add | R-bytes-29 |

<a id="l0-への差し戻し処理済み"></a>

## Referrals back to L0 (resolved)

1. **R-L0-01 and matrix cells.** R-L0-01 now says type-matrix cells are not synonyms even with identical runtime bodies, provided the matrix is declared and gated. Retain both `append_i16_le` and `append_u16_le`. #3851 F synonym gate excludes cells generated from the matrix declaration (R-bytes-03), not enumerated names.
2. **Out-of-range positional writes.** L0 now states positional writes abort out of range as contract violations (after R-L0-12). R-bytes-11 aligns `put_<型>`, `set`, `insert`, `remove_at` to abort.
