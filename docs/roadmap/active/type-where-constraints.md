<!-- description: where clauses on type/fn definitions for type constraints -->
# Type Where Constraints

> **Target: v0.24+**
> **Status: Design**

## Problem

Almide has no way to express type constraints on generic parameters. `Set[T]` should require `T: Hash + Eq`, but currently any type can be used.

## Solution

`where` clauses on type and function definitions:

```almide
type Set[T]
  where T: Hash + Eq
= { items: Map[T, Unit] }

fn max[T](items: List[T]) -> Option[T]
  where T: Ord
= list.fold(items, none, (best, x) => ...)
```

## Relationship to Test Where

Same keyword, same philosophy: "preconditions for the body to be valid."

- Type `where`: type-level constraints (compile-time)
- Test `where`: value/effect-level context (test-time)

Both defined in separate roadmaps to allow independent implementation.
