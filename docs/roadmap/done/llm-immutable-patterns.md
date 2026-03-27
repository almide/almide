<!-- description: Mitigations for LLM failures with immutable data patterns -->
# LLM and Immutable Data Structures

LLMs trained on Python/JS/Go default to mutable algorithms. Almide's immutable lists cause systematic failures when LLMs port mutable patterns directly.

## Done

| Mitigation | Version | Effect |
|------------|---------|--------|
| `cannot reassign immutable binding` error (param vs let hints) | v0.4.6 | Immediate feedback on parameter reassignment |
| `list.set` / `list.swap` returns new list | v0.4.5 | Semantics correct, UFCS-callable |
| Lost mutation warning (discarded `list.set/swap` return) | v0.4.6 | Warns when immutable update result is unused |
| Tuple return suggestion (Tier 1.1) | v0.4.7 | Warns when fn modifies list param but doesn't return it |
| Optional `else` / braceless let-chain | v0.4.5-6 | Less syntax friction for functional style |

## Remaining — Tier 1 (error messages)

### 1.2 Suggest `var` with shadowing pattern ✅ Already done
Parameter reassignment error already suggests `var {name}_ = {name}` pattern (v0.4.6).

### 1.3 Rich source location in errors ✅
Column numbers + caret underline implemented (v0.4.11). All errors now show `file:line:col` with `^^^` underlines.

## Remaining — Tier 2 (stdlib patterns)

Overlaps with [list-stdlib-gaps.md](./list-stdlib-gaps.md):
- `list.update(xs, i, f)` — functional update at index
- `list.slice` / `list.range` / `list.insert` / `list.remove_at`

These are tracked in list-stdlib-gaps.md.

## Success metric

An LLM should be able to write a working quicksort in Almide within 2 attempts:
1. First attempt: mutable pattern → clear error with actionable fix
2. Second attempt: functional pattern using tuple return + `list.swap`
