<!-- description: Native TS output for edge runtimes (Workers, Deno Deploy, Vercel) -->
<!-- done: 2026-03-18 -->
# TS Edge-Native Deployment

## Thesis

Almide's `--target ts` output is **plain TypeScript/JavaScript** that V8 executes directly. No WASM involved. This fundamentally avoids the problems that WASM-based languages (Rust→WASM, Go→WASM, MoonBit) face on edge runtimes like Cloudflare Workers, Deno Deploy, and Vercel Edge Functions.

```
Rust → WASM → V8 (WASM instantiate 5-50ms, no JIT, FFI overhead)
Almide → TS → V8 (JS parse <1ms, full JIT, native ecosystem)
```

**This means Almide has the potential to become the fastest non-JS language on the edge.**

## Why This Matters

### Real-world Problems with WASM on Edge

| Problem | Cause | Almide→TS Situation |
|---------|-------|---------------------|
| Cold start latency | WASM instantiate + memory allocation (5-50ms) | JS parse <1ms. No issue |
| No JIT optimization | WASM is AOT; V8's TurboFan doesn't apply | Full JIT optimization target |
| JS ecosystem disconnect | FFI overhead between WASM↔JS | Native JS. npm packages directly usable |
| Bundle size | WASM binaries hundreds of KB to MB | 45-100KB (room for improvement) |
| Difficult debugging | WASM stack traces are opaque | Generated TS is readable. Source maps are theoretically possible |

### Almide's Structural Advantages

1. **The type checker has full type information** — the emitter can choose optimal code based on types
2. **Multi-target** — the same code can also become a native binary via `--target rust`. Server in Rust, edge in TS — one language covers both
3. **Output is improvable** — runtime overhead is an emitter optimization issue, not an architectural constraint. Since type information is available, it can always be tightened later

## Current State

### What Works

- `--target ts` outputs TypeScript for Deno (working)
- `--target npm` outputs npm packages (working, selective module loading)
- Result erasure: `ok(x)` → `x`, `err(e)` → `throw` (TS-idiomatic)
- 22 stdlib modules (string, list, map, json, http, fs, crypto, etc.)

### Optimization Opportunities

Since the type checker knows the types, the following optimizations are possible in the emitter:

| Current | After Optimization | Condition |
|---------|--------------------|-----------|
| `__deep_eq(a, b)` | `a === b` | Both sides are primitive types (Int, String, Bool) |
| `__bigop("%", n, 3)` | `n % 3` | Both sides are Int and within non-BigInt range |
| `__bigop("+", a, b)` | `a + b` | Same as above |
| `__div(a, b)` | `Math.trunc(a / b)` or `a / b` | Int division or Float division determined by type |
| `__concat(a, b)` | `a + b` | Both sides are String |
| All stdlib modules embedded | Only used modules | Tree-shake for `--target ts` like npm |

**All of these require only emitter changes. No language spec or runtime changes needed.**

## Edge Platform Compatibility

| Platform | Runtime | Almide→TS Compatibility |
|----------|---------|------------------------|
| Cloudflare Workers | V8 isolate | Fully compatible. Script size limit Free 1MB / Paid 10MB → plenty of headroom |
| Deno Deploy | Deno (V8) | `--target ts` works as-is. Current primary target |
| Vercel Edge Functions | V8 (Edge Runtime) | ESM compatible. Supported via npm target |
| AWS Lambda@Edge | Node.js | Supported via `--target npm` |
| Fastly Compute | WASM only | Not supported (requires WASM target) |

## What Needs to Happen

### Phase 1: Lightweight TS Output (Emitter Optimization)

Eliminate helper functions based on type information. Implement the "Optimization Opportunities" table above.

- [ ] Primitive type `==`/`!=` → direct `===`/`!==` output
- [ ] Primitive type arithmetic → direct output (remove BigInt dispatch)
- [ ] Remove unused stdlib modules in `--target ts` (tree-shake like npm)
- [ ] Benchmark: compare bundle size and execution speed before/after optimization

### Phase 2: Platform / Target Separation

→ Extracted to **[platform-target-separation.md](platform-target-separation.md)**.

Separate `--target` (output language) from `--platform` (API availability), and introduce platform tiers to `@extern`. Which stdlib functions are available on edge is determined at compile time.

### Phase 3: Edge Entry Points

Enable natural patterns for writing HTTP handlers in Almide.

```almide
// Cloudflare Workers 向けの最小例
effect fn handle(req: Request) -> Response =
  match req.method {
    "GET" => Response.text("Hello from Almide"),
    _ => Response.text("Method not allowed", status: 405),
  }
```

- [ ] `Request`/`Response` 型の定義 (Web standard Fetch API 準拠 — `@extern(js-web, ...)`)
- [ ] `export default { fetch: handle }` 形式の出力
- [ ] Cloudflare Workers / Deno Deploy / Vercel Edge 向けのエントリポイントテンプレート

### Phase 4: Benchmarks & Validation

Prove "faster than WASM" with numbers.

- [ ] Cold start comparison on Cloudflare Workers with identical logic: Almide→TS vs Rust→WASM
- [ ] Execution speed comparison: JSON parse/serialize, HTTP routing, string processing
- [ ] Bundle size comparison

## Relationship to Other Roadmap Items

- **almide-ui.md**: Almide's reactive UI framework. Built on TS edge-native emitter optimization + builder mechanism. Phase 1 of this document is the performance foundation for Almide UI
- **emit-wasm-direct.md**: Independent of WASM direct output. The value of TS edge-native is not using WASM
- **cross-target-semantics.md**: TS output correctness is a prerequisite for this document. P0 fixes are essential
- **Result Builder (template.md)**: HTML builder + TS edge output = the complete story of running Almide web apps on the edge

## Why ON HOLD

At this point, emitter optimization (Phase 1) takes priority. Language core stabilization and expanding existing tests come first. However:

- **Zero architectural blockers** — everything needed is emitter-side improvements
- **The type checker already has type information** — the foundation for optimization already exists
- **`--target ts` is already working** — not starting from zero

High confidence. Just a matter of timing.
