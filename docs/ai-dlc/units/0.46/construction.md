# Unit 0.46 — Ledger

> Paired plan: [inception.md](./inception.md) — approved 2026-07-31 under the standing
> full-authority directive.

## Bolt ledger

| Bolt | What | Done-criteria for this Bolt | Status | Evidence |
|---|---|---|---|---|
| B1 | Choose the program and write down why | The shape and rough module layout are concrete; the rejected candidates carry their reason | **done** | Below |
| B2 | Skeleton: module/package layout, builds green, ~1k lines. Record build time | — | **first subcommand done, byte-identical** | `tools/almide-gates/` (2 modules, ~190 lines); output matches `LC_ALL=C bash docs/roadmap/generate-readme.sh` exactly — 390 lines, 59,262 bytes |
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


## B2 — the dogfood found a compiler bug in its first 180 lines

`tools/almide-gates/src/{main.almd, mdmeta/mod.almd}` (~180 lines) implements the first
subcommand: the `docs/roadmap/generate-readme.sh` replacement. It **type-checks clean**, and
the native build **fails**:

```
error[E0308]: mismatched types
3695 |         out.push(one(&*p));
     |             ---- ^^^^^^^^ expected `i64`, found `Result<i64, String>`
```

Minimized to 16 lines and filed as **[#1029](https://github.com/almide/almide/issues/1029)**:
an `effect fn` called inside a `for` loop body is not auto-unwrapped. Native emits invalid
Rust; **wasm silently prints heap addresses** (`[8336, 8356]` where `[1, 2]` was expected).
Same class as #1027 but worse — a single loop, and the wrong values print directly.

This is plan R3 landing on the first try, and it is the argument for the Unit: 180 lines of
real program found a silent-wrong-value bug that 324 spec files and seven fuzz campaigns had
not. The spec corpus is written by someone who knows the compiler; a program written to do a
job reaches for shapes nobody thought to test.

### Friction encountered (not bugs — language surface notes)

Recorded because a dogfood's job is also to report ergonomics:

- `?` is Result→Option; propagation is `!`. Reaching for `?` out of Rust habit cost a
  round-trip.
- `list.map`'s callback is PURE, so an `effect fn` inside it types the element as
  `Result[T, E]` and every downstream stage inherits the wrapper. The fix is to hoist the
  effect into a `for` — which is exactly the shape that hit #1029.
- `list.sort_by` takes a KEY extractor, not a comparator, and there is no `string.compare`,
  so descending order is ascending-then-reverse.
- One diagnostic pointed at the wrong definition: an E005 for `mdmeta.parse()` cited
  `effect fn main()` as "defined here".

### B2 result — one subcommand, byte-identical, and three bugs on the way

`generate-readme.sh` (101 lines of bash) is reimplemented in ~190 lines of Almide across two
modules, and its output is **byte-identical** to the original: 390 lines, 59,262 bytes.

The road there produced three findings, which is the Unit's whole thesis in miniature:

1. **[#1029](https://github.com/almide/almide/issues/1029)** — an `effect fn` in a `for`-loop
   body was not auto-unwrapped: native emitted invalid Rust, wasm printed heap addresses as
   values.
2. **[#1030](https://github.com/almide/almide/issues/1030)** — the ROOT of #1029, and smaller:
   list concat never constrained its right operand's element type. `[1] + ["a"]` type-checked;
   wasm printed `[1, 8244]`. Fixed with one call (`unify_infer` → `constrain`), which closed
   both issues and made #1029's diagnostic better than a bespoke one would have been.
3. **[#1031](https://github.com/almide/almide/issues/1031)** — a defect in the ORIGINAL: the
   bash generator's output depends on the machine locale (`sort`'s last-resort whole-line
   comparison follows ambient collation, and `LC_ALL` is not pinned). The committed
   `README.md` is therefore not reproducible; regenerating it elsewhere yields 63 lines of
   pure row-order noise. **Invisible while there was only one implementation** — a second
   implementation is what turned it into a diff.

The Almide version sorts by explicit byte order, so it is reproducible by construction rather
than by remembering to export a variable.

### Faithfulness notes (deliberate bug-for-bug reproduction)

A byte-match is the acceptance check, so the reimplementation copies the original's quirks
rather than improving them. Each is commented at its site:

- a `done/` file missing its `<!-- done: -->` line is silently DROPPED from the table (the
  bash `[ -z "$date" ] && continue`), while still counting toward "N items"
- equal dates are tie-broken by the whole row, descending — `sort -r`'s last-resort rule
- the trailing newline comes from the original's final `echo ""`

### State for the next session

**#1029 has a verified workaround** — `out = out + [one(p)!]` prints `[1, 2]` on native — so
B2 is NOT blocked; it can proceed with explicit `!` while the checker fix lands separately.
The fix direction is confirmed and recorded on the issue: stop unwrapping in NON-binding
positions and require `!`, which is what this codebase already did for the `if`/`match`
Result-ctor shape after the same class of failure. One step remains — locating which
unification path lets `[one(p)]` satisfy `List[Int]`.

`tools/almide-gates/src/` is committed and type-checks. B2 resumes by applying the `!`
workaround, then diffing the output against
`bash docs/roadmap/generate-readme.sh` — the byte-match is the acceptance check, and the
first attempt at that diff was a FALSE PASS (both sides empty, because the compile error was
swallowed and `cd` had moved the bash script's relative paths). Check both outputs are
non-empty before believing a match.

## Notes

- B1 was a decision Bolt by design (plan R1): sizing the program is the first real decision,
  and the Unit had to have a reviewable answer before any code.
