# Almide Roadmap

## Module System v2

### Design Principles

- **File = namespace**. Each `.almd` file is its own namespace. No barrel files, no `export` syntax, no `module` declaration.
- **`mod.almd` is optional**. If present, it defines the package's top-level namespace. Other files are accessible as sub-namespaces.
- **Only `src/main.almd` is special** — required for `almide run` / `almide build`.
- **Visibility controls access**, not file structure. `fn` = public, `mod fn` = same project, `local fn` = same file.

### Project Structure

```
myapp/ (application)               mylib/ (library)
  almide.toml                        almide.toml
  src/                               src/
    main.almd    ← fn main             mod.almd       ← package top-level (optional)
    config.almd                        parser.almd
    http/                              formatter.almd
      client.almd                      utils.almd
      server.almd                    tests/
  tests/                               parser_test.almd
    config_test.almd
```

### almide.toml

```toml
[package]
name = "mylib"
version = "0.1.0"

[dependencies]
json = { git = "https://github.com/almide/json", tag = "v1.0.0" }
```

No `type = "app"` / `type = "lib"` needed — determined by file existence.

### Import Syntax

#### Self imports (same project)

```almide
import self.config             // config.load(...)
import self.http.client        // client.get(...)
import self.http.client as c   // c.get(...)
```

#### External package imports

```almide
import mylib                   // mylib.parse(...), mylib.parser.parse(...)
import mylib.parser            // parser.parse(...)
import mylib.parser as p       // p.parse(...)
```

#### Stdlib imports

```almide
import string                  // string.trim(...)
import regex                   // regex.find(...)
```

### Package Access Model

When a user imports an external package, what they can access depends on whether `src/mod.almd` exists in that package:

#### With `mod.almd`

```
mylib/src/
  mod.almd        ← fn fuga(), fn hello()
  a.almd          ← fn fuga(), fn bar()
  b.almd          ← fn greet()
```

```almide
import mylib

mylib.fuga()       // mod.almd の fn fuga
mylib.hello()      // mod.almd の fn hello
mylib.a.fuga()     // a.almd の fn fuga (no conflict — different namespace)
mylib.a.bar()      // a.almd の fn bar
mylib.b.greet()    // b.almd の fn greet
```

`mod.almd` defines the package's top-level namespace. Other files are sub-namespaces accessed via `pkg.file.func()`.

#### Without `mod.almd`

```
mylib/src/
  parser.almd     ← fn parse()
  formatter.almd  ← fn format()
```

```almide
import mylib

mylib.parser.parse(...)      // OK — sub-namespace access
mylib.formatter.format(...)  // OK
mylib.parse(...)             // ❌ Error — no mod.almd, no top-level namespace
```

```almide
import mylib.parser          // Direct sub-module import also works

parser.parse(...)            // OK
```

### File Resolution Rules

| import | resolved location |
|---|---|
| `import self.utils` | `src/utils.almd` |
| `import self.http.client` | `src/http/client.almd` or `src/http/client/mod.almd` |
| `import mylib` | dep `src/mod.almd` (top-level) + all `src/*.almd` (sub-namespaces) |
| `import mylib.parser` | dep `src/parser.almd` or dep `src/parser/mod.almd` |

### 3-Level Visibility ✅ Implemented

| Syntax | Scope | Rust output |
|---|---|---|
| `fn f()` | public (default) | `pub fn` |
| `mod fn f()` | same project only | `pub(crate) fn` |
| `local fn f()` | this file only | `fn` (private) |

- Same modifiers apply to `type` declarations
- `pub` keyword is accepted for backward compatibility (no-op since default is already public)
- `mod fn` is invisible to external importers — enforced at checker stage

### Checker-level visibility errors ✅ Implemented

- [x] Error at checker stage when `local fn` is called from an external module
- [x] Error at checker stage when `mod fn` is called from an external package
- [x] Error message: "function 'xxx' is not accessible from module 'yyy'"

### Deprecations

| Old | New | Status |
|---|---|---|
| `module xxx` declaration | Not needed | Parser accepts but warns |
| `lib.almd` as package entry | `mod.almd` | Searched as fallback for now |

### Implementation Steps

- [x] Resolver: support `import pkg` loading `mod.almd` + sub-namespace files
- [x] Resolver: support `import pkg.submodule` for direct sub-module access
- [ ] Checker: validate cross-package access respects `mod.almd` boundary
- [x] CLI: `almide init` template — remove `module main` from generated code
- [x] CLI: `almide fmt` without args — format all `src/**/*.almd` recursively
- [x] CLI: `almide --help` and `almide --version`
- [x] CLI: `--dry-run` → `--check` rename for `almide fmt` (keep `--dry-run` as alias)
- [x] CLI: `almide build --release` (opt-level=2)
- [ ] Deprecation warning for `module` declarations
- [ ] Deprecation warning for `lib.almd` as package entry (suggest rename to `mod.almd`)

### Test Repository

- https://github.com/almide/mod-sample — for verifying visibility + self import behavior

---

## User-Defined Generics

Currently `List[T]`, `Option[T]`, `Result[T, E]` etc. are compiler built-ins, but users cannot define their own generic types or functions.

### Proposed Syntax

```almide
// generic type
type Stack[T] =
  | Empty
  | Push(T, Stack[T])

// generic function
fn map[A, B](xs: List[A], f: fn(A) -> B) -> List[B] =
  match xs {
    [] => []
    [head, ...tail] => [f(head)] ++ map(tail, f)
  }

fn identity[T](x: T) -> T = x
```

### Implementation Steps

- [ ] Parser: parse generic parameters in `fn name[T, U](...) -> ...` (partial support exists in `try_parse_generic_params`)
- [ ] Type checker: introduce type variables, type inference (unification-based)
- [ ] Rust emitter: convert to `fn name<T, U>(...) -> ...`
- [ ] TS emitter: convert to `function name<T, U>(...): ...` (type erasure in JS mode)

### Design Decisions

- Type parameters use `[T]` notation (consistent with existing `List[T]` in Almide)
- Type inference is the primary approach; explicit type arguments at call sites are not required
- Type constraints will be introduced after trait implementation, e.g. `fn sort[T: Ord](xs: List[T])`

---

## trait / impl

Parser support is complete (`trait` / `impl` declarations are parsed and stored in AST). Checker and emitter are not yet implemented.

### Syntax (parser-complete)

```almide
trait Show {
  fn show(self) -> String
}

impl Show for Color {
  fn show(self) -> String = match self {
    Red => "red"
    Green => "green"
    Blue => "blue"
  }
}
```

### Implementation Steps

- [ ] checker: register trait method signatures, type-check impl bodies
- [ ] checker: verify that impl provides all methods required by the trait
- [ ] Rust emitter: output `impl Trait for Type { ... }` directly
- [ ] TS emitter: output traits as interfaces, dispatch method calls
- [ ] `self` parameter handling: UFCS (both `show(color)` and `color.show()` work)

### Design Notes

- Almide has no classes. trait + impl + pattern matching is how type behavior is defined
- `self` is exclusively the receiver in trait methods, not a class instance reference
- `self` in `import self.xxx` is a separate use (distinguishable by context: import statement vs parameter list)
- `deriving` is already parser-complete (`type Color = ... deriving Show, Eq`)

---

---

## String Handling ✅ Implemented

### Heredoc

Multi-line strings with `"""..."""` syntax.

```almide
let query = """
  SELECT *
  FROM users
  WHERE id = ${user_id}
"""
```

- `"""..."""` syntax (consistent with Python/Kotlin/Swift)
- Leading whitespace stripped based on minimum indent of non-empty lines (Kotlin trimIndent)
- Interpolation `${expr}` works the same as in regular strings
- Raw heredoc: `r"""..."""` (no escape processing, no interpolation)
- Implemented entirely in the lexer — no AST, parser, or emitter changes needed

---

## stdin / Interactive I/O ✅ Implemented

### API

```almide
import io

effect fn main() -> Result[Unit, String] = {
  io.print("Enter weight (kg): ")
  let weight_str = io.read_line()
  let weight = float.parse(weight_str)
  // ...
}
```

### stdlib functions

```
io.read_line() -> String          // read one line from stdin (blocking), effect fn
io.print(s: String)               // print without newline (stdout), effect fn
io.read_all() -> String           // read all of stdin, effect fn
```

- All are effect fns (require `effect fn` context)
- `io.print` complements the existing `println` (which always adds newline)
- Rust emitter: uses `std::io::BufRead` / `std::io::Write`
- TS emitter: Deno → `prompt()` / `Deno.stdout.writeSync`, Node → `fs.readSync(0, ...)` / `process.stdout.write`

---

## Compiler Hardening

Eliminate all panics and unhandled edge cases. Other languages never crash on invalid input — Almide shouldn't either.

### Panic elimination ✅

All `unwrap()`, `panic!()` calls in compiler source eliminated. Generated code uses `expect()` with descriptive messages.

- [x] Parser: `panic!("Parser: no tokens available")` → static EOF token fallback
- [x] Emitter: `.unwrap()` on character case conversion → `.unwrap_or(c)` (emit_ts/mod.rs)
- [x] Emitter: `final_expr.unwrap()` in do-block → `.expect("guarded by is_some()")` (emit_rust/blocks.rs)
- [x] Checker: `path.last().unwrap()` in import resolution → `.map().unwrap_or()` (check/mod.rs)
- [x] CLI: `unwrap()` on file I/O in init/build commands → proper `if let Err` with exit(1) (cli.rs)
- [x] Codegen: `/dev/urandom` direct read with `unwrap()` → `.map_err()?` propagation (random module)
- [x] Codegen: `UNIX_EPOCH` duration `.unwrap()` → `.unwrap_or_default()` (time/env modules)
- [x] Project: `.unwrap()` on split results → `.expect()` with reason (project.rs)
- [x] Generated code: thread spawn/join `.unwrap()` → `.expect()` with message (emit_rust/program.rs)

### Codegen `todo!()` fallbacks ✅

All 16 module fallbacks replaced with compile-time ICE (Internal Compiler Error) that exits with code 70 instead of silently generating broken Rust code.

- [x] Audit all `format!("/* {}.{} */ todo!()", ...)` patterns in emit_rust/calls.rs — 16 modules
- [x] Replace with `eprintln!("internal error: ...")` + `exit(70)` — catches mismatches immediately
- [x] Verified: all stdlib signatures in `lookup_sig()` have corresponding emitter implementations (no gap)

### Error message improvements

- [x] Import resolution failures: include file path tried and hint for typos (already excellent)
- [x] Effect fn called outside effect context: suggest adding `effect` keyword (already excellent)
- [x] Interpolated string validation at checker stage — parse and type-check `${expr}` in checker, report syntax errors early
- [x] Parser error hints: type name casing, function name casing, parameter name hints, pattern syntax guide

---

## Stdlib Completeness

Fill gaps that make Almide less capable than Python/Go for everyday tasks.

### int module ✅

- [x] `int.parse(s)` → `Result[Int, String]` (parse decimal string)
- [x] `int.parse_hex(s)` → `Result[Int, String]`
- [x] `int.abs(n)` → `Int`
- [x] `int.min(a, b)` / `int.max(a, b)`

### string module ✅

- [x] `string.pad_right(s, n, ch)` → `String`
- [x] `string.trim_start(s)` / `string.trim_end(s)` → `String`
- [x] `string.count(s, sub)` → `Int`

### list module ✅

- [x] `list.index_of(xs, x)` → `Option[Int]`
- [x] `list.last(xs)` → `Option[T]`
- [x] `list.chunk(xs, n)` → `List[List[T]]`
- [x] `list.sum(xs)` / `list.product(xs)` → `Int`

### CLI improvements

- [x] `almide --help`: detailed help with all options and examples
- [ ] `almide check`: show progress for multi-file projects
- [ ] Exit codes: distinguish parse error (65), type error (66), codegen error (70)

---

## Codegen Optimization

Almide generates Rust code that is near-identical in performance to hand-written Rust for numeric workloads (n-body: 1.74s vs Rust 1.69s). However, heap-allocated types (String, List) incur unnecessary clone overhead. The goal is to close this gap **without exposing ownership to the user**.

### Phase 1: Eliminate unnecessary clones (transparent)

No language changes — the emitter generates smarter Rust code.

#### 1a. Last-use move analysis

If a variable's last usage is a function call or assignment, emit it directly instead of `.clone()`.

```almide
let name = "hello"
println(name)        // name is never used again
```

```rust
// Before: println!("{}", name.clone());
// After:  println!("{}", name);          // move, no clone
```

- [ ] Liveness analysis in emitter: track last usage of each variable
- [ ] Emit `.clone()` only when the variable is used again after the current expression
- [ ] Handle control flow (if/match branches) conservatively

#### 1b. String concatenation optimization

Detect `var = var ++ expr` pattern and emit `push_str` instead of allocating a new String.

```almide
var s = ""
for i in 0..n {
  s = s ++ "x"
}
```

```rust
// Before: s = format!("{}{}", s.clone(), "x".to_string());
// After:  s.push_str("x");
```

- [ ] Detect `Assign { name, value: BinOp(PlusPlus, Ident(same_name), rhs) }` pattern
- [ ] Emit `{name}.push_str(&{rhs})` for String, `.extend()` for List

### Phase 2: In-place mutation syntax

New syntax for mutating elements of `var` collections and record fields.

#### 2a. List element update

```almide
var xs = [1, 2, 3]
xs[1] = 99
```

```rust
xs[1] = 99i64;
```

- [ ] Parser: `Stmt::IndexAssign { target, index, value }`
- [ ] Checker: verify target is `var`, element type matches
- [ ] Emitter: direct index assignment

#### 2b. Record field update

```almide
var user = { name: "alice", age: 30 }
user.age = 31
```

```rust
user.age = 31i64;
```

- [ ] Parser: `Stmt::FieldAssign { target, field, value }`
- [ ] Checker: verify target is `var`, field exists, type matches
- [ ] Emitter: direct field assignment

### Phase 3: Borrow inference (future)

The compiler infers when a function parameter is read-only and emits `&str` / `&[T]` instead of owned types. Callers no longer need to clone.

```almide
fn len(s: String) -> Int = string.len(s)
```

```rust
// Before: fn len(s: String) -> i64 { s.clone().len() as i64 }
// After:  fn len(s: &str) -> i64 { s.len() as i64 }
```

- [ ] Analyze function bodies: does the parameter escape, get stored, or get mutated?
- [ ] If read-only: emit `&str` for String, `&[T]` for List
- [ ] Adjust call sites: pass `&x` instead of `x.clone()`

### Priority order

| Step | Difficulty | Impact | User-visible change |
|---|---|---|---|
| 1a. Last-use move | Medium | High | None (transparent) |
| 2a. List element update | Low | High | New syntax `xs[i] = v` |
| 1b. String concat optimization | Low | Medium | None (transparent) |
| 2b. Record field update | Low | Medium | New syntax `r.f = v` |
| 3. Borrow inference | High | High | None (transparent) |

---

## Other

- [ ] Package registry (to be considered in the future)
