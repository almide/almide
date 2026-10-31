# Spec Rules

## 仕様の権威と実装の証拠

- spec は実装と同時に更新する。コードを変更したら対応する spec も同じコミットで更新
- [SPEC の normative status](../SPEC.md) に従い、ALS と実装が食い違う場合は実装側の不一致として扱う。実装を観察しただけで normative な契約を書き換えない
- spec には検証テストのパスと確認範囲を書く。テストの成功はそのケースの証拠であり、仕様全体の正しさの証明ではない
- 未検証の箇所や仕様同士の矛盾は明記し、採用済みの規則と実装の現状を区別する
- 古い現行説明は訂正する。ADR・履歴・規範的契約は、実装と違うという理由だけでは削除しない
- 実装の現状を説明する節では、実装済みの挙動と確認範囲を明記し、推測や予定を現在の保証として書かない
- 未採択の将来提案は `docs/roadmap/` に書く。採択済み ALS の規範は未実装・不適合の箇所も保持し、その状態を明記する

## フォーマット

- 冒頭に `> Last updated: YYYY-MM-DD` を入れる
- 各セクションに検証テストのパスを明記: 「テスト: `spec/integration/modules/diamond_test.almd`」
- 仕様の記述にはコード例を添える。コード例はそのまま `.almd` ファイルに貼ってコンパイルできるものにする

## 現在の spec

| ファイル | 内容 |
|---|---|
| `language.md` | 言語仕様: 型, 宣言, 式, 文, パターン, 演算子, 可視性, コメント |
| `type-system.md` | 型システム: 推論, ジェネリクス, レコード, バリアント, プロトコル, 型エイリアス |
| `effect-system.md` | エフェクト: fn vs effect fn, 明示伝搬 (`!` のみ — ADR-0008), fan, 権限, E006/E007/E008 |
| `codegen.md` | コード生成: Nanopass pipeline, テンプレート, Rust/WASM 出力, モジュール命名 |
| `cli.md` | CLI: 全コマンド, オプション, エラーコード, Legacy Mode |
| `module-system.md` | モジュール: import, サブモジュール, ダイヤモンド依存, 可視性, @extern |
| `edit-locality.md` | 編集局所性: L1–L3 不変条件, L4–L5 宣言フレームと合成, 施行マップ, 反例狩り (2026-08-15), 側条件, 機能ゲート |
| `package-system.md` | パッケージ: 依存管理, MVS, バージョン共存, モジュール境界 |
| `result-option-effect.md` | Result/Option と `!` の文脈別規則、lambda 境界 |
| `effect-fn-call-semantics.md` | エフェクト呼び出しと明示伝搬 |
| `scoped.md` | scoped 借用と検証範囲 |
| `perceus.md` | RC 最適化のモデル、実装、証拠の境界 |
| `als/` | ALS の規範的記述と既知の整合性注記 |
