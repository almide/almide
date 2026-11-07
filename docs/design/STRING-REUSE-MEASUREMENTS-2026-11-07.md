# String buffer reuse measurements — 2026-11-07

Native strchurn improved from **128.306 to 111.823 ms (12.8% less time)** on the same VM. Rust took 83.353 ms and MoonBit 219.328 ms: Almide/Rust is 1.34×. Allocation diagnostics fell from 3,600,005 to 2,000,005 allocations (44.4%), with unchanged output `5513042 115927526` and unchanged peak live bytes (10,379,651).

## Change

Baseline is `c05c53f50`; candidate code is `ae5a27da2bebed33d330251ee1612738f4ee6d3b`. The candidate compiler SHA-256 is `d517423a43fc670238b1647d5cd8a87dd8e9aebea8190ac76190c7e91f3e7189`.

The five string trim/case declarations now carry `@reuse(s)`, backed by native owned twins. The existing last-use and borrow-lowering machinery accepts immutable string views as reusable sources and preserves the original string-view borrow when a move is unavailable. Borrowed parameters, borrowed iterator binders, globals and shared cells retain their borrowing rules. No new builtin-name property list was added.

`AlmideStr::retain_range` checks UTF-8 boundaries before any mutation, moves the retained bytes within a unique buffer and preserves its capacity through the unique view. Shared/static buffers copy only the retained slice; an unchanged range retains the original. ASCII casing edits a unique view. Non-ASCII casing still uses the existing Unicode implementation, including final sigma and the C-412 prohibition on non-ASCII-to-ASCII folding. Retaining a unique range keeps its capacity, as existing collection reuse does.

The generated strchurn map now calls `to_upper_owned(trim_owned(p))`. This removes two allocations per split piece. Splitting, joining and checksum materialization remain. Wasm's string implementation was not optimized in this change; the shared declarations were checked against its full ownership coverage.

## Method

All builds, tests and timing ran on GCE Spot `almide-string-1107`, `n2-standard-8`, Ubuntu 24.04, 500 GB `pd-standard`, project `almide-perf`, zone `asia-northeast1-b`. No Mac build/test was run. Rust 1.99.1, Wasmtime 50.0.1, Binaryen 133, moon 0.1.20261104 / moonc v0.10.20+65efd5b93; exact versions and CPU details are in `raw/evidence/setup/versions.json`.

One warmup was excluded, then seven alternating rounds were recorded. Compile measurements change an IR-reaching constant every time, keeping each exact input; those large-constant outputs are never executed. Runtime uses only original inputs. Native Rust uses matching optimization flags (default opt-level 1; release O3/fat LTO/codegen-units 1/x86-64-v3). Primary native comparisons use ordinary std, with `ALMIDE_RELEASE_STD_UNSPLIT=1`; rustc argument logs verify this. Runtime/dependency preparation precedes the measured rebuilds. MoonBit uses its product debug/release profiles, not identical LTO settings. Both Almide lanes have the same independent verifier installed; wasm compile timings include certificate checking. Allocation diagnostics are separate from ordinary timings.

The phase diagnostic omitted the checksum while keeping the same generated strings and length output: native 125.901 → 115.510 ms and precompiled wasm 231.886 → 209.468 ms. It motivated reducing build-side allocations first; these diagnostic times are not mixed into the primary table. VM-to-VM absolute timings are not treated as an optimization delta.

## Native runtime, milliseconds

| Program | Baseline | Candidate | Rust | MoonBit |
|---|---:|---:|---:|---:|
| hello | 0.868 | 0.843 | 0.862 | 0.768 |
| fib | 2.207 | 2.256 | 33.014 | 24.095 |
| btrees | 103.380 | 106.575 | 516.032 | 207.441 |
| snorm | 95.933 | 96.680 | 104.992 | 82.205 |
| wordfreq | 67.586 | 68.617 | 70.182 | 127.461 |
| strchurn | 128.306 | 111.823 | 83.353 | 219.328 |
| jsonround | 223.856 | 228.037 | 220.520 | 228.380 |
| iter | 2.103 | 2.163 | 2.281 | 248.069 |

## Wasm runtime, milliseconds

Same Wasmtime host, including ordinary startup. Precompiled samples are retained separately.

| Program | Baseline | Candidate | Rust | MoonBit |
|---|---:|---:|---:|---:|
| hello | 4.540 | 4.475 | 5.780 | 4.636 |
| fib | 46.022 | 46.010 | 55.171 | 78.501 |
| btrees | 115.473 | 114.437 | 693.171 | 714.550 |
| snorm | 131.917 | 131.604 | 109.349 | 230.219 |
| wordfreq | 266.298 | 269.132 | 106.793 | 245.147 |
| strchurn | 230.907 | 231.328 | 149.621 | 826.843 |
| jsonround | 341.938 | 341.600 | 215.522 | 661.783 |
| iter | 17.403 | 17.960 | 8.556 | 1372.550 |

## Compile time, milliseconds

| Program | Target | Profile | Baseline | Candidate | Rust | MoonBit |
|---|---|---|---:|---:|---:|---:|
| fib | native | default | 195.408 | 194.574 | 65.127 | 131.215 |
| btrees | native | default | 277.010 | 275.318 | 78.045 | 130.872 |
| jsonround | native | default | 467.966 | 464.060 | 188.754 | 276.164 |
| fib | native | release | 3029.426 | 3033.462 | 2907.282 | 229.297 |
| btrees | native | release | 3122.420 | 3116.412 | 2909.020 | 267.816 |
| jsonround | native | release | 4539.672 | 4518.996 | 3628.061 | 1895.041 |
| fib | wasm | default | 13.876 | 13.525 | 72.713 | 90.326 |
| btrees | wasm | default | 15.297 | 15.079 | 79.801 | 89.528 |
| jsonround | wasm | default | 167.226 | 167.349 | 136.862 | 147.846 |
| fib | wasm | release | 13.794 | 13.382 | 75.148 | 89.462 |
| btrees | wasm | release | 15.238 | 15.093 | 82.875 | 90.912 |
| jsonround | wasm | release | 168.365 | 166.625 | 389.077 | 146.170 |

## Validation and evidence

- Language suite: **4,987 tests / 591 files**, 585 wasm and six native fallback files, zero failures.
- Rust test results: `{"codegen": {"failed": 0, "ignored": 0, "passed": 89}, "mir": {"failed": 0, "ignored": 0, "passed": 283}, "miri": {"failed": 0, "ignored": 0, "passed": 3}, "native": {"failed": 0, "ignored": 0, "passed": 30}, "reuse-declarations": {"failed": 0, "ignored": 0, "passed": 2}, "source-borrow": {"failed": 0, "ignored": 0, "passed": 7}, "str": {"failed": 0, "ignored": 0, "passed": 25}, "witness": {"failed": 0, "ignored": 1, "passed": 1}}`. The three new raw-memory tests also pass Miri nightly-2026-08-01.
- Ownership coverage: **6,386 identities**, exact set equality against the recorded baseline, with the existing floor and stdlib coverage unchanged.
- All original-input runtime outputs agree across baseline/candidate/Rust/MoonBit on native and wasm; snorm agrees at 12 decimal places. Precompiled wasm, wasm-opt and stripped-native outputs also agree.
- The initial new test incorrectly expected empty stderr from explicitly enabling the retired v0 path; it now checks the exact expected notice. The initial failure and corrected pass are preserved.
- Evidence: `~/workspace/github.com/almide/almide-references/bench-sources/string-reuse-2026-11-07/`, including `raw/`, protocol scripts, exact samples, changed compile inputs, compiler and program hashes, allocation diagnostics and logs. All 2,615 archived files were verified locally.
- Archive SHA-256: `9660effa59891b4c7926f38a81500eee49ce0cf473dca97cf00de0131657bd7a`.
- VM and boot disk deleted after retrieval and verification; absence confirmed in `lifecycle.json`.

The owner prioritizes wasm optimization before further native work. The next investigation starts with wordfreq grouping, then strchurn. The broader goals remain open: native default compile time is still above Rust, and the remaining wasm losses need separate work. Almide LIR / the Wyve layer remains design-only pending owner review.
