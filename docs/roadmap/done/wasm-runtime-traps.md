<!-- description: Fix 44 WASM runtime traps (protocols, maps, closures, strings) -->
# WASM Runtime Traps [ACTIVE]

## Status: 21 pass, 44 runtime traps, 8 skipped, 0 compile failures

## Trap Categories (44 files)

### Protocol/Convention dispatch [8 files]
basic protocol method, basic protocol satisfaction, builder pattern via protocol,
convention method resolution, convention method via UFCS, diamond protocol,
generic function with protocol bound, protocol methods chained via pipe

**Root**: protocol メソッド呼び出しが `unreachable` に落ちる。
WASM codegen で convention/protocol dispatch が未実装。
Rust codegen は `TypeName.method(self)` の直接呼び出しに変換するが、WASM ではテーブル間接呼び出しか名前解決が必要。

**Fix**: emit_call で protocol method を Named call に解決する。

### Map operations [4 files]
map basic operations, map creation and get, empty map operations, empty map with type annotation

**Root**: Map 型の WASM runtime が未実装。
List は linear memory + length header で実装済みだが、Map はハッシュテーブルが必要。

**Fix**: Map runtime を実装（hash + bucket array）。大工事。

### Map iteration [3 files]
for over map, for with map tuple destructure, for with zip

**Root**: `for k, v in map` の WASM codegen が未実装。
List iteration は実装済み。Map iteration は Map runtime に依存。

**Fix**: Map runtime 後に実装。

### Fan (sequential fallback) [4 files]
fan basic, fan.map, fan.race, fan.any

**Root**: fan (並行実行) の WASM codegen が未実装。
WASM は single-thread だが、fan の semantics は「複数の式を実行して結果を集める」。
sequential に実行すれば同じ結果が得られる。

**Fix**: fan を sequential fallback で実装。
- `fan { a, b, c }` → `(a(), b(), c())`
- `fan.map(list, fn)` → `list.map(list, fn)`
- `fan.race(list, fn)` → 先頭要素を返す

### Deep equality [3 files]
nested list equality, deep equality on nested structures, recursive variant types

**Root**: `assert_eq` で nested containers (List[List[T]], variant with payloads) の
deep equality が incomplete。`option_eq_i64` / `list_eq` が shallow comparison のみ。

**Fix**: runtime の eq 関数を recursive にする。型情報に基づいた dispatch が必要。

### Record/Variant features [3 files]
let record destructure, nested open record, basic variant record construction

**Root**: record destructure (`let { name, age } = person`) と variant record
construction の WASM codegen が incomplete。

**Fix**: emit_stmt の BindDestructure と emit_record の variant record case を実装。

### Type features [5 files]
match constructor with payload, match nested option, default fields - omit all defaults,
comparison on type variable, multi type param generic

**Root**: 個別のパターンマッチや generic 関数の codegen edge case。
各テストで最初に trap する箇所を特定して個別修正。

**Fix**: 各ケースを調査して修正。

### String operations [2 files]
string split and join, json stringify

**Root**: `string.split`, `string.join` の WASM runtime が未実装 (stub)。

**Fix**: runtime に string split/join を実装。

### Import/Codec [4 files]
encode/decode roundtrip, result.map, naming strategy, unit variant encode

**Root**: Codec 系 + module import の WASM 対応。

**Fix**: 大部分は skip 対象。result.map は stdlib runtime に追加。

### Misc codegen [4 files]
UFCS basic, pipe into list function, nested closure, do guard

**Root**: 個別の codegen パターン。UFCS 変換、pipe desugar、closure の nested capture 等。

**Fix**: 各ケースを調査して修正。

### Other [4 files]
structured error, string keys, tco deep recursion, variant roundtrip

## Priority Order

| Priority | Category | Files | Impact | Effort |
|----------|----------|-------|--------|--------|
| 1 | Fan sequential fallback | 4 | -4 traps | Low |
| 2 | Protocol dispatch | 8 | -8 traps, 大量のテスト unblock | Medium |
| 3 | Record/Variant features | 3 | -3 traps | Medium |
| 4 | Type features | 5 | -5 traps | Medium (個別) |
| 5 | Misc codegen | 4 | -4 traps | Medium (個別) |
| 6 | Deep equality | 3 | -3 traps | Medium |
| 7 | String operations | 2 | -2 traps | Low |
| 8 | Import/Codec skip | 4 | -4 traps | 5 min |
| 9 | Map runtime | 7 | -7 traps | High (hash table) |
| 10 | TCO | 1 | -1 trap | High |

## Expected Progress

| After step | Pass | Traps |
|------------|------|-------|
| 現状 | 21 | 44 |
| 1 (fan) | 25 | 40 |
| 2 (protocol) | 29+ | 32 |
| 3-5 (features) | 41+ | 20 |
| 6-8 (equality/string/codec) | 50+ | 11 |
| 9 (map) | 57+ | 4 |
