<a id="wasm-performance-optimization--handoff"></a>

# WASM 性能最適化 — 引継ぎ

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](HANDOFF.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](HANDOFF.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](HANDOFF.zh-CN.md)

> 固定原文の非規範読者用スナップショット。原文を正本とし、自動同期しない。benchmark の独立再実行、現 CI 確認、実装検証はしない。歴史的測定・提案・指示は元の範囲を保持し、引用コマンドやプロンプトは実行しない。
> [Original source (Japanese prose) / 原文（日本語本文）](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/benchmark/stdlib/HANDOFF.md)


<a id="状況"></a>

## 状況

Almide WASM emitter（LLVMなし、自前コード生成）で Rust+LLVM の WASM と対決。
1Mスケール精密計測で **7勝1分3敗**。残り3つを潰して全勝を狙う。

<a id="引き継ぎプロンプト"></a>

## 引き継ぎプロンプト

```
Almide WASM パフォーマンス最適化の続き。

## 前回の成果
14の最適化を実装し、11ベンチマーク中7つでRust+LLVMのWASMに勝利:
- hash table map (1000x改善)、binary recursion transform (fib 2x)
- lambda inlining、stream fusion、branchless filter
- map.get??default → get_or fusion (Option heap alloc排除)
- 1-pass reverse copy sort、pointer-based iteration
- adaptive scratch locals、TCO in WASM pipeline

## 残り3敗の根本原因と修正方針

### 1. list_map (2.3x負け) — WASM SIMD
原因: LLVMがiter().map().collect()をSIMD化。1要素/iteration vs 2-4要素/iteration。
修正: `v128.load` + `i64x2.mul` + `v128.store` で2要素同時処理。
ファイル: `crates/almide-codegen/src/emit_wasm/calls_list_closure2.rs` の `emit_list_map`
条件: elem_ty == Int or Float のみ。スカラーtailで端数処理。

### 2. str_concat (1.7x負け) — inline append
原因: `s = s + "x"` → `__string_append` runtime function call。100k回のcall overhead。
修正: RHSが1-char literalの場合、inline WASM展開:
  if len < cap: mem[ptr+DATA_OFFSET+len] = byte; len++
  else: call __string_append (fallback)
ファイル: `crates/almide-codegen/src/emit_wasm/statements.rs` のAssign peephole

### 3. map_insert (1.6x負け) — capacity hint + inline hash
原因: cap=16→131072の13回resize。各resizeで全要素rehash。
修正案:
  A) `map.with_capacity(n)` stdlib追加 → stdlib/defs/map.toml + calls_map.rs
  B) growth factor 4x (2x→4x) → list_layout.rs MAP_INITIAL_CAP変更不要、resize時のshl量変更
  C) Int key hash簡略化 → emit_hash_key のi64乗算を削減

## ベンチマーク実行方法
cd research/benchmark/stdlib
# Almide (1Mスケール)
almide build /tmp/precise_all.almd --target wasm -o /tmp/precise_all.wasm && wasmtime /tmp/precise_all.wasm
# Rust比較
cd rust_wasm_compare && PATH="$HOME/.rustup/toolchains/stable-x86_64-apple-darwin/bin:$PATH" cargo build --release --target wasm32-wasip1 --bin precise-all && wasmtime target/wasm32-wasip1/release/precise-all.wasm

## 重要な学び
- map.getの真のボトルネックはhash tableレイアウトではなくOption heap alloc → get_or fusionで解決
- Swiss Table tag分離はWASMでは逆効果（アドレス計算増加 > キャッシュ効果）
- 100kスケールは計測ノイズに支配される。必ず1Mで検証
- ClosureConversionPassでcapture-free lambdaを残す変更済み（Lambda inlining前提）

## ブランチ・バージョン
- develop ブランチで作業中（v0.23.3 tagged on main）
- develop に未リリースの最適化コミット多数
- CI: push CI全グリーン確認済み

## ロードマップ
docs/roadmap/active/wasm-optimization-roadmap.md に詳細あり
```

<a id="ファイルマップ"></a>

## ファイルマップ

| ファイル | 役割 |
|---|---|
| `crates/almide-codegen/src/emit_wasm/calls_map.rs` | map ハッシュテーブル（Swiss Table レイアウト） |
| `crates/almide-codegen/src/emit_wasm/calls_list_closure2.rs` | list.map/filter/fold と stream fusion |
| `crates/almide-codegen/src/emit_wasm/calls_list_helpers.rs` | sort（run 検出）と出力 helper |
| `crates/almide-codegen/src/emit_wasm/list_layout.rs` | メモリレイアウト定数 |
| `crates/almide-codegen/src/emit_wasm/functions.rs` | 適応的 scratch local |
| `crates/almide-codegen/src/emit_wasm/runtime.rs` | 文字列の追加・割当て・連結 |
| `crates/almide-codegen/src/emit_wasm/statements.rs` | Assign の局所最適化（文字列連結） |
| `crates/almide-codegen/src/emit_wasm/expressions.rs` | EmptyMap, MapLiteral |
| `crates/almide-codegen/src/pass_tco.rs` | TCO と二分再帰変換 |
| `crates/almide-codegen/src/pass_peephole.rs` | map.get??default → get_or 融合 |
| `crates/almide-codegen/src/pass_closure_conversion.rs` | capture のない lambda の保持 |
| `crates/almide-codegen/src/target.rs` | WASM nanopass パイプライン |
| `research/benchmark/stdlib/wasm_compare.almd` | 100kスケールベンチ |
| `research/benchmark/stdlib/rust_wasm_compare/` | Rust比較用 |

> 原文注記：引用した引継ぎは歴史的な作業プロンプトであり、実行権限ではない。v0.23.3 当時のパス、一時入力、未公開コミット、CI 成功、SIMD/append/map の提案は、現在の存在や正しさを確認していない。7勝1分3敗と速度比は原文固有の歴史的主張であり、言語全般の順位ではない。原文の日本語プロンプトはバイト単位で保持する。
