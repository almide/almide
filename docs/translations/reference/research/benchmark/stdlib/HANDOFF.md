<a id="wasm-performance-optimization--handoff"></a>

# WASM Performance Optimization — Handoff

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](HANDOFF.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](HANDOFF.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](HANDOFF.zh-CN.md)

> Non-normative reader snapshot of the pinned original; original remains authoritative. No automatic synchronization, independent rerun of benchmarks, current CI verification or implementation validation. Historical measurements, proposals and instructions retain their original scope; quoted commands and prompts are not executed.
> [Original source (Japanese prose) / 原文（日本語本文）](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/benchmark/stdlib/HANDOFF.md)


<a id="状況"></a>

## Status

Comparing the Almide WASM emitter (no LLVM, its own code generation) against Rust+LLVM WASM. Precise measurements at the 1M scale yielded **7 wins, 1 tie and 3 losses**. The aim is to eliminate the remaining three losses and win all cases.

<a id="引き継ぎプロンプト"></a>

## Handoff prompt

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

Full English explanation of the quoted historical prompt (not an instruction to execute):

Continuation of Almide WASM performance optimization.

Previous results: 14 optimizations were implemented, beating Rust+LLVM WASM in 7 of 11 benchmarks:
- hash-table map (1000x improvement), binary recursion transform (fib 2x);
- lambda inlining, stream fusion, branchless filter;
- map.get??default → get_or fusion (eliminates Option heap allocation);
- one-pass reverse-copy sort, pointer-based iteration;
- adaptive scratch locals, TCO in the WASM pipeline.

Root causes and proposed fixes for the three remaining losses:

1. list_map (2.3x slower) — WASM SIMD. Cause: LLVM vectorizes iter().map().collect(), processing 2–4 elements per iteration rather than one. Proposed fix: process two elements together using v128.load + i64x2.mul + v128.store. Location: emit_list_map in crates/almide-codegen/src/emit_wasm/calls_list_closure2.rs. Restrict to elem_ty == Int or Float, and process the remainder with a scalar tail.
2. str_concat (1.7x slower) — inline append. Cause: s = s + "x" calls the __string_append runtime function, incurring 100k calls of overhead. Proposed fix: for a one-character literal RHS, expand inline WASM: if len < cap, write byte to mem[ptr+DATA_OFFSET+len] and increment len; otherwise call __string_append as fallback. Location: Assign peephole in crates/almide-codegen/src/emit_wasm/statements.rs.
3. map_insert (1.6x slower) — capacity hint and inline hash. Cause: 13 resizes from cap=16 to 131072, rehashing all elements each time. Options: A) add map.with_capacity(n) to the stdlib through stdlib/defs/map.toml and calls_map.rs; B) change the growth factor from 2x to 4x, changing the shift amount at resize without changing MAP_INITIAL_CAP in list_layout.rs; C) simplify Int-key hashing by reducing i64 multiplications in emit_hash_key.

Benchmark execution procedure: enter research/benchmark/stdlib. For Almide at 1M scale, build /tmp/precise_all.almd with target wasm to /tmp/precise_all.wasm, then run that file with wasmtime. For the Rust comparison, enter rust_wasm_compare, prepend $HOME/.rustup/toolchains/stable-x86_64-apple-darwin/bin to PATH, build with cargo --release for wasm32-wasip1 and bin precise-all, then run target/wasm32-wasip1/release/precise-all.wasm with wasmtime. The exact shell commands remain in the quoted prompt above.

Key lessons: the true bottleneck in map.get was Option heap allocation rather than hash-table layout, solved by get_or fusion. Separating Swiss Table tags was counterproductive on WASM because extra address calculation outweighed cache benefits. The 100k scale was dominated by measurement noise; verify at 1M. ClosureConversionPass had been changed to retain capture-free lambdas, a prerequisite for lambda inlining.

Branch and version at the time: work was on develop, with v0.23.3 tagged on main; develop contained many unreleased optimization commits. Push CI had been confirmed entirely green. Roadmap details were at docs/roadmap/active/wasm-optimization-roadmap.md.

<a id="ファイルマップ"></a>

## File map

| File | Role |
|---|---|
| `crates/almide-codegen/src/emit_wasm/calls_map.rs` | map hash table (Swiss Table layout) |
| `crates/almide-codegen/src/emit_wasm/calls_list_closure2.rs` | list.map/filter/fold + stream fusion |
| `crates/almide-codegen/src/emit_wasm/calls_list_helpers.rs` | sort (run detection) + emit helpers |
| `crates/almide-codegen/src/emit_wasm/list_layout.rs` | Memory-layout constants |
| `crates/almide-codegen/src/emit_wasm/functions.rs` | adaptive scratch locals |
| `crates/almide-codegen/src/emit_wasm/runtime.rs` | string append, alloc, concat |
| `crates/almide-codegen/src/emit_wasm/statements.rs` | Assign peephole (string concat) |
| `crates/almide-codegen/src/emit_wasm/expressions.rs` | EmptyMap, MapLiteral |
| `crates/almide-codegen/src/pass_tco.rs` | TCO + binary recursion transform |
| `crates/almide-codegen/src/pass_peephole.rs` | map.get??default → get_or fusion |
| `crates/almide-codegen/src/pass_closure_conversion.rs` | capture-free lambda preservation |
| `crates/almide-codegen/src/target.rs` | WASM nanopass pipeline |
| `research/benchmark/stdlib/wasm_compare.almd` | 100k-scale benchmark |
| `research/benchmark/stdlib/rust_wasm_compare/` | For the Rust comparison |

> Source caveat: the quoted handoff is a historical work prompt, not authority to execute it. The v0.23.3-era paths, temporary input, unreleased commits, green CI and proposed SIMD/append/map changes have not been checked for current existence or correctness. Its 7/1/3 outcome and speed ratios are source-specific historical claims, not universal language rankings. The original Japanese prompt is preserved byte-for-byte, with a complete English explanation outside the fence.
