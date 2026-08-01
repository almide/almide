<!-- description: Unit 0.50 construction — the decade gate: numbers published, derived, ratcheted -->
# Unit 0.50 — Construction

The decade's exit audit. #999's complaint was that build-speed, runtime-perf and safety numbers
existed but **none of them were in the README** — and a gate that publishes nothing measurable
is not a gate.

## What shipped

**S1 / S3 — the numbers are published and DERIVED, not typed.**

Two generated blocks in `README.md`, each rewritten between markers and freshness-checked in
CI, so a hand-edit is a defect the gate catches rather than a change a reader is trusted not to
make:

| block | source | generator | freshness gate |
|---|---|---|---|
| ledger claims (contract counts, flagged split, exceptions) | `docs/contracts/contracts.toml` | `scripts/gen-claims.sh` | `check-contracts.sh` |
| build speed | `docs/benchmarks/build-speed.txt` | `almide-gates -- bench --readme` | `-- bench --check-readme`, CI |

**S4 — the ratchet.** `-- bench --check` re-measures and fails at 1.5× the committed baseline.
It is deliberately a SEPARATE, opt-in step rather than part of the PR gate: the numbers are
machine-dependent, so re-measuring on a CI runner would fail for every machine except the one
that produced the baseline. **Freshness belongs in CI; the ratchet belongs where the baseline
was measured.** Conflating them would produce a gate that is red for the wrong reason and
therefore switched off.

Tolerance is 1.5×, and the reason is in the code: build timing on a shared machine is noisy at
the tens-of-percent level, so a gate that fires at 5% gets disabled within a week — which is
worse than no gate. 1.5× catches an added pass or a lost cache and ignores load.

**S2 — runtime perf is still NOT published**, and that is the same decision #999 made, held.
The `lang-bench` harness exists; wiring it is not this Unit's remaining hour. The README says
so in place of a number.

## The harness is an Almide program, and the reason is not dogfooding

`tools/almide-gates/src/bench.almd` — the eighth gate this repository runs on itself.

The dogfood argument applies, but there is a sharper one here: **a benchmark harness is the one
program whose own cost is part of its answer, and shell has no clock.** Every shell attempt
reaches for an external process to read the time, and on a 30ms measurement that process IS the
measurement. `datetime.monotonic_ns()` is in-process, so the only thing between the two
readings is the work.

This was not a prediction. A first draft of this harness was shell calling `python3 -c` for
each timestamp — two interpreter spawns per repetition, ~45ms each — and it reported a 28ms
command as **117ms**. The harness was four times the size of the thing it measured.

## Three measurement failures this Unit made, and what each is guarded by now

Recording all three because they are one habit, not three accidents: **a number was believed
before checking that the fast path did any work.**

| failure | what was actually measured | guard now in the code |
|---|---|---|
| `almide check` at 47ms, then 28.3ms twenty minutes later | one run on a loaded machine | every row is an N-run mean and **N is published** |
| a whole build curve where cold == warm at every size | `almide clean` clears the DEPENDENCY cache; artifacts live in `$TMPDIR/almide-run` | cold clears **both**, before every repetition |
| `almide check` at 117ms | two `python3` spawns per repetition | the loop is in-process; `datetime.monotonic_ns()` |

Each guard is a comment at the value it protects, so raising a tolerance or dropping an N means
disagreeing with a measurement written next to it.

## Done-criteria

- [x] Every published number states its method and its measurement date — the provenance line
      carries the binary version, the machine, the target, its size, and the date
- [x] No number is hand-typed where a generator could produce it — both blocks are spliced
      between markers from a committed source
- [x] A ratchet exists and CI fails on regression — freshness in CI, the 1.5× ratchet at the
      measuring machine, with the split reasoned above
- [ ] Runtime perf (S2) — deliberately unpublished, as #999 already decided; `lang-bench`
      remains the way in

## Retrospective (Try)

**Keep**: separating *measuring* from *publishing*. The generated block renders the committed
baseline and never re-measures, which is what lets a freshness check be a string comparison
that passes on any machine.

**Keep**: writing the guard next to the value. "N=20" is a number; "N=20 because one run of a
30ms process is scheduler noise, and here is the run where that bit us" is a decision that
survives the next person in a hurry.

**Change**: the three failures above were all caught by the number being *implausible*, not by
process. The cheap check is: before believing a fast number, confirm the fast path did work —
a cold build equal to a warm one, a 36× speedup from one flag, and a harness slower than its
subject were all visible on the face of the result.
