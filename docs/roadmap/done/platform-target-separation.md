# Platform / Target Separation [ON HOLD]

## Thesis

`--target` に出力形式とプラットフォームの 2 つの意味が混在している。これを分離する。

```bash
# 今: target にプラットフォームが混ざる
almide build app.almd --target ts-browser
almide build app.almd --target ts-node
almide build app.almd --target ts-worker

# あるべき姿: 直交する 2 軸
almide build app.almd --target ts --platform browser
almide build app.almd --target ts --platform node
almide build app.almd --target ts --platform worker
almide build app.almd --target rust --platform native
```

- **target** = 出力言語 (codegen の選択): `rust`, `ts`, `wasm`
- **platform** = 使える API の集合 (`@extern` の可用性): `browser`, `node`, `worker`, `native`

## Why Separate

### 問題: target にプラットフォームを混ぜると爆発する

target × platform の組み合わせが全部 `--target` のバリアントになる:

```
ts-browser, ts-node, ts-worker, ts-deno,
rust-native, rust-wasm,
wasm-browser, wasm-worker, ...
```

新しいプラットフォームが増えるたびに全 target に組み合わせが必要。スケールしない。

### 解決: 直交する 2 軸

```
target (codegen)     platform (API 可用性)
├── rust              ├── native
├── ts                ├── node
└── wasm              ├── browser
                      └── worker
```

target と platform は独立に選べる。組み合わせの妥当性はコンパイラが検証する:

| target | platform | 妥当性 |
|---|---|---|
| ts | browser | OK — DOM API 使用可能 |
| ts | node | OK — fs, process 使用可能 |
| ts | worker | OK — fetch, KV 使用可能 |
| ts | native | NG — TS は native ランタイムがない |
| rust | native | OK — std::fs, std::process 使用可能 |
| rust | browser | NG (wasm 経由なら可能、将来の話) |
| wasm | browser | OK — Web API + WASM import |
| wasm | worker | OK — WASM Workers |

## Platform Hierarchy

プラットフォームは包含関係を持つ階層構造:

```
any                  ← JSON, Math, Array, 基本型操作
├── web              ← Web standard API (fetch, URL, Request/Response, crypto.subtle)
│   ├── browser      ← DOM (document, window, navigator, localStorage)
│   └── worker       ← Edge-specific (KV, D1, env bindings, etc.)
├── node             ← Node.js API (fs, child_process, Buffer, path, etc.)
└── native           ← Rust std (std::fs, std::process, std::net, etc.)
```

**包含ルール:**
- `browser` の関数は `web` と `any` の関数も使える
- `worker` の関数は `web` と `any` の関数も使える
- `node` の関数は `any` の関数のみ使える (`web` は含まない)
- `native` の関数は `any` の関数のみ使える

`web` が `browser` と `worker` の共通基盤になる。Web standard Fetch API, URL, crypto.subtle 等は `web` に属し、どちらのプラットフォームでも使える。

## @extern の設計

### 現在

```almide
@extern(rs, "std::cmp", "min")
@extern(ts, "Math", "max")
fn my_max(a: Int, b: Int) -> Int
```

`@extern` の第 1 引数が target 言語。プラットフォームの概念がない。

### 新設計

```almide
// プラットフォーム能力に紐づく
@extern(platform: any, "JSON", "parse")
fn parse_json(s: String) -> Value

@extern(platform: web, "fetch")
effect fn fetch(url: String) -> Response

@extern(platform: browser, "document", "createElement")
fn create_element(tag: String) -> DomNode

@extern(platform: node, "fs", "readFileSync")
fn read_file(path: String) -> String

@extern(platform: native, "std::fs", "read_to_string")
fn read_file(path: String) -> String
```

### target と platform の交差

同じ関数に target 別の実装 + platform 制約を持てる:

```almide
// fs.read_file の定義 (stdlib 内部)
@extern(platform: node, "fs", "readFileSync")
@extern(platform: native, "std::fs", "read_to_string")
effect fn read_file(path: String) -> String
```

- `--target ts --platform node` → `fs.readFileSync` を使用
- `--target rust --platform native` → `std::fs::read_to_string` を使用
- `--target ts --platform browser` → コンパイルエラー: `read_file requires platform node or native`

### 後方互換

既存の `@extern(rs, ...)` / `@extern(ts, ...)` は以下のショートハンドとして維持:

```almide
@extern(rs, ...)  →  @extern(target: rust, platform: native, ...)
@extern(ts, ...)  →  @extern(target: ts, platform: any, ...)
```

既存コードは変更不要。

## Platform の推論

### ライブラリ — 宣言不要、使用から推論

ライブラリは platform を明示宣言しない。使っている `@extern` から必要な最低限の platform が自動推論される:

```almide
// my_lib.almd
import dom exposing (create_element, append)  // browser API を使用

fn render(text: String) -> DomNode = {
  let el = create_element("p")
  // ...
  el
}
```

コンパイラ推論: `my_lib` は `create_element` (@extern platform: browser) を使用 → **platform: browser が必要**

```bash
# ユーザーがこのライブラリを使う
almide build app.almd --target ts --platform node
# error: my_lib requires platform browser, but target platform is node
#   --> app.almd:1:1
#    |
#  1 | import my_lib
#    | ^^^^^^^^^^^^^^ my_lib uses dom.create_element (platform: browser)
#    |
#    = hint: use --platform browser, or remove the import
```

### アプリケーション — --platform で指定

```bash
almide build app.almd --target ts --platform browser   # 明示指定
almide build app.almd --target ts                      # デフォルト: node (後方互換)
almide build app.almd --target rust                    # デフォルト: native
```

### almide.toml でのデフォルト指定

```toml
[build]
target = "ts"
platform = "worker"
```

プロジェクトごとにデフォルトを固定できる。CI やチームでの統一に使う。

## stdlib への影響

各 stdlib モジュールの関数が適切な platform タグを持つ:

| モジュール | platform | 理由 |
|---|---|---|
| string, list, map, int, float | `any` | 純粋な計算 |
| math, random | `any` | JS Math / Rust std::f64 |
| json | `any` | JSON.parse は全ランタイム共通 |
| regex | `any` | JS RegExp / Rust regex crate |
| crypto (hash) | `any` | 基本的なハッシュは全ランタイム |
| crypto (subtle) | `web` | Web Crypto API |
| http (fetch) | `web` | Web standard Fetch API |
| http (server) | `node` | Node.js http module / Deno.serve |
| fs, path | `node` / `native` | ファイルシステム |
| process (spawn) | `node` / `native` | 子プロセス |
| env (get) | `any` | Deno.env, process.env, std::env 全対応 |
| env (set) | `node` / `native` | browser/worker では不可 |
| dom | `browser` | DOM API |
| datetime | `any` | Date / chrono |
| log | `any` | console.log / eprintln |

**1 モジュール内で関数ごとに platform が異なりうる。** `env.get` は `any` だが `env.set` は `node`。これは関数単位の `@extern` タグで自然に表現される。

## コンパイルエラーの設計

### 使えない API の呼び出し

```
error: fs.read_file requires platform node or native, but target platform is browser
  --> app.almd:5:3
   |
 5 |   let data = fs.read_file("config.json")
   |              ^^^^^^^^^^^^^
   |
   = hint: filesystem is not available in browser. Consider using fetch() to load data from a URL
```

### platform 不一致のインポート

```
error: cannot import server_lib (requires platform node) in platform worker
  --> app.almd:1:1
   |
 1 | import server_lib
   | ^^^^^^^^^^^^^^^^^
   |
   = note: server_lib uses fs.read_file (platform: node)
   = note: server_lib uses process.spawn (platform: node)
   = hint: use --platform node, or use a browser-compatible alternative
```

### target × platform の不正な組み合わせ

```
error: platform native is not compatible with target ts
  --> almide.toml:3:1
   |
 3 | platform = "native"
   | ^^^^^^^^^^^^^^^^^^^
   |
   = hint: use --target rust for native platform, or --platform node for ts target
```

## Relationship to Other Roadmap Items

- **ts-edge-native.md**: このドキュメントが解決する問題の前提。platform 分離がないとエッジで使える API がコンパイル時に分からない。ts-edge-native の Phase 2 は本ドキュメントに移管
- **almide-ui.md**: Almide UI は `platform: browser` に依存。platform 推論により、Almide UI を使ったコードは自動的に browser platform が要求される
- **cross-target-semantics.md**: platform 分離により「同じコードが Rust と TS で同じ結果」の検証スコープが `platform: any` の関数に限定できる
- **rainbow-gate.md**: Rainbow FFI Gate は本質的に platform-specific。@extern の platform タグが FFI の基盤になる

## Why ON HOLD

現在の `@extern(rs, ...)` / `@extern(ts, ...)` で言語コアの開発には十分。platform 分離が必要になるのは TS ターゲットでの本格的な Web/Edge 開発が始まるとき。

ただし:

- **設計はシンプル** — @extern に platform フィールドを追加し、コンパイラが階層を見て検証するだけ
- **後方互換** — 既存の `@extern(rs, ...)` / `@extern(ts, ...)` はショートハンドとして維持
- **段階的に導入可能** — まず `any` / `node` / `browser` の 3 つだけで始められる

ts-edge-native / almide-ui が動き出すときに必要になる。
