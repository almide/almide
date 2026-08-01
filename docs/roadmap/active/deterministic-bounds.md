<!-- description: A deterministic replacement for timeouts — fuel, quasi-determinism, oracle-relative contracts -->
# Deterministic Bounds — the scientific path to a timeout that does not break byte-identity

The effect-surface carve-out in [zero-committed-shell.md](./zero-committed-shell.md) — *a
timeout is admissible where the bounded operation is already nondeterministic* — is a
**boundary, not a solution**. It says where the problem may be ignored. It does not say how to
bound a computation and keep the answer a function of the program.

This document looks for the second thing. The literature has been circling it for forty years
under three different names, and the pieces that matter are all deployed somewhere in 2026.

## The problem, stated so it can be attacked

Byte-identity is a **2-safety property** (a hyperproperty over pairs of runs):

> for all programs `p` and inputs `i`:  `observe(native(p, i)) = observe(wasm(p, i))`

A wall-clock timeout adds an input that is not `i` and not the same on both sides — the host's
speed. `fan.timeout` was removed in 0.29.0 because of exactly this, and the removal was right.

But notice what the failure actually is. It is **not** that bounding computation is
intrinsically nondeterministic. It is that *wall-clock* is the wrong quantity to bound. Replace
the quantity with one the program itself determines, and the 2-safety property survives.

## Path A — deterministic fuel: bound STEPS, not seconds

**The idea, and where it is already running.** Give the computation an abstract budget in
*steps*, decremented by the program's own execution, and the bound becomes a function of the
program. Same program, same input, same budget ⇒ same outcome, on any machine, at any speed.

This is not speculative. It is load-bearing in production systems today:

- **EVM gas** — the entire correctness argument of a blockchain rests on every node computing
  the same "out of gas" verdict. Determinism of the bound is the whole point.
- **Wasmtime fuel metering** (`Config::consume_fuel`) — the compiler injects a decrement per
  operation; a program can be interrupted deterministically at a fuel bound. Wasmtime's *other*
  interruption mechanism, **epochs**, is explicitly the nondeterministic-but-cheap alternative,
  and the pair of them is the industry admitting exactly the trade this document is about.
- **Step-indexing** in program logics (Iris, and CompCert-adjacent developments) — "runs for at
  most n steps" is the standard device for giving a semantics to possibly-diverging programs
  inside a total logic. Fuel is step-indexing made operational.
- **Fuel-based recursion in Coq/Agda** — the routine way to define a non-structurally-recursive
  function totally.

**What Almide has that most languages do not.** Fuel across two backends requires that both
backends agree on what a "step" is. For a language with independent frontends per target, that
agreement is a wish. Almide compiles **one MIR to both renderers** — the trust spine — so a
cost model attached to MIR operations is, by construction, the same cost model on both legs.
The thing that is normally the hard part is the thing this compiler already has.

And the shape is familiar here: the ownership certificate proves the emitted artifact is
RC-balanced. A **cost certificate** — this artifact decrements fuel exactly once per MIR op of
class C — is the same kind of object, checked by the same kind of gate.

**Costs, honestly.** Fuel metering is not free; wasm engines report on the order of tens of
percent overhead when it is on globally. The answer is that it should not be on globally: the
budget applies to a *bounded region*, and code outside it is uninstrumented. That makes
`bounded(f, fuel)` a construct with a visible cost, which is correct — it is buying something.

**The new invariant, and therefore the new gate.** "Both targets consume identical fuel on
identical input" is a cross-target promise that does not exist today. It wants a contract and a
`spec/wasm_cross` fixture that prints the consumed fuel, so the claim is executable rather than
architectural. That fixture is also the cheapest possible falsifier: if the two legs ever
disagree by one unit, the whole approach is wrong and you find out in CI rather than in a
theorem.

## Path B — quasi-determinism: the exact formal status of "or it ran out"

A bounded computation has two outcomes, and pretending otherwise is where designs go wrong. The
literature has a precise name for this shape.

**LVars and Freeze-After-Writing** (Kuper, Turon, Krishnaswami, Newton — POPL 2014) study
deterministic parallelism where results live in a lattice and observations are *monotonic
threshold reads*. Determinism survives arbitrary scheduling because a reader can only observe
that a value has *reached* a threshold, never that it has not.

The theorem that matters here is the negative one. **Observing absence is non-monotonic**, and
non-monotonic observation is exactly what destroys determinism. A wall-clock timeout is the
canonical non-monotonic observation: it asks "has this NOT finished yet?"

Their answer to needing that power anyway is **quasi-determinism**: a program that is either
deterministic *or* raises an error — never two different answers. Formally, all terminating
runs agree, or a run errors.

That is precisely the status a fuel bound should claim:

```
bounded(f, n)  =  ok(v)          where v is THE deterministic value of f, or
                  err(Exhausted) when f's deterministic cost exceeds n
```

and with a *deterministic* cost model, even the second branch is deterministic — the design
lands **stronger than quasi-deterministic**, at fully deterministic, with `Exhausted` as an
ordinary value. Quasi-determinism is the fallback position if the cost model turns out to be
only approximately portable; it is worth knowing the weaker guarantee has a name and a proof
technique, because it is what an honest retreat looks like.

This also settles a question the concurrency stance left open. #1000 excluded cancellation
because "how far a cancelled sibling got is a function of scheduling." Under fuel, *how far it
got* is a function of its budget. **Deterministic cancellation is reachable** — not as a
wall-clock kill, but as a budget that runs out at the same point on every machine. That is a
real change to what the `fan` family could offer, and it is downstream of this document rather
than of a scheduling decision.

## Path C — oracle-relative contracts: for the effects that really are external

Fuel cannot help with a subprocess or a socket: the nondeterminism is not in our program. The
carve-out is right for these — but it can be stated as a theorem-shaped object rather than an
exception, using **relational Hoare logic** (Benton) and the standard treatment of external
input in 2-safety proofs.

Parameterize the property by the oracle:

> for all programs `p`, inputs `i`, and **oracle answers `ω`**:
>   `observe(native(p, i, ω)) = observe(wasm(p, i, ω))`

The timeout's firing becomes part of `ω` — an *input*, not an event. Then the contract splits
cleanly, and each half is checkable:

- **Given the same `ω`, the two targets agree.** A fixture pins this by forcing `ω` (a 5-second
  sleep against a 100ms bound fires on any plausible host — a 50× margin makes the oracle's
  answer effectively constant).
- **`ω` itself is not determined by the program.** Stated, not hidden.

The gain over "we allow it here" is that the exception acquires a shape: every external effect
already in the language (`process`, `http`, `fs`, `env.os`) is an oracle parameter, and C-189's
existing platform-reporting carve-out is the same construction. One frame, not a growing list
of special cases.

## Path D — static cost analysis: the bound you may not need at runtime

If the cost is known *before* running, no runtime bound is required for that code at all.

**Automatic Amortized Resource Analysis** — RAML (Hoffmann, Aehlig, Hofmann) and its
descendants (Liquid Resource Types, λ-amor, TiML) — infers polynomial bounds on evaluation
steps by reducing amortized analysis to LP over type annotations. It works on real functional
programs, not just toy ones.

Why this fits Almide specifically: the language is already heavily first-order and
combinator-shaped, the stdlib is self-hosted in Almide, and the MIR is where a cost model would
live anyway. AARA on the MIR would give a **static fuel bound**, which turns `bounded(f, n)`
from a runtime guard into a compile-time obligation for the code it can analyze — with the
runtime fuel counter as the fallback for the code it cannot.

This is the most speculative of the four and the one with the best payoff: a language whose
gates publish "this function is O(n) with constant 7" is a different claim from "this function
is fast on my laptop."

## The synthesis — what should actually be built

**Fuel is the mechanism, quasi-determinism is the formal frame, oracle-relativity handles the
genuinely-external, and AARA is the long game.**

Concretely, in a staged order where each stage is falsifiable on its own:

1. **A cost model on MIR ops**, and a `fuel` counter injected by both renderers from it.
   Falsifier: a `spec/wasm_cross` fixture printing consumed fuel. If the legs disagree by one
   unit, stop — the shared-MIR premise is wrong and everything above collapses.
2. **`bounded(f, n) -> Result[T, Exhausted]`**, a contract stating full determinism (not merely
   quasi-), and a fixture pinning both branches. This is the construct that replaces
   `fan.timeout` — and unlike it, it is legal on pure computation, which is the entire point.
3. **Deterministic cancellation for `fan`**, revisiting #1000's exclusion with a budget instead
   of a clock. Requires 1 and 2; changes what the concurrency stance can promise.
4. **Oracle-relative contracts** for `process`/`http`/`fs`, folding #1040's effect-surface
   timeout and C-189's platform carve-out into one frame instead of two exceptions.
5. **AARA over MIR** for static bounds, if 1–3 hold.

Stages 1, 2 and 4 are enough to close #1040 *properly* — with a bound that works on pure code
— rather than by agreeing not to ask.

## What would falsify this

- **Fuel diverges across targets.** The likeliest cause is any place a renderer emits a
  different number of MIR-level operations — a target-specific pass, a peephole on one leg
  only. Stage 1's fixture finds it immediately, and it would mean the shared-MIR claim is
  weaker than believed, which is worth knowing regardless.
- **Overhead is unacceptable even when scoped.** Then fuel is a debugging-and-gating construct,
  not a production one, and `bounded` is honest about that.
- **AARA cannot handle the stdlib's shape.** Likely for parts of it; the runtime counter is the
  fallback, and a partial static bound is still a real claim.

## Why this is worth doing rather than living with the carve-out

The carve-out's cost is not felt today; it is felt the first time someone wants to bound a pure
computation — a fuzz iteration budget, a user-supplied expression, a proof search, an
interpreter step limit — and the only available answer is "run it in a subprocess." That is the
language telling the user to leave the language.

A deterministic bound is also, unusually, a claim that makes the project's *existing* claims
stronger rather than adding a new axis: byte-identity currently holds for programs that
terminate. Fuel extends it to programs that do not.
