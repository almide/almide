<!-- description: Five-layer security model making web vulnerabilities compile-time errors -->
# Secure by Design

## Thesis

Almide will become **web-safe** in the same way Rust is memory-safe. Not "safe if you're careful," but "safe by default. Won't break unless you intentionally try to break it."

```
Rust:  Memory-safe unless you write unsafe
Almide: Web-safe unless you write @extern
```

## Security Model

Almide's security consists of 5 layers. Each layer functions independently, and when all layers are in place, structural safety including supply chain is established.

### Layer 1: Effect Isolation — pure fn cannot perform I/O

```almide
fn parse(s: String) -> Value = ...          // pure. I/O impossible
effect fn load(path: String) -> String = ... // I/O possible
```

- `fn` cannot call `effect fn`. Verified by the compiler
- Pure fn cannot access the outside world at all. Data exfiltration and external communication are type errors
- **Security implication**: If a package only exports pure fn, that package is harmless by definition

**Status: ✅ Implemented in the language.** Effect system is working.

### Layer 2: Single Bridge — @extern is the only contact point with the outside world

```almide
@extern(platform: web, "fetch")
effect fn fetch(url: String) -> Response
```

- The only way to call native APIs is `@extern`
- `eval()`, `require()`, dynamic `import()` do not exist in the language
- **Security implication**: grep for `@extern` in the codebase and all contact points with the outside world are enumerated

**Status: ✅ @extern is implemented.** ✅ Capability inference implemented (Layer 2: 7 categories, transitive, `almide check --effects`). ❌ platform tags not yet implemented.

### Layer 3: Opaque Types — Restrict the means of constructing dangerous output

```almide
type SafeHtml = opaque String
type SafeSql  = opaque String
type SafePath = opaque String
```

- `opaque` types cannot be constructed directly from outside
- The only way to create `SafeHtml` is through a builder (with auto-escape)
- The only way to create `SafeSql` is through parameterized query functions
- stdlib I/O APIs only accept opaque types: `Response.html(body: SafeHtml)`
- **Security implication**: XSS, SQL injection, command injection, and path traversal become type errors

```almide
// Compile error: Response.html requires SafeHtml, cannot pass String
let html = "<p>" ++ user_input ++ "</p>"
Response.html(html)  // ← type error: expected SafeHtml, got String

// OK: builder auto-escapes
let doc = Html { p { user_input } }
Response.html(doc |> render)  // ← returns SafeHtml
```

**Status: ❌ Opaque types are not implemented.** Requires addition to parser + checker + codegen as a language feature.

### Layer 4: Capability Inference — Compiler infers package permissions

The compiler traces the function call graph, tracking which `@extern` each function transitively reaches.

```
json-parser package
├── parse()      → fn (pure) → no @extern
├── stringify()  → fn (pure) → no @extern
└── Inferred: capabilities = [] (pure)

http-client package
├── get()        → effect fn → fetch → @extern(platform: web, "fetch")
├── post()       → effect fn → fetch → @extern(platform: web, "fetch")
└── Inferred: capabilities = [network]

sketchy-logger package
├── log()        → effect fn → write_file → @extern(platform: node, "fs", ...)
├── report()     → effect fn → http.post → @extern(platform: web, "fetch")
└── Inferred: capabilities = [fs, network] ⚠️
```

Restrict capabilities on the consumer side:

```toml
# almide.toml
[dependencies.json-parser]
version = "1.0"
capabilities = []              # Only pure allowed

[dependencies.sketchy-logger]
version = "1.0"
capabilities = ["fs"]          # Only fs allowed, network denied
```

```
error: sketchy-logger requires capability "network", but only ["fs"] granted
  --> almide.toml:7:1
   |
   = note: sketchy-logger/src/report.almd:5 calls http.post
   = note: http.post uses @extern(platform: web, "fetch")
   = hint: add "network" to capabilities, or use a different package
```

**Status: ❌ Not implemented.** Required:
- @extern platform tags (→ platform-target-separation.md)
- Compiler capability inference pass (extension of effect propagation)
- capabilities field in almide.toml
- Transitive capability verification and diagnostic messages

### Layer 5: Supply Chain Integrity — Package tampering detection

```toml
[dependencies.http-client]
version = "2.0"
hash = "sha256:a1b2c3d4e5f6..."
capabilities = ["network"]
```

- Packages are pinned by source content hash
- Even the same version with a different hash results in **compile error**
- Capability changes are also detected: a package that was pure in v2.0 requesting network in v2.1 → explicit approval required

```
warning: http-client 2.0 → 2.1 adds new capability "fs"
  --> almide.toml:3:1
   |
   = note: http-client@2.1/src/cache.almd uses fs.write_file
   = hint: add "fs" to capabilities, or pin to version 2.0
```

**Status: ❌ Not implemented.** The package registry itself is not yet built (→ package-registry.md).

## Attack Surface Elimination

When all 5 layers are in place, which layer stops each attack:

| Attack | Layer 1 | Layer 2 | Layer 3 | Layer 4 | Layer 5 |
|---|---|---|---|---|---|
| XSS (string injection) | | | **opaque SafeHtml** | | |
| SQL injection | | | **opaque SafeSql** | | |
| Command injection | | | **opaque SafeCmd** | | |
| Path traversal | | | **opaque SafePath** | | |
| Package data exfiltration | **effect isolation** | **@extern only** | | **capability detection** | |
| Install-time code execution | | **no eval** | | | **hash verification** |
| Transitive dependency poisoning | | | | **transitive tracking** | **hash verification** |
| Prototype pollution | **immutable** | **no prototype** | | | |
| Eval injection | | **no eval** | | | |
| Version overwrite attack | | | | | **content hash** |
| Typosquatting | | | | **capability mismatch** | **hash verification** |

**A single attack stopped by multiple layers = defense in depth.**

## Implementation Order

Implementation order based on dependencies:

```
Phase 1: opaque types
  ← Language feature addition. parser + checker + codegen
  ← Foundation for SafeHtml, SafeSql, SafePath
  ← Make builder lift return SafeHtml
  ← This alone makes XSS/SQLi/Command injection type errors

Phase 2: @extern platform tags
  ← platform-target-separation.md
  ← @extern(platform: web, ...) / @extern(platform: node, ...) etc.
  ← Prerequisite for capability inference

Phase 3: capability inference
  ← Built on Phase 2
  ← Compiler tracks transitive @extern reachability
  ← Auto-compute capabilities per package
  ← capabilities field in almide.toml
  ← This enables supply chain capability verification

Phase 4: supply chain integrity
  ← package-registry.md
  ← Content-addressed hash
  ← Capability change detection
  ← @extern usage restriction policy
```

**Phase 1 alone makes XSS/SQLi/Command injection/Path traversal type errors.** Maximum impact at minimum implementation cost.

Phase 2-3 adds supply chain security. Phase 4 depends on infrastructure (registry).

## Prerequisites

| Phase | Dependent roadmap item | Reason |
|---|---|---|
| Phase 1 | None (language core addition) | opaque is an independent type system extension |
| Phase 2 | platform-target-separation.md | @extern platform tags |
| Phase 3 | Phase 2 + effect system (existing) | capability = transitive inference of platform tags |
| Phase 4 | package-registry.md | Content hash requires a registry |

## What Already Works (Layer 0)

These are already baked into the language and require no changes:

- ✅ `fn` cannot do I/O (effect system)
- ✅ `@extern` is the only FFI bridge
- ✅ `eval()` / dynamic import do not exist
- ✅ Prototype chain does not exist
- ✅ Immutable by default
- ✅ Static types make all code paths visible
- ✅ Packages are .almd source files (no install-time code execution)

**This Layer 0 is the most important and hardest to change.** Almide already has it. It's impossible for npm/Node.js to gain this retroactively (`require()` having full permissions is fundamental to their design).

## Design Principle

**Not "make it impossible to write unsafe code" but "writing normally results in safe code."**

- Write HTML with builder → auto-escape (safe)
- Write SQL with `sql()` → auto-parameterize (safe)
- Use a package → capabilities are auto-inferred (safe)
- Write `@extern` → this is the only act of "intentionally stepping outside safety"

In Rust, `unsafe` is a marker for "from here on, I take responsibility." In Almide, `@extern` is the same. **The language default is safe, and danger is explicit.**

## Why ON HOLD

Phase 1 (opaque types) can begin after language core stabilization. Phase 2 onward depends on platform-target-separation.md and package registry.

However:

- **Layer 0 is already complete** — The most important and hardest-to-change part
- **Phase 1 (opaque) alone makes XSS/SQLi/Path traversal type errors** — Maximum effect at minimum investment
- **No unresolved research questions in the overall design** — A combination of known techniques

Just as Rust made memory safety a property of the language, Almide will make web safety a property of the language. Technically possible. It's a matter of sequencing.

## Coverage — What Gets Resolved After All Phases, What Remains

### Structurally Eliminated (Language Guarantees)

| Category | Resolution | Mechanism |
|---|---|---|
| Injection attacks (XSS, SQLi, CMDi) | **100%** | opaque types. Cannot pass String to dangerous sinks. Type error |
| Path traversal | **100%** | opaque SafePath. Validation forced at construction |
| Prototype pollution | **100%** | Prototype chain does not exist in the language |
| Eval injection | **100%** | eval / dynamic import do not exist in the language |
| Install-time code execution | **100%** | Packages are .almd source files. No execution hooks exist |
| Supply chain (malicious packages) | **95%** | Compile-time detection via capability inference. Abuse within granted capabilities remains |
| Version overwrite / typosquatting | **100%** | Content-addressed hash. Hash mismatch causes compile error |

### Structurally Unresolvable (Remains in Any Language)

| Category | Resolution | Reason |
|---|---|---|
| Logic bugs (authorization gaps, IDOR, etc.) | **0%** | "Should this operation be allowed" is business logic. Cannot be expressed in types |
| Correctness of validation function internals | **0%** | Construction methods for opaque types can be restricted, but the correctness of those construction functions is human responsibility |
| Side-channel / timing attacks | **0%** | Execution time uniformity is outside compiler guarantees |
| SSRF (complete prevention) | **Partial** | Can be mitigated with SafeUrl type + allowlist, but allowlist correctness is human responsibility |

### Implication

**The majority of OWASP Top 10 becomes structurally zero.** What remains are "problems that persist regardless of language." A clear separation between problems you don't need to worry about when writing in Almide and problems you must worry about in any language.
