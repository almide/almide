> Last updated: 2026-03-28

# Codegen Specification

Almide's codegen is a three-layer architecture that transforms typed IR into target-specific output. All semantic decisions are made in the IR before any text is emitted.

**Source**: `src/codegen/mod.rs`

```
IrProgram (typed IR)
    |
Layer 1: Nanopass Pipeline (target-specific semantic rewrites)
    |
Layer 2: Emit (target-specific output)
    Rust  -> Template Renderer (TOML-driven) -> source code
    WASM  -> Direct binary emit              -> .wasm bytes
```

## 1. Entry Point

**Source**: `src/codegen/mod.rs`

The single entry point is `codegen(program, target) -> CodegenOutput`.

```rust
pub enum CodegenOutput {
    Source(String),   // Rust target
    Binary(Vec<u8>),  // WASM target
}
```

The flow:

1. `target::configure(target)` builds a `TargetConfig` containing the nanopass pipeline and template set
2. The pipeline runs all applicable passes, transforming the IR in place
3. Target-specific emit:
   - **Rust**: Walker renders IR via templates, prepends runtime preamble
   - **WASM**: `emit_wasm::emit()` produces binary directly

```almd
// Input: hello.almd
fn greet(name: String) -> String =
  "Hello, " + name + "!"

fn main() =
  println(greet("world"))
```

```rust
// Output: Rust target (simplified, preamble omitted)
pub fn greet(name: String) -> String {
    format!("{}{}", format!("{}{}", "Hello, ".to_string(), name), "!".to_string())
}

pub fn main() -> () {
    println!("{}", greet("world".to_string()))
}
```

## 2. Target Enum

**Source**: `src/codegen/pass.rs`

```rust
pub enum Target {
    Rust,
    TypeScript,  // Removed (2026-03-28) -- use --target wasm
    Go,          // Pipeline stub
    Python,      // Pipeline stub
    Wasm,
}
```

Active targets with full pipelines: **Rust** and **WASM**. TypeScript codegen was removed; Go and Python have skeleton pipelines only.

Each target is configured via `target::configure()` which returns a `TargetConfig`:

**Source**: `src/codegen/target.rs`

```rust
pub struct TargetConfig {
    pub target: Target,
    pub pipeline: Pipeline,
    pub templates: TemplateSet,
}
```

## 3. Nanopass Pipeline

**Source**: `src/codegen/pass.rs`, `src/codegen/target.rs`

Each pass implements the `NanoPass` trait:

```rust
pub trait NanoPass: std::fmt::Debug {
    fn name(&self) -> &str;
    fn targets(&self) -> Option<Vec<Target>>;  // None = all targets
    fn depends_on(&self) -> Vec<&'static str>;
    fn run(&self, program: IrProgram, target: Target) -> PassResult;
}
```

Passes compose into a `Pipeline`. The pipeline runner:
- Skips passes not relevant to the current target
- Validates declared dependencies (panics if a dependency has not executed)
- Verifies IR integrity and declared `Postcondition`s between passes on every
  build — violations panic in debug and print as diagnostics in release. No
  opt-in env var (`ALMIDE_CHECK_IR` / `ALMIDE_VERIFY_IR` removed in
  v0.14.7-phase3.2); `expr.ty` is trustworthy by contract

### Rust Pipeline (in order)

**Source**: `src/codegen/target.rs`, `build_pipeline(Target::Rust)`

| # | Pass | Source | Description |
|---|------|--------|-------------|
| 1 | BoxDerefPass | `pass_box_deref.rs` | Insert Deref IR nodes for pattern variables bound from Box'd fields in recursive enums |
| 2 | TailCallOptPass | `pass_tco.rs` | Convert self-recursive tail calls into `while true` loops with parameter reassignment |
| 3 | LICMPass | `pass_licm.rs` | Hoist loop-invariant pure expressions to `let` bindings before the loop |
| 4 | TypeConcretizationPass | `pass.rs` | Box recursive types, generate anonymous record structs (stub) |
| 5 | StreamFusionPass | `pass_stream_fusion/mod.rs` | Fuse pipe chains (`map \|> filter \|> fold`) into single loops using algebraic laws |
| 6 | BorrowInsertionPass | `pass.rs` | Analyze parameter usage, mark `&T` vs `T` (stub) |
| 7 | CaptureClonePass | `pass_capture_clone.rs` | Pre-clone variables captured by multiple move closures to avoid E0382 |
| 8 | CloneInsertionPass | `pass_clone.rs` | Insert `Clone` IR nodes for heap-type variables based on use-count analysis |
| 9 | MatchSubjectPass | `pass_match_subject.rs` | Insert `.as_str()` on String match subjects, `.as_deref()` on `Option<String>` |
| 10 | EffectInferencePass | `pass_effect_inference.rs` | Infer capability requirements (IO, Net, Env, etc.) from stdlib usage |
| 11 | StdlibLoweringPass | `pass_stdlib_lowering.rs` | Rewrite `Module { "list", "map" }` calls to `Named { "almide_rt_list_map" }` with arg decoration |
| 12 | AutoParallelPass | `pass_auto_parallel.rs` | Rewrite pure `list.{map,filter,any,all}` calls to parallel `par_*` variants |
| 13 | ResultPropagationPass | `pass_result_propagation.rs` | Insert `Try` (Rust `?`) around Result-returning calls in `effect fn` |
| 14 | BuiltinLoweringPass | `pass_builtin_lowering.rs` | Convert `assert_eq`, `println`, etc. to `RustMacro` IR nodes |
| 15 | FanLoweringPass | `pass_fan_lowering.rs` | Strip auto-try from fan spawn closures (try applied at join point) |

### WASM Pipeline (in order)

| # | Pass | Description |
|---|------|-------------|
| 1 | TailCallOptPass | Convert self-recursive tail calls into loops |
| 2 | LICMPass | Hoist loop-invariant expressions |
| 3 | EffectInferencePass | Infer capability requirements |
| 4 | ResultPropagationPass | Insert Try for effect fn calls |
| 5 | FanLoweringPass | Strip auto-try from fan spawn closures |

### Other Passes (not in active pipelines)

| Pass | Target | Source | Description |
|------|--------|--------|-------------|
| ResultErasurePass | TS/Python | `pass_result_erasure.rs` | Erase Result/Option wrapping: `ok(x)` becomes `x`, `err(e)` becomes throw |
| MatchLoweringPass | TS | `pass_match_lowering.rs` | Lower `match` to `if/else` chains (TS has no native match) |
| ShadowResolvePass | TS | `pass_shadow_resolve.rs` | Convert let-shadowing to assignment (TS disallows redeclaration) |
| OptionErasurePass | TS/Python | `pass.rs` | Erase Option wrapping: `some(x)` becomes `x`, `none` becomes null |

## 4. Template System

**Source**: `src/codegen/template.rs`, `codegen/templates/rust.toml`

Templates define syntax only. All semantic decisions are made by nanopass passes.

### Structure

```rust
pub struct TemplateRule {
    pub template: String,           // "{placeholder}" holes
    pub when_type: Option<String>,  // Guard: match on expression type
    pub when_attr: Option<String>,  // Guard: match on TargetAttrs flag
}

pub struct TemplateEntry {
    pub rules: Vec<TemplateRule>,   // First matching rule wins
}

pub struct TemplateSet {
    pub target_name: String,
    pub entries: HashMap<String, TemplateEntry>,
}
```

### TOML Format

Single rule per construct:

```toml
[if_expr]
template = "if {cond} {{ {then} }} else {{ {else} }}"
```

Multiple rules with guards (array syntax):

```toml
[[concat_expr]]
when_type = "String"
template = "format!(\"{{}}{{}}\", {left}, {right})"

[[concat_expr]]
when_type = "List"
template = "AlmideConcat::concat({left}, {right})"
```

Attribute-guarded variants:

```toml
[[call_expr]]
when_attr = "needs_try"
template = "{callee}({args})?"

[[call_expr]]
template = "{callee}({args})"
```

### Guard System

**`when_type`**: Matches on the IR expression's type category (`"Int"`, `"Float"`, `"String"`, `"List"`, `"Option"`). Used for type-dispatched operations like `+` (concat vs add), `**` (pow vs powf), and unwrap_or.

**`when_attr`**: Matches on `TargetAttrs` flags set by nanopass passes. Used for:

| Flag | Set by | Effect |
|------|--------|--------|
| `needs_try` | ResultPropagationPass | Append `?` to call |
| `needs_clone` | CloneInsertionPass | Emit `.clone()` |
| `needs_borrow` | BorrowInsertionPass | Add `&` to parameter type |
| `none_type_hint` | TypeConcretizationPass | Emit `None::<T>` with explicit type |
| `repr_c` | CodegenOptions | Add `#[repr(C)]` to struct/enum |

### Rule Priority

Guarded rules are checked before unguarded defaults. First matching rule wins. The TOML loader sorts guarded rules before defaults within each construct.

### Template Placeholders

`{name}` is replaced by the corresponding binding. `{{` and `}}` are escape sequences for literal braces. Unknown placeholders are kept as-is.

### Key Template Constructs (Rust)

The Rust template set (`codegen/templates/rust.toml`) defines constructs for:

| Category | Examples |
|----------|----------|
| Expressions | `if_expr`, `call_expr`, `binary_op`, `field_access`, `index_access`, `pipe_call` |
| Option/Result | `some_expr`, `none_expr`, `ok_expr`, `err_expr`, `unwrap_expr`, `unwrap_or_expr`, `try_expr`, `to_option_expr` |
| Equality | `eq_expr` (`almide_eq!`), `ne_expr` (`almide_ne!`) |
| Concat | `concat_expr` (type-dispatched: String vs List) |
| Power | `power_expr` (type-dispatched: `.pow()` vs `.powf()`) |
| Literals | `int_literal` (`{value}i64`), `float_literal` (`{value}f64`), `string_literal` (`"{value}".to_string()`), `list_literal` (`vec![{elements}]`) |
| Statements | `let_binding`, `var_binding`, `assignment` |
| Declarations | `fn_decl`, `effect_fn_decl`, `test_block`, `struct_decl`, `enum_decl` |
| Types | `type_int` (`i64`), `type_string` (`String`), `type_list` (`Vec<{inner}>`), `type_map` (`HashMap<{key}, {value}>`) |
| Loops | `for_loop`, `while_loop`, `break_stmt`, `continue_stmt` |
| Lambda | `lambda` (`move \|{params}\| {{ {body} }}`), `lambda_single` |
| Match | `match_expr`, `match_arm`, `pattern_some`, `pattern_ok`, `pattern_variant`, etc. |
| Top-level | `top_let_const`, `top_let_lazy` (`LazyLock`) |
| Module calls | `module_call` (`almide_rt_{module}_{func}({args})`) |

## 5. Walker

**Source**: `src/codegen/walker/mod.rs`

The walker traverses typed IR and renders using templates. It is fully target-agnostic -- zero `if target == Rust` checks. Target differences are handled entirely by passes and templates.

### RenderContext

```rust
pub struct RenderContext<'a> {
    pub templates: &'a TemplateSet,
    pub var_table: &'a VarTable,
    pub indent: usize,
    pub target: Target,
    pub auto_unwrap: bool,          // true inside effect fn (not test)
    pub ann: CodegenAnnotations,
    pub type_aliases: HashMap<Sym, Ty>,
    pub minimal_generic_bounds: bool, // Clone only for bundled .almd modules
    pub repr_c: bool,               // Emit #[repr(C)] on structs/enums
}
```

### Rendering Functions

Split across submodules:

| Module | Function | Renders |
|--------|----------|---------|
| `walker/expressions.rs` | `render_expr()` | Expressions (recursively renders sub-expressions) |
| `walker/statements.rs` | `render_stmt()`, `render_pattern()` | Statements, match patterns |
| `walker/types.rs` | `render_type()` | Type annotations (named records, generics, tuples) |
| `walker/declarations.rs` | `render_type_decl()` | Struct, enum, alias declarations |
| `walker/mod.rs` | `render_function()`, `render_program()` | Function declarations, full program assembly |
| `walker/helpers.rs` | `terminate_stmt()`, `ty_contains_name()` | Utilities |

### Program Rendering Order

`render_program()` assembles the output in this order:

1. Anonymous record struct definitions
2. Type declarations (struct, enum, alias)
3. Top-level lets (const and lazy)
4. Non-test functions
5. Test functions (wrapped in `mod tests { use super::*; ... }`)
6. Imported module functions (prefixed with `almide_rt_{module}_{func}`)

### Function Name Sanitization

Function names are sanitized for target compatibility: spaces, dots, hyphens become underscores. Special characters (`+`, `/`, `*`, `(`, `)`, etc.) are replaced with descriptive names (`_plus_`, `_div_`, `_mul_`, etc.). Target-specific keywords are escaped via the `keyword_escape` template (Rust: `r#name`).

Test functions are prefixed with `__test_almd_` to avoid collision with real functions.

## 6. Rust Target Specifics

**Source**: `src/codegen/mod.rs`, `emit_source()`

### Preamble

Every Rust output starts with:

1. `#![allow(...)]` — suppress unused warnings
2. `use std::collections::{HashMap, HashSet};`
3. `AlmideConcat` trait — overloaded `+` for String and Vec concatenation
4. `almide_eq!` / `almide_ne!` macros — deep equality comparison
5. Runtime modules (only those referenced by user code)

### Runtime Inclusion

Runtime modules are embedded in the compiler binary via `include_str!` (generated by `build.rs`). At emit time, the emitter scans user code for `almide_rt_{module}_` patterns and includes only referenced modules. `mod tests { ... }` blocks are stripped from runtime source to avoid conflicts.

Inter-module runtime dependencies are resolved (e.g., `json` depends on `value`).

```almd
// Using list.map triggers inclusion of the list runtime module
import list

fn main() =
  let xs = [1, 2, 3]
  let doubled = xs.map(|x| x * 2)
  println(doubled)
```

```rust
// Output includes list runtime functions (almide_rt_list_*)
// but NOT string, map, json, etc.
```

### Effect Functions

`effect fn` compiles to Rust functions returning `Result<T, String>`. The `ResultPropagationPass` inserts `?` on fallible calls automatically. The template for `effect_fn_decl` is identical to `fn_decl` because the return type in IR is already `Result<T, String>`.

```almd
effect fn read_config(path: String) -> String =
  let content = fs.read_text(path)
  content
```

```rust
pub fn read_config(path: String) -> Result<String, String> {
    let content: String = almide_rt_fs_read_text(&path)?;
    content
}
```

### Generic Bounds

Two levels of generic bounds:

- **Full** (user functions): `T: Clone + std::fmt::Debug + PartialEq + PartialOrd`
- **Minimal** (bundled .almd module functions): `T: Clone`

## 7. WASM Target

**Source**: `src/codegen/emit_wasm/mod.rs`

The WASM target emits a standalone binary directly from IR, with no intermediate source code and no rustc dependency. It targets WASI preview1.

### Architecture

```
IrProgram -> WasmEmitter (register + compile) -> wasm_encoder::Module -> Vec<u8>
```

Uses the `wasm_encoder` crate to produce a valid WASM module with:
- Type section, Function section, Export section
- Memory section (linear memory, bump allocator)
- Data section (string literals, scratch areas)
- Code section (compiled functions)

### Memory Layout

```
[0..16)      Scratch area (iov struct for WASI fd_write)
[16..48)     int_to_string scratch buffer
[48]         Newline byte (0x0A)
[49..N)      String literal data ([len:i32][data:u8...] per string)
[N..)        Heap (bump allocator, grows upward)
```

### Module Organization

The WASM emitter is split across specialized submodules:

| Module | Responsibility |
|--------|---------------|
| `values.rs` | Value representation and type mapping |
| `expressions.rs` | Expression compilation |
| `statements.rs` | Statement compilation |
| `functions.rs` | Function compilation and registration |
| `control.rs` | Control flow (if/else, loops, match) |
| `strings.rs` | String literal encoding |
| `collections.rs` | List, map, set operations |
| `closures.rs` | Lambda/closure compilation |
| `equality.rs` | Deep equality comparison |
| `runtime.rs`, `runtime_eq.rs` | Built-in runtime functions |
| `rt_string.rs`, `rt_string_extra.rs` | String runtime operations |
| `rt_numeric.rs` | Numeric runtime operations |
| `rt_value.rs`, `rt_regex.rs` | Value/regex runtime |
| `calls_*.rs` | Stdlib call compilation (string, list, map, option, etc.) |
| `scratch.rs` | Scratch allocator for temporary memory |
| `dce.rs` | Dead code elimination |
| `wasm_macro.rs` | Helper macros for WASM instruction emission |

## 8. CodegenOptions

**Source**: `src/codegen/mod.rs`

```rust
pub struct CodegenOptions {
    pub repr_c: bool,  // Emit #[repr(C)] on structs/enums for stable C ABI layout
}
```

When `repr_c` is true, the `struct_decl` and `enum_decl` templates select the `when_attr = "repr_c"` variant, prepending `#[repr(C)]` to the derive block.

## 9. Module Function Naming

**Source**: `src/codegen/walker/mod.rs`, `render_program()`

Imported module functions are emitted with a prefixed name:

```
fn almide_rt_{module}_{function}(...)
```

The module identifier comes from `IrModule.versioned_name` (if set) or `IrModule.name`, with dots replaced by underscores. For example, module `string` version `1.0` with `versioned_name = "string_1_0"` produces `almide_rt_string_1_0_trim(...)`.

The template `module_call` handles the call site:

```toml
[module_call]
template = "almide_rt_{module}_{func}({args})"
```

`StdlibLoweringPass` rewrites `CallTarget::Module { module: "list", func: "map" }` into `CallTarget::Named { name: "almide_rt_list_map" }` with IR-level argument decoration (BorrowStr, BorrowRef, ToVec, LambdaClone, Direct) based on the build.rs-generated `arg_transforms` table.

## 10. CodegenAnnotations

**Source**: `src/codegen/annotations.rs`

Annotations are populated by nanopass passes and read by the walker. The walker never checks types or context directly.

```rust
pub struct CodegenAnnotations {
    pub lazy_vars: HashSet<VarId>,                          // Top-level lazy vars (need *DEREF)
    pub ctor_to_enum: HashMap<String, String>,              // Constructor -> enum name (Red -> Color)
    pub anon_records: HashMap<Vec<String>, String>,          // Field names -> generated struct name
    pub named_records: HashMap<Vec<String>, String>,         // Field names -> declared type name
    pub recursive_enums: HashSet<String>,                    // Enums with recursive variants
    pub boxed_fields: HashSet<(String, String)>,             // (ctor, field) pairs needing Box::new()
    pub default_fields: HashMap<(String, String), IrExpr>,   // Default field values for constructors
}
```

## 11. TargetAttrs

**Source**: `src/codegen/pass.rs`

Per-node attributes set by passes, consumed by the template renderer:

```rust
pub struct TargetAttrs {
    pub needs_try: bool,             // Append ? for auto-propagation
    pub needs_clone: bool,           // Emit .clone()
    pub needs_borrow: bool,          // Emit & reference
    pub needs_box: bool,             // Wrap in Box<T>
    pub none_type_hint: Option<String>, // None::<T> with explicit type
    pub match_as_str: bool,          // .as_str() on match subject
    pub lazy_init: bool,             // Top-level let -> LazyLock
    pub option_erased: bool,         // (TS) some(x) -> x
    pub result_wrapped: bool,        // (TS) Result in { ok, value/error }
}
```
