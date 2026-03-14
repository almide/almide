# Stdlib Runtime Architecture Reform [ACTIVE]

## Vision

stdlib は `.almd` を中心に定義される。純粋ロジックは Almide 自身で実装される。ホスト依存機能だけ `@extern` でターゲット実装を持つ。ネイティブ実装は本物の Rust/TS コードで、それぞれのテストツールで直接検証できる。`@extern` は stdlib 専用機能ではなく、ユーザーにも開放できる一般機構である。

---

## Decision Rules

設計判断の原則。迷ったらここに戻る。

### Rule 1: 純粋仕様で書けるなら `.almd` を優先する

Almide で書けるものは Almide で書く。self-hosted 化は言語の成熟を示す。

### Rule 2: ホスト能力に依存するなら `@extern`

OS/ランタイムへのアクセスが必要な関数だけ @extern にする。@extern は「雑に逃がす場所」ではなく、必要なものだけ外出しする場所。

### Rule 3: 性能が未熟なものだけ暫定 `@extern`

`list.map` 等のクロージャ関数は、コンパイラの最適化が追いつくまで暫定的に @extern に残す。最適化が成熟したら .almd に移行する。

### Rule 4: 同じ失敗は全ターゲットで同じ型表現にする

throw/panic ではなく Result/Option で正規化する。TS ターゲットでも `Result[A, E]` は値として表現する（throw に変換しない）。

### Rule 5: variant は環境差だけに使い、意味差には使わない

sync/async や API 面の差を variant に押し込まない。async は別モジュールまたは言語の effect/async model として扱う。

### Rule 6: コンパイラはデプロイ先を知らない

コンパイラが知るのは **target（rust / ts / js）、runtime（native / node / deno / browser / wasm）、version（optional）** の3軸だけ。Cloudflare、Docker、AWS Lambda、Deno Deploy 等のデプロイ先はコンパイラのスコープ外。外部ツール（wrangler, docker, sam 等）の責務とする。

Go が GOOS/GOARCH だけ知っていて、Docker に包むのは Dockerfile の仕事、Cloud Run に載せるのは gcloud の仕事、と10年以上ぶれずに設計が持っているのと同じ線引き。

```
コンパイラの仕事:    almide build app.almd --target ts --runtime node
外のツールの仕事:    docker build / wrangler deploy / sam deploy / etc.
```

この境界を崩さないことで、コンパイラの複雑さが増えず、長期的に設計が持つ。

### Rule 7: glue runtime は明示的に存在する

ターゲット間の値変換（Result 表現、Option 表現等）を行う glue layer は、隠れた魔法ではなく、リポジトリ上で普通に見えるファイルとして配置する。glue の責務は「翻訳者」に限定し、ロジック本体を glue に書かない。

```
stdlib/
  _glue/
    result.ts          Result<A,E> の TS 値表現 + 変換ヘルパー
    result.rs          Rust は Result がネイティブなので薄い
    types.ts           Option, Unit 等の共通型定義
    types.rs
```

---

## Non-Goals (v1)

- ユーザー定義 @extern パッケージの配布機構は v1 では作らない
- async model の一般解決は v1 ではしない
- 全 stdlib 関数の pure Almide 化は v1 では完了しない
- コンパイラバージョン間の ABI stability は v1 では保証しない
- API 語彙の改革（verb system）はこの文書のスコープ外。[Stdlib API Surface Reform](stdlib-verb-system.md) で扱う
- デプロイ先固有の設定生成（Dockerfile, wrangler.toml 等）はコンパイラでやらない

---

## Motivation

### 現状の問題

1. **TOML 二重管理**: 1関数につき Rust テンプレート + TS テンプレートの2つを書く
2. **build.rs の複雑さ**: TOML → Rust コード生成器が 1000 行超、デバッグ困難
3. **src/generated/ の不透明さ**: 生成コードが 3 ファイル、手動編集禁止だが理解も困難
4. **ユーザーが同じ機構を使えない**: stdlib だけ特別扱いで、ユーザーは UFCS 拡張できない
5. **ネイティブ実装がテスト不能**: `core_runtime.txt` は文字列テンプレートなので rust-analyzer も cargo test も効かない

**本質**: stdlib がコンパイラ内蔵の特殊機構になっている。テスト不能、拡張不能、デバッグ困難。これをコンパイラ外の通常の言語資産に戻す。

### 先行事例

| フレームワーク | 型定義 | ターゲット実装 | テスト |
|---|---|---|---|
| Flutter PlatformView | Dart (MethodChannel) | iOS: Swift, Android: Kotlin | 各プラットフォームのテストツール |
| React Native Turbo Modules | TypeScript spec → Codegen | iOS: Obj-C/Swift, Android: Kotlin | XCTest / JUnit |
| Android Resources | XML (values/) | values-v21/, values-v28/ | バージョン別 fallback |
| **Almide (提案)** | **.almd** | **Rust: .rs, TS: .ts + バリアント** | **cargo test / deno test** |

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
    extern.ts             TS: Deno（デフォルト）
    extern.node.ts        TS: Node fs
    extern.node.22.ts     TS: Node 22+（fs.glob 対応）
    extern.browser.ts     TS: File System Access API
    extern_test.rs
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
```

---

### @extern Contract

@extern 関数の厳密な契約。

#### 名前解決規則

- `@extern fn foo(...)` は `extern.{target}` ファイル内の `almide_rt_{module}_{func}` にマップされる
- コンパイラは特定の関数名を知らない。規則ベースで機械的に解決する

#### 型マッピング

```
Almide            Rust                       TS
─────────────────────────────────────────────────────
Int               i64                        number
Float             f64                        number
String            String / &str              string
Bool              bool                       boolean
List[A]           Vec<A>                     A[]
Map[K, V]         HashMap<K, V>              Map<K, V>
Option[A]         Option<A>                  A | null
Result[A, E]      Result<A, E>               { ok: true, value: A } | { ok: false, error: E }
Fn(A) -> B        impl Fn(A) -> B            (a: A) => B
Unit              ()                         void
```

#### エラー表現（Rule 4）

**Result は全ターゲットで値として表現する。**

```typescript
// TS での Result 表現 — throw ではなく値オブジェクト
type Result<A, E> =
  | { ok: true; value: A }
  | { ok: false; error: E };

// extern 関数は Result を返す
export function almide_rt_fs_read_text(path: string): Result<string, string> {
  try {
    return { ok: true, value: Deno.readTextFileSync(path) };
  } catch (e) {
    return { ok: false, error: String(e) };
  }
}
```

#### Purity / Effects

- @extern 関数は純粋関数でも副作用関数でもよい
- Almide 側で `effect fn` と宣言されていれば副作用関数
- extern 実装側は Almide の型シグネチャに従う責任がある

#### 整合性チェック

コンパイル時に以下を検証する:

- `mod.almd` の @extern 宣言に対応する関数が `extern.{target}` に存在するか
- 関数名が `almide_rt_{module}_{func}` の規則に従っているか
- 完全な型検査は行わない（trust-based）が、存在確認はコンパイルエラーにする

#### Missing implementation

- @extern 関数に対応する extern ファイルがない → コンパイルエラー
- extern ファイル内に対応する関数がない → コンパイルエラー（存在確認）
- バリアントが見つからない → fallback チェインで解決、全て失敗したらエラー

---

### バリアント解決システム

Go の `_linux.go` / `_darwin_arm64.go` と同じ発想。ファイル名に環境情報を含め、ビルド時に自動で選ばれる。特別な設定ファイルは不要。

```
Go                          Almide
────────────────────────────────────────────
GOOS (linux/darwin/...)     --target (rust/ts)
GOARCH (amd64/arm64/...)    --runtime (native/node/deno/wasm/browser)
_linux.go                   extern.rs
_linux_arm64.go             extern.wasm.rs
_js_wasm.go                 extern.browser.ts
//go:build tag              (将来の build constraint)
```

Go が GOOS/GOARCH の2軸だけで10年以上設計が持っているように、Almide も軸を無闇に増やさない。

#### v1 のバリアント軸（3軸のみ）

| 軸 | Go での対応 | Almide | 値 |
|---|---|---|---|
| **target** | GOOS | `--target` | rust, ts |
| **runtime** | GOARCH | `--runtime` | native, node, deno, browser, wasm |
| **version** | — | `--runtime-version` | 数値（optional） |

**v1 に含めないもの:**
- `async` — API差であって環境差ではない（Rule 5）。別モジュール（`fs_async`）または言語の async model で扱う
- デプロイ先固有の知識 — Cloudflare, Docker, Lambda 等はコンパイラのスコープ外（Rule 6）

#### ファイル命名規則

```
extern.{target}                        ベースライン
extern.{runtime}.{target}              ランタイムバリアント
extern.{runtime}.{version}.{target}    バージョン付きバリアント
```

実例:
```
extern.rs                  Rust デフォルト（native）
extern.wasm.rs             Rust + WASM

extern.ts                  TS デフォルト（Deno）
extern.node.ts             Node 全バージョン
extern.node.18.ts          Node 18+（native fetch）
extern.node.22.ts          Node 22+（fs.glob）
extern.browser.ts          ブラウザ（Web API）
```

#### 解決順序

コンパイラフラグ: `--target {lang} [--runtime {runtime}] [--runtime-version {ver}]`

```
--target ts --runtime node --runtime-version 22
  1. extern.node.22.ts    ← あれば最優先
  2. extern.node.18.ts    ← 降格（22 > 18 なので対象）
  3. extern.node.ts       ← ランタイムベースライン
  4. extern.ts            ← 汎用 fallback

--target rust --runtime wasm
  1. extern.wasm.rs       ← あればこれ
  2. extern.rs            ← fallback

--target ts                (runtime 未指定 → デフォルト)
  1. extern.ts            ← これだけ
```

バージョンは「指定バージョン以下で最大」を選ぶ。

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
        assert_eq!(almide_rt_list_map(vec![1, 2, 3], |x| x * 2), vec![2, 4, 6]);
    }

    #[test]
    fn test_filter() {
        assert_eq!(almide_rt_list_filter(vec![1, 2, 3, 4], |x| x % 2 == 0), vec![2, 4]);
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

---

### TS ターゲットの出力モデル

Gleam 方式（import で参照）を採用。extern ファイルは生成コードとは別ファイルで配置される。

```bash
almide build app.almd --target ts -o dist/
```

出力:
```
dist/
  app.ts              生成されたユーザーコード
  _extern/
    list.ts           stdlib/list/extern.ts のコピー
    fs.ts             stdlib/fs/extern.ts のコピー（バリアント解決済み）
```

```typescript
// dist/app.ts (生成コード)
import { almide_rt_list_map } from "./_extern/list.ts";
import { almide_rt_fs_read_text } from "./_extern/fs.ts";

function main() {
  const content = almide_rt_fs_read_text("hello.txt");
  const nums = almide_rt_list_map([1, 2, 3], (x) => x * 2);
}
```

`almide run` の場合は利便性のためインライン展開する（temp ファイル1つで実行可能）。

---

### 確認テスト体制

@extern は 3 層のテストで検証する:

| 層 | 何を検証 | ツール |
|---|---|---|
| **Native unit test** | extern 関数が単体で正しく動くか | `cargo test` / `deno test` |
| **Shared conformance test** | Rust/TS 間で挙動が一致するか | `almide test spec/stdlib/` |
| **Integration test** | @extern 関数が Almide コードから正しく呼べるか | `almide test` |

Native test と conformance test が分離されていることで、「Rust では動くが TS で動かない」問題を早期検出できる。

---

### モジュール分類と移行戦略

| Module | Runtime Strategy | 理由 |
|---|---|---|
| hash, csv, url, path, encoding, args, term | **pure** (extern 不要) | 既に純粋 Almide で実装済み |
| int, float | **hybrid** (大半 pure, 2-3 関数 extern) | `to_string`, `from_string` のみ extern |
| string | **hybrid** (大半 pure, ~10 関数 extern) | `len`, `slice`, `split` 等のみ extern |
| math | **hybrid** (大半 pure, ~10 関数 extern) | `sqrt`, `sin`, `cos` 等のみ extern |
| list | **hybrid** (~20 関数 extern) | クロージャ関数は暫定 extern (Rule 3) |
| map | **hybrid** (~5 関数 extern) | `new`, `get`, `set` のみ extern |
| json, regex | **extern** (~10 関数) | パーサー/エンジンがホスト依存 |
| fs, http, io, env, process, random, datetime | **extern** (全関数) | OS/ランタイムアクセス必須 |
| fs, http | **extern + variants** | Deno/Node/browser で API が異なる |

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
| ターゲット間の挙動一致 | 検証手段なし | Conformance test で保証 |

---

## Implementation Steps

成功確率を上げるため、小さいステップで一周ずつ回す。

### Step 1: @extern の最小核

対象: `io.print` 1 関数だけ。

確認すること:
- parser が `@extern fn` を認識するか
- checker が型シグネチャを処理するか
- lower が `IrFunction::Extern(module, func)` を生成するか
- codegen が `extern.rs` / `extern.ts` を include/import するか
- native test が回るか

まだ variant も scaffold もいらない。

### Step 2: バリアント解決（最小版）

`extern.ts` + `extern.node.ts` だけ入れる。version は後。

確認すること:
- `--runtime node` で `extern.node.ts` が選ばれるか
- 未指定で `extern.ts` に fallback するか

### Step 3: プラットフォームモジュールの移行

io → env → process → fs → http → random → datetime の順で移行。

@extern の価値が最大なモジュールから先にやる。

- `extern_test.rs` / `extern_test.ts` で各関数をテスト
- TOML 定義を削除
- fs / http は Deno/Node バリアントを分離

### Step 4: 純粋 Almide モジュールの確認

hash, csv, url, path, encoding, args — これらは既に .almd で動いているので、ディレクトリ構造を `stdlib/hash/mod.almd` に揃えるだけ。

### Step 5: ハイブリッドモジュールの移行

string, int, float, math, map, list, result — 大半を .almd で書き直し、@extern は最小限に。

- 型プリミティブ（len, get, to_string 等 ~10 関数）だけ @extern
- クロージャ関数（map, filter 等 ~20 関数）は暫定 @extern (Rule 3)
- パフォーマンス比較: 有意差がないことを確認

### Step 6: バージョン付きバリアント

`extern.node.22.ts` の解決ロジックを追加。

### Step 7: 生成コード撤去

- `stdlib/defs/*.toml` 全削除
- `build.rs` から stdlib 生成ロジックを削除
- `src/generated/` から stdlib 関連ファイルを削除
- `src/emit_rust/core_runtime.txt` 削除
- `src/emit_ts_runtime.rs` 削除

### Step 8: scaffold コマンド

`almide scaffold <module.almd>` で @extern 関数のスケルトンを自動生成。

### Future: ユーザー @extern 開放

- ユーザーが自分のパッケージで `@extern` を使えるようにする
- FFI 的な用途: Rust crate や npm パッケージを直接ラップ

---

## CI Integration

```yaml
# ネイティブ実装を Almide コンパイラと独立してテスト
extern-test-rust:
  name: Extern Tests (Rust)
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v4
    - run: cargo test --manifest-path stdlib/Cargo.toml

extern-test-deno:
  name: Extern Tests (Deno)
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v4
    - uses: denoland/setup-deno@v2
    - run: deno test stdlib/*/extern_test.ts

extern-test-node:
  name: Extern Tests (Node)
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v4
    - uses: actions/setup-node@v4
      with: { node-version: "22" }
    - run: npx tsx --test stdlib/*/extern_test.node.ts
```

---

## @extern 全リスト (~60 関数)

### Tier 1: 型プリミティブ (10)
```
string: len, char_at, slice, from_chars
list:   len, get, push, set
int:    to_string, from_string
```

### Tier 2: クロージャ最適化 — 暫定 (20)
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

---

## Success Criteria

- `almide test` が全テスト通過
- TOML 定義ファイル 0、build.rs に stdlib 生成コードなし
- `cargo test --manifest-path stdlib/Cargo.toml` でネイティブ Rust テスト通過
- `deno test stdlib/*/extern_test.ts` でネイティブ TS テスト通過
- バリアント解決が正しく動作
- @extern の契約がコンパイル時に検証される（存在確認）
- Rust/TS 間の挙動一致が conformance test で保証される

## Dependencies

- [IR Optimization Passes](ir-optimization.md) — Step 5 の判断に影響（for+append 最適化）
- [Codegen Refinement](codegen-refinement.md) — .almd 生成コードの品質

## Related

- [Stdlib API Surface Reform](stdlib-verb-system.md) — API 語彙の改革（別トラック）

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
src/cli.rs (add scaffold subcommand, --runtime/--runtime-version flags)
stdlib/_glue/result.rs (Result glue for Rust — thin)
stdlib/_glue/result.ts (Result glue for TS — value representation)
stdlib/_glue/types.rs (Option, Unit etc. for Rust)
stdlib/_glue/types.ts (Option, Unit etc. for TS)
stdlib/*/mod.almd (type signatures + pure Almide)
stdlib/*/extern.rs (Rust native implementations)
stdlib/*/extern.{variant}.rs (Rust variant implementations)
stdlib/*/extern_test.rs (Rust tests)
stdlib/*/extern.ts (TS native implementations)
stdlib/*/extern.{variant}.ts (TS variant implementations)
stdlib/*/extern_test.ts (TS tests)
stdlib/Cargo.toml (workspace for Rust extern tests)
```
