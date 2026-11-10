# Native build measurements and remaining costs

Measured on 2026-11-07 for #4595. Default native fib now builds in **166.7 ms**,
against **44.5 ms** for handwritten Rust: **3.75×**, so the rustc-or-better
goal is **not met**. The same-machine prior-four-change baseline was about
284 ms. The handoff's original 643 ms is historical, not the baseline of this
paired experiment. The next backend step is [design only](NATIVE-LIR-WYVE.md),
pending owner approval.

## Changes

| Change | Source commit | Result |
|---|---|---|
| Generate runtime index at compiler build time | `1d4004eb4` | Removes process-start scanning; generated index shares the scanner with its reference test and preserves filesystem provenance |
| Reuse entry AST in v1 native rendering | `874a0372b` | Removes the second entry-source parse; retains v1 preparation/check/lowering, diagnostics, modules and source fallback |
| Cache rustc version output within a process | `519d91142` | Reuses discovery/minimum-version probes; verbose toolchain identity remains distinct and cached |
| Use ring SHA-256 for native artifact digests | `a05dca64a` | Same full-file SHA-256 and hex encoding; no weakening of cache integrity or invalidation |
| LIR and Wyve design | `3d80f0417` | Rust generic backend, Almide-generated Rust scheduler, proof and ABI boundaries; no backend implementation |

Index generation also moves the full runtime text construction needed by that
index into the compiler build. The existing v0 fallback can still call its
runtime-crate generator. No speculative additional v0 caching was adopted.
Parsing once is a requested structural improvement; these small fixtures do
**not** demonstrate a speed improvement from that change alone.

## Method and provenance

GCE `almide-index-1107`, project `almide-perf`, zone `asia-northeast1-b`:
Spot `n2-standard-8`, Ubuntu 24.04, Intel Xeon 2.80 GHz, pd-standard boot disk.
The disk was enlarged from 80 to 500 GB before all timed comparisons after
debug linking showed severe I/O pressure. All paired timings used that same
machine and disk. AVX/AVX2 were available; SHA_NI was absent.

- rustc 1.99.1 (`58cf8f9c3`, 2026-10-13), wasmtime 50.0.1, wasm-opt 133.
- Source baseline `eb215df01`, then the handoff's four branches in order.
- `ab2.py` and `ct.py` were adapted to alternate sides for seven measured
  runs after a warmup, with raw samples preserved. Every build changes an
  IR-reaching constant using a fresh microsecond nonce. Rust's large unused
  btrees range uses an i64 suffix.
- Build-time inputs with huge constants were **never executed**. Runtime
  checks use original inputs and compare exit status, stdout and stderr.
- Runtime uses release programs. Rust uses `-O -C codegen-units=1
  -C strip=debuginfo`; jsonround uses Cargo with serde_json dependencies
  prebuilt. Almide release enables fat LTO, while the Rust comparison does
  not: these are the products' selected modes, not identical optimizer flags.
- No build or test ran on the Mac. VM and boot disk were deleted after
  evidence collection; subsequent GCE instance/disk queries returned no rows.

Evidence lives in
`almide-references/bench-sources/vs-rust-2026-11-07/native-fixed-costs/`.
Archive `almide-native-final-evidence.tar.gz` contains 243 files, 50,312,562 bytes;
SHA-256:
`e7c71628f81673fc16029ef3513fc35bbf8c47eecc3394ae15f02643afcb4fe1`.
It includes source/patch inputs, scripts, raw measurements, test logs, render
hash manifests, WAT and perf data. The archived compiler is Linux x86_64.
Profile source files also contain huge constants; do not execute their outputs.

## Independent before/after measurements

Default native build medians, milliseconds. Each row is its own paired run;
do not subtract across rows as if they were one uninterrupted measurement.

| Pair | fib | btrees | jsonround |
|---|---:|---:|---:|
| Prior four → generated index | 282 → 211 | 353 → 281 | 500 → 431 |
| Index → entry parse once | 211 → 212 | 279 → 280 | 431 → 434 |
| Parse once → version memo | 213 → 175 | 279 → 245 | 433 → 395 |
| Prior four → version memo stage | 284 → 175 | 352 → 244 | 501 → 392 |
| Version memo → ring digest, quiet final run | 175 → 167 | 242 → 234 | 393 → 384 |

All original-input observations matched. The quiet SHA microbenchmark hashed
the same 4,546,520-byte binary in about 20.18 ms with sha2 and 12.29 ms with
ring, with identical digests. The full-build improvement is smaller but
repeatable in all three fixtures. The profile identifies ring's AVX SHA path.

## Final comparison with Rust

`ct-digest-results.json`, seven raw samples per cell, median in milliseconds:

| Program | Native default | Rust debug | Ratio | Native release | Rust -O |
|---|---:|---:|---:|---:|---:|
| fib | 166.717 | 44.476 | 3.75× | 2454.096 | 49.039 |
| btrees | 231.438 | 51.122 | 4.53× | 2515.824 | 58.642 |
| jsonround | 380.011 | 92.395 | 4.11× | 3651.160 | 297.650 |

| Program | Wasm default | Rust debug wasm | Wasm release | Rust -O wasm |
|---|---:|---:|---:|---:|
| fib | 9.811 | 56.151 | 9.753 | 63.248 |
| btrees | 10.881 | 63.683 | 10.905 | 71.335 |
| jsonround | 114.686 | 103.013 | 114.013 | 212.381 |

The release build gap remains large (about 50× for fib) and includes the fat
LTO policy difference. It is not solved by removing frontend fixed costs.

## Correctness and runtime checks

- Initial `v1first-results.log`: 105 suites with rc=0.
- **1,994** render outputs byte-identical across baseline, generated index
  and parsed-entry API. This includes every original 1,990 path plus four
  newly added wasm_cross fixtures; no original path was lost.
- Fresh `cargo test -p almide-mir`, native_floor_compile_test, rustc_floor_test
  and run_cache_recovery_test passed; the combined MIR gate passed in release.
- check_rejects_unbuildable_test, check_and_build_agree_in_modules_test,
  native_v1_differential_test, fs_scope_test, fan_region_floor_test and
  fan_meter_splice_test passed.
- build_cache_private_test: all three passed, including independent sha2
  digest comparison and corrupt/planted executable recovery.
- Full final `almide test`: **4,983 tests, 590 files, zero failures**
  (584 wasm, six native fallback). The preceding pre-digest run also passed.

Original-input native runtime, seven alternating measured runs, milliseconds:

| Program | Prior four | Index + parse + version memo | Rust |
|---|---:|---:|---:|
| hello | 0.692 | 0.693 | 0.712 |
| fib | 1.953 | 2.125 | 26.759 |
| btrees | 94.947 | 96.250 | 334.465 |
| snorm | 87.003 | 86.853 | 77.509 |
| wordfreq | 53.777 | 53.527 | 65.663 |
| strchurn | 114.420 | 114.227 | 67.116 |
| jsonround | 163.182 | 163.330 | 167.627 |
| iter | 1.916 | 1.981 | 2.281 |

These runtime timings precede the artifact-hashing-only change. Prior/new
observations match exactly; the Rust snorm comparison uses the benchmark's
existing 12-decimal normalization, while the others match exactly. Longer
benchmarks vary by at most about 1.4%; small process-duration timings are noisy.
Fib moved by 0.17 ms (8.8%): its main (1,609 bytes) and fib (94 bytes) machine
code are individually byte-identical. This is not evidence that every binary
byte matches, or a proof of zero runtime regression for all inputs.

## Wasm iter investigation

The shipped route is the structural `almide-wasm` emitter, not the retired
MIR wasm renderer. `list_comb.rs::lower_list_sum` materializes its argument;
`list.rs::lower_list_fold_arm` uses `lower_list_fold_fused`, which can consume
the range/map/filter chain without intermediate lists.

Controlled source-only variants over the original ten-million-element input:

| Variant | Native ms | Wasm ms |
|---|---:|---:|
| Original sum | 2.124 | 91.181 |
| Explicit fold | 2.175 | 12.510 |
| Explicit loop | 2.149 | 12.528 |
| Rust | 2.361 | 5.720 |

All observations matched. Original wasm is 15.94× slower than Rust. The fold
experiment is 7.29× faster than sum but remains 2.19× slower than Rust.
This is an investigation, **not a shipped compiler fusion fix**.

The allocation diagnostic for sum reports three allocations, one reuse and
240,000,000 allocated bytes (`heap_end=160065752`); fold reports zero
allocations (`heap_end=65648`). Sum and fold modules are 2,175 and 520 bytes.
Fold's WAT retains a counted loop and signed remainder lowering. Rust's main
has no remainder or filter branch: its induction variable steps by 12 and
each iteration accumulates two accepted values. This supports further work
on range facts and loop simplification; the entire remaining timing gap has
not been causally isolated from startup and other costs.

A candidate compiler fix should share the fusion legality checks and add an
integer-sum sink, preserving callback effects, abort order, ownership and
range/overflow behavior. Gate it with effectful and aborting stages, aliases,
empty/extreme ranges and differential observations. Float reductions need
their own ordering argument; an integer result does not license reassociation.

## Native strchurn and remaining build cost

Strchurn remains about 1.70× slower than Rust. One perf run attributes 21.86%
of Almide samples to malloc and 7.36% to free, against 9.19% and 5.15% for
Rust; Almide's split-to-AlmideStr path also appears prominently (6.93%).
This supports an allocation/materialization hypothesis, not a complete causal
proof. Investigate borrowed split/trim views or safe pipeline fusion while
preserving Unicode behavior, ownership and copy-on-write semantics.

The final fib build perf recording has 358 task-clock samples, none lost:
rustc 39.39%, almide-main 18.16%, rust-lld 15.64%, LTO thread 12.29%, optimize
thread 11.45%. About 79% of CPU samples are now in rustc/LLVM/linking threads.
These are **CPU sample shares, not wall-clock phase durations**. DWARF
unwinding was used without inline debug attribution and with restricted kernel
symbols. Hashing and file copying remain visible. ALMIDE_TIME_PHASES reports
fib parse/resolve 1 ms, check/lower 2 ms, optimize/link 1 ms; these phase labels
do not cover the later external native toolchain.

## Superseded attempts and integration

Earlier attempts are retained in the archive rather than hidden:

- Tar-preserved mtimes caused Cargo to reuse a stale library and report a
  missing parsed-program API. Touching transferred sources fixed the build.
- fan_meter_splice initially could not locate the compiler; setting ALMIDE_BIN
  made all three cases pass before timing. A negated grep under `set -e` failed
  to stop the initial script; explicit result inspection caught it.
- Initial Rust iter omitted strip=debuginfo and inflated startup. Only
  `iter-investigation-final.log` and final result JSON support the table above.
- Initial SHA diagnostics were contended. Use `sha-idle.log` and the quiet
  `ab-fixed-digest-final.log`; archive compression overlapped an earlier pair.

The updated handoff names `post-rc1-fixes`; its four existing commits are
`ddfbee11e`, `57b880cb3`, `d5fba20f1`, `e1512ff76`. New work is prepared in a
separate integration worktree. The index extraction moves the earlier lexer
memo into native_floor_scan.rs, keeps its test in native_floor_tail.rs and
adds the Mutex build import, exactly as in `almide-bench-stages.sh`.
Use patches for parse-once so the earlier `mod v1_rustc_build` remains intact.
No PR or enqueue is performed; almide-74 owns the single combined PR.

LIR implementation remains an owner decision. The design spells out the C ABI,
five contract classes, missing-proof rules, stage0 regeneration and full
wasm_cross observation gates. Wyve's additive-monoid theorems do not establish
associativity of IEEE floating-point addition. The fib 35–50 ms estimate is
unverified; only an approved prototype can test it.
