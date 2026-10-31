# make-verify — the trust demo

**The claim is not "AI writes correct code." The claim is: when AI's code is _wrong_
— this demo exercises several such mistakes — Almide can make the failure
_clear_ and _recoverable_. Separate certificate checks establish specific
modeled properties within their documented coverage, not that all code which
compiles is correct or completely safe.**

The examples compare concrete compiler diagnostics with Python's `py_compile`
gate, not all tooling or safety techniques available in other languages.

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

## The reported eight-case experiment

The original demo experiment injected **8 AI-authored modification mistakes** (each authored by a separate
agent, both in Almide and in Python) and asked the *language*, at author/CI time:
caught-and-recoverable, or silent?

- **Almide caught 6/8 at compile** with an actionable diagnostic. The 2 it didn't:
  one was a pure-logic off-by-one (a control — no type system catches it), and one
  exposed a real Almide diagnostic gap we're tracking (a stale field name reported as
  an internal error rather than "unknown field").
- **Python's compile gate (`py_compile`) caught 0/8.** Those cases passed that syntax-compilation gate, then produced a wrong value
  or runtime error. This is not a measurement of all Python linters or tests.

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
2. **Modeled properties are checked** (the trust spine — separate layer):
   `almide verify` produces structural Wasm witnesses and delegates checks to
   the independent `almide-verify`. Supported core artifacts can be bound to
   the bundle by SHA-256; components are not supported by that route.
   Ownership balance, names, and capability bounds have coverage and trust
   assumptions. Byte binding is implemented (#4011), but it does not establish
   complete memory safety or functional correctness. See
   [the gap table](../../docs/design/MEMORY-SAFETY.md) and
   [proven vs trusted](../../docs/contracts/proven-vs-trusted.md).

The demonstrated distinction is actionable diagnostics versus silent or later
failures in these examples. Certificate evidence is a separate, bounded claim;
it does not establish that every wrong modification is detected.

## Honest scope

This demo isolates the *failure-clarity* axis (the cleanest, immediately-reproducible
one). Cross-target comparisons and the formal witness bundle are separate
evidence layers with declared scope; see `docs/` and `proofs/`. The point here
is the trust _experience_, shown with real compiler output, not a mockup.
