# gramide allocation experiment for Almide #2078

2026-09-10, Apple M3/macOS. Apply `gramide-cached-operators.patch` with
`git apply --unidiff-zero` to gramide `c6933d2ef93b9c5890540b1b57910da9d90d8b73`.
The client patch is published as [gramide PR #1](https://github.com/O6lvl4/gramide/pull/1).
It does not redesign Almide String or Token representation. It uses the existing compact Bytes API and
precomputes operator byte patterns once per tokenization. Only a successfully
matched operator clones its text. The source helper is named `source_byte_at`
to avoid a test compilation collision with balance.almd's List[Int] helper `at`.

The lexer formatter refuses the file because its safety check reports six lost
comments; do not force that formatting or remove the safety check.

## Measurements

The fixture is Go 1.26.1 `src/cmd/compile/internal/ssa/opGen.go`, 2,973,586 bytes.
The SHA256 is recorded in both JSON files. `check-results.json` records seven
shuffled, interleaved `/usr/bin/time -l` runs (seed 2078) after warmup. Full token
output hashes are identical for baseline, compact spans and cached operators.
The displayed checksum in the allocation probe is 1142484379396 for every variant.

| Variant | Median check wall time | Median peak RSS |
|---|---:|---:|
| Original | 0.51 s | 123.34 MiB |
| Compact source and span buffers | 0.51 s | 103.25 MiB |
| Also cache operator patterns | 0.25 s | 103.16 MiB |

The timing binaries are uninstrumented. The measurements precede the helper's
name-only collision fix; final token-output equality and project tests are checked
separately. These are local pilot measurements, not a language-wide speedup claim.

## Allocation attribution

Copy `bench_lex.almd` into gramide/src. It reads the same file, lexes Go and consumes
all token positions and text/kind lengths; no grammar compilation or parsing.
Emit it with Almide, rename its `fn main()` to `fn probe_main()`, append
`counting-allocator.rs`, and compile with rustc `--edition 2021 -C opt-level=3
-C lto=yes -C codegen-units=1 -C overflow-checks=no`. The allocator counts alloc
and realloc requests and requested sizes; sizes are cumulative, not peak live
memory. Atomic instrumentation changes timing, so do not time these binaries.
Counters include process initialization, source reading and the checksum output.

Original: 22,961,527 requests / 307,280,840 requested bytes.
Compact spans: 22,743,986 / 268,361,639.
Cached operators: 2,612,801 / 117,801,931 (about 88.6% fewer requests).

The large reduction identifies repeated operator candidate construction as a
major cost on this input. Owned token kind/text strings still allocate; this
experiment does not claim every representation problem in #2078 is solved.

Final helper-renamed patch: all 9 project test files pass (2 WASM, 7 native fallback); full token-output SHA256 still matches. See `final-validation.json`.

## Reference checkpoint

Inspected the local reference compilers before choosing the next representation
step: Rust `0d31508599a7814a7044e9a7a871e3dc5f037753`,
`compiler/rustc_lexer/src/lib.rs` (`Token`, `TokenKind`, `advance_token`), and Zig
0.15.2 `e4cbd752c8c05f131051f8c873cff7823177d7d3`,
`lib/std/zig/tokenizer.zig` (`Token`, `Tag`, `Tokenizer.next`). Both identify fixed
punctuation with token tags and direct character/byte branches, rather than
rebuilding candidate strings in the scan loop. Gramide accepts language specs as
values, so precomputing their byte patterns preserves its configurable ordering
without introducing a fixed-language scanner. Tag/span token representation is a
separate, broader API change; it is not needed for this measured reduction.

Hew checkpoint: cloned hew-lang/hew into ../almide-references/hew at
`a78df3b9e3b9e0558e63caa76e14d2aba4f39e4f`. `hew-lexer/src/lib.rs` uses
Logos compile-time DFA tokenization and `Token<'src>` with borrowed `&'src str`
string-literal payloads. This provides a third concrete reference for eliminating
per-token owned text; it does not establish that Hew development uses gramide.

Hew corpus compatibility: compared the original and final patched gramide
`tokens` output, stderr and exit status on all 1,282 Rust source files at that
revision. All 1,282 results match; 1 file is rejected by both versions. This is
a Rust-source compatibility check, not support for the Hew language grammar.
See `hew-token-equivalence.json`.
