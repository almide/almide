# Stdlib Self-Hosted Redesign (PlatformView方式) [ACTIVE]

## Summary

stdlib の定義方式を TOML + build.rs コード生成から、**PlatformView 方式**に移行する。型シグネチャは `.almd` に書き、ターゲット別のネイティブ実装は本物の `.rs` / `.ts` ファイルとして配置する。コンパイラはテンプレート展開の汎用ロジックだけを持ち、ランタイム実装はそのターゲット言語のテストツールで直接検証できる。

## Motivation

### 現状の問題

1. **TOML 二重管理**: 1関数につき Rust テンプレート + TS テンプレートの2つを書く
2. **build.rs の複雑さ**: TOML → Rust コード生成器が 1000 行超、デバッグ困難
3. **src/generated/ の不透明さ**: 生成コードが 3 ファイル、手動編集禁止だが理解も困難
4. **ユーザーが同じ機構を使えない**: stdlib だけ特別扱いで、ユーザーは UFCS 拡張できない
5. **ランタイムがテスト不能**: `core_runtime.txt` は文字列テンプレートなので rust-analyzer も cargo test も効かない

### 先行事例

| フレームワーク | 型定義 | ターゲット実装 | テスト |
|---|---|---|---|
| Flutter PlatformView | Dart (MethodChannel) | iOS: Swift, Android: Kotlin | 各プラットフォームのテストツール |
| React Native Turbo Modules | TypeScript spec | iOS: Obj-C/Swift, Android: Kotlin | XCTest / JUnit |
| **Almide (提案)** | **.almd** | **Rust: .rs, TS: .ts** | **cargo test / deno test** |

共通の本質: **型定義は共通言語、実装はターゲットごとに本物のコードで書き、それぞれのテストツールで検証できる。**

### 既に動いている証拠

以下のモジュールは既に純粋 Almide で実装済み（ネイティブ実装すら不要）:
- `hash.almd` — SHA-256/SHA-1/MD5 をビット演算だけで実装 (189行)
- `csv.almd` — ステートマシン CSV パーサー (70行)
- `url.almd` — RFC 3986 準拠 URL パーサー (220行)
- `path.almd` — パス正規化、結合、分解
- `encoding.almd` — base64/hex エンコード・デコード
- `args.almd` — CLI 引数パーサー

## Design

### Architecture

```
stdlib/
  list/
    mod.almd              型シグネチャ + 純粋 Almide 実装
    runtime.rs            Rust ネイティブ実装（本物の Rust コード）
    runtime_test.rs       cargo test で直接テスト可能
    runtime.ts            TS ネイティブ実装（本物の TS コード）
    runtime_test.ts       deno test で直接テスト可能
  fs/
    mod.almd              型シグネチャ（全関数が @platform）
    runtime.rs            Rust: std::fs ラッパー
    runtime_test.rs       cargo test でファイル操作テスト
    runtime.ts            TS: Deno.* / Node fs ラッパー
    runtime_test.ts       deno test でファイル操作テスト
  string/
    mod.almd              大半は純粋 Almide、len/slice 等は @platform
    runtime.rs            Rust: &str メソッドラッパー
    runtime_test.rs
    runtime.ts            TS: String.prototype ラッパー
    runtime_test.ts
  hash/
    mod.almd              全て純粋 Almide（runtime ファイル不要）
  csv/
    mod.almd              全て純粋 Almide（runtime ファイル不要）
  ...
```

### `@platform` 構文

`@builtin` ではなく `@platform` を使う。Flutter/RN と同じ語彙で、「ターゲットプラットフォームが実装を提供する」ことを明示する。

```almide
// stdlib/list/mod.almd

// @platform: ターゲット別のネイティブ実装が runtime.rs / runtime.ts にある
@platform
fn map[A, B](xs: List[A], f: Fn(A) -> B) -> List[B]

@platform
fn filter[A](xs: List[A], f: Fn(A) -> Bool) -> List[A]

@platform
fn len[A](xs: List[A]) -> Int

// 純粋 Almide: コンパイラは普通にコンパイルする（runtime ファイル不要）
fn contains[A](xs: List[A], value: A) -> Bool {
  for x in xs {
    if x == value { return true }
  }
  false
}

fn reverse[A](xs: List[A]) -> List[A] {
  var result: List[A] = []
  var i = xs.len() - 1
  do i >= 0 {
    result = result ++ [xs[i]]
    i = i - 1
  }
  result
}
```

### ネイティブ実装: 本物のコード

```rust
// stdlib/list/runtime.rs
// cargo test で直接テスト可能な、本物の Rust コード

#[inline]
pub fn almide_rt_list_map<A: Clone, B>(
    xs: Vec<A>,
    f: impl Fn(A) -> B,
) -> Vec<B> {
    xs.into_iter().map(f).collect()
}

#[inline]
pub fn almide_rt_list_filter<A: Clone>(
    xs: Vec<A>,
    f: impl Fn(&A) -> bool,
) -> Vec<A> {
    xs.into_iter().filter(|x| f(x)).collect()
}

#[inline(always)]
pub fn almide_rt_list_len<A>(xs: &[A]) -> i64 {
    xs.len() as i64
}
```

```rust
// stdlib/list/runtime_test.rs

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_map() {
        assert_eq!(
            almide_rt_list_map(vec![1, 2, 3], |x| x * 2),
            vec![2, 4, 6]
        );
    }

    #[test]
    fn test_filter() {
        assert_eq!(
            almide_rt_list_filter(vec![1, 2, 3, 4], |x| x % 2 == 0),
            vec![2, 4]
        );
    }

    #[test]
    fn test_len() {
        assert_eq!(almide_rt_list_len(&vec![1, 2, 3]), 3);
        assert_eq!(almide_rt_list_len::<i64>(&vec![]), 0);
    }
}
```

```typescript
// stdlib/list/runtime.ts
// deno test で直接テスト可能な、本物の TS コード

export function almide_rt_list_map<A, B>(xs: A[], f: (a: A) => B): B[] {
  return xs.map(f);
}

export function almide_rt_list_filter<A>(xs: A[], f: (a: A) => boolean): A[] {
  return xs.filter(f);
}

export function almide_rt_list_len<A>(xs: A[]): number {
  return xs.length;
}
```

```typescript
// stdlib/list/runtime_test.ts

import { assertEquals } from "jsr:@std/assert";
import { almide_rt_list_map, almide_rt_list_filter, almide_rt_list_len } from "./runtime.ts";

Deno.test("map", () => {
  assertEquals(almide_rt_list_map([1, 2, 3], (x) => x * 2), [2, 4, 6]);
});

Deno.test("filter", () => {
  assertEquals(almide_rt_list_filter([1, 2, 3, 4], (x) => x % 2 === 0), [2, 4]);
});

Deno.test("len", () => {
  assertEquals(almide_rt_list_len([1, 2, 3]), 3);
  assertEquals(almide_rt_list_len([]), 0);
});
```

### スケルトン自動生成

React Native Codegen と同様、`.almd` の型シグネチャから `runtime.rs` / `runtime.ts` のスケルトンを自動生成:

```bash
almide scaffold stdlib/list/mod.almd
```

生成結果:

```rust
// stdlib/list/runtime.rs (auto-generated skeleton)
// TODO: Implement each @platform function

pub fn almide_rt_list_map<A: Clone, B>(xs: Vec<A>, f: impl Fn(A) -> B) -> Vec<B> {
    todo!("implement list.map")
}

pub fn almide_rt_list_filter<A: Clone>(xs: Vec<A>, f: impl Fn(&A) -> bool) -> Vec<A> {
    todo!("implement list.filter")
}

pub fn almide_rt_list_len<A>(xs: &[A]) -> i64 {
    todo!("implement list.len")
}
```

型マッピングは自動:
```
Int       → i64 (Rust) / number (TS)
Float     → f64 (Rust) / number (TS)
String    → String (Rust) / string (TS)
Bool      → bool (Rust) / boolean (TS)
List[A]   → Vec<A> (Rust) / A[] (TS)
Map[K, V] → HashMap<K, V> (Rust) / Map<K, V> (TS)
Option[A] → Option<A> (Rust) / A | null (TS)
Result[A, E] → Result<A, E> (Rust) / A (TS, throws on err)
Fn(A) -> B → impl Fn(A) -> B (Rust) / (a: A) => B (TS)
```

### コンパイラの役割

コンパイラは @platform 関数に対して:

1. `.almd` から型シグネチャを読む（通常の型チェック）
2. Codegen 時に対応する `runtime.rs` / `runtime.ts` を探す
3. ランタイム関数を生成コードに include/import する
4. 呼び出しサイトを `almide_rt_{module}_{func}(args...)` に展開する

コンパイラは**特定の関数を知らない**。知っているのは:
- `@platform` 付き関数は `runtime.{target拡張子}` に実装がある
- 関数名は `almide_rt_{module}_{func}` の規則でマップされる
- 型マッピングは固定テーブル（上記）

### モジュール分類

```
純粋 Almide（runtime ファイル不要）
├── hash           SHA-256, SHA-1, MD5
├── csv            パース, stringify
├── url            パース, ビルド
├── path           正規化, 結合, 分解
├── encoding       base64, hex
├── args           CLI 引数パーサー
├── term           色, スタイル
└── (多数の関数)   contains, reverse, take, drop, abs, clamp...

@platform あり（runtime.rs + runtime.ts 必要）
├── list           map, filter, sort_by 等 (~20 関数)
├── string         len, slice, split 等 (~10 関数)
├── int            to_string, from_string (~2 関数)
├── float          to_string, from_string (~2 関数)
├── math           sqrt, sin, cos 等 (~10 関数)
├── map            new, get, set 等 (~5 関数)
├── json           parse, stringify (~2 関数)
├── regex          全関数 (~8 関数)
├── fs             全関数 (~20 関数)
├── http           全関数 (~10 関数)
├── io             全関数 (~3 関数)
├── env            全関数 (~8 関数)
├── process        全関数 (~5 関数)
├── random         全関数 (~4 関数)
└── datetime       全関数 (~15 関数)
```

### 移行で消えるもの

| 消えるもの | 行数 | 代替 |
|---|---|---|
| `stdlib/defs/*.toml` (14 ファイル) | ~2000 行 | stdlib/*/mod.almd |
| `build.rs` の stdlib 生成部分 | ~1000 行 | なし |
| `src/generated/stdlib_sigs.rs` | ~800 行 | パーサーが .almd を直接読む |
| `src/generated/emit_rust_calls.rs` | ~1200 行 | stdlib/*/runtime.rs |
| `src/generated/emit_ts_calls.rs` | ~600 行 | stdlib/*/runtime.ts |
| `src/emit_rust/core_runtime.txt` | ~800 行 | stdlib/*/runtime.rs に分散 |
| `src/emit_ts_runtime.rs` | ~400 行 | stdlib/*/runtime.ts に分散 |
| **合計** | **~6800 行** | **テスト可能なネイティブコード** |

### 得られるもの

| 観点 | 現状 | PlatformView 方式 |
|---|---|---|
| ランタイムのテスト | 不可能（文字列テンプレート） | `cargo test` / `deno test` で直接テスト |
| IDE サポート | なし（.txt ファイル） | rust-analyzer / TS LSP が完全に効く |
| 新関数の追加 | TOML + 2ターゲットのテンプレート | .almd に型定義 → `almide scaffold` → 実装 |
| ユーザー拡張 | 不可能 | `@platform` で同じ仕組みを使える |
| コンパイラの責務 | 343 関数のディスパッチを知っている | テンプレート展開の汎用ロジックだけ |

---

## Phases

### Phase 0: @platform 構文と scaffold コマンド
- パーサーに `@platform` アトリビュートを追加
- チェッカーで @platform fn の型シグネチャを処理
- `almide scaffold <module.almd>` でスケルトン生成
- Lower で `IrFunction::Platform(module, func)` を生成
- Codegen で `runtime.rs` / `runtime.ts` から関数を include
- テスト: 1 モジュール（`io`）を @platform で動かす

### Phase 1: プラットフォームモジュールの移行
- fs, http, io, env, process, random, datetime → @platform + runtime ファイル
- これらは全関数が @platform（OS アクセス必須）
- `runtime_test.rs` / `runtime_test.ts` で各関数をテスト
- TOML 定義を削除

### Phase 2: 純粋ロジックモジュールの移行
- string, int, float, math, map, result → 純粋 .almd 実装
- ~95 関数を Almide で書き直す
- 型プリミティブ（string.len, list.get 等 ~10 関数）だけ @platform
- パフォーマンス比較: Rust 直書きと .almd 経由で有意差がないことを確認

### Phase 3: クロージャ関数の移行
- list.map, filter, sort_by, flat_map 等 ~20 関数
- @platform として残すか、コンパイラの最適化で .almd 化するか判断
- 判断基準: `for + append` パターンの最適化パスが実装済みか
- 実装済みなら .almd 化、未実装なら @platform に残す

### Phase 4: 生成コード撤去
- `stdlib/defs/*.toml` 全削除
- `build.rs` から stdlib 生成ロジックを削除
- `src/generated/` から stdlib 関連ファイルを削除
- `src/emit_rust/core_runtime.txt` 削除
- `src/emit_ts_runtime.rs` 削除
- `src/stdlib.rs` を簡素化（UFCS 解決は .almd のエクスポートから自動導出）

### Phase 5: ユーザー @platform 開放
- ユーザーが自分のパッケージで `@platform` を使えるようにする
- FFI 的な用途: Rust crate や npm パッケージを直接ラップ
- `runtime.rs` / `runtime.ts` を自分のパッケージに含める

---

## CI Integration

### ランタイムテストの独立実行

```yaml
# .github/workflows/ci.yml に追加
runtime-test-rust:
  name: Runtime Tests (Rust)
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v4
    - run: cargo test --manifest-path stdlib/Cargo.toml

runtime-test-ts:
  name: Runtime Tests (TS)
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v4
    - uses: denoland/setup-deno@v2
    - run: deno test stdlib/*/runtime_test.ts
```

ランタイム実装は Almide コンパイラと**独立して**テストできる。コンパイラの変更でランタイムが壊れないことを保証。

---

## @platform 全リスト (~60 関数)

### Tier 1: 型プリミティブ (10)
```
string: len, char_at, slice, from_chars
list:   len, get, push, set
int:    to_string, from_string
```

### Tier 2: クロージャ最適化 (20)
```
list:   map, filter, find, any, all, each, sort_by, flat_map,
        filter_map, take_while, drop_while, reduce, group_by,
        fold, scan, zip_with, partition, count, find_index, update
```

### Tier 3: プラットフォーム (30)
```
fs:      read_text, read_bytes, write, write_bytes, append,
         mkdir_p, exists, remove, list_dir, is_dir, is_file,
         copy, rename, walk, stat, glob, temp_dir
http:    get, post, put, patch, delete, request
io:      print, read_line
env:     get, set, args, cwd, os
process: exec, exec_status, exit
random:  int, float, bytes, choice
json:    parse, stringify
regex:   new, is_match, find, find_all, replace, split, captures
```

## Success Criteria

- `almide test` が全テスト通過
- TOML 定義ファイル 0、build.rs に stdlib 生成コードなし
- `cargo test --manifest-path stdlib/Cargo.toml` でランタイム Rust テスト通過
- `deno test stdlib/*/runtime_test.ts` でランタイム TS テスト通過
- 新しい stdlib 関数の追加手順:
  1. `mod.almd` に `@platform fn` を追加
  2. `almide scaffold` でスケルトン生成
  3. `runtime.rs` / `runtime.ts` に実装を書く
  4. `runtime_test.rs` / `runtime_test.ts` でテスト

## Dependencies

- [IR Optimization Passes](ir-optimization.md) — Phase 3 の判断に影響（for+append 最適化）
- [Codegen Refinement](codegen-refinement.md) — .almd 生成コードの品質

## Supersedes

- [Stdlib Strategy](stdlib-strategy.md) の戦略 1 (TOML + ランタイム) と戦略 2 (@extern)
  - 戦略 3 (self-host) と戦略 4 (x/ パッケージ) は引き続き有効
- [Stdlib Architecture: 3-Layer Design](../on-hold/stdlib-architecture-3-layer-design.md) の内部実装方式

## Files
```
src/parser/mod.rs (add @platform parsing)
src/check/ (handle @platform fn signatures)
src/lower.rs (emit Platform IR nodes)
src/emit_rust/ (include runtime.rs, dispatch @platform calls)
src/emit_ts/ (include runtime.ts, dispatch @platform calls)
src/cli.rs (add scaffold subcommand)
stdlib/*/mod.almd (type signatures + pure Almide)
stdlib/*/runtime.rs (Rust native implementations)
stdlib/*/runtime_test.rs (Rust tests)
stdlib/*/runtime.ts (TS native implementations)
stdlib/*/runtime_test.ts (TS tests)
stdlib/Cargo.toml (workspace for runtime Rust tests)
```
