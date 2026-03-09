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

## String Handling

### Heredoc

Multi-line strings without escape noise. Useful for SQL, HTML, JSON templates, etc.

```almide
let query = """
  SELECT *
  FROM users
  WHERE id = ${user_id}
"""

let html = """
  <html>
    <body>${content}</body>
  </html>
"""
```

- `"""..."""` syntax (consistent with Python/Kotlin/Swift)
- Leading whitespace stripped based on indentation of closing `"""`
- Interpolation `${expr}` works the same as in regular strings

### Implementation Steps

- [ ] Lexer: recognize `"""` as heredoc open token; suppress newline-as-separator handling until closing `"""`
- [ ] Lexer: capture raw content with indentation, strip common leading whitespace on close (dedent based on closing `"""` column)
- [ ] Parser: reuse existing interpolation logic for `${expr}` inside heredocs
- [ ] Rust emitter: emit as `format!(...)` (same as regular interpolated strings)
- [ ] TS emitter: emit as template literals `` `...` ``

### Key Lexer Concern

Almide's lexer is newline-sensitive (newlines act as statement separators). Inside `"""..."""`, newlines must be treated as literal content, not as token delimiters. The lexer needs a `in_heredoc` flag that suppresses normal newline handling until the closing `"""` is encountered.

### Design Notes

- Indentation stripping follows the closing `"""` column (same as Kotlin trimIndent)
- No need for separate raw heredoc — use `r"""..."""` if escape-free is needed
- Single-line `"..."` with `${expr}` already works; heredoc is purely for multi-line ergonomics

---

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

## Other

- [ ] Package registry (to be considered in the future)
