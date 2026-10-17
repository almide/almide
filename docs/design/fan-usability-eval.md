# Fan usability evaluation: can a writer pick the right fan, and does it misread cancellation?

> Last updated: 2026-10-17 · design only. The evaluation runs in
> [almide-dojo](https://github.com/almide/almide-dojo), as all MSR work does
> (CLAUDE.md, "Repo Boundary"). Its result decides
> [ADR-0028](../adr/0028-fan-selection-names-and-cost-contract.md)'s
> falsifier.

## What is measured

The evaluation answers two questions.

1. **Selection.** Given a task described in plain words, does the writer (a
   model or a person) choose the fan form whose selection kind matches?
   The kinds are K1–K5 in [fan-cross-language.md](./fan-cross-language.md).
2. **Cancellation.** Does the writer believe that something is stopped, or
   undone, when it is not? Examples: that `fan.map` stops at the first Err,
   that `fan.any` cancels a request already sent, or that `fan.race` stops
   losers early.

Both are measured as *behaviour of the written program*, judged by an
oracle, and never by reading the prose the writer produced.

## Task bank

Each task is a short brief, a starter file and a hidden oracle. The brief
never names a fan form. Six families, each with 6 tasks (36 in total), half
"write" and half "modify" (MSR):

| Family | Brief shape (example) | Correct answer | Typical wrong answer |
|---|---|---|---|
| F1 K1 | "Try the mirrors in priority order and use the first one that answers." | `fan.any(mirrors, f)` | `fan.race` (expecting first in time) |
| F2 K4 | "Several solvers are pure; pick the one that needs the least work, the same answer on every machine." | `fan.race { … }` | `fan.any` (takes the first, not the cheapest) |
| F3 K5 all-or-error | "Fetch every page; fail if any page fails, and report which one." | `fan.map(xs, f)!` | a `for` loop that stops early, when the brief requires every page to be attempted |
| F4 K5 all results | "Run every check and print a report with each check's outcome." | `fan.settle(xs, f)` | `fan.map` (loses the per-element outcome) |
| F5 not expressible | "Use whichever server answers first; ignore the slow one." (K2), or "return as soon as any job finishes, even if it failed." (K3) | say it is not expressible, and use `fan.any` with the stated trade-off | `fan.race` with I/O arms (E006), or claiming the slow call is cancelled |
| F6 cancellation | "Stop the remaining uploads once one fails." | a `for … !` loop, since `fan.map` never stops early | `fan.map`, with a claim, comment or test that the later uploads did not happen |

Modify tasks start from a correct program of the family and change one
requirement. Examples: "the error must be the first page in the list, not
the first to fail" (already true, so no change is needed), or "also keep
the failed outcomes" (`fan.map` → `fan.settle`).

## Oracle

Every task's oracle is a deterministic program in the form of
`tests/fan_acceptance_test.rs`:

- **Same output on every leg.** The submission is run on four legs: native,
  native with `ALMIDE_FAN_COST_OFF=1`, native with `ALMIDE_FAN_SEQUENTIAL=1`,
  and wasm. Their stdouts must be equal.
- **Expected output.** The stdout must equal the brief's expected output.
- **Slow and fast elements.** The oracle's helper fns make some elements slow
  and some fast, so an answer that relies on completion order fails.
  Expected outputs never depend on wall time.
- **Side effects are logged.** Every effect is a `println` with a tag, so the
  oracle can tell which elements ran (F1, F6).

There are two scores per task:

- **Selection correct.** The program compiles on both targets and the
  oracle's stdout matches.
- **Misconception.** A finding when the program, its comments or the
  writer's explanation asserts any of these: an unstarted or stopped element
  that the oracle shows did run; a cancelled request; or a K2/K3 result.
  Dojo's existing malicious-hint detector already reads comments, and this
  reuses it with a fixed phrase list (`cancel`, `abort the others`,
  `whichever finishes first`, `stops the rest`).

## Conditions

| Condition | Context given to the writer |
|---|---|
| C0 | the current CHEATSHEET fan section, verbatim |
| C1 | C0 plus ADR-0028's D-a hint text and D-c glosses |
| C2 | C1 with `fan.first_ok` / `fan.cheapest` aliases spelled in, i.e. option B. This is measured only to size B's benefit; nothing ships from it. |

The writers are the Dojo model roster for the day. Each (task, condition,
model) runs 5 samples. The model, sample count and date are recorded with
each result row.

## Procedure

1. Freeze the task bank and oracles in Dojo under `tasks/fan/`, versioned.
2. For each condition, run all tasks × all models × 5 samples. This happens
   outside CI, with the API keys on the operator's machine (Dojo rule).
3. Score each run on selection and misconception. Report per family and per
   condition, with the total counts beside each rate ("of how many").
4. Compare C0 → C1 to see whether option A's diagnostics and glosses reduce
   the misreadings. Compare C1 → C2 to see how much more renaming would buy.
5. Apply ADR-0028's falsifier. If the F1 + F2 + F5 misreading rate under C1
   is above 20%, revisit option B or C.

## What this evaluation does not claim

- It measures the written program, not understanding.
- It does not measure latency. A slower but correct F1 answer passes.
- A family with fewer than 30 scored runs per condition is reported but not
  used for the falsifier.
