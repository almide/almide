# Stdlib Self-Hosted Redesign [ACTIVE]

## Summary

stdlib の定義方式を TOML + build.rs コード生成から、**@extern + ネイティブ実装ファイル方式**に移行する。型シグネチャは `.almd` に書き、ターゲット別のネイティブ実装は本物の `.rs` / `.ts` ファイルとして配置する。コンパイラはテンプレート展開の汎用ロジックだけを持ち、ネイティブ実装はそのターゲット言語のテストツールで直接検証できる。

## Motivation

### 現状の問題

1. **TOML 二重管理**: 1関数につき Rust テンプレート + TS テンプレートの2つを書く
2. **build.rs の複雑さ**: TOML → Rust コード生成器が 1000 行超、デバッグ困難
3. **src/generated/ の不透明さ**: 生成コードが 3 ファイル、手動編集禁止だが理解も困難
4. **ユーザーが同じ機構を使えない**: stdlib だけ特別扱いで、ユーザーは UFCS 拡張できない
5. **ネイティブ実装がテスト不能**: `core_runtime.txt` は文字列テンプレートなので rust-analyzer も cargo test も効かない

### 先行事例

| フレームワーク | 型定義 | ターゲット実装 | テスト |
|---|---|---|---|
| Flutter PlatformView | Dart (MethodChannel) | iOS: Swift, Android: Kotlin | 各プラットフォームのテストツール |
| React Native Turbo Modules | TypeScript spec → Codegen | iOS: Obj-C/Swift, Android: Kotlin | XCTest / JUnit |
| Android Resources | XML (values/) | values-v21/, values-v28/ | バージョン別 fallback |
| **Almide (提案)** | **.almd** | **Rust: .rs, TS: .ts + バリアント** | **cargo test / deno test** |

共通の本質: **型定義は共通言語、実装はターゲットごとに本物のコードで書き、それぞれのテストツールで検証できる。**

### 既に動いている証拠

以下のモジュールは既に純粋 Almide で実装済み（ネイティブ実装すら不要）:
- `hash.almd` — SHA-256/SHA-1/MD5 をビット演算だけで実装 (189行)
- `csv.almd` — ステートマシン CSV パーサー (70行)
- `url.almd` — RFC 3986 準拠 URL パーサー (220行)
- `path.almd` — パス正規化、結合、分解
- `encoding.almd` — base64/hex エンコード・デコード
- `args.almd` — CLI 引数パーサー

---

## Design

### Architecture

```
stdlib/
  list/
    mod.almd              型シグネチャ + 純粋 Almide 実装
    extern.rs             Rust ネイティブ実装（本物の Rust コード）
    extern_test.rs        cargo test で直接テスト可能
    extern.ts             TS ネイティブ実装（本物の TS コード）
    extern_test.ts        deno test で直接テスト可能
  fs/
    mod.almd              型シグネチャ（全関数が @extern）
    extern.rs             Rust: std::fs（sync, native）
    extern.wasm.rs        Rust: WASM 環境（制限付き or stub）
    extern.async.rs       Rust: tokio::fs（async）
    extern_test.rs
    extern.ts             TS: Deno（デフォルト）
    extern.node.ts        TS: Node fs
    extern.node.22.ts     TS: Node 22+（fs.glob 対応）
    extern.browser.ts     TS: File System Access API
    extern_test.ts
    extern_test.node.ts
  hash/
    mod.almd              全て純粋 Almide（extern ファイル不要）
  csv/
    mod.almd              全て純粋 Almide（extern ファイル不要）
```

### `@extern` 構文

Rust (`extern`)、Gleam (`@external`)、C (`extern`) で確立された用語。「この関数の実装は Almide の外にある」ことを明示する。

```almide
// stdlib/list/mod.almd

// @extern: ターゲット別のネイティブ実装が extern.rs / extern.ts にある
@extern
fn map[A, B](xs: List[A], f: Fn(A) -> B) -> List[B]

@extern
fn filter[A](xs: List[A], f: Fn(A) -> Bool) -> List[A]

@extern
fn len[A](xs: List[A]) -> Int

// 純粋 Almide: コンパイラは普通にコンパイルする（extern ファイル不要）
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

### バリアント解決システム

Android のリソース解決（`values-v21/`, `values-v28/`）と同じ発想。最も具体的なマッチを優先し、なければ汎用に fallback する。

#### バリアント軸

| 軸 | 例 | 影響 |
|---|---|---|
| **ターゲット言語** | rust, ts | 必須。extern.rs / extern.ts |
| **実行環境** | native, wasm, browser | WASM では fs/http が使えない |
| **ランタイム** | deno, node, bun | TS の API が違う |
| **同期モデル** | sync, async | `std::fs` vs `tokio::fs` |
| **バージョン** | 18, 22, etc. | メジャーバージョンで API が変わる |

#### ファイル命名規則

```
extern.{target}                    ベースライン
extern.{variant}.{target}          環境/ランタイムバリアント
extern.{variant}.{version}.{target}  バージョン付きバリアント
```

例:
```
extern.rs                  Rust デフォルト（sync, native）
extern.wasm.rs             Rust + WASM
extern.async.rs            Rust + async（tokio）

extern.ts                  TS デフォルト（Deno）
extern.node.ts             Node 全バージョン
extern.node.18.ts          Node 18+（native fetch）
extern.node.22.ts          Node 22+（fs.glob）
extern.browser.ts          ブラウザ（Web API）
extern.bun.ts              Bun（将来）
```

#### 解決順序

コンパイラフラグ: `--target {lang} [--env {env}] [--env-version {ver}]`

```
--target rust --env wasm
  1. extern.wasm.rs       ← あればこれ
  2. extern.rs            ← fallback

--target ts --env node --env-version 22
  1. extern.node.22.ts    ← あれば最優先
  2. extern.node.18.ts    ← 降格（22 > 18 なので対象）
  3. extern.node.ts       ← ランタイムベースライン
  4. extern.ts            ← 汎用 fallback

--target ts                (env 未指定 → デフォルト)
  1. extern.ts            ← これだけ
```

バージョンは **「指定バージョン以下で最大」** を選ぶ。Node 22 で `extern.node.18.ts` はマッチするが `extern.node.25.ts` はマッチしない。

#### 実用例: fs モジュール

```
stdlib/fs/
  mod.almd                型シグネチャ（全関数が @extern）

  extern.rs               std::fs（sync, native）
  extern.wasm.rs           WASM（stub: "fs not available in WASM"）
  extern.async.rs          tokio::fs

  extern.ts               Deno（Deno.readTextFileSync 等）
  extern.node.ts           Node（require("fs")）
  extern.node.22.ts        Node 22+（fs.glob 追加）
  extern.browser.ts        File System Access API / stub

  extern_test.rs
  extern_test.ts
  extern_test.node.ts
```

大半のモジュールは `extern.rs` + `extern.ts` の **2ファイルだけ**。バリアントが要るモジュールだけファイルを追加する。

---

### ネイティブ実装: 本物のコード

```rust
// stdlib/list/extern.rs
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
// stdlib/list/extern_test.rs

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
// stdlib/list/extern.ts
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
// stdlib/list/extern_test.ts

import { assertEquals } from "jsr:@std/assert";
import { almide_rt_list_map, almide_rt_list_filter, almide_rt_list_len } from "./extern.ts";

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

React Native Codegen と同様、`.almd` の型シグネチャから `extern.rs` / `extern.ts` のスケルトンを自動生成:

```bash
almide scaffold stdlib/list/mod.almd
```

生成結果:

```rust
// stdlib/list/extern.rs (auto-generated skeleton)
// TODO: Implement each @extern function

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

コンパイラは @extern 関数に対して:

1. `.almd` から型シグネチャを読む（通常の型チェック）
2. Codegen 時に対応する `extern.{variant}.{target}` を解決（バリアント fallback）
3. ネイティブ関数を生成コードに include/import する
4. 呼び出しサイトを `almide_rt_{module}_{func}(args...)` に展開する

コンパイラは**特定の関数を知らない**。知っているのは:
- `@extern` 付き関数は `extern.{target拡張子}` に実装がある
- バリアント解決の fallback ルール
- 関数名は `almide_rt_{module}_{func}` の規則でマップされる
- 型マッピングは固定テーブル（上記）

### モジュール分類

```
純粋 Almide（extern ファイル不要）
├── hash           SHA-256, SHA-1, MD5
├── csv            パース, stringify
├── url            パース, ビルド
├── path           正規化, 結合, 分解
├── encoding       base64, hex
├── args           CLI 引数パーサー
├── term           色, スタイル
└── (多数の関数)   contains, reverse, take, drop, abs, clamp...

@extern あり（extern.rs + extern.ts 必要）
├── list           map, filter, sort_by 等 (~20 関数)
├── string         len, slice, split 等 (~10 関数)
├── int            to_string, from_string (~2 関数)
├── float          to_string, from_string (~2 関数)
├── math           sqrt, sin, cos 等 (~10 関数)
├── map            new, get, set 等 (~5 関数)
├── json           parse, stringify (~2 関数)
├── regex          全関数 (~8 関数)
├── fs             全関数 (~20 関数) ← バリアント多数
├── http           全関数 (~10 関数) ← バリアントあり
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
| `src/generated/emit_rust_calls.rs` | ~1200 行 | stdlib/*/extern.rs |
| `src/generated/emit_ts_calls.rs` | ~600 行 | stdlib/*/extern.ts |
| `src/emit_rust/core_runtime.txt` | ~800 行 | stdlib/*/extern.rs に分散 |
| `src/emit_ts_runtime.rs` | ~400 行 | stdlib/*/extern.ts に分散 |
| **合計** | **~6800 行** | **テスト可能なネイティブコード** |

### 得られるもの

| 観点 | 現状 | @extern 方式 |
|---|---|---|
| ネイティブ実装のテスト | 不可能（文字列テンプレート） | `cargo test` / `deno test` で直接テスト |
| IDE サポート | なし（.txt ファイル） | rust-analyzer / TS LSP が完全に効く |
| 新関数の追加 | TOML + 2ターゲットのテンプレート | .almd に型定義 → `almide scaffold` → 実装 |
| ユーザー拡張 | 不可能 | `@extern` で同じ仕組みを使える |
| コンパイラの責務 | 343 関数のディスパッチを知っている | バリアント解決 + include の汎用ロジックだけ |
| 環境対応 | Deno/Node が1ファイルに混在 | バリアントファイルで分離 |
| バージョン対応 | なし | fallback チェインで自然に対応 |

---

## Phases

### Phase 0: @extern 構文とバリアント解決
- パーサーに `@extern` アトリビュートを追加
- チェッカーで @extern fn の型シグネチャを処理
- バリアント解決ロジック実装（ファイル探索 + fallback チェイン）
- `--env`, `--env-version` コンパイラフラグ追加
- `almide scaffold <module.almd>` でスケルトン生成
- Lower で `IrFunction::Extern(module, func)` を生成
- Codegen で解決済み extern ファイルから関数を include
- テスト: 1 モジュール（`io`）を @extern で動かす

### Phase 1: プラットフォームモジュールの移行
- fs, http, io, env, process, random, datetime → @extern + extern ファイル
- これらは全関数が @extern（OS アクセス必須）
- fs: extern.rs / extern.ts / extern.node.ts のバリアント分離
- `extern_test.rs` / `extern_test.ts` で各関数をテスト
- TOML 定義を削除

### Phase 2: 純粋ロジックモジュールの移行
- string, int, float, math, map, result → 純粋 .almd 実装
- ~95 関数を Almide で書き直す
- 型プリミティブ（string.len, list.get 等 ~10 関数）だけ @extern
- パフォーマンス比較: Rust 直書きと .almd 経由で有意差がないことを確認

### Phase 3: クロージャ関数の移行
- list.map, filter, sort_by, flat_map 等 ~20 関数
- @extern として残すか、コンパイラの最適化で .almd 化するか判断
- 判断基準: `for + append` パターンの最適化パスが実装済みか
- 実装済みなら .almd 化、未実装なら @extern に残す

### Phase 4: 生成コード撤去
- `stdlib/defs/*.toml` 全削除
- `build.rs` から stdlib 生成ロジックを削除
- `src/generated/` から stdlib 関連ファイルを削除
- `src/emit_rust/core_runtime.txt` 削除
- `src/emit_ts_runtime.rs` 削除
- `src/stdlib.rs` を簡素化（UFCS 解決は .almd のエクスポートから自動導出）

### Phase 5: ユーザー @extern 開放
- ユーザーが自分のパッケージで `@extern` を使えるようにする
- FFI 的な用途: Rust crate や npm パッケージを直接ラップ
- `extern.rs` / `extern.ts` を自分のパッケージに含める
- バリアントも自由に追加可能

---

## CI Integration

### ネイティブ実装テストの独立実行

```yaml
# .github/workflows/ci.yml に追加
runtime-test-rust:
  name: Extern Tests (Rust)
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v4
    - run: cargo test --manifest-path stdlib/Cargo.toml

runtime-test-ts:
  name: Extern Tests (TS/Deno)
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v4
    - uses: denoland/setup-deno@v2
    - run: deno test stdlib/*/extern_test.ts

runtime-test-node:
  name: Extern Tests (Node)
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v4
    - uses: actions/setup-node@v4
      with: { node-version: "22" }
    - run: npx tsx --test stdlib/*/extern_test.node.ts
```

ネイティブ実装は Almide コンパイラと**独立して**テストできる。コンパイラの変更でネイティブ実装が壊れないことを保証。

---

## @extern 全リスト (~60 関数)

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
- `cargo test --manifest-path stdlib/Cargo.toml` でネイティブ Rust テスト通過
- `deno test stdlib/*/extern_test.ts` でネイティブ TS テスト通過
- バリアント解決が正しく動作（`--env node --env-version 22` で `extern.node.22.ts` が選ばれる）
- 新しい stdlib 関数の追加手順:
  1. `mod.almd` に `@extern fn` を追加
  2. `almide scaffold` でスケルトン生成
  3. `extern.rs` / `extern.ts` に実装を書く
  4. `extern_test.rs` / `extern_test.ts` でテスト
  5. バリアントが要れば `extern.{variant}.{target}` を追加

## Dependencies

- [IR Optimization Passes](ir-optimization.md) — Phase 3 の判断に影響（for+append 最適化）
- [Codegen Refinement](codegen-refinement.md) — .almd 生成コードの品質

## Supersedes

- [Stdlib Strategy](stdlib-strategy.md) の戦略 1 (TOML + ランタイム) と戦略 2 (@extern)
  - 戦略 3 (self-host) と戦略 4 (x/ パッケージ) は引き続き有効
- [Stdlib Architecture: 3-Layer Design](../on-hold/stdlib-architecture-3-layer-design.md) の内部実装方式

## Files
```
src/parser/mod.rs (add @extern parsing)
src/check/ (handle @extern fn signatures)
src/lower.rs (emit Extern IR nodes)
src/emit_rust/ (include extern.rs, dispatch @extern calls)
src/emit_ts/ (include extern.ts, dispatch @extern calls)
src/resolve.rs (variant resolution logic)
src/cli.rs (add scaffold subcommand, --env/--env-version flags)
stdlib/*/mod.almd (type signatures + pure Almide)
stdlib/*/extern.rs (Rust native implementations)
stdlib/*/extern.{variant}.rs (Rust variant implementations)
stdlib/*/extern_test.rs (Rust tests)
stdlib/*/extern.ts (TS native implementations)
stdlib/*/extern.{variant}.ts (TS variant implementations)
stdlib/*/extern_test.ts (TS tests)
stdlib/Cargo.toml (workspace for Rust extern tests)
```
