# Error Codes + JSON Output [ACTIVE — 1.0 Phase II]

> Rust: 10 年の error message 投資。TypeScript: TS2322 は全開発者が認識。
> Go: sub-second compile が採用を決めた。MoonBit: constrained sampler で LLM 精度向上。

## 概要

安定したエラーコード体系と、LLM agent 向けの構造化 JSON 出力を実装する。

## エラーコード

- [ ] E0001-E9999 のコード体系設計
- [ ] 全診断メッセージにコードを付与
- [ ] `almide check --explain E0001` でエラーの詳細説明

## JSON 出力

- [ ] `almide check --json`: 構造化エラー出力
- [ ] `almide test --json`: 構造化テスト結果
- [ ] JSON schema を文書化（agent loop の安定インターフェース）

## check 速度

- [ ] `almide check` を 500 行プログラムで < 1 秒に
- [ ] `--check-only` モード: parser + checker のみ（lower/codegen スキップ）

## hint 修復率

- [ ] ベンチマーク: hint を適用して再コンパイルが通る率を計測
- [ ] 目標: 70%+
