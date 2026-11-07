# Native build profiles with matched LTO settings

Measured 2026-11-07, following the [fixed-cost measurements](NATIVE-BUILD-MEASUREMENTS-2026-11-07.md). **The compiler and shipping defaults are unchanged.** This is a controlled experiment, not a new optimization release.

The earlier fib release comparison (Almide 2.45 s versus Rust 49 ms) compared fat LTO with no cross-crate LTO. With both sides using fat LTO and the same CPU/optimization policy, fib is **2532.750 / 2448.351 ms = 1.03×**. Removing LTO brings Almide fib to **162.649 ms**, but slows its strchurn runtime by **33.6%**. A blanket removal does not meet the requirement to preserve runtime speed.

## Compile time

Seven measured alternating rounds after one warmup, median milliseconds. Runtime/dependency caches are warm; each program build changes an IR-reaching constant.

| Program | LTO | Almide | Rust | Almide / Rust |
|---|---|---:|---:|---:|
| fib | fat | 2532.750 | 2448.351 | 1.03× |
| fib | thin | 2265.788 | 2141.931 | 1.06× |
| fib | off | 162.649 | 45.935 | 3.54× |
| btrees | fat | 2612.744 | 2449.024 | 1.07× |
| btrees | thin | 2352.182 | 2179.382 | 1.08× |
| btrees | off | 229.811 | 55.608 | 4.13× |
| jsonround | fat | 3701.707 | 2969.977 | 1.25× |
| jsonround | thin | 2653.945 | 2466.566 | 1.08× |
| jsonround | off | 426.268 | 468.820 | 0.91× |

The matched default-profile comparison also removes an asymmetry: Almide default uses opt-level 1, debug assertions on and overflow checks off, while the earlier handwritten Rust debug command used rustc defaults.

| Program | Almide default | Rust with matching dev policy | Ratio |
|---|---:|---:|---:|
| fib | 172.598 | 51.542 | 3.35× |
| btrees | 240.792 | 63.993 | 3.76× |
| jsonround | 394.498 | 152.821 | 2.58× |

These are new same-machine comparisons, not speedups over the previous machine. The earlier product-setting measurements remain valid as observations of those commands, but should not be presented as identical optimization settings.

## Original-input runtime

Median milliseconds. All profile variants match exactly within each language for stdout, stderr and exit code. Across languages, snorm uses the existing 12-decimal normalization; every other observation matches exactly.

| Program | Almide fat | Almide thin | Almide off | Off vs fat | Rust fat | Rust thin | Rust off |
|---|---:|---:|---:|---:|---:|---:|---:|
| hello | 0.700 | 0.716 | 0.701 | +0.1% | 0.658 | 0.678 | 0.674 |
| fib | 2.143 | 2.191 | 2.237 | +4.4% | 26.597 | 26.816 | 26.652 |
| btrees | 103.704 | 101.832 | 103.094 | -0.6% | 341.491 | 340.799 | 360.146 |
| snorm | 87.039 | 74.910 | 74.279 | -14.7% | 86.870 | 73.944 | 74.628 |
| wordfreq | 53.783 | 57.073 | 56.265 | +4.6% | 62.514 | 63.514 | 65.160 |
| strchurn | 114.385 | 116.423 | 152.786 | +33.6% | 61.189 | 62.629 | 65.288 |
| jsonround | 160.878 | 164.412 | 170.582 | +6.0% | 155.056 | 151.214 | 157.487 |
| iter | 1.909 | 1.947 | 1.995 | +4.5% | 2.011 | 1.933 | 1.928 |

Negative percentages mean faster execution. Tiny process-duration rows are noisy and do not establish a robust small regression or improvement. The larger observations matter for policy: strchurn is 114.385 → 152.786 ms without LTO; wordfreq is 53.783 → 56.265 ms; jsonround is 160.878 → 170.582 ms. Conversely, snorm improves from 87.039 to 74.279 ms. Thin LTO still costs over two seconds for fib and btrees, and wordfreq is about 6.1% slower than fat in this sample. It is not a universal replacement either.

## Controls and evidence

- Same GCE Spot VM for all new lanes: `almide-lto-1107`, `almide-perf`, `asia-northeast1-b`, n2-standard-8, Ubuntu 24.04, 100 GB pd-standard. This is a different VM/disk from the earlier fixed-cost experiment; do not pool the samples.
- rustc 1.99.1 (`58cf8f9c3`, 2026-10-13). The existing Linux compiler is byte-identical to the final fixed-cost candidate: SHA-256 `8edee5f8f8584414822a06cd6cf1be5ab7bb7a67f4f2f6c14e636827742241c8`.
- Release policy on both sides: opt-level 3, codegen-units 1, strip debuginfo, x86-64-v3, LLVM `-force-ordered-reductions=false`, overflow checks off, debug assertions off, and the selected fat/thin/off LTO. Implicit rustc release defaults and explicit equivalents are distinguished in the archived argv.
- The experiment-only C exec adapter replaces the existing Almide `-C lto=fat` argument, forwards probes/runtime calls unchanged, preserves the rustup proxy and records every actual rustc argument vector. It is not a supported compiler option. Each lane has a separate TMPDIR/cache because this external override is not part of the compiler cache identity.
- Runtime rlibs and Rust serde_json dependencies are prebuilt per lane. Rust jsonround uses Cargo profile overrides for LTO/codegen/strip and RUSTFLAGS for CPU/LLVM settings. Its no-LTO lane uses Cargo lto=false with one codegen unit; it does not enable cross-crate or multi-unit local LTO. No dependency versions were changed.
- Matched dev Rust uses opt-level 1, debug assertions on, overflow checks off, no debuginfo and no incremental compilation. For Cargo jsonround, codegen-units is 16, matching nonincremental rustc defaults; dependency debug assertions are off, matching the split runtime policy. Different runtimes and source implementations remain inherent to this language comparison.
- Each compile-only input changes the original fib call, btrees bound or jsonround count with a fresh microsecond-based nonce. Those binaries are never executed. Runtime binaries come from original inputs in a separate directory.
- One excluded warmup and seven measured rounds, alternating/reversing lane order. No concurrent builds or archive compression during timing. Raw times, output observations, actual flags and original-artifact hashes are archived.
- No compiler source or product profile was changed, and no build/test ran on macOS. The full compiler test suite was not rerun for a measurement-only task. The original-input programs and experiment shim were compiled/executed only on GCE.

## Existing standard-library split was unavailable

The stock release diagnostic was:

```text
almide: release std split unavailable: llvm-ar not found (rustup component add llvm-tools)
```

The existing `src/cli/std_split.rs` optimization needs the toolchain LLVM tools to precompile the panic/backtrace portion of std. This VM used the minimal pinned toolchain, so all matrix lanes used the unsplit standard library. We did not install those tools midway through the experiment or mix split and unsplit samples. The benefit of enabling that existing optimization is a separate unmeasured candidate here; it must not be counted as achieved.

## Decision

1. Correct the progress page: show matched LTO comparisons separately from the historical product-default comparisons. The 50× fib claim is not a comparison with equal optimization policy.
2. Preserve the shipping release profile for now. Turning LTO off reduces fib build time by 93.6%, but the strchurn/wordfreq/jsonround runtime losses fail a blanket no-regression decision. No global off/thin change is adopted.
3. Default native overhead remains material even after matching the dev policy (2.58–3.76× across these three fixtures). Matching expensive fat-LTO costs does not establish the rustc-or-better build goal.
4. Before a future policy change, investigate existing std-split availability and which cross-crate inlining/runtime boundaries account for the no-LTO regressions. No numeric-loop/LIR implementation is authorized by this experiment.

## Reproduction archive

`almide-references/bench-sources/vs-rust-2026-11-07/lto-matched/almide-lto-evidence.tar.gz` contains 1,596 files, 1,238,530 bytes.

SHA-256: `6478323adad9f894e38dc23c20737456a22fb33a84d92806874e412a8b76a56a`.

Start with `lto-results/summary.json`, `results.json`, `actual-profile-counts.json`, `original-artifacts.json`, and `default-matched/results.json`. Scripts and generated/input sources are included. `file-manifest.json` records per-file hashes; all were verified after download. Huge-constant binaries and cache binaries are excluded. The unchanged compiler is retained in the earlier native-fixed-costs evidence archive.

The measurement and collection jobs finished with rc=0. The VM and boot disk are deleted after the verified archive is collected; lifecycle verification is recorded alongside the durable archive.
