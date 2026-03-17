# Cross-Target CI [ACTIVE — 1.0 Phase I]

> "ターゲット選択がプログラムの挙動を変えてはならない" — TypeScript の教訓

## 概要

全テストを Rust ターゲットと TS ターゲットの両方で実行し、出力が一致することを自動検証する。

## 実装

- [x] CI スクリプト: `tools/cross-target-check.sh` (Deno で TS 実行)
- [x] GitHub Actions: `.github/workflows/ci-cross-target.yml` (develop push で自動実行)
- [x] **spec/lang: 45/45 pass**
- [x] **spec/stdlib: 13/13 pass**
- [x] **spec/integration: 11/12 pass** (1 known: codegen_do_block_test — break in IIFE)
- [x] **exercises: 21/21 pass**
- [x] **合計: 90/91 (98.9%)**

## TS codegen 修正 (実施済み)

- [x] VarId suffix で変数名ユニーク化 (shadowing 衝突解消)
- [x] Block/While/ForIn を値位置で block-body IIFE にラップ
- [x] Result unwrap 一時変数に atomic counter (`__r_name_N`)
- [x] TS 予約語 (default, delete 等) の sanitize
- [x] `unwrap_or` runtime を `__almd_unwrap_or` にリネーム
- [x] テスト関数を `async () =>` で emit (await 対応)
- [x] DoBlock tail を fn_body で flatten (IIFE 回避)
- [x] guard-loop tail に常に return を emit

## 既知の制限

- `codegen_do_block_test`: do block 内の `guard else break` が IIFE 内で `break` を使えない (TS の構造的制限)

## ターゲット品質階層 (Swift の教訓)

| Tier | ターゲット | 基準 | 現状 |
|------|-----------|------|------|
| Tier 1 | Rust | 全テスト通過、全 exercises 動作 | ✅ |
| Tier 2 | TS/JS | 全テスト通過 | 90/91 (98.9%) ✅ |
| Tier 3 | WASM | 基本動作確認 | CI あり (smoke test) |
