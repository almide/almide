<!-- description: TOML-driven declarative stdlib codegen replacing manual match arms -->
# Declarative Stdlib Codegen

Inspired by React Native's TurboModules architecture: define stdlib once in a declarative format, auto-generate type signatures, UFCS mappings, and per-target codegen.

## Problem

Adding a new stdlib function today requires editing 3-4 files:

1. `src/stdlib.rs` — type signature (`lookup_sig` match arm)
2. `src/stdlib.rs` — UFCS mapping (`resolve_ufcs_candidates` match arm)
3. `src/emit_rust/calls.rs` — Rust code generation (match arm)
4. `src/emit_ts/expressions.rs` — TS/JS code generation (match arm)

These are ~1,450 lines of hand-written match arms that must stay in sync. Every new function is 4 coordinated edits. Missing one causes silent bugs or panics.

### Scale of the problem

| File | Lines | Match arms |
|------|-------|------------|
| stdlib.rs (sigs) | 384 | ~120 functions |
| stdlib.rs (UFCS) | 50 | ~80 methods |
| emit_rust/calls.rs | 625 | ~150 patterns |
| emit_ts/expressions.rs | 438 | ~120 patterns |
| **Total** | **1,497** | **~470 hand-written patterns** |

## Design: Declarative Stdlib Definitions

### Definition format (TOML or inline DSL)

Each stdlib function is defined once in a declarative spec:

```toml
# stdlib/defs/string.toml

[trim]
params = [{ name = "s", type = "String" }]
return = "String"
ufcs = true
rust = "({s}).trim().to_string()"
ts = "({s}).trim()"

[split]
params = [{ name = "s", type = "String" }, { name = "sep", type = "String" }]
return = "List[String]"
ufcs = true
rust = "({s}).split(&*{sep}).map(|s| s.to_string()).collect::<Vec<_>>()"
ts = "({s}).split({sep})"

[len]
params = [{ name = "s", type = "String" }]
return = "Int"
ufcs = true
rust = "({s}).chars().count() as i64"
ts = "({s}).length"

[contains]
params = [{ name = "s", type = "String" }, { name = "sub", type = "String" }]
return = "Bool"
ufcs = true
rust = "({s}).contains(&*{sub})"
ts = "({s}).includes({sub})"
```

```toml
# stdlib/defs/list.toml

[swap]
params = [{ name = "xs", type = "List[T]" }, { name = "i", type = "Int" }, { name = "j", type = "Int" }]
return = "List[T]"
ufcs = true
rust = "almide_rt_list_swap(&{xs}, {i}, {j})"
ts = "__almd_list.swap({xs}, {i}, {j})"

[map]
params = [{ name = "xs", type = "List[T]" }, { name = "f", type = "fn(T) -> U" }]
return = "List[U]"
ufcs = true
rust = "({xs}).iter().map(|__x| ({f})(__x.clone())).collect::<Vec<_>>()"
ts = "({xs}).map({f})"
```

```toml
# stdlib/defs/fs.toml

[read_text]
params = [{ name = "path", type = "String" }]
return = "Result[String, IoError]"
effect = true
ufcs = false
rust = "std::fs::read_to_string(&*{path}).map_err(|e| e.to_string())"
ts = "await Deno.readTextFile({path})"
```

### What gets generated

From these definitions, a build.rs (or compile-time macro) generates:

1. **`lookup_sig()`** — the entire match arm from params/return types
2. **`resolve_ufcs_candidates()`** — from `ufcs = true` fields, grouped by ambiguity
3. **`emit_rust_call()`** — from `rust = "..."` templates
4. **`emit_ts_call()`** — from `ts = "..."` templates
5. **`emit_js_call()`** — same as TS but stripped of type annotations (already the case)

### Template language

Simple placeholder substitution: `{param_name}` is replaced with the emitted expression for that argument. No complex logic needed — current codegen is already template-like.

For complex cases that don't fit a template (e.g., `list.fold` with special ownership semantics in Rust), allow `rust_fn = "emit_list_fold"` to delegate to a hand-written function.

## Architecture

```
stdlib/
├── defs/                ← NEW: declarative definitions
│   ├── string.toml
│   ├── list.toml
│   ├── map.toml
│   ├── int.toml
│   ├── float.toml
│   ├── fs.toml
│   ├── env.toml
│   ├── process.toml
│   ├── json.toml
│   ├── math.toml
│   ├── random.toml
│   ├── regex.toml
│   └── io.toml
├── args.almd            ← existing bundled modules (unchanged)
├── path.almd
├── time.almd
├── encoding.almd
├── hash.almd
└── term.almd

src/
├── stdlib.rs            ← GENERATED from defs/*.toml (or calls generated module)
├── emit_rust/calls.rs   ← GENERATED
└── emit_ts/expressions.rs ← GENERATED
```

### Build-time generation (build.rs)

```rust
// build.rs
fn main() {
    let defs_dir = Path::new("stdlib/defs");
    let mut sigs = String::new();
    let mut ufcs = String::new();
    let mut rust_calls = String::new();
    let mut ts_calls = String::new();

    for entry in fs::read_dir(defs_dir).unwrap() {
        let module_name = entry.file_stem();
        let toml: ModuleDef = toml::from_str(&fs::read_to_string(entry)?)?;
        for (fn_name, fn_def) in &toml.functions {
            sigs.push_str(&gen_sig(module_name, fn_name, fn_def));
            if fn_def.ufcs { ufcs.push_str(&gen_ufcs(module_name, fn_name)); }
            rust_calls.push_str(&gen_rust_call(module_name, fn_name, fn_def));
            ts_calls.push_str(&gen_ts_call(module_name, fn_name, fn_def));
        }
    }

    write_generated("src/generated/stdlib_sigs.rs", &sigs);
    write_generated("src/generated/stdlib_ufcs.rs", &ufcs);
    write_generated("src/generated/emit_rust_calls.rs", &rust_calls);
    write_generated("src/generated/emit_ts_calls.rs", &ts_calls);
}
```

### Migration path: gradual

Don't rewrite all 120 functions at once. Instead:

1. Set up the build.rs infrastructure + TOML parser
2. Migrate one module (e.g., `math` — simple, no generics, no UFCS ambiguity)
3. Generated code calls into existing code for non-migrated functions (fallback match arm)
4. Migrate module by module: math → float → int → string → list → map → fs → ...
5. Delete hand-written match arms as each module is migrated

## Implementation Phases

### Phase 1: Infrastructure (1 day)

- Define TOML schema for stdlib definitions
- Write build.rs that reads `stdlib/defs/*.toml` and generates Rust source
- Generate a single module (`math`) to validate the approach
- Existing hand-written code stays as fallback

### Phase 2: Pure functions (3-5 days)

Migrate modules with no generics and no UFCS ambiguity:
- `math` (12 functions) — simplest, no UFCS
- `float` (12 functions) — simple types
- `int` (18 functions) — simple types, includes bitwise
- `regex` (8 functions) — simple types
- `io` (3 functions) — small

~53 functions, eliminates ~200 match arms.

### Phase 3: UFCS + generics (3-5 days)

Migrate the big three with UFCS resolution and generic types:
- `string` (~30 functions) — UFCS, some ambiguous with list
- `list` (~40 functions) — UFCS, generics (List[T]), higher-order functions
- `map` (~15 functions) — UFCS, generics (Map[K,V])

This is the hard part: UFCS ambiguity groups (len, contains, get) need special handling in the generator.

### Phase 4: Effect modules (2-3 days)

- `fs` (15 functions) — effect, IoError return types
- `env` (8 functions) — effect
- `process` (4 functions) — effect, complex return types (Record)
- `json` (18 functions) — Named types (Json)
- `random` (4 functions) — effect

### Phase 5: Cleanup (1 day)

- Delete hand-written match arms from stdlib.rs, calls.rs, expressions.rs
- These files become thin wrappers around generated code
- Verify all tests pass

## Benefits

### For development speed
- Adding a new stdlib function: edit 1 TOML file (4 lines) instead of 4 Rust files
- Less chance of desync between type checker and codegen
- New targets (WASM direct emit) just need a new template field: `wasm = "..."`

### For WASM direct emit
- When `emit_wasm/` is built (see `planned/emit-wasm-direct.md`), add `wasm = "..."` field to each TOML definition
- No need to write another 600-line match arm by hand
- The WASM template generates WASM instruction sequences

### For LLM contribution
- TOML definitions are much easier for LLMs to write than Rust match arms
- Could auto-generate TOML from natural language function specs
- Enables rapid stdlib expansion

### For documentation
- TOML definitions serve as the single source of truth
- Can auto-generate the CLAUDE.md stdlib section from TOML
- Can auto-generate API docs for the website

## Risks

### Template limitations
Some codegen is complex (e.g., `list.fold` needs different ownership handling in Rust debug vs release). Solution: `rust_fn = "custom_handler"` escape hatch for functions that don't fit templates.

### Build time
build.rs adds a compilation step. TOML parsing + code generation should be fast (< 1s for ~120 functions), but need to verify.

### TOML readability
At ~120 functions, the TOML files might get unwieldy. Mitigated by splitting per module (13 files, ~10 functions each on average).

## Success Criteria

- Adding `list.window(xs, n)` requires editing only `stdlib/defs/list.toml`
- `almide test` passes with 100% generated stdlib code (no hand-written match arms)
- New WASM target gets stdlib support by adding `wasm = "..."` to existing TOML definitions
