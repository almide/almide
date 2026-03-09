# Adding Standard Library Modules

Almide has two kinds of stdlib modules: **hardcoded** (implemented in Rust inside the compiler) and **bundled** (written in Almide, embedded at compile time).

## Bundled modules (preferred for new modules)

Bundled modules are `.almd` files in this directory. They are compiled into the binary via `include_str!()` and loaded as regular Almide modules at resolution time. No Rust codegen is needed.

**Examples:** `path.almd`, `time.almd`, `args.almd`, `encoding.almd`

### Steps to add a bundled module

1. **Create `stdlib/<name>.almd`** with your module code. Use only hardcoded stdlib functions and other bundled modules — no circular dependencies.

2. **Register in `src/stdlib.rs`** — add a case to `get_bundled_source()`:
   ```rust
   "mymod" => Some(include_str!("../stdlib/mymod.almd")),
   ```

3. **Write tests** in `exercises/stdlib-test/<name>_test.almd`.

4. **Update docs:**
   - `docs/CHEATSHEET.md` — add function signatures to the stdlib section
   - `../benchmark/stdlib.md` — full reference (if the module is useful for benchmarks)
   - `../benchmark/stdlib-extra.md` or a new `stdlib-<name>.md` — modular reference

5. **Run all exercises** to verify nothing breaks:
   ```bash
   for f in exercises/*/*.almd; do almide run "$f"; done
   ```

That's it. No changes to the type checker, emitter, or UFCS tables are needed.

## Hardcoded modules (for primitives and performance-critical ops)

Hardcoded modules have their type signatures and code generation implemented directly in the compiler. This is necessary for operations that can't be expressed in Almide itself (e.g., FFI, memory layout, platform syscalls).

**Examples:** `string`, `list`, `map`, `int`, `float`, `fs`, `env`, `process`, `io`, `json`, `math`, `random`, `regex`

### Steps to add a hardcoded function

1. **`src/stdlib.rs`** — add type signature to `lookup_sig()`:
   ```rust
   ("mymod", "my_func") => FnSig { params: vec![...], ret: ..., is_effect: false },
   ```

2. **`src/stdlib.rs`** — if the function should be callable via UFCS (dot syntax), add it to `resolve_ufcs_module()`:
   ```rust
   "my_func" => Some("mymod"),
   ```

3. **`src/emit_rust/calls.rs`** — add Rust code generation in the module's match arm.

4. **`src/emit_ts/expressions.rs`** — add TypeScript code generation (if applicable).

5. **`src/emit_ts_runtime.rs`** — if the TS implementation needs a runtime helper, add it to both the Deno and Node runtime sections.

6. **Write tests** in `exercises/stdlib-test/`.

7. **Update docs** (same as bundled).

8. **Run all exercises.**

### Adding a new hardcoded module

In addition to the per-function steps above:

- Add the module name to `STDLIB_MODULES` in `src/stdlib.rs`
- If it's a platform module (requires OS access), add it to `PLATFORM_MODULES` in `src/check/mod.rs` — this will produce a compile error when targeting WASM

## Module classification

| Layer | Available on | Examples |
|-------|-------------|----------|
| **core** | All targets including WASM | string, list, map, int, float, math, json, regex, path, time, args, encoding |
| **platform** | Native only (WASM = compile error) | fs, process, io, env, http, random |

When adding a new module, decide which layer it belongs to. If it requires OS access (file I/O, networking, environment variables, entropy), it's a platform module.
