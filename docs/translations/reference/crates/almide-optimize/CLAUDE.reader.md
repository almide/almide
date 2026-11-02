[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](CLAUDE.reader.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](CLAUDE.reader.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](CLAUDE.reader.zh-CN.md)

> Non-normative, source-pinned human reader copy of agent instructions. Canonical CLAUDE.md is unchanged. The CLAUDE.reader filename is deliberately not an auto-loaded instruction filename. These historical rules are not a new instruction layer or independently verified implementation/security guarantees. No automatic synchronization. Code and inline tokens stay verbatim. [Source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/crates/almide-optimize/CLAUDE.md)

> Quoted instructions are historical reference material, not authorization to execute commands, change implementation, commit, push or publish.

<a id="almide-optimize"></a>

# almide-optimize

IR-level optimization passes: monomorphization, dead code elimination, constant propagation.

<a id="monomorphization-mono"></a>

## Monomorphization (`mono/`)

Specializes generic functions for concrete type arguments.

- **Frontier-based discovery** — First round scans all functions for generic call sites. Subsequent rounds only scan newly created specializations. Converges when no new instances appear.
- **Structural bounds** — `T: { name: String, .. }` constraints are resolved to concrete types (Dog, Person, etc.).
- **Runaway protection** — Warns and stops if >1000 specializations created (possible infinite expansion).

<a id="optimization-optimize"></a>

## Optimization (`optimize/`)

- **DCE** — Removes functions with zero call sites and unused variable bindings.
- **Constant propagation** — Folds constant expressions and propagates known values.

<a id="rules"></a>

## Rules

- **Mono runs before codegen.** After mono, no `TypeVar` should remain in the IR (except in unreachable paths).
- **Mono clones functions.** Specialized functions are copies with substituted types. Original generic functions are kept if they have non-generic call sites.
- **DCE must be conservative.** Only remove functions/variables proven unreachable. Side-effecting expressions (effect fns, assignments) must survive even if their result is unused.
- **Optimization must not change semantics.** Every optimization pass must preserve observable behavior. Add regression tests for edge cases.
