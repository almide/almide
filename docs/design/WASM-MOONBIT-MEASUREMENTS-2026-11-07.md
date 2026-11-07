# Same-VM native and wasm comparison with Rust and MoonBit

Measured 2026-11-07. This replaces the mixed-date progress-page values with
one same-machine measurement of all three languages. No compiler or shipping
profile is changed in this task. Almide is the already verified strchurn fix
at `7d003524f71cac6af274f2858c50ae3dea29f27a`.

The wasm result is mixed: Almide wins 2, ties 1, and loses 5 against Rust;
it wins 7 and ties 1 against MoonBit. A tie means a measured ratio within
5% of one, not a statistical equivalence claim. Tiny process-duration rows
are particularly noise-sensitive. The wasm iter gap against Rust remains:
95.609 / 6.154 ms = 15.54× with ordinary startup, and 96.210 / 5.158 ms =
18.65× after Wasmtime precompilation. Startup/JIT does not explain most of
that gap. This measurement does not itself identify a new compiler cause
or implement the earlier intermediate-list optimization.

Native build time is slower than MoonBit in all six native profile/fixture
cells, under the product-profile boundary documented below. Normalized native
size is also larger in seven fixtures and tied in one; wasm size is smaller
in all eight. The previous page's native-size win claims do not survive
removing debug information consistently from both languages.

## Runtime

Median milliseconds, seven measured rounds after one excluded warmup.
All use original inputs. Native uses release artifacts. Wasm uses the
ordinary `wasmtime run module.wasm` command, including process startup and
module loading/compilation. MoonBit targets linear-memory `wasm`, not
`wasm-gc` or its separate `moonrun` host.

| Program | Almide native | Rust native | MoonBit native | Almide wasm | Rust wasm | MoonBit wasm |
|---|---:|---:|---:|---:|---:|---:|
| hello | 0.748 | 0.734 | 0.642 | 3.712 | 4.484 | 3.717 |
| fib | 2.214 | 26.626 | 18.908 | 41.530 | 42.257 | 70.532 |
| btrees | 97.757 | 322.529 | 132.261 | 95.853 | 501.879 | 523.945 |
| snorm | 87.522 | 87.750 | 75.198 | 117.600 | 90.465 | 186.784 |
| wordfreq | 53.580 | 63.309 | 107.371 | 161.468 | 90.214 | 211.757 |
| strchurn | 93.499 | 60.882 | 176.596 | 182.223 | 121.280 | 727.907 |
| jsonround | 163.922 | 153.805 | 172.079 | 271.744 | 164.007 | 553.305 |
| iter | 2.068 | 2.242 | 185.261 | 95.609 | 6.154 | 1123.180 |

Native strchurn includes the previous allocation fix; wasm strchurn does not
benefit from that native-only runtime change. Its measured wasm runtime is
182.223 / 121.280 / 727.907 ms (Almide / Rust / MoonBit): about 1.50× Rust,
and 3.99× faster than MoonBit. These are cross-language ratios from this VM,
not an improvement estimate pooled with older machines.

## Wasm after engine precompilation

`wasmtime compile` is outside the timed interval. The timed command is
`wasmtime run --allow-precompiled module.cwasm`, with the same engine version
and original inputs. Startup and loading the precompiled module are still
included; this is not a function-only microbenchmark. These artifacts were
created locally from the verified source modules before being loaded.

| Program | Almide | Rust | MoonBit |
|---|---:|---:|---:|
| hello | 3.332 | 3.405 | 3.377 |
| fib | 41.311 | 41.279 | 69.705 |
| btrees | 95.301 | 495.972 | 523.629 |
| snorm | 117.385 | 88.938 | 186.195 |
| wordfreq | 159.829 | 88.802 | 211.597 |
| strchurn | 180.123 | 120.032 | 724.234 |
| jsonround | 270.790 | 161.225 | 549.592 |
| iter | 96.210 | 5.158 | 1121.288 |

## Compile time

Median milliseconds. Each lane first prepares dependencies/runtime objects.
Every measured build changes an IR-reaching source constant, not a comment.
MoonBit keeps the project and its warmed build directory, changing the main
source each round. This corrects the older MoonBit measurement's fresh-project
build on every round. It must not be treated as a compiler speedup over that
older, different setup.

The constants stay within MoonBit's Int32 range where necessary, but are
large and unique across builds. Those outputs are never executed. Runtime
binaries come from a separate original-input directory.

| Program | Target | Profile | Almide | Rust | MoonBit |
|---|---|---|---:|---:|---:|
| fib | native | default | 169.362 | 50.834 | 101.828 |
| btrees | native | default | 237.716 | 63.279 | 102.098 |
| jsonround | native | default | 387.178 | 151.648 | 216.476 |
| fib | native | release | 2459.619 | 2341.207 | 180.913 |
| btrees | native | release | 2538.445 | 2349.512 | 212.671 |
| jsonround | native | release | 3673.461 | 2942.788 | 1543.536 |
| fib | wasm | default | 10.060 | 56.771 | 68.915 |
| btrees | wasm | default | 11.207 | 62.655 | 69.317 |
| jsonround | wasm | default | 115.498 | 106.553 | 115.175 |
| fib | wasm | release | 10.098 | 59.022 | 69.133 |
| btrees | wasm | release | 11.157 | 65.767 | 68.838 |
| jsonround | wasm | release | 115.252 | 322.872 | 114.168 |

### Profile boundaries

- Native Rust default is matched to Almide's dev policy: opt-level 1,
  debug assertions on, overflow checks off, no debug information and no
  incremental compilation. Rust jsonround has codegen-units 16; its
  dependency debug assertions are off, matching the split-runtime policy.
- Native Rust release is matched to Almide: opt-level 3, fat LTO,
  codegen-units 1, strip debuginfo, x86-64-v3,
  `-force-ordered-reductions=false`, debug assertions and overflow checks off.
- Rust wasm default uses opt-level 0 with debug assertions/overflow checks
  on. Rust wasm release uses opt-level 3, codegen-units 1, strip debuginfo,
  and no cross-crate LTO. Cargo jsonround uses the corresponding settings,
  with dependencies prepared in advance and only the main program rebuilt.
- Almide wasm uses its product default/release switches. Those are not a
  promise of the same optimization pipeline as Rust or MoonBit.
- MoonBit default is explicit `--debug`; release is `--release`, each with
  `--target native` or `--target wasm`. The archived dry-run shows native
  debug emitted directly as an x86-64 object with `-O0`, while native release
  emits C and compiles it with GCC `-O2`, `-fwrapv`,
  `-fno-strict-aliasing`, `-ffp-contract=off`, and the mimalloc runtime.
  It is not a matched-fat-LTO comparison. The progress page labels this
  as a comparison of product profiles.
- The standard-library split diagnostic remains `llvm-ar not found`;
  llvm-tools is not installed. No split/unsplit lanes are mixed.

Actual rustc argument vectors, outer build commands and MoonBit dry-run
commands are archived. The rustc adapter records and forwards arguments;
it does not override LTO in this experiment.

## Artifact sizes

Native: each release output is copied through `objcopy --strip-debug`,
removing debug information consistently across all languages while retaining
other symbols. Every copy is executed and compared with its original output.
Wasm: each release output is optimized with Binaryen 133 using
`-Oz --all-features --disable-compact-imports`; every optimized output is
executed under the same Wasmtime and checked. Runtime tables above use the
original wasm artifacts, not these size-optimized copies.

| Program | Almide native | Rust native | MoonBit native | Almide wasm -Oz | Rust wasm -Oz | MoonBit wasm -Oz |
|---|---:|---:|---:|---:|---:|---:|
| hello | 370368 | 361984 | 349976 | 304 | 39781 | 2122 |
| fib | 370232 | 364080 | 354128 | 399 | 40157 | 3937 |
| btrees | 381080 | 365736 | 354248 | 1537 | 40742 | 3965 |
| snorm | 428968 | 388896 | 366256 | 5706 | 63289 | 12059 |
| wordfreq | 445832 | 392384 | 369304 | 8109 | 65983 | 11725 |
| strchurn | 425320 | 376072 | 374352 | 11444 | 49817 | 15608 |
| jsonround | 559552 | 436432 | 442080 | 25659 | 99813 | 53970 |
| iter | 403136 | 364184 | 354632 | 1597 | 40135 | 5246 |

For example, this run's MoonBit strchurn executable is 504,680 bytes before
and 374,352 after removing debug information. Almide is 425,568 before and
425,320 after. Comparing the raw MoonBit output to an already mostly stripped
Almide output reverses that size result. The older page did not normalize
both sides this way; its native-size win counts are replaced, not carried
forward.

Raw, unnormalized sizes and hashes are also retained in `results.json` and
`file-manifest.json`.

## Environment, validation and evidence

- `almide-wasm-moon-1107`, GCE `almide-perf`, `asia-northeast1-b`,
  n2-standard-8 Spot, Ubuntu 24.04, 100 GB pd-standard. This is a different
  VM/disk from earlier reports; each displayed language comparison is within
  this VM. No build, benchmark or test ran on macOS.
- Almide 0.69.1 (dev), compiler SHA-256
  `0829216f3061ad19c06a0dcd5afda9710b99ff0133d57f30d7657a12d6bd9880`.
  This is byte-identical to the previously validated native strchurn compiler.
- Rust 1.99.1 (`58cf8f9c3`), LLVM 23.1.1, wasm32-wasip1 installed.
- MoonBit installed from the [official installer](https://www.moonbitlang.com/download/):
  moon 0.1.20261104 (`940aa18`), moonc v0.10.20+65efd5b93 (2026-11-05).
  The earlier comparison used v0.10.19. Executable hashes and exact CLI output
  are in `post.json` and `setup/versions.txt`. See the
  [official toolchain reference](https://docs.moonbitlang.com/en/stable/toolchain/)
  for target/profile terminology; archived commands determine this run's settings.
- Wasmtime 50.0.1 (`c307ec81d`), Binaryen 133, GCC 13.3.0.
- Seven measured samples per cell after one excluded warmup. Lane order
  reverses each round. No other build, compression or postprocessing job
  ran concurrently with timed measurements.
- 36 compile cells, 48 native/wasm runtime cells and 24 precompiled-wasm
  runtime cells completed. Every runtime observation checks exit status and
  stderr exactly, and stdout exactly except snorm's established 12-decimal
  floating-point normalization. All checks passed. Another 24 optimized-wasm
  and 24 stripped-native outputs were checked successfully.
- Source inputs are the existing eight-task fixtures. No benchmark algorithm
  was rewritten, and no compiler fix was made in this measurement task.
  The compiler test suite was not rerun; the previous compiler validation
  remains applicable to the identical binary, and this task adds the new
  cross-language artifact/output checks.
- The first attempt stopped when wasm-opt's unrestricted feature selection
  emitted compact imports that the default Wasmtime configuration rejected.
  Its partial measurements are excluded. `run-2` is the complete accepted
  run, with compact imports disabled for every language's size optimization.
- A postprocessing-only diagnostic initially lacked the rustc adapter's real
  compiler environment variable. The completed timed measurements were kept;
  that untimed diagnostic and collection were rerun successfully. Both logs
  are retained. It was not a compiler or benchmark-output failure.

The durable archive is
`almide-references/bench-sources/wasm-moon-rematch-2026-11-07/almide-wasm-moon-evidence.tar.gz`:
2,101 files, 45,466,766 bytes.

SHA-256: `db0318f3fb6a8710e495c0ae89096a6a83857f9cef1700fdb8fd1cbcc7654a97`.

Every file was verified after download. Start with
`evidence/run-2/results.json`, `evidence/run-2/post.json`, and
`evidence/setup/versions.txt`. Sources, scripts, all exact compile-only
inputs, logs, argument vectors, original executables, precompiled wasm,
optimized size artifacts and the compiler binary are retained. Build caches
and huge-constant executables are excluded.

The VM and boot disk are deleted after the verified archive is saved;
`lifecycle.json` alongside it records the deletion check. The progress pages
show only these latest measurements, with one compile-time comparison table
per opponent rather than historical/experimental profile tables.
