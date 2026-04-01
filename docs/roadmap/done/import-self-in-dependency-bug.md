<!-- description: Fix import self resolution in dependency packages (blocks almide-lander) -->
<!-- done: 2026-04-02 -->
# `import self` in Dependency Packages Bug

## Problem

パッケージ依存として取得されたモジュール内で `import self as pkg_name` が解決できない。ファイルパスとして `src/self.almd` を探しに行ってしまう。

```
Fetching bindgen from https://github.com/almide/almide-bindgen.git (main)
module 'self' not found
  searched: src/self.almd, src/self/mod.almd, src/self/src/mod.almd, src/self/src/lib.almd
```

## Impact

almide-lander のビルドがこのバグで完全にブロックされている。bindgen パッケージ内の全ジェネレーター（python.almd, go.almd 等）が `import self as bindgen` で親パッケージを参照しており、dependency としてキャッシュから読み込まれた時に解決が失敗する。

## Reproduction

```bash
cd almide-lander
almide clean && rm -f almide.lock
almide run src/main.almd -- --dry-run --lang python spec/mathlib.almd
```

## Context

- bindgen パッケージの各ジェネレーターは `import self as bindgen` を使って共有ヘルパー（`get_str`, `get_arr` 等）を参照している
- ローカルで直接 `almide check src/bindings/python.almd` すると通る（`import self` が正しく解決される）
- dependency として fetch された時（`~/.almide/cache/bindgen/...`）だけ失敗する

## Fix Direction

`src/resolve.rs` の dependency module 解決で、`import self` を dependency パッケージのルートモジュール（`mod.almd`）として解決する処理が必要。現在 `import self` はメインプロジェクトの self_module_name にマッピングされるが、dependency 内のソースコードに対しても同じ解決が行われる必要がある。

## Files to Investigate

- `src/resolve.rs` — dependency のモジュール解決
- `src/main.rs` — dependency module の infer/lower 前の self_module_name 設定
- `crates/almide-frontend/src/import_table.rs` — `build_import_table()` の self module ハンドリング
- `crates/almide-frontend/src/check/mod.rs` — `infer_module()` の self_module_name 設定

## Related

- `versioned-module-codegen-bug.md` (done) — Bug 1 のバージョン名不一致は修正済み
- `almide-to-almide-ffi.md` (on-hold) — lander が動くことが前提
