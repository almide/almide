<!-- description: Drive committed shell to zero; the two primitives that block it -->
# Zero Committed Shell

**The goal is not "stop using shell." It is: no `.sh` file is committed to this repository.**
Interactive shell stays exactly where it is — a one-liner in a terminal, `grep | sort | uniq -c`,
`for f in *.png` — and nothing here argues otherwise. What changes is that anything durable
enough to be committed, reviewed, and depended on becomes a program.

## This is not a proposal. It is an extrapolation from a measurement.

Unit 0.46 ported seven of this repository's gates from bash to Almide with **byte-identity as
the acceptance check**, and the last one additionally against **15 mutations**:

| | |
|---|---|
| bash replaced | ~1,300 lines across 7 gates |
| Almide written | ~2,300 lines, 14 modules |
| tests | 47, on decision rules that in bash were reachable only through their I/O |
| defects found by porting | **8** |

Five of the eight were the same shape — **a rule duplicated because duplication was cheaper
than the abstraction** — and that shape is not an accident of who wrote the scripts. Shell has
no modules and no type checker, so duplication IS the cheapest option, and every copy agrees
on the day it is written. A locale pin in eleven scripts; an unquoting expression in five
extractors; an evidence-class enum in two places. The port did not find them by being clever;
a byte-diff refused a plausible restatement.

## Where it stands

**50 committed `.sh` files, 6,159 lines**, of which 7 gates have byte-identical Almide twins
today (the `.sh` originals stay as the oracle until the ratchet says otherwise).

The count is the ratchet: **down only**. A new `.sh` file added to the repository is the thing
this item exists to prevent, and a gate that counts them is one line.

## The two primitives that block the rest

Measured, by hitting them:

### 1. A timeout — and the honest form it can take

`output-parity` needed to bound a subprocess and **still shells out to
`perl -e 'alarm …; exec …'`**. The Almide port depends on perl, which is absurd for a program
whose point is that the shell dependency is gone.

`fan.timeout` used to exist and was **removed in 0.29.0**, correctly: a timeout on a PURE
computation makes its result a function of the machine, and byte-identity across native and
wasm is the one thing the model exists to guarantee. Reintroducing it as it was would be
undoing the decision, not improving on it.

**The line that can be drawn**: a timeout is admissible exactly where **the operation it
bounds is already outside the byte-identity contract.**

| bounded thing | already nondeterministic? | timeout admissible |
|---|---|---|
| `process.exec_status` (spawns a process) | yes | **yes** |
| `http.get` (network) | yes | **yes** |
| reading a pipe / a socket | yes | **yes** |
| a pure `fan` sibling | **no** | **no — this is what 0.29.0 removed** |

Bounding a subprocess adds no nondeterminism that spawning it did not already add. Bounding
`fib(35)` invents some.

And the contract can be stated precisely rather than waved at, in two halves:

> **Guaranteed byte-identical**: IF the timeout fires, both targets produce the same error
> value, the same message, and the same exit path.
> **NOT guaranteed**: whether it fires. That is a function of the host.

A fixture can pin the first half by making the second half not a coin flip — a 5-second sleep
against a 100ms bound fires on any plausible machine. That is exactly how C-200's fan-sibling
trap fixture already works (a 1.5s sleeping sibling, both targets aborting in ~0s).

So the shape is a bound on the **effect surface**, not a general combinator:
`process.exec_status(cmd, args, timeout_ms)` and its siblings — never `fan.timeout(thunk, ms)`.

Tracked as [#1040](https://github.com/almide/almide/issues/1040).

### 2. Effectful list combinators

`list.map`'s callback is PURE, so `list.map(files, (f) => read_text(f))` types the element as
`Result[T, E]` and every downstream stage has to unwrap it. The dogfood's own code works around
this with `var` + `for` — the exact pattern
[CLAUDE.md](../../../CLAUDE.md#prefer-list-combinators-over-var--for) tells contributors not to
write. A tool whose own idiom guide it violates is evidence the surface is missing something,
not evidence the author was lazy.

Tracked as [#1041](https://github.com/almide/almide/issues/1041).

## Two more frictions, named but not blocking

- **No lazy pipeline composition.** `exec_with_stdin` exists; `A | B` streaming does not, so a
  large intermediate output is held in memory. No gate has needed it yet.
- **Startup + compile latency.** 13.3ms process startup, 0.37s warm build, 0.93s cold. **This
  is shell's real moat** and this item does not claim to cross it: nobody pays 0.4s for a
  throwaway three-liner. It is also why the goal is scoped to COMMITTED shell — a file worth
  committing is a file worth 0.4 seconds.

## Done-criteria

- `git ls-files '*.sh'` returns **zero**, or every remaining entry carries a written reason.
- A gate fails CI when the count goes up.
- Each replacement is byte-identical to the script it replaces, or — where there is no stable
  output to diff — carries a mutation suite, as `check-contracts` does.
- #1040 and #1041 are closed or explicitly deferred with the numbers that justify it.

## Risks

- **R1 — porting for its own sake.** A script nobody runs is not worth a program. Absorption:
  order by blast radius, gates first; a script with no consumer gets deleted, not ported.
- **R2 — the Almide twin drifts from the `.sh` oracle.** Absorption: the `.sh` original stays
  in-tree until the twin has a mutation suite, and the byte-diff runs in CI.
- **R3 — the timeout design reopens what 0.29.0 closed.** Absorption: the admissibility rule
  above is a CONTRACT, not a convention — `fan.timeout` stays removed, and the tombstone
  diagnostic keeps pointing at the effect-surface form.
