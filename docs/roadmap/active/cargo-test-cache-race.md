<!-- description: Fix parallel-cargo-test cache race in fix_test/run_test that masks real failures -->
# `cargo test --all` Cache Race

## Symptom

`cargo test --all --release` flakes 3 fix_test cases (and likely others
under `tests/*_test.rs` that spawn `almide run`):

- `fix_removes_return_keyword_iteratively`
- `fix_rewrites_comparison_calls_to_operators`
- `fix_rewrites_let_in_to_newline_chain`

Re-running the same tests with `--test-threads=1` passes them all
(verified 2026-04-16, both pre- and post-`bundled-almide-dispatch`).

## Root cause

Tests spawn `almide run <tmp>.almd` as a sub-process. The CLI uses two
shared paths:

- `.almide/cache/<file_path_munged>.hash` — incremental compile hash
- `/tmp/almide-run/target/debug/almide-<hash[..12]>` — cached binary

Multiple test threads writing to / reading from the same dir cause:

1. Thread A starts `cargo build` of one binary.
2. Thread B sees a partial binary, runs it, gets garbled / empty
   output.
3. Test asserts on stdout content → fails.

Per-thread cache directories or a per-test temp dir would eliminate
the race. The current isolation (per-file binary name via
`hash[..12]`) only protects against same-source collisions, not against
two threads racing on the same source.

## Why this matters

Until fixed, `cargo test --all --release` is unreliable as a CI gate.
Single-thread runs are slower but currently the only trustworthy
signal. This was masking my read of the post-`bundled-almide-dispatch`
test results — I initially misattributed the 3 failures to my changes
before isolating with `--test-threads=1` + a stash-and-rerun
comparison.

## Fix sketch

Per-test temp dir, scoped via `tempfile::TempDir`:

```rust
let dir = tempfile::TempDir::new().unwrap();
let project_dir = dir.path();   // unique per test
```

The hash file colocates with the binary (or moves to the same temp
dir) so cache invalidation is self-contained.

Risk: test compile time goes up — every test rebuilds from scratch
instead of sharing the `target/debug/` cache. Mitigation: keep a
per-thread (not per-test) shared cache; thread id keys the cache dir.

## Scope

~2-3h: rewrite `cli/run.rs::compile_to_binary` to take an optional
`cache_dir: Option<&Path>` arg, default to `.almide/cache` for prod,
threaded scope for tests. Plus `tests/common.rs` helper that test
files use.

Not blocking any feature ship — but should be fixed before the
`release-0.14.6` cut so CI signals are trustworthy.
