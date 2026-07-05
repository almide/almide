# make-verify — the trust demo

**The claim is not "AI writes correct code." The claim is: when AI's code is _wrong_
— and across the model zoo it often is — Almide makes the failure _clear_ and
_recoverable_; and when it's _right_, Almide _proves_ it is safe.**

Mainstream languages give you neither: a wrong modification compiles, runs, exits 0,
and ships the bug to production.

## Why this framing (not "modification survival rate")

We measured modification-survival directly (LLM modifies working code; does it still
pass?). With a strong model (Claude-class) on moderate tasks it's ~100% in **every**
language — a ceiling effect: a capable model simply doesn't make the mistakes. That is
the wrong thing to measure.

The real differentiator is **what happens when there _is_ a mistake** — and:

- **the model zoo is not just Claude.** Agent fleets run cheaper / smaller / open /
  older models that err far more often, on far larger changes.
- so the honest, model-agnostic measurement is to **inject the mistake** and ask the
  _language_, not the model: *is the failure visible and fixable, or silent?*

That response is a property of the **language + compiler + proof spine** — it does not
evaporate as models get stronger.

## The measured number (unbiased)

We injected **8 realistic AI modification mistakes** (each authored by a separate
agent, both in Almide and in Python) and asked the *language*, at author/CI time:
caught-and-recoverable, or silent?

- **Almide caught 6/8 at compile** with an actionable diagnostic. The 2 it didn't:
  one was a pure-logic off-by-one (a control — no type system catches it), and one
  exposed a real Almide diagnostic gap we're tracking (a stale field name reported as
  an internal error rather than "unknown field").
- **Python's compile gate (`py_compile`) caught 0/8.** Every mistake passed lint/compile
  and shipped — surfacing later as a silently wrong value or a runtime crash.

`./run.sh` reproduces three of them live with real compiler output (below is the first).

## The scenarios

`./run.sh` walks three: (1) a non-exhaustive match (Python ships a silently wrong
**value**), (2) a missed call site after a signature change, and (3) an Option/None
result used without handling the absent case (Python passes its compile gate, then
crashes at runtime). In all three, Almide refuses to build the wrong program with an
actionable diagnostic. The detailed walkthrough of #1:

An AI is asked to "add a `Triangle` variant to `Shape`." It updates the **type** but
forgets to handle it in `area()` — the single most common modification mistake.

Run it yourself:

```
./run.sh
```

### 1. Correct program — fully type-checked, compiles, runs

```
$ almide run shape.almd
12
12
```

It is exhaustively type-checked (note step 2 — a missing case is a compile error, not a
runtime surprise). The deeper **proof** layer is described below.

### 2. The wrong modification in **Almide** — caught at compile, with the fix

```
$ almide run shape_buggy.almd
error[E010]: non-exhaustive match: missing Triangle(_, _)
  --> shape_buggy.almd:5:34
  in match
  here: fn area(s: Shape) -> Int = match s {
  hint: add arms for Triangle(_, _):
  Triangle(arg1, arg2) => _
Or use `_ => todo()` to compile incrementally.
  |
5 | fn area(s: Shape) -> Int = match s {
  |                                  ^
compile failed
```

The failure is **clear** (the exact missing case), **located** (the caret), and
**recoverable** (it hands you the arm to add — Elm-grade). A model reading this fixes
it in one step; so does a human.

### 3. The same wrong modification in **Python** — ships silently

```
$ python3 shape_buggy.py
None
(exit 0)
```

It compiles, runs, exits 0, and returns `None` for a triangle's area. No error. The bug
reaches production and is found — if ever — by a downstream test or an end user.

## The two trust layers this shows

1. **Failure is clear + recoverable** (this demo): a wrong change is caught at compile
   with an actionable diagnostic. Model-agnostic — it pays off for every model that errs.
2. **Correctness is proven** (the trust spine — separate layer): Almide's `proofs/`
   pipeline runs a **Coq-proven checker** that, in the build/CI (not in `almide run`),
   re-verifies ownership / name-resolution / capability bounds over the compiled corpus
   and emits a certificate per verified function. Almide is the only producer aiming to
   ship a *proof*, not just an artifact. This layer is real and gated in CI but
   **in-progress** (byte-level binding is the last mile) — see `proofs/` and `docs/`.

That is the trust layer Aid-On sells: machine-written software you can both **fix fast
when it's wrong** and **prove safe when it's right** — independent of which model wrote it.

## Honest scope

This demo isolates the *failure-clarity* axis (the cleanest, immediately-reproducible
one). The execution layer (Almide → wasm byte-matches the native oracle) and the formal
proof bundle are separate, in-progress layers; see `docs/` and `proofs/`. The point here
is the trust _experience_, shown with real compiler output, not a mockup.
