<!-- description: Run all tests on both Rust and TS targets, verify output match -->
<!-- done: 2026-03-18 -->
# Cross-Target CI

> "ターゲット選択がプログラムの挙動を変えてはならない" — TypeScript の教訓

## 概要

全テストを Rust ターゲットと TS ターゲットの両方で実行し、出力が一致することを自動検証する。

## 実装

- [x] CI スクリプト: `.github/workflows/ci-cross-target.yml` (develop push で自動実行)
- [x] **spec/lang: 45/45 pass**
- [x] **spec/stdlib: 14/14 pass**
- [x] **spec/integration: 13/13 pass**
- [x] **exercises: 34/34 pass**
- [x] **合計: 106/106 (100%)**

## Codegen v3 による達成 (2026-03-18)

- [x] `is_rust()` 42 → 0: walker 完全 target-agnostic
- [x] ResultErasurePass: ok(x)→x, err(e)→throw (TS/Python)
- [x] ShadowResolvePass: let shadowing → assignment (TS)
- [x] MatchLoweringPass 拡張: Constructor + RecordPattern + guard
- [x] break-in-IIFE 解決: contains_loop_control で IIFE 回避
- [x] 40+ TOML テンプレートで Rust/TS 構文差異を吸収

## 既知の制限

なし。全 106 テストが Rust + TS 両方で pass。

## ターゲット品質階層

| Tier | ターゲット | 基準 | 現状 |
|------|-----------|------|------|
| Tier 1 | Rust | 全テスト通過 | 72/72 ✅ |
| Tier 1 | TS/JS | 全テスト通過 | 106/106 ✅ |
| Tier 3 | WASM | 基本動作確認 | CI あり (smoke test) |
