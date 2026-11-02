<a id="phase-0-決定ゲート--結果記録"></a>

# Phase 0 decision gate — results record

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](GATE.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](GATE.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](GATE.zh-CN.md)

> Non-normative historical reader snapshot. The pinned original is authoritative; no automatic synchronization. No independent rerun of measurements, proof gates or current implementation checks. Quoted commands are not executed. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/research/spike/v1-mir/GATE.md)
> The five-shape experiment is historical evidence, not a general theorem over all programs. The source table describes list_get_643 as leak/divergence while finding 3 calls #643 RC double-free; this source inconsistency is retained, not resolved by inventing results.

Status: **PASSED — 5/5 shapes, 0 conditional, 0 fail, 0 escape hatch (2026-06-13)**
Spec: [docs/roadmap/active/v1-mir-architecture.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/v1-mir-architecture.md) §8
Rerun: `research/spike/v1-mir/run-gate.sh` (validated with rustc 1.95.0)

<a id="ゲートが問うたこと"></a>

## What the gate asked

Kill or prove the core thesis of §2.2 —

> **Perceus RC is a general model; affine/move is merely the special case of RC whose count is statically known to be 1.**

— on the five most troublesome ownership shapes. Each shape has **one Perceus ownership decision** in minimal MIR and renders it as both **(A) idiomatic Rust (move/borrow/clone)** and **(B) reference counting (handwritten RC modeling wasm semantics)**. Acceptance conditions:

1. A and B **agree by construction** (identical output).
2. Faithful RC has no leak / double-free.
3. A renderer that **re-decides ownership** (buggy variant) **is caught**.

Each shape is a self-contained meta-harness: construct the MIR decision, render it two ways, compile both with `rustc --edition 2021 -O`, execute and compare, and exit 0 only on PASS.

<a id="結果"></a>

## Results

| # | shape | One decision | A=B | RC clean | buggy caught | escape hatch | verdict |
|---|---|---|---|---|---|---|---|
| 1 | alias_return | Last consume = transfer payload ptr and release **only the shell** | ✓ | ✓ | double-free | none | **PASS** |
| 2 | list_get_643 | alias-inc + scope-dec; per-iter drop of iteration heap temp | ✓ | ✓ | leak/divergence | none | **PASS** |
| 3 | boxed_pattern_610 | **Borrow through the box** for a boxed field; Leaf payload is Scalar/Copy | ✓ | ✓ | double-free | none | **PASS** |
| 4 | closure_capture | capture = Dup into env + Drop at closure-drop; each call borrows | ✓ | ✓ | rustc reject / double-free | none | **PASS** |
| 5 | alias_cow | Mutation of potentially shared ref **starts with MakeUnique** (clone \| cow_check) | ✓ | ✓ | value destruction (identical in both idioms) | none | **PASS** |

Shape files: `src/main.rs` (#2, cargo entry) + `shapes/{alias_return,boxed_pattern_610,closure_capture,alias_cow}.rs`.

<a id="正準形を精緻化する-4-つの正直な発見"></a>

## Four honest findings refining the canonical form

The gate revealed not only “PASS” but four **boundaries** of the canonical form in §2.2. These are not escape hatches (breaks in the canonical form) but clarifications of **what it decides once** (reflected in design doc §2.2/§3).

1. **Ownership polarity is a per-binding/per-parameter MIR fact** (alias_return). Even for the same source, consume yields move, while borrow yields clone/dup. This is not a renderer re-decision: MIR decides it **once per binding/parameter**. “Last consume → move” in canonical form §2.2 is the case where this polarity is consume.

2. **SYNTACTIC differences may be absorbed; SEMANTIC decisions may not** (boxed_pattern_610). Rust lacks a box-pattern (`box Leaf(a)` is unstable), so a nested ctor in a boxed field renders as a tag-guard + `&**deref` nested-match. This is a **syntactic difference**, not an ownership/layout decision. Renderers may resolve target syntax differences but **must never re-decide** ownership/layout.

3. **The buggy observation signature differs by shape** (alias_cow vs 643). Even within the same “renderer re-decided ownership” class, #643 appears as **RC double-free**, while AliasCow appears as **wrong-output** (a value-semantics violation: RC **stays balanced**, and both idioms destroy `a` identically). → Phase 1 validation therefore needs **value equivalence**, not just leak/double-free detection. The gate was honestly built for these signatures, not rigged with a false double-free.

4. **Captures that are shared + mutated across calls + returned mark the boundary of the Rc<RefCell> region** (closure_capture scope note). This gate's read-only capture-called-twice rendered as a plain owned-String move-closure (Fn), with zero Rc/RefCell. But a capture “shared, mutated across multiple calls, and returned” belongs to the alias_cow shape: that is where explicit Rc begins. Phase 1 handles it as MIR MakeUnique/shared mutation (a known edge of the canonical form).

<a id="結論"></a>

## Conclusion

In all five shapes, **one Perceus ownership decision rendered faithfully into both idioms and agreed**, and only the buggy versions re-deciding ownership were caught. RC and Rust move/borrow **share one canonical form**. Under the passing branch of §8, **proceed to Phase 1 (MIR core + two renderers)**. The four points above feed Phase 1 MIR design: per-binding polarity, syntax-versus-semantics layering, value-equivalence gates, and the shared-mutation edge.
