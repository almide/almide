# Cross-Target CI [ACTIVE — 1.0 Phase I]

> "ターゲット選択がプログラムの挙動を変えてはならない" — TypeScript の教訓

## 概要

全テストを Rust ターゲットと TS ターゲットの両方で実行し、出力が一致することを自動検証する。

## 実装

- [x] CI スクリプト: `tools/cross-target-check.sh` (Deno で TS 実行)
- [x] **spec/lang: 45/45 pass** (Rust + TS 両方で実行成功)
- [ ] spec/stdlib: TS 実行検証
- [ ] spec/integration: TS 実行検証
- [ ] exercises 25 本を両ターゲットで実行
- [ ] GitHub Actions 等での CI 自動化

## TS codegen 修正 (実施済み)

- [x] VarId suffix で変数名ユニーク化 (shadowing 衝突解消)
- [x] Block/While/ForIn を値位置で block-body IIFE にラップ
- [x] Result unwrap 一時変数に atomic counter (`__r_name_N`)
- [x] TS 予約語 (default, delete 等) の sanitize
- [x] `unwrap_or` runtime を `__almd_unwrap_or` にリネーム
- [x] テスト関数を `async () =>` で emit (await 対応)

## ターゲット品質階層 (Swift の教訓)

| Tier | ターゲット | 基準 | 現状 |
|------|-----------|------|------|
| Tier 1 | Rust | 全テスト通過、全 exercises 動作 | ✅ |
| Tier 2 | TS/JS | 全テスト通過 | spec/lang 45/45 ✅ |
| Tier 3 | WASM | 基本動作確認 | 未検証 |
