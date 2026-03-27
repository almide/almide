<!-- description: Separate --target (output language) from --platform (runtime env) -->
<!-- done: 2026-03-18 -->
# Platform / Target Separation

## Thesis

`--target` conflates two meanings: output format and platform. Separate them.

```bash
# Current: platform mixed into target
almide build app.almd --target ts-browser
almide build app.almd --target ts-node
almide build app.almd --target ts-worker

# Ideal: two orthogonal axes
almide build app.almd --target ts --platform browser
almide build app.almd --target ts --platform node
almide build app.almd --target ts --platform worker
almide build app.almd --target rust --platform native
```

- **target** = output language (codegen selection): `rust`, `ts`, `wasm`
- **platform** = available API set (`@extern` availability): `browser`, `node`, `worker`, `native`

## Why Separate

### Problem: Mixing platform into target causes combinatorial explosion

All target x platform combinations become `--target` variants:

```
ts-browser, ts-node, ts-worker, ts-deno,
rust-native, rust-wasm,
wasm-browser, wasm-worker, ...
```

Every new platform requires combinations with all targets. Does not scale.

### Solution: Two orthogonal axes

```
target (codegen)     platform (API availability)
├── rust              ├── native
├── ts                ├── node
└── wasm              ├── browser
                      └── worker
```

Target and platform can be chosen independently. The compiler validates the combination:

| target | platform | Validity |
|---|---|---|
| ts | browser | OK — DOM API available |
| ts | node | OK — fs, process available |
| ts | worker | OK — fetch, KV available |
| ts | native | NG — TS has no native runtime |
| rust | native | OK — std::fs, std::process available |
| rust | browser | NG (possible via wasm, future work) |
| wasm | browser | OK — Web API + WASM import |
| wasm | worker | OK — WASM Workers |

## Platform Hierarchy

Platforms have a hierarchical inclusion structure:

```
any                  ← JSON, Math, Array, basic type operations
├── web              ← Web standard API (fetch, URL, Request/Response, crypto.subtle)
│   ├── browser      ← DOM (document, window, navigator, localStorage)
│   └── worker       ← Edge-specific (KV, D1, env bindings, etc.)
├── node             ← Node.js API (fs, child_process, Buffer, path, etc.)
└── native           ← Rust std (std::fs, std::process, std::net, etc.)
```

**Inclusion rules:**
- `browser` functions can also use `web` and `any` functions
- `worker` functions can also use `web` and `any` functions
- `node` functions can only use `any` functions (`web` is not included)
- `native` functions can only use `any` functions

`web` serves as the common foundation for `browser` and `worker`. Web standard Fetch API, URL, crypto.subtle etc. belong to `web` and are available on both platforms.

## @extern Design

### Current

```almide
@extern(rs, "std::cmp", "min")
@extern(ts, "Math", "max")
fn my_max(a: Int, b: Int) -> Int
```

The first argument of `@extern` is the target language. No concept of platform.

### New Design

```almide
// Tied to platform capabilities
@extern(platform: any, "JSON", "parse")
fn parse_json(s: String) -> Value

@extern(platform: web, "fetch")
effect fn fetch(url: String) -> Response

@extern(platform: browser, "document", "createElement")
fn create_element(tag: String) -> DomNode

@extern(platform: node, "fs", "readFileSync")
fn read_file(path: String) -> String

@extern(platform: native, "std::fs", "read_to_string")
fn read_file(path: String) -> String
```

### Intersection of Target and Platform

A single function can have target-specific implementations + platform constraints:

```almide
// fs.read_file definition (inside stdlib)
@extern(platform: node, "fs", "readFileSync")
@extern(platform: native, "std::fs", "read_to_string")
effect fn read_file(path: String) -> String
```

- `--target ts --platform node` — uses `fs.readFileSync`
- `--target rust --platform native` — uses `std::fs::read_to_string`
- `--target ts --platform browser` — compile error: `read_file requires platform node or native`

### Backward Compatibility

Existing `@extern(rs, ...)` / `@extern(ts, ...)` are maintained as shorthand for:

```almide
@extern(rs, ...)  →  @extern(target: rust, platform: native, ...)
@extern(ts, ...)  →  @extern(target: ts, platform: any, ...)
```

No changes required to existing code.

## Platform Inference

### Libraries — No declaration needed, inferred from usage

Libraries do not explicitly declare platform. The minimum required platform is automatically inferred from the `@extern` usage:

```almide
// my_lib.almd
import dom exposing (create_element, append)  // uses browser API

fn render(text: String) -> DomNode = {
  let el = create_element("p")
  // ...
  el
}
```

Compiler inference: `my_lib` uses `create_element` (@extern platform: browser) -> **requires platform: browser**

```bash
# User uses this library
almide build app.almd --target ts --platform node
# error: my_lib requires platform browser, but target platform is node
#   --> app.almd:1:1
#    |
#  1 | import my_lib
#    | ^^^^^^^^^^^^^^ my_lib uses dom.create_element (platform: browser)
#    |
#    = hint: use --platform browser, or remove the import
```

### Applications — Specified via --platform

```bash
almide build app.almd --target ts --platform browser   # explicit
almide build app.almd --target ts                      # default: node (backward compatible)
almide build app.almd --target rust                    # default: native
```

### Default in almide.toml

```toml
[build]
target = "ts"
platform = "worker"
```

Set per-project defaults. Useful for CI and team-wide consistency.

## Impact on stdlib

Each stdlib module function carries the appropriate platform tag:

| Module | platform | Reason |
|---|---|---|
| string, list, map, int, float | `any` | Pure computation |
| math, random | `any` | JS Math / Rust std::f64 |
| json | `any` | JSON.parse is common across all runtimes |
| regex | `any` | JS RegExp / Rust regex crate |
| crypto (hash) | `any` | Basic hashing works on all runtimes |
| crypto (subtle) | `web` | Web Crypto API |
| http (fetch) | `web` | Web standard Fetch API |
| http (server) | `node` | Node.js http module / Deno.serve |
| fs, path | `node` / `native` | Filesystem |
| process (spawn) | `node` / `native` | Child processes |
| env (get) | `any` | Deno.env, process.env, std::env all supported |
| env (set) | `node` / `native` | Not possible in browser/worker |
| dom | `browser` | DOM API |
| datetime | `any` | Date / chrono |
| log | `any` | console.log / eprintln |

**Platform can differ per function within a module.** `env.get` is `any` but `env.set` is `node`. This is naturally expressed through function-level `@extern` tags.

## Compile Error Design

### Calling unavailable APIs

```
error: fs.read_file requires platform node or native, but target platform is browser
  --> app.almd:5:3
   |
 5 |   let data = fs.read_file("config.json")
   |              ^^^^^^^^^^^^^
   |
   = hint: filesystem is not available in browser. Consider using fetch() to load data from a URL
```

### Platform-mismatched imports

```
error: cannot import server_lib (requires platform node) in platform worker
  --> app.almd:1:1
   |
 1 | import server_lib
   | ^^^^^^^^^^^^^^^^^
   |
   = note: server_lib uses fs.read_file (platform: node)
   = note: server_lib uses process.spawn (platform: node)
   = hint: use --platform node, or use a browser-compatible alternative
```

### Invalid target x platform combination

```
error: platform native is not compatible with target ts
  --> almide.toml:3:1
   |
 3 | platform = "native"
   | ^^^^^^^^^^^^^^^^^^^
   |
   = hint: use --target rust for native platform, or --platform node for ts target
```

## Relationship to Other Roadmap Items

- **ts-edge-native.md**: Prerequisite for the problem this document solves. Without platform separation, available APIs at the edge cannot be determined at compile time. ts-edge-native Phase 2 is transferred to this document
- **almide-ui.md**: Almide UI depends on `platform: browser`. Through platform inference, code using Almide UI automatically requires browser platform
- **cross-target-semantics.md**: Platform separation limits the scope of "same code produces same results in Rust and TS" verification to `platform: any` functions
- **rainbow-gate.md**: Rainbow FFI Gate is inherently platform-specific. The @extern platform tag becomes the foundation for FFI

## Why ON HOLD

The current `@extern(rs, ...)` / `@extern(ts, ...)` is sufficient for core language development. Platform separation becomes necessary when serious Web/Edge development on the TS target begins.

However:

- **Design is simple** — just add a platform field to @extern and have the compiler validate against the hierarchy
- **Backward compatible** — existing `@extern(rs, ...)` / `@extern(ts, ...)` maintained as shorthand
- **Incrementally adoptable** — can start with just 3: `any` / `node` / `browser`

Will be needed when ts-edge-native / almide-ui work begins.
