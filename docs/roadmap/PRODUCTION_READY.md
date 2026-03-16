# Production Ready Criteria

Almide が「プロダクションレディ」と宣言するための定量指標。全項目を満たした時点で Semver 1.0 をリリースする。

---

## Vision

Almide は「LLM が最も正確に書ける言語」。単一の `.almd` ソースから Rust, TypeScript, Go, Python, C, WASM に出力し、async/await を書かずに並行処理ができ、pure fn の Effect Isolation で supply chain attack を構造的に防ぐ。

1.0 は「LLM エージェントが Almide だけで CLI ツール・Web API・データパイプラインを書き、デプロイできる」状態を目指す。

---

## 現在地: v0.6.0

```
コンパイラ          84 ファイル / 19,536 行 (pure Rust, 外部 crate ゼロ)
stdlib             22 モジュール / 355 関数 / ランタイム 100%
テスト             2,033+ (96 .almd + 714 Rust unit)
ターゲット          Rust, TypeScript, JavaScript, npm package, WASM
Exercises          25 本 / 6 tiers
並行処理           fan { }, fan.map, fan.race, fan.any, fan.settle, fan.timeout
セキュリティ       Effect Isolation (Layer 1) — pure fn → effect fn 禁止
Codec              auto-derive encode/decode, Value 型, JSON roundtrip
IR                 Typed IR + constant folding, dead code elimination
Borrow             use-count ベースの clone 挿入/削除
診断               file:line + context + actionable hint + error recovery
```

---

## 1. コンパイラの正確性

| 指標 | 基準 | v0.6.0 | Gap |
|------|------|--------|-----|
| 型健全性違反 | 0 件 | ほぼ 0 | Unknown 伝播の残存ケース |
| クロスターゲット不一致 | 0 件 | 未計測 | CI 仕組み必要 |
| 生成 Rust の rustc エラー | 0 件 | exercises 25 本通過 | ほぼ達成 |
| 生成 TS の tsc エラー | 0 件 | npm target 生成可能 | ほぼ達成 |
| ICE | 0 件 | panic ゼロ目標 | 継続改善 |

## 2. テストカバレッジ

| 指標 | 基準 | v0.6.0 | Gap |
|------|------|--------|-----|
| 総テスト数 | 2,500+ | **2,033** | +467 |
| 言語テスト | — | 644 (lang) + 143 (integration) | — |
| stdlib テスト | 355 関数 × 1+ | 334 テスト (**94%**) | +21 |
| コンパイラ単体テスト | 800+ | **714** | +86 |
| クロスターゲット通過率 | 100% | 未計測 | CI 構築 |
| エッジケース | 空, NaN, i64 境界, 再帰 | 部分的 | 体系化 |

## 3. 標準ライブラリ

| 指標 | 基準 | v0.6.0 | Gap |
|------|------|--------|-----|
| モジュール数 | 38+ | **22** | +16 (csv, toml, url, html, set, sorted, ...) |
| 関数数 | 700+ | **355** | +345 |
| ランタイム実装率 | 100% | **100%** ✅ | 達成 |
| description カバレッジ | 100% | 高 | 微調整 |
| regex | 外部 crate 不要 | **自前 350 行エンジン** ✅ | 達成 |

## 4. 言語機能の成熟度

| 機能 | v0.1.0 | v0.6.0 | 1.0 目標 |
|------|--------|--------|----------|
| 型システム | Int, String, Bool | Generics, Record, Variant, Option, Result, Union | 安定 |
| エラー処理 | なし | effect fn, auto-?, do block, guard | 安定 |
| 並行処理 | なし | **fan ファミリー 6 API** | + async backend opt-in |
| セキュリティ | なし | **Effect Isolation (Layer 1)** | + Capability (Layer 2-3) |
| Codec | なし | auto-derive, Value, JSON roundtrip | + TOML, YAML, CSV |
| マルチターゲット | Rust のみ | **Rust + TS + JS + npm + WASM** | + Go, Python |
| FFI | なし | なし | Rainbow Bridge/Gate |
| パッケージ管理 | なし | almide.toml | + lock ファイル + registry |
| 自己ホスティング | なし | なし | Almide で Almide を書く |

## 5. エコシステム

| 指標 | 基準 | v0.6.0 | 依存 |
|------|------|--------|------|
| lock ファイル | あり | なし | パッケージシステム |
| LSP | diagnostics + hover + go-to-def | なし | 独立 |
| doc 生成 | `almide doc` → HTML | なし | 独立 |
| FFI | Rust crate / npm 呼び出し | なし | Rainbow Bridge |
| パッケージレジストリ | あり | なし | lock + registry |
| Almide Shell | AI-native REPL | なし | IR Interpreter |
| Web Framework | Hono 相当 | なし | Server Async |

## 6. LLM 適性（Almide の存在理由）

| 指標 | 基準 | v0.6.0 | 計測基盤 |
|------|------|--------|---------|
| Modification Survival Rate | 85%+ | 未計測 | Grammar Lab あり |
| エラー自動修復率 | 70%+ | 未計測 | hint system 実装済み |
| 初回正答率 | 80%+ | 未計測 | exercises 25 本がベンチ候補 |
| LLM → IR 直接生成 | 動作 | 未着手 | IR JSON schema あり |

---

## チェックリスト

```
Production Ready = ALL of:

コンパイラ正確性
  ■ 型健全性違反 ≈ 0
  □ クロスターゲット不一致 = 0
  ■ 生成 Rust/TS コンパイル通過

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

リリース
  □ Semver 1.0 宣言

■ = 達成 (4/12)   □ = 未達 (8/12)
```

## 1.0 への道

### Phase I: テスト + stdlib 拡充 (量)
- テスト +467 → 2,500+
- stdlib +16 モジュール / +345 関数 → 38 / 700+
- クロスターゲット CI 構築

### Phase II: エコシステム基盤
- lock ファイル + 依存解決
- LSP (diagnostics → hover → go-to-def)
- `almide doc` 生成

### Phase III: LLM 計測 + 最適化
- Grammar Lab で MSR 計測開始
- exercises ベースの初回正答率ベンチマーク
- hint system の修復率ベンチマーク

### Phase IV: 拡張ターゲット + FFI
- Go / Python codegen
- Rainbow Bridge (外部コード → Almide パッケージ)
- Rainbow Gate (Almide → 外部言語ライブラリ)

### Beyond 1.0
- Almide Shell (AI-native REPL)
- Self-Hosting (Almide で Almide を書く)
- Security Layer 2-5 (Capability, Package Boundary, Sandbox, Supply Chain)
- Async Backend (tokio opt-in)
- Streaming (WebSocket, SSE)
- LLM → IR 直接生成 (parser bypass)
