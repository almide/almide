<!-- description: Detect user fns whose signature matches a stdlib fn, suggest delegation -->
# Reimpl Lint: Signature-Match Detection of Stdlib Reimplementations

Trigger: implement when dojo measurement shows `list.binary_search` /
`string.run_length_encode` / other stdlib primitives are being
reimplemented from scratch (after SYSTEM_PROMPT "Prefer stdlib" section
is applied and still doesn't move the needle).

## Goal

Emit an info/warning-level diagnostic when a user-defined top-level `fn`
has a **signature exactly matching** a stdlib function whose name is very
similar. The diagnostic points to the stdlib function and suggests
delegating, so LLM retries can converge to the shorter, idiomatic form.

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

## Non-triggers (explicitly must not fire)

- Different parameter order (`fn lookup(target: Int, xs: List[Int])`)
- Different arg type (`fn binary_search(xs: List[Float], target: Float)`)
- Different return shape (`fn binary_search(...) -> Int` returning -1 on miss)
- Name distance > 2 (`fn find_by_binary_search`)
- User-defined generic that the stdlib version isn't (and vice versa)

When in doubt, do NOT fire. This is an optional optimization, not a
correctness check.

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

## What this lint does NOT do

- Does not detect algorithm-by-shape (while + mid + low + high → binary
  search). That's a different tool (maybe Phase 5 as a real static
  analyzer); too fuzzy for Phase 3.
- Does not rewrite the body.
- Does not apply via `almide fix` — the user (or LLM retry) must
  consciously accept the shim.

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

## When to implement

- dojo run #10 (with `almide fix` retry loop integration + "Prefer
  stdlib" SYSTEM_PROMPT section) lands at ≤ +2pt vs current baseline.
- Or: direct evidence from log that binary-search / similar tasks still
  reimplement stdlib after the harness improvements.

Otherwise defer — the measurement window is cheap, the implementation
is ~50 lines of checker code plus tests.
