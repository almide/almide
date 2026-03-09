# Module System v2 Specification

> Verified by `exercises/mod-test/mod_system_test.almd` (25 tests) + error tests.

---

## 1. Package Structure

A package is a directory with `.almd` source files under `src/`.

```
mylib/
  src/
    mod.almd          ← package top-level (optional)
    parser.almd       ← sub-module: mylib.parser
    formatter.almd    ← sub-module: mylib.formatter
    http/
      mod.almd        ← sub-namespace: mylib.http
      client.almd     ← sub-module: mylib.http.client
```

### Rules

- `mod.almd` defines the package's top-level namespace. If absent, the package has no top-level — only direct sub-module imports work.
- Every sibling `.almd` file (excluding `mod.almd`, `lib.almd`, `main.almd`) becomes a sub-module named `pkg.filename`.
- Subdirectories with a `mod.almd` create deeper sub-namespaces, scanned recursively to arbitrary depth.
- Package identity is declared in `almide.toml`, not in source files. There is no `module` declaration.

---

## 2. Import Syntax

```
import pkg                      -- load package + all sub-namespaces
import pkg.sub                  -- load specific sub-module only
import pkg as alias             -- alias the entire package
import pkg.sub as alias         -- alias a specific sub-module
```

### Prohibited

- `import pkg.*` (wildcard) — compile error.
- Circular imports — detected at resolve time, compile error.

---

## 3. Name Resolution

### 3.1 Top-level Access

```
import mylib
mylib.hello()          -- calls fn hello() in mylib/src/mod.almd
```

### 3.2 Sub-namespace Access

```
import mylib
mylib.parser.parse(x)     -- calls fn parse() in mylib/src/parser.almd
mylib.formatter.format(x) -- calls fn format() in mylib/src/formatter.almd
```

Importing a package with `import pkg` automatically loads all sub-namespaces. No separate `import pkg.sub` is needed.

### 3.3 Deep Nesting (Arbitrary Depth)

```
import deeplib
deeplib.hello()                -- 1 level
deeplib.http.info()            -- 2 levels
deeplib.http.client.get(url)   -- 3 levels
```

Resolution uses a `flatten_member_chain` algorithm: the AST member chain `a.b.c.func()` is flattened into segments `["a", "b", "c"]` + function `"func"`. The compiler tries progressively longer dotted paths (`a.b.c`, `a.b`, `a`) to find the matching module.

### 3.4 Direct Sub-module Import

```
import mylib.parser
parser.parse(x)        -- accessible by last segment name
```

When importing `pkg.sub` without an alias, the sub-module is accessible by its last path segment.

### 3.5 Name Conflicts

Different namespaces never conflict. These coexist without ambiguity:

```
mylib.add(1, 2)            -- from mod.almd
mylib.parser.parse("x")   -- from parser.almd
```

---

## 4. Aliases

### 4.1 Package Alias

```
import mylib as m
m.hello()              -- top-level via alias
m.parser.parse(x)      -- sub-module via alias
m.add(1, 2)            -- functions with args via alias
```

Alias resolution applies to the first segment only. `m.parser.parse()` resolves `m → mylib`, then looks up `mylib.parser`.

### 4.2 Sub-module Alias

```
import mylib.formatter as fmt
fmt.format_upper(x)
```

### 4.3 Multiple Aliases

Multiple aliases coexist in the same file without conflict:

```
import mylib as m
import mylib.formatter as fmt
m.hello()              -- works
fmt.format_upper(x)    -- works independently
```

### 4.4 Duplicate Import Deduplication

Importing the same module via different statements loads it only once:

```
import mylib
import mylib as m
mylib.add(5, 5)   -- works
m.add(5, 5)       -- same module, also works
```

---

## 5. Diamond Dependency

When multiple packages depend on the same leaf package, it is loaded exactly once.

```
main → dmod_b → dmod_d
main → dmod_c → dmod_d
```

`dmod_d` appears once in the compiled output. Both `dmod_b` and `dmod_c` reference the same module. Deduplication uses a `loaded_names: HashSet<String>` in the resolver.

```
import dmod_b
import dmod_c
import dmod_d

dmod_b.from_b()    -- "B says: from D"  (B calls D internally)
dmod_c.from_c()    -- "C says: from D"  (C calls D internally)
dmod_d.shared()    -- "from D"           (direct access also works)
```

---

## 6. Sub-module Imports

Sub-modules can import other packages (both stdlib and user packages). Their imports are resolved recursively during the parent package's loading.

```
// mylib/src/formatter.almd
fn format_upper(s: String) -> String = string.to_upper(s)   -- uses stdlib

// mylib/src/utils.almd
import extlib
fn describe(s: String) -> String = extlib.pub_fn() ++ ": " ++ s   -- uses user package
```

---

## 7. Visibility

Three visibility levels control access across module boundaries:

| Modifier | Scope | Example |
|---|---|---|
| `fn` | Public — accessible from anywhere | `fn pub_fn() -> String` |
| `mod fn` | Same project only — not from external consumers | `mod fn internal() -> String` |
| `local fn` | Same file only — not from any other module | `local fn helper() -> String` |

### Enforcement

External access to `mod fn` or `local fn` produces a compile error:

```
error: function 'mod_fn' is not accessible from module 'extlib'
  hint: 'mod_fn' has restricted visibility and cannot be accessed from here
```

### Self-import Distinction

The compiler tracks whether a module is a self-import (same project, via `import self.xxx`) or external. `is_self_import` is propagated through the module resolution pipeline as a boolean flag. `mod fn` is accessible when `is_self_import = true`.

---

## 8. Effect Functions Across Modules

Effect functions (`effect fn`) from external packages are callable in effect context:

```
// effectlib/src/mod.almd
effect fn read_config() -> Result[String, String] = ok("config_value")
fn pure_fn() -> String = "pure"

// caller.almd
import effectlib
effect fn main(_args: List[String]) -> Result[Unit, String] = {
  let config = effectlib.read_config()   -- auto-unwrapped in effect context
  effectlib.pure_fn()                     -- pure fn also callable
}
```

In effect context, `Result[T, E]` return values from module calls are auto-unwrapped (the `?` operator is inserted by the compiler).

---

## 9. Package Without mod.almd

A package directory may omit `mod.almd`. In that case, there is no top-level namespace — only direct sub-module imports work:

```
nomod_lib/
  src/
    parser.almd       ← only sub-module, no mod.almd

import nomod_lib.parser as p
p.parse("hello")                -- works
-- nomod_lib.parse("hello")     -- would NOT work (no top-level)
```

---

## 10. Compiler Pipeline

### Resolve Phase (`src/resolve.rs`)

1. Parse import declarations from the source file
2. For each `import pkg`: find `pkg/src/mod.almd`, parse it, recursively resolve its imports (depth-first), then scan sub-namespaces
3. For each `import pkg.sub`: find the specific sub-module file, register with dotted name
4. Deduplication via `loaded_names: HashSet<String>` — prevents double-loading in diamond scenarios
5. Circular dependency detection via `loading: HashSet<String>`
6. Output: `Vec<(name, Program, Option<PkgId>, is_self_import)>`

### Check Phase (`src/check/`)

1. Register each module's exported functions and types with dotted prefix
2. Register import aliases (explicit `as` and implicit last-segment for multi-segment imports)
3. On call: `flatten_member_chain` → alias resolution on first segment → progressive dotted path matching → type check

### Emit Phase (`src/emit_rust/`)

1. Register import aliases
2. Each module emitted as `mod pkg_sub { ... }` (dots replaced with underscores)
3. On call: same `flatten_member_chain` + alias resolution → `pkg_sub::func()` in generated Rust

---

## 11. File Resolution Order

When resolving `import pkg`, the compiler searches in order:

1. `{base_dir}/pkg.almd`
2. `{base_dir}/pkg/mod.almd`
3. `{base_dir}/pkg/src/mod.almd`
4. `{base_dir}/pkg/src/lib.almd` (legacy)
5. Dependencies listed in `almide.toml`

When resolving `import pkg.sub`, the compiler searches:

1. `{pkg_src_dir}/sub.almd`
2. `{pkg_src_dir}/sub/mod.almd`

---

## Test Reference

All behaviors above are verified by executable tests in `exercises/mod-test/`:

| File | Tests | Covers |
|---|---|---|
| `mod_system_test.almd` | 25 | Sections 1–6, 8–9 |
| `vis_effect_test.almd` | 2 assertions | Section 8 |
| `vis_mod_error_test.almd` | error check | Section 7 (`mod fn` rejected) |
| `vis_local_error_test.almd` | error check | Section 7 (`local fn` rejected) |
| `run_tests.sh` | runner | Executes all above |

### Test Packages

| Package | Structure | Purpose |
|---|---|---|
| `mylib` | mod.almd + parser + formatter + utils | Basic, sub-ns, sub-import |
| `deeplib` | mod.almd + http/mod.almd + http/client.almd | 3-level nesting |
| `extlib` | fn + mod fn + local fn | Visibility |
| `dmod_b`, `dmod_c`, `dmod_d` | Diamond: B→D, C→D | Diamond dependency |
| `effectlib` | effect fn + pure fn | Cross-module effects |
| `nomod_lib` | parser.almd only (no mod.almd) | No top-level package |
