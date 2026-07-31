# Unit 0.46 — Ledger

> Paired plan: [inception.md](./inception.md) — approved 2026-07-31 under the standing
> full-authority directive.

## Bolt ledger

| Bolt | What | Done-criteria for this Bolt | Status | Evidence |
|---|---|---|---|---|
| B1 | Choose the program and write down why | The shape and rough module layout are concrete; the rejected candidates carry their reason | **done** | Below |
| B2 | Skeleton: module/package layout, builds green, ~1k lines. Record build time | — | **first subcommand done, byte-identical** | `tools/almide-gates/` (2 modules, ~190 lines); output matches `LC_ALL=C bash docs/roadmap/generate-readme.sh` exactly — 390 lines, 59,262 bytes |
| B3 | The TOML reader (the load-bearing component) | Parses the real `contracts.toml`; its own tests | **done** | `tools/almide-gates/src/toml/mod.almd`; 200 tables / 369 evidence items on the real ledger — matching independent `grep` counts; 5 tests green |
| B4 | The contracts-README subcommand (the first TOML consumer) | Byte-identical to the bash original | **done** | 231 lines / 25,297 bytes identical; found and fixed a truncation bug in the original (#1032) |
| B5 | `conformance.md` — the third TOML consumer | Byte-identical | **done** | 81 lines identical; surfaced a native codegen bug (#1033) |
| B7 | `fuzz-track-record` — the first subcommand that parses JSON | Byte-identical | **done** | 12 lines / 759 bytes identical, first try; replaced `gh api --jq` with real `json` parsing so the response shape faces the type checker |
| B6 | `output-parity` — the first subcommand that RUNS things (3 processes/fixture, 3 observables, a retry, a ratchet) | Byte-identical | **done** | 10 lines / 607 bytes identical, both exit 0; the port's first draft produced 32 FALSE xfails and named the cause: a stream's final newline is a terminator, not an empty line |

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

### The subcommand order, and why the next one is bigger

`generate-readme.sh` was the right first target because it reads markdown front-matter — no
parser needed beyond line scanning. The remaining generators are not uniform in difficulty,
and the order matters:

| Subcommand | bash | What it needs from Almide | Size |
|---|---|---|---|
| roadmap README | 101 lines | line scanning | **done, byte-identical** |
| contracts README | 101 lines | **a TOML reader** (awk parses `contracts.toml` today) | next, and it is the real step up |
| conformance.md | — | the same TOML reader | after |
| `check-contracts.sh` | — | TOML + the bidirectional link audit | the largest |
| `output-parity.sh` | 190 lines | process invocation + a 382-row baseline | independent of TOML |
| `fuzz-track-record.sh` | — | `gh` invocation + date arithmetic | independent |

The TOML reader is the load-bearing piece: three of the six need it, and it is the first part
of this program that is a real component rather than glue. That makes it the natural B3 —
build it once, with its own tests, and three subcommands follow.

**Its size is bounded, and measured.** `contracts.toml` uses a SMALL subset of TOML — exactly
eight key names, one table kind, and two value shapes:

| construct | count | note |
|---|---|---|
| `[[contract]]` array-of-tables | 200 | the only table form used |
| `key = "..."` string | 8 distinct keys | `id`, `spec`, `title`, `statement`, `since`, `status`, `doc`, `evidence` |
| `key = [` multi-line array | 200 | always `evidence` |
| `{ path = "...", class = "...", name = "..." }` inline table | 369 | array elements |

No nested tables, no dotted keys, no dates, no numbers, no multi-line basic strings. A reader
for THIS subset is a line scanner with three states — well short of a general TOML parser, and
the reader should say so in its header so nobody later mistakes it for one.

**There is a working template**: `stdlib/json*.almd` is a self-hosted parser in Almide (99
fns across 9 files). It proves the shape works in this language and shows the idioms — which
is worth reading before starting rather than rediscovering.

`output-parity.sh` and `fuzz-track-record.sh` need no TOML, so they can land in parallel if
the TOML work stalls. Recording that so the next session is not forced into a single chain.

### B3 result — the reader works on the real ledger

`tools/almide-gates/src/toml/mod.almd`: a three-state line scanner for the measured subset,
with 5 tests. Run against the actual `docs/contracts/contracts.toml`:

```
tables: 200
first id: C-001
first evidence key: evidence  items: 3
total evidence items: 369
```

**200 and 369 match the counts taken independently with `grep`** before the reader existed —
which is the check that matters, because a parser that silently drops or duplicates blocks
would still print a plausible number.

Two design notes worth keeping:

- **The narrowness is deliberate and stated in the file's header.** It is a reader for the
  subset the ledger uses, not a TOML parser, and anyone reaching for general TOML should
  write a general parser rather than widen this one. A file that says what it is NOT is
  cheaper than one that quietly grows into something nobody audits.
- **The scanner threads state as arguments** rather than a mutable cursor, which is the shape
  CHEATSHEET recommends over `var i` + `while`. Writing it the recommended way was also the
  easier way here — the three states are the three argument shapes.

One language friction: `then (` followed by a newline is a parse error; the block form
`then { … }` is required. Not a bug, but the diagnostic (`Expected expression … got Newline`)
does not suggest the fix.

### B4 result — and a third defect in the original

`contracts-readme` is byte-identical to `docs/contracts/generate-readme.sh`: **231 lines,
25,297 bytes**. `almide-gates` is now ~400 lines across four modules.

The diff came down to exactly one row, and **the Almide side was the correct one**:

```
bash:   | C-050 | string.split(\ | 0.24.0 | active | fixture | 1 |
almide: | C-050 | string.split(\"\") and string.run_length_encode are codepoint-granular | … |
```

The awk unquoted values with `sub(/".*$/,"",v)` — delete from the FIRST remaining quote — so
a title containing an escaped quote was truncated. `docs/contracts/README.md` had been
publishing a cut-off contract name. Filed and fixed as
**[#1032](https://github.com/almide/almide/issues/1032)**, applying the fix to all five
extractors rather than just `title`: the defect is in the unquoting rule, not in the field,
and fixing one field is how the next one gets missed.

**This is the second time the reimplementation was right and the original was wrong**
(#1031 was locale-dependent sort order). Both share a cause worth naming: with one
implementation there is nothing to diff against, so a wrong answer and a right answer look
identical. That is the dogfood's value stated precisely — not "we found bugs in the
compiler", but "a second implementation makes the first one falsifiable".

**A faithfulness decision.** The Unit's rule is that a byte-match is the acceptance check and
the reimplementation copies the original's quirks rather than improving them. That rule was
written for behavioural conventions (dropping undated rows). It does NOT extend to a
corruption of published output: reproducing the truncation would have meant writing code
whose purpose was to be wrong. The bash was fixed instead, and both now agree.

### Two more language-surface notes

- `env.args()` returns the USER arguments only, 0-based — the program name is not element 0.
  Reaching for argv[0] out of C habit cost a debug cycle.
- `inline_get` (quoted values) and `inline_num` (bare values like `n = 1000`) are separate
  functions on purpose. One function guessing between the two grammars would silently return
  the wrong thing for `n = "1000"`, and the ledger has exactly one bare field — small enough
  that the ambiguity is avoidable rather than manageable.

### B5 shape — what `conformance.md` needs beyond what exists

Analysed before starting, so the next session does not re-read the script:

`generate-conformance.sh` is **embedded Python**, not awk — a different shape from the two
already ported. Beyond the TOML reader it needs four things, none of which exists yet in
`almide-gates`:

1. **Group by `spec` key.** Contracts are joined per ALS section; the reader returns them in
   source order, so this is a grouping pass over `List[Table]`.
2. **The ALS sort key.** `ALS-T6` sorts as `("T", 6)`, not lexically — so `ALS-T10` follows
   `ALS-T9`, which a string sort gets wrong. `list.sort_by` takes a key extractor, so the key
   has to be a single sortable value: zero-pad the number into the string (`"T" + pad(6)`).
3. **Evidence filtered to `fixture`/`exhaustive`.** Not every class counts here, unlike the
   README table which ranks all of them.
4. **"How CI runs it" from the PATH**, four buckets: `spec/wasm_cross/` → byte-compare,
   `tests/diagnostics/` → checker, other `spec/` → both-target test, `tests/*.rs` → cargo
   gate. A prefix match, and the ORDER matters — `spec/wasm_cross/` must be tested before the
   general `spec/` prefix or every fixture reads as "both-target test".

Also needed: **distinct** fixture counting across sections (a fixture cited by two contracts
counts once), which is a set operation — `list.unique` or a sort-then-dedup.

Point 2 is the one worth flagging: it is a real difference between the two languages here.
The Python uses a tuple key `(letters, int(digits))`; Almide's `sort_by` wants one value, so
the number has to be encoded into the string rather than compared separately.

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

## The two remaining TOML-free subcommands — analysed, not started

Both need capabilities `almide-gates` does not yet have, and they need DIFFERENT ones, so
neither unlocks the other:

**`scripts/fuzz-track-record.sh`** (85 lines) — needs **process invocation** (`gh api`) and
**JSON parsing**. The stdlib has `json` (99 fns) and `process`, so both exist; what does not
exist is any use of them in this program. Its logic is a streak fold over nightly runs.
Risk: it hits the network, so a "byte-identical" check is against a moving target — the
acceptance check has to be a FIXED captured JSON input, not a live `gh` call, or the diff is
untestable.

**`proofs/output-parity.sh`** (190 lines) — needs **process invocation** (`almide build`/`run`
per fixture) and a 382-row baseline. No JSON. Its bash version already refuses to run when
the PATH binary and the workspace build disagree — a guard worth preserving verbatim, since
it is the same one-binary discipline the fuzz campaigns depend on.

**`scripts/check-contracts.sh`** is the largest overall (TOML + the bidirectional link audit
+ freshness checks) and the one whose correctness matters most, since it gates every contract
change. It goes LAST, when the reader and the process-invocation pieces are both proven by
smaller subcommands.

Suggested order: `output-parity` (process invocation, no JSON) → `fuzz-track-record` (adds
JSON) → `check-contracts` (composes everything). Each step adds exactly one capability.

## Note for whoever picks up #1033

`git stash list` on this machine shows `stash@{0}: sibling: codegen render_expr Var-arm
extraction WIP`. That is the exact area #1033 lives in (`render_expr` on a `Var` deciding
ownership per-occurrence). It is NOT mine and was not touched. Check with its owner before
starting there — the fix may already be in flight.

## B6 — the toolchain stamp (2026-08-01)

`proofs/lib/stamp.sh`'s `stamp_toolchain` is ported and **byte-identical** (7 lines).
`almide-gates` is now ~570 lines across six modules, with **four** matching subcommands.

Ported before the rest of `output-parity.sh` on purpose: it is the SHARED piece every proof
gate calls first, it is small, and its failure mode is silently-wrong evidence rather than a
crash. It also earned its keep during this very session — a parity run refused to start until
`make install` caught the PATH binary up with the workspace build.

The Almide version reproduced that: run against a tree where `cargo build --release` had
outrun `make install`, it printed the same FATAL and exited 1.

```
FATAL: PATH almide (f35f6fc687fe538e) != workspace build (4bbc743d07767e9a).
```

Two language-surface notes:

- **`ok` cannot be a binding name** — it is the `Result` constructor. The parse error
  (`Expected identifier … got Ok 'ok'`) names the token but not the reason.
- The stamp arm must NOT go through `main`'s trailing-character trim. That trim exists
  because the GENERATORS' bash originals end with `echo ""`; `stamp_toolchain` ends with its
  rule and no blank. The arm prints and exits instead of returning a body — a reminder that
  "every subcommand ends the same way" was an assumption, not a fact.

Remaining: `output-parity` proper (the 382-row fixture sweep on top of this stamp),
`fuzz-track-record` (adds JSON), `check-contracts` (composes everything).

## B7 — the parity verdict rule, extracted and tested (2026-08-01)

`tools/almide-gates/src/parity.almd`: `verdict_of` is `output-parity.sh`'s `run_one` decision
as a PURE function of what the three commands did, with 4 tests.

That extraction is the point. In the bash the five verdicts (match / mismatch / wall /
runerr / v0fail) are interleaved with process invocation, so the RULE cannot be exercised
without running the whole 382-fixture sweep. Pulled out, it is four assertions that run in
milliseconds — including the two whose order is load-bearing:

    render fails + oracle also failed  → v0fail
    render fails + oracle succeeded    → wall

Getting those backwards reports every broken fixture as a v1 wall, and **the wall count is a
ratchet** — so a mislabel there inflates a number the project manages as debt. The bash has
it right; nothing tested that it stayed right.

This is a second kind of value from the port, distinct from finding defects: the
reimplementation can be STRUCTURED so the decision logic is testable, where the original's
shape made it reachable only through its I/O.

Remaining for `output-parity`: the sweep itself (382 fixtures × 3 processes) on top of this
rule and the B6 stamp. Then `fuzz-track-record` (adds JSON), then `check-contracts`.

## B8 — the nightly streak rule, extracted and tested (2026-08-01)

`tools/almide-gates/src/streak.almd`: the fold from `scripts/fuzz-track-record.sh`, pure,
with 6 tests. Same treatment as B7's verdict rule and for the same reason — in the bash it is
interleaved with `gh api` calls, so the RULE can only be exercised by hitting the network.

The rule worth pinning is that **the two streaks stop independently**:

| night | green streak (#796, closes at 2) | full-budget streak (#924, closes at 14) |
|---|---|---|
| no findings | continues | continues |
| findings | **ends** | **continues** — the instrument ran to completion |
| truncated | ends | ends |

A night with findings ends one and not the other, because #924 measures whether the
instrument RAN, not whether it was quiet. Collapsing them into one counter — the obvious
simplification, and what someone reading the bash quickly would write — makes #924
unmeasurable: every finding would reset it, and the 14-night streak could never accumulate on
a compiler that is still being fixed.

One test pins the state this session actually measured (four full-budget nights, all with
findings → #924 at 4/14, #796 at 0/2), so the rule is anchored to a real observation rather
than only to invented cases.

`almide-gates` is now **~680 lines across eight modules**, with four byte-identical
subcommands and two extracted decision rules under test.

## B9 — the bidirectional-link rule, extracted and verified on the real ledger (2026-08-01)

`tools/almide-gates/src/contract_audit.almd`: `check-contracts.sh`'s symmetry rule, pure,
with 5 tests — and run against the actual ledger:

```
fixture evidence links checked: 346
asymmetric (ledger says, fixture does not): 0
```

An independent implementation reaching the same conclusion as the bash audit is the strongest
check available short of byte-identity, and it is available NOW, before the full subcommand
is ported.

The rule reports its two asymmetries **separately**, because they fail differently:

- **ledger lists the fixture, the fixture does not name the contract** — the contract's
  evidence points at a file certifying something else. The contract looks covered and is not.
- **the fixture names the contract, the ledger does not list it** — the fixture is not
  counted, so deleting it would break nothing and nobody would notice.

Collapsing them into one "links disagree" error would lose which side is lying, and the
repair differs: the first is a wrong evidence entry, the second is a missing one.

Also pinned: the id shape is exactly `C-` + THREE digits. `C-1` and `C-1000` are malformed
rather than leniently accepted — the bash regex says so, and a lenient reimplementation would
silently admit ids the ledger's own tooling cannot resolve.

`almide-gates`: **~750 lines across nine modules**, four byte-identical subcommands, and
three decision rules extracted under test (parity verdict, nightly streak, link symmetry).
All three were reachable in the bash only through their I/O.

## v0.44.0 release blocked on a CI infra condition (2026-08-01)

PR #1035 is open and NOT merged. `Build (macos-latest)` and `Build (windows-latest)` fail the
`#983` tool-arming tripwire: `ALMIDE_EXPECT_TOOLS=1 but wasmtime is not runnable`, even though
that job's own `Install wasmtime` step reports success.

Not caused by the PR's diff — the same commit is green on develop (those two legs run only on
a PR to main), PR #1034 had both legs pass 40 minutes earlier on the same workflow, and this
PR's only workflow change adds a step to a DIFFERENT job (`test-rust`).

**Left red deliberately.** The tripwire exists so a leg that loses its tools turns red instead
of skipping 18 suites as pass. Merging past it would defeat exactly what it was built for, and
"green on develop" is the reasoning it was designed to reject.

Next: find why an installed `wasmtime` is not runnable in the `build` job — the gap is between
the install step and the test step's environment (PATH propagation or a cache restore), not a
missing install.

## B6 — `output-parity`, the first subcommand that RUNS things (2026-08-01)

The four subcommands ported so far all read files and print text. This one drives three
processes per fixture over the whole of `spec/`, compares three observables, retries under a
different timeout, and gates on a ratchet. It is the first port where the interesting content
is not parsing.

**Split along the line the bash cannot draw.** `parity.almd` holds the decision — six verdicts,
the trap comparison, and both stderr normalisations — as pure functions with 10 tests.
`parity_sweep.almd` holds everything that needs a process or a filesystem, with 5 more tests on
the parts that are still pure (the class report, the regression set difference, the skip count).
In the bash all of this is reachable only by running the gate, which is why nothing in it was
ever tested: exercising the "wasmtime trap frame" normaliser meant producing a wasmtime trap.

**Three things the reimplementation had to be told, and would otherwise have gotten wrong:**

- **`skip=N` is part of the summary.** The first draft filtered non-runnable files out of the
  sweep and never counted them. That reads as full coverage — "300 files agree" instead of
  "300 of the files I chose to look at agree". The count is the only thing standing between
  those two sentences, and it is now its own test.
- **The XFAIL heading is two lines.** A wrapped sentence, not two headings. Rewrapping it is a
  byte difference for no gain, so `class_report_wrapped` takes a heading LIST and the
  one-line form is the special case.
- **`find spec` runs after a `cd $ROOT`, so its paths carry no `./`.** The baseline is a list
  of exactly those strings, so a root of `"."` would produce `./spec/…` and match nothing —
  every baseline entry a regression, every file a new match. Handled explicitly rather than
  by hoping the caller passes an absolute root.

**And two that look like bugs and are load-bearing**, both carried over deliberately with the
reason written at the call site: the solo RETRY of every non-match (a load artifact can surface
as any verdict, so only the quiet re-run counts — a non-deterministic verification result is
not a result), and the RE-SORT before the baseline diff (the retry appends after the first
sort, and an unsorted tail once reported three phantom regressions).

**The stamp comes first.** `stamp.toolchain` was the first thing ported for exactly this
moment: if the PATH binary and the workspace build disagree, a parity result describes a
different compiler than the tree under test. It fired for real during this port — a rebuild had
moved `target/release/almide` out from under the installed binary, and the gate refused to
start rather than produce evidence about the wrong compiler.

### B6 result — byte-identical, and the sweep found one more thing about `sed`

Both implementations, same binary, same tree, back to back:

```
output-parity: match=383 wall=3 MISMATCH=1 RUNERR=3 XFAIL=0 v0fail=0 skip=343
  (MISMATCH = renders and runs but the stdout bytes diverge — silent miscompile class):
    ! spec/wasm_cross/env_get.almd
  (RUNERR = renders but wasmtime rejects or traps where v0 succeeds):
    r spec/wasm_cross/fs_preopen_resolve.almd
    r spec/wasm_cross/fs_relative_path.almd
    r spec/wasm_cross/host_floor_string_alloc.almd
output-parity: NEW matches not yet in baseline (run --update to ratchet):
  + spec/wasm_cross/option_tuple_payload_matrix.almd
output-parity: OK — all 382 baseline files still byte-match v0.
```

**10 lines, 607 bytes, identical, and both exit 0.** The only difference in the captured files
was the label naming which implementation produced them. (The toolchain stamp above this block
names the binary's mtime and the tree's dirty count, which move between two runs for reasons
that are not the gate — so the comparison starts at the first `output-parity:` line.)

**The first draft got 351 matches and 32 XFAILs, and the 32 were all false.** The trap fixtures
— `int_div_by_zero`, `index_bounds`, `to_fixed_domain_abort`, and 29 others — agree on stdout,
on exit code, and on stderr byte-for-byte, and the reimplementation called every one of them a
divergence:

> **A stream's final newline is a line TERMINATOR, not an empty line.**

`sed` reads it that way and `diff` compares what `sed` produced. `string.split(s, "\n")` does
not: it yields one extra empty element at the end. That element is invisible until the two
normalisers treat it differently — and they do, necessarily, because the wasmtime frame
contains a REAL blank line that must be dropped while the program's own stderr may legitimately
print one. So the phantom element survived on the oracle side and vanished on the wasm side,
and 32 identical streams compared unequal.

This is the same class as #1032 (an `awk` extractor that truncated at the first quote) and
#1031 (an unpinned locale in eleven scripts): **a shell text operation whose edge case is
invisible in the common case, reimplemented from what it looks like it does rather than from
what it does.** The port is the thing that surfaces it, because the port has to state the rule
explicitly, and the byte-diff is what refuses to let a plausible-looking restatement pass. It
is now `lines_of`, with a test that asserts the two normalisers agree on a plain one-line
stream and a second test asserting they deliberately DISAGREE on an interior blank line.

`almide-gates`: **~900 lines across ten modules**, five byte-identical subcommands, 17 tests on
the parity rules alone — a decision surface that in bash was reachable only by producing a real
wasmtime trap.

## B7 — `fuzz-track-record`, and dropping `--jq` on purpose (2026-08-01)

The streak RULE was already extracted and tested (`streak.almd`, 6 tests) — the two counters
that stop independently, which is what makes #924 measurable at all. What was left is what it
takes to feed it: two GitHub API calls per night, a verdict that depends on a nested step
conclusion, and `printf` column formatting.

**Byte-identical on the first run**: 12 lines, 759 bytes, matching
`scripts/fuzz-track-record.sh 8` exactly, including the current state — full-budget 4/14, green
0/2.

**The bash reads the API through `gh api --jq`; this port parses the JSON in Almide.** That is
a deliberate divergence in METHOD with no divergence in OUTPUT, and the reason is the pattern
the last three ports established: `--jq` is a second language living in a shell string,
invisible to the type checker, and untestable without the network. It is the same shape as the
`awk` that truncated a title at the first quote (#1032) and the `sed` whose empty-line rule
silently disagreed with its own terminator (B6, above) — expressions that look obviously
correct and are load-bearing in an edge case nobody runs. Parsing here puts the response shape
in front of the compiler: `get_array("jobs") |> find(name == …) |> get_array("steps")` is a
chain the checker walks, and a wrong key is a `none` with a named fallback rather than an empty
jq result that silently scores a night as TRUNCATED.

Three rules got tests they could not have had in bash, all of them about formatting that is
invisible until it is wrong:

- **The last printf column is not padded.** `%-12s %-13s %-11s %s` — the final `%s` has no
  width. Padding it would put trailing spaces on every row of every report.
- **`%-12s` is a MINIMUM.** An over-wide run id is printed in full, not truncated. A port that
  "formats to 12 columns" would corrupt exactly the rows that matter.
- **An in-flight run is PRINTED but not SCORED.** Dropping it shifts the streak window by a
  night; scoring it counts a night that has not happened. Both are wrong in a way that shows up
  as a plausible number.

`almide-gates`: **~1,050 lines across eleven modules**, six byte-identical subcommands, 28
tests. Remaining: `check-contracts` (426 lines — the one that composes everything).

## Where the port stands, and what `check-contracts` needs (2026-08-01)

Six of seven subcommands are done and byte-identical. The last one is
`scripts/check-contracts.sh` — 426 lines, and unlike the others it is not one transformation
but **ten independent checks over one parse**, each emitting its own `::error::` lines:

| # | check | state |
|---|---|---|
| (e) | id shape `C-` + THREE digits, uniqueness, status enum, `doc=` file exists | rule pinned in `contract_audit.almd` |
| (a) | every evidence `path` exists; class in the shared vocabulary; `fuzz` requires `n>=1` | — |
| — | named-unit grep for `*.rs` / `*.lean` / `*.toml` and for fuzz/lean/exhaustive | — |
| (b) | every ACTIVE contract carries evidence of class >= `fixture` | — |
| (c)(d) | the two edge sets — ledger→fixture and `// @contract:`→contract — must be IDENTICAL | **done** in `contract_audit.almd`, both asymmetries reported separately |
| (j) | cited source paths must not name a retired subsystem | — |
| (f) | ids contiguous `C-001..C-NNN`, no gaps | — |
| (f) | flagged-for-revision count is a down-only ratchet | — |
| — | spec-keying: every contract names an ALS section, every section resolves | — |
| — | spec-COVERAGE: every normative section is cited by >=1 contract | — |
| (g)(h)(i) | freshness of the README claims block, the contract index, the conformance report | — |

**Why it is last and not first**: it composes what the other six built. The TOML reader
(`toml/mod.almd`), the class vocabulary (`contracts.almd`), the link symmetry
(`contract_audit.almd`), and the two generators whose freshness it checks are all already
byte-identical, so the remaining work is the checks themselves plus the `::error::` emission
order — which IS the byte-identity surface, since a gate's output is a list of errors in a
fixed sequence.

**The acceptance check stays the same and is available now**: `bash scripts/check-contracts.sh`
currently prints an OK block over 201 contracts and 330 fixtures, so every increment can be
diffed against a real, non-trivial output rather than a constructed one. Port order that keeps
that property: the PARSER first (it feeds everything), then the checks in the order the bash
emits them, diffing after each.

**One caution recorded from B6**: the checks are independent but their OUTPUT is not — an
error emitted in a different order is a byte difference even when the finding is identical. So
the port must preserve the sequence, not merely the set.

### The acceptance check for this one is MUTATION, not byte-identity alone

A clean ledger makes most of these checks print NOTHING. So a port that implements two of the
ten and skips the rest would still produce a byte-identical clean run — and the byte-match
would be a lie, because the checks that emitted nothing were never run. Byte-identity is a
necessary condition here and nowhere near sufficient.

The bash already carries the right criterion, in its own closing comment: **every check flips
green→red on a one-line edit**, and it enumerates the twelve edits. That list is the port's
acceptance suite — for each mutation, the Almide gate must go red with the SAME `::error::`
line as the bash:

| # | one-line edit | must fire |
|---|---|---|
| 1 | delete a fixture path from a contract's evidence | (d) only_rev |
| 2 | remove a `// @contract:` line from a fixture | (c) "no header" |
| 3 | downgrade an active contract's only evidence to `by-construction` | (b) |
| 4 | typo a class | (e) bad-class |
| 5 | flag any contract | (f) ratchet |
| 6 | renumber a contract to leave a gap | (f) coverage |
| 7 | hand-edit a number inside README's claims markers | (g) stale-claims |
| 8 | add a contract without regenerating the index | (h) stale-index |
| 9 | cite a new section without regenerating conformance | (i) stale-report |
| 10 | delete a `since = ` line | (e) missing-required |
| 11 | point a fixture header at a retired subsystem | (j) dead-path |
| 12 | an UNALIGNED bogus spec key (`spec = "ALS-BOGUS"`, single space) | spec-existence |

Mutation 12 is the one worth reading twice: the check used to grep for a six-space-aligned
`spec      = "..."`, so a key written with different spacing was silently DROPPED — it passed
the presence check and skipped resolution entirely (#989). That is the same failure shape as
B6's phantom trailing line and #1032's truncating `awk`: a text pattern that is right about the
input it was written against and silent about the input it was not.

**The parser is already there.** `toml/mod.almd` yields `Table { scalars, array_items }`, which
is exactly what `parse_ledger`'s TAB-record protocol reconstructs by hand — the bash needs the
protocol because awk cannot return a structure. So the port skips `parse_ledger` entirely and
builds the ten checks on the tables. That is also why this subcommand was left for last rather
than being the hardest thing attempted first.

### B8 (in progress) — the schema half, on the tables the earlier ports built

`ledger_schema.almd` implements checks **(e)**, **(a)** and **(b)** as pure functions over
`toml.Table`, with 7 tests. It runs on WASM with no native fallback, which the process-driving
modules cannot — the schema rules touch nothing but data.

The `parse_ledger` TAB-record protocol is simply gone. The bash needs it because awk cannot
return a structure; `toml.parse_tables` returns one, so the checks read fields instead of
re-splitting a line format. That is the compounding return on doing this subcommand last.

Three rules got tests that state something the bash only implies:

- **The class RANK comes from the shared file's line order**, not a list written here. Two
  gates read `scripts/lib/contract-classes.txt` so their enums provably cannot drift; a rank
  hard-coded in the port would reintroduce exactly the divergence the file prevents.
- **The active-evidence floor exempts `flagged-for-revision` and nothing else.** Being flagged
  is the honest way to say a claim currently rests on prose — and the flagged COUNT is itself a
  down-only ratchet, so the exemption cannot be used to park a claim indefinitely.
- **The error list is ordered, not a set.** A gate's output IS its `::error::` lines, so the
  test asserts the five schema errors come out in the bash's emission sequence. Two
  implementations that find the same problems in a different order are not byte-identical.

`ledger_coverage.almd` adds **(j)** and both halves of **(f)** — the rules that read the ledger
as a whole rather than one contract at a time — with 6 more tests, also WASM-clean. Verified
against the real ledger in one run: 201 contracts, max id C-201, **0 gaps, 0 flagged, 0 bad
ids, 0 bad or missing `since`** — the same numbers the bash reports.

Each of the three carries the violation that caused it, because a gate whose reason is lost
gets weakened by the next person who trips it:

- **(j) fires on a dead PARENT DIRECTORY, never a dead file.** Deliberately narrow. Statements
  legitimately name illustrative files (`fs.stat("spec/x.almd")`), so flagging a missing file
  would make the check useless within a week; a vanished DIRECTORY is the retired-subsystem
  signature. When the v0 wasm emitter went and 115 files under `emit_wasm/` with it, 16
  citations rotted in place and kept sending readers to code that no longer existed (#941).
  A single deleted file inside a surviving directory is the accepted blind spot — the price of
  zero false positives, and evidence paths are checked exactly.
- **(f) contiguity**: a gap means a contract was DELETED rather than superseded. Retiring a
  promise is a real operation — flip its status, or replace it and say so — but silently
  vacating a number leaves every reference dangling with nothing to notice.
- **(f) ratchet**: the ceiling is ZERO. C-033 converged; C-006 was retired by removing
  `fan.timeout` in 0.29.0. Raising the ceiling to admit a new divergence is precisely the move
  this check exists to make impossible without saying so out loud.

`ledger_speckey.almd` adds both spec-keying directions and the freshness messages, with 6 more
tests. Verified against the real ledger and `docs/specs/als/` in one run: **64 distinct spec
keys, 64 normative sections, 0 contracts without a key, 0 unresolved keys, 0 orphan sections**
— the same numbers the bash prints.

The REVERSE direction is the one worth keeping in view. Forward — every contract names a
section that exists — is bookkeeping. Reverse — every normative section is cited by at least
one contract — is not: an uncited section is a claim the spec makes that no executable evidence
certifies, and its first run found ALS-T4 adjudicating `chunk/windows(n <= 0)` while BOTH
targets disagreed with it (native raising a raw panic, wasm silently returning `len+1` empty
windows). Nothing else in the project would have caught it, because every test agreed with the
implementation and nothing read the section.

Two more rules got tests that pin an edge the bash expresses only in a regex:

- **A section heading matches on a BOUNDARY.** `grep -qE "^## $sec( |$)"` — without the
  trailing alternation, `ALS-T1` resolves against `## ALS-T14 …` and a bogus key passes
  validation by prefix.
- **Keys are counted DISTINCT and byte-sorted.** `sort -u` under a pinned `LC_ALL=C`: counting
  duplicates inflates the summary, and an unpinned collation reorders the error lines between
  machines, which is #1031 exactly.

Remaining for B8: wiring the three checks into one `check-contracts` subcommand with the
bash's emission order and its summary/histogram block, then running the twelve mutations —
each must turn the Almide gate red with the same line as the original.

**One latent bug to reproduce rather than fix, and to name where it is reproduced**: the
parser unquotes with `sub(/".*$/,"",v)` — truncate at the FIRST quote — which is precisely the
bug #1032 fixed in `generate-readme.sh` (the fixed version anchors at `"[ \t]*$`). It is
latent here only because the fields it reads (`id`, `status`, `doc`, `since`, `path`, `class`,
`name`) happen never to contain an escaped quote. `title` and `statement` — the two that DO —
are read as presence flags, so the truncation never reaches them. The port should note it at
the call site instead of quietly hardening it, so the day a `doc =` path grows a quote, the
divergence is a known one.

## The dogfood found a bug in the dogfood (2026-08-01)

Writing `ledger_schema.almd`'s `rank_of` — which reads the evidence-class vocabulary from
`scripts/lib/contract-classes.txt` — put it side by side with `contracts.almd`, which had the
six names hard-coded in a `match`. Two rank implementations, in the same program, for the enum
whose entire reason for living in a file is that **two gates must not be able to disagree about
it**.

The hard-coding was deliberate and argued, in a comment: a reader for a one-column list is more
machinery than six stable strings are worth, and a mismatch shows up immediately as a wrong
"Strongest Evidence" column. Both halves of that are true and the conclusion is still wrong.
The file is not an implementation detail of the bash — it is the mechanism by which the ledger
gate and the rt-oracle-registry gate provably share one vocabulary. A third copy inside the
tool that CHECKS them reintroduces exactly the drift the file was created to prevent, and it
does so invisibly, because it is byte-identical today. That is how such a copy survives long
enough to matter.

Fixed by threading the class list in as a PARAMETER — the caller reads the file once, and
`class_rank` / `strongest` / `rows` stay pure and testable without a filesystem. The parser
strips comments and blanks exactly as `grep -vE '^[[:space:]]*(#|$)'` does, with a test saying
why: the line order AFTER stripping is the rank, so a comment counted as a class silently
demotes `fixture` — which is the FLOOR an active contract must reach.

`docs/contracts/README.md` re-verified after the change: **232 lines, 25,450 bytes, still
byte-identical** to the bash original.

The general shape, now three for three across this Unit: **a rule that is duplicated because
duplication is cheaper than the abstraction is a rule that will eventually be two rules.** The
locale pinning (#1031) was eleven copies of one `export LC_ALL=C`; the quote-truncation
(#1032) was five copies of one unquoting expression; this is two copies of one enum. In each
case the copies agreed on the day they were written.
