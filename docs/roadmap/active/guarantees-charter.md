<!-- description: Almide Guarantees Charter — eliminate categories of human/AI worry by mechanical, inferred, LLM-fixable guarantees ("proof without puzzles") -->
# Almide Guarantees Charter

The north star that binds Almide's correctness work. Almide's mission is the
language **LLMs write most accurately** (modification survival rate). The lever
this charter commits to:

> **Eliminate whole categories of concern by construction, each backed by a
> mechanical/mathematical guarantee — but with *zero annotation burden* and
> *local, one-edit-fixable* diagnostics. Proof without puzzles.**

## The principle: proof without puzzles

Rust's borrow checker mechanically erased use-after-free and data races — a
triumph. But it added a *new* category of worry: lifetime puzzles (fighting the
checker). For an LLM, that trade is bad: satisfying a global, non-local checker
tanks modification survival.

Almide takes the rung above: **borrow-checker-grade guarantees, but inferred and
LLM-fixable.** The value to an AI is twofold:

1. **The bad thing can't be expressed** → no reasoning required (you can't get
   UAF wrong if UAF is unexpressable).
2. **When you do err, the check rejects with a local, actionable, one-edit
   diagnostic** → the LLM fixes it in a single shot (high survival).

[Perceus](../../specs/perceus.md) is the existing proof of this principle:
memory safety, mechanically proven (Lean, 23 theorems), fully inferred, zero
annotations. **Perceus is the template; every other guarantee should earn the
same shape.**

## The gate: four conditions every guarantee must meet

A guarantee is only admitted to Almide if it is **all four**. Miss one and it
becomes anti-LLM (Rust's lifetime fatigue in a new costume).

1. **Mechanical / mathematical** — a decidable static check or a by-construction
   impossibility, not "lint" or convention.
2. **Inferred — zero annotation burden** — the safe path is the default and
   needs no ceremony (Perceus needs no annotations).
3. **Local, fixable diagnostics** — a violation points at one place with an
   actionable message; never a whole-program puzzle.
4. **Low false-positive** — does not reject legitimate programs.

## Worry categories Almide eliminates

| Concern an AI/human would otherwise track | Mechanism | Status |
|---|---|---|
| Memory: use-after-free / double-free / leak | **Perceus** RC (Lean, 23 theorems) | ✅ proven · inferred · template |
| Null dereference | no null + `Option` + exhaustive `match` | ✅ by construction |
| Swallowed errors | `Result` + `effect fn` + `?` + exhaustiveness | ✅ |
| Aliasing / unexpected mutation | value semantics + no mutable cycles ([ADR](adr-no-wasm-gc.md)) | ✅ by construction |
| "What does this function touch?" (IO / state / globals) | **effect + capability system** | 🚧 flagship next |
| Out-of-range / divide-by-zero / broken invariants | **refinement via `where`** ([type-where](type-where-constraints.md)) | 🚧 in progress |
| Data races | value semantics + no shared mutation → race-free by construction | ⚠ holds; not yet stated as a proof |
| Resource leaks (file / socket / handle) | scoped capabilities / linear use | ⚪ future |
| API state misuse (use-after-close) | typestate | ⚪ future |

Almide already holds roughly half a "borrow-checker-grade guarantee suite." The
effect/capability and `where`-refinement axes are the two wheels that complete
it: once both land, **a function's type mechanically tells you what it touches,
what it assumes, and what it returns** — exactly what an AI most needs.

## Non-goals (deliberately rejected)

- **Totality / termination checking** (Idris/Agda-style). Powerful, but proving
  termination violates conditions (2) and (3): it forces annotations/puzzles an
  LLM can't fix locally. Almide admits only **decidable, inferable, locally
  fixable** guarantees.
- **WASM-GC / host-GC memory** — see [ADR](adr-no-wasm-gc.md). Determinism +
  the Perceus proof are part of the guarantee story, not to be ceded.
- Anything that buys a guarantee by adding a global annotation burden.

## How this binds the roadmap

Every correctness item is an *instance* of this charter — pursued because it
mechanically erases a worry category under the four conditions:

- [Perceus belt](almide-perceus-belt.md) — memory (the proof exemplar).
- [Effect/capability system](effect-system-capability.md) — effects (**flagship
  next**: highest AI leverage — makes "is this pure / does it do IO?" a typed,
  inferred fact).
- [Type `where` constraints](type-where-constraints.md) — refinements.
- [Region inference](region-inference.md) — optimization, not a correctness
  necessity (no cycles ⇒ Perceus alone doesn't leak; per the ADR).
- [No WASM-GC ADR](adr-no-wasm-gc.md) — preserves the determinism + proof that
  back the memory guarantee.
- [Correctness guarantee gaps](correctness-guarantee-gaps.md) — the
  *compiler-internal* proof chain (source → correct binary), the dual of this
  *language-level* charter.

## Flagship next: effect + capability

Rationale: the single highest-leverage worry to erase for an AI is **"what does
this function touch?"** A typed, inferred effect/capability row turns IO, global
state, and mutation into facts visible at the call site — so an LLM never has to
*guess* purity, and a mechanical check rejects an effect that escapes its
capability with a local diagnostic. Pursued under the four conditions:
inferred effect rows (no hand-written effect annotations on every function),
local rejection, no false positives on pure code.

## Adding a new guarantee

Before building one, check it against the gate (mechanical · inferred · local-
fixable · low-false-positive) and name the worry category it erases. If it can't
pass all four, it doesn't belong in Almide — find a by-construction reshaping
instead (as the ADR did for cycles: don't check cycles, make them unexpressable
and provide `Arena`/`Handle`).
