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

## Tuple & Record Improvements

### Named Record Construction ✅ Implemented

```almide
type Point = {x: Int, y: Int}

let p = Point {x: 1, y: 2}   // named construction
let q = {x: 3, y: 4}         // anonymous (still works)
```

- [x] Parser: `TypeName {field: value, ...}` → `Expr::Record { name: Some("TypeName"), ... }`
- [x] AST: `Expr::Record` has `name: Option<String>`
- [x] Rust emitter: `Point { x: 1i64, y: 2i64 }`
- [x] TS emitter: name ignored (plain JS object)
- [x] Formatter: preserves name in output

### Tuple Index Access ✅ Implemented

```almide
let t = (1, "hello")
let x = t.0     // → 1
let s = t.1     // → "hello"
```

- [x] Parser: integer literal after `.` → `Expr::TupleIndex`
- [x] AST: `Expr::TupleIndex { object, index }`
- [x] Checker: validate index within tuple bounds, return element type
- [x] Rust emitter: `(expr).0`
- [x] TS emitter: `(expr)[0]`
- [x] Formatter: preserves `t.0` syntax

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

### Stdlib Phase 5: HIGH priority gaps ✅

Functions that every mainstream language has and AI-generated code will expect.

#### string

- [x] `string.is_empty?(s)` → `Bool`
- [x] `string.reverse(s)` → `String`
- [x] `string.strip_prefix(s, prefix)` → `Option[String]` — remove prefix if present
- [x] `string.strip_suffix(s, suffix)` → `Option[String]` — remove suffix if present

#### list

- [x] `list.first(xs)` → `Option[T]` — alias-like for `list.get(xs, 0)`
- [x] `list.is_empty?(xs)` → `Bool`
- [x] `list.flat_map(xs, f)` → `List[U]` — map then flatten
- [x] `list.min(xs)` → `Option[T]` — minimum element
- [x] `list.max(xs)` → `Option[T]` — maximum element
- [x] `list.join(xs, sep)` → `String` — join `List[String]` with separator (UFCS: `xs.join(",")`)

#### map

- [x] `map.merge(a, b)` → `Map[K, V]` — merge two maps (b wins on conflict)
- [x] `map.is_empty?(m)` → `Bool`

#### fs

- [x] `fs.is_dir?(path)` → `Bool` (effect)
- [x] `fs.is_file?(path)` → `Bool` (effect)
- [x] `fs.copy(src, dst)` → `Result[Unit, IoError]` (effect)
- [x] `fs.rename(src, dst)` → `Result[Unit, IoError]` (effect)

#### process

- [x] `process.exec_status(cmd, args)` → `Result[{code: Int, stdout: String, stderr: String}, String]` (effect) — full exec result with exit code

### Stdlib Phase 6: MEDIUM priority gaps (future)

#### string
- `string.replace_first`, `string.last_index_of`, `string.to_float`

#### list
- `list.filter_map`, `list.group_by`, `list.take_while`, `list.drop_while`
- `list.count`, `list.partition`, `list.reduce`

#### map
- `map.map_values`, `map.filter`, `map.from_entries`

#### int / float
- `int.clamp`, `float.min`, `float.max`

#### path
- `path.stem`, `path.normalize`, `path.resolve`

#### fs
- `fs.walk`, `fs.stat`

#### json
- `json.get_float`, `json.from_float`, `json.stringify_pretty`

#### New modules (future)
- **encoding**: `base64_encode`, `base64_decode`, `url_encode`, `url_decode`
- **set**: `Set[T]` API — `new`, `from_list`, `add`, `remove`, `contains`, `union`, `intersection`, `difference`, `len`, `to_list`, `is_empty?`

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

## Stdlib Self-Hosting

Currently all stdlib functions are hardcoded in the compiler (`stdlib.rs` for type signatures, `emit_rust/calls.rs` for Rust codegen). This doesn't scale — every new function requires compiler changes. The goal: **Almide writes its own stdlib in Almide**, achieving automatic multi-target support.

### Why self-hosting matters

```
extern "rust" で書く → Rustでしか動かない
Almideで書く         → Rust/TS 両方に自動出力される
```

Almideの設計原則は「同じコードが複数ターゲットに出力される」こと。stdlibもこの原則に従うべき。`extern` は最終手段であり、主戦略は **Almideの表現力を上げてstdlibをAlmideで書く**。

### Architecture: Two-Layer Stdlib

```
┌──────────────────────────────────────────────┐
│  Upper layer: Almide stdlib packages          │  ← .almd files, written in Almide
│  string.reverse, list.flat_map, args.parse,   │     自動的にRust/TS両方で動く
│  hash.sha256, encoding.base64, csv.parse ...  │
└──────────────┬───────────────────────────────┘
               │ calls
┌──────────────▼───────────────────────────────┐
│  Lower layer: compiler primitives             │  ← hardcoded in calls.rs (20-30 functions)
│  fs.read_text, process.exec, string.len,      │     OS syscalls, data structure internals
│  list.get, map.set, int.to_string ...         │
└──────────────────────────────────────────────┘
```

### Phase 0: Language Primitives for Self-Hosting

Before Almide can write its own stdlib, the language needs low-level primitives. These are not stdlib functions — they are **language-level operators and types**.

#### 0a. Bitwise Operators

Required for: hash algorithms (SHA-1, SHA-256, MD5), encoding (base64, hex), compression, binary protocols.

| Operator | Name | Rust emit | TS emit | Notes |
|----------|------|-----------|---------|-------|
| `band(a, b)` | bitwise AND | `({} & {})` | `({} & {})` | |
| `bor(a, b)` | bitwise OR | `({} \| {})` | `({} \| {})` | |
| `bxor(a, b)` | bitwise XOR | `({} ^ {})` | `({} ^ {})` | `^` is already used for pow/xor contextually |
| `bshl(a, n)` | shift left | `({} << {})` | `({} << {})` | |
| `bshr(a, n)` | shift right | `({} >> {})` | `({} >>> {})` | unsigned shift in TS |
| `bnot(a)` | bitwise NOT | `(!{})` | `(~{})` | |

**Design choice**: Use named functions (`band`, `bor`, `bxor`) rather than symbolic operators (`&`, `|`, `^`). Rationale:
- `&` conflicts with potential reference syntax
- `|` is used for variant types and lambdas
- `^` is already used for power/XOR (contextual)
- Named functions are explicit, unambiguous, readable
- Most Almide code never needs bitwise ops — they shouldn't pollute the operator space

Implementation:
- [x] stdlib.rs: add `int.band`, `int.bor`, `int.bxor`, `int.bshl`, `int.bshr`, `int.bnot` signatures
- [x] emit_rust/calls.rs: emit corresponding Rust operators
- [x] emit_ts_runtime.rs: emit corresponding JS operators (note: `>>>` for unsigned shift)
- [x] Test: verify all operators with known values

#### 0b. Wrapping Arithmetic ✅

Required for: hash algorithms that operate on 32-bit unsigned integers with overflow wrapping.

```almide
int.wrap_add(a, b, bits)    // (a + b) mod 2^bits
int.wrap_mul(a, b, bits)    // (a * b) mod 2^bits
int.rotate_right(a, n, bits) // circular right rotation
int.rotate_left(a, n, bits)  // circular left rotation
int.to_u32(a)               // truncate to 0..2^32-1
int.to_u8(a)                // truncate to 0..255
```

Implementation:
- [x] stdlib.rs: add wrapping arithmetic signatures to `int` module
- [x] emit_rust/calls.rs: use Rust wrapping operations with bitmask
- [x] emit_ts_runtime.rs: use `Math.imul()`, manual rotation, `>>> 0` for u32
- [x] Test: SHA-256 style round operations (sigma0, Ch, Maj)

#### 0c. Byte Array Type (future consideration)

Currently bytes are `List[Int]` which is `Vec<i64>` — 8x memory overhead. For serious binary processing, a dedicated `Bytes` type may be needed. But `List[Int]` works for correctness and can be optimized later.

### Phase 1: Stdlib Package Mechanism ✅

Allow stdlib modules to be implemented as `.almd` files that ship with the compiler. No language changes needed — uses existing module system.

```
almide/
  stdlib/
    args.almd          ← argument parsing (pure Almide) ✅ implemented
    term.almd          ← terminal colors (pure Almide)
    csv.almd           ← CSV parsing (pure Almide)
    hash.almd          ← SHA-256, SHA-1 (pure Almide, uses bitwise ops)
    encoding.almd      ← base64, hex (pure Almide, uses bitwise ops)
```

#### Implementation Steps

- [x] Resolver: bundled stdlib via `include_str!` in compiler binary
- [x] Resolver: fallback to bundled source when module not found locally
- [x] stdlib.rs: `get_bundled_source()` returns embedded `.almd` source
- [x] Test: `args` module entirely in Almide as proof of concept
- [x] Bug fix: sanitize `?` in user module function names and calls (`flag?` → `flag_qm_`)

#### Proof of concept: `args` module

```almide
// stdlib/args.almd — argument parsing, pure Almide

fn flag?(name: String) -> Bool = {
  let args = env.args()
  list.any(args, fn(a) => a == "--" ++ name || a == "-" ++ string.slice(name, 0, 1))
}

fn option(name: String) -> Option[String] = {
  let args = env.args()
  let long = "--" ++ name
  let eq_match = list.find(args, fn(a) => string.starts_with?(a, long ++ "="))
  match eq_match {
    some(a) => string.strip_prefix(a, long ++ "=")
    none => {
      let idx = list.index_of(args, long)
      match idx {
        some(i) => list.get(args, i + 1)
        none => none
      }
    }
  }
}

fn option_or(name: String, default: String) -> String =
  match option(name) {
    some(v) => v
    none => default
  }

fn positional() -> List[String] =
  list.filter(env.args(), fn(a) => not string.starts_with?(a, "-"))
    |> list.drop(1)
```

#### hash module (pure Almide, after Phase 0)

```almide
// stdlib/hash.almd — SHA-256 in pure Almide

fn sha256(data: String) -> String = {
  let bytes = string.to_bytes(data)
  let padded = pad_message(bytes)
  // ... rounds using int.wrap_add, int.rotate_right, int.bxor etc.
  // ... outputs hex string
  // Runs on both Rust and TS targets — no extern needed
}
```

### Phase 2: Migrate & Extend Stdlib via .almd

方針: **既存のstring/list/map等はRustのまま残す**（既にRust/TS両方で動いており、HOFはラムダインライン最適化がある）。移行は **丸ごと置き換えられるモジュール** と **新規追加** に集中する。

#### 2a. path モジュール ✅ 完了

全5関数を `stdlib/path.almd` に移行。コンパイラの `STDLIB_MODULES` から除外済み。

| Function | Almide implementation |
|----------|----------------------|
| `join` | `++` with `/` separator, absolute child replaces |
| `dirname` | `split("/")` → take all but last → `join("/")` |
| `basename` | `split("/")` → last non-empty part |
| `extension` | `split(".")` on basename → last part |
| `is_absolute?` | `starts_with?(p, "/")` |

#### 2b. time モジュール ✅ 完了

全12関数を `stdlib/time.almd` に完全移行。`STDLIB_MODULES` から除外済み。
`now/millis/sleep` は `env.unix_timestamp/env.millis/env.sleep_ms` プリミティブのラッパー。
残り9関数（year/month/day/hour/minute/second/weekday/to_iso/from_parts）は純粋なAlmide実装（Hinnant日付算術）。

| Function | Almide implementation |
|----------|----------------------|
| `hour` | `(ts % 86400) / 3600` |
| `minute` | `(ts % 3600) / 60` |
| `second` | `ts % 60` |
| `weekday` | `(ts / 86400 + 4) % 7` |
| `year` | UNIX timestamp → date arithmetic (leap year calc) |
| `month` | same |
| `day` | same |
| `to_iso` | decompose + string formatting |
| `from_parts` | reverse date arithmetic |

#### 2c. 新規モジュール（コンパイラ変更ゼロで追加）

| Module | Functions | Needs bitwise? | Priority |
|--------|-----------|---------------|----------|
| `hash` | `sha256`, `sha1`, `md5` | Yes | HIGH |
| `encoding` | `base64_encode/decode`, `hex_encode/decode`, `url_encode/decode` | Yes | HIGH |
| `term` | `color`, `bold`, `dim`, `reset` | No | MEDIUM |
| `csv` | `parse`, `parse_with_header`, `stringify` | No | MEDIUM |

#### 2d. Phase 6 の新規関数を .almd で追加

Phase 6 で追加予定の派生関数は、コンパイラに追加せず `.almd` で実装する。ただし既存のハードコードモジュール (string/list/map) に関数を追加するには **ハイブリッドresolver** が必要（ハードコード + bundled .almd のマージ）。

候補:
- `list.filter_map`, `list.group_by`, `list.take_while`, `list.drop_while`
- `list.count`, `list.partition`, `list.reduce`
- `map.map_values`, `map.filter`, `map.from_entries`
- `string.replace_first`, `string.last_index_of`, `string.to_float`

#### Strategy summary

| 分類 | 方針 |
|------|------|
| **既存の string/list/map/int/float** | Rust のまま維持。既に両ターゲットで動作 |
| **既存の fs/process/io/env/json/regex/random/http** | Rust のまま維持。OS/crate依存 |
| **path** | ✅ `.almd` に移行済み |
| **time decomposition** | `.almd` に移行予定（now/millis/sleep は残留） |
| **新規モジュール** | `.almd` で作成（コンパイラ変更ゼロ） |
| **既存モジュールへの新規関数追加** | ハイブリッドresolver 実装後に `.almd` で追加 |
| **合計** | **157** | **60** | **99** | **38% を .almd に移行可能** |

### Phase 3: `extern` — Last Resort FFI

For cases where pure Almide is impractical (performance-critical inner loops, OS-specific APIs), `extern` provides target-specific escape hatches.

```almide
extern "rust" {
  fn fast_sha256(data: List[Int]) -> String
}
extern "ts" {
  fn fast_sha256(data: List[Int]) -> String
}
```

This is intentionally the **last phase** — if Almide's own language features are sufficient, extern is rarely needed. It exists for:
- Performance-critical code where pure Almide is too slow
- Platform-specific APIs (WASM, native GUI, etc.)
- Wrapping existing ecosystem libraries

#### Implementation Steps (when needed)

- [ ] Lexer: add `Extern` token
- [ ] Parser: parse `extern "target" { fn name(params) -> ret }` declarations
- [ ] AST: add `Decl::Extern` variant
- [ ] Checker: register extern fn signatures
- [ ] Emitter: emit target-specific function stubs

### Priority Order

| Phase | What | Difficulty | Impact | Enables |
|-------|------|-----------|--------|---------|
| **0a.** Bitwise operators | `int.band/bor/bxor/bshl/bshr/bnot` | Low | High | hash, encoding, binary protocols |
| **0b.** Wrapping arithmetic | `int.wrap_add/wrap_mul/rotate_right/left` | Low | High | SHA-256, SHA-1 in pure Almide |
| **1.** Stdlib package mechanism | resolver + bundled .almd | Medium | High | args, term, csv, hash, encoding |
| **2.** Migrate existing stdlib | move pure functions to .almd | Low | Medium | shrinks calls.rs |
| **3.** `extern` FFI | target-specific escape hatch | Medium-High | Low (rarely needed) | platform-specific APIs |

### CLI Stdlib Gaps (to be filled via self-hosting)

#### Via Almide stdlib packages (after Phase 0 + Phase 1)

| Module | Functions | Needs bitwise? | Priority |
|--------|-----------|---------------|----------|
| `args` | `flag?`, `option`, `option_or`, `positional`, `positional_at` | No | CRITICAL |
| `hash` | `sha256`, `sha1`, `md5`, `sha256_bytes` | Yes | CRITICAL |
| `encoding` | `base64_encode/decode`, `hex_encode/decode`, `url_encode/decode` | Yes | HIGH |
| `term` | `color`, `bold`, `dim`, `is_tty?`, `width` | No | MEDIUM |
| `csv` | `parse`, `parse_with_header`, `stringify` | No | MEDIUM |

#### Via compiler primitives (small additions to calls.rs — both targets)

| Module | Functions | Priority |
|--------|-----------|----------|
| `float` | `to_fixed(n, decimals)` | CRITICAL |
| `fs` | `walk`, `remove_all`, `glob`, `file_size`, `temp_dir` | HIGH |
| `process` | `exec_in(dir, cmd, args)`, `exec_with_stdin` | HIGH |
| `time` | `format(ts, fmt)`, `parse(s, fmt)` | HIGH |
| `http` | fix missing type signatures in stdlib.rs (bug) | HIGH |
| `http` | `get_with_headers`, `request(method, url, body, headers)` | MEDIUM |

---

## Stdlib Architecture: 3-Layer Design

Almide の stdlib を 3 層に分離する。WASM を一級市民として扱い、pure な計算と OS 依存を明確に分ける。

参考にした言語:
- **MoonBit**: core (pure) / x (platform) の 2 層。WASM-first。JSON は core に含む
- **Gleam**: stdlib (target-independent) / gleam_erlang / gleam_javascript の分離
- **Rust**: core / alloc / std の 3 層。WASM で使えない関数はコンパイルエラー
- **Zig**: comptime でターゲット判定。未使用コード自動削除

### Layer 1: core（全ターゲット、WASM OK）

auto-import または `import xxx` で使える。pure な計算のみ。OS 依存なし。

| Module | Status | Notes |
|--------|--------|-------|
| `string` | hardcoded (calls.rs) | 文字列操作 |
| `list` | hardcoded (calls.rs) | リスト操作、HOF |
| `int` | hardcoded (calls.rs) | 数値変換、ビット演算 |
| `float` | hardcoded (calls.rs) | 数値変換 |
| `map` | hardcoded (calls.rs) | ハッシュマップ |
| `math` | hardcoded (calls.rs) | 数学関数 |
| `json` | hardcoded (calls.rs) | パース・シリアライズ。WASM interop の共通言語 |
| `regex` | hardcoded (calls.rs) | 正規表現 |
| `path` | bundled .almd | パス操作（pure 文字列処理） |
| `time` | bundled .almd | 日付分解（year/month/day 等。now/sleep は platform） |
| `args` | bundled .almd | 引数パース（env.args() は platform 経由で注入） |

### Layer 2: platform（native only）

`import platform.fs` 等で明示的に import する。WASM ターゲットで import すると**コンパイルエラー**。

| Module | Status | Notes |
|--------|--------|-------|
| `fs` | hardcoded (calls.rs) | ファイル I/O |
| `process` | hardcoded (calls.rs) | 外部コマンド実行 |
| `io` | hardcoded (calls.rs) | stdin/stdout |
| `env` | hardcoded (calls.rs) | 環境変数、args、unix_timestamp、millis、sleep_ms |
| `http` | hardcoded (calls.rs) | HTTP サーバー/クライアント |
| `random` | hardcoded (calls.rs) | OS エントロピーベースの乱数 |

### Layer 3: x（公式拡張パッケージ）

`almide.toml` に依存追加して使う。公式メンテナンスだが stdlib とは独立してバージョン管理。

| Package | Status | Notes |
|---------|--------|-------|
| `encoding` | implemented (.almd) → 分離予定 | hex, base64 |
| `hash` | 未実装 | SHA-256, SHA-1, MD5 |
| `crypto` | 未実装 | 暗号化 |
| `csv` | 未実装 | CSV パース |
| `term` | 未実装 | ANSI カラー、ターミナル制御 |

### Implementation Steps

#### Phase A: WASM コンパイルエラー
- [ ] checker: WASM ターゲット時に platform モジュールの import を検出してエラー
- [ ] `--target wasm` 時に checker にターゲット情報を渡す仕組み

#### Phase B: platform namespace 導入
- [ ] `import platform.fs` 構文の設計
- [ ] 既存の `import fs` からの移行パス（deprecation warning → エラー）
- [ ] platform モジュールの resolver 実装

#### Phase C: x パッケージ分離
- [ ] encoding を `almide/encoding` リポジトリに分離
- [ ] パッケージマネージャ経由で利用可能に
- [ ] hash, csv, term を x パッケージとして新規作成

### Multi-Target Design (要検討)

Gleam の `@external` パターン（同一関数にターゲット別実装を提供）を Almide でどう実現するか。

```gleam
// Gleam の例
@external(erlang, "gleam_stdlib", "string_length")
@external(javascript, "../gleam_stdlib.mjs", "string_length")
pub fn length(string: String) -> Int
```

Almide での候補:
```almide
// 案1: extern ブロック（既に ROADMAP にある）
extern "rust" { fn fast_hash(data: String) -> String }
extern "ts"   { fn fast_hash(data: String) -> String }

// 案2: target 属性
@target(rust) fn fast_hash(data: String) -> String = ...
@target(ts)   fn fast_hash(data: String) -> String = ...
```

この設計は別途議論が必要。HTTP の「型だけ共通、実装はターゲット別」パターン（Gleam 方式）も検討対象。

---

## Other

- [ ] Package registry (to be considered in the future)
