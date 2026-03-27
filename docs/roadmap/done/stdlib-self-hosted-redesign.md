<!-- description: Self-hosted stdlib with .almd-first design and @extern for host deps -->
<!-- done: 2026-03-17 -->
# Stdlib Runtime Architecture Reform

## Vision

stdlib is defined with `.almd` files at the center. Pure logic is implemented in Almide itself. Only host-dependent features have target implementations via `@extern`. Native implementations are real Rust/TS code, directly testable with their respective testing tools. `@extern` is not a stdlib-only feature but a general mechanism that can be opened to users.

---

## Decision Rules

Design decision principles. When in doubt, return here.

### Rule 1: Prefer `.almd` if it can be written as pure logic

What can be written in Almide should be written in Almide. Self-hosting demonstrates language maturity.

### Rule 2: Use `@extern` when dependent on host capabilities

Only use @extern for functions that need OS/runtime access. @extern is not a dumping ground but a place for only what must be external.

### Rule 3: Only use provisional `@extern` for performance-immature cases

Closure functions like `list.map` remain provisionally in @extern until compiler optimizations catch up. Migrate to .almd once optimization matures.

### Rule 4: The same failure should have the same type representation across all targets

Normalize with Result/Option, not throw/panic. Even on the TS target, `Result[A, E]` is represented as a value (not converted to throw).

### Rule 5: Use variants only for environment differences, not semantic differences

Do not push sync/async or API surface differences into variants. async is handled as a separate module or the language's effect/async model.

### Rule 6: Functions tied to the language's basic types do not require import

Prelude (Layer 1) modules are implicitly auto-imported. Criterion: **does the type have a literal in language syntax?** `"hello"` -> string, `[1,2,3]` -> list, `42` -> int, `3.14` -> float, `true` -> bool, `{"k": v}` -> map, `ok(x)` -> result, `some(x)` -> option. If a literal exists, the type's operation functions should be usable without import.

```almide
// No import needed (Prelude)
let xs = [1, 2, 3].map((x) => x * 2)
let name = "hello".to_upper()
let n = int.from_string("42")

// Import required (Pure Library / Platform)
import fs
import csv
let data = csv.parse(fs.read_text("data.csv"))
```

### Rule 7: The compiler does not know the deployment target

The compiler knows only 3 axes: **target (rust / ts / js), runtime (native / node / deno / browser / wasm), version (optional)**. Deployment targets like Cloudflare, Docker, AWS Lambda, Deno Deploy are outside the compiler's scope. They are the responsibility of external tools (wrangler, docker, sam, etc.).

Same boundary as Go knowing only GOOS/GOARCH, where wrapping in Docker is Dockerfile's job and deploying to Cloud Run is gcloud's job -- a design that has held for over 10 years.

```
Compiler's job:      almide build app.almd --target ts --runtime node
External tools' job: docker build / wrangler deploy / sam deploy / etc.
```

By not breaking this boundary, compiler complexity does not grow and the design holds long-term.

### Rule 8: glue runtime is an explicit, thin translation layer

The glue layer that converts values between targets is placed as ordinary visible files in the repository, not hidden magic. Glue is **aggregated per target**, not per module. Because the representation of Result, passing of List, and String encoding rules are the same across fs, list, http, etc.

```
runtime/
  ts/
    core.ts           Result, Option, basic type representations
    core.node.ts      Node-specific differences (if any)
    core.browser.ts   Browser-specific differences (if any)
    core_test.ts      Tests for the glue itself
  rust/
    core.rs           Result, Option, basic type representations
    core.wasm.rs      WASM-specific differences (if any)
    core_test.rs      Tests for the glue itself
```

**Responsibilities absorbed by glue (only these):**
1. **Value representation conversion** -- conversion rules for nested types
2. **Result/Option normalization** -- convert host API throw/null to Almide value representations
3. **String boundary handling** -- conversion rules for UTF-8 (Almide) <-> UTF-16 (TS)
4. **Panic/exception capture** -- catch host API throw and convert to Result. Never leak throw outside

**What does NOT go into glue:**
- stdlib logic itself
- Module-specific dispatch
- Complex conditional branching or platform detection
- Optimization

**Design metric:** core.ts should be ~50 lines, core.rs ~20 lines. If it grows beyond that, it's a sign the design is wrong.

Go's `wasm_exec.js` suffered because glue was made into an untestable hidden file. Almide makes glue itself unit-testable, so that when users write `@extern` packages in the future, they use the same glue via `import { ok, err, catchToResult } from "almide/runtime"`

---

## Non-Goals (v1)

- Distribution mechanism for user-defined @extern packages will not be built in v1
- General solution for async model will not be done in v1
- Full pure Almide conversion of all stdlib functions will not be completed in v1
- ABI stability between compiler versions will not be guaranteed in v1
- API vocabulary reform (verb system) is outside the scope of this document. Handled in [Stdlib API Surface Reform](stdlib-verb-system.md)
- Deployment-target-specific configuration generation (Dockerfile, wrangler.toml, etc.) will not be done by the compiler

---

## Motivation

### Current Problems

1. **TOML dual management**: Writing both Rust template + TS template per function
2. **build.rs complexity**: TOML -> Rust code generator exceeds 1000 lines, hard to debug
3. **src/generated/ opacity**: 3 generated files, manual editing forbidden but also hard to understand
4. **Users cannot use the same mechanism**: stdlib gets special treatment, users cannot extend via UFCS
5. **Native implementations are untestable**: `core_runtime.txt` is a string template, so rust-analyzer and cargo test don't work

**Essence**: stdlib has become a special mechanism built into the compiler. Untestable, unextensible, hard to debug. This returns it to normal language assets outside the compiler.

### Prior Art

| Framework | Type Definitions | Target Implementations | Tests |
|---|---|---|---|
| Flutter PlatformView | Dart (MethodChannel) | iOS: Swift, Android: Kotlin | Each platform's testing tools |
| React Native Turbo Modules | TypeScript spec → Codegen | iOS: Obj-C/Swift, Android: Kotlin | XCTest / JUnit |
| Android Resources | XML (values/) | values-v21/, values-v28/ | Per-version fallback |
| **Almide (proposed)** | **.almd** | **Rust: .rs, TS: .ts + variants** | **cargo test / deno test** |

---

## Design

### Architecture

```
runtime/                    <- Target-common translation layer (glue)
  ts/
    core.ts                 Result, Option, basic type TS representations
    core.node.ts            Node-specific differences (if any)
    core.browser.ts         Browser-specific differences (if any)
    core_test.ts            Tests for the glue itself
  rust/
    core.rs                 Result, Option, basic type Rust representations
    core.wasm.rs            WASM-specific differences (if any)
    core_test.rs            Tests for the glue itself

stdlib/                     <- Per-module definitions + native implementations

  # Layer 1: Prelude (auto-import, some with extern)
  prelude/
    string/
      mod.almd              Type signatures + pure Almide implementations
      extern.rs             Rust implementations of len, slice, split, etc.
      extern.ts             TS implementations of len, slice, split, etc.
      extern_test.rs
      extern_test.ts
    list/
      mod.almd              Type signatures + pure Almide implementations
      extern.rs             Rust implementations of map, filter, sort_by, etc.
      extern.ts             TS implementations of map, filter, sort_by, etc.
      extern_test.rs
      extern_test.ts
    int/
      mod.almd              Mostly pure, to_string/from_string are extern
      extern.rs
      extern.ts
    float/
      mod.almd
      extern.rs
      extern.ts
    math/
      mod.almd
      extern.rs             sqrt, sin, cos, etc.
      extern.ts
    map/
      mod.almd
      extern.rs             new, get, set
      extern.ts
    result/
      mod.almd              All pure Almide (no extern needed)
    option/
      mod.almd              All pure Almide (no extern needed)

  # Layer 2: Pure Library (explicit import, no extern needed)
  hash/
    mod.almd                All pure Almide
  csv/
    mod.almd                All pure Almide
  url/
    mod.almd                All pure Almide
  path/
    mod.almd                All pure Almide
  encoding/
    mod.almd                All pure Almide
  args/
    mod.almd                All pure Almide
  toml/
    mod.almd                All pure Almide
  term/
    mod.almd                All pure Almide (ANSI escapes)

  # Layer 3: Platform (explicit import, extern required)
  fs/
    mod.almd                Type signatures (all functions are @extern)
    extern.rs               Rust: std::fs
    extern.wasm.rs          Rust: WASM (restricted or stub)
    extern.ts               TS: Deno
    extern.node.ts          TS: Node fs
    extern.node.22.ts       TS: Node 22+ (fs.glob support)
    extern.browser.ts       TS: File System Access API
    extern_test.rs
    extern_test.ts
    extern_test.node.ts
  http/
    mod.almd
    extern.rs               reqwest
    extern.ts               fetch (Deno)
    extern.node.ts          node:http
    extern.browser.ts       fetch (Web API)
  io/
    mod.almd
    extern.rs
    extern.ts
  env/
    mod.almd
    extern.rs
    extern.ts
  process/
    mod.almd
    extern.rs
    extern.ts
  random/
    mod.almd
    extern.rs
    extern.ts
  datetime/
    mod.almd
    extern.rs
    extern.ts
  json/
    mod.almd                Mostly pure (get_path, set_path, etc.)
    extern.rs               Only parse, stringify are extern
    extern.ts
  regex/
    mod.almd
    extern.rs               All functions are extern
    extern.ts
```

### `@extern` Syntax

An established term in Rust (`extern`), Gleam (`@external`), and C (`extern`). Makes explicit that "this function's implementation is outside Almide."

```almide
// stdlib/list/mod.almd

// @extern: native implementations per target exist in extern.rs / extern.ts
@extern
fn map[A, B](xs: List[A], f: Fn(A) -> B) -> List[B]

@extern
fn filter[A](xs: List[A], f: Fn(A) -> Bool) -> List[A]

@extern
fn len[A](xs: List[A]) -> Int

// Pure Almide: the compiler compiles it normally (no extern file needed)
fn contains[A](xs: List[A], value: A) -> Bool {
  for x in xs {
    if x == value { return true }
  }
  false
}
```

---

### @extern Contract

Strict contract for @extern functions.

#### Name Resolution Rules

- `@extern fn foo(...)` maps to `almide_rt_{module}_{func}` in the `extern.{target}` file
- The compiler does not know specific function names. Resolution is mechanical and rule-based

#### Type Mapping

```
Almide            Rust                       TS
─────────────────────────────────────────────────────
Int               i64                        number
Float             f64                        number
String            String / &str              string
Bool              bool                       boolean
List[A]           Vec<A>                     A[]
Map[K, V]         HashMap<K, V>              Map<K, V>
Option[A]         Option<A>                  A | null
Result[A, E]      Result<A, E>               { ok: true, value: A } | { ok: false, error: E }
Fn(A) -> B        impl Fn(A) -> B            (a: A) => B
Unit              ()                         void
```

#### Error Representation (Rule 4)

**Result is represented as a value across all targets.** The glue layer (`runtime/ts/core.ts`) provides type definitions and helpers, and all externs import and use them.

```typescript
// runtime/ts/core.ts — glue layer (~50 lines)

export type AlmResult<T, E> =
  | { ok: true; value: T }
  | { ok: false; error: E };

export function ok<T, E>(value: T): AlmResult<T, E> {
  return { ok: true, value };
}

export function err<T, E>(error: E): AlmResult<T, E> {
  return { ok: false, error };
}

// Wrapper to convert host API throw to Result
export function catchToResult<T>(f: () => T): AlmResult<T, string> {
  try {
    return ok(f());
  } catch (e) {
    return err(e instanceof Error ? e.message : String(e));
  }
}

export type AlmOption<T> =
  | { some: true; value: T }
  | { some: false };

export function some<T>(value: T): AlmOption<T> {
  return { some: true, value };
}

export const none: AlmOption<never> = { some: false };

export function fromNullable<T>(value: T | null | undefined): AlmOption<T> {
  return value != null ? some(value) : none;
}
```

```rust
// runtime/rust/core.rs — Thin because Rust has native Result/Option (~20 lines)

pub type AlmResult<T> = Result<T, String>;
pub type AlmOption<T> = Option<T>;

/// Wrapper for when host API may panic
pub fn catch_to_result<T, F: FnOnce() -> T>(f: F) -> AlmResult<T> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(f))
        .map_err(|e| {
            if let Some(s) = e.downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = e.downcast_ref::<String>() {
                s.clone()
            } else {
                "unknown panic".to_string()
            }
        })
}
```

**Example of extern using glue:**

```typescript
// stdlib/fs/extern.ts — imports and uses glue
import { catchToResult, type AlmResult } from "../../runtime/ts/core.ts";

export function almide_rt_fs_read_text(path: string): AlmResult<string, string> {
  return catchToResult(() => Deno.readTextFileSync(path));
}
```

```typescript
// stdlib/fs/extern.node.ts — Node variant also uses the same glue
import { ok, err, type AlmResult } from "../../runtime/ts/core.ts";
import * as fs from "node:fs";

export function almide_rt_fs_read_text(path: string): AlmResult<string, string> {
  try {
    return ok(fs.readFileSync(path, "utf-8"));
  } catch (e) {
    return err(e instanceof Error ? e.message : String(e));
  }
}
```

```rust
// stdlib/fs/extern.rs — Rust side simply returns Result naturally
use crate::runtime::core::AlmResult;

pub fn almide_rt_fs_read_text(path: String) -> AlmResult<String> {
    std::fs::read_to_string(&path).map_err(|e| e.to_string())
}
```

#### Purity / Effects

- @extern functions can be either pure or effectful
- If declared as `effect fn` on the Almide side, it is an effectful function
- The extern implementation is responsible for conforming to the Almide type signature

#### Consistency Checks

The following are verified at compile time:

- Whether a function corresponding to the @extern declaration in `mod.almd` exists in `extern.{target}`
- Whether function names follow the `almide_rt_{module}_{func}` convention
- Full type checking is not performed (trust-based), but existence checks cause compile errors

#### Missing implementation

- No extern file for an @extern function -> compile error
- No corresponding function in the extern file -> compile error (existence check)
- Variant not found -> resolve via fallback chain, error if all fail

---

### Variant Resolution System

Same concept as Go's `_linux.go` / `_darwin_arm64.go`. Include environment info in file names, automatically selected at build time. No special config files needed.

```
Go                          Almide
────────────────────────────────────────────
GOOS (linux/darwin/...)     --target (rust/ts)
GOARCH (amd64/arm64/...)    --runtime (native/node/deno/wasm/browser)
_linux.go                   extern.rs
_linux_arm64.go             extern.wasm.rs
_js_wasm.go                 extern.browser.ts
//go:build tag              (future build constraint)
```

Just as Go's design has held for over 10 years with only the 2 axes of GOOS/GOARCH, Almide also avoids needlessly adding axes.

#### v1 Variant Axes (3 axes only)

| Axis | Go Equivalent | Almide | Values |
|---|---|---|---|
| **target** | GOOS | `--target` | rust, ts |
| **runtime** | GOARCH | `--runtime` | native, node, deno, browser, wasm |
| **version** | -- | `--runtime-version` | Numeric (optional) |

**Not included in v1:**
- `async` -- this is an API difference, not an environment difference (Rule 5). Handled as a separate module (`fs_async`) or the language's async model
- Deployment-target-specific knowledge -- Cloudflare, Docker, Lambda, etc. are outside the compiler's scope (Rule 6)

#### File Naming Convention

```
extern.{target}                        Baseline
extern.{runtime}.{target}              Runtime variant
extern.{runtime}.{version}.{target}    Versioned variant
```

Examples:
```
extern.rs                  Rust default (native)
extern.wasm.rs             Rust + WASM

extern.ts                  TS default (Deno)
extern.node.ts             Node all versions
extern.node.18.ts          Node 18+ (native fetch)
extern.node.22.ts          Node 22+ (fs.glob)
extern.browser.ts          Browser (Web API)
```

#### Resolution Order

Compiler flags: `--target {lang} [--runtime {runtime}] [--runtime-version {ver}]`

```
--target ts --runtime node --runtime-version 22
  1. extern.node.22.ts    <- highest priority if available
  2. extern.node.18.ts    <- downgrade (eligible since 22 > 18)
  3. extern.node.ts       <- runtime baseline
  4. extern.ts            <- generic fallback

--target rust --runtime wasm
  1. extern.wasm.rs       <- use this if available
  2. extern.rs            <- fallback

--target ts                (runtime unspecified -> default)
  1. extern.ts            <- this only
```

For versions, the "largest version less than or equal to the specified version" is selected.

Most modules require only **2 files**: `extern.rs` + `extern.ts`. Additional files are added only for modules that need variants.

---

### Native Implementations: Real Code

```rust
// stdlib/list/extern.rs
// Real Rust code, directly testable with cargo test

#[inline]
pub fn almide_rt_list_map<A: Clone, B>(
    xs: Vec<A>,
    f: impl Fn(A) -> B,
) -> Vec<B> {
    xs.into_iter().map(f).collect()
}

#[inline]
pub fn almide_rt_list_filter<A: Clone>(
    xs: Vec<A>,
    f: impl Fn(&A) -> bool,
) -> Vec<A> {
    xs.into_iter().filter(|x| f(x)).collect()
}

#[inline(always)]
pub fn almide_rt_list_len<A>(xs: &[A]) -> i64 {
    xs.len() as i64
}
```

```rust
// stdlib/list/extern_test.rs

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_map() {
        assert_eq!(almide_rt_list_map(vec![1, 2, 3], |x| x * 2), vec![2, 4, 6]);
    }

    #[test]
    fn test_filter() {
        assert_eq!(almide_rt_list_filter(vec![1, 2, 3, 4], |x| x % 2 == 0), vec![2, 4]);
    }

    #[test]
    fn test_len() {
        assert_eq!(almide_rt_list_len(&vec![1, 2, 3]), 3);
        assert_eq!(almide_rt_list_len::<i64>(&vec![]), 0);
    }
}
```

```typescript
// stdlib/list/extern.ts
// Real TS code, directly testable with deno test

export function almide_rt_list_map<A, B>(xs: A[], f: (a: A) => B): B[] {
  return xs.map(f);
}

export function almide_rt_list_filter<A>(xs: A[], f: (a: A) => boolean): A[] {
  return xs.filter(f);
}

export function almide_rt_list_len<A>(xs: A[]): number {
  return xs.length;
}
```

```typescript
// stdlib/list/extern_test.ts

import { assertEquals } from "jsr:@std/assert";
import { almide_rt_list_map, almide_rt_list_filter, almide_rt_list_len } from "./extern.ts";

Deno.test("map", () => {
  assertEquals(almide_rt_list_map([1, 2, 3], (x) => x * 2), [2, 4, 6]);
});

Deno.test("filter", () => {
  assertEquals(almide_rt_list_filter([1, 2, 3, 4], (x) => x % 2 === 0), [2, 4]);
});

Deno.test("len", () => {
  assertEquals(almide_rt_list_len([1, 2, 3]), 3);
  assertEquals(almide_rt_list_len([]), 0);
});
```

### Automatic Skeleton Generation

Like React Native Codegen, automatically generate `extern.rs` / `extern.ts` skeletons from `.almd` type signatures:

```bash
almide scaffold stdlib/list/mod.almd
```

---

### TS Target Output Model

Adopts the Gleam approach (reference via import). Extern files are placed as separate files from generated code.

```bash
almide build app.almd --target ts -o dist/
```

Output:
```
dist/
  app.ts              Generated user code
  _extern/
    list.ts           Copy of stdlib/list/extern.ts
    fs.ts             Copy of stdlib/fs/extern.ts (variant resolved)
```

```typescript
// dist/app.ts (generated code)
import { almide_rt_list_map } from "./_extern/list.ts";
import { almide_rt_fs_read_text } from "./_extern/fs.ts";

function main() {
  const content = almide_rt_fs_read_text("hello.txt");
  const nums = almide_rt_list_map([1, 2, 3], (x) => x * 2);
}
```

For `almide run`, inline expansion is used for convenience (executable as a single temp file).

---

### Testing Framework

@extern is verified with 3 layers of tests:

| Layer | What is Verified | Tool |
|---|---|---|
| **Native unit test** | Whether extern functions work correctly in isolation | `cargo test` / `deno test` |
| **Shared conformance test** | Whether behavior matches between Rust/TS | `almide test spec/stdlib/` |
| **Integration test** | Whether @extern functions can be called correctly from Almide code | `almide test` |

Separating native tests and conformance tests enables early detection of "works in Rust but not in TS" issues.

---

### Module 3-Layer Classification

stdlib is classified into 3 layers along 2 axes: **import requirement** and **platform dependency**.

#### Layer 1: Prelude (implicit auto-import, platform-independent)

**Always available without import.** Functions tied to the language's basic types. Same as Go's `len()`, `append()` or Elm's `List`, `String`, `Maybe` requiring no import.

| Module | Content | extern | Reason |
|---|---|---|---|
| **string** | trim, split, replace, contains, len, slice... | ~10 functions extern (len, slice, split, etc.) | String is part of the language |
| **list** | map, filter, contains, reverse, sort, len... | ~20 functions extern (map, filter, etc. Rule 3) | List is part of the language |
| **int** | abs, clamp, to_string, from_string, to_hex... | ~2 functions extern (to_string, from_string) | Int is part of the language |
| **float** | round, ceil, floor, to_string... | ~2 functions extern (to_string, from_string) | Float is part of the language |
| **bool** | -- | -- | Exists as a literal |
| **math** | pow, sqrt, sin, cos, log, pi... | ~10 functions extern (sqrt, sin, cos, etc.) | Numeric operations are fundamental |
| **map** | get, set, contains, keys, values, merge... | ~5 functions extern (new, get, set) | Map is part of the language |
| **result** | map, flat_map, unwrap_or, is_ok, is_err... | 0 (all pure) | Error type is part of the language |
| **option** | map, flat_map, unwrap_or, is_some, is_none... | 0 (all pure) | Option is part of the language |

```almide
// No import needed -- always available as Prelude
fn main() =
  let xs = [1, 2, 3]
  let doubled = xs.map((x) => x * 2)       // list.map
  let name = "hello".to_upper()             // string.to_upper
  let n = int.from_string("42")             // int.from_string
  println(doubled.len().to_string())        // list.len, int.to_string
```

**Compiler implementation**: Prelude module functions are implicitly brought into scope during resolution. UFCS infers the module from the type (`"hello".len()` -> String type -> string module). Explicit calls with module names (`list.map(xs, f)`) also work without import.

#### Layer 2: Pure Library (explicit import, platform-independent)

**Use with `import`.** Implemented in pure Almide. No extern needed. The same code runs on all targets.

| Module | Content | extern | Reason |
|---|---|---|---|
| **hash** | sha256, sha1, md5 | 0 | All pure Almide (189 lines) |
| **csv** | parse, parse_with_header, stringify | 0 | All pure Almide (70 lines) |
| **url** | parse, build | 0 | All pure Almide (220 lines) |
| **path** | join, normalize, dirname, extension | 0 | All pure Almide |
| **encoding** | base64_encode/decode, hex_encode/decode | 0 | All pure Almide |
| **args** | positional, flag, option | 0 | All pure Almide |
| **toml** | parse, stringify | 0 | All pure Almide |
| **term** | red, green, bold (ANSI escapes) | 0 | String operations only |

```almide
import csv
import hash

fn main() =
  let data = csv.parse_with_header("name,age\nalice,30")
  let checksum = hash.sha256("hello")
```

**Cost of adding a new target: Zero.** The compiler simply converts .almd -> target language.

#### Layer 3: Platform (explicit import, platform-dependent @extern)

**Use with `import`.** extern.rs / extern.ts required. Variant resolution applies.

| Module | Content | extern | Variants |
|---|---|---|---|
| **fs** | read_text, write, glob, walk, mkdir_p... | All functions | wasm, node, browser |
| **http** | get, post, serve, request... | All functions | node, browser |
| **io** | print, read_line | All functions | -- |
| **env** | get, set, args, cwd, os | All functions | -- |
| **process** | exec, exec_status, exit | All functions | -- |
| **random** | int, float, bytes, choice | All functions | -- |
| **datetime** | now, parse_iso, format | All functions | -- |
| **json** | parse, stringify | 2 functions extern | -- |
| **regex** | is_match, find, find_all, replace | All functions | -- |

```almide
import fs
import http

effect fn main() = do {
  let content = fs.read_text("data.csv")
  let resp = http.post_json("/api/upload", content)
}
```

**Cost of adding a new target: Write extern files.** However, since the glue layer (`runtime/{lang}/core.*`) absorbs type conversions, each extern is thin.

---

### 3-Layer Decision Criteria

| Criterion | Layer 1 (Prelude) | Layer 2 (Pure Library) | Layer 3 (Platform) |
|---|---|---|---|
| Import required? | No | Yes | Yes |
| Extern required? | Partial (type primitives + performance) | No | All or most |
| Cost of adding new target | Only the extern portion | Zero | Extern files needed |
| Platform dependency | None | None | Yes |
| Reason for classification | **Tied to language basic types** | **General-purpose but specific use** | **OS/runtime dependent** |

**Criteria for inclusion in Layer 1**: Does the type have a literal in language syntax? (`"hello"`, `[1,2,3]`, `42`, `3.14`, `true`, `{"k": v}`, `ok(x)`, `some(x)`). If a literal exists, the type's operation functions should be usable without import.

### What Disappears After Migration

| What Disappears | Lines | Replacement |
|---|---|---|
| `stdlib/defs/*.toml` (14 files) | ~2000 lines | stdlib/*/mod.almd |
| `build.rs` stdlib generation portion | ~1000 lines | None |
| `src/generated/stdlib_sigs.rs` | ~800 lines | Parser reads .almd directly |
| `src/generated/emit_rust_calls.rs` | ~1200 lines | stdlib/*/extern.rs |
| `src/generated/emit_ts_calls.rs` | ~600 lines | stdlib/*/extern.ts |
| `src/emit_rust/core_runtime.txt` | ~800 lines | Distributed into stdlib/*/extern.rs |
| `src/emit_ts_runtime.rs` | ~400 lines | Distributed into stdlib/*/extern.ts |
| **Total** | **~6800 lines** | **Testable native code** |

### What We Gain

| Aspect | Current State | @extern Approach |
|---|---|---|
| Testing native implementations | Impossible (string templates) | Direct testing with `cargo test` / `deno test` |
| IDE support | None (.txt files) | rust-analyzer / TS LSP fully functional |
| Adding new functions | TOML + templates for 2 targets | Type definition in .almd -> `almide scaffold` -> implementation |
| User extensions | Impossible | Same mechanism available via `@extern` |
| Compiler responsibility | Knows dispatch for 343 functions | Only variant resolution + include generic logic |
| Environment support | Deno/Node mixed in one file | Separated via variant files |
| Behavior consistency across targets | No means of verification | Guaranteed by conformance tests |

---

## Implementation Steps

To increase success probability, iterate in small steps one cycle at a time.

### Step 1: Minimal Core of @extern

Target: `io.print` -- a single function only.

What to verify:
- Whether the parser recognizes `@extern fn`
- Whether the checker processes type signatures
- Whether the lowerer generates `IrFunction::Extern(module, func)`
- Whether codegen includes/imports `extern.rs` / `extern.ts`
- Whether native tests pass

No need for variants or scaffold yet.

### Step 2: Variant Resolution (Minimal Version)

Include only `extern.ts` + `extern.node.ts`. Versions come later.

What to verify:
- Whether `extern.node.ts` is selected with `--runtime node`
- Whether it falls back to `extern.ts` when unspecified

### Step 3: Platform Module Migration

Migrate in order: io -> env -> process -> fs -> http -> random -> datetime.

Start with modules where @extern provides the most value.

- Test each function with `extern_test.rs` / `extern_test.ts`
- Delete TOML definitions
- Separate Deno/Node variants for fs / http

### Step 4: Verify Pure Almide Modules

hash, csv, url, path, encoding, args -- these are already working as .almd, so just align the directory structure to `stdlib/hash/mod.almd`.

### Step 5: Hybrid Module Migration

string, int, float, math, map, list, result -- rewrite most in .almd, minimize @extern.

- Only type primitives (len, get, to_string, etc. ~10 functions) use @extern
- Closure functions (map, filter, etc. ~20 functions) remain provisionally @extern (Rule 3)
- Performance comparison: verify no significant difference

### Step 6: Versioned Variants

Add resolution logic for `extern.node.22.ts`.

### Step 7: Remove Generated Code

- Delete all `stdlib/defs/*.toml`
- Remove stdlib generation logic from `build.rs`
- Remove stdlib-related files from `src/generated/`
- Delete `src/emit_rust/core_runtime.txt`
- Delete `src/emit_ts_runtime.rs`

### Step 8: scaffold Command

Auto-generate @extern function skeletons with `almide scaffold <module.almd>`.

### Future: Open @extern to Users

- Enable users to use `@extern` in their own packages
- FFI use cases: directly wrapping Rust crates and npm packages

---

## CI Integration

```yaml
# Test native implementations independently from the Almide compiler
extern-test-rust:
  name: Extern Tests (Rust)
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v4
    - run: cargo test --manifest-path stdlib/Cargo.toml

extern-test-deno:
  name: Extern Tests (Deno)
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v4
    - uses: denoland/setup-deno@v2
    - run: deno test stdlib/*/extern_test.ts

extern-test-node:
  name: Extern Tests (Node)
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v4
    - uses: actions/setup-node@v4
      with: { node-version: "22" }
    - run: npx tsx --test stdlib/*/extern_test.node.ts
```

---

## @extern Full List (~60 functions)

### Tier 1: Type Primitives (10)
```
string: len, char_at, slice, from_chars
list:   len, get, push, set
int:    to_string, from_string
```

### Tier 2: Closure Optimization -- Provisional (20)
```
list:   map, filter, find, any, all, each, sort_by, flat_map,
        filter_map, take_while, drop_while, reduce, group_by,
        fold, scan, zip_with, partition, count, find_index, update
```

### Tier 3: Platform (30)
```
fs:      read_text, read_bytes, write, write_bytes, append,
         mkdir_p, exists, remove, list_dir, is_dir, is_file,
         copy, rename, walk, stat, glob, temp_dir
http:    get, post, put, patch, delete, request
io:      print, read_line
env:     get, set, args, cwd, os
process: exec, exec_status, exit
random:  int, float, bytes, choice
json:    parse, stringify
regex:   new, is_match, find, find_all, replace, split, captures
```

---

## Success Criteria

- `almide test` passes all tests
- Zero TOML definition files, no stdlib generation code in build.rs
- `cargo test --manifest-path stdlib/Cargo.toml` passes native Rust tests
- `deno test stdlib/*/extern_test.ts` passes native TS tests
- Variant resolution works correctly
- @extern contract is verified at compile time (existence check)
- Behavior consistency between Rust/TS guaranteed by conformance tests

## Dependencies

- [IR Optimization Passes](ir-optimization.md) — Affects Step 5 decision (for+append optimization)
- [Codegen Refinement](codegen-refinement.md) — Quality of .almd generated code

## Related

- [Stdlib API Surface Reform](stdlib-verb-system.md) -- API vocabulary reform (separate track)

## Supersedes

- [Stdlib Strategy](stdlib-strategy.md) Strategy 1 (TOML + runtime) and Strategy 2 (@extern)
  - Strategy 3 (self-host) and Strategy 4 (x/ packages) remain valid
- [Stdlib Architecture: 3-Layer Design](../on-hold/stdlib-architecture-3-layer-design.md) internal implementation approach

## Files
```
runtime/ts/core.ts (Result, Option, basic type TS representations -- ~50 lines)
runtime/ts/core.node.ts (Node-specific differences -- if any)
runtime/ts/core.browser.ts (Browser-specific differences -- if any)
runtime/ts/core_test.ts (Tests for the glue itself)
runtime/rust/core.rs (Result, Option, basic type Rust representations -- ~20 lines)
runtime/rust/core.wasm.rs (WASM-specific differences -- if any)
runtime/rust/core_test.rs (Tests for the glue itself)
src/parser/mod.rs (add @extern parsing)
src/check/ (handle @extern fn signatures)
src/lower.rs (emit Extern IR nodes)
src/emit_rust/ (include extern.rs, dispatch @extern calls)
src/emit_ts/ (include extern.ts, dispatch @extern calls)
src/resolve.rs (variant resolution logic)
src/cli.rs (add scaffold subcommand, --runtime/--runtime-version flags)
stdlib/*/mod.almd (type signatures + pure Almide)
stdlib/*/extern.rs (Rust native implementations — import runtime/rust/core)
stdlib/*/extern.{variant}.rs (Rust variant implementations)
stdlib/*/extern_test.rs (Rust tests)
stdlib/*/extern.ts (TS native implementations — import runtime/ts/core)
stdlib/*/extern.{variant}.ts (TS variant implementations)
stdlib/*/extern_test.ts (TS tests)
stdlib/Cargo.toml (workspace for Rust extern tests)
```
