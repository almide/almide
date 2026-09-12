# Issue fixes: research and evidence (2026-09-12)

Baseline: `edbd7b751e4902e7ea3daff622757d35c71ded13`.
Implementation changes remain local; no issue has been closed or release published.
The prerequisite C-214 judge amendment is submitted separately as
[almide/als#65](https://github.com/almide/als/pull/65).

## Changes

| Issue | Change | Regression evidence |
| --- | --- | --- |
| #2114 | Grow stdin lines through the existing owned-string append helper; stop at LF/EOF, preserve the shared cursor, and match native CR trimming. | `tests/io_read_line_boundary_test.rs`: native, core WASI, direct component; lengths 4094–4097, 8193, 65537; UTF-8 across the former boundary; CRLF, EOF, subsequent line/byte/drain reads. |
| #2112 | Accept borrowed slices in both Value collection constructors; copy into the owned Value payload. | `spec/wasm_cross/value_borrowed_constructors.almd`, attached to existing C-103: parameters, empty inputs, nested objects, and snapshot independence. Native/wasm/interpreter agreement. |
| #2107 | Ignore comments in named-record lookahead and accept newline/comment trivia before a record pattern closes without a trailing comma. | `tests/record_comment_parse_test.rs`. |
| #2106 | Preserve case comments as nonsemantic variant metadata; preserve expression-member comments using the existing ExprId table. Print comment-bearing containers on physical lines. Named and anonymous records share a field parser. | `tests/fmt_container_comments_test.rs`: variant spellings, records, spread records, calls, tuples, conservation and idempotence. Existing formatter/parser suite. |
| #2113 (partial) | E015 requires exact names, excludes host/effect candidates, explicitly states that bodies were not compared, and offers no replacement snippet. Outline recognizes bundled `args`. Direct component builds reject unavailable emitted host operations with E081, naming the operation and p2/p3 target before writing output. | `tests/reimpl_lint_best_match_test.rs`, updated E015 diagnostic fixtures, `tests/component_availability_test.rs` (p2/p3 argv refusal, existing output preservation, preview1 argv acceptance). |
| #2108 | Replace retired `0..n` with `0..<n` in CHEATSHEET. | Source review against the surrounding range examples. |
| #2109 | Diagnose rejected range-pattern, range-type and prefix `..` / `..=` forms with E031 and grammar-specific guidance, without mechanical rewrites. Preserve valid rest markers. | `tests/retired_range_context_test.rs`: issue reproductions, operator spans, no unsafe fix-it, legal list/record rest and current range spellings. |
| #2110 | Add structural library emission with an initialization-only `_start`; require every public export and its reachable callees to lower. Route main-less builds through it while ordinary program emission still requires main. | `tests/wasm_library_route_test.rs`: full board-parser repro, default/forced structural routing, export presence, Wasmtime calls, strict rejection of an unlowerable public body. Existing export DCE regressions pass. |
| #2111 | Replace the stale HTTP stub proposal with the stock/embedded/component distinction and the function-level availability authority. | `proofs/target-availability.toml` and existing component/embedded implementation. |

Before/after reproduction: the pre-change release binary returns **4096** for
4097 ASCII bytes; the changed binary returns **4097**. The Value fixture fails
with E0308 on the pre-change binary and passes on the changed implementation.

## Design references

Read from the requested `../almide-references` checkouts:

- Koka `3ac4f001ab7277b484d661fdbada1aaf8d01ecbf`,
  `kklib/src/os.c`, `kk_os_read_line`: a fixed scratch buffer is repeatedly
  appended until newline/EOF. Its buffer size is not a line-length limit.
  Almide retains its own EOF/CR conventions and existing geometric append
  allocator rather than adopting Koka's error surface or C-string assumptions.
- Go `e51216de8e26247ee0f3d2cfa576233b0d29f542`,
  `src/go/ast/commentmap.go`: comments are associated with nodes in source
  order. Almide likewise keeps trivia out of semantic serialization and
  distinguishes same-line trailing comments from leading comments.
  Its `src/go/parser/parser.go`, `errorExpected`, also grounds rejected
  syntax in the current token. E031 extensions use the enclosing Almide
  grammar and the actual token span; unsupported forms do not acquire
  the expression operator's machine-applicable rewrite.
- Rust `0d31508599a7814a7044e9a7a871e3dc5f037753`,
  `compiler/rustc_mir_build/src/builder/expr/as_place.rs`: conversion of a
  borrowed projection to owned storage uses `to_vec()`. The Value boundary
  follows that ownership distinction; it does not require a borrowed Vec.
- Zig `lib/std/os/wasi.zig` explicitly binds argv/environment to
  `wasi_snapshot_preview1`; those services cannot be inferred for a
  component world. The component audit uses the operations implemented
  by each direct shim, preserving p3 filesystem and HTTP capabilities.
  `lib/std/wasm.zig` models function/memory exports independently of the
  host entry point. The structural library mode follows Almide's existing
  #881 ABI documented in `almide-mir/src/pipeline.rs`: `_start` initializes
  globals, and public functions remain host-callable export roots.
- Rust `library/std/src/thread/scoped.rs` ties borrowed captures to a
  lifetime bounded by joining every child before scope exit. This illustrates
  why capture-clone elimination must establish non-escape/lifetime bounds;
  merely observing read-only access is insufficient for returned closures.

## ALS and quality

The judge's CONTRIBUTING instructions and ledger at the existing pin
`52af321db00a452aa41d591a3ec4591354c8dbd8` were read. No contract statement,
contract ID, or ALS pin was changed. C-103 gains implementation evidence;
its bidirectional fixture links and generated documentation were refreshed.
The contract gate, pinned-ID gate, and syntax-element coverage gate pass.

Codopsy **2.2.0**, using the existing configuration: the commissioned wasm
crate remains **A / 91**. Syntax, frontend and tools remain in A. Do not
interpret those scores as a repository-wide A: an isolated HEAD archive
already measures **B / 82** for `crates` and **B / 86** for `src`; those
aggregate scores were unchanged before the codegen cleanup; the latest
`crates` aggregate is B / 83. No thresholds or exclusions were relaxed.
The codegen crate started at B / 89 (confirmed at the isolated baseline).
After replacing the partial variable-reference traversal with the common
free-variable analysis and propagating runtime-registry I/O errors, it
scores A / 91. Missing include files and declared kernel modules now fail
with the source path rather than panicking without context or being silently
omitted. `tests/runtime_registry_io_test.rs` covers both missing-source cases.
The repository-wide aggregate A requirement remains outstanding.
After this cleanup, all 13 codegen snapshots, the native fold/index-block
regression, both registry I/O regressions, and all 438 Almide files pass.

## Validation

- Final release build: `almide test` passes all **438** files, **425** through
  wasm and **13** through native fallback. Loopback TCP tests require execution
  outside the filesystem/network sandbox; they pass there.
- Formatter/parser regression suites: **156** Rust tests pass, including
  formatting copies of the existing single-file corpus and checking the output.
- The six diagnostic-harness checks pass (the changed hint expectation was
  updated, then its failing check rerun); E015/outline has five focused tests.
- The stdin boundary matrix passes on native, core WASI and direct components.
  The new Value fixture passes both cross-target and interpreter-oracle gates.
- Contract ledger, ALS pin, ALS syntax coverage, wasm file discipline and
  `git diff --check` pass. This is not a run of every repository CI workflow.
- After #2109: parser/formatter/context suites pass 156 tests, the two
  existing range-spelling CLI checks pass, and the context tests also
  pass with exact diagnostic-span assertions. Rebuilt release passes
  all 438 Almide test files again. Syntax Codopsy remains A / 95.
- After the #2113 component build audit: the new rejection/output-preservation
  test passes, all 12 existing p2/p3 component tests execute and pass,
  and the rebuilt release again passes all 438 Almide files. The changed
  wasm-run crate scores A / 96 with Codopsy 2.2.0.

## Remaining work

### WASI version verification

On 2026-09-12, the official [WASI release](https://github.com/WebAssembly/WASI/releases/tag/v0.3.1)
is **0.3.1**, released 2026-08-11. It requires Component Model maps,
implements and external-id support in addition to the 0.3.0 async features.
The checked-in `crates/almide-wasm-run/wit/p3/world.wit` explicitly targets
**0.3.0**. A version string alone neither proves nor disproves compatibility
with a newer host; full 0.3.1 conformance has not been demonstrated.

The existing p3 implementation is operational: `cargo test --test
component_p3_test -- --nocapture` passed all **11** tests on Wasmtime
**47.0.3**, with no execution skips. Coverage includes stdout/stderr/exit
agreement, stdin, deterministic fan, filesystem reads/writes, concurrent
prefetch and cancellation, structural routing, HTTP and framed HTTP, and
recovery after transport errors. The tests use `ALMIDE_COMPONENT_P3=1`
and `--component`. Default direct components still select p2. Stale comments
claiming p3 filesystem writes were pending have been corrected.

### Open issues

- #2103: C-214 in the previous judge explicitly promised `exec failed:`.
  Its normative amendment is [almide/als#65](https://github.com/almide/als/pull/65),
  commit `2490704`, prepared in `/tmp/als-issue2103`. ALS-R5, C-214 and the
  missing-command fixture are changed together; the exact timeout-fire
  message is preserved. All local judge gates pass, including reference
  independence/kernel/totality and runner self-tests. The author/verifier
  record explicitly states no independence. Required CI passed and the
  amendment merged as `4fa62a2211d71823b198dbd4076f881503bf4729`.
  Separate commit `9e29ef1af` advances the implementation pin; the runtime
  status twins now implement the amended command-identifying spelling. The incoming develop
  changes already implement C-348's checked byte slices.
- #2098: closure-construction capture copies and enumerate materialization
  still need broader optimization work. Scalar enumerate/fold now removes
  the tuple list and per-element tuple allocation when the tuple cannot
  escape, using a scalar snapshot to preserve eager-source semantics.
  `research/benchmark/perf/spectralnorm/compare.py`
  now compares three spellings on both targets with output agreement,
  warmup, nine round-robin samples, and an optional enforced ratio budget.
  At n=1200, native indexed fold costs 2.36× and wasm enumerate fold 3.21×
  relative to the checked-in imperative corpus baseline. See its README
  for medians and limitations; this is not an optimization-completion claim.
  After this first optimization, wasm enumerate measures 129.72 ms / 1.17×
  its imperative baseline, down from 365.81 ms / 3.21×. Empty/scalar and
  escaping-tuple/heap-element fallback cases are tested against native in
  `tests/enumerate_fold_test.rs`. All 438 Almide files pass; wasm Codopsy
  remains A / 91. Native synchronous scalar folds now borrow read-only
  captures through a proven immediate iterator call; escaping and shared
  mutable captures keep their existing ownership. Iterator collector
  closures are unboxed for the Rust iterator ABI. The indexed benchmark
  now measures 58.28 ms / 1.42× on native, versus 97.39 ms / 2.36× before.
  `tests/fold_capture_borrow_test.rs` checks output, escaping/mutable
  behavior, empty input, and absence of generated capture-copy bindings.
  The three existing capture/borrow suites pass six additional tests,
  all 13 codegen snapshots pass (the direct-fold snapshot was reviewed
  and updated), all four native differential checks pass, and all 438
  Almide files pass again. Broader capture borrowing remains.
- #2113: normal direct component builds now reject unsupported host ops
  with a named diagnostic. The generic runtime refusal remains for callers
  using the low-level transforms directly; its stderr/exit behavior needs
  separate runtime work. E015 and outline are fixed.
- #2110's board parser now checks and builds without main (31,517-byte
  stock-WASI artifact in the manual reproduction). Both the export
  inventory and calls exercising RateLimited/BoardNotFound pass on Wasmtime.

### Integration with develop at dc2f934df

The initial fixes and separate ALS pin commit were published on
`fix/issue-regressions-20260912` before integrating the incoming develop
changes. Preserve its checked byte slices, ownership fixes, process/fs
diagnostics, and stream fusion. Move the complete free-variable check to
the newly extracted statement-lowering module. Capture-free folds stay on
the existing fusion route: promoting them early would insert a collection
between a map/filter chain and its fold, caught by the pipe-chain snapshot.

After integration, all 440 Almide test files pass (4,081 tests; 428 WASM,
12 native fallback). The 36 focused Rust integration and snapshot tests
pass. Contract traceability covers 348 contracts and 704 symmetric fixtures;
the ALS pin, pass roster, file discipline, and layout checks pass. Codopsy
v2.2.0 measures codegen A/91 and wasm A/90 without changing thresholds.
These module scores do not certify rank A for the whole repository.

### Process status diagnostics (#2103)

Following the merged ALS-R5/C-214 amendment and separate pin advance, both
status functions identify their call and double-quoted command, escape
embedded quotes/backslashes/control characters, and omit the argument list.
The timeout twin includes the supplied millisecond bound. Host error text
remains verbatim; deadline errors retain `exec timed out after <ms>ms`.
The local Rust reference (`library/std/src/sys/process/unix/unix.rs`, spawn
at lines 64–68 and 148–150) preserves distinct InvalidInput and host errno
errors. This implementation decorates that error instead of reconstructing
or classifying it. The ordinary process functions retain their existing
quoting contract; this amendment governs the status twins.

The process matrix now includes both status functions. A compiled fixture
checks quotes, backslashes, CR/LF/tab, Unicode, omitted private arguments,
and the actual host error suffix. Seven focused tests pass, including the
orphaned-pipe deadline regression. All 440 Almide files pass (4,082 tests).
Only the changed timeout fixture AST hash is refreshed. The earlier Value
constructor fixture also gains its missing AST/check/run manifest entries,
measured from its emitted AST, successful check, and WASM output.

The first PR CI run exposed stale E015 diagnostic hashes in 14 existing
fixtures. All retain exit status 0. Ten contain the revised signature-only
warning; four lose fuzzy-name matches (`c_sqrt`/`c_floor`/`c_ceil`, `parse1`,
UFCS helper names, and `bo`). Their source declarations and the previous
substring/edit-distance rule were reviewed. Refresh only these measured
rows; no diagnostic exclusions or comparison floors are relaxed.

## Second pass: the CI run's own findings (same day)

Five red gates on the first two PR runs were each a real defect, not flake.

| Gate | Defect | Change |
| --- | --- | --- |
| Commissioned wasm gates | the new `value_borrowed_constructors` fixture was in the spine/AST manifests but in neither size ledger nor the allocation ledger | its three rows added, measured; every other row left alone (a full regeneration would have claimed unrelated drift in the same commit) |
| Test Rust shard 3 | `pass_capture_borrow.rs`'s `Reads` visitor matched `expr.kind` with `_ => {}`; the traversal totality lint reads the shape, not the following `walk_expr` | rewritten as guards, semantics unchanged |
| Test Rust shard 0 | #2106 forced physical lines whenever a member carried ANY leading comment, breaking #1404's rule that an inline `/* */` stays on its node's line | the trigger is now "a comment that cannot share a line" (`//`, or a block comment spanning lines); `fmt_expr_sans_leading` prints the inline ones |
| Coq / TCB | the extracted checker measures 1,348 lines under Rocq 9.1.1 and 1,196 under the older Rocq in `develop`'s cached opam root | the block regenerated from a LOCAL extraction (`proofs/build-checker.sh`, Rocq 9.1.1) that reproduces CI's number |
| Emit & Format | Hello, world lost 7 bytes when the op-31 shim left the transform | `scripts/gen-readme-stats.sh --measure` |

## #2116 — `io.read_all` was bounded by the park, not the heap

Measured before the change: native reads any size; stock WASI refuses at
**257,024 B** wearing `Error: host op unsupported in the WASI build`; a p2
component dies at **326,657 B** with empty stdout, empty stderr and exit 1 —
the symptom #2113 reported from a Cloudflare Worker, reproduced under plain
`wasmtime run`.

`io.read_all` now takes the stream in 4096-byte chunks off the shared cursor
(op 35, served by all three shims) and appends into the owned-string
accumulator, the way #2114 lowered `io.read_line`. All three legs agree at
every size measured, to 1 MB in `tests/io_read_all_capacity_test.rs` and to
100 MB by hand. **Host op 31 is retired** from the guest, the p1/p2/p3 shims,
the embedded host and `P1_SERVED_OPS`; with it go two of the three sites that
shared the unsupported-op message, and the `g_pcap` global. The env.set
overlay's refusal — the third — now names itself.

The reference read: Zig `e4cbd752c`, `lib/std/heap/arena_allocator.zig`'s
`resize`, which extends a buffer in place only when its end coincides with the
arena's frontier and refuses otherwise. That precedent belongs to #2117's
remaining half rather than to this change; it is recorded on the issue.

## #2117 — the accumulator above the largest size class

`$alloc` rounds to a class only below `16 << 15`, and `$free` abandons any
block whose total is ≥ 2^20, so a string past 512 KB reallocated on every
append and leaked each outgrown copy: 5.5 MB of accumulator exhausted the
address space and took C-197. `$str_append` now asks for double the capacity
above that ceiling — `$list_push`'s and `$map_reserve`'s own policy — leaving
everything below it byte-identical. `io.read_all` reaches 100 MB in 0.08 s;
`acc = acc + s` builds 8 MB in 0.01 s. `tests/string_accumulator_growth_test.rs`
requires both legs to finish under a 32 MiB `--heap-cap`; reverting the
allocator change alone turns its wasm leg back into `Error: out of memory`.

The size ledgers are re-claimed in their own commit: 197 emitted modules pay
the geometric append (+0.90 % aggregate) and 509 shipped modules lose the
op-31 shim (−149 B each, +0.59 % aggregate after the codec fixtures' growth).

## #2098 — the spelling tax is now gated

`scripts/check-spelling-ratio.sh` compares spellings of one program on one leg
in one round-robin run, so the ratio cancels the runner exactly as the
native/Rust ratio gate does. Measured at n=1200, five samples, six artifacts
byte-identical on stdout: native `1.00 / 1.39 / 1.70`, wasm `1.00 / 0.89 /
0.72`. The 4.25× this issue opened on is now **0.72×** — the documented
spelling is the fastest of the three on the structural leg. The gate runs in
the perf-ratchet job at 2.5×, above the native figure with room for noise.

## Filed, not fixed

Three defects of the same family were measured, reproduced and filed rather
than rushed into this change: #2118 (op 32 writes `b_len` bytes into the park
with no bound — unreachable from source today, every stdlib caller passes 8),
#2119 (the direct component's `cabi_realloc` exits silently on a failed grow,
against C-197's form), and #2120 (`env.get` answers `none` and `env.args`
answers `[]` above the park's 261,120-byte staging room, where native returns
the value — a silent wrong answer, and the worst of the family).
