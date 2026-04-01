<!-- description: Fix versioned module name mismatch in codegen for package dependencies -->
<!-- done: 2026-04-02 -->
# Versioned Module Codegen Bug

## Problem

パッケージ依存（`almide.toml [dependencies]`）経由で import されたモジュールの関数名が、定義と呼び出しで不一致になる。

**定義側:** `almide_rt_bindgen_v0_supported_languages`（`v0` suffix あり）
**呼び出し側:** `almide_rt_bindgen_supported_languages`（`v0` suffix なし）

これにより almide-lander のビルドが全面的に失敗する（158 errors）。

## Reproduction

```bash
cd almide-lander
almide run src/main.almd -- --dry-run --lang python spec/mathlib.almd
```

```
error[E0425]: cannot find function `almide_rt_bindgen_supported_languages` in this scope
  help: a function with a similar name exists: `almide_rt_bindgen_v0_supported_languages`
```

## Root Cause

`src/main.rs` の `lower_module()` が `versioned_name` を使って関数定義にバージョン prefix を付けるが、呼び出し側（consumer モジュール）の codegen がバージョンなしの名前を生成する。

関連コード:
- `src/main.rs:264` — `pid.mod_name()` でバージョン付き名前を生成
- `src/main.rs:276` — `lower_module(name, mod_prog, &checker.env, &checker.type_map, versioned)` でバージョン付き名前を渡す
- `crates/almide-frontend/src/lower/mod.rs:462` — `lower_module` が `versioned_name` を使って関数名にプレフィックスを付ける

呼び出し側（consumer）が `CallTarget::Module { module, func }` を解決する時に、バージョン付き名前に変換されていない。

## Additional Bug: `local fn` in Package Modules

パッケージ内モジュールの `local fn` が生成 Rust で正しくスコープされない。各ジェネレーター（c.almd, java.almd, cpp.almd 等）の `local fn get_str`, `local fn py_type` 等が `cannot find function` エラーになる。

## Impact

- almide-lander が全面的にビルド不能（almide-bindgen 依存パッケージの全関数が undefined）
- パッケージ依存を使う全プロジェクトに影響する可能性

## Fix Direction

1. **呼び出し側の module 名解決**: `CallTarget::Module { module, func }` → `CallTarget::Named { name: "almide_rt_{versioned_module}_{func}" }` に変換する時に、バージョン付き名前を使う
2. **`local fn` のスコープ**: パッケージ内モジュールの `local fn` が codegen で正しいプレフィックスを持つように修正

## Files to Investigate

- `src/main.rs` — モジュール解決とバージョン名の組み立て
- `crates/almide-frontend/src/lower/mod.rs` — `lower_module()` の versioned_name 処理
- `crates/almide-codegen/src/pass_stdlib_lowering.rs` — Module call → Named call の変換
- `crates/almide-codegen/src/walker/` — 生成 Rust での関数名レンダリング
