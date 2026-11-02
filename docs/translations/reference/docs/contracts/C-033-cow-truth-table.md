# C-033 — Copy-on-write for aliased mutables

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](C-033-cow-truth-table.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](C-033-cow-truth-table.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](C-033-cow-truth-table.zh-CN.md)

> Non-normative, source-pinned reader snapshot; unchanged original remains authoritative. No automatic synchronization, independent execution, measurements or fresh verification of contract guarantees. Current and historical statements refer to the pinned source. C-006 distinguishes removal of the old timeout form from its later cooperative block form. C-033 preserves pre-fix diagnosis and closure instructions as history; do not execute its old deletion/gate-edit instructions. Repr helper/export details describe the historical emitter, not the current public ABI. Code and comments remain verbatim. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/contracts/C-033-cow-truth-table.md)

> Prose companion to contract C-033 in [contracts.toml](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/contracts/contracts.toml).
> Status: **active** — FIXED by AliasCowPass + native pass_clone (PR #394,
> 2026-06-06): the truth table below is now byte-identical on both targets,
> locked by `spec/wasm_cross/alias_cow.almd`. The history below was ported from
> the formerly-grandfathered `cow_check` note in
> `crates/almide-codegen/rt-oracle-registry.toml` (retired with the v0 emitter, #782 —
> see git history; the live gates are spec/wasm_cross + the interp oracle).
> Before that fix this was a doc-only record of a divergence. The sections
> below preserve that pre-fix diagnosis and its closure criteria; they are
> not current implementation status.

## The promise

```almide
var a = [1, 2, 3, 4, 5]
var b = a        // b is a value copy, not an alias
a[0] = 999       // mutate a in place
// a == [999, 2, 3, 4, 5], b == [1, 2, 3, 4, 5]   (b UNCHANGED)
```

Value semantics: `var b = a` copies, so a later in-place mutation of `a` must not
be observable through `b`. The mechanism is copy-on-write — copy the backing
buffer at the first in-place mutation of a shared value.

## Historical divergence (before the fix)

| Step | native (Rust) | wasm |
|------|---------------|------|
| `var a = [1,2,3,4,5]` | `a` -> buffer A | `a` -> heap buffer A |
| `var b = a` | should `Rc::make_mut` / clone-on-write | coalesces `a` and `b` into ONE local; copy-propagates the alias |
| `a[0] = 999` | classification gap: minimal repro hits **E0382** move-after-use | mutates buffer A **in place** (no COW) |
| read `a`, `b` | (does not compile on the minimal repro) | `a = 999,2,3,4,5` **and** `b = 999,2,3,4,5` |

Observed probe: native `999,2,3,4,5 | 1,2,3,4,5` vs wasm
`999,2,3,4,5 | 999,2,3,4,5` — `b` is corrupted on wasm.

## Why the historical runtime-only fix was insufficient

`__cow_check` (the wasm routine meant to prevent this) has **no callers** in the
wasm emitter — `IndexAssign` / `FieldAssign` / `ListSwap` / `ListReverse` all
mutate in place. Wiring `__cow_check` at `IndexAssign` is **necessary but not
sufficient**: the wasm pipeline also **coalesces aliased mutable `var`s into one
local** and copy-propagates the alias, so heap COW has nothing to separate. And
native itself has matching VarStorage classification gaps — the same minimal
repro hits E0382 move-after-use on the Rust target.

A correct fix spans the **VarStorage / alias-coalescing model on both targets**:
a write-back-to-distinct-local COW at every in-place mutation site **plus**
suppression of mutable-var alias copy-propagation. That is out of scope for the
runtime-routine drain, so the routine is left **grandfathered honestly** rather
than force a false "verified" flip.

## Historical closure criteria (fulfilled by the fix above)

When the VarStorage/alias work lands: add a `var b = a; a[i] = v; read both`
fixture to `spec/wasm_cross/`, flip `C-033` to `status = "active"` with that
fixture as `class = "fixture"` evidence, drop this doc, and lower
`MAX_GRANDFATHERED` in `scripts/check-rt-oracle-registry.sh` and `MAX_FLAGGED` in
`scripts/check-contracts.sh` — **all in the same PR**. The flagged count is a
ratchet; it may only go down.
