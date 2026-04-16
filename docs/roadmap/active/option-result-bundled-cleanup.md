<!-- description: Bundled option/result are signature-override layer; pick a path to consolidate -->
# Option/Result Bundled `.almd` — Not Cosmetic After All

## Background

`stdlib/option.almd` and `stdlib/result.almd` declare the same fn surface
as `stdlib/defs/option.toml` / `stdlib/defs/result.toml`. After the
`bundled-almide-dispatch` change (2026-04-16, see `done/`) it looked
like the bundled `.almd` sources were dead code: they parsed and
lowered, but `src/main.rs`'s TOML duplicate prune dropped any IR fn
overlapping the runtime, so codegen-side they never produced output.

**Initial conclusion was wrong.** Trying to delete them broke
`spec/stdlib/coverage_misc_test.almd`:

```
error[E005]: argument 'f' expects fn(Unit) -> Option[Int]
            but got fn() -> Option[Int]
  in call to option.or_else()
   |
55 |   let b = option.or_else(a, () => some(42))
```

The TOML signatures use `Fn[Unit] -> X` (a 1-arg fn taking `Unit`), but
all real-world callers — including the test suite — write `() => x`
(a 0-arg fn returning `X`). The bundled `.almd` declares
`fn or_else[A](o: Option[A], f: fn() -> Option[A])` and **the bundled
signature wins type-checking**. Without the bundled override, the TOML
signature surfaces and breaks every existing caller.

So the bundled `.almd` files are silently doing important work: they
are the source of truth for the *signature* (`fn() -> X`), while the
TOML provides the *runtime dispatch* (`almide_rt_*`). The two halves
are accidentally co-dependent.

## Two viable resolutions

### A. Make TOML signatures match Almide convention (`fn() -> X`)

- Change every `Fn[Unit] -> X` in `stdlib/defs/*.toml` to whatever
  syntax represents 0-arg fn (likely `Fn[] -> X`; needs parser
  confirmation).
- Delete the bundled `option.almd` / `result.almd`.
- Bundled-Almide stays useful for genuinely new fns
  (`stdlib/list.almd::split_at`, `iterate`).

Pros: one source of truth per fn. No more "bundled silently overrides
TOML signature" gotcha. Removes ~70 lines of duplicate fn definitions.

Cons: cross-cutting TOML signature change. Must verify the codegen
template `Fn[]` rendering still produces valid Rust closures (current
templates assume the `(unit)` arg). Plus regenerate `arg_transforms`
and `stdlib_sigs`.

### B. Make the bundled `.almd` authoritative, drop TOML

- Delete `stdlib/defs/option.toml`, `stdlib/defs/result.toml`.
- Reimplement runtime dispatch in Almide (or keep `runtime/rs/option.rs`
  thin and route via bundled IR fns).
- Bundled `.almd` becomes the single source.

Pros: same one-source-of-truth benefit. Demonstrates bundled-Almide
on something real.

Cons: WASM emit (`emit_wasm/calls_option.rs`) has its own dispatch path
that consumes TOML-derived registration; rewriting that for bundled
fns is non-trivial. The runtime has fns the `.almd` doesn't
(`unwrap_or_else_throw`, `collect`, `partition`) that would need to
be ported. Higher risk surface than A.

### C. Document the split, leave the duplication

- Rename `stdlib/option.almd` → `stdlib/option.signatures.almd`.
- Add a header comment: "This file overrides the TOML signature.
  Runtime dispatch is `almide_rt_option_*`. Add new fns here only if
  there is a matching TOML/Rust runtime entry."
- Add a docs-gen check: every fn in `stdlib/{option,result}.almd`
  MUST have a matching TOML entry.

Pros: lowest-effort. Honest about what's happening.

Cons: leaves the duplication. Future contributor will still trip on
"why are these defined twice."

## Recommendation

**A.** Resolves the gotcha at the root, leaves the bundled-dispatch
infrastructure free to be used for genuinely new fns. Estimated 2-3h
including TOML migration + codegen template adjust + verifying WASM
target.

B is a longer-term north star (full bundled-Almide stdlib) but
shouldn't be tackled before the bundled path has more real fns living
on it.

C is a workaround if A turns out to have hidden cost.

## Decision needed

User confirms which path to take before starting. A is the
least-likely-to-regress path that still ends with a clean source-of-
truth.
