# CI critical path: measured baseline, peer survey and the ranked plan (2026-09-21)

Research note behind #2381 (PRs #2440, #2444 and their follow-ups). Read-only survey of ../almide-references and a job-by-job inventory of ci.yml; every saving is arithmetic from the measured job timings named in section 0. The measured result of each landed step is appended below section 4 as it arrives.


Read-only research. Baseline = branch `ci-2381-solo-giants` (worktree
`.claude/worktrees/ci-2381-critical-path`), which carries PR #2440 (ledger out of
the serial nextest group, family spread one per shard) and PR #2444 (five giant
targets as six `Test Rust (solo …)` jobs). Every number below is either from the
GitHub Jobs API for a named run, from a file in that worktree, or an explicit
estimate marked as such.

## 0. Measured baseline (what the plan is arithmetic against)

Develop run 35569545602 (last green develop run before tonight, 66 min end-to-end,
06:41:34 → 07:46:59), per job (started → completed):

| job | wall | notes (step timings ≥ 20 s) |
|---|---|---|
| Test Rust (shard 0/4) | 60.0 min | `Cargo tests` 3531 s = interp_abstain_ledger 1666 s then interp_bridge_fallback_ledger 1586 s, serialized |
| Test Rust (shard 2/4) | 41.1 min | interp_oracle 1223 + opt_parity 1075 serialized (family group) |
| Test Rust (shard 3/4) | 27.9 min | |
| Commissioned wasm gates | 27.8 min | carried gates `cargo test --release` 679 s → release-only root tests 436 s → double-free sweep 76 s → target-availability 426 s → ratio 25 s (1642 s of 1668 s, strictly sequential) |
| Test Rust (shard 1/4) | 25.6 min | |
| Almide gates | 15.4 min | reachability ratchet 485 s, parity goldens 140 s, spec tests 135 s, WAT audit 50 s, rest < 30 s each |
| WASM browser-ABI determinism | 11.4 min | one 654 s step, compile-bound (wasm-pack build of a harness crate, `actions/cache` keyed on the emitter sources) |
| WASM host-arch determinism | 8.6 min | one 450 s step, same shape (native + wasm32-wasip1 harness builds) |
| Emit & Format | 7.6 min | pass-order shuffle 183 s, stability closure 93 s, ~40 sub-minute gates |
| Commissioned aviation quality | 7.4 min | codopsy build (cached) + score |
| Test WASM (Windows host) | 6.0 min | |
| Perf ratchet | 6.0 min | |
| Build (Linux) | 5.4 min | cargo build 138 s, nextest archive 86 s, rustc ratchet 27 s, clippy ratchet 37 s, + checkout/toolchain/cache/upload |
| Test shards cover every target | 2.4 min | |
| Lean / Test WASM / Test ARM / ratchet-separation / coverage / changes | ≤ 0.7 min each | |

PR #2440 run 35581361256: the run was created 09:05:52 and `Change class` (the
first job) started 09:34:44 — **29 min of runner-capacity wait** before any work;
`build` finished 09:40:51 and the shards were picked up 09:42:38 (another ~2 min of
queue latency per `needs` hop under load). Shards ran 33–44 min with every giant
1.4–1.9× slower than alone (contention).

Giant solo times (the numbers the plan uses): interp_abstain_ledger 1666 s,
interp_bridge_fallback_ledger 1586 s, wasm_cross_target_spec 1377 s, run_parity
1310 s, interp_cross_target_spec 1223 s, wasm_opt_parity_spec 1075 s. Everything
else ≈ 6.5 ks summed (`scripts/ci-test-weights.txt` sums to 15,964 s including the
giants; the next-largest target, `diagnostic_harness_test`, is 1501 s and is a
7-test fixture loop — see §2.5).

**Expected floor after #2444 (not yet measured — no run exists on the branch):**
build 5.5 + solo setup ~2 + 1666 s (27.8 min) ≈ **35–36 min**, with Commissioned
wasm gates finishing at 5.5 + 27.8 ≈ 33 min right behind it. Note the ledger's 1666 s
was measured on a contended shard; its true solo time is unknown until #2444 runs
(the interp leg is the same function the oracle computes inside its 1223 s total,
so the ledger alone should be well under 1666 s in isolation).

Required checks on develop today (API): Build (Linux), Test WASM, Test ARM,
Emit & Format, WASM host-arch determinism, Lean Proofs, Test Rust (shard 0–3/4),
Test shards cover every target, Almide gates, WASM browser-ABI determinism, and the
acceptance-ring names. **Not required: Commissioned wasm gates, Commissioned
aviation quality, Perf ratchet, Test WASM (Windows host), Coverage ratchet,
Ratchet-commit separation, Test Rust (solo …)** (the solo legs are covered only via
the coverage job's proxy step). Merge queue ruleset on develop: `REBASE`,
`ALLGREEN`, max 5 entries to build/merge, check-response timeout 120 min. Develop
branch protection has no PR-review requirement, no push restrictions, admins not
enforced — a direct push to develop is technically possible.

## 1. Peer survey (what the reference repos actually do)

Sources: `.github/workflows/*.yml` of every repo under `almide-references/`.
`RESEARCH-neighbours.md` has no CI content beyond a one-row topology comparison with
wado (line 60). swift and moonbit-compiler carry no CI config; zig's CI is
`.forgejo/workflows/ci.yaml`.

| repo | lang / suite | runner sizes | caching | test parallelism | PR vs merge/nightly | merge_group | durations / timeouts |
|---|---|---|---|---|---|---|---|
| **aver** | Rust; 15 integration targets ≈ 2,066 s measured; 33 cert lanes | ubuntu-latest ×11 + 1 windows | rust-cache every job; `actions/cache` for binaryen/wasm-tools/elan/prelude; no archive handoff | LPT bin-packing on measured seconds (`tools/ci_test_shard.py i 4`) then **`cargo nextest --partition slice:i/4` inside the two split targets**; cert lanes = `-E 'test(/…/)' --partition slice:` from JSON, with a `decide` job proving partitions are disjoint and exhaustive | PR = ci + 5 smoke cert lanes (~5 min); full 33 lanes every 6 h + release/**; fuzz/rust-codegen nightly-only; proptest cases 256 on PR vs 2000 nightly | none | "330 minutes of kernel work across 30 lanes … each PR queued behind ~40 min" → moved off PR; ci.yml has no timeouts |
| **vera** | Python, 12,290 pytest tests | ubuntu + macos-15/26 + windows + arm (13 cells) | none | pytest-xdist `-n auto` | stress suite nightly + path-filtered PRs; cancel-in-progress on PRs only | none | suite 3–4 min; coverage cell 8 → 3 min |
| **jacquard** | OCaml/dune | ubuntu-latest | none | `cc: [clang, gcc]` | all on PR + main; main pushes never cancelled | none | 60/90/30/240-min timeouts documented in `docs/ci-cd.md` |
| **hexa** | self-hosted language, ~32 bash gate workflows | hosted mix + **self-hosted pool** with hosted fallback action | ccache keyed on source hashes | fan-out across workflows; xargs with per-file cap | tight `paths:` filters on PRs, unfiltered on main; cancel PR only | none (aggregator `selfhost-gates-summary`) | darwin self-hosted queue **median 168.8 min (p90 230.7) for a 2.7-min slot** |
| **sui** | Python | 3×3 matrix | none | none | PR + main | none | none |
| **vibe** | selfhost compiler, 495-file unit battery | ubuntu-latest ×29 | ~25 `actions/cache` keys; **compiler built once, shipped as a 16 MB artifact to 14 jobs** | **8 shards, LPT-weighted (`scripts/unit_test_weights.tsv`), `VIBE_UNIT_TEST_SHARD=i/8`** | coverage main-only; no concurrency block | none (aggregator `ci-required`) | budget "CI must stay under 5 minutes"; cold battery 5m20s → 4m11s; "build once cut job time 7879 → 4971 s but lost wall time"; median job start 365 s vs 3 s |
| **wado** | Rust, 4,671 fixtures | ubuntu-latest ×20 | rust-cache `shared-key: ci, save-if: false`; cache **published by a separate `cargo-cache.yml`** on lockfile changes | `level: [O0..Os]` matrix + 3 sibling jobs; EMI nightly `WADO_EMI_SHARD=i/8` | `changes` docs-only skip job (not `paths-ignore`, to avoid a stuck required check); EMI nightly only | none (aggregator `test`, `if: always()`) | `timeout-minutes: 60` on every job |
| **koka** | Haskell, ~718 `.kk` cases | 5-OS matrix | stack-action, vcpkg | none | same on PR/dev; cancel-in-progress | none | none |
| **lean4** | C++/Lean, ~4.4k test files via ctest | ubuntu-latest + **self-hosted `chonk` with `runner-fallback-action` → nscloud 8×16 / 32×64 / macOS 6×14 / Windows 4×16** | ccache + stage1 olean cache keyed on sha; external Lake cache | ctest `-j$NPROC`; regex `-E/-R` subsetting; primary vs `matrix-secondary` | **label-driven check-level**: PR = Linux only; macOS/Windows/sanitizers only on merge_group / nightly / tags | **yes** (aggregator `all-done`) | none stated; 20-min step timeouts on cache network |
| **roc** | Zig, custom MiniCI | hosted; benchmarks self-hosted; arm shards disabled (network drops) | only the Zig deps cache works (`.zig-cache` > 2 GiB limit); **`minici-build.tar` built once, consumed by 7 shards** | **range sharding by named boundaries** (`--minici-from/-to/-after/-before`) | **PR = minici per OS; full suite nightly**, which fast-forwards a `nightly` branch on green | none (aggregator `finish`) | shard timeouts 120/180 |
| **rust** | Rust+LLVM | ubuntu 4c free; **hosted 4/8/16-core**, arm 8-core, windows 8-core; **AWS CodeBuild 8c/36c, EC2 c8a.8xlarge** | sccache → S3; ghcr Docker cache | ~90-job matrix; hand-maintained numbered shards (`x86_64-gnu-llvm-22-{1,2,3}`) | **12 PR jobs (~40 min) vs ~90 auto jobs (~2 h) via bors** | bors (not merge_group) | PR ~40 min, auto ~2 h, ~10 merges/day, rollups |
| **gleam** | Rust | hosted | rust-cache `key: v1-<target>`; binary artifact → e2e job | 7-way OS matrix | same on PR/main; `paths-ignore` docs | none | 30/10 |
| **grain** | OCaml | hosted | esy cache with a `lookup-only` warming job ("so one cache build does not block every target") | 3 OS × {native, js} | same on PR/main/queue | **yes** | none |
| **wasmi** | Rust | hosted | rust-cache; nightly pinned to keep cache warm | nextest only under Miri | everything on every PR; Miri spec nightly | none | none |
| **spacewasm** | Rust | hosted incl. arm | rust-cache **`save-if: main`** | matrix | Miri integration daily only | none | none |
| **plumbum / xonsh / zod / sh** | Python / TS | hosted | uv cache / none | matrix | same on PR/main | none | none; xonsh per-test `--timeout=600` |
| **zig** | Zig | **100 % self-hosted** exotic targets | deliberately none | 16 parallel jobs | riscv64 push-only; cancel PR only | none | 120–600-min timeouts |

Cross-cutting facts that matter for almide:

- **nextest `--partition` is used by exactly one peer (aver)**, and only where a
  single test *case* dominates — the same shape as almide's giants. Nobody uses
  `test-groups`, `slow-timeout` or archives except almide itself.
- **"Build once, ship an artifact" is the common pattern** (vibe, roc, gleam, grain,
  almide's `almide-linux` + nextest archive). vibe measured the trap: it cut job
  time 37 % and *raised* wall time because each consumer paid an artifact download
  plus a `needs` hop under runner-queue latency (median job start 365 s).
- **Tiered gating is the main lever peers with long suites actually pulled**:
  aver (cert 330 min → 5-lane smoke on PR), lean4 (check-level 0 on PR), roc
  (minici on PR, full suite nightly), rust (12 PR jobs vs 90 auto). Every one of
  them accepts that a class of failure is found only post-PR (queue/nightly/bors).
  Peers whose suites are 2–5 min (vera, vibe, zod, sui) run everything everywhere;
  their approaches do not transfer.
- **Larger/self-hosted runners** are used by rust and lean4 only, both with a
  hosted fallback; hexa's self-hosted darwin pool is a cautionary tale (168-min
  median queue for a 2.7-min job).
- **Aggregator required job** (vibe `ci-required`, wado `test`, hexa
  `selfhost-gates-summary`, roc `finish`, lean4 `all-done`) is the near-universal
  answer to "matrix rename silently un-requires a check".
- **Job timeouts on every job** (wado 60, jacquard documented per job, roc per
  shard) — almide's ci.yml has `timeout-minutes` only on Windows/cross/coverage
  jobs; a hung giant costs the 360-min default plus a runner slot.
- No peer documents its CI durations except rust (PR ~40 min, auto ~2 h) and vibe
  (5-min budget). rust's numbers show a 40-min PR lane is considered normal for a
  compiler; vibe's show what a 495-test compile-per-test battery costs when every
  fixture compile is 1–6 s — comparable per-fixture cost to almide's corpus.

## 2. almide's ci.yml — job inventory, duplicates, fan-out candidates, shardability

### 2.1 Jobs (name → needs → what it does)

| job | needs | work |
|---|---|---|
| changes | — | docs-only classification (0.1 min) |
| ratchet-separation | — | per-commit ratchet/impl separation (0.2) |
| build | — | `cargo build --release`, nextest archive (`ci-test` profile), rustc + clippy ratchets, uploads `almide-linux` + `almide-tests-archive` |
| shard-coverage | changes, build, test-rust-solo | proves shards ∪ solos == all targets; ledger legs partition their binary; proxy "every solo leg passed" |
| test-rust ×4 | changes, build | replay archive over this shard's binary ids (packer in `scripts/ci-test-shard.sh`, weights file) |
| test-rust-solo ×6 | changes, build | one giant each, same setup (toolchain, nextest, archive, wasmtime v47, apt binaryen) |
| almide-gates | changes, build | registry regen (`cargo build -p almide-codegen`), parity goldens, `almide test spec/ --target rust`, examples, fallback ratchet, wall corpus, reachability ratchet (485 s), WAT audit (`cargo build --example`), dogfood, README block, frees churn |
| test-arm | changes | macos-14 NEON test (0.3) |
| test-wasm | changes, build | `almide test spec/ --target wasm` (0.5) |
| test-wasm-windows | changes | Windows parity subset (6) |
| wasm-host-determinism | changes | `cargo check` wasm32 lib + harness builds native and wasm32-wasip1 + fixture loop (8.6) |
| wasm-browser-determinism | changes | wasm-pack harness + node fixture loop (11.4) |
| lean-proofs | — | sorry/axiom greps, 3× `lake build`, conformance regen (0.7, cached) |
| coverage-ratchet | changes, build | relevance-skipped unless crates/src/runtime/spec/stdlib changed; otherwise ~14 min instrumented rebuild |
| checks (Emit & Format) | build | ~45 shell gates over the binary and the repo (7.6) |
| perf-ratchet | changes, build | valgrind + wasmtime; three ratio gates (6) |
| build-cross / corpus-cross / test-rust-cross | PRs to main only | macOS/Windows |
| commissioned-wasm-gates | changes, build | two `cargo test --release` compiles + three script gates, sequential (27.8) |
| commissioned-aviation-quality | changes | codopsy A + 800-line cap (7.4) |
| commissioned-mutation-gate | changes | PR only (0.5) |
| trigger-playground | main push only | |

Runner-slot count per full run: 4 shards + 6 solos + 15 other jobs ≈ **25 jobs**.
The queue's `max_entries_to_build: 5` means up to 5 concurrent queue runs plus PR
runs plus the develop push run, i.e. 150+ job slots against a standard-runner
concurrency cap (20 for a Free org, 60 Team — the org plan was not readable with
this token). That is the 29–37-min wait, not any single job.

### 2.2 Work done more than once

| duplicated work | where | cost |
|---|---|---|
| **Every queue merge also fires a full `push` run on develop** (run 4a3d9dd4b at 10:06 today is one) | `on: push: branches: [main, develop]` | one whole 25-job run per merged PR — the single largest source of runner-slot pressure. The queue's `merge_group` run tested that exact tree (REBASE, group tree == develop tip when the group merges). Caveat in §4. |
| **The corpus table is built 3× per run**: `corpus.rs::build_corpus` builds native + wasm + wasm-opt + interp legs for all 722 fixtures **unconditionally**, and each of `wasm_runtime_cross_target`, `_opt_parity`, `_interp_oracle` builds its own copy. The opt-parity gate never reads native or interp; the cross-target gate never reads wasm-opt or interp; the oracle never reads wasm-opt. | `tests/wasm_runtime_test_parts/corpus.rs:51-125` | ~3 × 1100–1400 s of runner time, of which roughly half is legs the reading gate discards (per-leg split not measured; the native leg is a rustc compile per fixture and is the expensive one) |
| **The interp leg is computed 3× more**: both ledger tests (`interp_abstain_ledger`, `interp_bridge_fallback_ledger`) call `run_interp_capture(_with_fallbacks)` over the whole corpus, and the oracle computes the same leg inside its table | `tests/wasm_runtime_interp_ledger.rs:76-84, 273-284` | 1666 + 1586 s for two consumers of one sweep |
| wasmtime install | 14 jobs (v47.0.3 cached tarball) + 2 jobs on **a different pin, v27.0.0** (almide-gates, commissioned) | ~10 s each; the pin split is a correctness smell more than a cost |
| `apt-get update && apt-get install binaryen` | 4 shards + 6 solos + almide-gates + checks = 12 jobs | ~30 s each, ~6 runner-min/run, and `apt-get update` is the flaky half |
| `cargo-nextest` install from `get.nexte.st/latest` (**unpinned**) | build, 4 shards, 6 solos, shard-coverage = 12 jobs | ~5 s each; unpinned means a nextest release mid-run can mismatch the archive format between `build` and a consumer |
| `almide-tests-archive` download | 11 jobs | ~35 s each |
| rust toolchain install | 12 jobs (needed only for the `cargo nextest` shim in archive consumers) | ~20 s each |
| release-profile workspace compile | commissioned job compiles 6 crates' tests (679 s incl. run) then root's 3 release-only tests (436 s incl. run); coverage-ratchet compiles instrumented; determinism jobs compile harness crates (cache keyed on emitter hash → miss on every MIR PR) | the commissioned compiles are the only ones on the critical path |

### 2.3 Jobs that could run on `merge_group` only

Candidates by "the PR author cannot act on the result before the queue anyway":
`commissioned-wasm-gates` (27.8), `wasm-browser-determinism` (11.4),
`wasm-host-determinism` (8.6), `perf-ratchet` (6.0), `test-wasm-windows` (6.0),
`commissioned-aviation-quality` (7.4). But see §4 item 6: GitHub cannot require a
check on the queue that it does not also require on the PR, so a queue-only lane
needs the aggregator pattern, and the repo's own recorded preference (commit
eb48f630b: "the two gates that each cost a CI round today") is to *add* gates to the
PR lane, not remove them. Listed for completeness; ranked low.

### 2.4 Independent steps inside `Commissioned wasm gates`

The five steps share nothing but the release target dir:

- A. `cargo test --release -p almide-wasm -p almide-wasm-run -p almide-spine -p almide-syntax -p almide-corpus -p almide-layout` — 679 s.
- B. `cargo test --release --test embedded_cross_test --test module_type_repr_test --test wasm_vm_parity_test` — 436 s (root crate release-test compile + run).
- C. `ALMIDE_RC_TRAP_DOUBLE_FREE=1 cargo test --release -p almide-wasm --test backend_parity --test gauntlet` — 76 s (rides A's compile).
- D. `scripts/check-target-availability.sh` — 426 s, needs only the downloaded binary, no cargo.
- E. `scripts/check-wasm-runtime-ratio.sh` — 25 s, binary only.

D+E have no compile dependency and can leave the job today (into their own job or
into `almide-gates`, which already has the binary and wasmtime). A+C and B are
separable into two jobs each paying its own release compile (rust-cache restores
deps; the workspace crates recompile). Wall: 27.8 → max(A+C ≈ 13.5, B ≈ 8.5, D+E ≈ 8.5)
≈ **13.5 min**; runner cost +~8 min per run.

### 2.5 Are the six giants fixture-range shardable?

All six enumerate the corpus the same way: `read_dir(spec/wasm_cross)` (722
`.almd`; `spec/wasm_fail` 4), `sort_by_key(path)`, one `for` loop, results
accumulated in `Vec`s and asserted at the end. None spawns threads. Partition hooks
today:

- `corpus.rs:78` honours `ALMIDE_CORPUS_FILTER=<substring>` (retain by stem) — a
  developer loop, explicitly "never set in CI".
- `wasm_runtime_interp_ledger.rs` and `crates/almide-spine/tests/run_parity.rs`
  have no filter at all.

A range/modulo partition (`ALMIDE_CORPUS_SHARD=k/N` applied after the sort:
`entries.retain(|(i, _)| i % N == k)`) is a ~10-line change in three places
(corpus.rs, the two ledger loops, run_parity's manifest loop). **Per-gate
soundness of a partial corpus:**

| gate | per-fixture assertion? | whole-corpus assertion that a partition breaks |
|---|---|---|
| wasm_cross_target_spec | yes (equal / allowed / stale / failed per fixture) | none |
| wasm_opt_parity_spec | yes | the "all `wasm_opt.is_none()` → skip" guard — fine per shard |
| interp_cross_target_spec | yes | none |
| run_parity | yes | **`n_unsupported <= ceiling`, `n_fuel <= ceiling`** (shrink-only counts over the whole manifest) and `manifest.len() > 550` — per-shard these must become "sum across shards ≤ ceiling" (emit counts as an artifact, sum in the coverage job) or the ceiling is checked N times against N partial counts, which is a silent loosening |
| interp_abstain_ledger | set equality observed ↔ ledger, **both directions** | **stale detection**: a ledgered fixture not in this shard reads as "no longer abstains" → false red (the comment at corpus.rs:74-77 already warns about this for the filter). Fix: compare against the ledger restricted to the shard's fixture set. New-abstain detection stays sound per shard. |
| interp_bridge_fallback_ledger | set equality over *names* (`module.func`), not fixtures | a name reached only by fixtures outside this shard reads as stale → false red; and a shard cannot tell. Must aggregate observed name sets across shards before comparing (artifact + combine job), or keep this gate unsharded (1586 s → the new floor) or make it fast by §4 item 2 instead. |

Coverage proof (the repo's rule: a partition that drops a fixture must be red, not
fast): a `--list-corpus k/N` mode printing the shard's stems, with the coverage job
asserting `∪ shards == ls spec/wasm_cross` and no duplicates — the same shape as
`Test shards cover every target`. Without that step, range sharding is exactly the
"faster green that tests less" failure and must not ship.

**Per-job estimates** (setup S ≈ 120 s measured from shard job start → `Cargo
tests` start; native-leg warm-up W ≈ 60–120 s *estimate* — the first `almide
build` in a fresh `/tmp/almide-run` compiles the runtime deps; ledgers and
run_parity have no W):

| giant | solo (s) | N=2 per job | N=4 per job |
|---|---|---|---|
| interp_abstain_ledger | 1666 | 833 + 120 ≈ **16 min** | 417 + 120 ≈ **9 min** |
| interp_bridge_fallback_ledger | 1586 | 793 + 120 ≈ 15 min (only with the aggregate compare) | 397 + 120 ≈ 8.6 min |
| wasm_cross_target_spec | 1377 | 689 + 210 ≈ 15 min | 344 + 210 ≈ 9.2 min |
| run_parity | 1310 | 655 + 120 ≈ 13 min | 328 + 120 ≈ 7.5 min |
| interp_cross_target_spec | 1223 | 612 + 210 ≈ 13.7 min | 306 + 210 ≈ 8.6 min |
| wasm_opt_parity_spec | 1075 | 538 + 210 ≈ 12.5 min | 269 + 210 ≈ 8 min |

Critical path: 5.5 (build) + ~16 (N=2) ≈ **21.5 min**, or 5.5 + ~9.2 (N=4) ≈ **15 min**
for the giants — but only if `Commissioned wasm gates` (33 min end) and the gravel
shards are also addressed, otherwise they are the floor. Job count: 6 → 12 (N=2)
or 24 (N=4) solo jobs; at N=4 the run is ~43 jobs, which worsens the capacity wait
in §0 unless §4 items 5/7 land first. N=2 is the sensible first step.

**The next giant after the six:** `diagnostic_harness_test` (1501 s summed; 7
`#[test]`s, two of them fixture loops over `collect_cases()`). Per-test split is
not in the weights file; if one test is ~1000 s the shard carrying it is ~20 min
of wall on its own once the six leave. Measure before assuming the gravel shards
are ~16 min.

## 3. GitHub-side levers

- **Larger hosted runners**: Linux 4-core $0.016/min, 8-core $0.032, 16-core
  $0.064 (billed even on public repos; standard runners are free here). The giants
  are one process running one fixture at a time; a 16-core runner would not shorten
  1666 s at all, and #2444 already removed the contention they suffered. The gravel
  shards are bounded by `BuildDirLock` (one `almide` native compile per machine at a
  time). The one thing larger runners buy is **a separate concurrency pool** — the
  29-min capacity wait would vanish for jobs moved onto them — at roughly 25 jobs ×
  ~8 min × $0.016 ≈ $3–4 per run on 4-core larger runners. Cheaper: cut the job
  count (items 5, 7).
- **Self-hosted**: hexa's darwin pool (168-min median queue) and roc's disabled arm
  shards are the peer evidence against a small pool; lean4 only does it with a
  hosted fallback action. Not recommended.
- **`concurrency`**: already `${{ github.workflow }}-${{ github.ref }}` with
  cancel-in-progress — correct for PR pushes. It also cancels a develop push run when
  the next queue merge lands (run 35565762632 → cancelled at 06:41 by the next push),
  which is fine only because the queue run judged the tree. Four peers (vera,
  jacquard, hexa, wado) deliberately never cancel main-branch runs to keep a
  per-commit record; almide's `save-if: develop` rust-cache write also dies with a
  cancelled run, so the dep cache is refreshed less often than it looks.
- **`merge_group`-only heavy lanes**: possible only through an aggregator (see item 6).
- **Required-check redesign**: replace the 13 named contexts with one aggregator job
  (`if: always()`, `needs:` every gate, fails on any non-success; on a docs-only run
  accepts `skipped` only for jobs guarded by `changes`). Retires the matrix-rename
  hazard the ci.yml header warns about, the solo-leg proxy step in
  `shard-coverage`, and the current state where seven substantive gates are not
  required. This is a strengthening, not a speed-up, but it is the precondition for
  any lane split.
- **nextest**: `--partition hash:k/N` over the whole archive would partition at the
  *test* level (so `diagnostic_harness_test`'s 7 tests spread) and is complete by
  construction (still provable with `nextest list --partition` unions), but it
  balances by count, not weight, and cannot split a single-test giant — it replaces
  the packer for gravel only. `slow-timeout = { period = "…", terminate-after = N }`
  in `.config/nextest.toml` plus `timeout-minutes` on every job closes the
  4h20m-hang class (#1008) that today can hold a slot for 6 h. Archives and
  test-groups are already in use; `--extract-to` per job is already the cheapest
  form.

## 4. Ranked plan

Savings are on the critical path from the post-#2444 baseline (~36 min + capacity
wait). "Risk" is specifically how the change could make a green mean less.

| # | item | mechanism | critical-path saving | risk | effort |
|---|---|---|---|---|---|
| 1 | **Measure #2444 first, then range-shard the giants at N=2 with a corpus-coverage gate** | `ALMIDE_CORPUS_SHARD=k/N` after the sort in corpus.rs / ledger / run_parity; matrix `leg × shard`; a `--list-corpus` mode; coverage job asserts ∪ == `ls spec/wasm_cross` and no dupes; run_parity emits counts, coverage job sums against the ceilings; abstain ledger compares against the ledger ∩ shard; bridge-fallback ledger **stays unsharded or aggregates names** | giants 27.8 → ~16 min per job: **~12 min** (36 → ~24), only realised together with #3 | a shard that silently sees fewer fixtures is the exact failure; the coverage step is mandatory in the same PR; the two ratchet/ledger gates need their aggregate form or they loosen | M (code in 3 test files + shard script + coverage job) |
| 2 | **Parallelise the interp sweep in-process** (the ledger's two gates and the oracle's interp leg): a scoped thread pool over the sorted fixture list, results collected by index | backend-free, no spawn, no flock, no shared fixture: `Interpreter` is `Rc`-based but thread-confined; the only statics are `OnceLock`/`Mutex`/`RwLock` (`stdlib_pool.rs:66`, `bundled_sigs.rs:47,107,230`) which compile today as `Sync`. Also compute both ledgers from one sweep (they call the same function). | ledger 1666 s → ~1666/3 ≈ 9–10 min on 4 vCPU without any job split; combined with #1 at N=2 → ~5 min per job. Runner minutes go *down* (one sweep for two ledgers). | a lock inside `parse_cached`/`bundled_sigs` serialises and caps the speedup (visible, not silent); nondeterministic ordering must not change the ledger text (collect by index; the ledger writer sorts already) | S–M |
| 3 | **Fan out `Commissioned wasm gates`** into A+C / B / D+E (commands verbatim) | three jobs, `needs: [changes, build]` | 27.8 → ~13.5 min; with #1 this is what moves the floor from 33 to ~24 | none to gate strength if the commands are copied byte-for-byte and all three become required (today the job is not required at all) | S |
| 4 | **Build the corpus legs each gate actually reads** | per-binary const `NEEDED_LEGS`; `build_corpus` skips the others (opt_parity: no native/no interp; cross_target: no wasm-opt/no interp; oracle: no wasm-opt) | unmeasured per leg; the native leg is a rustc compile per fixture, so opt_parity likely halves (1075 → ~500 s) and cross_target loses the interp leg (~1/4). Makes every #1 shard cheaper. | a gate that "no longer reads" a leg must genuinely not compare it — the three tests' bodies are the proof (`l.wasm_opt` unused in cross_target, etc.); add a compile-time `let _ = ` on the unused field is not enough, keep a test asserting the leg set per binary | S |
| 5 | **Stop the duplicate develop `push` run** for jobs the queue already ran | `if: github.event_name != 'push' || github.ref == 'refs/heads/main'` on the heavy jobs; keep `build` (rust-cache `save-if: develop` writer), `changes`, `checks`, lean on push | 0 on any single run's critical path; **removes ~25 job-slots per merged PR**, the largest single contributor to the 29–37-min capacity wait | develop has no push restriction (protection API: `restrictions: null`, `enforce_admins: false`), so a direct push would go untested; land only with a push restriction (or a ruleset rule) that forces the queue. Also the `github.event.before`-based docs-only path becomes moot. | S (yml) + repo settings |
| 6 | **Aggregator required check** (`CI required`, `if: always()`) and add the solo legs, Commissioned ×2, perf, Windows, coverage to it | vibe/wado/lean4 pattern | 0 min; it is the precondition for #1's matrix growth and any lane split, and it fixes seven currently-unrequired gates | an aggregator that treats `skipped` as pass must whitelist exactly the `changes`-guarded jobs; a `needs` list that misses a job silently drops it from the gate — keep `scripts/check-workflow-parse` style negative control listing every job name | S |
| 7 | **Timeouts + pinned tools + one setup composite** | `timeout-minutes` on every job (wado: 60 everywhere; giants 45, others 20); nextest `slow-timeout`/`terminate-after`; pin `cargo-nextest` version; cache a binaryen tarball instead of `apt-get update` (12 jobs); one composite action for toolchain+nextest+archive+wasmtime+binaryen; unify the wasmtime pin (v27 vs v47) | ~0.5–1 min per job on setup; a hang costs 45 min not 360 | none; pinning strengthens reproducibility | S |
| 8 | **Move `check-target-availability` (426 s) and `reachability ratchet` (485 s) into their own job(s)** once #1–#3 bring the floor to ~20 min | binary-only scripts | Almide gates 15.4 → ~7.5; matters only when it becomes the floor | none if verbatim | S |
| 9 | **Merge-group-only heavy lane** (Commissioned, determinism ×2, perf, Windows, aviation) | jobs `if: github.event_name != 'pull_request'`, enforced by the #6 aggregator failing on `skipped` when `event_name == merge_group` | PR lane −0 min after #1–#3 (those jobs are no longer the floor) but −6 job-slots per PR push; the queue run is unchanged | the class of failure it hides on the PR is found one queue round (~25 min) later and dequeues the PR — exactly the cost eb48f630b chose to pay up front; only worth it if capacity, not wall, is the complaint | S, but a policy change (○×) |
| 10 | **Larger hosted runners** | `runs-on: ubuntu-4-core`/8-core for the giants | ≈0 min per giant (single process); ~29 min of capacity wait avoided only by moving into their pool, at $3–4/run | none | S, but recurring cost; do #5 first |
| 11 | **Corpus table built once per shard and shared by the three gates** (a table artifact keyed on binary sha + fixture hashes; gates read it and assert the key) | replaces 3 builds with 1 per shard | after #4 the discarded legs are already gone; remaining win is native+wasm built 2× instead of 1× per shard (~5 min of runner time per shard, 0 on the critical path unless the table job is on it) | a stale or mismatched table silently judges old outputs — the key assertion is the whole gate; the repo's own history (#2090: "the scary wasm breakages are silent") argues for keeping legs and gates in one process | M; rank last |

**Projected end-to-end after 1–4 + 7** (arithmetic, not measured): queue pickup →
changes 0.1 → build 5.5 → max(giant shard ≈ 9–16 min with #2 making the ledger
≈ 5–9, Commissioned A ≈ 13.5, Almide gates 15.4, gravel shards ≈ 16–20 depending on
`diagnostic_harness_test`) ≈ **5.5 + ~16–20 = 22–26 min**, from 66 tonight and ~36
after #2444. Below that, `Almide gates` and the gravel shard carrying
`diagnostic_harness_test` are the floor and need #8 and a per-test measurement.
The capacity wait (29–37 min under a burst) is independent of all of this and is
addressed only by #5 (fewer runs), #7/#9 (fewer jobs per run) or #10 (another pool).

**What does not transfer from peers**: the 5-minute-suite repos (vibe, vera, zod)
optimise cache warmth and job start latency because their work is small; almide's
work is 13 ks of genuine execution. aver's `--partition slice:` transfers only to
multi-test binaries (the ledger binary has 2 tests, the rest 1). rust's bors/auto
tiering and lean4's label tiers are the only peer precedent for a PR/queue split,
and both accept later detection — the trade this repo has so far refused.
