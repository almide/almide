原文の description メタデータ：LayoutRegistry、WasmIR、WasmBuilder を中心とする WASM engine の全面再設計
<a id="almide-wasm-engine--complete-redesign"></a>

# Almide WASM Engine — 全面再設計

[![English](https://img.shields.io/badge/English-d0d7de?style=flat-square)](wasm-engine-redesign.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-24292f?style=flat-square)](wasm-engine-redesign.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](wasm-engine-redesign.zh-CN.md)

> 固定原文の非規範読者用スナップショット。正本は原文であり、自動同期しない。独立した実装検証や引用指示の実行は行わない。歴史的状態、計画、比較主張は原文の範囲を保持する。
> [Original source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/wasm-engine-redesign.md)

> 原文注記：done 配置は全 phase の出荷を意味せず、冒頭は完了日なしの歴史的 emitter 提案と明示する。本文の広い stack/layout correctness や有効 byte 主張は設計案であり、メモリ安全性・全意味論・現行 component 対応の独立証明ではない。単純な stack depth 概略は不変更で残し、完全な型/control-flow 検証とはみなさない。数値指標と Wasm/WASI version 名は歴史的なもので、再測定・現代化していない。


> **状態**：歴史的な設計提案。以下の基盤・残作業の説明は旧 emitter のもので、現行 compiler ではない。`done/` に archive 設計記録として保持するが、完了日は記録されておらず、配置が全提案 phase の出荷を意味しない。現行 structural emitter 経路と制限は [WASM-OUTPUT](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/wasm/WASM-OUTPUT.md) と [wasm_route.rs](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/src/wasm_route.rs) を参照。
> **動機**：手書き WASM 出力は correctness の最大の穴。また WASM 3.0、WASI Preview 2、Component Model はすべて、生の命令組立でなく適切な compile pipeline を要する。
> **目標**：40 file の手書き assembler を、型を認識し layout-safe、stack-verified な WASM compiler に置換し、core module または Component Model component を出力する。

<a id="problem-statement"></a>

## 問題の記述

現状の `emit_wasm/` は assembler：AlmideIR → 生の `wasm-encoder` 命令。memory access ごとに offset を手計算し、stack balance は人間が検証している。

**結果**：
- layout 変更 → 40 file を grep → 箇所を見落とす → 静かな memory corruption
- 静的 stack-effect 検証なし → runtime の stack bug
- Swiss Table map 反復は method ごとに 30 行以上の boilerplate
- WASI Preview 1 の生 import は Component Model ecosystem と相互運用できない
- WIT 生成なし → WASM component ecosystem に参加できない

<a id="architecture"></a>

## アーキテクチャ

```
AlmideIR (after nanopass pipeline)
  ↓
┌──────────────────────────────────────────────────┐
│  Almide WASM Engine v2                           │
│                                                  │
│  LayoutRegistry          ← memory layout SoT     │
│  Lowering (AlmideIR → WasmIR)                    │
│  Stack-Effect Verifier   ← static balance proof   │
│  Optimize (RC fusion, local coloring, peephole)   │
│  Core Emit (WasmIR → wasm-encoder)               │
│  Component Wrap (core → component + WIT)          │
│  WASI Adapter (canonical ABI for system calls)    │
│                                                  │
└──────────────────────────────────────────────────┘
  ↓                        ↓
Core WASM module     Component (.wasm + WIT)
```

図の訳：nanopass pipeline 後の AlmideIR → Almide WASM Engine v2。内部は memory layout の正本 LayoutRegistry、AlmideIR → WasmIR の lowering、静的 balance 証明の Stack-Effect Verifier、最適化（RC fusion・local coloring・peephole）、WasmIR → wasm-encoder の Core Emit、core → component + WIT の Component Wrap、system call の canonical ABI 用 WASI Adapter。出力は Core WASM module または Component（.wasm + WIT）。

<a id="key-design-decisions"></a>

### 主要な設計判断

1. **内部 layout は Almide 固有のまま**（LayoutRegistry）。内部データに canonical ABI overhead はなく、component 境界（import/export）のみ canonical ABI を使う。
2. **stack effect は注記でなく構造的**。各 WasmIR Op は `(pops: u8, pushes: u8)` を宣言する。verifier は block/function ごとの正味 stack effect を計算し、出力前に不一致を拒否する。
3. **Component Model は書換えでなく wrapper**。core module を通常どおりコンパイルし、別の Component Wrap phase が以下を生成して component に引き上げる：
   - Almide の `pub fn` / `pub type` export から WIT 宣言
   - canonical ABI adapter 関数（Almide layout ↔ canonical layout）
   - `wit-component` を介した WASI Preview 2 import
4. **WASI Preview 2 が Preview 1 を置換**。現状の生 import（`fd_write`、`clock_time_get` など）を Component Model import（`wasi:filesystem/types`、`wasi:http/outgoing-handler` など）に置換する。

<a id="core-components"></a>

## 中核 component

<a id="1-layoutregistry-exists"></a>

### 1. LayoutRegistry（存在）

全 memory layout を宣言的に定義し、ほかに hardcoded offset を置かない。

組込み layout：String、List、SwissMap、Set、AllocHeader、ClosurePair、Variant、Option、Result。

<a id="2-wasmir-with-stack-effects"></a>

### 2. stack effect を持つ WasmIR

```rust
pub struct WasmOp {
    pub kind: WasmOpKind,
    pub pops: u8,    // values consumed from stack
    pub pushes: u8,  // values produced onto stack
}

pub enum WasmOpKind {
    // ── Typed memory access (layout-safe) ──
    FieldLoad { layout: LayoutId, field: FieldId },   // pops: 1 (base), pushes: 1
    FieldStore { layout: LayoutId, field: FieldId },   // pops: 2 (base, val), pushes: 0
    FieldAddr { layout: LayoutId, field: FieldId },    // pops: 1 (base), pushes: 1 (addr)

    // ── Collection iteration (single IR node) ──
    ListForEach { body: Vec<WasmOp> },     // pops: 1 (list), pushes: 0
    MapForEach { body: Vec<WasmOp> },      // pops: 1 (map), pushes: 0

    // ── Allocation ──
    Alloc { layout: LayoutId, size: SizeExpr },
    AllocCollection { layout: LayoutId, len_local: Local, stride: u32 },

    // ── Reference counting ──
    RcInc,                                  // pops: 1 (ptr), pushes: 0
    RcDec { layout: LayoutId },            // pops: 1 (ptr), pushes: 0
    CowCheck { clone_body: Vec<WasmOp> },  // pops: 1 (ptr), pushes: 1 (ptr or clone)

    // ── Structured control flow ──
    Block { body: Vec<WasmOp>, result_ty: Option<ValType> },
    Loop { body: Vec<WasmOp> },
    If { then: Vec<WasmOp>, else_: Vec<WasmOp>, result_ty: Option<ValType> },
    Br(u32),
    BrIf(u32),
    Return,
    ReturnCall(FuncId),          // WASM 3.0 tail call

    // ── Stack primitives ──
    LocalGet(Local),
    LocalSet(Local),
    LocalTee(Local),
    Const(WasmConst),
    Drop,
    BinOp(WasmBinOp),
    Call(FuncId),
    CallIndirect { sig: SigId },

    // ── High-level ops (lowered to primitives in emit) ──
    StringConcat,                           // pops: 2, pushes: 1
    StringInterp { part_count: u32 },       // pops: N, pushes: 1
    DeepEq { ty: AlmideTy },               // pops: 2, pushes: 1 (i32 bool)

    // ── Component Model canonical ABI ──
    CanonLift { wit_func: WitFuncId },      // core → component boundary
    CanonLower { wit_func: WitFuncId },     // component → core boundary
    CanonStringToMemory,                    // canonical string → Almide string
    CanonStringFromMemory,                  // Almide string → canonical string
    CanonListToMemory { elem_ty: ValType }, // canonical list → Almide list
    CanonListFromMemory { elem_ty: ValType },

    // ── Memory ──
    MemoryCopy { dst_mem: u32, src_mem: u32 },
    MemoryGrow(u32),
    MemorySize(u32),

    Seq(Vec<WasmOp>),
    Unreachable,
}
```

コード注釈の訳：pops は stack から消費する値数、pushes は生成する値数。型付き memory access は layout-safe で、FieldLoad は base 1 個から値 1 個、FieldStore は base と値 2 個から 0 個、FieldAddr は base 1 個から address 1 個。collection iteration は単一 IR node で list/map 1 個から 0 個。続いて allocation、reference counting（RcInc/RcDec は pointer 1 個から 0 個、CowCheck は pointer 1 個から元 pointer または clone 1 個）、構造化 control flow（ReturnCall は WASM 3.0 tail call）、stack primitive。高水準 operation は emit で primitive に lowering され、StringConcat は 2→1、StringInterp は N→1、DeepEq は 2→i32 bool 1 個。Component Model canonical ABI では core/component 境界の lift/lower、canonical string/list と Almide string/list の相互変換を行い、末尾に memory operation がある。

<a id="3-stack-effect-verifier"></a>

### 3. Stack-Effect Verifier

```rust
fn verify_stack_balance(ops: &[WasmOp], expected_result: Option<ValType>) -> Result<(), StackError> {
    let mut depth: i32 = 0;
    for op in ops {
        depth -= op.pops as i32;
        if depth < 0 { return Err(StackError::Underflow { op, depth }); }
        depth += op.pushes as i32;
    }
    let expected = if expected_result.is_some() { 1 } else { 0 };
    if depth != expected {
        return Err(StackError::Imbalance { expected, actual: depth });
    }
    Ok(())
}
```

各 Block/If/Loop body で再帰実行し、入れ子 scope は独立に検証する。

<a id="4-lowering-almideir--wasmir"></a>

### 4. Lowering（AlmideIR → WasmIR）

意味的翻訳。各 AlmideIR node を WasmIR operation に対応づける。

<a id="5-core-emit-wasmir--wasm-encoder"></a>

### 5. Core Emit（WasmIR → wasm-encoder）

機械的翻訳。生 WASM 命令に触れる**唯一**の箇所。正しいと検証済みの WasmIR を入力し、有効な WASM byte を出力する。

<a id="6-component-wrap"></a>

### 6. Component Wrap

core module から Component Model component を生成：

```
Core WASM module
  + WIT declarations (from pub fn/type exports)
  + Canonical ABI adapter functions
  + WASI Preview 2 import declarations
  = Component (.wasm)
```

図の訳：Core WASM module + pub fn/type export 由来の WIT 宣言 + canonical ABI adapter 関数 + WASI Preview 2 import 宣言 = Component（.wasm）。

重い処理に `wit-component` crate を使い、Almide は次を生成：
- **型から WIT**：`pub type User = { name: String, age: Int }` → `record user { name: string, age: s64 }`
- **関数から WIT**：`pub fn greet(name: String) -> String` → `greet: func(name: string) -> string`
- **canonical ABI adapter**：export 境界で Almide 内部 layout と canonical ABI の間を変換

<a id="7-wasi-preview-2-adapter"></a>

### 7. WASI Preview 2 Adapter

生 WASI Preview 1 import を Component Model import に置換：

| 現行（Preview 1）| 新方式（CM 経由 Preview 2）|
|---------------------|----------------------|
| `fd_write(fd, iovs, ...)` | `wasi:cli/stdout.get-stdout` + `output-stream.write` |
| `fd_read(fd, iovs, ...)` | `wasi:cli/stdin.get-stdin` + `input-stream.read` |
| `path_open(...)` | `wasi:filesystem/types.open-at` |
| `clock_time_get(...)` | `wasi:clocks/monotonic-clock.now` |
| （なし）| `wasi:http/outgoing-handler.handle` |

<a id="implementation-phases"></a>

## 実装段階

<a id="phase-0-stack-effect-verifier"></a>

### Phase 0：Stack-Effect Verifier

既存 WasmIR Op enum に `(pops, pushes)` を追加し verifier を実装、WasmBuilder 出力で実行する。これだけで engine 層の correctness の穴を閉じるという構想。

- [ ] WasmIR Op に stack-effect field を追加
- [ ] `verify_stack_balance()` 再帰 checker を実装
- [ ] WasmBuilder に接続し、各関数構築後に検証
- [ ] gate：`almide test spec/ --target wasm` で stack-effect 違反ゼロ

<a id="phase-1-new-lowering"></a>

### Phase 1：新しい lowering

手書き出力を AlmideIR → WasmIR lowering に置換。

- [ ] 全 IrExprKind variant の `lower_expr()`
- [ ] 全 IrStmtKind variant の `lower_stmt()`
- [ ] local allocation と構造化 control flow を持つ `lower_function()`
- [ ] Core emit：WasmIR → wasm-encoder（機械的）
- [ ] gate：全 `spec/lang/` + `spec/stdlib/` が新 pipeline に合格（240/240）

<a id="phase-2-component-model"></a>

### Phase 2：Component Model

- [ ] Almide pub export から WIT 生成（`almide build --target wasm-component`）
- [ ] canonical ABI adapter 生成（string、list、record、variant、option、result）
- [ ] component wrapping に `wit-component` を統合
- [ ] gate：compile した component が `wasm-tools validate --features component-model` に合格

<a id="phase-3-wasi-preview-2"></a>

### Phase 3：WASI Preview 2

- [ ] fd_write/fd_read を `wasi:cli/std{in,out,err}` に置換
- [ ] path_open/fd_seek などを `wasi:filesystem/types` に置換
- [ ] clock_time_get を `wasi:clocks/monotonic-clock` に置換
- [ ] HTTP client 用 `wasi:http/outgoing-handler` を追加
- [ ] gate：`almide run app.almd --target wasm` が WASI Preview 2 の wasmtime 上で動作

<a id="phase-4-exception-handling-wasm-30-v2"></a>

### Phase 4：例外処理（WASM 3.0 v2）

wasmtime で既定有効になるのを待つ。

- [ ] effect fn の `?` 伝播に `try_table`/`throw`
- [ ] zero-cost error path（Result heap allocation なし）
- [ ] gate：effect fn benchmark に測定可能な改善

<a id="phase-5-component-model-async-wasm-30-v3"></a>

### Phase 5：Component Model Async（WASM 3.0 v3）

- [ ] `fan` → `future<T>` + `waitable-set` 多重化
- [ ] streaming data 用 `stream<T>`
- [ ] gate：fan benchmark が WASI 0.3 の wasmtime 上で動作

<a id="what-dies"></a>

## 廃止するもの

現状の `emit_wasm/` 全体（40 file、約15K LOC）：

| 現行 | 置換先 |
|---------|------------|
| `expressions.rs`、`statements.rs`、`control.rs` | `lower_expr()`、`lower_stmt()` |
| `calls_*.rs`（12 file）| stdlib lowering rule |
| `rt_*.rs`（6 file）| WasmIR sequence としての runtime |
| `runtime.rs`、`runtime_eq.rs` | emit phase primitive |
| `closures.rs`、`calls_lambda.rs` | closure lowering |
| `equality.rs`、`collections.rs` | `DeepEq` operation、collection lowering |
| `scratch.rs` | emit phase の local allocator |
| `values.rs` | LayoutRegistry |
| `dce.rs` | WasmIR レベル DCE pass |

残す：`engine/`（LayoutRegistry、WasmIR、WasmBuilder、emit）。これが新 engine である。

<a id="existing-foundation"></a>

## 既存の基盤

`emit_wasm/engine/` に既に実装：
- **LayoutRegistry**：全 layout 定義（String、List、SwissMap、Set、AllocHeader、ClosurePair、Variant、Option、Result）
- **WasmIR**：型付き memory access を持つ 40 以上の operation variant
- **WasmBuilder**：layout-safe field access、collection iteration、RC operation を持つ chainable API
- **Emit**：WasmIR → wasm-encoder 翻訳

23 file が既に LayoutRegistry へ移行済み。WasmBuilder は list_layout.rs と Perceus core で使用。

<a id="metrics"></a>

## 指標

- **correctness**：`almide test spec/ --target wasm` — 240/240
- **stack safety**：stack-effect 違反ゼロ（構造的強制）
- **layout safety**：hardcoded offset ゼロ（LayoutRegistry で強制）
- **binary size**：≤ 現状
- **performance**：≥ 現状
- **code 量**：emit_wasm/ LOC を ≥50% 削減
- **Component Model**：`wasm-tools validate --features component-model` に合格
- **WASI Preview 2**：Preview 1 adapter なしで wasmtime 上で実行
