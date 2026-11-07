# Native string churn: fewer allocations and cross-crate inlining

Following the [matched-LTO experiment](NATIVE-LTO-MEASUREMENTS-2026-11-07.md),
this change removes redundant native string allocations. The shipping release
profile still uses fat LTO. The no-LTO measurements use an experiment-only
rustc adapter; they do not introduce a product option or change the default.

## Implementation

- ASCII upper/lower case conversion now copies directly into an `AlmideStr`
  block through dedicated case-converting constructors. It previously allocated a Rust `String`
  and then copied that into a second allocation. Empty strings retain the
  static, allocation-free path. Non-ASCII case conversion is unchanged,
  including C-412 and contextual Greek final sigma.
- `trim`, `trim_start` and `trim_end` construct the result directly from the
  trimmed slice, removing the intermediate Rust `String`.
- `list.join` uses the existing exact-size `AlmideStr::join`, removing the
  intermediate joined Rust `String`.
- Case-converting constructors and case/trim/join wrappers are marked inline
  so the split runtime boundary does not require LTO for these optimizations.
  General accessors, conversions and unique-view methods keep their existing
  inline policy. No language semantics, ownership pass or benchmark source
  was changed.

The new constructors write only into their own freshly allocated block. They
skip the write for an empty input, preserving the immortal empty block, and
leave non-ASCII bytes unchanged. ASCII byte mapping preserves length and
UTF-8 validity. Shared input strings are only borrowed and never modified.

## Measurement

Seven alternating rounds after warmup on the same VM. Median milliseconds.
The final direct-construction candidate is adopted; general string accessors
and unique-view methods retain their previous inline policy.

| Runtime | Before fat | After fat | Before off | After off |
|---|---:|---:|---:|---:|
| strchurn | 112.857 | 93.572 | 150.341 | 114.598 |
| wordfreq | 53.273 | 53.684 | 56.488 | 56.317 |
| jsonround | 165.424 | 163.680 | 172.464 | 172.562 |
| snorm | 87.800 | 87.697 | 74.782 | 75.319 |
| fib | 2.052 | 2.051 | 2.132 | 2.133 |
| btrees | 95.096 | 94.583 | 95.967 | 95.536 |
| iter | 2.010 | 2.056 | 1.922 | 1.976 |
| hello | 0.752 | 0.745 | 0.736 | 0.748 |

Strchurn improves **17.1% with fat LTO** (112.857 → 93.572 ms) and **23.8%
without LTO** (150.341 → 114.598 ms). The latter is within 1.5% of the old
fat-LTO result, but still 22.5% slower than the newly improved fat-LTO result.
A blanket LTO removal is therefore still not adopted. Wordfreq changes by
+0.8% with fat LTO and -0.3% without it; this sample does not establish a
material regression. Tiny process-duration differences are noise-sensitive.

An earlier broad-inline candidate reduced strchurn to 91.832 / 111.911 ms,
but wordfreq was about 3% slower, also in a separate 21-round check. That
candidate is not shipped. The archive retains its measurements and sources.
The final implementation keeps the allocation improvement without those
broad annotations.

| Compile | Before fat | After fat | Before off | After off |
|---|---:|---:|---:|---:|
| fib | 2460.744 | 2463.140 | 158.940 | 157.498 |
| btrees | 2566.220 | 2549.862 | 224.455 | 223.941 |
| jsonround | 3897.023 | 3911.900 | 442.928 | 443.189 |

No material compile-time improvement is claimed from this runtime change.

## Latest comparison against Rust

A separate final pass on the same VM compares the adopted compiler directly
with Rust, using the shipping dev and fat-LTO release profiles. These are the
values on the progress page; experimental thin/off and historical native
comparisons are removed from that page. Do not pool this pass with the A/B
samples above.

| Program | Profile | Almide | Rust | Ratio |
|---|---|---:|---:|---:|
| fib | default | 171.267 | 51.190 | 3.35× |
| btrees | default | 239.804 | 62.937 | 3.81× |
| jsonround | default | 389.860 | 152.638 | 2.55× |
| fib | release | 2503.033 | 2385.299 | 1.05× |
| btrees | release | 2566.668 | 2353.321 | 1.09× |
| jsonround | release | 3689.650 | 2976.319 | 1.24× |

Default uses opt-level 1, debug assertions on and overflow checks off on
both sides. Rust jsonround dependencies are built in advance for each profile;
only its program is rebuilt in timed rounds. The dev dependency assertions
are off and codegen-units is 16, matching the earlier controlled comparison.

| Runtime, fat LTO | Almide | Rust | Ratio |
|---|---:|---:|---:|
| hello | 0.688 | 0.682 | 1.01× |
| fib | 2.160 | 26.665 | 0.08× |
| btrees | 95.779 | 317.623 | 0.30× |
| snorm | 87.753 | 86.291 | 1.02× |
| wordfreq | 53.872 | 63.091 | 0.85× |
| strchurn | 91.339 | 60.665 | 1.51× |
| jsonround | 164.635 | 156.742 | 1.05× |
| iter | 1.989 | 2.006 | 0.99× |

The final cross-language runtime pass checks exact exit status and stderr;
stdout also matches exactly except snorm, using the established 12-decimal
normalization. The wasm build rows retained on the progress page are its
latest available 2026-11-07 measurements from the previous VM; they were not
remeasured here. The native runtime fix does not modify the wasm leg.


## Controls and validation

All builds, execution and tests ran on `almide-strchurn-1107`, an n2-standard-8
GCE Spot VM in `almide-perf` / `asia-northeast1-b`, Ubuntu 24.04, 500 GB
pd-standard. No build or test ran on macOS. This is a separate machine from
previous reports; every before/after comparison in this report is within this
VM and measurement run.

The baseline is the retained final fixed-cost compiler, SHA-256
`8edee5f8f8584414822a06cd6cf1be5ab7bb7a67f4f2f6c14e636827742241c8`.
The source base is `4b783ec68`. Rust is 1.99.1; release lanes use opt-level 3,
codegen-units 1, strip debuginfo, x86-64-v3, and LLVM
`-force-ordered-reductions=false`. The adapter changes only `lto=fat` to
`lto=off`, records actual rustc argv, and uses a separate cache per lane.
The existing standard-library split remains unavailable without llvm-tools.

Each main result is a median of seven alternating/reversed rounds after an
excluded warmup. Runtime uses original inputs, and requires identical stdout,
stderr and exit status for every lane of each fixture. Only strchurn includes
Rust in this experiment; the other seven fixtures compare Almide versions and
profiles. Compile timing changes an IR-reaching constant on every build with
warm runtime caches. Huge-constant binaries are never executed.

The new native/wasm integration test covers Unicode casing, contextual final
sigma, Unicode whitespace, empty inputs, shared aliases, empty/single/multiple
joins and Unicode separators. The `almide-str` tests additionally exercise all
128 ASCII bytes, mixed UTF-8, static and shared input buffers, exact capacity,
and preservation of the allocation-free empty result. Miri checks the two new
constructor tests. The MIR tests also exercise runtime-index behavior.

No language contract changes. Generated native runtime source changes are
intentional; the acceptance check here is semantic output parity, not byte
identity with the old runtime implementation.

## Scope

This removes concrete costs in strchurn. It does not establish that disabling
LTO preserves speed for all programs, or that Almide strings match Rust's
borrowed-slice pipeline. Rust's strchurn keeps split/trim slices borrowed until
case conversion; Almide still materializes owned split and trim results.
Eliminating those remaining intermediates would need an ownership-aware
transformation, beyond this runtime-only change.

No LIR/Wyve backend implementation is included. That design remains awaiting
owner review.

## Final verification and evidence

- `cargo test --release -p almide-str -p almide-mir`: 305 tests passed.
- Five related integration suites, including the new native/wasm observation
  test: 14 tests passed.
- `almide test`: 4,983 tests in 590 files passed (584 via wasm, six native
  fallback files).
- `cargo +nightly miri test -p almide-str ascii_case_`: both new tests passed,
  Miri 0.1.0 (`c980177d81`, 2026-11-06).
- All benchmark and verification jobs returned zero. Final sources and the
  generated runtime registry were downloaded from GCE and byte-verified
  against the local worktree. No Mac build or test was used.

The durable archive is
`almide-references/bench-sources/vs-rust-2026-11-07/strchurn/almide-strchurn-evidence.tar.gz`:
5,146 files, 132,366,045 bytes.

SHA-256: `bdd319fc79351207623b5df3aa783f28567678ee23e52d65cbe3494792295d85`.

Every file was verified against `file-manifest.json` after download. Start at
`evidence/bench-direct/results.json`, `evidence/latest-dev/results.json`, and
`evidence/latest-release/results.json`. Other bench directories contain the
intermediate candidates. Scripts, flags, outputs, source overlays, final
sources, compiler binaries and original-artifact hashes are retained.
Compile-only binaries and runtime caches are excluded.

The VM and boot disk are deleted after the verified archive is saved;
`lifecycle.json` alongside the archive records the deletion check.

Final compiler SHA-256: `0829216f3061ad19c06a0dcd5afda9710b99ff0133d57f30d7657a12d6bd9880`.
