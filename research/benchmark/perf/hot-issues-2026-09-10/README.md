# Hot native issues, 2026-09-10

Design and measurement notes for #2077–#2080. Machine: Apple M3, macOS.
Compiler base: `14e667fdc`, with the changes in this PR. Research results are
local experiments, not universal speedup claims.

## Decisions

- **#2077:** `string.slice` retains clamped codepoint semantics, but stops scanning
  the unrequested suffix. It still walks the prefix. New `string.byte_slice`
  returns an owned `String?`: negative, reversed, out-of-range or non-UTF-8-boundary
  offsets return `none`. Valid boundaries cost O(1) to check; copying costs O(window).
  Native uses Rust `str::get`; the self-host uses checked byte bounds and UTF-8
  continuation bits. It is admitted through the structural SUM-builder tier because
  its result is an Option, rather than bypassing the layout certificate.
- **#2079:** accept `#!` only at the logical beginning of the source, after existing
  BOM removal. Keep it as trivia so source positions survive; format it before
  dialect stamps and imports. OS execution still needs the executable bit, a
  working interpreter path and an OS supporting the selected `env` invocation.
- **#2080:** default native build was accepting fan through MIR's serial WASM
  lowering, although `emit` showed scoped threads. Refuse fan blocks and fan.map /
  fan.any_map before lowering erases their boundaries, letting the existing native
  codegen handle concurrency. This does not claim every effectful map callback is
  parallelizable or close the broader #2044 work.

Reference source was read from `../almide-references/rust` at
`0d31508599a7814a7044e9a7a871e3dc5f037753`: `compiler/rustc_lexer/src/lib.rs`
(`strip_shebang`) and `library/core/src/str` (checked boundary slicing).
The Zig reference is pinned to 0.15.2, commit
`e4cbd752c8c05f131051f8c873cff7823177d7d3`.

## Native fan reproduction

Build `fanblock.almd` with the baseline and fixed compiler; warm each binary and
measure `/usr/bin/time -p`. Baseline real/user: 0.52/0.51–0.52 s. Fixed warm
real/user: 0.09/0.64 s. Both print `fanblock 4389696`. Initial cold fixed run was
0.60 s; it is not mixed into the warm comparison. This is a small manual probe,
not a stable timing threshold in CI. Routing and behavior have regression tests.

## #2078 remains open

`gramide.json` records three interleaved warm runs, fixture/revision/compiler
hashes, and matching full token-output hashes. The Go fixture is 2,973,586 bytes,
not the issue's 3.95 MB fixture. `gramide-compact-spans.patch` applies to gramide
`c6933d2ef93b9c5890540b1b57910da9d90d8b73` and is an experiment, not a published
client change (apply with `git apply --unidiff-zero`). Build each variant with the same compiler; run `gramide check`
and compare `gramide tokens` output on the recorded Go source.

| Variant | Median real | Median peak RSS |
|---|---:|---:|
| Original List[Int] source buffer | 0.49 s | 122.75 MiB |
| Bytes source buffer | 0.49 s | 116.44 MiB |
| Bytes buffer and bytes.slice for spans | 0.48 s | 103.09 MiB |

Bytes already stores compact u8 elements; its string conversion copies once.
It does not make the whole process eight times smaller. These few samples show
no convincing runtime win, and RSS varies between samples. Owned token strings
and record allocation still need profiling and representation work. Documentation
now correctly describes the copy instead of claiming zero-copy conversion.

## Zig arena attribution, not a semantic change

`math-attribution.json` contains seven shuffled interleaved runs per large size,
seed 42, and matching displayed checksums at sizes 10, 23 and 24. Input is
`../listbuild/listbuild_prealloc.almd`. Compare default build, `almide emit` plus
rustc, and an experimental emitted-source variant changing only the libm sin/cos
wrappers to `x.sin()` / `x.cos()`. Rust flags: `--edition 2021 -C opt-level=3
-C lto=yes -C codegen-units=1 -C overflow-checks=no`.

At size 24 medians are 0.40986, 0.41457 and 0.21267 s respectively. Changing the
native producer does little; the math implementation explains a large part of
this workload's cost. These are attribution runs, not a fresh interleaved Zig
comparison. Displayed checksums do not establish bitwise equivalence for all
inputs. `runtime/rs/src/math.rs` intentionally uses vendored musl libm for
cross-platform determinism. Do not ship the platform-math substitution. A follow-up
must optimize deterministic sin/cos (for example shared range reduction) with
numerical and cross-target evidence before claiming an arena win.

## Validation

- `tests/hot_native_regressions_test.rs`: initial-only shebang, positions,
  formatting and native concurrency routing; `tests/fan_surface_matrix_test.rs`.
- `spec/cli/shebang.almd`: check, compile, test, format and direct OS execution.
- `spec/wasm_cross/string_byte_slice.almd`: C-348 Unicode byte boundaries, invalid
  ranges, empty ranges and existing codepoint slicing, on native and WASM.
- 440 native test files passed across the sweep and nine network-enabled retries.
  Default sweep plus two network-enabled retries also passes all 440 files.
- Release allocation ledger and structural witness floor pass; existing allocation
  rows are unchanged, with one new fixture row. Docs generation check passes.
- Codopsy codegen and WASM both retain A (90/100), existing 20/30 thresholds.

## CI integration follow-up

The new fixture uses explicit Option matches so the incumbent MIR host harness
also compiles every case; the expected results and invalid-boundary cases are
unchanged. Native, wasm32-wasip1 and the browser ABI emit identical 17,620-byte
incumbent modules for it, with no new refused fixture. The structural emitted /
WASI sizes are 5,098 / 5,608 bytes. Only its rows are added to the size baselines;
existing rows and refusal ceilings remain unchanged. The new API is registered
as pure, in the exercised surface, in IDE snapshots and in the contract tables.
The native concurrency refusal now carries its source span, checked by a regression
test and the spanless-wall ratchet.
