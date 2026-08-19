# SPIKE S1 — salsa spine over the ported parser: VERDICT

Date: 2026-08-19. Gate: ARCHITECTURE.md §6.5. Harness:
`cargo run --release -p almide-spine --bin spine_bench` (crates/almide-spine).
Machine: local dev (darwin/ARM); all numbers are same-machine relatives, the
form the incumbent's own edit-loop gate insists on (wall-clock absolutes swing
6x under load — scripts/edit-loop-scale-baseline.txt:4-8).

## Numbers (real corpus: spec/, 1,098 files, 55,089 lines)

| measurement | result |
|---|---|
| batch front-end re-parse (what `almide check`'s front end pays per invocation) | 63.81 ms (median/30) |
| salsa cold (first full derive) | 69.58 ms, 1,098 parses |
| **(b) cold overhead vs batch** | **+9.0% — PASS** (<20%) |
| salsa warm re-derive after a 1-file edit | **0.182 ms** (median/30) |
| **(a) parses per warm round** | **max 1, and 0 on a no-edit round — PASS** |
| **(c) warm vs batch** | **351x — PASS** (≥10x) |

## What this proves

- The query mechanics work on the REAL ported parser over the REAL corpus:
  invalidation is exactly file-precise, memo hits are free, and salsa's
  bookkeeping costs ~9% once at cold start.
- The incumbent's structural tax — re-lexing/re-parsing the world (including
  the 4,142-line bundled stdlib) on every check — is not a law of nature:
  the identical parser, unmodified, does the same work incrementally at 351x
  on the slice measured.

## What this does NOT prove (do not over-claim)

- The check phase is 73% of the incumbent's loop (`share_check 0.7320`) and
  is not ported. Full-loop speedup TODAY would be ~1.4x. The big number
  requires sema-as-queries (unit 4) — per-function typecheck queries with
  cross-module dependencies, which is the remaining concentrated risk.
- Single-file edits only; no measurement yet of edits that change a module
  interface (fan-out invalidation). That is a unit-4 spike question.

## Decision (per §6.5 gate)

All three criteria green → **greenfield continues**, with the order amended:
next is the **unit-4 sema spike** (per-function check queries + interface
fan-out measurement), before the interp port. Independent of this verdict:
E1 (misspellings / code registry / multifix) and ADR-0002/0012 remain
develop-eligible and are queued for backport — they are not greenfield
returns and never were.
