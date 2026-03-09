# Almide Roadmap

## Module System ✅ Implemented

### Project Structure

```
myapp/
  almide.toml
  src/
    main.almd              # entry point
    utils.almd             # import self.utils
    http/
      client.almd          # import self.http.client
      server.almd          # import self.http.server
  tests/
    utils_test.almd
```

### almide.toml

```toml
[package]
name = "myapp"
version = "0.1.0"

[dependencies]
json = { git = "https://github.com/almide/json", tag = "v1.0.0" }
```

Package identity is managed in `almide.toml`. No `module` declaration is needed in source files.

### import syntax ✅

```almide
import self.utils              // utils.add(1, 2)
import self.http.client        // client.get(url)
import self.http.client as c   // c.get(url)
import json                    // external dependency (almide.toml)
```

- `self.xxx` = local module → resolved under `src/`
- anything else = stdlib or external dependency
- `as` alias supported
- User modules take priority over stdlib when names conflict

### Module File Resolution ✅

| import statement | resolved location |
|---|---|
| `import self.utils` | `src/utils.almd` |
| `import self.http.client` | `src/http/client.almd` |
| `import json` | dependency in almide.toml → `~/.almide/cache/` |

### 3-Level Visibility ✅

| Syntax | Scope | Rust output |
|---|---|---|
| `fn f()` | public (default) | `pub fn` |
| `mod fn f()` | same project only | `pub(crate) fn` |
| `local fn f()` | this file only | `fn` (private) |

- Same modifiers apply to `type` declarations
- `pub` keyword is accepted for backward compatibility (no-op since default is already public)

### Test Repository

- https://github.com/almide/mod-sample — for verifying visibility + self import behavior

---

## Remaining Improvements

### Checker-level visibility errors ✅

- [x] Error at checker stage when `local fn` is called from an external module
- [x] Error at checker stage when `mod fn` is called from an external package (determined by `is_external` flag)
- [x] Error message: "function 'xxx' is not accessible from module 'yyy'"

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

### int module

- [ ] `int.parse(s)` → `Result[Int, String]` (parse decimal string)
- [ ] `int.parse_hex(s)` → `Result[Int, String]`
- [ ] `int.abs(n)` → `Int`
- [ ] `int.min(a, b)` / `int.max(a, b)` (aliases for math.min/max)

### string module

- [ ] `string.pad_right(s, n, ch)` → `String`
- [ ] `string.trim_start(s)` / `string.trim_end(s)` → `String`
- [ ] `string.count(s, sub)` → `Int`

### list module

- [ ] `list.index_of(xs, x)` → `Option[Int]`
- [ ] `list.last(xs)` → `Option[T]`
- [ ] `list.chunk(xs, n)` → `List[List[T]]`
- [ ] `list.sum(xs)` / `list.product(xs)` → `Int`

### CLI improvements

- [ ] `almide --help`: detailed help with all options and examples
- [ ] `almide check`: show progress for multi-file projects
- [ ] Exit codes: distinguish parse error (65), type error (66), codegen error (70)

---

## Other

- [ ] Package registry (to be considered in the future)
