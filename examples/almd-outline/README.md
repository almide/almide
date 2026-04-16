# almd-outline

Dogfood demo: a reimplementation of `almide ide outline`'s formatter in
Almide itself.

The main toolchain's `almide ide outline <file>` does three things:

1. Parse + type-check the source file (via compiler internals — Rust).
2. Extract a `ModuleInterface` (via `almide-tools::interface::extract`).
3. Render that interface as one-line-per-decl text.

This example covers **step 3** — rendering — written in pure Almide.
The input is the JSON that `almide compile --json <file>` emits.

## Run

```sh
# 1. Generate a ModuleInterface JSON for some source file
almide compile --json /path/to/your.almd > /tmp/iface.json

# 2. Render it with the Almide-written formatter
almide run examples/almd-outline/src/main.almd /tmp/iface.json
```

The output is bit-identical to `almide ide outline /path/to/your.almd`
for the same source.

## Why this matters

Almide's mission is "the language LLMs can write most accurately." If
a non-trivial Almide program that exercises the core patterns (JSON
parsing, pattern matching, recursive rendering, string interpolation,
effectful I/O) is clean to write, that's a stronger validation than any
micro-benchmark. This file is ~140 lines and covers:

- `import json / env / fs` with explicit-import discipline for effectful modules
- `Value` + `json.as_*` / `json.get_*` option-returning walkers
- `match` on tag strings with string-interpolated arms
- Pipe chains: `xs |> list.map(f) |> list.join(", ")`
- Recursive rendering (`render_type` → `render_inner` → `render_type` via records)
- `guard cond else err("msg")` for `effect fn` early-return
- `expr ?? default` and `expr!` for `Option` / `Result` handling

## Gaps found while writing this

Writing this demo surfaced real gaps, captured here to inform Phase 3:

1. **`guard cond else { println(...) }`** (side-effect only, no return value)
   should be a checker-level error saying "guard else must return or err",
   not leak to rustc as `expected Result<(), String>, found ()`. The
   [rustc-leak wrap](../../src/cli/mod.rs) currently only fires on
   `almide test`, not on `almide run` / `almide build`. Should extend.

2. **`effect` is a reserved keyword** — cannot be used as a variable name.
   Worth surfacing in the diagnostic list for `expect_ident` so LLMs
   don't repeatedly fumble on this.

3. **JSON field names not self-documenting**. `ModuleInterface` serializes
   function return types as `"return"` (not `"ret"`), constants as
   `"type"` (not `"ty"`). Writing a reader without a spec required
   guessing then correcting. Either freeze the schema in
   [crates/almide-tools/src/interface.rs](../../crates/almide-tools/src/interface.rs)
   via doc comments + a published JSON schema, or publish `llms.txt`
   (Phase 3-3) so future reimplementations don't need to reverse-engineer.

Despite these, the overall experience was: **the language holds up for
this class of work**. No features felt missing, no awkward workarounds
were needed once the gaps above were noted.
