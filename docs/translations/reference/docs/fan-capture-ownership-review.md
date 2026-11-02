# Fan sibling capture ownership (#2061)

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](fan-capture-ownership-review.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](fan-capture-ownership-review.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](fan-capture-ownership-review.zh-CN.md)

> Non-normative source-pinned historical reader snapshot. The original remains authoritative and is not replaced as a machine input or executable gate. No automatic synchronization. Recorded test results, counts, implementation status and proof claims are source-scoped, not independently rerun or established as current guarantees here. The unsupported Option-arm case and outstanding repository-wide A grade remain explicit limitations, not claimed successes. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/fan-capture-ownership-review.md)

`fan { list.sum(rows), list.len(rows) }` passed type checking but emitted
two `move` closures sharing the same Rust `rows` binding, causing E0382.
The first spawn moved the binding even though both calls only borrowed it.

CaptureClone now treats fan arms as implicit closures and prepares owned
read-only captures outside the entire Fan expression. Each arm receives a
distinct VarId and emitted name; each clone therefore precedes all spawns.
The original remains available after the fan expression. Borrowed slice
and string parameters use the same owned materialization as explicit
lambdas. Captures directly mutated by an arm retain their existing handling;
this change does not invent shared mutable state between threads.

The existing capture-binding implementation is shared by lambda and fan
lowering. CloneInsertion still counts uses inside existing Clone nodes,
but no longer inserts an extra clone beneath them. Call-argument ownership
logic and capture binding construction now have separate modules, keeping
each source below Codopsy's 800-line limit without changing its thresholds.

<a id="reference-research"></a>

## Reference research

The cloned Rust reference at commit
`0d31508599a7814a7044e9a7a871e3dc5f037753` now includes
`library/std/src/thread/scoped.rs`: `Scope::spawn` takes a closure value
with `FnOnce + Send` bounds (lines 201–206). Scoped lifetime support does
not let two move closures consume one owned value. Capture preparation
must happen before the closure value is passed into `spawn`. The existing
Gleam reference's `javascript/decision.rs::assign_subject` similarly binds
computed subjects before reuse to preserve evaluation. Both repos live in
`../almide-references`.

<a id="regression-coverage"></a>

## Regression coverage

`tests/fan_sibling_capture_test.rs` executes List, String, Map and record
captures, borrowed parameters, use after fan, and nested fan expressions
on native and structural WASM. It also inspects the user portion of emitted
Rust to reject repeated `.clone().clone()`. Map callbacks return scalar
values: returning an Option directly from a fan arm currently reaches a
separate structural `bind:unmapped` wall and is not claimed as supported.

Codopsy 2.2.0, unchanged configuration: CaptureClone, capture bindings,
CloneInsertion, and call-argument helpers each score 100/A. The codegen
crate scores 90/A; the all-crates aggregate is 83/B (two files unparsed by
Codopsy), so repository-wide A is still outstanding.
