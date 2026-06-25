# StringInterp is NOT special syntax — desugar it to `concat + to_string(part)`

The ideal end-state (set 2026-06-17). Supersedes the per-position interp-lowering approach.

## The principle

`"a=${x}, b=${y}"` IS, semantically, nothing but `concat("a=", to_string(x), ", b=", to_string(y))`.
So the right architecture is: **the frontend desugars `StringInterp` into a `ConcatStr` chain whose
each Expr part is wrapped in its type's `to_string`** — and MIR/lowering NEVER sees a `StringInterp`
node at all. Then interpolation rides the ALREADY self-hosted + byte-verified `__str_concat` and the
per-type `to_string` primitives. The `interp_str_lowerable` predicate and the "385/460 excluded" set
**cease to exist** — they only existed because interp was treated as a special lowering construct.

This is the trust-spine philosophy itself: **reduce special syntax to proven primitives.** The leaf
slice (commit 83a72efa) added a predicate + per-position handling — the WRONG direction (more special
casing). The right direction removes the specialness.

## STATUS 2026-06-17 — step 2 SHIPPED v1-ONLY (v0 oracle UNTOUCHED), independently verified

The desugar landed (commit f81a93a0, develop-v1) in the **v1 lowering**
(`crates/almide-mir/src/lower/mod.rs` `desugar_string_interp`), NOT the shared frontend. This
deliberately defers the frontend desugar (layer 1) to keep v0 risk at zero. Evidence: the step-2
diff touches only `almide-mir` (+ a new `stdlib/bool.almd`); `git diff crates/almide-frontend` is
EMPTY; v0's `emit_string_interp` (in `crates/almide-codegen/src/emit_wasm/calls_string.rs`) is
unchanged. What was retired is the v1-internal `interp_str_lowerable` predicate — NOT the v0 oracle.
The dual-oracle is fully intact and IS the gate: v1's desugar output is byte-compared against v0's
output (corpus-wall (a)=250 interps byte-match v0; + independent goldens: Int/String/Bool/edge
byte-match on wasmtime, Float/compound cleanly WALL). The +22 caps gain was adversarially verified
SOUND (a Stdout-reaching interp operand surfaces Stdout transitively → witness `|0` declared∅ →
REJECT, never false-green). Bars: caps 4134 (ACCEPT), mir>ir 0, FORBIDDEN 0, 428 tests, 3-property
ACCEPT.

### THE ORACLE-PRESERVATION LAW (non-negotiable) — binds the EVENTUAL frontend desugar (layer 1)
Moving the desugar into the SHARED frontend is the only step that would touch v0's interp codegen.
It is DEFERRED, and when attempted it is bound by: **`emit_string_interp` MUST NOT be deleted.** Run
BOTH paths, byte-compare across the WHOLE corpus in CI, and retire `emit_string_interp` ONLY after
parity is PROVEN (translation-validation, the #570 pattern, applied to the desugar itself). "Delete
first, bet on the desugar" is FORBIDDEN — never delete the oracle before its formal replacement.
Until then, the v1-only desugar delivers the trust-spine benefit with ZERO v0 risk.

### BUDGET: two independent hard sub-problems (don't underestimate — clean framework ≠ done)
The desugar framework is clean, but byte-matching v0 has two independent known-hard formatting walls.
Until each lands, its interp part **cleanly WALLs** (verified: Float `${f}` → Unsupported naming
`float.to_string`) — never a silent miscompile.
- **`float.to_string` = dtoa (task #63)** — must byte-match v0's `f64` Display to 1 byte: correct
  shortest round-trip rounding, integer-valued `.0` suffix, NaN / Inf / -0.0, and the
  scientific-notation threshold. (history: float native↔wasm display drift.) The salvaged dtoa WIP
  reproducing v0's formatter EXACTLY is the body of step 4, not a footnote.
- **compound repr (`${list}` / `${record}` / `${tuple}`) = task #64** — must reproduce v0's observed
  format exactly: brackets, separators, string-element quoting, nesting, and the empty case. The
  self-hosted `emit_repr_value` must mirror v0's observed output, not a plausible-looking one.

## What it reveals: the real work is TOTAL `to_string` (Display)

Once interp = `to_string(part)`, the only remaining work is: **every type has a `to_string`**.
The 385/460 "broken" corpus interps are broken ONLY because the type's `to_string` is missing:

| part | what it needs |
|---|---|
| `${n}` Int | `int.to_string` ✓ (done) |
| `${s}` String | identity ✓ |
| `${list.len(xs)}` | materialize the call → wrap the Int in `int.to_string` |
| `${b}` Bool | `bool.to_string` (an `if` → "true"/"false") — EASY, missing |
| `${f}` Float | `float.to_string` = **dtoa** — HARD, missing |
| `${xs}` `${rec}` | `List.to_string` / record Debug-repr (`emit_repr_value` self-host) — missing |

**So "StringInterp" is really the "total `to_string` (Display)" problem.** When `to_string` is total,
interp works for free, everywhere.

## This redraws "tail vs essential"

- **`to_string` family — Int / Bool / Float(=dtoa) / List / record Display — is ESSENTIAL.** Interp
  AND `print`/`println` both go through it; it is reused everywhere.
- **Transcendentals (sin/cos/tan/log_gamma) are the TRUE tail** — interp and print never reach them.

**Consequence: `float_dtoa` was NOT tail drift.** It is the *prerequisite for `float.to_string`* =
essential. It only looked like a motiveless tail because the "interp = `to_string`" framing wasn't in
place. With the framing, **dtoa is a priority essential** (salvaged at `/Users/o6lvl4/cctmp/float_dtoa.almd.salvage`,
revive it). trig, by contrast, genuinely was tail (correctly low priority — but it was done, and
that's fine).

## Verification becomes COMPOSITIONAL

- Now: byte-match interp PER POSITION (bind/call-arg/tail/match-arm/concat) = combinatorial.
- Ideal: byte-match each TYPE's `to_string` ONCE → every interp using that type is auto-verified.
  Prove `int.to_string` once → `"${n}"`, `"x=${n}"`, `"${a+b}"` are ALL verified, compositionally.

## "Wall vs hole" dissolves structurally

With the desugar: a part whose type has **no `to_string`** simply cannot be desugared → an **explicit
frontend error (a clean WALL)**. A part that can → rides a proven primitive. **Silent miscompile (and
the current invalid-wasm-the-corpus-wall-misses) is STRUCTURALLY IMPOSSIBLE** — there is no "deferred
Opaque interp" path left to emit broken wasm. (The diagnostic confirmed today's non-leaf interp emits
INVALID WASM, loud, which the corpus-wall does not catch — the desugar removes that path entirely.)

## Implementation layers (the care points)

1. **Frontend desugar** `StringInterp{parts}` → `ConcatStr` fold with `to_string(part)` per type. The
   frontend is SHARED with v0 — so this changes v0's interp codegen path too (from `emit_string_interp`
   to concat+to_string). MUST be **dual-oracle verified**: the desugared form byte-matches v0's CURRENT
   `emit_string_interp` output for every part type (incl. compound repr, Float display edge cases).
   This is the gate — a v0 regression here is unacceptable.
2. **count==lower for free**: because the desugar is in the frontend, BOTH `count_ir_calls` (harness)
   and `lower_function` see the desugared `ConcatStr`+`to_string` IR — no special predicate, no
   count-vs-lower divergence (the whole [[../../../.claude .../project_v1_gate_count_vs_lower]] trap evaporates).
3. **v1 interp coverage == v1 `to_string` coverage.** A part whose `to_string` is self-hosted in v1 →
   works. A part whose `to_string` is NOT yet self-hosted → the v1 lowering of that `to_string` call
   must be a **clean Unsupported wall**, not an unlinked-call → invalid wasm. So a prerequisite is:
   **v1 lowering of an unregistered/unlinked stdlib call → `Unsupported` (wall), never invalid wasm.**
   (Or: the frontend desugar itself rejects when no `to_string` is available for the type — but that
   would also reject it for v0, a v0 regression unless v0 has the `to_string`. Since v0 HAS Display for
   every type, the desugar should always succeed for v0; the v1 wall is the unlinked-call → Unsupported.)
4. Then **fill in `to_string` per type** (each automatically enables its interps, compositionally):
   Bool (easy) → revive Float=dtoa (the salvaged WIP, essential) → compound repr (List/record Display).

## Build order
1. v1: unlinked stdlib call → `Unsupported` (structural wall prerequisite). 2. Frontend desugar
StringInterp → concat+to_string, dual-oracle byte-match v0 (retire `interp_str_lowerable` + the
per-position handling). 3. `bool.to_string`. 4. revive `float.to_string` (dtoa). 5. compound Display.
Each step's detector is the per-TYPE `to_string` byte-match — not per-position interp.
