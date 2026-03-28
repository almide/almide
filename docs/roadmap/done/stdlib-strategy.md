<!-- description: Stdlib expansion strategy via Rust ecosystem wrapping -->
<!-- done: 2026-03-18 -->
# Stdlib Strategy

Proliferation requires a stdlib thick enough to "write what you want immediately." Currently 15 modules, 266 functions. Compared to major languages:

| Language | stdlib Module Count | Function/Method Count |
|------|-------------------|----------------|
| Go | ~150 packages | Thousands |
| Python | ~200 modules | Tens of thousands |
| Rust (std) | ~50 modules | Thousands |
| Deno (std) | ~30 modules | Hundreds |
| **Almide** | **15 + 6 bundled** | **~282** |

Leveraging the fact that Almide compiles to Rust, **wrapping the Rust ecosystem** is the fastest expansion strategy.

---

## Current State (v0.5.13)

### Layer 1: core (all targets, WASM OK)
| Module | Functions | Status |
|--------|-----------|--------|
| string | 36 | ✅ Comprehensive |
| list | 54 | ✅ Comprehensive |
| int | 21 | ✅ Sufficient |
| float | 16 | ✅ Sufficient |
| map | 16 | △ Basic only |
| math | 21 | ✅ Sufficient |
| json | 36 | ✅ Comprehensive |
| regex | 8 | △ Basic only |
| result | 9 | ✅ Complete |

### Layer 2: platform (native only)
| Module | Functions | Status |
|--------|-----------|--------|
| fs | 19 | △ Basic only |
| process | 6 | △ Minimal |
| io | 3 | △ Minimal |
| env | 9 | △ Basic only |
| http | 8 | △ Basic only |
| random | 4 | △ Minimal |

### Bundled .almd
| Module | Status |
|--------|--------|
| path | ✅ Sufficient |
| time | △ Basic only |
| hash | △ SHA/MD5 only |
| encoding | △ base64/hex only |
| args | △ Basic only |
| term | △ Basic only |

---

## Expansion Strategy

### Strategy 1: Add via TOML + Runtime (current approach)

Cost of adding a new function:
1. Add definition to `stdlib/defs/<module>.toml`
2. Add Rust implementation to `src/emit_rust/<xxx>_runtime.txt`
3. Add TS implementation to `src/emit_ts_runtime.rs`
4. Auto-generated via `cargo build`

**Pros:** No compiler core changes needed, type-safe, TOML definitions readable by LLMs
**Cons:** Implementation needed for 2 targets, manual translation required to wrap Rust crate features

**Applicable to:** Core layer function additions (expanding string, list, map, math)

### Strategy 2: Wrap Rust crates with @extern

Call Rust crate functions directly with `@extern(rs, "crate", "function")`.

```almide
@extern(rs, "chrono", "Utc::now().to_rfc3339")
@extern(ts, "Date", "new Date().toISOString")
fn now_iso() -> String
```

**Pros:** Access to full Rust ecosystem features, minimal implementation cost
**Cons:** TS side also needs separate implementation, type mapping is trust-based (safety is user's responsibility)

**Applicable to:** New platform layer modules (datetime, crypto, database, etc.)

### Strategy 3: Self-host in Almide

Write pure computation logic in Almide itself.

**Pros:** Automatic support for both targets, tests writable in Almide
**Cons:** Performance depends on Almide's generated code quality

**Applicable to:** csv, toml parsers, data conversion, validation

### Strategy 4: Official extension packages (x/)

Versioned independently from stdlib. Add dependencies in `almide.toml`.

**Pros:** Free from stdlib version lock, easier for community contributions
**Cons:** Requires package registry (currently on-hold)

**Applicable to:** Large features (web frameworks, ORM, template engines)

---

## Missing Modules (by priority)

### Tier 1: Cannot write practical programs without these

| Module | Content | Strategy | Estimate |
|--------|---------|----------|----------|
| **datetime** | Date parsing/formatting/timezone/comparison | TOML + runtime (Rust: chrono, TS: Intl) | 20-30 functions |
| **fs (expansion)** | Directory traversal, recursive delete, permissions, temp, watch | TOML + runtime | +15 functions |
| **http (expansion)** | Header manipulation, status codes, cookie, multipart | TOML + runtime | +20 functions |
| **error** | Structured error types, stack trace, chain | TOML + runtime | 10 functions |

### Tier 2: Needed by many applications

| Module | Content | Strategy | Estimate |
|--------|---------|----------|----------|
| **csv** | parse/stringify, with headers, streaming | self-host (.almd) | 8-10 functions |
| **toml** | parse/stringify | self-host (.almd) | 6-8 functions |
| **yaml** | parse/stringify | @extern (serde_yaml / js-yaml) | 4-6 functions |
| **url** | Parse/build/encode/query parameters | self-host (.almd) | 10 functions |
| **crypto** | HMAC, AES, RSA, random bytes | @extern (ring / Web Crypto) | 10-15 functions |
| **uuid** | v4 generation, parse, format | @extern (uuid / crypto.randomUUID) | 4 functions |

### Tier 3: Needed for ecosystem growth

| Module | Content | Strategy | Estimate |
|--------|---------|----------|----------|
| **sql** | Parameterized queries, SQLite/PostgreSQL | @extern + x/ package | 15 functions |
| **websocket** | client/server, message send/receive | @extern | 8 functions |
| **log** | Structured logging, levels, formatters | TOML + runtime | 6 functions |
| **test** | Mock, spy, benchmark | TOML + runtime | 10 functions |
| **compress** | gzip/zstd compression/decompression | @extern | 4 functions |
| **image** | Basic image manipulation | x/ package | 15 functions |

---

## Numerical Targets

| Milestone | Modules | Functions | Baseline |
|-----------|---------|-----------|----------|
| Current (v0.5.13) | 21 | ~282 | -- |
| **v0.6 (minimum practical)** | 25 | 400+ | Tier 1 complete |
| **v0.8 (app development ready)** | 32 | 550+ | Tier 2 complete |
| **v1.0 (production ready)** | 38+ | 700+ | Key Tier 3 modules |

Comparison: Go 1.0 shipped with ~100 packages. Deno 1.0 with ~30 modules. Since Almide compiles to Rust, using Rust crates via @extern provides access to more capabilities than the module count suggests.

---

## LLM Suitability Perspective

### Consistent naming conventions

```
module.verb_noun(args)     — basic form
module.verb_noun(args)     — returns Bool
module.try_verb(args)      — returns Result
```

Enforce this pattern across all modules. LLMs learn consistent patterns more easily.

### Natural calling with UFCS

```almide
// Both forms work
string.trim(s)
s.trim()

// LLMs tend to prefer UFCS (method chains are more readable)
text
  |> string.trim()
  |> string.split(",")
  |> list.map(fn(x) => string.trim(x))
```

### Machine-readability of stdlib documentation

Add a `description` field to TOML definitions so that future `almide doc` is also usable for LLMs:

```toml
[trim]
description = "Remove leading and trailing whitespace from a string"
params = [{ name = "s", type = "String" }]
return = "String"
```

This enables automatic injection of stdlib reference into LLM prompts.

---

## Implementation Order

```
1. Tier 1 (datetime, fs expansion, http expansion, error)  ← v0.6 target
   ↓
2. Add description field to TOML                           ← LLM suitability
   ↓
3. Tier 2 (csv, toml, url, crypto, uuid)                   ← v0.8 target
   ↓
4. Strengthen @extern type safety                           ← Phase 0 prerequisite
   ↓
5. Tier 3 (sql, websocket, log, test)                      ← v1.0 target
   ↓
6. x/ package separation                                    ← package registry prerequisite
```

## Auto-Collection Tool (Built in Almide)

**Implement in Almide itself** a tool that automatically collects API references from other languages for stdlib design. Three birds with one stone: dogfooding + practical tool + stdlib completeness benchmark.

### Concept

```almide
// Get stdlib/lib info in a unified format per language
effect fn main() =
  let go_time = fetch_module("go", "time")
  let py_datetime = fetch_module("python", "datetime")
  let report = compare([go_time, py_datetime])
  fs.write_text("docs/roadmap/stdlib/auto/time.md", render_markdown(report))
```

### Unified Output Format

```json
{
  "language": "go",
  "module": "time",
  "functions": [
    {
      "name": "Now",
      "params": [],
      "return": "Time",
      "description": "returns the current local time"
    }
  ]
}
```

### Data Sources

**Recommended: Via each language's reflection/documentation tools (no scraping needed)**

```bash
# Python: Convert all function signatures to JSON using inspect
python3 -c "import inspect, json, datetime; print(json.dumps([
  {'name': n, 'params': str(inspect.signature(f))}
  for n, f in inspect.getmembers(datetime, inspect.isfunction)
]))"

# Go: Structured output of package info with go doc -json
go doc -all -json time

# Rust: rustdoc supports JSON output
rustdoc --output-format json --edition 2021 src/lib.rs

# Deno: deno doc supports JSON output
deno doc --json https://deno.land/std/csv/mod.ts

# Node: Parse TypeScript type definitions (.d.ts)
# or: Get function list with Object.keys(require('fs'))
```

By using Almide's `process.exec` to invoke these commands and merging the JSON, you get accurate API references. More reliable and accurate than HTTP scraping.

| Language | Tool | Format | Accuracy |
|------|--------|------|------|
| Python | `inspect` module | JSON (custom conversion) | Complete (signatures + docstrings) |
| Go | `go doc -json` | JSON | Complete (includes type info) |
| Rust | `rustdoc --output-format json` | JSON | Complete (per crate) |
| Deno | `deno doc --json` | JSON | Complete (per module) |
| Node/npm | `.d.ts` file parsing | TypeScript AST | High accuracy |
| Swift | `swift-symbolgraph-extract` | JSON (Symbol Graph) | Complete (per module) |
| Kotlin | `dokka` or `kotlin-reflect` | JSON / Reflection | High accuracy |
| Ruby | `ri --format=json` or `RDoc::RI` | JSON | Complete (methods + docstrings) |

```bash
# Swift: Output all APIs as JSON via Symbol Graph
swift-symbolgraph-extract -module-name Foundation -target x86_64-apple-macosx

# Kotlin: List classes/functions with kotlin-reflect
kotlinc -script -e "kotlin.io.path.Path::class.members.forEach { println(it) }"
# or: Generate JSON documentation with Dokka

# Ruby: List methods with ri
ri --format=json File
# or: Ruby reflection
ruby -e "puts File.methods(false).sort"
```

**Fallback: Web API / HTML Scraping**

Fallback when local tools are unavailable:

| Language | Source | Format |
|------|--------|------|
| Go | `pkg.go.dev` | HTML scraping |
| Python | `docs.python.org` | HTML scraping |
| Rust | `docs.rs` | HTML scraping |
| Deno | `doc.deno.land` | JSON API |
| npm | `registry.npmjs.org` | JSON API |
| Swift | `developer.apple.com/documentation` | HTML scraping |
| Kotlin | `kotlinlang.org/api` | HTML scraping |
| Ruby | `ruby-doc.org` | HTML scraping |

### Extensions

- Expand to third-party libs using the same mechanism as stdlib
- `almide stdlib-compare datetime` displays a list of datetime equivalents across Go/Python/Rust/Deno
- Run periodically in CI -> auto-update `docs/roadmap/stdlib/auto/`

### Prerequisites

- Fully implementable with Almide's http + json + string (no additional features needed)
- A simple selector for HTML parsing would be useful (future stdlib candidate)

## Benchmark Target Languages

### Traditional Languages (stdlib feature comparison)

| Language | Reflection Tool | stdlib Scale |
|------|-------------------|-------------|
| Go | `go doc -json` | ~150 packages |
| Python | `inspect` module | ~200 modules |
| Rust | `rustdoc --output-format json` | ~50 modules + crates |
| Deno | `deno doc --json` | ~30 modules |
| Swift | `swift-symbolgraph-extract` | Foundation + standard |
| Kotlin | `dokka` / `kotlin-reflect` | kotlin-stdlib + kotlinx |
| Ruby | `ri --format=json` | ~100 modules |

### LLM-Era Languages (modification success rate and design philosophy comparison)

| Language | Appeared | Characteristics | Comparison Points with Almide |
|------|------|------|----------------------|
| **Mojo** | 2023 | Python superset, AI/ML focused, compiled | Balance of performance vs writability, designed for LLMs to leverage Python knowledge |
| **Moonbit** | 2023 | WASM-first, 2-layer stdlib of core/x, designed for AI assistance | Most relevant stdlib design reference. Origin of Almide's 3-layer design |
| **Gleam** | 2024 (1.0) | Type-safe, BEAM + JS multi-target, simple syntax | Multi-target codegen, error design, reference for @extern pattern |
| **Pkl** | 2024 | By Apple, configuration language, typed structured data | DSL design for configuration file purposes |
| **Bend** | 2024 | Massively parallel functional, automatic GPU parallelization | Parallel computation model, functional optimization |
| **Roc** | In development | Functional, zero runtime exceptions, platform separation | Platform separation, error-free design, simplicity for LLMs |

### 3 Languages for Direct Comparison by LLM Modification Success Rate

1. **Mojo** -- Python-compatible knowledge transfer vs Almide's unique syntax. Mojo may have an advantage due to LLMs' existing Python knowledge
2. **Moonbit** -- WASM-first + designed for AI assistance. Closest stdlib design philosophy. Direct competitor
3. **Gleam** -- Simple syntax + strong types. Prior example of multi-target. Almide referenced Gleam's `@extern` pattern

### Measurement Method

Using Grammar Lab's A/B testing framework, have LLMs write the same tasks in Almide / Mojo / Moonbit / Gleam, and compare modification success rates.

```
Example tasks:
- FizzBuzz -> JSON API server (progressively more complex)
- CSV -> JSON conversion tool
- TODO app (CRUD + tests)

Metrics:
- First-attempt success rate (can it write correctly on the first try?)
- Modification success rate (can it self-repair from errors?)
- Code volume (how many lines for the same feature?)
- Error message usefulness (can the LLM fix code from error messages?)
```

### Auto-Collection Targets

Include LLM-era languages in the collection targets:

| Language | Tool | Notes |
|----------|------|-------|
| Mojo | `mojo doc` (in development) | Partially available via Python's `inspect` |
| Moonbit | `moon doc` | Official documentation tool |
| Gleam | `gleam docs` | HTML generation, JSON output not yet supported |
| Roc | `roc docs` | In development |

## Related Roadmap

- [Stdlib Architecture: 3-Layer Design](../on-hold/stdlib-architecture-3-layer-design.md) — Detailed layer separation design
- [Package Registry](../on-hold/package-registry.md) — Distribution infrastructure for x/ packages
- [Rainbow FFI Gate](../on-hold/rainbow-gate.md) — Multi-language FFI (evolution of @extern)
- [Codec Protocol & JSON](active/codec-and-json.md) — Next format support beyond JSON
