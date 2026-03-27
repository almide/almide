<!-- description: AOT cross-compilation producing multiple target artifacts in one build -->
<!-- done: 2026-03-18 -->
# Cross-Target AOT Compilation [PLANNED]

## Motivation

Almide already has multiple emit targets (Rust / TS / JS / future WASM) and can declaratively switch target-specific implementations via `@extern`. Leveraging this structure, build an AOT cross-compilation system that generates artifacts for multiple targets simultaneously with a single `almide build`.

## Existing foundation

Almide's compilation pipeline already has cross-target-ready structure:

```
Source (.almd)
  → Lexer → Parser → AST
  → Checker → IR (共通)
  → emit_rust/  (native CLI / cargo crate)
  → emit_ts/    (TS/JS / npm パッケージ)
  → emit_wasm/  (将来: WASM 直接出力 + JS グルー)
```

- Checker and IR are common across all targets
- `@extern(rs, ...)` / `@extern(ts, ...)` support target-specific implementations at the language level
- Target-specific code is confined to the emit layer

## Goal

```bash
almide build --target all
```

```
dist/
├── native/     Rust → binary (macOS / Linux / Windows)
├── web/        WASM + JS glue (for browsers)
├── npm/        JS package (npm publishable)
└── deno/       TS module (for Deno / Bun)
```

## Comparison with other languages

| Language/Framework | Cross-target strategy | Difference from Almide |
|---|---|---|
| **Kotlin Multiplatform** | `commonMain` → JVM / iOS / JS / WASM | Target branching via `expect`/`actual`. Same design philosophy as Almide's `@extern` |
| **Rust** | `#[cfg(target)]` + cross-compile | Conditional compilation is powerful, but no JS/TS targets |
| **Go** | Cross-compile via `GOOS`/`GOARCH` | Native only. Web targets via TinyGo |
| **Dart (Flutter)** | AOT (iOS/Android) + JIT (dev) | Platform channel branching. Not at the language level |
| **Zig** | `comptime` + target detection | Zero-cost abstraction. But web targets are limited |

**Almide's advantage**: Since `@extern` is a first-class language feature, target branching is type-safe and declarative rather than ad-hoc conditional branching. Furthermore, few languages have both native (Rust) and web (TS/JS/WASM) as first-class targets.

## Implementation Phases

### Phase 1: Extend `almide build --target`

- [ ] `--target all` to sequentially build all targets
- [ ] Comma-separated multiple targets like `--target native,npm`
- [ ] Set default targets in `almide.toml` `[build]` section

```toml
[build]
targets = ["native", "npm"]
```

### Phase 2: Target-specific optimizations

Insert target-specific optimization passes between IR → emit:

| Target | Optimizations |
|--------|---------------|
| Rust (native) | borrow analysis, lifetime inference, zero-copy optimization |
| TS/JS (web) | tree shaking, minification-friendly naming |
| WASM (direct) | size optimization, linear memory layout optimization |
| npm (package) | bundle only used stdlib, ESM/CJS dual output |

### Phase 3: Artifact packaging

- [ ] Unified output to `dist/` directory
- [ ] npm: auto-generate `package.json` (partially implemented in `emit_npm_package`)
- [ ] WASM: bundle `.wasm` + `.js` glue (see emit-wasm-direct.md)
- [ ] native: per-target-triple binaries (`x86_64-apple-darwin` etc.)

### Phase 4: CI/CD integration

```yaml
# Build all targets with GitHub Actions
- run: almide build --target all
- uses: actions/upload-artifact@v4
  with:
    path: dist/
```

- [ ] `almide publish` command for bulk publishing to npm + crates.io + GitHub Release
- [ ] Provide GitHub Actions template

## Relationship between @extern and cross-target

`@extern` is the core of this system. Declares target branching in a type-safe way:

```almide
// Native uses OS API, Web uses fetch API
@extern(rs, "reqwest", "get")
@extern(ts, "fetch_wrapper", "get")
fn http_get(url: String) -> Result[String, String]

// Common logic works on all targets as-is
fn parse_response(body: String) -> Data =
  json.parse(body) |> unwrap_or(default_data())
```

The difficulty of cross-target is "where to branch by target," but in Almide, the `@extern` boundary is clear, so the separation between common code and target-specific code is guaranteed at the compiler level.

## Dependencies

- emit-wasm-direct.md: WASM target is a prerequisite from Phase 3 onward
- package-registry.md: `almide publish` requires a registry
- stdlib-architecture-3-layer-design.md: platform separation ensures cross-target safety
