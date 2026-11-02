<a id="almide-standard-library"></a>

# Almide 標準ライブラリ

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](README.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](README.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](README.zh-CN.md)



> 原文: [stdlib/README.md @ 35297a](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/stdlib/README.md)。非規範的な読者向けスナップショット翻訳です。自動同期されません。計測値・主張の独立した再検証は行っていません。原文を正本とします。

stdlib は **セルフホスト**されています。全関数を `stdlib/*.almd` の純粋な Almide で宣言します。旧 `stdlib/defs/*.toml` パイプラインは廃止されました（Stdlib Declarative Unification arc）。TOML はなく、TOML からのビルド時ディスパッチ生成もありません。

<a id="architecture-overview"></a>

## アーキテクチャ概要

```
stdlib/<module>.almd            Module body: signatures + @intrinsic decls +
                                @inline_rust templates + pure-Almide impls
stdlib/<module>_<part>.almd     Split files: pure-Almide implementations used
                                by the WASM self-hosting path
        │
        ├─▶ crates/almide-types/src/stdlib_info.rs
        │     bundled_source(): include_str! of module bodies.
        │     The frontend extracts FnSigs; codegen extracts @inline_rust.
        │
        ├─▶ crates/almide-types/src/self_host_registry.rs
        │     self_host_runtime(): (SRC_<STEM>, [(impl_fn, "module.fn")]) registrations:
        │     pure-Almide impls linked into IR and emitted by almide-wasm.
        │     An unlinked stdlib call is a wall (hard error) in the renderer.
        │
        └─▶ runtime/rs/src/<module>.rs
              Hand-written almide_rt_* natives, targeted by
              @intrinsic("almide_rt_*") declarations (native target only).
```

<a id="adding-or-changing-a-function"></a>

## 関数の追加・変更

1. **Almide で実装** — `stdlib/<module>.almd`（または WASM 経路用の新しい分割ファイル `stdlib/<module>_<part>.almd`）に関数を追加します。
2. **Wasm/セルフホストのカバレッジ** — 新実装を `@implements("module.fn")` で宣言します。`self_host_runtime()` は宣言された本体を縮小専用の旧テーブルと組み合わせます。手管理する名前の対応表をもう一つ追加しないでください。`crates/almide-types/src/implements.rs` を参照してください。
3. **Native intrinsic（純粋な Almide では足りない場合のみ）** — `runtime/rs/src/<module>.rs` に `almide_rt_*` を実装し、モジュール本体で `@intrinsic("almide_rt_*")` により宣言します。
4. **新モジュール** — `crates/almide-types/src/stdlib_info.rs` の `STDLIB_MODULES`、`BUNDLED_MODULES`、`bundled_source()`（自動 import 対象には `AUTO_IMPORT_BUNDLED` も）を更新します。
5. **テスト** — `spec/stdlib/*_test.almd`（またはインライン `test` ブロック）を追加します。インタープリタは対応範囲内のセルフホスト本体を自動評価します。その範囲で関数を処理できない場合に限りインタープリタの接続コードを追加し、3-way oracle のカバレッジが気づかないうちに減らないことを確認します。`crates/almide-interp/CLAUDE.md` を参照してください。

<a id="auto-import"></a>

## 自動 import

自動 import されるモジュールは、正本となる二つのリストの和集合です。

- `crates/almide-frontend/src/import_table.rs` の seed リスト
- `crates/almide-types/src/stdlib_info.rs` の `AUTO_IMPORT_BUNDLED`

複製したモジュール一覧ではなく、このリストが自動 import を定義します。和集合に含まれないモジュールには明示的な `import` が必要です。生成された [stdlib リファレンス](../../../stdlib) に、利用可能な公開モジュールを掲載しています。

<a id="documentation"></a>

## ドキュメント

- 言語利用者向けリファレンス: [docs/CHEATSHEET.md](../../../CHEATSHEET.md)
- モジュール別ノート: [docs/stdlib/](../../../stdlib)
- クロスターゲット意味論マニフェスト（CI で検査、手編集禁止）: `docs/stdlib/semantics-manifest.toml`
- 各モジュールのインターフェース: `almide compile <module> --json`
