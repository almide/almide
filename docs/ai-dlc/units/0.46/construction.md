# Unit 0.46 — Ledger

> Paired plan: [inception.md](./inception.md) — approved 2026-07-31 under the standing
> full-authority directive.

## Bolt ledger

| Bolt | What | Done-criteria for this Bolt | Status | Evidence |
|---|---|---|---|---|
| B1 | Choose the program and write down why | The shape and rough module layout are concrete; the rejected candidates carry their reason | **done** | Below |
| B2 | Skeleton: module/package layout, builds green, ~1k lines. Record build time | — | pending | — |
| B3 | Grow to ~5k lines of working functionality. Record build time | — | pending | — |
| B4 | Reach ~10k lines. Record build time; plot against the assumed linear | — | pending | — |
| B5 | Resolve #1003's and #1002's triggers with the measured numbers | — | pending | — |

## B1 — the program: `almide-gates`, this repo's own gate and generator toolchain

**Chosen.** Reimplement the repo's own quality-gate and doc-generation toolchain in Almide,
as one program with a real module layout.

The target it replaces is measured, not guessed: `scripts/`, `proofs/`, and `docs/` own
**35 shell scripts totalling 4,063 lines**, and they are the gates that run on every commit —
`check-contracts.sh` (contract↔fixture bidirectional link audit), `output-parity.sh`
(the 329-fixture ratcheting byte-match), `generate-conformance.sh` / `generate-readme.sh` /
`gen-claims.sh` (derived docs), `fuzz-track-record.sh` (the nightly streak table).

### Why this one

**It is self-verifying.** Every one of these has an existing output. A reimplementation is
correct exactly when its bytes match the bash version's — so the dogfooding cannot fool
itself the way a greenfield demo can. That property is rare and worth a lot: the usual risk
with a dogfood project is that it "works" in the sense that nobody has checked.

**It is used, not demonstrated.** These run on every commit and in CI. A bug in the Almide
version turns a gate red the same day, which is the pain signal #1001 exists to generate.

**It stresses the right surfaces.** File I/O, TOML and markdown parsing, string building,
process invocation, and enough data structure to hold a 200-contract ledger and a 329-row
parity baseline. That is the module system, the package system, and the diagnostics under
load — the things the bundled `stdlib/` (59k lines, per the plan's correction) does NOT
exercise, because it compiles through `bundled_source()` rather than as a user project.

**It removes bash.** 4,063 lines of untyped shell currently guard the project's strongest
correctness claims. Moving them to a typed language with real errors is an improvement
independent of the dogfooding.

### Candidates rejected, with reasons

- **Task-bank tooling for almide-dojo.** Real need, but mostly JSON plumbing and HTTP — it
  would exercise the stdlib more than the language, and it has no existing output to diff
  against, so correctness would rest on my own judgement.
- **An `.almd`-aware code-search/index tool.** The most language-stressing option (a lexer
  and parser in Almide would exercise pattern matching and recursion hard) and it would
  reach the line count easily. Rejected for now because it duplicates the Rust frontend
  without a byte-identical oracle to check it against — a second parser that disagrees with
  the first is a liability, not a proof point. Worth revisiting AFTER the gates land, when
  there is a working large program to build it on.

### On the line count

The plan's R2 warns against picking a target to hit 10k. So: 4,063 lines of bash will
probably become **6–8k lines of Almide**, not 10k — shell is terse where it is unsafe, and a
typed reimplementation spends lines on the structure that makes it safe.

**If it lands short of 10k, that is not a failure of the Unit.** The number in #1001 is a
proxy for "large enough that the module system, the build-time curve, and the diagnostics are
under real load". Whether the proxy is met is a question the build-time table in B2–B4
answers directly, and it answers it better than the line count does. If the curve is flat and
the diagnostics hold at 8k, the Unit has produced its signal.

### Rough module layout

    almide-gates/
      src/
        main.almd          — subcommand dispatch (check-contracts, gen-docs, parity, …)
        ledger/            — contracts.toml parse + the bidirectional link audit
        parity/            — the fixture sweep + the ratcheting baseline
        docsgen/           — conformance.md / README.md / claims generation
        fuzznight/         — the nightly streak table
        fs_util/, md/, toml/ — shared helpers

Each subcommand is independently diffable against its bash original, so B2–B4 can land one
at a time with the byte-match as the acceptance check rather than deferring all verification
to the end.

## Notes

- B1 was a decision Bolt by design (plan R1): sizing the program is the first real decision,
  and the Unit had to have a reviewable answer before any code.
