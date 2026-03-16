# TS Edge-Native Deployment [ON HOLD]

## Thesis

Almide の `--target ts` 出力は **素の TypeScript/JavaScript** であり、V8 が直接実行する。WASM を経由しない。これにより Cloudflare Workers, Deno Deploy, Vercel Edge Functions 等のエッジランタイムで、WASM ベースの言語（Rust→WASM, Go→WASM, MoonBit）が抱える問題を根本的に回避できる。

```
Rust → WASM → V8 (WASM instantiate 5-50ms, no JIT, FFI overhead)
Almide → TS → V8 (JS parse <1ms, full JIT, native ecosystem)
```

**これは「Almide はエッジで最速の非-JS 言語」になれる可能性を意味する。**

## Why This Matters

### WASM on Edge の現実的な問題

| 問題 | 原因 | Almide→TS での状況 |
|------|------|-------------------|
| コールドスタート遅延 | WASM instantiate + メモリ確保 (5-50ms) | JS パース <1ms。問題なし |
| JIT 最適化なし | WASM は AOT、V8 の TurboFan が効かない | フル JIT 最適化対象 |
| JS エコシステム断絶 | WASM↔JS 間の FFI オーバーヘッド | ネイティブ JS。npm パッケージ直接利用可能 |
| バンドルサイズ | WASM バイナリ 数百KB〜MB | 45-100KB (改善余地あり) |
| デバッグ困難 | WASM のスタックトレースは不透明 | 生成 TS は可読。ソースマップも原理的に可能 |

### Almide が持つ構造的優位性

1. **型チェッカーが全型情報を持っている** — emitter は型に基づいて最適なコードを選択できる
2. **マルチターゲット** — 同じコードが `--target rust` でネイティブバイナリにもなる。サーバーは Rust、エッジは TS、という使い分けが 1 言語で完結する
3. **出力は改善可能** — ランタイムのオーバーヘッドはアーキテクチャの制約ではなく、emitter 最適化の問題。型情報があるので後からいくらでも絞れる

## Current State

### 動くもの

- `--target ts` で Deno 向け TypeScript を出力 (動作中)
- `--target npm` で npm パッケージを出力 (動作中、モジュール選択的読み込み)
- Result erasure: `ok(x)` → `x`, `err(e)` → `throw` (TS-idiomatic)
- stdlib 22 モジュール (string, list, map, json, http, fs, crypto, etc.)

### 最適化余地

型チェッカーが型を知っているので、emitter 側で以下が可能:

| 現状 | 最適化後 | 条件 |
|------|---------|------|
| `__deep_eq(a, b)` | `a === b` | 両辺がプリミティブ型 (Int, String, Bool) |
| `__bigop("%", n, 3)` | `n % 3` | 両辺が Int かつ BigInt 不要な範囲 |
| `__bigop("+", a, b)` | `a + b` | 同上 |
| `__div(a, b)` | `Math.trunc(a / b)` or `a / b` | Int 除算 or Float 除算が型で判明 |
| `__concat(a, b)` | `a + b` | 両辺が String |
| stdlib 全モジュール埋め込み | 使用モジュールのみ | `--target ts` でも npm 同様の tree-shake |

**これらは全て emitter の変更で済む。言語仕様やランタイムの変更は不要。**

## Edge Platform Compatibility

| Platform | Runtime | Almide→TS の互換性 |
|----------|---------|-------------------|
| Cloudflare Workers | V8 isolate | 完全互換。スクリプトサイズ上限 Free 1MB / Paid 10MB → 余裕 |
| Deno Deploy | Deno (V8) | `--target ts` がそのまま動く。現在のメインターゲット |
| Vercel Edge Functions | V8 (Edge Runtime) | ESM 互換。npm target で対応可能 |
| AWS Lambda@Edge | Node.js | `--target npm` で対応 |
| Fastly Compute | WASM のみ | 非対応 (WASM target が必要) |

## What Needs to Happen

### Phase 1: TS 出力の軽量化 (emitter 最適化)

型情報に基づくヘルパー関数の除去。上記「最適化余地」の表を実装する。

- [ ] プリミティブ型の `==`/`!=` → `===`/`!==` 直接出力
- [ ] プリミティブ型の算術演算 → 直接出力 (BigInt ディスパッチ除去)
- [ ] `--target ts` での未使用 stdlib モジュール除去 (npm 同様の tree-shake)
- [ ] 計測: 最適化前後のバンドルサイズ・実行速度比較

### Phase 2: JS Platform Target の細分化

現在の `@extern(ts, ...)` は JS ランタイムを区別しない。`document.createElement` (ブラウザ専用) と `fs.readFileSync` (Node.js 専用) が同じ `ts` ターゲットに混在するため、「コンパイル通るがランタイムで死ぬ」問題が起きる。

#### ターゲット階層

```
js                      ← 全 JS ランタイム共通 (JSON, Math, Array, etc.)
├── js-web              ← Web standard API (fetch, Request/Response, URL, crypto.subtle)
│   ├── js-browser      ← ブラウザ固有 (DOM, window, navigator, localStorage)
│   └── js-worker       ← Edge Worker 固有 (Cloudflare KV, Deno.env, etc.)
└── js-node             ← Node.js 固有 (fs, child_process, Buffer, etc.)
```

- `js-web` は browser, Deno, Cloudflare Workers で共通。Web standard Fetch API, URL, crypto.subtle 等
- `js-browser` は DOM API。Workers/Node.js では使えない
- `js-worker` はエッジ固有 API (KV, D1 等)。ベンダーごとに分ける可能性あり
- `js-node` は Node.js 固有。fs, child_process, Buffer 等

#### @extern の使い分け

```almide
// 全 JS ランタイムで動く
@extern(js, "JSON", "parse")
fn parse_json(s: String) -> Value

// Web standard — browser + Deno + Workers で共通
@extern(js-web, "fetch")
effect fn fetch(url: String) -> Response

// ブラウザ専用 — Workers/Node.js ではコンパイルエラー
@extern(js-browser, "document", "createElement")
fn create_element(tag: String) -> DomNode

// Node.js 専用 — browser/Workers ではコンパイルエラー
@extern(js-node, "fs", "readFileSync")
fn read_file(path: String) -> String
```

#### コンパイル時プラットフォーム検証

`--target ts-browser` / `--target ts-node` / `--target ts-worker` でプラットフォームが確定。使えない `@extern` を参照するとコンパイルエラー:

```
error: fs.read_file requires js-node, but target is js-browser
  --> app.almd:5:3
   |
 5 |   let data = read_file("config.json")
   |              ^^^^^^^^^
   |
   = hint: fs module is not available in browser target
```

#### stdlib への影響

stdlib の各モジュールが適切なプラットフォームタグを持つ:

| stdlib モジュール | プラットフォーム | 理由 |
|---|---|---|
| string, list, map, int, float, math | `js` | 純粋な計算。全ランタイムで動く |
| json, regex, crypto (hash) | `js` | 標準 JS API のみ使用 |
| http (fetch) | `js-web` | Web standard Fetch API |
| fs, path | `js-node` | Node.js fs API |
| process (spawn) | `js-node` | child_process |
| env | `js` (基本) / `js-node` (一部) | `env.get` は全ランタイム、`env.set` は Node.js のみ |
| dom (Almide UI) | `js-browser` | DOM API |

これにより「エッジで動く stdlib」と「動かない stdlib」がコンパイル時に確定する。

#### 実装

- [ ] `@extern` のターゲットを `rs` / `ts` から `rs` / `js` / `js-web` / `js-browser` / `js-node` / `js-worker` に拡張
- [ ] `--target` フラグの細分化: `ts-browser`, `ts-node`, `ts-worker` (既存の `ts` は `ts-node` のエイリアス)
- [ ] コンパイル時プラットフォーム検証: 不適切な `@extern` 参照をエラーにする
- [ ] stdlib の各 `@extern` にプラットフォームタグを付与
- [ ] 診断メッセージ: どのプラットフォームで使えるかを hint で表示

### Phase 3: Edge 向けエントリポイント

HTTP ハンドラのパターンを Almide で自然に書けるようにする。

```almide
// Cloudflare Workers 向けの最小例
effect fn handle(req: Request) -> Response =
  match req.method {
    "GET" => Response.text("Hello from Almide"),
    _ => Response.text("Method not allowed", status: 405),
  }
```

- [ ] `Request`/`Response` 型の定義 (Web standard Fetch API 準拠 — `@extern(js-web, ...)`)
- [ ] `export default { fetch: handle }` 形式の出力
- [ ] Cloudflare Workers / Deno Deploy / Vercel Edge 向けのエントリポイントテンプレート

### Phase 4: ベンチマーク & 実証

「WASM より速い」を数値で証明する。

- [ ] Cloudflare Workers 上で同一ロジックの cold start 比較: Almide→TS vs Rust→WASM
- [ ] 実行速度比較: JSON parse/serialize, HTTP routing, string processing
- [ ] バンドルサイズ比較

## Relationship to Other Roadmap Items

- **almide-ui.md**: Almide 製リアクティブ UI フレームワーク。TS edge-native の emitter 最適化 + builder 機構の上に構築される。このドキュメントの Phase 1 が Almide UI の性能基盤
- **emit-wasm-direct.md**: WASM 直接出力とは独立。TS edge-native は WASM を使わないことが価値
- **cross-target-semantics.md**: TS 出力の正確性はこのドキュメントの前提。P0 の修正は必須
- **Result Builder (template.md)**: HTML builder + TS edge 出力 = Almide で書いた Web アプリをエッジで動かす完全なストーリーになる

## Why ON HOLD

現時点では emitter 最適化 (Phase 1) が先決。言語コアの安定化と既存テストの充実が優先。ただし:

- **アーキテクチャ上のブロッカーはゼロ** — 必要なものは全て emitter 側の改善
- **型チェッカーが型情報を持っている** — 最適化に必要な基盤は既にある
- **`--target ts` は既に動いている** — ゼロからではない

確度は高い。タイミングの問題。
