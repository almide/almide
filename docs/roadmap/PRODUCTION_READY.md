# Production Ready Criteria

Almide が「プロダクションレディ」と宣言するための定量指標。全項目を満たした時点で Semver 1.0 をリリースする。

---

## 1. コンパイラの正確性

| 指標 | 基準 | 計測方法 |
|------|------|---------|
| 型健全性違反 | 0 件 | `Unknown` が codegen に到達する内部 assert → CI 全通過 |
| クロスターゲット不一致 | 0 件 | `almide test --target rust` と `--target ts` の結果差分 = 0 |
| 生成 Rust の rustc エラー | 0 件 | 型チェック通過 → 生成 .rs が rustc で通る（CI） |
| 生成 TS の tsc エラー | 0 件 | 同上、TS ターゲット |
| ICE (Internal Compiler Error) | 0 件 | panic/unwrap ゼロ。全て診断メッセージで返す |

## 2. テストカバレッジ

| 指標 | 基準 | 現状 (v0.5.13) |
|------|------|----------------|
| 言語テスト数 | 2,500+ | 1,700+ |
| コンパイラ単体テスト | 800+ | 567 |
| クロスターゲットテスト通過率 | 100% | 未計測（仕組みなし） |
| stdlib 関数テストカバレッジ | 100%（全 282 関数に最低 1 テスト） | 未計測 |
| エッジケーステスト | 空文字列, 空リスト, NaN, i64 境界, 再帰 100 段 | 未整備 |

## 3. 生成コードの品質

| 指標 | 基準 | 計測方法 |
|------|------|---------|
| 不要 clone 率 | borrow 分析で除去可能な `.clone()` が 10% 未満 | サンプルプログラム群で計測 |
| rustc warning | 0 件 | `#![deny(warnings)]` で exercises/ 全通過 |
| n-body ベンチマーク | 手書き Rust の 1.05x 以内 | CI で計測 |

## 4. Code Quality (codopsy)

| Metric | Target | Current (v0.5.13) |
|--------|--------|-------------------|
| Quality score | B (75+) | D (49) |
| Max cognitive complexity per function | ≤ 30 | 264 (check_expr_inner) |
| Max cyclomatic complexity per function | ≤ 30 | 220 (check_expr_inner) |
| Max file length | ≤ 300 lines | 1017 (check/mod.rs) |
| Total warnings | 0 | 139 |
| .unwrap() in library code | 0 | 3 |

Baseline: `.codopsy-baseline.json`. Run `codopsy analyze src/ --no-degradation --baseline-path .codopsy-baseline.json` to detect regressions.

## 5. エコシステム

| 指標 | 基準 | 現状 |
|------|------|------|
| lock ファイル | あり（`almide.lock`） | なし |
| 依存解決の再現性 | 同じ almide.lock → 同じビルド結果 | なし |
| LSP 基本機能 | diagnostics + hover + go-to-def | なし |
| doc コメント | `///` → `almide doc` で HTML 生成 | なし |
| FFI | 最低 1 ターゲット（Rust crate 呼び出し） | なし |

## 5. 標準ライブラリ

| 指標 | 基準 | 現状 (v0.5.13) |
|------|------|----------------|
| stdlib モジュール数 | 38+ | 21 (15 TOML + 6 bundled) |
| stdlib 関数数 | 700+ | ~282 |
| Tier 1 完了 (datetime, fs/http 拡充, error) | 全モジュール実装済み | なし |
| Tier 2 完了 (csv, toml, url, crypto, uuid) | 全モジュール実装済み | なし |
| 全関数に description | TOML 定義に `description` フィールド 100% | なし |
| 全関数にテスト | spec/stdlib/ で全関数に最低 1 テスト | 未計測 |

詳細: [Stdlib Strategy](active/stdlib-strategy.md)

## 6. 安定性

| 指標 | 基準 | 計測方法 |
|------|------|---------|
| breaking change | 1.0 宣言後はゼロ | CHANGELOG + CI で既存テスト全通過 |
| 言語仕様カバー率 | コア構文の 100% が spec/lang/ にテストあり | spec カバレッジ計測 |
| deprecation 期間 | 最低 2 マイナーバージョン | ポリシー文書 |

## 7. LLM 適性（Almide 固有）

| 指標 | 基準 | 計測方法 |
|------|------|---------|
| Modification Survival Rate | 85%+ | Grammar Lab A/B テスト基盤で継続計測 |
| エラーからの自動修復率 | 70%+（エラー + hint だけで LLM が修正できる率） | LLM にエラーを渡して修正させるベンチマーク |
| 初回正答率 | 80%+（LLM が一発で正しく書ける率） | 定型タスクセットで計測 |

---

## チェックリスト

```
Production Ready = ALL of:
  □ 型健全性違反 = 0
  □ クロスターゲット不一致 = 0
  □ ICE = 0
  □ rustc/tsc warning = 0
  □ テスト 2,500+（うちクロスターゲット 100% 通過）
  □ stdlib 38+ モジュール, 700+ 関数
  □ stdlib 全関数テスト済み + description 100%
  □ lock ファイル実装済み
  □ LSP diagnostics + hover + go-to-def
  □ FFI（Rust crate 最低限）
  □ Semver 1.0 宣言
  □ Modification Survival Rate 85%+
  □ LLM 自動修復率 70%+
```

## 依存関係

```
Phase 0 (基盤)     → 正確性指標の達成
Phase A (コード品質) → 生成コード品質指標の達成
Phase B (型安全性)  → テストカバレッジ・安定性指標の達成
Stdlib 拡充        → 標準ライブラリ指標の達成 (282 → 700+)
エコシステム整備    → lock, LSP, doc, FFI の達成
LLM ベンチマーク構築 → LLM 適性指標の計測開始
```

前半 6 項目は Phase 0〜B で到達可能。後半 6 項目はエコシステムと運用の仕事。
