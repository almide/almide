> Original description, inactive: Stdlib redesign L2/L3 — ffi (new, #3853): the five raw-address functions leave the always-visible bytes module for an explicitly imported ffi with one address op that is null on empty on both targets.
<a id="l2l3-ffi新しいモジュール"></a>

# L2/L3: ffi (new module)

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](L2-ffi.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](L2-ffi.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](L2-ffi.zh-CN.md)

> Non-normative source-pinned reader snapshot; the unchanged original remains authoritative. No automatic synchronization, independent execution, measurement or safety verification. The October 27 addendum supersedes the October 21 RawPtr proposals; the old signatures, accepted labels, pure/effect classifications, argument order and epoch-16 migration plan below are historical, not current API instructions. The nine-language comparisons and measured divergence are the source’s dated evidence, not newly checked here. Explicit import alone is not a memory-safety or permission boundary: the restrictive permission design was excluded from that proposal. The Region/Handle addendum is retained without asserting whole-artifact memory safety or trusted-extern correctness. Code comments and placeholders remain verbatim. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/stdlib-redesign/L2-ffi.md)

> Last updated: 2026-10-21
> Prerequisites: [L0](../../../../../roadmap/stdlib-redesign/L0-conventions.md) and [L1](../../../../../roadmap/stdlib-redesign/L1-categories.md) (R-L1-26: move raw pointers to `ffi`, #3853)
> Sources: Rust https://doc.rust-lang.org/std/primitive.slice.html#method.as_ptr and https://doc.rust-lang.org/std/slice/fn.from_raw_parts.html; Go https://pkg.go.dev/unsafe; Python https://docs.python.org/3/library/ctypes.html; TS/JS https://nodejs.org/api/n-api.html (on the JS side, `WebAssembly.Memory.buffer`); Gleam — (none; external functions use `@external`); Elixir — (NIFs are on the C side); OCaml https://ocaml.org/manual/latest/intfc.html; Kotlin https://kotlinlang.org/api/core/kotlin-stdlib/kotlinx.cinterop/ (Native); Swift https://developer.apple.com/documentation/swift/unsafemutablerawpointer

> **2026-10-27 addendum**: The `RawPtr` surface described here (`ffi.address`, `ffi.to_int`, address-and-length `ffi.read` / `ffi.write`, and old spellings such as `bytes.as_ptr`) was superseded by [ADR-0031](../../../../../adr/0031-raw-pointers-carry-provenance.md) (Accepted 2026-10-27, dialect epoch 15). The current surface: `RawPtr` is renamed to the opaque external token `Handle` (stdlib functions neither accept nor return it). Reads and writes go through a length-bearing `Region`: `ffi.region(b)` (read-only), `ffi.alloc(n)` (effect fn, writable, zero-filled), `ffi.len`, `ffi.is_writable`, `ffi.read(r, off, n)`, `ffi.write(r, off, b)`, and `ffi.to_bytes(r)`. Out-of-bounds access returns `err`. `ffi.address`, `ffi.to_int`, `bytes.as_ptr`, `as_mut_ptr`, `data_ptr`, `from_raw_ptr`, and `copy_to_ptr` are removed. A `Region` argument to `@extern(c)` crosses as two arguments, (ptr, size_t len); returning a `Region` requires `@region(len = …, release = "…")` (E090 if absent). The following remains as a record of 2026-10-21.

<a id="l2-役割の比較"></a>

## L2: Role comparison

| Role | Almide (current → proposal) | Rust | Go | Python | TS/JS | Gleam | Elixir | OCaml | Kotlin | Swift |
|---|---|---|---|---|---|---|---|---|---|---|
| Address of the first byte | `bytes.as_ptr` / `as_mut_ptr` (null when empty) / `data_ptr` (`Int`, non-null even when empty) → `ffi.address` | `as_ptr` / `as_mut_ptr` | `unsafe.SliceData` | `ctypes.addressof` | offset in `Memory.buffer` | — | — | `Bigarray` | `addressOf` / `refTo` | `withUnsafeBytes` |
| Address as an integer | `bytes.data_ptr` → `ffi.to_int` | `as usize` | `uintptr` | `ctypes.cast(…, c_void_p).value` | — | — | — | — | `toLong` | `Int(bitPattern:)` |
| Read from an address (copy) | `bytes.from_raw_ptr` → `ffi.read` | `slice::from_raw_parts` | `unsafe.Slice` | `ctypes.string_at` | `new Uint8Array(buf, off, len)` | — | — | — | `readBytes` | `UnsafeRawBufferPointer` |
| Write to an address | `bytes.copy_to_ptr` → `ffi.write` | `ptr::copy_nonoverlapping` | `copy(unsafe.Slice…)` | `ctypes.memmove` | `Uint8Array.set` | — | — | — | `memcpy` | `copyMemory` |

<a id="完全性の規則"></a>

### Completeness rules

- The role set is closed at 4: {get an address, turn an address into an integer, read from an address, write to an address}.
- There is only 1 address-taking function; it returns null for an empty byte sequence (the same answer on native and wasm). There are not 2 spellings for reading and writing (native `as_ptr` also returned `*mut u8`; the distinction was only in the name, #3853).
- `import ffi` is required. `bytes` is visible in every file without an import, so it must not contain memory-unsafe functions.
- Address-taking and integer-conversion functions are pure (the value changes, but they do not read the outside world). They are checked by the “round-trip” verdict of ADR-0027 D3.

<a id="l2-の判定"></a>

### L2 assessment

- Duplication: 3 address-returning spellings (`as_ptr`, `as_mut_ptr`, `data_ptr`) give different answers when empty → R-ffi-01.
- Wrong location: 5 functions are in the automatically visible `bytes` → R-ffi-01.

<a id="案"></a>

## Proposals

<a id="r-ffi-01-ffi-を作りbytes-の生ポインタ-5-関数を移す---採用"></a>

### R-ffi-01 Create `ffi` and move the 5 raw-pointer functions from `bytes` — ○ Accepted

- Content:
  ```almide
  fn address(b: Bytes) -> RawPtr                     // 先頭の番地。空なら null
  fn to_int(p: RawPtr) -> Int                        // 番地の整数（JS host に渡す用。null は 0）
  fn read(ptr: RawPtr, len: Int) -> Bytes            // len バイトのコピー。null か len <= 0 なら空
  fn write(b: Bytes, ptr: RawPtr, cap: Int) -> Int   // 最大 cap バイトを書き、書いた数。null なら 0
  ```
  Rewrites: `bytes.as_ptr(b)` / `bytes.as_mut_ptr(b)` → `ffi.address(b)`; `bytes.data_ptr(b)` → `ffi.to_int(ffi.address(b))`; `bytes.from_raw_ptr(p, n)` → `ffi.read(p, n)`; `bytes.copy_to_ptr(b, p, c)` → `ffi.write(b, p, c)`.
- Semantic difference: `data_ptr` returned `handle + 12` even for an empty byte sequence. `ffi.to_int(ffi.address(b))` is 0. Only a length-0 read occurs, so there is no difference on the JS host path (#907, #3420 lifetime rule `spec/wasm_host_js/extern_data_ptr_lifetime.almd`). This change occurs only on the new-name side (R-L0-14 item 4).
- Spelling: `to_int` follows R-L0-07 (`to_<先>` in the source type's module). `RawPtr` belongs to `ffi` (R-L0-02).
- Permissions restricting which files may call these functions (a proposal to reject `ffi.*` from files without `@extern`, in the same shape as `prim` E085) are not part of this proposal. The `[permissions]` key design is to be decided together with `proc` (ADR-0025), tracked in #3853. This proposal stands without permissions (just requiring `import ffi` removes it from completion candidates in ordinary code).
- Supporting L0 rules: R-L0-01 (one address spelling), R-L0-02, R-L0-07, R-L1-26.
- (a) In all 9 languages, raw addresses live in explicitly named areas such as `unsafe`, `ctypes`, or `cinterop`. Only Rust places them alongside the ordinary byte-sequence type (`as_ptr` is safe; the use is `unsafe`).
- (b) native: `*mut u8`. wasm: a linear-memory address (#440 / C-062, `stdlib/bytes_rawptr.almd`). Both use the same body and align the empty-case answer. No host facility is needed.
- (c) LLMs will no longer see `bytes.as_mut_ptr` among completion candidates in ordinary code.
- (d) Addition and removal on the `bytes` side (same assessment in L2-bytes). `@deprecated(since=15, use="ffi.…")`, `removal_epoch = 16`. `almide fix` rewrites one-to-one (also adding `import ffi`). Usage counts (spec/tests/docs, #3853): `as_ptr` 10/1/7, `as_mut_ptr` 9/0/5, `data_ptr` 12/0/10, `from_raw_ptr` 6/0/4, `copy_to_ptr` 11/0/5.
- Proposed gate: no public function in `bytes` has `RawPtr` as an argument or return value. A cross fixture compares the empty-case result of `ffi.address` between native and wasm.
- issue: #3853 (existing)

<a id="r-ffi-02-ffiread-と-ffiwrite-を-effect-fn-にする---採用"></a>

### R-ffi-02 Make `ffi.read` and `ffi.write` effect fns — ○ Accepted

- Current: `fn ffi.read(ptr: RawPtr, len: Int) -> Bytes` and `fn ffi.write(b: Bytes, ptr: RawPtr, cap: Int) -> Int` are pure fns.
- Proposal: `effect fn ffi.read(…)`, `effect fn ffi.write(…)`. `address` and `to_int` stay pure.
- Supporting L0 rules: R-L0-13 (functions reading the environment are effectful), ADR-0022 (pure fns may only write output and abort). `write` modifies memory outside Almide values; `read` reads memory written by external code.
- (a) In all 9 languages, raw-address reads/writes are in areas distinguished from ordinary functions, such as `unsafe` or `ctypes`.
- (b) Measurement shows divergence between the two targets. Writing to the address of a `let`-bound `Bytes` inside a pure fn changes the caller's immutable value on native but not on wasm (reproduced with the current `bytes.as_mut_ptr` and `copy_to_ptr`; #3966, to be fixed separately as a release-blocking defect). Treating it as a pure fn allows optimizations to change it further.
- (d) Only the new functions' declarations change. Pure fns using the migration-source `bytes.from_raw_ptr` / `copy_to_ptr` get E006 after rewriting, so the hint says “make the caller an effect fn”.
- Completeness rule: functions reading/writing state outside Almide values are effectful. In `ffi`, {address, to_int} are pure and {read, write} are effectful.
- issue: #3974

<a id="r-ffi-03-ffiwrite-の引数を書き込む先が先にする---採用"></a>

### R-ffi-03 Put the destination first in `ffi.write` arguments — ○ Accepted

- Current (proposal stage): `fn ffi.write(b: Bytes, ptr: RawPtr, cap: Int) -> Int`. Other standard write operations put the destination first (`fs.write_bytes(path, data)`, `net.tcp_write(stream, data)`, `unix.write(sock, data, fds)`, `bytes.put_<型>(mut b, pos, v)`).
- Proposal: `fn ffi.write(ptr: RawPtr, b: Bytes, cap: Int) -> Int`. This also aligns the leading pointer with `ffi.read(ptr, len)`.
- Supporting L0 rule: R-L0-03 (subject argument first).
- (d) Only changes the shape of a function not yet released. The `almide fix` rewrite from `bytes.copy_to_ptr(b, p, n)` swaps argument order.
- issue: #3979

<a id="l3-関数表"></a>

## L3: Function table

| Function | Signature (proposal) | Main spellings in other languages | Assessment | Proposal |
|---|---|---|---|---|
| `ffi.address` | `fn address(b: Bytes) -> RawPtr` | Rust `as_ptr` / Go `unsafe.SliceData` / Python `addressof` | Add (from `bytes.as_ptr` / `as_mut_ptr`) | R-ffi-01 |
| `ffi.to_int` | `fn to_int(p: RawPtr) -> Int` | Go `uintptr` / Swift `Int(bitPattern:)` | Add (role of `bytes.data_ptr`) | R-ffi-01 |
| `ffi.read` | `effect fn read(ptr: RawPtr, len: Int) -> Bytes` | Rust `from_raw_parts` / Python `string_at` / Go `unsafe.Slice` | Add (from `bytes.from_raw_ptr`) | R-ffi-01 |
| `ffi.write` | `effect fn write(ptr: RawPtr, b: Bytes, cap: Int) -> Int` | Rust `copy_nonoverlapping` / Python `memmove` | Add (from `bytes.copy_to_ptr`; destination first) | R-ffi-01, R-ffi-03 |

> Reading translation of the retained historical code comments: address returns the first address, null when empty. to_int returns the address as an integer for a JS host, with null becoming 0. read copies len bytes and returns empty when null or len <= 0. write writes at most cap bytes and returns the number written, or 0 for null. The retained Japanese placeholders mean “destination” in to_<…> and “type” in bytes.put_<…>. These are explanations of the superseded proposal, not current API guarantees.
