> Last updated: 2026-10-31

# Codegen Specification

Almide's codegen (`crates/almide-codegen`) is a three-layer architecture that
transforms typed IR into source output for the **Rust** target (and WGSL). All
semantic decisions are made in the IR before any text is emitted.

The **WASM** target does not flow through this crate. The shared route in
`src/wasm_route.rs` uses the structural emitter in `crates/almide-wasm` to
produce Wasm bytes directly; the retired WAT renderer is not a fallback. See
[../ARCHITECTURE.md](../ARCHITECTURE.md) and
[../wasm/WASM-OUTPUT.md](../wasm/WASM-OUTPUT.md).

**Source**: `crates/almide-codegen/src/lib.rs`

```
IrProgram (typed IR)
    |
Layer 1: Nanopass Pipeline (target-specific semantic rewrites)
    |
Layer 2: Emit (target-specific output)
    Rust  -> Template Renderer (TOML-driven) -> source code
    WGSL  -> emit_wgsl                       -> shader source
```

## 1. Entry Point

**Source**: `crates/almide-codegen/src/lib.rs`

The single entry point is `codegen(program, target) -> CodegenOutput`
(`codegen_with` accepts `CodegenOptions`).

```rust
pub enum CodegenOutput {
    Source(String),   // Rust / WGSL targets
    Binary(Vec<u8>),  // legacy variant; no longer produced
}
```

The flow:

1. `target::configure(target)` builds a `TargetConfig` containing the nanopass pipeline and template set
2. The pipeline runs all applicable passes, transforming the IR in place
3. Target-specific emit:
   - **Rust**: Walker renders IR via templates, prepends runtime preamble
   - **WGSL**: `emit_wgsl::emit()` renders shader source
   - **Wasm**: `unreachable!` — routed to the structural `almide-wasm` emitter before reaching this crate

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

**Source**: `crates/almide-codegen/src/pass.rs`

```rust
pub enum Target {
    Rust,
    Wasm,   // tombstone: codegen arm is unreachable! (v0 emitter retired, #782)
    Wgsl,
}
```

Active targets in this crate: **Rust** and **WGSL**. TypeScript codegen was
removed (2026-03-28). The current Wasm route is `src/wasm_route.rs` →
`crates/almide-wasm`, not the retired MIR-to-WAT renderer.

Each target is configured via `target::configure()` which returns a `TargetConfig`:

**Source**: `crates/almide-codegen/src/target.rs`

```rust
pub struct TargetConfig {
    pub target: Target,
    pub pipeline: Pipeline,
    pub templates: TemplateSet,
}
```

## 3. Nanopass Pipeline

**Source**: `crates/almide-codegen/src/pass.rs`, `crates/almide-codegen/src/target.rs`

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
- Verifies IR integrity and every established `Postcondition` after EVERY
  pass in EVERY profile — a violation is a compiler bug and fails the build,
  release included. The per-pass walk used to be debug-only (`ALMIDE_VERIFY_IR`
  opted a release build in) on a stale cost claim of ~1.2 s per file; measured
  over the 218 files of spec/lang it is ~8 ms per file, so the profile that
  ships now runs the same walk as `cargo test`. `ALMIDE_IR_FAULT=<pass>`
  (harness switch) injects a violation after the named pass, the release
  binary's negative control (`tests/ir_verify_every_profile_test.rs`)

### Rust pipeline source of truth

[`build_pipeline(Target::Rust)`](../../crates/almide-codegen/src/target.rs)
is the authoritative pass order, including dependencies and conditional gates.
This is an implementation pipeline, not a frozen language-level sequence.

Its stages include type/call normalization, region and ownership analysis,
borrowing/cloning, effect and Result adaptation, stream fusion, Rust-specific
lowering, runtime-call normalization and final linking/storage decisions.
`RegionWindowPass`, `StreamFusionPass` and `OwnedSourceHoistPass` are part of
the current implementation; the old `AutoParallelPass` table is obsolete.
Parallel scheduling follows the explicit `fan` forms, not that removed pass.


## 4. Template System

**Source**: `crates/almide-codegen/src/template.rs`, `crates/almide-codegen/templates/rust.toml`

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

The Rust template set (`crates/almide-codegen/templates/rust.toml`) defines constructs for:

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
| Top-level | `top_let_const`, `top_let_lazy` (`LazyLock`), `top_let_thread_lazy` (per-thread slot for a closure-holding value) |
| Module calls | `module_call` (`almide_rt_{module}_{func}({args})`) |

## 5. Walker

**Source**: `crates/almide-codegen/src/walker/mod.rs`

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

Test functions are named `__test_almd_<NNNN>_<label>` (`almide_ir::test_fn_name`): the prefix keeps them apart from real functions, and the declaration-order ordinal keeps two tests apart whose labels sanitize to one spelling (`"a b"` / `"a_b"`, #3488).

## 6. Rust Target Specifics

**Source**: `crates/almide-codegen/src/lib.rs`, `emit_source()`

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
  let doubled = xs.map((x) => x * 2)
  println(doubled)
```

```rust
// Output includes list runtime functions (almide_rt_list_*)
// but NOT string, map, json, etc.
```

### Parameter passing on the native target

**Source**: `pass_borrow_inference_ownership.rs` (`param_borrow`), `pass_borrow_lowering.rs`.

A non-`mut` param of a borrow-eligible type (`String`, `List[T]`, a record) is
passed **by reference** (`&str` / `&[T]` / `&T`) unless the body needs the value
owned. The body needs it owned when it:

- returns the param, concatenates it, builds it into a record / list / tuple
  literal, matches on it, or seeds a fold with it;
- captures it in a closure (closures capture through `Rc<dyn Fn>`, which cannot
  hold a borrow — the capture is a move, or a clone when the param is read again);
- hands it to a **stdlib** slot that consumes (`list.push`'s element): those
  lower into runtime calls that take the value;
- iterates it — a `for` loop, or a list combinator the fusion pass inlines —
  with a body that needs the **elements** owned (below).

**An iteration's source follows its elements, not the combinator's slot**
(#2287). `list.map`, `fold`, `filter`, `any`, … declare their list slot
`@consume(xs)`, but that describes the unfused runtime twin; the fused chain
and the `for` loop decide the source's mode from what the body does with
each element (`element_reads_only`, one rule read by the borrow verdict, the
clone pass and the ownership certifier):

- a `Copy` element (`List[Int]`) never needs the source owned: `mapped(ns:
  List[Int]) = ns |> list.map((n) => n * 2)` is `mapped(ns: &[i64])` over
  `ns.iter().cloned()`, and a caller that reads its list twice clones nothing;
- a heap element every receiving lambda only READS — a borrowed call slot, a
  field read, a clone, a `&str` comparison, an interpolation part, or a
  consuming use the clone pass clones anyway because the binder is read again
  in the same iteration — leaves the source borrowed and binds `&T` off
  `xs.iter()`: `lens(ws: &[String]) = ws.iter().map(|w| len(w.as_str()))`,
  no element cloned (a `filter`-family step, which Rust hands `&&T`, is
  rebound `let w = *w` instead of `let w = w.clone()`);
- an element that leaves the chain owned — returned as the mapped value,
  concatenated, built into a value, handed to an owned slot, collected by a
  `filter` / `find` — keeps the source consumed: `shouted(ws: Vec<String>)`
  over `ws.into_iter()`, the caller's last use moves the list and each
  element moves out for free where a borrowed source could only clone it.

This is the per-element rule the reference-counted compilers apply (a list
read borrows into the list; the element is retained only when an occurrence
demands it), lifted to the source's mode because a borrowed source on the
native leg can only clone.

A list-combinator callback the stream-fusion pass inlines (`list.map(xs, (x) =>
… t …)`, `filter`, `fold`, `any`, …) is **not a closure** for this policy: the
pass runs before the borrow pass, and a chain step is a scope that runs once
per element inside the chain and never escapes it. A param it only reads stays
`&T` — `(x) => x + list.len(t.names)` keeps `t: &Table` — and the step renders
without `move` and without a capture bind, borrowing what it reads for the
chain's duration. A read that consumes inside the step (returning `t` per
element) is cloned there whether or not the param is owned, so it earns the
param nothing. Only a closure that outlives its call — returned, stored,
captured by another closure, or handed to a slot whose callee keeps it —
captures, and captures own (Lean, Koka and Roc make the same distinction: a
closure that exists owns what it holds; the win comes from erasing the closure
before ownership is decided).

**A fn-typed param is borrowed unless its callable escapes** (#2288). A user
higher-order fn's `f: (A) -> B` param is `&dyn Fn(A) -> B` when every
occurrence only CALLS it, borrows it, or hands it to another fn's borrowed fn
slot (the same fixed point that decides `&T`, optimistic for a pending or
self-recursive callee); it is the `Rc<dyn Fn>` handle when an occurrence lets
the callable outlive the call — returned, bound to a local, built into a
record / list, captured by a closure, handed to an owned slot (a runtime twin
such as `list.map`'s unfused form, a stored chain callback). A lambda literal
at a borrowed slot is then a scope like a chain step: `apply(xs, &|x| x * k)`
— no `Rc::new`, no `move`, no capture bind, and a param it reads stays
borrowed; a closure VALUE at that slot is lent through its handle (`&*g`); a
tail-recursive fn carries the borrowed callable through its loop like a
`&Vec<u8>`. This is the borrowed capture record the closure-free reference
passes as an ordinary argument, without lambda sets: the fn-typed param IS the
ABI, and the escape verdict decides which of its two forms a body needs. The
ownership certifier's C5 re-reads the verdict on the final IR (a `&dyn Fn`
param that escapes; a boxed closure at a borrowed slot), and
`ALMIDE_FN_ESCAPE_OFF=1` is its negative control.

A `let`-bound closure with the same lifetime is a scope too (#3455). When
every occurrence of its binder is one that rule lets through (called,
borrowed, lent to a `&dyn Fn` slot — never returned, stored, captured by an
outliving closure or used in a `fan` arm), and every variable it captures is
never written, no `var`, and — for a heap value — never consumed anywhere in
the fn, the bind is the non-`move` `let f = &|i: i64| …`: its reads are
borrows, the params it reads stay `&T`, and no caller clones its argument to
make the call. The verdict is the greatest fixed point inside each borrow
round, spelled `Borrow { Lambda }` once final, so `CaptureClone` adds no
`__cap_*` bind; a TCO-bound fn is left out. `ALMIDE_SCOPED_CLOSURE_OFF=1`
restores the owned capture.

Handing the param bare to a callee's owned slot (`stored(t, 1)` where `stored`
keeps `t` in a record) makes the param owned only when that site is the
param's LAST use: ownership then buys a move, and a caller that still needs
its value clones once. When the param is read again after the site (`{ let h
= stored(t, 1); h.n + plain(t) }`), the site is cloned whether or not the
param is owned — so the param stays `&T` and that one site owns its read
(`stored(t.clone(), 1)`): one clone at the site, none at any caller. The
allocation ledger (`tests/native_borrow_oracle_test.rs`) pins the
alternative's cost: cloning at every such site regardless of liveness (#2278's
first draft) raised the count of all five pinned programs. A `mut` param is
`&mut T` for its whole body and takes the same site-level clone at every
by-value position (#2266).

A **variant** param (`s: Shape`) follows the same policy, with one rule for its
`match`: the subject position reads the param by reference when every binder
the arms introduce is only READ — a `Copy` scalar, or a heap payload whose
every occurrence is a non-consuming position (a borrowed argument, a member
read, a comparison, an interpolation part, the subject of a further match).
The arms then bind `&T` payloads (Rust's default binding modes: a scalar read
derefs, a `&b` of a heap payload is the naked binder, a boxed payload matches
again as `&**b`), and no caller clones the value to pass it. One payload
returned, concatenated, built into a value, captured or handed to an owned slot
keeps the param owned: matching by value moves that payload out for free where
a borrowed match would clone it. The ownership certifier's C4 reads the same
predicate, so an owned variant param whose matches only read is a named defect
(`tests/variant_param_borrowed_test.rs`).

A param that is owned costs every caller that still needs its value a clone; a
borrowed param costs nothing at the call. Whether a fn ends up `&T` or `T` is
visible in the emitted Rust (`almide app.almd --target rust`), and the
ownership certifier (`ALMIDE_CERTIFY_OWNERSHIP=report`) names a param owned
that no occurrence needs owned.

### Effect Functions

A user `effect fn` declaring success type `T` has a fallible Result interface
(default error type `String`); an explicitly declared Result keeps its error
type. Source propagation is explicit: write `!`, or handle/discard the Result
as specified by ADR-0008. The frontend lowers that intent into IR; native
`ResultPropagationPass` adapts signatures, tails and calls to Rust
representations. It does not restore implicit propagation in source code.
The following Rust is schematic (runtime wrapper types are omitted).

```almd
effect fn read_config(path: String) -> String =
  let content = fs.read_text(path)!
  content
```

```rust
pub fn read_config(path: String) -> Result<String, String> {
    let content: String = almide_rt_fs_read_text(&path)?;
    Ok(content)
}
```

### Generic Bounds

Two levels of generic bounds:

- **Full** (user functions): `T: Clone + std::fmt::Debug + PartialEq + PartialOrd`
- **Minimal** (bundled .almd module functions): `T: Clone`

## 7. WASM Target (not in this crate)

**Source**: `src/wasm_route.rs`, `crates/almide-wasm`

The CLI and library consumers share one route: linked typed IR goes through
the structural emitter to Wasm bytes, not through MIR-to-WAT rendering or a
CLI `wat` assembly step. Programs outside the supported surface produce a
named wall (`E082` at the CLI); there is no fallback to the retired renderer.
The stock-WASI form used by browser runners is derived with
`almide_wasi::to_wasi`. Runtime imports and self-hosted standard-library bodies
are part of the linked program; they are not a reason to select a different
renderer.

Details: [../wasm/WASM-OUTPUT.md](../wasm/WASM-OUTPUT.md),
[../ARCHITECTURE.md](../ARCHITECTURE.md), [crates/CLAUDE.md](../../crates/CLAUDE.md).

## 8. CodegenOptions

**Source**: `crates/almide-codegen/src/lib.rs`

```rust
pub struct CodegenOptions {
    pub repr_c: bool,  // Emit #[repr(C)] on structs/enums for stable C ABI layout
}
```

When `repr_c` is true, the `struct_decl` and `enum_decl` templates select the `when_attr = "repr_c"` variant, prepending `#[repr(C)]` to the derive block.

## 9. Module Function Naming

**Source**: `crates/almide-codegen/src/walker/mod.rs`, `render_program()`

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

**Source**: `crates/almide-codegen/src/annotations.rs`

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

**Source**: `crates/almide-codegen/src/pass.rs`

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
}
```
