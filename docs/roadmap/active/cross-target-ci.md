# Cross-Target CI [ACTIVE — 1.0 Phase I]

> "ターゲット選択がプログラムの挙動を変えてはならない" — TypeScript の教訓

## 概要

全テストを Rust ターゲットと TS ターゲットの両方で実行し、出力が一致することを自動検証する。

## 実装

- [ ] CI スクリプト: `almide test --target rust` と `almide test --target ts` を実行
- [ ] 出力 diff: 不一致があれば CI 失敗
- [ ] exercises 25 本を両ターゲットで実行
- [ ] spec/ テストのうち TS 対応可能なものを特定・実行

## ターゲット品質階層 (Swift の教訓)

| Tier | ターゲット | 基準 |
|------|-----------|------|
| Tier 1 | Rust | 全テスト通過、全 exercises 動作 |
| Tier 2 | TS/JS | 全テスト通過 |
| Tier 3 | WASM | 基本動作確認 |
