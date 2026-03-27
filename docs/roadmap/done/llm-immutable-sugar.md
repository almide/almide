<!-- description: Language-level sugar for immutable collection mutations -->
<!-- done: 2026-03-18 -->
# LLM Immutable Sugar

Language-level sugar for immutable collection mutations. Split from llm-immutable-patterns.md Tier 3.

## 3.1 Mutable local collections (`var xs = [1,2,3]`)
Allow `var` lists to use `xs[i] = v` syntax that compiles to `xs = list.set(xs, i, v)` under the hood.
```
var arr = [3, 1, 2]
arr[0] = 99          // desugars to: arr = list.set(arr, 0, 99)
```
LLMs write this naturally. Semantics stay immutable (copy-on-write), syntax is familiar.

**Trade-off**: Looks mutable, is actually immutable. May confuse developers expecting O(1) mutation.
**Effort**: High. Needs parser + codegen changes for IndexAssign on var-bound lists.

## 3.2 `with` expression for bulk updates
```
let arr2 = arr with {
  [0] = 99
  [2] = 42
}
```
Batches multiple updates into a single new list creation.

**Effort**: High. New syntax + optimization pass.

## Why on hold
- Tier 1 (error messages) and Tier 2 (stdlib patterns) cover the most impactful cases
- These sugar features need spec stabilization and careful design
- Benchmark data needed to justify the complexity
