<!-- description: Almide vs Mojo capability parity assessment (Mojo = 100) -->
<!-- done: 2026-05-11 -->
# Almide vs Mojo: Capability Parity Assessment

> Mojo = 100 as baseline. Scored per axis based on feature completeness,
> depth of implementation, and practical usability. Last updated 2026-05-11.

## Scoring Summary

| Axis | Mojo | Almide | Gap | Notes |
|------|------|--------|-----|-------|
| **Type system** | 100 | 55 | -45 | Generics + protocols + **const generics** (`fn f[N: Int]()`). No HKT syntax, dependent types |
| **Compile-time computation** | 100 | 35 | -65 | **Value parameters work end-to-end** (`zeros[5]()`, `dot[3](a,b)`). No comptime eval yet |
| **Memory model** | 100 | 40 | -60 | LLVM alloca/store/load for mutable vars; no user-facing ownership |
| **SIMD / Vectorization** | 100 | 20 | -80 | LLVM O2 auto-vectorization enabled; no explicit SIMD types yet |
| **GPU** | 100 | 0 | -100 | Mojo: GPU kernels ship today; almide: nothing |
| **Optimizer** | 100 | 68 | -32 | egg + 30 nanopass + dialect SSA + **LLVM O2 pipeline** (inline, vectorize, mem2reg) |
| **Backend maturity** | 100 | 70 | -30 | 3 backends: Rust + WASM + **LLVM native** (7/7 equiv tests, C-perf, 33KB) |
| **Stdlib** | 100 | 62 | -38 | 746 fns / 35 modules; bit introspection, Hashable protocol |
| **Concurrency** | 100 | 50 | -50 | `fan` structured concurrency is clean but limited; no GPU parallelism |
| **Effect system** | 100 | 80 | -20 | Almide is stronger here — `effect fn` / `fn` separation is principled |
| **Tooling** | 100 | 45 | -55 | formatter + test + pkg manager; no debugger/profiler/LSP complete |
| **Interop** | 100 | 25 | -75 | Mojo: Python seamless; almide: WASM interop only, no FFI exposed |
| **Ecosystem** | 100 | 10 | -90 | Mojo: Modular backing + community; almide: single developer |
| **Documentation** | 100 | 55 | -45 | Good internal docs, CHEATSHEET, ARCHITECTURE; no user tutorial/book |
| **LLM code generation** | 50 | 100 | +50 | Almide's unique axis — MSR-driven design, almide-dojo |
| **Multi-target** | 70 | 75 | +5 | Almide: Rust + WASM + LLVM; Mojo: CPU + GPU (no WASM) |
| **Weighted total** | **100** | **47** | | Previous: 38→42→47 (2026-05-11) |

### Weighted Score Methodology

Weights reflect what matters for a **production systems/AI language**:

| Axis | Weight | Mojo weighted | Almide weighted |
|------|--------|---------------|-----------------|
| Type system | 12% | 12.0 | 5.4 |
| Compile-time computation | 10% | 10.0 | 1.0 |
| Memory model | 8% | 8.0 | 2.8 |
| SIMD / Vectorization | 8% | 8.0 | 0.4 |
| GPU | 5% | 5.0 | 0.0 |
| Optimizer | 10% | 10.0 | 5.5 |
| Backend maturity | 8% | 8.0 | 3.2 |
| Stdlib | 7% | 7.0 | 4.2 |
| Concurrency | 5% | 5.0 | 2.5 |
| Effect system | 3% | 3.0 | 2.4 |
| Tooling | 7% | 7.0 | 3.2 |
| Interop | 5% | 5.0 | 1.3 |
| Ecosystem | 5% | 5.0 | 0.5 |
| Documentation | 3% | 3.0 | 1.7 |
| LLM code generation | 2% | 1.0 | 2.0 |
| Multi-target | 2% | 1.4 | 1.5 |
| **Total** | **100%** | **98.4** | **37.6** |

**Almide is at ~38/100 relative to Mojo.**

---

## Detailed Breakdown

### 1. Type System (Almide: 45/100)

**What almide has:**
- Generics with protocol bounds (`fn f[T: Action](x: T)`)
- 7 built-in protocols (Eq, Repr, Ord, Hash, Codec, Encode, Decode, Numeric)
- User-defined protocols with `protocol` keyword + `impl` blocks
- Monomorphization-based dispatch (no dynamic dispatch)
- Record types, variant types (ADTs), tuples
- 18 scalar types (Int, Float, sized integers, Float32)
- Bidirectional type inference

**What Mojo has that almide doesn't:**
- `parameter` (compile-time values) — the foundation of Mojo's type-level computation
- Dependent-type-like `Tensor[DType, Layout, rows, cols]`
- Associated types in traits
- Ownership/borrowing as type-level annotations (`borrowed`, `owned`, `inout`)
- `@value` decorator for automatic lifecycle methods
- Compile-time `if` / `for` in type-level expressions
- `AnyType` / `Movable` / `Copyable` trait hierarchy

**Path to 70:**
- Add `comptime` parameters → const generics in Rust codegen
- Protocol inheritance (`protocol B: A`)
- Associated type projections

### 2. Compile-time Computation (Almide: 10/100)

**What almide has:**
- `@rewrite` rules compiled to egg rewrite rules (algebraic optimization)
- Constant folding pass

**What Mojo has:**
- `alias` for compile-time constants
- `parameter` declarations — first-class compile-time values
- `@parameter if` — conditional compilation
- Full expression evaluation at compile time
- Compile-time function execution
- Type-level arithmetic (`StaticIntTuple`, `DimList`)

**Path to 50:**
- `comptime` keyword for compile-time parameters
- `Array[T, N: comptime Int]` — fixed-size arrays
- Compile-time `if` in function bodies

### 3. Memory Model (Almide: 35/100)

**What almide has:**
- Rust target: ownership delegated to rustc (borrow inference pass inserts `&`, `.clone()`)
- WASM target: linear memory with manual layout
- `@consume` attribute for parameter ownership hints
- 4 related passes: borrow inference (58k), box deref, clone insertion, capture clone

**What Mojo has:**
- Full ownership system (`borrowed`, `owned`, `inout`)
- Deterministic destruction (ASAP)
- Move semantics with `^` transfer operator
- No garbage collection, no reference counting by default
- Compile-time borrow checking (like Rust, but integrated)

**Path to 55:**
- Expose ownership annotations in almide syntax
- Deterministic destruction semantics (not just Rust delegation)

### 4. SIMD / Vectorization (Almide: 5/100)

**What almide has:**
- WASM SIMD128 used internally by matrix module (v128 instructions in emit_wasm)
- No user-facing SIMD API

**What Mojo has:**
- `SIMD[DType, width]` as a first-class type
- `vectorize(fn, width, size)` — auto-vectorization primitive
- Compile-time SIMD width selection
- EVL (Effective Vector Length) for masked operations
- Direct hardware SIMD mapping via MLIR

**Path to 30:**
- `fan simd[width]` — SIMD hint in fan blocks
- LLVM backend auto-vectorization (free with Inkwell)
- `int.count_leading_zeros` etc. already map to WASM i64.clz — extend to LLVM intrinsics

### 5. GPU (Almide: 0/100)

**What Mojo has:**
- GPU kernel compilation via MAX Engine
- `@parameter for` on GPU
- Device context abstraction
- CUDA/ROCm targeting

**Path to 15:**
- SPIR-V or NVVM emit from dialect (Stage 3 roadmap item)
- `@schedule(device=gpu)` attribute

### 6. Optimizer (Almide: 55/100)

**What almide has:**
- egg-based equality saturation (algebraic optimization, fusion)
- 30 nanopass passes (TCO, LICM, const fold, borrow inference, etc.)
- Stream/matrix fusion via egg rewrite rules
- Auto-parallelization pass

**What Mojo has:**
- MLIR's full optimization pipeline
- Progressive lowering (high-level → affine → LLVM)
- LLVM's battle-tested optimization passes
- Profile-guided optimization hooks
- Hardware-specific tuning

**Path to 70:**
- Wire dialect → LLVM (done: PoC), then LLVM optimizations are free
- Progressive lowering via melior (Stage 2-3)

### 7. Backend Maturity (Almide: 40/100)

**What almide has:**
- Rust source codegen (mature, 224 tests pass)
- WASM direct emit (mature, cross-target CI)
- LLVM IR via Inkwell (PoC: i64/f64 arithmetic + if/else + recursion)
- Dialect as intermediate layer (SSA, 30+ op kinds)

**What Mojo has:**
- MLIR → LLVM → native binary (production-grade)
- GPU kernel compilation
- Cross-platform support (Linux, macOS)

**Path to 60:**
- LLVM backend: add String/List/Record types
- `almide build --target native` producing standalone binaries
- Stdlib runtime linked as native library (not Rust source)

### 8. Stdlib (Almide: 60/100)

**What almide has:**
- 736 functions across 35 modules
- Comprehensive: string, list, map, set, json, regex, http, fs, bytes, matrix, datetime
- Codec auto-derivation (encode/decode for any type)
- Pipe chain optimization (iterator fusion)

**What Mojo has:**
- Smaller stdlib but deeper in numerics
- `algorithm` module (vectorize, parallelize, reduce, cumsum)
- `layout` / `tensor` for ML workloads
- `bit` module, `hash` module
- `sys` module (hardware info, memory management)

**Almide advantage:** Broader application coverage (HTTP, JSON, regex, datetime)
**Mojo advantage:** Deeper numeric/ML primitives

### 9. Concurrency (Almide: 50/100)

**What almide has:**
- `fan { }` structured concurrency (Rust `rayon`-backed)
- `fan.map`, `fan.race`, `fan.any`, `fan.settle`
- Effect inference categorizes concurrent operations

**What Mojo has:**
- `parallelize(fn, n)` for data parallelism
- GPU parallelism (massive)
- `async fn` support (though limited in Mojo 25.x)

**Almide advantage:** `fan` is more principled than async/await
**Mojo advantage:** GPU-level parallelism

### 10. Effect System (Almide: 80/100)

**What almide has (Mojo doesn't):**
- `fn` = pure, `effect fn` = side effects — compiler-enforced separation
- Effect categories: IO, Net, Env, Time, Rand, Fan, Log
- Auto `?` propagation in effect fns
- Guard statements for early error return

**What Mojo has:**
- `raises` annotation (partial)
- No purity guarantee

**This is almide's strongest axis relative to Mojo.**

### 11. LLM Code Generation (Almide: 100, Mojo: 50)

**What almide has (unique):**
- Language design optimized for LLM accuracy (MSR metric)
- almide-dojo benchmark suite
- Claude Sonnet 4.6: 100% MSR (30/30 tasks)
- Syntax designed to minimize LLM confusion
- Diagnostic messages designed for LLM correction loops

**Mojo:** Not designed for this axis. LLMs can write Mojo but accuracy is not a design goal.

---

## Path to Parity: Priority Actions

### Phase 1: 50/100 (from 38)
1. **Const generics** (`comptime`) — biggest single-axis improvement (+15 on type system, +30 on compile-time)
2. **LLVM native binary** — `almide build --target native` with linked runtime (+15 on backend)
3. **Full test suite on dialect pipeline** — validate the new architecture

### Phase 2: 65/100
4. **SIMD via LLVM auto-vectorization** — free performance from Inkwell (+20 on SIMD)
5. **Protocol inheritance** — `protocol Hashable: Eq` (+5 on type system)
6. **Ownership annotations in syntax** — `owned`, `borrowed` keywords (+10 on memory)
7. **LSP completion** — developer experience (+10 on tooling)

### Phase 3: 80/100
8. **GPU PoC via SPIR-V** — `almide build --target gpu` (+15 on GPU)
9. **Progressive MLIR lowering** — melior adoption (+10 on optimizer)
10. **Package registry** — ecosystem bootstrapping (+5 on ecosystem)

### Unreachable axes (without massive investment)
- **Ecosystem (10→50+)**: Requires community, which requires users, which requires killer app
- **Interop (25→70+)**: Python interop would be transformative but is a massive undertaking
- **GPU (0→80+)**: Full GPU stack requires dedicated hardware expertise

---

## Key Insight

Almide at 38/100 vs Mojo is not as bad as it sounds. Mojo has:
- ~$200M+ funding
- 50+ compiler engineers
- 3+ years of focused development on the MLIR/GPU stack

Almide is a **single-developer project** that has:
- A working compiler with 3 backends
- 736 stdlib functions
- 30 optimization passes
- egg-based algebraic optimizer
- Effect system that Mojo lacks entirely

The gap is largest in **hardware-level features** (SIMD, GPU, memory model) and **ecosystem**. These are addressable with the LLVM backend (done as PoC) and time. The **type system gap** (const generics) is the highest-leverage item to close next.
