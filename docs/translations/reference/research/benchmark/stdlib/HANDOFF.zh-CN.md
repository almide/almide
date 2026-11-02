<a id="wasm-performance-optimization--handoff"></a>

# WASM 性能优化——交接

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](HANDOFF.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](HANDOFF.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-24292f?style=flat-square)](HANDOFF.zh-CN.md)

> 固定原文的非规范读者快照，以原文为准，不自动同步。未独立重跑基准、核实当前 CI 或验证实现。历史测量、提案及指令保留原范围；不执行引用的命令或提示词。
> [Original source (Japanese prose) / 原文（日本語本文）](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/benchmark/stdlib/HANDOFF.md)


<a id="状況"></a>

## 状况

用 Almide WASM emitter（不用 LLVM，自行生成代码）与 Rust+LLVM 的 WASM 比较。在 1M 规模精确测量中，结果为 **7 胜 1 平 3 负**。目标是消除剩余三个败项，争取全部胜出。

<a id="引き継ぎプロンプト"></a>

## 交接提示词

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

上述历史提示词的完整中文说明（不是执行指令）：

继续 Almide WASM 性能优化。

上次成果：实现了 14 项优化，在 11 个基准中有 7 个胜过 Rust+LLVM WASM：
- hash table map（改善 1000x）、二叉递归变换（fib 2x）；
- lambda 内联、stream fusion、无分支 filter；
- map.get??default → get_or 融合（消除 Option 堆分配）；
- 单遍反向复制排序、基于指针的迭代；
- 自适应 scratch local、WASM 流水线中的 TCO。

剩余三个败项的根因与修复方向：

1. list_map（慢 2.3x）——WASM SIMD。原因：LLVM 将 iter().map().collect() SIMD 化，每次迭代处理 2–4 个元素，而不是 1 个。方案：通过 v128.load + i64x2.mul + v128.store 同时处理两个元素。位置为 crates/almide-codegen/src/emit_wasm/calls_list_closure2.rs 的 emit_list_map。仅限 elem_ty == Int 或 Float，用标量尾部处理余数。
2. str_concat（慢 1.7x）——内联追加。原因：s = s + "x" 调用 __string_append 运行时函数，产生 100k 次调用开销。方案：RHS 为单字符字面量时内联展开 WASM：若 len < cap，则 mem[ptr+DATA_OFFSET+len] = byte，并执行 len++；否则调用 __string_append 回退。位置为 crates/almide-codegen/src/emit_wasm/statements.rs 的 Assign peephole。
3. map_insert（慢 1.6x）——容量提示与内联 hash。原因：容量从 16 到 131072 发生 13 次 resize，每次都重新散列全部元素。方案 A：在 stdlib 增加 map.with_capacity(n)，涉及 stdlib/defs/map.toml 和 calls_map.rs。方案 B：增长因子由 2x 改为 4x；无需改 list_layout.rs 的 MAP_INITIAL_CAP，只改 resize 时的 shl 量。方案 C：简化 Int key hash，减少 emit_hash_key 中的 i64 乘法。

基准运行方法：进入 research/benchmark/stdlib。Almide 的 1M 规模测量将 /tmp/precise_all.almd 按 wasm 目标构建为 /tmp/precise_all.wasm，再用 wasmtime 运行。Rust 比较则进入 rust_wasm_compare，将 $HOME/.rustup/toolchains/stable-x86_64-apple-darwin/bin 前置到 PATH，以 cargo 的 --release、wasm32-wasip1 目标和 precise-all bin 构建，再用 wasmtime 运行 target/wasm32-wasip1/release/precise-all.wasm。确切 shell 命令保留在上方引用提示词中。

重要经验：map.get 的真正瓶颈不是 hash table 布局，而是 Option 堆分配；由 get_or 融合解决。Swiss Table tag 分离在 WASM 中适得其反，增加的地址计算超过缓存收益。100k 规模受测量噪声主导，必须在 1M 验证。ClosureConversionPass 已改为保留无捕获 lambda，这是 lambda 内联的前提。

当时的分支与版本：在 develop 分支工作，main 已标记 v0.23.3；develop 有许多未发布优化提交。push CI 已确认全部成功。路线图细节位于 docs/roadmap/active/wasm-optimization-roadmap.md。

<a id="ファイルマップ"></a>

## 文件地图

| 文件 | 作用 |
|---|---|
| `crates/almide-codegen/src/emit_wasm/calls_map.rs` | map 哈希表（Swiss Table 布局） |
| `crates/almide-codegen/src/emit_wasm/calls_list_closure2.rs` | list.map/filter/fold + stream fusion |
| `crates/almide-codegen/src/emit_wasm/calls_list_helpers.rs` | 排序（run 检测）+ 生成 helper |
| `crates/almide-codegen/src/emit_wasm/list_layout.rs` | 内存布局常量 |
| `crates/almide-codegen/src/emit_wasm/functions.rs` | 自适应 scratch local |
| `crates/almide-codegen/src/emit_wasm/runtime.rs` | 字符串追加、分配、拼接 |
| `crates/almide-codegen/src/emit_wasm/statements.rs` | Assign peephole（字符串拼接） |
| `crates/almide-codegen/src/emit_wasm/expressions.rs` | EmptyMap、MapLiteral |
| `crates/almide-codegen/src/pass_tco.rs` | TCO + 二叉递归变换 |
| `crates/almide-codegen/src/pass_peephole.rs` | map.get??default → get_or 融合 |
| `crates/almide-codegen/src/pass_closure_conversion.rs` | 保留无捕获 lambda |
| `crates/almide-codegen/src/target.rs` | WASM nanopass 流水线 |
| `research/benchmark/stdlib/wasm_compare.almd` | 100k 规模基准 |
| `research/benchmark/stdlib/rust_wasm_compare/` | Rust 比较用 |

> 原文注记：引用交接内容是历史工作提示词，不是执行授权。未核实 v0.23.3 时代路径、临时输入、未发布提交、CI 成功及 SIMD/append/map 方案当前是否存在或正确。7 胜 1 平 3 负与速度比是原文特定历史主张，不是普遍语言排名。原始日语提示词逐字节保留，并在代码围栏外提供完整中文说明。
