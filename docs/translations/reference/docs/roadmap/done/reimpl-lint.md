<a id="reimpl-lint-signature-match-detection-of-stdlib-reimplementations"></a>

# Reimpl Lint: Signature-Match Detection of Stdlib Reimplementations

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](reimpl-lint.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](reimpl-lint.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](reimpl-lint.zh-CN.md)

> Non-normative reader snapshot of the [English canonical source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/done/reimpl-lint.md), pinned at `35297a612fb29e3f78f6587aba253aa91d51f466`. The original remains authoritative. This snapshot is not automatically synchronized and is not generator/checker input. Dates, plans, completion status, measurements and comparative claims below are retained as historical source statements, not independently reproduced or validated for a current release. Code blocks and comments remain verbatim. Historical collection metadata is displayed as an inactive reader note.

> Historical metadata: Detect user fns whose signature matches a stdlib fn, suggest delegation; done — 2026-04-20.

> Source caveat: the completion section records Warning and a one-way stdlib-TypeVar wildcard; the retained older plan says Info, describes different generic matching and excludes both generic directions. These historical differences are not silently reconciled. Near-zero false-positive claims and +1/+2 measurement hypotheses are the source’s rationale, not independently demonstrated semantic equivalence or fresh benchmark results. The delegation shim remains opt-in.

<a id="completion-status-2026-04-20"></a>

## Completion status (2026-04-20)

Landed as `E015 Possible stdlib reimplementation` — a Warning-level
diagnostic fired from `check_reimpl_lint` (`crates/almide-frontend/src/check/mod.rs`).

Detection rule:

1. Top-level `fn` (not test / derive / monomorphised / convention method).
2. Short name within Levenshtein ≤ 2 of a stdlib short name
   (case-insensitive; the existing `almide_base::diagnostic::levenshtein`
   helper is now `pub` for cross-crate use).
3. Parameter arity + types + return type match structurally
   (`ty_reimpl_eq` in `check/mod.rs`). `TypeVar` on the stdlib side
   is a wildcard (monomorphic user fn matches generic stdlib fn);
   the reverse does not match.

All three must hold; any miss suppresses the suggestion so
false-positive rate stays near zero.

Output is a warning + delegation `try:` shim (`fn <user_name>(args) ->
T = <module>.<stdlib_fn>(args)`). Doc file at
`docs/diagnostics/E015.md`. The shim is not auto-applied — user or
LLM retry must consciously accept it.

Levels: stayed at `Warning` rather than adding `Info` to the
`Level` enum. Matches roadmap allowance of "info/warning-level" and
avoids the cross-crate variant cascade. Revisit if dojo measurement
shows warnings get too noisy.

<a id="original-plan-retained-for-history"></a>

## Original plan (retained for history)

Trigger: implement when dojo measurement shows `list.binary_search` /
`string.run_length_encode` / other stdlib primitives are being
reimplemented from scratch (after SYSTEM_PROMPT "Prefer stdlib" section
is applied and still doesn't move the needle).

<a id="goal"></a>

## Goal

Emit an info/warning-level diagnostic when a user-defined top-level `fn`
has a **signature exactly matching** a stdlib function whose name is very
similar. The diagnostic points to the stdlib function and suggests
delegating, so LLM retries can converge to the shorter, idiomatic form.

<a id="scope-discipline"></a>

## Scope discipline

This lint **must not** emit false positives. A user writing a specialized
version of `list.first` for their own reasons should not be nagged. The
trigger condition is strict:

- Name similarity: Levenshtein distance ≤ 2 between user fn name and
  stdlib fn name (short-form), AND
- Parameter signature: types structurally identical (element-wise), AND
- Return type: structurally identical.

All three conditions must hold. Signature-exact match makes
false-positive rate essentially zero: if a user writes
`fn binary_search(xs: List[Int], target: Int) -> Option[Int]`, the
probability they meant something *different* from `list.binary_search` is
vanishingly small.

<a id="example-binary-search-case"></a>

## Example (binary-search case)

```almide
// User writes (~30 lines of algorithm):
fn binary_search(xs: List[Int], target: Int) -> Option[Int] =
    let low = 0
    let high = list.len(xs) - 1
    // ... loop implementation ...
```

Diagnostic emitted:

```
info: fn 'binary_search' has the same signature as stdlib `list.binary_search`
  --> binary_search.almd:1:4
  |
1 | fn binary_search(xs: List[Int], target: Int) -> Option[Int] =
  |    ^^^^^^^^^^^^^^^^^
  hint: if this is the standard algorithm, delegate to stdlib:
  try:
      fn binary_search(xs: List[Int], target: Int) -> Option[Int] =
        list.binary_search(xs, target)
```

The `try:` snippet is a **delegation shim**, not a rewrite of the user's
body. User (or LLM retry) chooses whether the existing body is worth
keeping or should be replaced with the one-liner.

<a id="non-triggers-explicitly-must-not-fire"></a>

## Non-triggers (explicitly must not fire)

- Different parameter order (`fn lookup(target: Int, xs: List[Int])`)
- Different arg type (`fn binary_search(xs: List[Float], target: Float)`)
- Different return shape (`fn binary_search(...) -> Int` returning -1 on miss)
- Name distance > 2 (`fn find_by_binary_search`)
- User-defined generic that the stdlib version isn't (and vice versa)

When in doubt, do NOT fire. This is an optional optimization, not a
correctness check.

<a id="implementation-sketch"></a>

## Implementation sketch

1. **Table build (once per checker instance)**: collect
   `(module, fn_name, param_tys, ret_ty, generics)` for every stdlib
   function visible in scope. Source: `almide_frontend::stdlib::module_functions`
   + `lookup_sig`. Group by short-name (the part after the module
   prefix).

2. **Per user fn**: after inference, iterate top-level `Decl::Fn`:
   - Compute `user_key = (param_tys, ret_ty)` from the signature.
   - Look up candidate stdlib fns whose short-name has edit distance ≤ 2
     from the user fn name.
   - For each candidate: compare `(param_tys, ret_ty)` with `user_key`
     structurally (see below). If match: emit the info diagnostic.

3. **Structural equality on types**:
   - `Ty::Int == Ty::Int`, etc.
   - `Ty::Applied(id_a, args_a) == Ty::Applied(id_b, args_b)` iff
     `id_a == id_b && args elementwise ==`.
   - `Ty::TypeVar("A")` is considered equal to another `TypeVar` at
     the same position (monomorphic signatures should match user-side
     polymorphic ones — no generalization pretense).
   - Protocol / structural bounds are ignored (stdlib fns rarely have
     them; if they do and the user doesn't, skip).

4. **Diagnostic**: attach a `try_snippet` that's the delegation shim.
   Severity `Info` (not `Warning`) so it never gates compilation.

<a id="what-this-lint-does-not-do"></a>

## What this lint does NOT do

- Does not detect algorithm-by-shape (while + mid + low + high → binary
  search). That's a different tool (maybe Phase 5 as a real static
  analyzer); too fuzzy for Phase 3.
- Does not rewrite the body.
- Does not apply via `almide fix` — the user (or LLM retry) must
  consciously accept the shim.

<a id="measurement-hypothesis"></a>

## Measurement hypothesis

If implemented and enabled:

- 70b: +1 on binary-search (the exact case that motivated this).
- 70b: +1 on run-length-encoding (if signature matches; currently stdlib
  returns `List[(String, Int)]` so user-side `String`-returning RLE
  would NOT match — by design, see Non-triggers).
- 8b: minimal effect (8b rarely writes signatures that exactly match
  stdlib — it usually picks wrong types).

Total expected: **+1 to +2 at 70b**. Low risk because the diagnostic is
`Info` level — no blocked compilation, only retry nudging.

<a id="when-to-implement"></a>

## When to implement

- dojo run #10 (with `almide fix` retry loop integration + "Prefer
  stdlib" SYSTEM_PROMPT section) lands at ≤ +2pt vs current baseline.
- Or: direct evidence from log that binary-search / similar tasks still
  reimplement stdlib after the harness improvements.

Otherwise defer — the measurement window is cheap, the implementation
is ~50 lines of checker code plus tests.
