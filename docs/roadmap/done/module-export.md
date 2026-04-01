<!-- description: Export Almide modules as native packages for Python, JS/TS, Ruby, and WASM -->
<!-- done: 2026-04-01 -->
# Module Export: Almide Libraries for Every Language

> **Solved by [almide-lander](https://github.com/almide/almide-lander)** — 20 languages supported via almide-bindgen + shared library export. See that repo for details.

**Goal**: `almide export mylib.almd --lang python` produces a pip-installable package with idiomatic Python types. Same for JS/TS, Ruby, and raw WASM components.

**Core insight**: Almide already compiles to both Rust (142/142) and WASM (129/129). The missing piece is the binding layer that turns compiled output into language-native modules with proper type mapping.

---

## Architecture

```
Almide source (.almd)
    │
    ├─ parse + typecheck + lower
    │
    ▼
Module Interface (extracted from IR)
    │  - exported types (records, variants, aliases)
    │  - exported functions (name, params, return type)
    │  - documentation strings
    │
    ├──────────────────────────┬──────────────────────┐
    ▼                          ▼                      ▼
WASM path                 Native path            Interface-only
    │                          │                      │
    ├─ compile to .wasm        ├─ emit Rust source    ├─ .d.ts
    ├─ generate JS glue        ├─ generate PyO3       ├─ .pyi
    ├─ generate .d.ts          ├─ generate Magnus     └─ RBS
    └─ npm package             ├─ generate napi-rs
                               └─ language package
```

### Module Interface

The shared foundation. Extracted from `IrProgram` after type checking:

```rust
struct ModuleInterface {
    name: String,
    types: Vec<ExportedType>,    // records, variants, aliases
    functions: Vec<ExportedFn>,  // name, params, return type, doc
}
```

Type mapping rules (one table, all languages derive from it):

| Almide | Python | TypeScript | Ruby | WASM Component |
|--------|--------|------------|------|----------------|
| Int | int | number | Integer | s64 |
| Float | float | number | Float | f64 |
| String | str | string | String | string |
| Bool | bool | boolean | true/false | bool |
| List[T] | list[T] | T[] | Array[T] | list\<T\> |
| Option[T] | T \| None | T \| null | T \| nil | option\<T\> |
| Result[T, E] | T (raises on Err) | Result\<T, E\> | T (raises on Err) | result\<T, E\> |
| (A, B) | tuple[A, B] | [A, B] | [A, B] | tuple\<A, B\> |
| Record | dataclass | interface | Struct/Data | record |
| Variant | enum (tagged union) | discriminated union | sealed class | variant |

---

## Phases

### Phase 1: Module Interface extraction

Extract the public API from IR into a structured `ModuleInterface` representation.

- Identify exported items (top-level `pub fn`, `pub type`)
- Resolve all types to concrete forms (no TypeVars in exports)
- Generate a machine-readable interface file (JSON or custom format)
- This is the foundation — everything else depends on it

### Phase 2: First end-to-end target

Pick one language and build the full pipeline: compile → bind → package → install → use.

Two candidates, run in parallel:

**Python (native path)**
- Almide → Rust → PyO3 bindings → maturin build → wheel
- PyO3 is production-grade, maturin handles packaging
- Type stubs (.pyi) generated from Module Interface
- Records → `@dataclass`, Variants → tagged unions, Option → `Optional`
- `pip install ./almide-mylib` just works

**JS/TS (WASM path)**
- Almide → WASM → JS glue + TypeScript declarations
- Already have WASM backend at 100%
- Generate `.d.ts` from Module Interface
- Handle string/list/record marshaling across WASM boundary
- `npm install ./almide-mylib` just works

### Phase 3: Remaining languages + WASM Component Model

- Ruby via Magnus (native) or wasmtime-rb (WASM)
- WASM Component Model with WIT interface generation
- `almide export --lang wasm` produces a portable component
- Evaluate Component Model maturity and adopt when ready

### Phase 4: Package registry integration

- `almide publish` → uploads to language-specific registries
- PyPI, npm, RubyGems, wapm
- Versioning derived from Almide module version
- Cross-language dependency resolution

---

## Design decisions

### Why both WASM and native?

| | WASM | Native (Rust FFI) |
|---|---|---|
| Portability | Any platform with WASM runtime | Must compile per platform |
| Performance | Near-native, marshaling overhead | Native speed, zero overhead |
| Packaging | Single binary, all platforms | Platform-specific wheels/gems |
| Maturity | Evolving (Component Model) | Production-grade (PyO3, napi-rs) |

The right choice depends on the use case. CPU-intensive libraries benefit from native. Portable utilities benefit from WASM. Almide supports both from the same source.

### Why not transpile?

Previous TS codegen taught us: transpilation creates impedance mismatch at every level (Result handling, match semantics, type systems, runtime behavior). Compiling to a binary and generating thin bindings avoids this entirely — the semantics are Almide's, only the API surface adapts.

---

## Success metric

```python
# This should work:
from almide_json_utils import JsonPath, get_path, parse

data = parse('{"users": [{"name": "Alice"}]}')
path = JsonPath.root().field("users").index(0).field("name")
name = get_path(data, path)  # "Alice"
```

An Almide library feels native in the target language. No WASM boilerplate visible. Types are discoverable via IDE autocomplete. Errors are language-idiomatic exceptions.
