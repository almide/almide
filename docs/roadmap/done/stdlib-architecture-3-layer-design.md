<!-- description: Three-layer stdlib design (core/platform/external) for WASM parity -->
<!-- done: 2026-03-18 -->
# Stdlib Architecture: 3-Layer Design

Separate Almide's stdlib into 3 layers. Treat WASM as a first-class citizen and clearly separate pure computation from OS dependencies.

Languages referenced:
- **MoonBit**: core (pure) / x (platform) の 2 層。WASM-first。JSON は core に含む
- **Gleam**: stdlib (target-independent) / gleam_erlang / gleam_javascript の分離
- **Rust**: core / alloc / std の 3 層。WASM で使えない関数はコンパイルエラー
- **Zig**: comptime でターゲット判定。未使用コード自動削除

### Layer 1: core (all targets, WASM OK)

Available via auto-import or `import xxx`. Pure computation only. No OS dependencies.

| Module | Status | Notes |
|--------|--------|-------|
| `string` | ✅ runtime (`core_runtime.txt`) | String operations (30 functions) |
| `list` | ✅ runtime (`collection_runtime.txt`) | List operations, HOF (all functions including lambda) |
| `int` | ✅ runtime (`core_runtime.txt`) | Numeric conversion, bitwise operations (22 functions) |
| `float` | ✅ runtime (`core_runtime.txt`) | Numeric conversion (9 functions) |
| `map` | ✅ runtime (`collection_runtime.txt`) | HashMap (all functions including lambda) |
| `math` | ✅ runtime (`core_runtime.txt`) | Math functions (12 functions) |
| `json` | ✅ runtime (`json_runtime.txt`) | Parse/serialize. Common language for WASM interop |
| `regex` | ✅ runtime (`regex_runtime.txt`) | Regular expressions |
| `path` | bundled .almd | Path operations (pure string processing) |
| `time` | ✅ runtime (`time_runtime.txt`) | Date decomposition (year/month/day etc. now/sleep are platform) |
| `args` | bundled .almd | Argument parsing (env.args() injected via platform) |
| `encoding` | bundled .almd | base64, hex, url_encode/decode |

### Layer 2: platform (native only)

Explicitly import via `import platform.fs` etc. Importing on WASM target causes a **compile error**.

| Module | Status | Notes |
|--------|--------|-------|
| `fs` | ✅ runtime (`platform_runtime.txt`) | File I/O (14 functions) |
| `process` | ✅ runtime (`platform_runtime.txt`) | External command execution (4 functions) |
| `io` | ✅ runtime (`platform_runtime.txt`) | stdin/stdout (3 functions) |
| `env` | ✅ runtime (`platform_runtime.txt`) | Environment variables, args, unix_timestamp, millis, sleep_ms (7 functions) |
| `http` | ✅ runtime (`http_runtime.txt`) | HTTP server/client |
| `random` | ✅ runtime (`platform_runtime.txt`) | OS entropy-based random (4 functions) |

### Layer 3: x (Official Extension Packages)

Used by adding dependencies in `almide.toml`. Officially maintained but versioned independently from stdlib.

| Package | Status | Notes |
|---------|--------|-------|
| `encoding` | ✅ implemented (bundled .almd) -> planned separation | hex, base64, url_encode/decode |
| `hash` | ✅ implemented (bundled .almd) | SHA-256, SHA-1, MD5 — pure Almide |
| `crypto` | planned | encryption |
| `csv` | planned (external package) | CSV parse/stringify -- `almide/csv` |
| `term` | ✅ implemented (bundled .almd) | ANSI colors, terminal formatting |

### Playground Stdlib Support

Enable bundled .almd modules in Playground (WASM).

- Retrieve via `stdlib::get_bundled_source()` -> parse -> pass to `emit_with_modules` modules argument
- Only browser-compatible ones (csv, encoding, hash, path) are bundle targets
- args depends on `env.args()` and is not available; time has unsupported `env.unix_timestamp()` etc.; term is meaningless in browser
- Revisit after Phase B platform namespace introduction (if bundled modules importing platform dependencies cause compile errors, all modules can be safely bundled)

### Implementation Steps

#### Phase A: WASM Compile Error ✅
- [x] checker: detect platform module imports on WASM target and error
- [x] mechanism to pass target information to checker when `--target wasm`

#### Phase B: Introduce platform namespace
- [ ] Design `import platform.fs` syntax
- [ ] Migration path from existing `import fs` (deprecation warning -> error)
- [ ] Implement platform module resolver

#### Phase C: Separate x packages
- [ ] Separate encoding into `almide/encoding` repository
- [ ] Make available via package manager
- [ ] Create hash, csv, term as new x packages

### Extern / FFI Design ✅ (implemented in v0.2.1)

Gleam の `@external` パターンを参考に、Almide 版の extern を実装。

**Design decisions:**
- Syntax: `@extern(target, "module", "function")` attribute — target は `rs`/`ts`
- Specification: module + function name (not file paths)
- Type mapping: trust-based (compiler trusts the declared signature)
- Body = fallback: if a body exists, it's used for targets without `@extern`
- Completeness check: if no body and a target is missing `@extern`, compile error

**Reference languages:** Gleam (`@external` + body fallback), Kotlin (`expect`/`actual` exhaustiveness), Zig (rejected: inline foreign code pollutes source), Roc (rejected: platform-level separation is overkill), Dart (rejected: file-level granularity too coarse)

**Implementation:**
- Parser: `@extern` collection before `fn` declarations (`src/parser/declarations.rs`)
- Checker: completeness validation — body-less functions require both `rs` and `ts` `@extern` (`src/check/mod.rs`)
- Rust emitter: `@extern(rs, ...)` emits `module::function(args)` delegation (`src/emit_rust/program.rs`)
- TS emitter: `@extern(ts, ...)` emits `module.function(args)` delegation (`src/emit_ts/declarations.rs`)
- Formatter: preserves `@extern` annotations (`src/fmt.rs`)
- Test: `exercises/extern-test/extern_test.almd`

#### Usage patterns

```almide
// Pattern 1: Pure Almide (no extern needed, both targets use this)
fn add(a: Int, b: Int) -> Int = a + b

// Pattern 2: Override one target, body is fallback for the other
@extern(rs, "std::cmp", "min")
fn my_min(a: Int, b: Int) -> Int = if a < b then a else b
// Rust uses std::cmp::min, TS uses the Almide body

// Pattern 3: Both targets extern (no body = both required)
@extern(rs, "std::cmp", "max")
@extern(ts, "Math", "max")
fn my_max(a: Int, b: Int) -> Int
// Missing either @extern is a compile error
```

#### Type mapping (trust-based)

Primitive type correspondence is well-defined:

| Almide | Rust | TypeScript |
|--------|------|------------|
| `Int` | `i64` | `number` |
| `Float` | `f64` | `number` |
| `String` | `String` | `string` |
| `Bool` | `bool` | `boolean` |
| `Unit` | `()` | `void` |
| `List[T]` | `Vec<T>` | `T[]` |
| `Map[K, V]` | `HashMap<K, V>` | `Map<K, V>` |
| `Option[T]` | `Option<T>` | `T \| null` |
| `Result[T, E]` | `Result<T, E>` | `T` (throw on err) |

The compiler trusts that the extern function matches the declared Almide signature. Type mismatches are the user's responsibility (runtime errors, not compile errors). Future phases may add automatic marshalling or verified extern annotations.

#### Stdlib runtime extraction (completed in v0.2.1)

All stdlib functions have been extracted from inline codegen to separated runtime files:

```
Phase 1: ✅ @extern syntax in parser, checker, emitters
Phase 2: ✅ Extract platform modules (fs, process, io, env, random) → platform_runtime.txt
Phase 3: ✅ Extract core modules (string, int, float, math) → core_runtime.txt
         ✅ Extract collection modules (list, map, including lambda-based) → collection_runtime.txt
Phase 4: Remove calls.rs dispatch entirely (calls.rs becomes pure @extern routing)
```

**Rust runtime files:**
| File | Modules | Functions |
|------|---------|-----------|
| `platform_runtime.txt` | fs, env, process, io, random | 32 |
| `core_runtime.txt` | string, int, float, math | 73 |
| `collection_runtime.txt` | list, map (including lambda-based) | 46 |
| `json_runtime.txt` | json | (pre-existing) |
| `http_runtime.txt` | http | (pre-existing) |
| `regex_runtime.txt` | regex | (pre-existing) |
| `time_runtime.txt` | time | (pre-existing) |

**TS runtime:** All modules use `__almd_<module>` namespaced objects in `emit_ts_runtime.rs`.

`calls.rs` now contains only dispatch logic (`almide_rt_*` function calls), no inline Rust code generation. Adding a new stdlib function requires zero compiler codegen changes — just the runtime function and a dispatch entry.

---
