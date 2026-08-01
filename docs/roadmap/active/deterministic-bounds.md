<!-- description: Legalising fan.timeout under byte-identity — fuel, deterministic allocation, winner order, effect isolation -->
# Deterministic Bounds — legalising a bound on computation under byte-identity

**Revision note.** A first draft of this document claimed that introducing fuel would, by
itself, make `fan` cancellation deterministic. **That was wrong**, and the error is instructive
enough to keep visible: a *shared* fuel counter is still a function of scheduling, because
which sibling spends the budget first decides which one runs out. Fuel is a necessary
ingredient, not a sufficient one. Four other claims were overstated and are corrected in place;
each correction is marked.

The question is not "can we have a timeout." It is: **can a bound on computation be legal
inside the byte-identity contract**, rather than tolerated where the contract already does not
reach.

## The reframe, which survives review

`fan.timeout` was removed in 0.29.0 because a wall-clock bound makes a result a function of the
host's speed. The reframe is that **the failure is in the quantity, not in bounding**. Replace
wall-clock with a quantity the program itself determines and the property can survive.

This is not speculative. Wasmtime documents its two interruption mechanisms as exactly this
trade: **fuel** interrupts at the same point for the same program, same initial state and same
fuel — absent other sources of nondeterminism — while **epoch** interruption is wall-clock
based and its interruption point is not deterministic. The industry has already named both
sides.

**EVM gas** is the same argument at scale: every node must apply the same gas schedule to the
same state transition and reach the same out-of-gas verdict, which the Yellow Paper defines as
an exceptional halt. Determinism of the bound is the correctness argument, not a nicety.

## What the relational property actually is

*(Corrected: the first draft called this 2-safety unconditionally.)*

The property of interest relates two implementations:

> ∀ p, i.  `observe(native(p, i)) = observe(wasm(p, i))`

Calling this a **2-safety** property — violation demonstrable by at most two finite traces
(Clarkson & Schneider) — is clean **only on a fragment where every run reaches a finite
observation.** Full observational equivalence including divergence and co-termination is not
unconditionally 2-safety: a counterexample may require exhibiting that neither run ever
terminates, which no pair of finite traces shows.

This is not a technicality to route around; it is the reason the bounded fragment matters.
**Under a fuel bound every run lands in `Ok` or `Exhausted` in finite time**, so on that
fragment the property is finitely refutable and the 2-safety framing holds. The honest
statement:

> Observable equality on the *bounded fragment* is a relational safety property, and under the
> finite-observation semantics used here it is 2-safety. Outside the bounded fragment the
> claim is weaker and should not be stated as 2-safety.

One terminology check for this repo: what is written above is equality of **observable output**
(stdout, stderr, exit code — the contract ledger's definition). "Byte-identity" is also used
here for artifact-level byte equality in the org-verify sweep. They are different claims and
this document means the first.

## Path A — fuel, and the part that is genuinely hard

**The mechanism.** A budget in abstract steps, decremented by the program's own execution, is a
function of the program. Same program, same input, same budget ⇒ same outcome on any machine.

**Almide's real advantage, stated precisely.** *(Corrected: the first draft said a shared MIR
makes the two legs agree. It does not, on its own.)* Fuel across two backends needs both
backends to agree on what a step is. A shared MIR makes that *expressible*; it does not make it
*true*. What is required is stronger:

> Both renderers must **preserve the charge events of the shared MIR** — same events, same
> order, once each.

A renderer-local peephole that fuses two MIR ops, or reorders them, breaks this even though the
MIR was shared. So the invariant to gate is not "we have one MIR" but "the charge trace is
preserved by lowering":

    chargeTrace(native(M)) = chargeTrace(wasm(M)) = chargeTrace(M)

**Comparing total consumed fuel is not enough.** Two legs can consume the same total and still
diverge: if the charge ORDER differs, one leg can hit the bound mid-way where the other does
not. A fixture must therefore compare three things — `result`, `consumed_fuel`, and the
**charge-event trace**.

**A fixture refutes; it does not prove.** It is the cheapest falsifier and should be written
before the feature. Proof-shaped evidence is a validator that each charge event is preserved
exactly once — the same kind of object as the existing ownership certificate — plus property
testing over generated MIR.

### The cost model becomes part of the language semantics

Once a user can observe `Exhausted`, the cost model is no longer an implementation detail. It
needs, at minimum:

1. **Versioning.** A cost-model change is a semantic change and belongs in the contract ledger.
2. **Optimisation-level invariance.** `-O` must not change which programs exhaust.
3. **Renderer-optimisation invariance.** No target-specific pass may move a charge point.
4. **Defined arithmetic.** Overflow of the counter, and the meaning of nested `bounded`.
5. **Variable-cost operations.** What a list copy or a string concat costs, as a function of
   size, not a constant lie.
6. **A charging policy for host calls, allocation and RC traffic** — charged or free, stated.

## Path B — quasi-determinism: a precedent, not a theorem to lean on

*(Corrected: the first draft cited a "negative theorem" that the paper does not contain.)*

**LVars / Freeze-After-Writing** (Kuper, Turon, Krishnaswami, Newton — POPL 2014) is the
closest design precedent. Its relevant content is not a general theorem that non-monotonic
observation always destroys determinism; it is that **exposing a non-monotonic, mid-execution
property such as quiescence can force full determinism down to quasi-determinism** — where all
terminating runs agree on the value, or some run raises an error — and that the weakening can
be avoided when the information is not allowed to escape.

That is the right frame for a bound's two outcomes, and it names the honest retreat if the cost
model turns out only approximately portable. With a *deterministic* cost model the design lands
stronger than quasi-deterministic: `Exhausted` is an ordinary value.

**But only on the pure fragment.** *(Corrected: the first draft stated this loosely.)* For
`Exhausted` to be an ordinary value, the bounded computation must not leak partial state:

- no external I/O,
- no mutable state whose intermediate result escapes (or it is rolled back transactionally),
- no captured effectful closure.

So the signature carries an effect constraint, not just a function type:

    bounded(f: Pure[() -> T], n: Fuel) -> Result[T, Exhausted]

## Path C — oracle-relative contracts, and what they do NOT buy

*(Corrected on two points.)*

Fuel cannot help with a subprocess or a socket. Those can be organised as **external input**
rather than as exceptions, using the standard relational treatment. But the naive form —
quantifying over one shared `ω` — silently assumes both implementations consult the oracle at
the same semantic points and in the same order. The general form relates two oracle streams:

> ∀ p, i, ω_N, ω_W.  `R_Ω(ω_N, ω_W)` ⇒ `observe(native(p, i, ω_N)) = observe(wasm(p, i, ω_W))`

where `R_Ω` relates corresponding HTTP responses, filesystem results, and process outputs.

**And the limit must be stated plainly**: recasting a wall-clock timeout as an oracle event does
**not** make it byte-identity compatible. The two implementations can still receive the event at
different semantic points. Path C organises environment-derived nondeterminism into one frame —
replacing a growing list of carve-outs, including C-189's platform-reporting exemption — and
that is all it does.

## Path D — static bounds, scoped correctly

*(Corrected: "if the cost is known statically you do not need a runtime bound" was too strong.)*

**AARA** (RAML — Hoffmann, Aehlig, Hofmann; and descendants) infers polynomial resource bounds
for first-order functional programs, over evaluation steps, heap, or user-defined ticks. It is
incomplete: programs that are polynomial-time can still defeat the analysis.

So the accurate claim is:

> Where AARA proves an upper bound and that bound is within the policy budget, runtime metering
> can be **omitted for that call**. Everywhere else the runtime counter remains.

## The actual question: can `fan.timeout` be legalised?

Yes — but not by fuel alone, and the naming matters.

    deterministic fan bound  =  fuel
                             +  deterministic budget allocation
                             +  logical winner selection
                             +  effect isolation

### 1. Budget allocation must not be a race

A single shared counter is exactly the bug. Given

    fan.bounded(1000) { A  B }

a schedule that advances A by 900 steps first, and one that advances B first, disagree about
who exhausts — same total budget, different outcome, decided by the scheduler. **Allocation
must be a function of the source and the MIR**: either a deterministic split (`A: 500`,
`B: 500`), or a per-branch budget with a separate logical-time cap for the block. Which rule is
the design question; that there must be one is not.

### 2. The winner is chosen by logical order, never by physical completion

"Whichever branch finished first on an OS thread" is precisely what differs between native and
wasm. The selection rule must be total and syntactic:

- earlier deterministic logical completion step wins;
- ties break by source order.

So a branch completing at logical step 250 beats one completing at 300 regardless of wall
clock, and two branches completing at the same step are ordered by where they appear.

### 3. Cancellation is an OPTIMISATION, not an observable

This is the part that makes the whole thing implementable. What must be deterministic is:

- which branch's result is adopted,
- which branches are deemed `Exhausted`,
- which effects are committed.

*When* the native leg actually stopped a losing branch need not match wasm — provided it is
**unobservable**. Native may halt A instantly while wasm lets A run further; if neither leaks,
the observable behaviour is identical.

This holds only under the isolation from Path B: pure branches, or effects buffered and
committed only for the winner, or effects restricted to a cancellation-safe set.

### 4. Two constructs, because the user buys two different guarantees

*(Adopting the split.)* Technically fuel could be spelled `fan.timeout(1000)`, but `timeout(1000)`
reads as milliseconds to every user who has met the word before. A construct whose guarantee is
"deterministic computational budget" and one whose guarantee is "give up after some real time"
are not the same product:

    fan.bounded(1000)      // deterministic: logical budget, byte-identical outcome
    fan.timeout(5.seconds) // oracle-dependent: environment-relative, Path C contract

`fan.timeout` therefore comes back — as an **honestly environment-dependent** construct with the
oracle-relative contract — while the deterministic bound gets its own name and its own, much
stronger, promise.

## The claim, pushed to its real strength

The interesting statement is not "fuel fixes timeouts." It is:

> **Because `fan` is a compiler-known construct rather than a library, the compiler owns budget
> allocation, winner selection, and effect visibility — the three things beyond fuel that a
> deterministic bound requires.** A language where concurrency is a library cannot make this
> guarantee, because those decisions live in user code and in the scheduler.

That is what shared MIR plus a first-class `fan` buys, and it is a claim about the language's
shape rather than about one feature.

## Staged plan, each stage falsifiable

1. **Cost model on MIR ops + charge-trace preservation in both renderers.** Falsifier: a
   `spec/wasm_cross` fixture comparing `result`, `consumed_fuel`, and the **charge-event
   trace**. A one-unit or one-position disagreement stops the programme — and would mean the
   charge-preservation property is false, which is worth knowing on its own.
2. **`bounded(f: Pure[() -> T], n) -> Result[T, Exhausted]`** on sequential pure computation,
   with the cost model entered in the contract ledger as a versioned semantic object.
3. **Deterministic `fan.bounded`**: budget allocation rule, logical winner selection, effect
   isolation. Revisits #1000's cancellation exclusion — as a redesign of fan's semantics, not
   as a consequence of stage 1.
4. **Oracle-relative contracts** with `R_Ω` for `process` / `http` / `fs`, folding in #1040's
   effect-surface timeout and C-189, and re-admitting `fan.timeout` as the environment-relative
   construct.
5. **AARA over MIR** to omit metering where a proved bound is within budget.

## What would falsify this

- **The charge trace is not preserved by lowering** — most likely at a renderer-local peephole.
  Stage 1 finds it, and it invalidates stages 2–3.
- **Overhead is unacceptable even when scoped.** Wasmtime documents epoch interruption as the
  cheaper mechanism and reports roughly 10% for it; fuel is costlier, and no blanket figure
  should be quoted here without our own benchmark. Overhead depends on workload and charging
  granularity, which is exactly why the budget applies to an explicit bounded region rather
  than globally.
- **Effect isolation proves impractical** for the effects users actually want inside a bounded
  region. Then `bounded` stays pure-only, which is still the thing `fan.timeout` never was.
- **AARA cannot handle the stdlib's shape.** Likely in part; the runtime counter is the
  fallback and a partial static bound is still a real claim.
