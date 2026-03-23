# Almide Standard Library

## Architecture Overview

```
stdlib/defs/*.toml          TOML function definitions (type sigs + codegen templates)
        │
        ▼  build.rs (compile time)
src/generated/
  ├── stdlib_sigs.rs        Type signatures for the checker
  ├── emit_rust_calls.rs    Rust codegen dispatch
  └── emit_ts_calls.rs      TypeScript codegen dispatch
        │
        ▼  codegen (when almide compiles user code)
runtime/rust/src/*.rs       Rust runtime functions (embedded via include_str!)
src/emit_ts_runtime/        TypeScript runtime functions (embedded in output)

stdlib/*.almd               Bundled pure-Almide modules (no codegen needed)
```

### Pipeline

1. **Define**: Add function signature + codegen template to `stdlib/defs/<module>.toml`
2. **Generate**: `cargo build` runs `build.rs` which reads all TOML and generates 3 files in `src/generated/`
3. **Implement**: Write the `almide_rt_*` function in `runtime/rust/src/<module>.rs`
4. **Embed**: The compiler embeds runtime functions into generated Rust code via `include_str!` (see `src/emit_rust/lower_rust.rs`)
5. **Compile**: User code calls `string.len("hi")` → generated Rust calls `almide_rt_string_len(&*s)`

Zero hand-written codegen dispatch. Every stdlib function is driven entirely from TOML.

### File Map

| Path | Purpose |
|------|---------|
| `stdlib/defs/*.toml` | Function definitions (22 modules, 362 functions) |
| `runtime/rust/src/*.rs` | Rust runtime implementations (12 files, canonical) |
| `src/emit_ts_runtime/` | TypeScript runtime implementations |
| `src/generated/` | Auto-generated from TOML. **Do not edit.** |
| `stdlib/*.almd` | Bundled pure-Almide modules (11 modules) |
| `src/stdlib.rs` | Module registry, UFCS resolution, bundled module loading |
| `build.rs` | TOML → codegen generator |
| `tools/gen-stdlib-spec.py` | Generates `docs/STDLIB-SPEC.md` from TOML |

## Module Types

### Native Modules (TOML-defined, 22 modules)

Defined in `stdlib/defs/`. Each function has:
- Type signature (for the checker)
- Rust codegen template (calls `almide_rt_*` runtime functions)
- TypeScript codegen template (calls `__almd_*` runtime functions)

Runtime status: 230/362 functions implemented (63%). See `docs/STDLIB-SPEC.md` for per-function status.

| Layer | Modules | Available on |
|-------|---------|-------------|
| **core** | string, list, map, set, int, float, math, json, regex, result, option, error, testing, log, value | All targets including WASM |
| **platform** | fs, process, io, env, http, random, datetime | Native only |
| **lang** | fan | Rust (other targets TBD) |

### Bundled Modules (pure Almide, 2 modules)

Written in Almide, embedded via `include_str!` in `src/stdlib.rs`. Compiled as regular Almide code at import time.

| Module | Description |
|--------|-------------|
| args | Command-line argument parsing |
| path | Path manipulation |

## Runtime Implementation

All runtime functions live in `runtime/rust/src/`. Each file is a flat list of `pub fn almide_rt_*` functions with no external dependencies (no `use` statements, no crate imports). These files are `include_str!`-embedded into every generated Rust program.

```
runtime/rust/src/
├── int.rs        19/21 implemented
├── float.rs      16/16 implemented
├── string.rs     41/41 implemented
├── list.rs       54/54 implemented
├── map.rs        16/16 implemented
├── result.rs      9/9  implemented
├── option.rs      (Option helpers)
├── error.rs       3/3  implemented
├── testing.rs     (assert helpers)
├── value.rs      19/19 implemented
├── env.rs         9/9  implemented
└── process.rs     5/6  implemented
```

Missing runtime modules (TOML defined but no runtime): crypto, datetime (partial), fs, http (partial), io, log, math (partial), random, regex, testing, uuid.

### Adding a Runtime Function

1. Add TOML definition to `stdlib/defs/<module>.toml`:
   ```toml
   [my_function]
   description = "Does something useful."
   params = [{ name = "s", type = "String" }]
   return = "Int"
   rust = "almide_rt_string_my_function(&*{s})"
   ts = "__almd_string.my_function({s})"
   ```

2. Implement in `runtime/rust/src/<module>.rs`:
   ```rust
   pub fn almide_rt_string_my_function(s: &str) -> i64 { s.len() as i64 }
   ```

3. `cargo build` auto-generates codegen dispatch.

4. Write a test in `spec/stdlib/`:
   ```almide
   test "string.my_function" {
     assert_eq(string.my_function("hello"), 5)
   }
   ```

### Adding a New Module

1. Create `stdlib/defs/<name>.toml`
2. Add module name to `STDLIB_MODULES` in `src/stdlib.rs`
3. Create `runtime/rust/src/<name>.rs` with runtime functions
4. Add `include_str!` line in `src/emit_rust/lower_rust.rs` (runtime embedding)
5. `cargo build && almide test`

### Adding a Bundled Module

1. Create `stdlib/<name>.almd`
2. Register in `src/stdlib.rs` `get_bundled_source()`:
   ```rust
   "mymod" => Some(include_str!("../stdlib/mymod.almd")),
   ```
3. Write tests in `spec/stdlib/<name>_test.almd`

## TOML Definition Format

Each `.toml` file defines one module. Each top-level table is a function.

### Function Fields

| Field | Required | Description |
|-------|----------|-------------|
| `params` | yes | Array of `{ name, type }` parameter definitions |
| `return` | yes | Return type string |
| `description` | no | Human-readable description |
| `example` | no | Usage example |
| `type_params` | no | Generic type variable names (e.g. `["A", "B"]`) |
| `rust` | yes | Rust codegen template |
| `ts` | no | TypeScript codegen template |
| `effect` | no | `true` if this is an effect function |
| `rust_effect` | no | Alternative Rust template for effect context with `?` |
| `rust_min` | no | Rust template when optional params are omitted |
| `ts_min` | no | TS template when optional params are omitted |

### Param Fields

| Field | Required | Description |
|-------|----------|-------------|
| `name` | yes | Parameter name |
| `type` | yes | Type string |
| `optional` | no | `true` if this param can be omitted |

### Type Strings

| TOML | Internal Type |
|------|--------------|
| `Int`, `Float`, `String`, `Bool`, `Unit` | Primitives |
| `Unknown` | `Ty::Unknown` (permissive wildcard) |
| `A`, `B`, `K`, `V` (in `type_params`) | `Ty::TypeVar` (generic) |
| `List[T]` | `Ty::List` |
| `Option[T]` | `Ty::Option` |
| `Result[T, E]` | `Ty::Result` |
| `Map[K, V]` | `Ty::Map` |
| `Fn[A, B] -> C` | `Ty::Fn` (closure) |

### Template Placeholders

| Placeholder | Expands to |
|-------------|-----------|
| `{param}` | Emitted expression for the parameter |
| `{f.args}` | Closure parameter names, comma-separated |
| `{f.body}` | Closure body expression |
| `{f.clone_bindings}` | `let x = x.clone();` for each closure param |

### Effect Variants

When a function has both `rust` and `rust_effect`, codegen selects based on whether the call is in an effect context:

```toml
[map]
rust = "almide_rt_list_map(({xs}).clone(), |{f.args}| {{ {f.body} }})"
rust_effect = "almide_rt_list_map_effect(({xs}).clone(), |{f.args}| -> Result<_, String> {{ Ok({{ {f.body} }}) }})?"
```

### Optional Parameters

```toml
[slice]
params = [
    { name = "s", type = "String" },
    { name = "start", type = "Int" },
    { name = "end", type = "Int", optional = true },
]
rust = "almide_rt_string_slice(&*{s}, {start}, Some({end}))"
rust_min = "almide_rt_string_slice(&*{s}, {start}, None)"
```

## Tooling

- `python3 tools/gen-stdlib-spec.py` — Regenerate `docs/STDLIB-SPEC.md` from TOML definitions
- `almide run tools/stdlib-crawler/main.almd -- python --all` — Crawl Python stdlib for API design reference
- `almide run tools/stdlib-crawler/main.almd -- python --verbs` — Analyze verb frequency across Python stdlib
