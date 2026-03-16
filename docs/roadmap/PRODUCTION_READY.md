# Production Ready Criteria

Almide が「プロダクションレディ」と宣言するための定量指標。全項目を満たした時点で Semver 1.0 をリリースする。

---

## Vision

Almide は「LLM が最も正確に書ける言語」。

単一の `.almd` ソースから Rust, TypeScript, WASM に出力し、async/await を書かずに `fan` で並行処理ができ、`effect fn` の Effect Isolation で supply chain attack を構造的に防ぐ。

LLM が犯す典型的なミス（await 忘れ、型の不一致、可変状態の競合）を文法レベルで構造的に不可能にする — これが他の言語との決定的な差。

1.0 は「LLM エージェントが Almide だけで CLI ツール・Web API・データパイプラインを書き、テストし、デプロイできる」状態。

---

## 現在地: v0.6.0

```
コンパイラ          84 ファイル / 19,536 行
                    生成コードは外部 crate 不要（stdlib ランタイムを自己内包）
stdlib             22 モジュール / 355 関数 / ランタイム 100%
テスト             2,033+ (96 .almd ファイル + 714 Rust unit tests)
ターゲット          Rust, TypeScript, JavaScript, npm package, WASM
Exercises          25 本 / 6 tiers
並行処理           fan { }, fan.map, fan.race, fan.any, fan.settle, fan.timeout
セキュリティ       Effect Isolation (Layer 1) — pure fn は effect fn を呼べない
Codec              auto-derive encode/decode, Value 型, JSON roundtrip
IR                 Typed IR + constant folding, dead code elimination
Borrow             use-count ベースの clone 挿入/削除
診断               file:line + context + actionable hint + error recovery
```

---

## v0.1.0 → v0.6.0: 何が変わったか

| 機能 | v0.1.0 | v0.6.0 |
|------|--------|--------|
| 型システム | Int, String, Bool | + Float, List, Map, Tuple, Record, Variant, Option, Result, Generics, Union |
| エラー処理 | なし | effect fn, auto-?, do block, guard |
| 並行処理 | なし | fan ファミリー 6 API (fan, map, race, any, settle, timeout) |
| セキュリティ | なし | Effect Isolation — pure fn → effect fn 呼び出しコンパイルエラー |
| Codec | なし | auto-derive encode/decode, Value 型, JSON roundtrip |
| ターゲット | Rust のみ | Rust + TypeScript + JS + npm + WASM |
| パターンマッチ | 基本 | 網羅性チェック、ネスト、ガード、Record/Variant destructure |
| 診断 | 行番号のみ | file:line + context + hint + error recovery (複数エラー同時報告) |
| テスト | 0 | 2,033+ |
| stdlib | 数関数 | 22 モジュール / 355 関数 / ランタイム 100% |
| ツール | `almide run` のみ | run, build, test, check, fmt, clean, init |
| IR | なし | Typed IR + 最適化パス |
| Borrow | なし | use-count 分析 + clone 自動挿入/削除 |
| フォーマッタ | なし | `almide fmt` |
| モジュール | なし | import / module / almide.toml |

---

## 1.0 基準

### 1. コンパイラの正確性

| 指標 | 基準 | v0.6.0 | Gap |
|------|------|--------|-----|
| 型健全性違反 | 0 件 | 残存ケースあり | Unknown 伝播の hardening |
| クロスターゲット不一致 | 0 件 | 未計測 | CI 仕組み必要 |
| 生成 Rust の rustc エラー | 0 件 | exercises 25 本通過 | ほぼ達成 |
| 生成 TS の tsc エラー | 0 件 | npm target 生成可能 | ほぼ達成 |
| ICE | 0 件 | panic ゼロ目標 | 継続改善 |
| Borrow/Clone 正確性 | 全パターン正常 | 一部ケースで clone 漏れ | active で修正中 |

### 2. テストカバレッジ

| 指標 | 基準 | v0.6.0 | Gap |
|------|------|--------|-----|
| 総テスト数 | 2,500+ | **2,033** | +467 |
| 言語テスト (spec/lang/) | — | 644 | — |
| stdlib テスト (spec/stdlib/) | — | 334 (**94%** の関数をカバー) | +21 |
| 統合テスト (spec/integration/) | — | 143 | — |
| コンパイラ単体テスト (Rust) | 800+ | **714** | +86 |
| クロスターゲット通過率 | 100% | 未計測 | CI 構築 |

### 3. 標準ライブラリ

| 指標 | 基準 | v0.6.0 | Gap |
|------|------|--------|-----|
| モジュール数 | 38+ | **22** | +16 (csv, toml, url, html, set, sorted 等) |
| 関数数 | 700+ | **355** | +345 |
| ランタイム実装率 | 100% | **100%** ✅ | 達成 |
| regex | 外部 crate 不要 | **自前 350 行エンジン** ✅ | 達成 |

### 4. エコシステム (1.0 必須)

| 指標 | 基準 | v0.6.0 |
|------|------|--------|
| lock ファイル | `almide.lock` で再現性保証 | なし |
| LSP | diagnostics + hover + go-to-def | なし |
| FFI | 最低 1 ターゲット (Rust crate 呼び出し) | なし |

### 5. LLM 適性（Almide の存在理由）

| 指標 | 基準 | v0.6.0 | 計測基盤 |
|------|------|--------|---------|
| Modification Survival Rate | 85%+ | 未計測 | Grammar Lab あり |
| エラー自動修復率 | 70%+ | 未計測 | hint system 実装済み |
| 初回正答率 | 80%+ | 未計測 | exercises 25 本がベンチ候補 |

---

## チェックリスト

```
Production Ready = ALL of:

コンパイラ
  □ 型健全性違反 = 0
  □ クロスターゲット不一致 = 0
  □ Borrow/Clone 全パターン正常
  ■ 生成コードが rustc/tsc 通過

テスト
  □ テスト 2,500+                        (2,033 — あと 467)
  ■ stdlib ランタイム 100%               ✅

標準ライブラリ
  □ 38+ モジュール / 700+ 関数           (22 / 355)

エコシステム
  □ lock ファイル
  □ LSP
  □ FFI

LLM
  □ Modification Survival Rate 85%+
  □ 自動修復率 70%+

■ = 達成 (2/12)   □ = 未達 (10/12)
```

---

## 1.0 への道

### Phase I: 正確性 + テスト
- Borrow/Clone gaps の修正（active）
- テスト +467 → 2,500+
- クロスターゲット CI 構築
- Unknown 伝播 hardening

### Phase II: stdlib 拡充
- +16 モジュール / +345 関数 → 38 / 700+
- Verb 標準化 (active: stdlib-verb-system)
- 全関数テスト + description 100%

### Phase III: エコシステム基盤
- lock ファイル + 依存解決
- LSP (diagnostics → hover → go-to-def)
- FFI (Rainbow Bridge — 外部コードを Almide パッケージ化)

### Phase IV: LLM 計測
- Grammar Lab で MSR 計測開始
- exercises ベースの初回正答率ベンチマーク
- hint system の修復率ベンチマーク

---

## Beyond 1.0

| 項目 | roadmap |
|------|---------|
| Go / Python codegen | on-hold/new-codegen-targets.md |
| Almide Shell (AI-native REPL) | on-hold/almide-shell.md |
| Self-Hosting | on-hold/self-hosting.md |
| Security Layer 2-5 | active/security-model.md |
| Async Backend (tokio opt-in) | on-hold/async-backend.md |
| Streaming (WebSocket, SSE) | on-hold/streaming.md |
| LLM → IR 直接生成 | on-hold/llm-ir-generation.md |
| Web Framework | on-hold/web-framework.md |
| Almide UI | on-hold/almide-ui.md |
| パッケージレジストリ | on-hold/package-registry.md |
