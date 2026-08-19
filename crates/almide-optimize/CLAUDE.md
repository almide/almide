# almide-optimize

IR-level optimization passes: monomorphization, dead code elimination, constant propagation.

## Monomorphization (`mono/`)

Specializes generic functions for concrete type arguments.

- **Frontier-based discovery** — First round scans all functions for generic call sites. Subsequent rounds only scan newly created specializations. Converges when no new instances appear.
- **Structural bounds** — `T: { name: String, .. }` constraints are resolved to concrete types (Dog, Person, etc.).
- **Runaway protection** — Warns and stops if >1000 specializations created (possible infinite expansion).

## Optimization (`optimize/`)

- **DCE** — Removes functions with zero call sites and unused variable bindings.
- **Constant propagation** — Folds constant expressions and propagates known values.

## Rules

- **Mono runs before codegen.** After mono, no `TypeVar` should remain in the IR (except in unreachable paths).
- **Mono clones functions.** Specialized functions are copies with substituted types. Original generic functions are kept if they have non-generic call sites.
- **DCE must be conservative.** Only remove functions/variables proven unreachable. Side-effecting expressions (effect fns, assignments) must survive even if their result is unused.
- **Optimization must not change semantics.** Every optimization pass must preserve observable behavior. Add regression tests for edge cases.
