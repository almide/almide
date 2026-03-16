# Error Codes + JSON Output [ACTIVE — 1.0 Phase II]

> Rust: 10 年の error message 投資。TypeScript: TS2322 は全開発者が認識。
> Go: sub-second compile が採用を決めた。MoonBit: constrained sampler で LLM 精度向上。

## 概要

安定したエラーコード体系と、LLM agent 向けの構造化 JSON 出力を実装する。

## エラーコード

- [x] コード体系設計 + Diagnostic に `code` フィールド追加
- [x] 主要エラーにコード付与:
  - E001: type mismatch
  - E002: undefined function
  - E004: wrong argument count
  - E006: effect isolation violation
  - E007: fan in pure fn
  - E008: var capture in fan
  - E010: non-exhaustive match
- [ ] 残りの診断メッセージにコード付与 (E003 undefined variable, E005 arg type, E009 assign to immutable 等)
- [ ] `almide check --explain E001` でエラーの詳細説明

## JSON 出力

- [x] `almide check --json`: 構造化エラー出力 (1行1 diagnostic)
  ```json
  {"level":"error","code":"E001","message":"...","hint":"...","file":"app.almd","line":5,"col":17}
  ```
- [ ] `almide test --json`: 構造化テスト結果
- [ ] JSON schema を文書化（agent loop の安定インターフェース）

## check 速度

- [ ] `almide check` を 500 行プログラムで < 1 秒に計測
- [ ] `--check-only` モード: parser + checker のみ（lower/codegen スキップ）

## hint 修復率

- [ ] ベンチマーク: hint を適用して再コンパイルが通る率を計測
- [ ] 目標: 70%+
