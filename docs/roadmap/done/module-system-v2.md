<!-- description: File-based module system with visibility controls and mod.almd -->
# Module System v2

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
- [x] Checker: validate cross-package access respects `mod.almd` boundary (handled by resolver — sub-namespaces loaded as `pkg.file`)
- [x] CLI: `almide init` template — remove `module main` from generated code
- [x] CLI: `almide fmt` without args — format all `src/**/*.almd` recursively
- [x] CLI: `almide --help` and `almide --version`
- [x] CLI: `--dry-run` → `--check` rename for `almide fmt` (keep `--dry-run` as alias)
- [x] CLI: `almide build --release` (opt-level=2)
- [x] Deprecation warning for `module` declarations
- [x] Deprecation warning for `lib.almd` as package entry (suggest rename to `mod.almd`)
- [x] `import self` — load own package entry point (`mod.almd`) from `main.almd`
- [x] `import self as alias` — with alias support
- [x] Self-import alias codegen — correct Rust module path for aliased self-imports
- [x] Checker: recognize module aliases in Ident expressions (no false "undefined variable" error)

---
