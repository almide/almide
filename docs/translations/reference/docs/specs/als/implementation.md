<a id="als--実装不変量implementation-invariants"></a>

# ALS — Implementation Invariants

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](implementation.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](implementation.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](implementation.zh-CN.md)

> Non-normative reader snapshot of the [pinned original](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/specs/als/implementation.md). Normative status and requirements below belong to the original, not this copy. No automatic synchronization; historical implementation, conformance and safety claims have not been independently revalidated for this translation. Code and rule identifiers are preserved.

> Last updated: 2026-10-27

Implementation requirements that do not appear directly in observations (stdout/stderr/exit), but underpin memory safety, resources and determinism. Referenced by the `spec` key in the contract ledger.

<a id="als-i1-参照カウント規律perceus"></a>

## ALS-I1 Reference-counting discipline (Perceus)

RC for heap values follows the Perceus discipline: pass-through combinators return their own reference (+1); sharing in interpolation, spread and Value construction is balanced by +1; and the WASM heap is reclaimed by default (zero leaks at program termination is the requirement — the exception is the per-iteration leak of a TCO self-call loop recorded by C-066 itself, documented in its statement as known remaining debt until Stage C).
Verification is provided by Lean proofs, emit-time Σ-probes and RC-counter fixtures.
Contracts: C-041, C-066, C-071, C-086, C-121, C-122, C-146, C-149.

<a id="als-i2-コンパイラの決定性と資源"></a>

## ALS-I2 Compiler determinism and resources

Code generation is deterministic independently of the host architecture (same input → byte-identical output). Compilation does not overflow the native stack on wide or deeply nested input (a dedicated 256 MiB compilation thread + iterative passes over statement chains — C-059; deeply nested expressions are cleanly rejected by the parser's MAX_DEPTH=500 and never enter compilation in the first place).
Contracts: C-040, C-059.

<a id="als-i3-v1-lowering-エッジの等価証拠"></a>

## ALS-I3 Equivalence evidence for v1 lowering edges

Lowering edges specific to the v1 (MIR) route — scalar-value edge cases; tuple/list Ok-payload round trips; the custom base64 implementation; filter_map with conditional keep/skip; owned-binding clusters of borrowed parameters; direct constructor binding of stdlib-call payloads; value combinators on heap-Ok Results; constructor if-payloads; and the non-silent-empty guarantee for scalar list literals — are byte-identical to v0 (a group of contracts fixing the results of the v1 verification campaign).
When linking two or more self-host modules, if module-local helpers beginning with `__` collide by name, **do not merge them by matching names**: stop at an explicit link error (merging rebinds callers to the wrong signature, causing Wasm validation to fail after an `Ok` response).
In test mode, each `test` block observes MUTABLE module top-lets starting from their declaration initializers (thread isolation on native; reinitialization for each test on wasm).
Contracts: C-075, C-107, C-109, C-116, C-120, C-138, C-139, C-143, C-144, C-152, C-212, C-321.

<a id="als-i4-コンパイルコストの予算"></a>

## ALS-I4 Compilation-cost budgets

The compiler handles untrusted input (PR CI, LSP immediately upon opening a file, graders and the playground). Every phase whose work grows with the input is constrained by a named **budget**. The inventory is the table emitted by `almide budgets` (embedded in `docs/specs/cli.md`), with rows for `nesting` (nesting depth of one expression), `type_size` (node count of one inferred type), `generic_instances` (number of specialized instances of a bounded generic function; each instance's type arguments are also constrained by `type_size`), `exhaustiveness_steps` (steps to check exhaustiveness of one `match`), `manifest_bytes` (size of one `almide.toml` / `almide.lock`), `dependency_count` and `dependency_depth`.

- Budgets are **counted**, not measured by time or available memory. The same input receives the same verdict on every machine.
- No attribute, manifest key, lock entry or environment variable changes a budget. Only the command-line `--budget NAME=N` can change it, and `almide check --profile critical` rejects this flag.
- Input exceeding a budget terminates with that budget's diagnostic and a nonzero exit code, rather than a crash, out-of-memory kill or hang. `type_size` produces E111 `type too large (N+ nodes, max M)` (at the expression or constraint where the type exceeds the budget); `generic_instances` produces E112; `exhaustiveness_steps` produces E113 `match too complex to check for exhaustiveness`; `nesting` produces the syntax error `expression nesting too deep (max N)`; and the three project-side budgets terminate with an error naming the file, number and budget. An over-budget type is never accepted by guessing, and a `match` whose check could not finish is never considered exhaustive.
- Types share structure. Even with `type_size` removed, a program nesting `let t(i) = (t(i-1), t(i-1))` n times (directly, or through `fn dup[T](x: T) -> (T, T)`) passes checking in time and memory linear in n.

Tests: `tests/diagnostics/e111-type-doubling-chain/`,
`tests/diagnostics/e111-type-doubling-through-a-generic/`,
`tests/diagnostics/e113-bool-matrix-too-complex/`, `tests/compile_budget_test.rs`.
Contracts: C-440.

> Source context: the dated requirements include the C-066 per-iteration leak exception through Stage C and strong compilation-resource claims. They are retained as source requirements and historical statements, not independently established guarantees for every compiler build or resource environment.
