# runtime/rs — the native runtime source template

`runtime/rs/src/*.rs` holds the native (`almide_rt_*`) implementations behind
`@intrinsic("almide_rt_*")` declarations in `stdlib/*.almd`. It is **not a
crate you build**, and it is **not a workspace member**. This file records why,
and where its behaviour is actually tested.

## How this directory is consumed

Nothing links `runtime/rs` as a cargo dependency. `crates/almide-codegen/build.rs`
reads each file here as **source text** (resolving `include!` directives) and
embeds it in `crates/almide-codegen/src/generated/rust_runtime.rs` as
`RUST_RUNTIME_MODULES`. At emit time the compiler concatenates the modules a
program needs into **one flat module**, prepends a synthesised prelude, and
hands that to rustc:

- `emit_runtime_crate()` → the shared `almide_rt` rlib (`--crate-name almide_rt`,
  every std-only module; `NON_STD_RUNTIME_MODULES` — `http`, `zlib`, `sse` — are
  left out because bare rustc has no `--extern` for rustls/flate2);
- `emit_source()` → the runtime preamble of a generated Rust program.

Both paths call `strip_test_blocks`, which deletes every `#[cfg(test)]` block
before rustc sees it.

`strip_test_blocks` used to count braces per line with no notion of a string
literal, so a test body whose literals held unbalanced braces made it over-run
and delete to end of file. `regex.rs` had two (`r"a\{x"`, `r"a{2"`, +4 depth),
and the stripper ran from the test module to the file's last line, eating the
trailing `// ---- End Regex Runtime ----` comment on the way. It was harmless
only because that module was the last item in the file; real code after such a
block would have been deleted from every emitted crate, silently. There were
two copies of the stripper, byte-identical — this one and the build script's,
over `crates/almide-kernel/src/*.rs` — and neither fact was checked.

Both are fixed and both are checked (#2511). There is now ONE stripper,
`crates/almide-codegen/src/strip_test_blocks.rs`, `#[path]`-shared with the
build script. It tracks literal state instead of counting characters (normal
and raw strings at any hash depth, char literals told apart from lifetimes,
line and nested block comments, all carrying across lines), and losing a
block's end is an ERROR naming the file and the opening line rather than a
silent delete to EOF — "I could not strip it" is a safe failure, "I stripped
everything after it" is not.

Deleting the seven test modules (#2507) changed the EMITTED text by exactly
eight lines, all blank or comment, verified by running `strip_test_blocks` over
the before and after sources: six files lost one blank line, and `regex.rs`
regained the comment the over-run had been eating. No code line moved.

Because a `runtime/rs/src` edit changes the embedded text, CI regenerates
`rust_runtime.rs` / `runtime_fn_modes.rs` and fails on any diff (ci.yml,
"Generated runtime registry matches committed sources"). Rebuild
`almide-codegen` and commit the generated files with your edit.

## Why it is not a workspace member (#2507, measured 2026-09-22)

Adding `"runtime/rs"` to `members` in the root `Cargo.toml` and running
`cargo check -p almide_rt` produces **220 errors** (124 × E0425, 83 × E0433,
13 × E0405) across 15 of the 25 modules `src/lib.rs` declares. They fall into
two classes, and neither is a dependency or target-configuration problem —
rustls and webpki-roots resolve fine:

1. **The prelude does not exist as source.** `AlmideRepr`, `AlmideMap`,
   `AlmideKey`, `AlmideKeyLookup`, `AlmideMapKey`, `ALMIDE_MAP_INDEX_THRESHOLD`,
   `almide_stdout_flush`, `almide_stdout_write_bytes`, `almide_rt_intern_key`
   and `almide_http_request_stream_impl` are emitted by
   `rust_runtime_prelude()` — Rust written as string literals inside
   `crates/almide-codegen/src/lib.rs`, in two visibility variants. No file
   under `runtime/rs` defines them.

   ```
   error[E0425]: cannot find function `almide_stdout_flush` in this scope
     --> runtime/rs/src/process.rs:44:5
   error[E0405]: cannot find trait `AlmideRepr` in this scope
     --> runtime/rs/src/value.rs:422:6
   ```

2. **The module tree in `src/lib.rs` is a fiction.** The files call each other
   with no path, which only resolves under the flat concatenation:
   `json.rs` → `almide_rt_value_stringify`, `math.rs`/`matrix.rs` →
   `almide_rt_libm_*`, `fan.rs` → `almide_rt_list_par_*`, `sse.rs` →
   `almide_rt_json_parse`. Under `pub mod json; pub mod value; …` every one is
   E0425. `lib.rs` also omits `hash.rs`, `net.rs` and `zlib.rs` entirely —
   three files the embedder ships and the module tree has never mentioned.

   ```
   error[E0425]: cannot find function `almide_rt_value_stringify` in this scope
     --> runtime/rs/src/json.rs:6:62
   error[E0425]: cannot find type `AlmideValue` in this scope
     --> runtime/rs/src/json.rs:6:37
   ```

Making it a member would therefore mean either restructuring the runtime source
(which the embedder's flat concatenation depends on) or re-deriving the prelude
in a second place. Joining also adds `almide-kernel`, `almide_rt` and a second
`webpki-roots` major version (0.26.11 beside the 1.0.9 `almide-rt-core` pins)
to the workspace lock, for a crate that ships nothing.

`Cargo.toml` and `build.rs` are kept as the record of what the runtime source
needs (crates.io deps; the `have_blas` cfg `matrix.rs` reads). They are not a
build unit — no CI job and no script invokes cargo here.

## The behavioural oracle is `spec/`

**The behavioural oracle for `runtime/rs` is the executable `spec/` suite.**
It exercises this code the way programs do — through the stdlib, on native and
on both wasm legs — and it is what CI runs. A Rust `#[cfg(test)]` block written
here runs nowhere: no cargo build compiles this directory, and
`strip_test_blocks` removes the block from every emitted crate.

So: **do not add `#[cfg(test)]` under `runtime/rs`.** Write the test in
`spec/stdlib/` (or `spec/wasm_cross/` when the change is a cross-target
promise). `scripts/check-runtime-test-placement.sh` enforces this in CI.

### The one narrow exception: a self-contained module, included by a root test

When the cell under test is **private** and its only public door is
non-deterministic, `spec/` cannot reach it. `random.rs`'s 53-bit construction
(`almide_float_from_u64`) is the case: `random.float()` draws a u64 nobody
chooses, so `spec/stdlib/random_test.almd` can assert the sampled floor and the
construction's top value but not the individual u64 inputs where the old
quotient rounded to 1.0 (#2495).

For that, `tests/runtime_random_float_construction_test.rs` `include!`s
`runtime/rs/src/random.rs` into a module of its own and tests the **shipped
source text** under `cargo test --workspace`. The test file, not the runtime
source, holds the accessor for the private item, so nothing about what ships
changes.

This works only because `random.rs` depends on `std::cell::Cell` and nothing
else. Every other module here resolves only under the embedder's flat
concatenation plus the synthesised prelude (the 220 errors above), so the same
include would not compile. Treat this as one measured exception, not a pattern:
if a second module ever qualifies, say in the test file why it is
self-contained.
