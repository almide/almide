# Stability — the freeze declaration and the breaking-change policy

> Declared 2026-08-20 (owner decision; sequenced after ADR-0012 D2 landed —
> `-> T!E` was the last surface change gated ahead of this declaration).
> This page is normative. Tracking: A0-1 in the mission-critical attack list
> (#1514), carried by #530 / #1485 / #1483.

## 1. The frozen surface

**The LLM-facing surface is the stable surface.** Anything shown in
[`docs/CHEATSHEET.md`](CHEATSHEET.md) or [`llms.txt`](../llms.txt) is a
promise: programs written against it keep meaning what they mean. Anything
not intended as a promise does not appear there (#1483's rule — our primary
reader is a model that cannot notice a doc/reality gap, so the training
surface and the tested surface must be the same surface).

Three consequences, each with a gate either standing or tracked:

1. **Everything on the surface has executable coverage** — a `spec/` test,
   and a `spec/wasm_cross/` fixture under a contract for anything with
   cross-target meaning. (Gate: the contract ledger + the cheatsheet's
   compile-gated examples; the full bidirectional sweep is #1483's open
   half.)
2. **Nothing experimental appears on the surface.** Experimental work lives
   in issues, ADRs, and gated code paths until it is promised.
3. **The stdlib surface matches the compiler** — `tools/gen-stdlib-doc-index.py
   --check` already gates `docs/stdlib/` against the compiler's own module
   interfaces.

## 2. The conformance clause (#530)

The **Almide Language Specification** (`docs/SPEC.md` + `docs/specs/`,
including the ALS sections cited by the contract ledger) is **normative**:

> Where an implementation disagrees with the ALS, the implementation is
> wrong.

The interpreter (`crates/almide-interp`) is the ALS's **executable
companion**: the third judge of the 3-way oracle, held to the spec, never a
spec itself. Where prose, interp, and the compiled targets disagree, the
disagreement is a bug against the ALS — filed and fixed on the implementation
side, with the fixture citing the ALS section it pins. (The known prose
contradictions are #530's open burn-down; each one closed converts a page of
prose into a cited, fixture-pinned rule.)

## 3. The breaking-change policy

A **breaking change** is any change under which a program that is valid
against the frozen surface changes meaning or stops compiling.

- A breaking change requires ALL of:
  1. an **explicit major** framing in the release that ships it (never a
     minor/patch),
  2. a **`@dialect` epoch bump** with its row in
     `proofs/dialect-epochs.toml` (the writer-observable ledger — a model can
     tell a stale dialect from a wrong program),
  3. a **migration note** in the release notes, and where mechanical, an
     `almide fix` rule.
- **Additive changes stay free**: new syntax, new stdlib names, and new
  capabilities that leave every existing program's meaning intact ship in
  minors, exactly as before (`-> T!E` is the model case: `T!` kept meaning
  `T!String`, every existing spelling unaffected).
- **Two spellings, one meaning** is allowed on the frozen surface only when
  the equivalence is total (the `Option[T]`/`T?` pair). Where an equivalence
  is intended but not yet established compiler-wide, BOTH spellings stay
  legal and fmt does not rewrite across the line (the `Result[T, E]`/`T!E`
  pair — see ADR-0012's 2026-08-20 amendment for the three falsifications
  that set this rule).

## 4. What "stable" will mean, measurably (#1485)

The freeze declares the *surface* still. The **stability claim** — "the
defect curve has bent" — is a separate, measurable statement defined in
[`proofs/stability-closure.toml`](../proofs/stability-closure.toml): six
criteria (fuzz nights, conformance weeks, wasm frontier, wall corpus,
blocker count, dialect stillness) that must hold simultaneously for one
30-day window, with every N fixed in that file *before* the numbers arrive.
`scripts/check-stability-closure.sh` reports the current standing on every
push, blocking nothing. Changing any N requires a written reason in the same
commit.

## 5. What this is not

Not a 1.0 declaration, not a promise to stop changing Almide, and not a
claim that the implementations are done: the wall burn-down (#1527), the
negative-test ladder (#1528), and the marker/explicit unification recorded
in ADR-0012 all continue — under the policy above instead of ahead of it.
