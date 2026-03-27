<!-- description: Three-layer stdlib design (core/platform/external) for WASM parity -->
# Stdlib Architecture: 3-Layer Design

Almide の stdlib を 3 層に分離する。WASM を一級市民として扱い、pure な計算と OS 依存を明確に分ける。

参考にした言語:
- **MoonBit**: core (pure) / x (platform) の 2 層。WASM-first。JSON は core に含む
- **Gleam**: stdlib (target-independent) / gleam_erlang / gleam_javascript の分離
- **Rust**: core / alloc / std の 3 層。WASM で使えない関数はコンパイルエラー
- **Zig**: comptime でターゲット判定。未使用コード自動削除

### Layer 1: core（全ターゲット、WASM OK）

auto-import または `import xxx` で使える。pure な計算のみ。OS 依存なし。

| Module | Status | Notes |
|--------|--------|-------|
| `string` | ✅ runtime (`core_runtime.txt`) | 文字列操作 (30 functions) |
| `list` | ✅ runtime (`collection_runtime.txt`) | リスト操作、HOF (lambda含む全関数) |
| `int` | ✅ runtime (`core_runtime.txt`) | 数値変換、ビット演算 (22 functions) |
| `float` | ✅ runtime (`core_runtime.txt`) | 数値変換 (9 functions) |
| `map` | ✅ runtime (`collection_runtime.txt`) | ハッシュマップ (lambda含む全関数) |
| `math` | ✅ runtime (`core_runtime.txt`) | 数学関数 (12 functions) |
| `json` | ✅ runtime (`json_runtime.txt`) | パース・シリアライズ。WASM interop の共通言語 |
| `regex` | ✅ runtime (`regex_runtime.txt`) | 正規表現 |
| `path` | bundled .almd | パス操作（pure 文字列処理） |
| `time` | ✅ runtime (`time_runtime.txt`) | 日付分解（year/month/day 等。now/sleep は platform） |
| `args` | bundled .almd | 引数パース（env.args() は platform 経由で注入） |
| `encoding` | bundled .almd | base64, hex, url_encode/decode |

### Layer 2: platform（native only）

`import platform.fs` 等で明示的に import する。WASM ターゲットで import すると**コンパイルエラー**。

| Module | Status | Notes |
|--------|--------|-------|
| `fs` | ✅ runtime (`platform_runtime.txt`) | ファイル I/O (14 functions) |
| `process` | ✅ runtime (`platform_runtime.txt`) | 外部コマンド実行 (4 functions) |
| `io` | ✅ runtime (`platform_runtime.txt`) | stdin/stdout (3 functions) |
| `env` | ✅ runtime (`platform_runtime.txt`) | 環境変数、args、unix_timestamp、millis、sleep_ms (7 functions) |
| `http` | ✅ runtime (`http_runtime.txt`) | HTTP サーバー/クライアント |
| `random` | ✅ runtime (`platform_runtime.txt`) | OS エントロピーベースの乱数 (4 functions) |

### Layer 3: x（公式拡張パッケージ）

`almide.toml` に依存追加して使う。公式メンテナンスだが stdlib とは独立してバージョン管理。

| Package | Status | Notes |
|---------|--------|-------|
| `encoding` | ✅ implemented (bundled .almd) → 分離予定 | hex, base64, url_encode/decode |
| `hash` | ✅ implemented (bundled .almd) | SHA-256, SHA-1, MD5 — pure Almide |
| `crypto` | planned | encryption |
| `csv` | planned (external package) | CSV parse/stringify — `almide/csv` |
| `term` | ✅ implemented (bundled .almd) | ANSI colors, terminal formatting |

### Playground Stdlib Support

Playground (WASM) で bundled .almd モジュールを利用可能にする。

- `stdlib::get_bundled_source()` で取得 → パース → `emit_with_modules` の modules 引数に渡す
- ブラウザ互換のもの（csv, encoding, hash, path）のみバンドル対象
- args は `env.args()` 依存で不可、time は `env.unix_timestamp()` 等が未対応、term はブラウザで無意味
- Phase B の platform namespace 導入後に再検討（platform 依存を import した bundled module がコンパイルエラーになれば安全に全モジュールバンドル可能）

### Implementation Steps

#### Phase A: WASM コンパイルエラー ✅
- [x] checker: WASM ターゲット時に platform モジュールの import を検出してエラー
- [x] `--target wasm` 時に checker にターゲット情報を渡す仕組み

#### Phase B: platform namespace 導入
- [ ] `import platform.fs` 構文の設計
- [ ] 既存の `import fs` からの移行パス（deprecation warning → エラー）
- [ ] platform モジュールの resolver 実装

#### Phase C: x パッケージ分離
- [ ] encoding を `almide/encoding` リポジトリに分離
- [ ] パッケージマネージャ経由で利用可能に
- [ ] hash, csv, term を x パッケージとして新規作成

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
