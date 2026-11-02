# Effect inspection harness

## Adopted design and implemented harness

The October 12 problem counts and order of work below are a design snapshot.
[ADR-0027](../../adr/0027-effect-functions-are-inspected-in-a-fixed-host-world.md)
adopted the inspection rules on October 17; the final owner question below is
not an undecided policy request. The current
[catalogue](../../../tools/xtarget-fuzz/src/generator/catalogue.rs) admits
supported signatures from sandboxed fs/env/io/args/process/net/http modules,
hostless zlib functions and property-verdict functions. It does not skip all
effect functions. Admission remains subject to signature and target filters;
this is not a claim that every effect function has passed inspection.
Use [the inspection ledger](../../../proofs/stdlib-inspection.toml) and
[target availability](../../../proofs/target-availability.toml), not the old
133/36 counts, for per-function state.

**Status:** design, 2026-10-12; decided in [ADR-0027](../../adr/0027-effect-functions-are-inspected-in-a-fixed-host-world.md) (2026-10-17).
**Tracks:** #3552 (the stdlib inspection campaign).

## Problem

The stdlib inspection ledger (`proofs/stdlib-inspection.toml`) wants every public function inspected against five criteria. One of them is that the function behaves the same on native and wasm. The criteria and the `class = "effect"` row field already exist, and for effect rows they require parity on the embedded host and on a stock WASI runtime.

The program generator (`tools/xtarget-fuzz`) skips effect functions altogether (`signature_from_decl`). So **133 of the 458 uninspected functions** cannot be inspected at all, whatever their quality.

| Module | Uninspected effect fns |
|---|---|
| fs | 34 |
| http | 28 |
| net | 24 |
| process | 13 |
| env | 8 |
| zlib (file forms) | 8 |
| args | 6 |
| io | 6 |
| random | 4 |
| datetime | 2 |

## What was consulted

- **`../almide-references`.** `RESEARCH-verification.md` covers differential fuzzing (V-3, V-4), but only for pure programs. I found nothing on testing effects, sandboxing, or WASI conformance. **Material for this design is missing there; the design rests on the patterns below.**
- **This repo's existing cross-target effect tests.** `tests/wasm_runtime_test_parts/common.rs` and `corpus.rs` run every effect fixture in a fresh `tempfile::tempdir()`, with `wasmtime --dir=/` or a preopened directory and `-S inherit-env=y`. Native runs with that directory as its cwd. The interpreter has its own sandboxed fs (`crates/almide-interp/src/dispatch_vfs.rs`).
- **`proofs/target-availability.toml`.** It lists which effect functions each wasm leg serves. 36 of the 133 (http streaming and `*_response`, all of `net`, `process.stdin_lines`) are unavailable on the embedded host, and 62 on stock p1.

## Design: four tiers, by what makes a run reproducible

### E1: sandboxable and deterministic (about 62 functions)

Covers fs, env (`get`, `set`, `cwd`, `temp_dir`, `args`), args, io (stdin), and the zlib file forms.

- **Fixed world per run.** Every program runs on each leg in a fresh scratch directory seeded with a fixed tree (a few files of known bytes, a subdirectory, a symlink where the host allows it). Env, argv and stdin are fixed too.
  - native: cwd = the scratch dir;
  - embedded host: `almide run --target wasm` with cwd = the scratch dir;
  - stock WASI: `wasmtime --dir=.` plus explicit `--env` and args.
- **What parity compares:** stdout, stderr and exit code, plus a snapshot of the scratch tree after the run (paths, kinds, sizes, bytes). A write that lands differently is a divergence even when nothing is printed.
- **Generator:** `effect fn main`. The fs functions take path arguments drawn from the seeded tree (hits and misses), so error paths are exercised, not just success.
- **Complexity:** the size parameter is the file length (`read_text`, `read_bytes`, `fold_lines`) or the line count; the judge is unchanged. Functions whose cost is the host's (`mkdir_p`, `exists`) are declared 1.
- **Diamond:** the same sandbox setup per run.

### E2: non-deterministic (random, `datetime.now` / `monotonic_ns`, `env.millis` / `unix_timestamp` / `sleep_ms`)

Values cannot match across legs. Parity is judged on **properties**: exit code, type, and declared ranges (`random.int(lo, hi)` within bounds, a shuffle is a permutation, two `monotonic_ns` reads are non-decreasing, `sleep_ms(k)` takes at least k ms). The ledger's parity field names the properties checked, so a reader knows this is weaker than byte equality.

### E3: process (13)

Only fixed, side-effect-free commands are spawned: `/bin/echo`, `/usr/bin/true`, `/bin/cat` with a stdin. Parity compares the captured outputs and statuses. `process.exit` is judged on the exit code alone. `process.kill` and `spawn` get a child that the harness reaps.

### E4: network (http 28, net 24)

A loopback fixture server started per run: an HTTP server with canned responses (including streaming and chunked responses) and TCP / Unix echo servers. No external host, and no API keys (the `anthropic_` / `openai_` streaming calls point at the fixture through their base URL).

**Blocked on wasm for 36 functions** (`target-availability.toml`, the embedded leg): criterion 1 cannot hold for them until the wasi:http@0.3 port (#1710) lands. They stay uninspected. The criterion is not narrowed to "native only".

## Alternatives considered

- **Mock the effects inside the stdlib for testing.** Rejected: the inspection would judge the mock, not the legs' real host bindings, which are exactly where the cross-target bugs live.
- **Record and replay host calls.** Rejected for E1 and E3, which can run for real in a sandbox. It would be useful for E4 only if the loopback fixture proves flaky.
- **Leave effects to the hand-written `spec/wasm_cross` fixtures.** Rejected: those are per-function examples. Criterion 4 asks for fresh generated combinations every run, which found nine defects in one batch of pure functions today.

## Compatibility

- Tool-only: `tools/xtarget-fuzz` and the inspection scripts. No language, stdlib or codegen change.
- The ledger's `parity` field for an E2 row names its property checks.

## Order of work

1. **E1:** sandbox setup on the three runners; tree snapshot in the comparison; an effect `main` in the generator; fs, env, args and io in the catalogue.
2. **E2:** property judges.
3. **E3:** process with the fixed command set.
4. **E4:** the loopback fixtures, starting with the wasm-served http subset.

## Open question for the owner

The 36 functions with no embedded-wasm service: stay uninspected until #1710 (the design above), or get a separate, explicitly native-only stamp? This is not decided here.
