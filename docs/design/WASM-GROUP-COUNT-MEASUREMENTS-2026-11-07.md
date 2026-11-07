# Wasm group counts — 2026-11-07

Commit `3e7bee510` streams `map → group_by(identity) → map(list.len)` into an insertion-ordered count map. An immediate group/count pair also avoids storing the elements in each group. Literal callbacks retain their original evaluation order; computed keys still use the staged source. Range sources are counted rather than allocated, and an effectful callback keeps a snapshot of a list source. Owned keys are moved or released on the respective lookup arms.

On one GCE n2-standard-8 Spot VM, wordfreq fell from **160.180 to 114.763 ms (28.4% faster)**. Rust was **90.348 ms**, MoonBit **212.076 ms**. These are ordinary Wasmtime launches, including startup. Cumulative allocation fell from 44,321,666 to 852,034 bytes (98.1%); all 73,307 candidate allocations were freed. The output is unchanged. The other seven wasm benchmark binaries are byte-identical to the baseline; native lowering did not change.

## Current runtime comparison

| Program | Almide native | Rust native | MoonBit native | Almide wasm | Rust wasm | MoonBit wasm |
|---|---:|---:|---:|---:|---:|---:|
| hello | 0.718 | 0.709 | 0.645 | 3.447 | 4.303 | 3.539 |
| fib | 2.167 | 26.665 | 18.921 | 41.656 | 42.307 | 70.770 |
| btrees | 94.994 | 319.235 | 131.794 | 94.001 | 492.857 | 515.071 |
| snorm | 87.765 | 87.481 | 75.241 | 117.997 | 90.456 | 186.728 |
| wordfreq | 52.796 | 62.527 | 106.451 | 114.763 | 90.348 | 212.076 |
| strchurn | 82.862 | 60.781 | 177.183 | 179.974 | 121.401 | 728.237 |
| jsonround | 163.218 | 157.718 | 171.972 | 272.255 | 167.111 | 555.289 |
| iter | 1.988 | 2.149 | 179.289 | 12.882 | 6.200 | 1122.680 |

All times are milliseconds. Each lane uses seven alternating measured rounds after one excluded warmup. Native Rust uses matched O3, fat LTO, one codegen unit, x86-64-v3 and LLVM settings; MoonBit uses its product release settings. Wasm uses each product's release output on the same Wasmtime 50.0.1 host. Raw data also records AOT execution separately. Rust 1.99.1, moon 0.1.20261104, moonc 0.10.20, Binaryen 133, Ubuntu 24.04; 500 GB pd-standard. Cross-VM absolute timings are not an optimization A/B.

## Validation and evidence

- 96 wasm tests passed (one ignored), including seven new group-count regression tests; the declaration-name gate passed.
- 4,987 language tests in 591 files passed.
- Ownership floor passed, retaining the exact previous set of 6,386 identities. No thresholds changed.
- All eight original programs agree across baseline/candidate Almide, Rust and MoonBit, native/wasm, AOT and size-processing checks. Spectral norm uses the existing 12-decimal comparison.
- Compile timings change a constant that reaches IR on every build. Changed-constant outputs are never executed. Both Almide compiler directories carry the same independent verifier.
- All 2,663 evidence files, sources, compiler and reported artifacts were SHA-256 verified after retrieval. The profiling interval overlaps only build preparation and excluded warmups, not recorded samples.

An initial boundary test incorrectly used Float map keys, which the language rejects; it was corrected to the supported key types. Two ownership tests initially raced on the process-global witness collector; they now share one collection. The later diagnostic script accidentally reused the first diagnostic directory: its two candidate lane labels therefore referenced the same binary. Those provisional lane comparisons are not used in the tables above; the final benchmark has separate baseline/candidate paths and complete raw samples. Initial logs and this limitation are retained.

Evidence archive SHA-256: `7c34628d0ac9848dc89970c6704e0d4f05cdf1f4af29d61bec82798a85e85bd9`.

Local evidence: `almide-references/bench-sources/wasm-group-count-2026-11-07/` (raw data, archive, receipt and verification). The VM `almide-wasmdeep-1107` remains in use for the next wasm experiments; it and its disk must be deleted after those results are retrieved. No Mac builds or tests were run.

## Remaining work

Continue wasm first: profile and measure string-key hashing/equality and immutable scalar constant exposure, then strchurn and the remaining Rust gaps. Keep only improvements supported by repeated same-VM measurements without correctness, ownership, size or compile-time regressions. Then return to native. The LIR/Wyve implementation remains subject to the owner's separate design decision.
