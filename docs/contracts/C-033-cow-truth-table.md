# C-033 — Copy-on-write for aliased mutables

> Prose companion to contract C-033 in [contracts.toml](contracts.toml).
> Status: **active** — FIXED by AliasCowPass + native pass_clone (PR #394,
> 2026-06-06): the truth table below is now byte-identical on both targets,
> locked by `spec/wasm_cross/alias_cow.almd`. The history below was ported from
> the formerly-grandfathered `cow_check` note in
> [crates/almide-codegen/rt-oracle-registry.toml](../../crates/almide-codegen/rt-oracle-registry.toml).
> This is the SOLE doc-only contract — it records a real, currently-unfixable
> native<->wasm divergence, not a behaviour we can yet certify.

## The promise (what *should* hold)

```almide
var a = [1, 2, 3, 4, 5]
var b = a        // b is a value copy, not an alias
a[0] = 999       // mutate a in place
// a == [999, 2, 3, 4, 5], b == [1, 2, 3, 4, 5]   (b UNCHANGED)
```

Value semantics: `var b = a` copies, so a later in-place mutation of `a` must not
be observable through `b`. The mechanism is copy-on-write — copy the backing
buffer at the first in-place mutation of a shared value.

## The divergence (what actually happens)

| Step | native (Rust) | wasm |
|------|---------------|------|
| `var a = [1,2,3,4,5]` | `a` -> buffer A | `a` -> heap buffer A |
| `var b = a` | should `Rc::make_mut` / clone-on-write | coalesces `a` and `b` into ONE local; copy-propagates the alias |
| `a[0] = 999` | classification gap: minimal repro hits **E0382** move-after-use | mutates buffer A **in place** (no COW) |
| read `a`, `b` | (does not compile on the minimal repro) | `a = 999,2,3,4,5` **and** `b = 999,2,3,4,5` |

Observed probe: native `999,2,3,4,5 | 1,2,3,4,5` vs wasm
`999,2,3,4,5 | 999,2,3,4,5` — `b` is corrupted on wasm.

## Why it cannot be flipped today

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

## How this clears (the ratchet rule)

When the VarStorage/alias work lands: add a `var b = a; a[i] = v; read both`
fixture to `spec/wasm_cross/`, flip `C-033` to `status = "active"` with that
fixture as `class = "fixture"` evidence, drop this doc, and lower
`MAX_GRANDFATHERED` in `scripts/check-rt-oracle-registry.sh` and `MAX_FLAGGED` in
`scripts/check-contracts.sh` — **all in the same PR**. The flagged count is a
ratchet; it may only go down.
