# VERDICT — the greenfield wasm backend against the reference corpus

2026-08-25. The formal quality determination the greenfield arc was chartered
to produce: the backend judged criterion-by-criterion against the ratified
comparison basis `../almide-references` (nine compiler clones at pinned SHAs +
the RESEARCH survey series), and against the incumbent release v0.59.1.

The bar is the one RESEARCH-championship.md establishes: *"the bar is not
features … a narrow, enforced set of engineering properties, and that is where
the contest is."* Every claim below is measured, with the measurement named.

Scope: `crates/almide-wasm` (the emitter), `crates/almide-spine` (the front
pipeline), `crates/almide-wasm-run` (the product host runner). The reference
cohort for backend design is the three non-LLVM wasm backends in the corpus —
zig (direct emit), roc (direct emit, Zig rewrite), grain (Binaryen delegation)
— codified as the W-canon in RESEARCH-wasm-backends.md.

## 1. The W-canon, position by position

| canon | reference doctrine | greenfield position |
|---|---|---|
| W-1 fn values | i32 table index, +1 bias, address-taken-only registration (zig) | **Built as specified**: `FnTable` (lib.rs), +1-biased funcref slots (work.rs), first-class-only registration, `call_indirect` through the type table (calls.rs) |
| W-2 closures | closure blocks, self-recursion outside the captures (grain) | **Built**: existing pack_fields layout; literal lambdas inline at call sites (the general form of grain's count inference, noted in the canon itself) |
| W-3 safety checks | before the emitter, bare `unreachable` (zig) | **Deliberate divergence, documented in the canon**: almide has a stderr contract on abort, so guards are message-bearing and stay emitter-owned (C-002). Independently arrived at before the survey |
| W-4 local discipline | typed freelists + refusal on exhaustion (zig); bulk pre-allocation (roc) | **Both**: typed hold pools with hard caps that refuse — never corrupt — on exhaustion (emitter.rs `HOLD_*_POOL`); bulk local collection (collect.rs, the roc Storage shape) |
| W-5 aggregates | linear memory; SP-global frames; roc's dead-frame copy window is a standing hazard | **Linear memory + bump-heap returns — the dead-frame hazard class is structurally absent** (no frames) |
| W-6 control flow | neither zig nor roc implements `return_call` | **Ahead of the cohort**: `return_call` and `return_call_indirect` tail calls (tco.rs, calls.rs; C-292) — the canon records this as "ahead of all three" |
| W-7 abort | message via host, then trap; host separates semantic crash from engine fault (roc) | **Built**: eprintln + exit import + `unreachable`; the host records exit as a normal outcome, errors mean engine/compiler fault (host.rs); OOM is the defined "Error: out of memory" + exit 1 (C-197), never a raw trap — corpus-pinned by `count_domain_nonbytes` |
| W-8 RC runtime | grain's post-header RC, callee-owned args | **Deliberately not adopted**: bump + deep-copy is run-to-completion sound; the OOM boundary is defined and fixture-pinned. Recorded as the decision, not a gap |
| W-9 test mechanisms | roc's differential runner; zig's lesson that encoding bugs evade behavior tests | **Burn-up manifest: 599 byte-exact stdout+exit goldens, divergence zero, grow-only floor; 42-patch semantic mutation gate (none of the three references has one); `wasmparser::validate` walls every instantiation; linked programs routinely carry 400+ functions, so LEB index boundaries are exercised by the corpus itself** |

Verdict on the canon: every position is met, exceeded, or diverged from by a
documented contract-forced decision. No position is unmet.

## 2. The championship axes a backend can contest

- **Verified envelope (axis B4)** — grain ships whatever Binaryen emits; LLVM
  backends ship what LLVM emits. Greenfield ships exactly the bytes it emitted,
  validated on every run. The axis the references structurally cannot claim is
  retained by construction.
- **Conformance discipline** — 599/599 fixtures, byte-exact normalized stdout +
  exit code, divergence zero, enforced by a grow-only `SUPPORTED_FLOOR` in CI
  (`backend_parity.rs`). Roc's differential runner is the only comparable
  mechanism in the nine; it has no golden floor ratchet.
- **Mutation gate** — 42 semantic-mutation patches, refresh-never-retire, run
  pre-push and in CI. Zero of the reference wasm backends carry one.
- **Performance, measured not asserted** — probe with a measured empty-program
  baseline (perf_probe.rs), 7 kernels. Against incumbent v0.59.1 wasm:
  every kernel at or ahead — scalar kernels 2–4 ms ahead (int_loop 80 vs 82,
  float_math 24 vs 28, str_build 55 vs 59, recursion 80 vs 82), list kernels
  2× (list_sort 7 vs 17, list_pipeline 5 vs 10). Emit is 14–17 ms per kernel.
- **Module size, corpus-wide** — all 599 fixtures emitted by both compilers
  (2026-08-25): greenfield smaller on **450 of 599**, median ratio **0.675**,
  aggregate **4.11 MB vs 10.54 MB (2.6× smaller)**. The 149 losses are
  near-empty programs where the ~3.1 KB fixed runtime preamble dominates
  (worst case 3.1 KB vs 1.1 KB); on string/unicode-bearing programs the ratio
  reaches 0.10. Fixed cost ~2 KB higher, marginal cost decisively lower,
  crossover below 4 KB.
- **Asymptotic honesty** — the reference survey's only asymptotic defect in
  the whole cohort (C2: incumbent `list.sort` O(n²) on wasm) is closed and
  gated: `list.sort` and `list.sort_by` are both bottom-up merge sorts
  (list_sort.rs), measured at 200k elements in 33 ms end-to-end, with kernel
  rows in the probe so the class is measured, not assumed.
- **Complexity budget** — codopsy A(90) on a 23.2 K LOC self-contained crate,
  vs the incumbent render stack at B(89) across ~103.6 K LOC. Gated pre-push.

## 3. The incumbent A/B (v0.59.1) — the non-carryover proof

The charter forbids carrying the incumbent's deficits forward. Measured state:

| dimension | incumbent 0.59.1 (wasm leg) | greenfield |
|---|---|---|
| conformance (599 manifest) | 599/599 (48 s sweep, ~70 ms/fixture median) | 599/599 (13.6 s in-process) |
| scalar perf kernels | 1× | **1.03–1.17×** (ahead on all four) |
| list perf kernels | 1× | **2×** |
| module size, corpus aggregate | 10.54 MB | **4.11 MB** |
| sort asymptotics | O(n²) family (survey C2) | **O(n log n), measured** |
| host runner stdin | read-to-end | lazy read-to-end: programs that skip stdin never block on an open terminal |
| admission provenance | accreted | **PORT-MATRIX ledger: 435 linked / 9 rejected with recorded reasons / 792 native-covered or honest walls** |

Every deficit surfaced during this arc was closed in the same arc, none
deferred: module-size bloat → two-pass DCE (24.7 KB → 3.1 KB class);
no product host → `almide-wasm-run` (one host serves the CLI and the test
harness); `sort_by` insertion sort → lockstep merge sort; runner eager-stdin
block → lazy `StdinSource`. NON-CARRYOVER.md holds the closed incumbent-defect
ledger; nothing on it survives in the greenfield backend.

## 4. The determination

Within the ratified comparison field — the nine reference compilers of
`../almide-references` at their pinned SHAs, and the incumbent v0.59.1 — as of
2026-08-25:

**No wasm backend in the field combines a verified-envelope shipped artifact,
a byte-exact divergence-zero conformance corpus with a grow-only floor, a
semantic mutation gate, measured perf and size records, and tail-call support.
The greenfield backend has all of them, meets or exceeds every W-canon
position, and carries zero known incumbent defects. On the axes the
championship survey establishes as the contest — enforced engineering
properties — the greenfield wasm backend holds the top position in the
surveyed 2026 field.**

The claim is scoped to what was measured: the surveyed field, the backend
axes, this date. It is not a claim about unsurveyed compilers or unmeasured
dimensions — the survey series (Survey 6) is the ratified basis for "the
field", and every row above names its evidence. Anything that later widens
the field or the axes re-opens the determination through the same mechanism
that produced it: measure, gate, then claim.
