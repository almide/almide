> **履歴資料（#916）。** 以下のvs-Rust成績表と27件すべての「完了した最適化」は、v0.25.0時点で、**#782で廃止されたv0 emitter**を対象に、現在は存在しないfileで測定した。生log自身が負荷ありと記すmachineでの測定である。キャンペーンの記録として残す。baselineを明示したv1 legの再計測は#917。現行のvs-Rustの話はdocs/wasm/WASM-OUTPUT.mdとmemory ledgerの「Rust-wasm打倒アーク」（v0.34.1、adversarial-Rust 4-0）にある。

<!-- description: WASM最適化roadmapとRust+LLVMに対する成績表 -->
<a id="wasm-optimization-roadmap"></a>

# WASM最適化ロードマップ

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](wasm-optimization-roadmap.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](wasm-optimization-roadmap.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](wasm-optimization-roadmap.zh-CN.md)

> 非規範的な履歴読者資料。固定原文が正本、原本不変更。自動同期・独立benchmark/gate実行なし。負荷あり機の旧v0 emitter成績、古い現行表現、後のv1 wasm-opt記録、提案目標は各時点を保つ。表示時間と整合しない比率セルも勝手に訂正しない。全勝や言語固有最適化の普遍的優位は原文キャンペーンの主張で、新しい比較根拠や保証ではない。差分gateは外部wasm-opt変換の形式検証ではない。 [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/wasm-optimization-roadmap.md)

> AlmideのWASM emitter：hello world 336B。**当時の状態：Rust+LLVMに3勝/2分/6敗（v0.25.0対rustc 1.95、2026-06-07 — 相手も動く。Baseline Update参照）**。v0.23.4時点は5W/2T/4L。このroadmapは、その時々のstable rustcに対して**11本すべて**のbenchmarkで勝つことを目指す。

<a id="precision-benchmark-1m-scale-v0234"></a>

## 精密benchmark（1M規模、v0.23.4）

| Benchmark          | Almide  | Rust   | 比率      | 状態     |
|--------------------|---------|--------|------------|------------|
| fib38              | 163ms   | 164ms  | ~1.0x      | ≈ 引分      |
| sort_1M            |   6.1ms |  6.7ms | **0.91x**  | ✓ 勝ち      |
| list_map_1M        |   3.9ms |  1.6ms | 3.9x       | ✗ 負け   |
| list_filter_1M     |   2.6ms |  2.2ms | 1.5x       | ✗ 負け   |
| list_fold_1M       |   1.1ms |  0.4ms | 2.8x       | ✗ 負け   |
| str_concat_1M      |   2.6ms |  1.4ms | 2.2x       | ✗ 負け   |
| map_insert_100k    |   3.8ms |  4.2ms | **0.90x**  | ≈ 引分      |
| map_get_100k       |   5.0ms |  6.3ms | **0.79x**  | ✓ 勝ち      |
| int_parse_1M       |  57ms   | 66ms   | **0.86x**  | ✓ 勝ち      |
| int_tostring_1M    |  35ms   | 59ms   | **0.59x**  | ✓ 勝ち      |
| math_sqrt_1M       |   0.4ms |  1.5ms | **0.27x**  | ✓ 勝ち      |

**成績：5勝、2分、4敗**

---

<a id="baseline-update-2026-06-07--v0250-vs-rustc-195-相手が動いた"></a>

## Baseline更新2026-06-07 — v0.25.0対rustc 1.95（相手が動いた）

| Benchmark          | Almide 0.25.0 | Rust 1.95 | 比率     | 状態  | v0.23.4比 |
|--------------------|---------------|-----------|-----------|---------|-----------|
| fib38              | 145.2ms       | 142.7ms   | 1.02x     | ≈ 引分   | TIE 維持 |
| sort_1M            |   5.8ms       |  4.8ms    | 1.21x     | ✗ 負け | **WIN→LOSS 反転** |
| list_map_1M        |   3.3ms       |  0.7ms    | 4.7x      | ✗ 負け | Almide 改善・比は悪化 |
| list_filter_1M     |   2.6ms       |  1.7ms    | 1.53x     | ✗ 負け | 横ばい |
| list_fold_1M       |   1.1ms       |  0.4ms    | 2.75x     | ✗ 負け | 横ばい |
| str_concat_1M      |   3.0ms       |  1.4ms    | 2.14x     | ✗ 負け | 横ばい |
| map_insert_100k    |   4.2ms       |  3.7ms    | 1.14x     | ✗ 負け | **TIE→LOSS 反転** |
| map_get_100k       |   4.8ms       |  5.4ms    | **0.89x** | ✓ 勝ち   | WIN 維持 |
| int_parse_1M       |  55.6ms       | 56.0ms    | 0.99x     | ≈ 引分   | WIN→TIE |
| int_tostring_1M    |  18.6ms       | 50.8ms    | **0.37x** | ✓ 勝ち   | **35→18.6ms 改善** |
| math_sqrt_1M       |   0.4ms       |  1.4ms    | **0.29x** | ✓ 勝ち   | WIN 維持 |

**成績：3勝/2分/6敗。** Almideは非退行（むしろ改善）だが、rustc 1.95 + wasmtime 42がmap 1.6→0.7ms / sort 6.7→4.8msと前進し2本が反転した。生データ・計測条件：`research/benchmark/stdlib/results/v0.25.0-vs-rust1.95.txt`（負荷あり機・min-of-5。sort / map_insertの反転は静音環境で要再確認）。バイナリサイズはAlmide 5.9KB対Rust 109KB（18.6x）。

<a id="教訓--タスク-ratio-ratchet-gate"></a>

### 教訓 → タスク：ratio-ratchet gate

凍結した相手への目標値は黙って腐る。下のFix PlanのExpected値は旧rustc相手の導出で、達成してもmapは約2.9x残る。

- [ ] **perf-ratchet gate**：Rust双子（`rust_wasm_compare/src/precise_all.rs`）を**current stable rustcで毎回再ビルド**し、min-of-5のratioをratchet（悪化でCI赤）。結果JSONは`make verify` dossierの性能項目を兼ねる。
- [ ] **Fix PlanのExpectedをrustc 1.95基準で再導出**（fold ≈TIE、concat ≈TIEの見込み。mapはhoist後も約2.9x。Rustの0.7ms ≒ 23GB/sは帯域律速で、そこまで詰める追加施策が11/11の本丸）。
- [ ] **sort_1M / map_insert_100kの反転調査**：静音環境で再計測し、実差なら根因分析。
- [ ] 着手順：fold（小）→ str_concat（小）→ map base-pointer hoist（中）→ filter（大）。

---

<a id="remaining-losses--root-cause--fix-plan"></a>

## 残る敗北 — 根因と修正計画

4敗すべては「こちらのemitがLLVMより非効率」であり、WASMの制約ではない。AlmideもRustもWASMを出力し、同じwasmtime JITで動く。LLVMも同じv128命令を使うが、より締まったloopを出力する。

<a id="1-list_fold-28x--simd-accumulator-not-implemented"></a>

### 1. list_fold（2.8x）— SIMD accumulator未実装

**根因**：こちらはscalar loopをemitし、LLVMはi64x2.add accumulatorをemitする。

```
Almide (scalar):
  loop: i64.load → i64.add acc → ptr += 8

LLVM (SIMD):
  loop: v128.load → i64x2.add acc_v128 → ptr += 16
  end:  extract_lane 0 + extract_lane 1 → final sum
```

**修正**：Int要素の`fold(init, (a, x) => a + x)`を検出し、i64x2 accumulatorと終端のhorizontal addをemit。4倍unroll。

**工数**：小。fold lambda bodyのpattern matchで、map SIMDと同じ手法。
**期待**：1.1ms → 約0.4ms（Rustに並ぶ）。

<a id="2-list_map-39x--simd-loop-overhead"></a>

### 2. list_map（3.9x）— SIMD loop overhead

**根因**：SIMDは実装されているが、v128 opごとに冗長な`local.get dst_ptr` / `local.get src_ptr`がある。LLVMは単一base pointerに相対的なload/storeの`offset`で回避する。

```
Almide:
  local.get dst_ptr          ;; redundant per-op
  local.get src_ptr          ;; redundant per-op
  v128.load offset=0
  ...
  v128.store offset=0

LLVM:
  v128.load offset=0 (base=src)   ;; no extra local.get
  ...
  v128.store offset=0 (base=dst)
```

**修正**：`local.get src_ptr`と`local.get dst_ptr`をunroll blockの外にhoistし、`local.tee`でbaseをstack上に維持。各unroll opはN=0,16,32,48の`v128.load offset=N`だけを使う。

**工数**：中。SIMD emitをstack-based addressingへ再構成。
**期待**：3.9ms → 約2ms。

<a id="3-str_concat-22x--initial-cap0-causes-20-grows"></a>

### 3. str_concat（2.2x）— 初期cap=0で20回拡張

**根因**：`var s = ""`はcap=0のstringを作る。growごとに`__string_append`を呼び、関数call overheadが生じる。1M appendで20回grow。Rustも0からgrowするが、LLVMはgrow経路を完全にinline化する。

**修正**：`var s = ""`を初期cap=16（または64）でemit。最初の16 appendは関数callゼロのinline byte storeのみになる。さらにgrow経路全体をinline化し、string_append callを避ける。

**工数**：小。空stringのemitにcapacityを含める。
**期待**：2.6ms → 約1.5ms。

<a id="4-list_filter-15x--scalar-predicate"></a>

### 4. list_filter（1.5x）— scalar述語

**根因**：こちらのbranchless filterは良好だがscalar。LLVMはSIMDで述語検査をbatch化する可能性がある。

**修正**：`x % 2 == 0`（ModInt peepholeで当時は`x & 1 == 0`）のような単純述語にSIMD batch検査をemit：`v128.load` → `v128.and(mask)` → `i64x2.eq(zero)` → 条件付きcopy。WASM SIMDにcompress-storeがないため難しい。

**工数**：大。SIMD filterには工夫が必要。
**期待**：2.6ms → 約2.0ms（控えめな改善。SIMD filterは難しい）。

---

<a id="completed-optimizations-v0233--v0234"></a>

## 完了した最適化（v0.23.3 + v0.23.4）

<a id="session-1-v0233-14-optimizations"></a>

### Session 1（v0.23.3）：14件の最適化

| # | 最適化 | 効果 | ファイル |
|---|---|---|---|
| 1 | hash table map（open addressing） | mapを1000倍高速化 | calls_map.rs, list_layout.rs |
| 2 | sort run検出（昇順/降順） | sortを5倍高速化 | calls_list_helpers.rs |
| 3 | lambda inline化（captureなし） | map/filter/fold ~2x | calls_list_closure2.rs, pass_closure_conversion.rs |
| 4 | 二分再帰変換 | fibを2倍高速化 | pass_tco.rs, target.rs |
| 5 | stream融合（map→filter→fold） | pipelineを4倍高速化 | calls_list_closure2.rs |
| 6 | 分岐なしfilter | filterを1.5倍高速化 | calls_list_closure2.rs |
| 7 | pointerによる反復 | loop overhead削減 | calls_list_closure2.rs, calls_list_helpers.rs |
| 8 | WASM pipelineのTCO | 末尾再帰 → loop | target.rs |
| 9 | 適応型scratch local | fib local数88→8 | functions.rs |
| 10 | string layout移行（data@8） | 10以上のfileを修正 | runtime.rs, rt_*.rs, calls_*.rs |
| 11 | Swiss Table layout（1-byte tag） | cacheに適したprobe | calls_map.rs, expressions.rs |
| 12 | map.get??default → get_or融合 | Option heap alloc除去 | pass_peephole.rs |
| 13 | sort向け1-pass逆順copy | sortがRustに並ぶ | calls_list_helpers.rs |
| 14 | pointerによるlist.map | idx乗算除去 | calls_list_closure2.rs |

<a id="session-2-v0234-13-optimizations"></a>

### Session 2（v0.23.4）：13件の最適化

| # | 最適化 | 効果 | ファイル |
|---|---|---|---|
| 15 | SIMD v128 list.map（4倍unroll） | map SIMD経路 | calls_list_closure2.rs, wasm_macro.rs |
| 16 | 1文字string appendをinline化 | 関数callを省略 | statements.rs |
| 17 | string_append memory.copy | 3個のbyte loop除去 | runtime.rs |
| 18 | pointerによるfilter/fold | idx乗算除去 | calls_list_closure2.rs |
| 19 | ModInt peephole（x%n==0 → x&(n-1)==0） | i64.rem_s回避 | expressions.rs |
| 20 | map拡張倍率4倍 | resize回数13→7 | calls_map.rs |
| 21 | 指数的拡張allocator | 償却O(1)のgrow | runtime.rs |
| 22 | 初期memory 128KB | 最小footprint | mod.rs |
| 23 | 条件付きfs初期化 | fs非使用時preopen省略 | mod.rs, functions.rs, dce.rs |
| 24 | 積極的な関数DCE | element table整理 | mod.rs |
| 25 | dead data除去 | 未使用string削除 | dce.rs |
| 26 | offset 4096のstring pool | 安全なdata DCE | mod.rs |
| 27 | binary size：25KB → 336B | hello worldを98.7%削減 | 上記すべて |

<a id="tier-2--medium-term"></a>

## Tier 2 — 中期

<a id="21-escape-analysis-for-optionresult"></a>

### 2.1 Option/Resultのescape analysis

escapeしないOption/Resultを検出し、wrapperをheapに確保する代わりにWASM multi-value `(i32, i64)`で返す。

<a id="22-arena-allocator"></a>

### 2.2 Arena allocator

escapeしない割当のregion-based memory管理。escape analysisと組み合わせる。

<a id="23-partial-evaluation"></a>

### 2.3 部分評価

既知のcall siteで高階関数を特殊化する。

<a id="24-wasm-opt-integration-optional--done-2026-07-23"></a>

### 2.4 wasm-opt統合（任意）— 完了（2026-07-23）

`almide build --target wasm --wasm-opt`として出荷。上の「追加1%未満」という見積もりは旧v0 emitter内蔵DCE（廃止#782以前）に対する測定で、もう成立しない。v1のrenderer内reachability DCE（当sessionで入ったpreamble/import/function/data-segment pruningとfunction nameのみを残すname-section trim）はHello Worldの既定の検証済みmoduleを8,713 Bから770 Bに減らすが、`wasm-opt -Oz`はさらに28–43%を削減する（2026-07-23測定）：770→548 B（Hello）、11,965→6,868 B（Variant）。reachability DCEは到達しないunit全体しか削除できず、wasm-optの命令単位処理（local coalescing、inlining、dead-store elimination）はできない。両者は重複ではなく補完的。

厳格にopt-in、既定offを維持する。trust-spineは自らのcert付きrendering processが生成したbyteそのものを出荷し、`wasm-opt`はその出力に対する外部の未検証変換だからである。`--wasm-opt`自身の安全主張（Almide生成wasmの観測可能な挙動を変えない）は、単なる主張ではなく、専用差分gate `tests/wasm_runtime_opt_parity.rs::wasm_opt_parity_spec`をCI（`.github/workflows/ci.yml`）で実行することで裏付ける。詳細：[docs/wasm/WASM-OUTPUT.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/wasm/WASM-OUTPUT.md)。

<a id="25-memory-layout-type-safety"></a>

### 2.5 Memory layoutの型安全性

magic constant（SCRATCH_ITOA、NEWLINE_OFFSET）をnewtype wrapper（MemOffset、StringPoolOffset、HeapPtr）へ置換。整数定数とdata section offsetが衝突する類のbugを防ぐ。

<a id="tier-3--long-term"></a>

## Tier 3 — 長期

<a id="31-wasm-gc-proposal"></a>

### 3.1 WASM GC提案
<a id="32-wasm-component-model"></a>

### 3.2 WASM Component Model
<a id="33-profile-guided-optimization"></a>

### 3.3 Profile-Guided Optimization

---

<a id="principles"></a>

## 原則

1. **同じWASM、同じruntime。** AlmideとRustは同じwasmtime JIT向けのWASM bytecodeを出す。性能差はemit品質であってplatform制約ではない。
2. **言語知識 > 汎用最適化。** Almide固有の各変換（get_or fusion、itoa、sqrt）はLLVMの汎用相当処理を上回る。
3. **実際のbottleneckを測る。** map.getの問題はOption heap allocで、hash table layoutではなかった。最適化の前に必ずprofileする。
4. **1M規模で真実が見える。** 100k benchmarkはnoiseに支配される。必ず1M以上の規模で検証する。

コード図の説明訳：Almideのscalar loopはi64をloadし、accへ加算し、pointerを8進める。LLVMのSIMD loopはv128をloadし、i64x2 accumulatorに加算し、pointerを16進め、最後にlane 0と1を足す。mapの図ではAlmideの各opのpointer取得が冗長で、LLVMの例は追加のlocal.getなしでbase相対offsetを使う。コード内の表記・コメントは原文のまま保持した。
