# Stream sum fusion and native compiler startup measurements

Measured 2026-11-07 on one GCE Spot VM. Candidate code: `14edf88215392b50e794171d146278003f9cd2a3`; native probe change: `b1c3d4731`. Baseline source: `4620c89f5` (the carried benchmark executable is from `7d003524f`; the intervening commit changes documentation only).

The wasm `iter` program improves from **98.463 to 12.885 ms (7.64×)**. Rust takes 6.176 ms on the same host: the gap falls from 15.94× to **2.09×**. Its three intermediate list allocations, totaling 240,000,000 requested bytes, become zero allocations/zero bytes. These are results for the measured programs, not a claim of language-wide parity or universal wins.

## Implementation

- `list.sum` shares fold's stage judgment and stream loop: numeric accumulation preserves wrapping Int addition and sequential Float addition from positive zero. Range and enumerate sources use existing loop-source lowering. Intermediate owned values use the existing release and ownership-witness paths on both kept and skipped edges.
- Scalar-returning List forwarders expose a List-producing call argument only when the body passes every parameter exactly once and in declaration order. Bare List variables keep their ordinary call and ownership witness; there is no producer chain to fuse at those sites. This exposes the Numeric forwarding chain without a builtin-name list or benchmark-specific rule. Existing scalar and byte-view inlining policy is retained.
- Captured `MapInsert` is treated as an observable write. A draft failed the stage-order regression (2 instead of 3); the refusal is covered by a regression test. Printing, captured writes, multiple partial stages, wrapping, floating-point order, heap owners, and certificates are tested.
- Native rustc discovery, version floor, and cache identity share one cached `rustc -vV` probe. Cache/profile semantics remain unchanged; fake-rustc tests assert exactly one probe. No LIR/Wyve implementation is included.

## Measurement conditions

- VM `almide-fusion-1107`: n2-standard-8, Intel Cascade Lake, Ubuntu 24.04, 500 GB pd-standard, project `almide-perf`, zone `asia-northeast1-b`. All builds and executions ran on this VM; none ran on macOS.
- Rust 1.99.1 / LLVM 23.1.1, Wasmtime 50.0.1, Binaryen 133, moon 0.1.20261104 / moonc 0.10.20+65efd5b93, GCC 13.3.0. Exact version/CPU output is in `evidence/setup/versions.json`.
- The final candidate compiler itself is built with `CARGO_INCREMENTAL=0`, matching the previous compiler-build procedure. An additional fresh target-directory build independently reproduced the same behavior. Early attempts inherited the repository's incremental release setting; those measurements are retained under `prior-attempts` and are not the progress-page data.
- One excluded warmup, seven measured rounds, alternating/reversed language order. All medians below are milliseconds unless stated otherwise. The source constant reaching the IR changes on every timed build; per-build source and rustc argv are retained. Huge-constant compile-only outputs are never executed. Runtime uses separately built original-input artifacts.
- Native default: Almide/Rust opt-level 1, debug assertions on, overflow checks off. Native release: both O3, fat LTO, codegen-units 1, x86-64-v3, `-force-ordered-reductions=false`.
- Both Almide compilers have the same independent `almide-verify` installed for the final build comparison: certificate recording/checking is enabled on both. Earlier apparent wasm compile regressions compared a baseline without this companion binary against a candidate with it. Equalizing the verifier removed that mismatch; the final table includes the verification cost. Baseline wasm modules for all eight inputs remain byte-identical with the verifier installed, so the original runtime samples remain valid. `verifier_sha256` and `verifier_artifact_identity` in results.json record this check.
- Primary native comparison uses ordinary std on both sides (`ALMIDE_RELEASE_STD_UNSPLIT=1` for Almide); logged Almide program rustc invocations are checked to contain no custom `--sysroot`. The existing std-split feature is measured separately below.
- MoonBit uses its product debug/release settings (native release C/GCC O2), so its optimization settings differ from Almide's fat LTO. Wasm uses each product's settings: Almide direct wasm, Rust debug/O3, MoonBit linear-memory `wasm`, not wasm-gc.
- Dependencies/runtime caches are prepared; Rust jsonround's serde_json is built before timing under each relevant profile. This is edited-program latency, not an empty-cache dependency build.
- All languages run wasm through the same Wasmtime. Ordinary process times include host startup/JIT; AOT times are separate. Exit status/stdout/stderr are checked; snorm's cross-language numeric result is compared at 12 decimal places. AOT, wasm-opt, and stripped-native outputs are also checked.

## Build time: final candidate and comparison languages

| Program / target / profile | Before | Almide | Rust | MoonBit | Almide/Rust | Almide/MoonBit |
| --- | --- | --- | --- | --- | --- | --- |
| fib / native / default | 171.727 | 160.430 | 51.872 | 102.437 | 3.09× | 1.57× |
| btrees / native / default | 239.372 | 230.023 | 63.630 | 103.475 | 3.62× | 2.22× |
| jsonround / native / default | 390.302 | 380.003 | 152.513 | 219.810 | 2.49× | 1.73× |
| fib / native / release | 2473.383 | 2463.280 | 2362.659 | 181.451 | 1.04× | 13.58× |
| btrees / native / release | 2554.594 | 2548.760 | 2376.745 | 214.125 | 1.07× | 11.90× |
| jsonround / native / release | 3686.106 | 3665.120 | 2965.809 | 1554.057 | 1.24× | 2.36× |
| fib / wasm / default | 11.081 | 11.311 | 57.090 | 70.499 | 0.20× | 0.16× |
| btrees / wasm / default | 12.530 | 12.613 | 63.449 | 70.982 | 0.20× | 0.18× |
| jsonround / wasm / default | 137.175 | 139.272 | 107.812 | 115.722 | 1.29× | 1.20× |
| fib / wasm / release | 11.092 | 11.157 | 59.654 | 70.263 | 0.19× | 0.16× |
| btrees / wasm / release | 12.562 | 12.514 | 66.283 | 69.922 | 0.19× | 0.18× |
| jsonround / wasm / release | 136.543 | 138.372 | 324.308 | 115.482 | 0.43× | 1.20× |

## native execution

| Program | Before | Almide | Rust | MoonBit |
| --- | --- | --- | --- | --- |
| hello | 0.719 | 0.704 | 0.703 | 0.642 |
| fib | 2.100 | 2.135 | 26.569 | 18.895 |
| btrees | 99.105 | 99.400 | 331.265 | 134.942 |
| snorm | 87.490 | 85.863 | 86.357 | 75.229 |
| wordfreq | 54.042 | 54.385 | 62.448 | 107.713 |
| strchurn | 93.539 | 93.367 | 60.864 | 177.053 |
| jsonround | 168.259 | 166.418 | 159.167 | 172.840 |
| iter | 1.919 | 2.038 | 2.137 | 181.576 |

## wasm execution

| Program | Before | Almide | Rust | MoonBit |
| --- | --- | --- | --- | --- |
| hello | 3.560 | 3.520 | 4.374 | 3.586 |
| fib | 41.554 | 41.824 | 42.395 | 70.403 |
| btrees | 95.778 | 96.964 | 504.179 | 531.734 |
| snorm | 118.008 | 117.804 | 90.355 | 187.052 |
| wordfreq | 164.691 | 166.827 | 90.041 | 212.493 |
| strchurn | 180.000 | 179.836 | 121.445 | 727.262 |
| jsonround | 275.075 | 274.437 | 167.028 | 556.159 |
| iter | 98.463 | 12.885 | 6.176 | 1122.320 |

### Wasm after AOT compilation

| Program | Before | Almide | Rust | MoonBit |
| --- | --- | --- | --- | --- |
| hello | 3.294 | 3.323 | 3.436 | 3.314 |
| fib | 41.311 | 41.353 | 41.324 | 69.652 |
| btrees | 95.204 | 95.376 | 510.506 | 530.992 |
| snorm | 117.383 | 117.559 | 89.106 | 186.424 |
| wordfreq | 163.222 | 163.030 | 89.010 | 211.329 |
| strchurn | 179.573 | 178.627 | 120.630 | 724.555 |
| jsonround | 274.679 | 271.753 | 164.602 | 551.168 |
| iter | 97.185 | 12.550 | 5.226 | 1121.480 |

### Short-process stability follow-up

The original artifacts were run again in 21 alternating measured rounds after a warmup; these do not replace the primary seven-round table. Native emitted Rust is byte-identical on all eight benchmark sources.

| Program / target | Before | Almide | Rust | MoonBit |
| --- | --- | --- | --- | --- |
| iter / native | 1.992 | 1.983 | 2.082 | 180.111 |
| fib / native | 2.071 | 2.087 | 26.572 | 18.692 |
| hello / wasm | 3.440 | 3.470 | 4.350 | 3.559 |

## Existing std-split feature: separate experiment

With llvm-tools installed, the existing split-std cache can remove unused std/formatting IR before fat LTO. This is a configuration experiment, not a new optimization in this patch. Plain and split use the same candidate compiler, fat LTO and input; cache preparation is excluded. Eight original-program outputs match, and the existing panic/backtrace probes succeed before the cache is marked READY. Do not compare these split-std numbers against ordinary-std Rust as a matched-LTO win.

| Program | Ordinary std | Split std |
| --- | --- | --- |
| fib | 2444.603 | 641.316 |
| btrees | 2525.292 | 723.229 |
| jsonround | 3874.291 | 1999.705 |

## Size (bytes)

Native artifacts have debug information removed uniformly with `objcopy --strip-debug`. Wasm artifacts use Binaryen 133 `-Oz --all-features --disable-compact-imports` uniformly. Each transformed artifact is executed and its output checked. The external wasm-opt transform changes the renderer-certified bytes; these size results are output-checked, not a claim of a renewed certificate for the transformed bytes.

| Program / target | Almide | Rust | MoonBit |
| --- | --- | --- | --- |
| hello / native | 370352 | 361984 | 349976 |
| fib / native | 370232 | 364080 | 354128 |
| btrees / native | 381064 | 365736 | 354248 |
| snorm / native | 428968 | 388896 | 366256 |
| wordfreq / native | 445816 | 392384 | 369304 |
| strchurn / native | 425320 | 376072 | 374352 |
| jsonround / native | 559536 | 436432 | 442080 |
| iter / native | 403136 | 364184 | 354632 |
| hello / wasm | 304 | 39781 | 2122 |
| fib / wasm | 399 | 40157 | 3937 |
| btrees / wasm | 1537 | 40742 | 3965 |
| snorm / wasm | 5706 | 63289 | 12059 |
| wordfreq / wasm | 8109 | 65983 | 11725 |
| strchurn / wasm | 11415 | 49817 | 15608 |
| jsonround / wasm | 25647 | 99813 | 53970 |
| iter / wasm | 394 | 40135 | 5246 |

## Verification and limitations

- Language suite: **4,986 passed**, 590 files (584 wasm, 6 native fallback), no failures.
- Wasm library/selected integration tests: **93 passed, 1 existing ignored**. Native rustc-floor/profile/name-key tests: **4 passed**. Arm-args and file-discipline checks pass.
- The full ownership corpus is audited separately. It reports **6386 distinct identities on baseline and 6386 on candidate**, with identical covered identity sets, no removed/added identities and no certificate-checker rejection. The full corpus test **passes** against the unchanged **6,386** floor (one separate test remains ignored). See full logs and `witness-diff.json`.
- The first broad forwarding draft removed the RC-bearing `<lambda#1>` identity from `spec/wasm_cross/fan_block_rc_closure_capture.almd`, reducing the count to 6,385. An initial baseline audit erroneously reused a Cargo artifact because archive mtimes predated its fingerprints. After forcing an actual rebuild, baseline passed at 6,386 and exposed the regression. The final patch restricts forwarding to producer-call arguments, adds a focused captured-list closure regression, and restores the exact 6,386-identity set. Neither the floor nor the covered identities were reduced. Source hashes and the real baseline build log are retained.
- All primary cells are complete: 48 compile, 64 ordinary runtime, 32 AOT, each seven samples. Std-split has 6 compile and 16 runtime cells. Compiler/artifact/source hashes are checked before collection.
- The broader forwarding and incremental-compiler attempts are preserved under `prior-attempts`; their measurements do not replace the final candidate's data. Apparent wasm compile regressions prompted compiler-build and Cargo-artifact checks; the decisive mismatch was the independent verifier being present on only one side. Final compile numbers use it on both sides. A repeated std-split experiment also reused its constant sequence and hit the program cache; that rejected series is retained as `std-split-cached-rejected`. The final std-split run uses new constants, preserves their sources, and requires an actual program rustc invocation for every timed build. No threshold was lowered, no failed test was declared passed, and no new release default or LIR backend was introduced.

## Evidence and cleanup

Permanent local evidence directory: `almide-references/bench-sources/stream-fusion-2026-11-07/` (sibling of the repository). It contains the full archive, receipt, source hashes, raw samples/logs/argv, original benchmark inputs, candidate source files and binaries, and lifecycle verification.

- Archive: `almide-fusion-evidence.tar.gz` (159,611,458 bytes).
- Archive SHA-256: `54b54fba3786993ba9aaf36d7dbe5dee451033633b1f4d7d150082b385cff163`.
- Candidate compiler SHA-256: `5c37e225d19df1c09066690098770256c49e88a45548dbaac11954c7fd978a0f`.
- Every member in the archive's `SHA256.json` manifest was checked after local retrieval.
- VM and boot disk were deleted after successful retrieval/verification; both absence checks are recorded in `lifecycle.json`.

The Downloads progress page shows only these latest measurements in one build table and one execution table per target/opponent. LIR / The Wyve layer remains design-only pending owner review.
