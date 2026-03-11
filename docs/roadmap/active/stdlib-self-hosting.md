# Stdlib Self-Hosting [IN PROGRESS]

As of v0.2.1, all stdlib functions have been extracted from inline codegen to separated runtime files (see [Stdlib runtime extraction](#stdlib-runtime-extraction-completed-in-v021)). Type signatures remain in `stdlib.rs` and dispatch logic in `calls.rs`. The next goal: **Almide writes its own stdlib in Almide**, achieving automatic multi-target support with zero compiler changes.

### Why self-hosting matters

```
extern "rust" で書く → Rustでしか動かない
Almideで書く         → Rust/TS 両方に自動出力される
```

Almideの設計原則は「同じコードが複数ターゲットに出力される」こと。stdlibもこの原則に従うべき。`extern` は最終手段であり、主戦略は **Almideの表現力を上げてstdlibをAlmideで書く**。

### Architecture: Two-Layer Stdlib

```
┌──────────────────────────────────────────────┐
│  Upper layer: Almide stdlib packages          │  ← .almd files, written in Almide
│  path.join, time.year, args.parse,            │     runs on both Rust/TS targets
│  hash.sha256, encoding.base64, csv.parse ...  │
└──────────────┬───────────────────────────────┘
               │ calls
┌──────────────▼───────────────────────────────┐
│  Lower layer: runtime functions               │  ← *_runtime.txt files + calls.rs dispatch
│  fs.read_text, process.exec, string.len,      │     OS syscalls, data structure internals
│  list.get, map.set, int.to_string ...         │     TS: __almd_<module> objects
└──────────────────────────────────────────────┘
```

### Phase 0: Language Primitives for Self-Hosting

Before Almide can write its own stdlib, the language needs low-level primitives. These are not stdlib functions — they are **language-level operators and types**.

#### 0a. Bitwise Operators

Required for: hash algorithms (SHA-1, SHA-256, MD5), encoding (base64, hex), compression, binary protocols.

| Operator | Name | Rust emit | TS emit | Notes |
|----------|------|-----------|---------|-------|
| `band(a, b)` | bitwise AND | `({} & {})` | `({} & {})` | |
| `bor(a, b)` | bitwise OR | `({} \| {})` | `({} \| {})` | |
| `bxor(a, b)` | bitwise XOR | `({} ^ {})` | `({} ^ {})` | `^` is already used for pow/xor contextually |
| `bshl(a, n)` | shift left | `({} << {})` | `({} << {})` | |
| `bshr(a, n)` | shift right | `({} >> {})` | `({} >>> {})` | unsigned shift in TS |
| `bnot(a)` | bitwise NOT | `(!{})` | `(~{})` | |

**Design choice**: Use named functions (`band`, `bor`, `bxor`) rather than symbolic operators (`&`, `|`, `^`). Rationale:
- `&` conflicts with potential reference syntax
- `|` is used for variant types and lambdas
- `^` is already used for power/XOR (contextual)
- Named functions are explicit, unambiguous, readable
- Most Almide code never needs bitwise ops — they shouldn't pollute the operator space

Implementation:
- [x] stdlib.rs: add `int.band`, `int.bor`, `int.bxor`, `int.bshl`, `int.bshr`, `int.bnot` signatures
- [x] emit_rust/calls.rs: emit corresponding Rust operators
- [x] emit_ts_runtime.rs: emit corresponding JS operators (note: `>>>` for unsigned shift)
- [x] Test: verify all operators with known values

#### 0b. Wrapping Arithmetic ✅

Required for: hash algorithms that operate on 32-bit unsigned integers with overflow wrapping.

```almide
int.wrap_add(a, b, bits)    // (a + b) mod 2^bits
int.wrap_mul(a, b, bits)    // (a * b) mod 2^bits
int.rotate_right(a, n, bits) // circular right rotation
int.rotate_left(a, n, bits)  // circular left rotation
int.to_u32(a)               // truncate to 0..2^32-1
int.to_u8(a)                // truncate to 0..255
```

Implementation:
- [x] stdlib.rs: add wrapping arithmetic signatures to `int` module
- [x] emit_rust/calls.rs: use Rust wrapping operations with bitmask
- [x] emit_ts_runtime.rs: use `Math.imul()`, manual rotation, `>>> 0` for u32
- [x] Test: SHA-256 style round operations (sigma0, Ch, Maj)

#### 0c. Byte Array Type (future consideration)

Currently bytes are `List[Int]` which is `Vec<i64>` — 8x memory overhead. For serious binary processing, a dedicated `Bytes` type may be needed. But `List[Int]` works for correctness and can be optimized later.

### Phase 1: Stdlib Package Mechanism ✅

Allow stdlib modules to be implemented as `.almd` files that ship with the compiler. No language changes needed — uses existing module system.

```
almide/
  stdlib/
    args.almd          ← argument parsing (pure Almide) ✅ implemented
    term.almd          ← terminal colors (pure Almide) ✅
    hash.almd          ← SHA-256, SHA-1, MD5 (pure Almide, uses bitwise ops) ✅
    encoding.almd      ← base64, hex, url_encode/decode (pure Almide, uses bitwise ops) ✅
```

#### Implementation Steps

- [x] Resolver: bundled stdlib via `include_str!` in compiler binary
- [x] Resolver: fallback to bundled source when module not found locally
- [x] stdlib.rs: `get_bundled_source()` returns embedded `.almd` source
- [x] Test: `args` module entirely in Almide as proof of concept
- [x] Bug fix: sanitize `?` in user module function names and calls (`flag?` → `flag_qm_`)

#### Proof of concept: `args` module

```almide
// stdlib/args.almd — argument parsing, pure Almide

fn flag?(name: String) -> Bool = {
  let args = env.args()
  list.any(args, fn(a) => a == "--" ++ name || a == "-" ++ string.slice(name, 0, 1))
}

fn option(name: String) -> Option[String] = {
  let args = env.args()
  let long = "--" ++ name
  let eq_match = list.find(args, fn(a) => string.starts_with?(a, long ++ "="))
  match eq_match {
    some(a) => string.strip_prefix(a, long ++ "=")
    none => {
      let idx = list.index_of(args, long)
      match idx {
        some(i) => list.get(args, i + 1)
        none => none
      }
    }
  }
}

fn option_or(name: String, default: String) -> String =
  match option(name) {
    some(v) => v
    none => default
  }

fn positional() -> List[String] =
  list.filter(env.args(), fn(a) => not string.starts_with?(a, "-"))
    |> list.drop(1)
```

#### hash module (pure Almide, after Phase 0)

```almide
// stdlib/hash.almd — SHA-256 in pure Almide

fn sha256(data: String) -> String = {
  let bytes = string.to_bytes(data)
  let padded = pad_message(bytes)
  // ... rounds using int.wrap_add, int.rotate_right, int.bxor etc.
  // ... outputs hex string
  // Runs on both Rust and TS targets — no extern needed
}
```

### Phase 2: Migrate & Extend Stdlib via .almd

方針: **既存のstring/list/map等はRustのまま残す**（既にRust/TS両方で動いており、HOFはラムダインライン最適化がある）。移行は **丸ごと置き換えられるモジュール** と **新規追加** に集中する。

#### 2a. path モジュール ✅ 完了

全5関数を `stdlib/path.almd` に移行。コンパイラの `STDLIB_MODULES` から除外済み。

| Function | Almide implementation |
|----------|----------------------|
| `join` | `++` with `/` separator, absolute child replaces |
| `dirname` | `split("/")` → take all but last → `join("/")` |
| `basename` | `split("/")` → last non-empty part |
| `extension` | `split(".")` on basename → last part |
| `is_absolute?` | `starts_with?(p, "/")` |

#### 2b. time モジュール ✅ 完了

全12関数を `stdlib/time.almd` に完全移行。`STDLIB_MODULES` から除外済み。
`now/millis/sleep` は `env.unix_timestamp/env.millis/env.sleep_ms` プリミティブのラッパー。
残り9関数（year/month/day/hour/minute/second/weekday/to_iso/from_parts）は純粋なAlmide実装（Hinnant日付算術）。

| Function | Almide implementation |
|----------|----------------------|
| `hour` | `(ts % 86400) / 3600` |
| `minute` | `(ts % 3600) / 60` |
| `second` | `ts % 60` |
| `weekday` | `(ts / 86400 + 4) % 7` |
| `year` | UNIX timestamp → date arithmetic (leap year calc) |
| `month` | same |
| `day` | same |
| `to_iso` | decompose + string formatting |
| `from_parts` | reverse date arithmetic |

#### 2c. 新規モジュール（コンパイラ変更ゼロで追加）

| Module | Functions | Needs bitwise? | Priority |
|--------|-----------|---------------|----------|
| `hash` | `sha256`, `sha1`, `md5` | Yes | ✅ Done |
| `encoding` | `base64_encode/decode`, `hex_encode/decode`, `url_encode/decode` | Yes | ✅ Done |
| `term` | `color`, `bold`, `dim`, `reset`, `strip` | No | ✅ Done |
| `csv` | `parse`, `parse_with_header`, `stringify` | No | Planned (external package) |

#### 2d. Phase 6 の新規関数を .almd で追加

Phase 6 で追加予定の派生関数は、コンパイラに追加せず `.almd` で実装する。ただし既存のハードコードモジュール (string/list/map) に関数を追加するには **ハイブリッドresolver** が必要（ハードコード + bundled .almd のマージ）。

候補:
- `list.filter_map`, `list.group_by`, `list.take_while`, `list.drop_while`
- `list.count`, `list.partition`, `list.reduce`
- `map.map_values`, `map.filter`, `map.from_entries`
- `string.replace_first`, `string.last_index_of`, `string.to_float`

#### Strategy summary

| Category | Approach |
|----------|----------|
| **Core modules** (string/list/map/int/float/math) | ✅ Extracted to runtime files (`core_runtime.txt`, `collection_runtime.txt`). Both targets supported |
| **Platform modules** (fs/process/io/env/random) | ✅ Extracted to `platform_runtime.txt`. OS-dependent |
| **Existing runtime modules** (json/http/regex/time) | ✅ Already in separate runtime files |
| **path** | ✅ Migrated to `.almd` |
| **time decomposition** | ✅ Migrated to `.almd` (now/millis/sleep remain via env primitives) |
| **New modules** | Create as `.almd` files (zero compiler changes) |
| **New functions for existing modules** | Add runtime function + dispatch entry in `calls.rs` |

### Phase 3: `@extern` FFI ✅ Implemented (v0.2.1)

`@extern(target, "module", "function")` provides target-specific implementation references. See [Extern / FFI Design](#extern--ffi-design--implemented-in-v021) for details.

Use cases:
- Performance-critical code where pure Almide is too slow
- Platform-specific APIs (WASM, native GUI, etc.)
- Wrapping existing ecosystem libraries

### Priority Order

| Phase | What | Status | Enables |
|-------|------|--------|---------|
| **0a.** Bitwise operators | `int.band/bor/bxor/bshl/bshr/bnot` | ✅ Done | hash, encoding, binary protocols |
| **0b.** Wrapping arithmetic | `int.wrap_add/wrap_mul/rotate_right/left` | ✅ Done | SHA-256, SHA-1 in pure Almide |
| **1.** Stdlib package mechanism | resolver + bundled .almd | ✅ Done | args, term, csv, hash, encoding |
| **2a.** Runtime extraction | all stdlib → runtime files | ✅ Done (v0.2.1) | clean codegen separation |
| **2b.** Migrate more stdlib to .almd | move pure functions to .almd | Next | shrinks calls.rs further |
| **3.** `@extern` FFI | target-specific escape hatch | ✅ Done (v0.2.1) | platform-specific APIs |

### CLI Stdlib Gaps (to be filled via self-hosting)

#### Via Almide stdlib packages (after Phase 0 + Phase 1)

| Module | Functions | Needs bitwise? | Priority |
|--------|-----------|---------------|----------|
| `args` | `flag?`, `option`, `option_or`, `positional`, `positional_at` | No | ✅ Done |
| `hash` | `sha256`, `sha1`, `md5` | Yes | ✅ Done |
| `encoding` | `base64_encode/decode`, `hex_encode/decode`, `url_encode/decode` | Yes | ✅ Done |
| `term` | `red/green/blue/...`, `bold`, `dim`, `color(256)`, `strip` | No | ✅ Done |
| `csv` | `parse`, `parse_with_header`, `stringify` | No | Planned (external) |

#### Via runtime additions (runtime file + dispatch entry in calls.rs — both targets)

| Module | Functions | Priority |
|--------|-----------|----------|
| `float` | `to_fixed(n, decimals)` | ✅ Done (v0.4.8) |
| `fs` | `walk` ✅, `remove_all` ✅, `file_size` ✅, `temp_dir` ✅, `glob` (deferred) | ✅ Done |
| `process` | `exec_in(dir, cmd, args)` ✅, `exec_with_stdin` ✅ | ✅ Done |
| `time` | `format(ts, fmt)`, `parse(s, fmt)` | Planned (.almd) |
| `http` | `get_with_headers` ✅, `request(method, url, body, headers)` ✅ | ✅ Done |

---
