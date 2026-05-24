# WASM Performance Optimization — Handoff

## 状況

Almide WASM emitter（LLVMなし、自前コード生成）で Rust+LLVM の WASM と対決。
1Mスケール精密計測で **7勝1分3敗**。残り3つを潰して全勝を狙う。

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

## ファイルマップ

| ファイル | 役割 |
|---|---|
| `crates/almide-codegen/src/emit_wasm/calls_map.rs` | map hash table (Swiss Table layout) |
| `crates/almide-codegen/src/emit_wasm/calls_list_closure2.rs` | list.map/filter/fold + stream fusion |
| `crates/almide-codegen/src/emit_wasm/calls_list_helpers.rs` | sort (run detection) + emit helpers |
| `crates/almide-codegen/src/emit_wasm/list_layout.rs` | メモリレイアウト定数 |
| `crates/almide-codegen/src/emit_wasm/functions.rs` | adaptive scratch locals |
| `crates/almide-codegen/src/emit_wasm/runtime.rs` | string append, alloc, concat |
| `crates/almide-codegen/src/emit_wasm/statements.rs` | Assign peephole (string concat) |
| `crates/almide-codegen/src/emit_wasm/expressions.rs` | EmptyMap, MapLiteral |
| `crates/almide-codegen/src/pass_tco.rs` | TCO + binary recursion transform |
| `crates/almide-codegen/src/pass_peephole.rs` | map.get??default → get_or fusion |
| `crates/almide-codegen/src/pass_closure_conversion.rs` | capture-free lambda preservation |
| `crates/almide-codegen/src/target.rs` | WASM nanopass pipeline |
| `research/benchmark/stdlib/wasm_compare.almd` | 100kスケールベンチ |
| `research/benchmark/stdlib/rust_wasm_compare/` | Rust比較用 |
