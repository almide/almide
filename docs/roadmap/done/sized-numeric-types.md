<!-- description: Swift-style Int8/Int32/UInt32/Float32 scalar types; unblocks bytes redesign + Matrix[T] dtype -->
<!-- done: 2026-04-19 -->
# Sized Numeric Types

## Completion status (2026-04-19)

Arc delivered across Stages 1 → 3 plus Stage 4a/4b (bytes typed IO). Closed in three merge points:

- **Stage 1/2** (parser + type system + codegen) landed earlier in the arc (`project_sized_numeric_types.md` memory).
- **Stage 3 (Conversion UFCS)** landed with the Stdlib Unification push (3b00b1ba / 49d341f6) and completed by the safety-variant pass on 2026-04-19:
  - Narrowing conversions: `int.to_<T>` (truncating) + `int.from_<T>` (widening) shipped as `@intrinsic` for all 8 sized types (Int8-64, UInt8-64, Float32-64).
  - Safety variants on top: `int.to_<T>_checked -> Option[T]` and `int.to_<T>_saturating -> T` for 7 narrow int targets + `to_float32`; mirror set on `float.to_<T>_checked` / `_saturating` for 8 int targets + `to_float32`.
  - `_checked` follows Swift `Int(exactly:)` semantics: NaN / infinity / out-of-range / fractional parts all return `None`. Round-trip via `int.to_float` detects the fractional case.
  - `_saturating` clamps to `T::MIN` / `T::MAX`; NaN → 0 on float sources.
  - 22 new spec tests (`spec/stdlib/sized_conversion_safety_test.almd`).
- **Stage 4a/4b (bytes typed IO)** landed in the Endian-dispatch migration (3b00b1ba): `bytes.{read,write,set}_{uint16,uint32,int32,float32}` as bundled Almide bodies pivoting through canonical `int` / `float`. `bytes.almd` has no width×endian name explosion beyond the `_le` / `_be` runtime primitives it composes over.
- **WASM bug fix caught during the arc**: `emit_store_at` / `emit_load_at` were missing `Float32` (F32) and narrow int (Int8/16, UInt8/16) variants, causing Option[sized-type] construction to emit unbalanced stack sequences. Fixed — all sized types now round-trip through the heap layout on both targets.

### What remains (deferred, tracked as future work)

- **Generic `bytes.read[T: FixedWidthInteger]`** (Stage 4 full vision) requires a protocol / type-bound system. Currently `bytes.read_uint16` / `read_int32` / `read_float32` are discrete fns; the generic single-entry surface awaits the protocol arc.
- **`Matrix[T]` dtype parameterization** — landed separately in the Matrix arc (see `project_matrix_dtype_design` memory).
- **`FixedWidthInteger` / `BinaryFloatingPoint` protocols** — out of scope; belongs in a distinct protocol / trait system arc.

## Motivation

Almide today has exactly two numeric scalars: `Int` (i64) and
`Float` (f64). Every operation that cares about bit-width or
signedness has to encode that information in **function names**.
The clearest symptom is `stdlib/defs/bytes.toml`:

```
read_u32_le / read_u32_be / read_i32_le / read_i32_be / ...
```

Width × endianness × operation (read / write / set / append) =
**82 out of 126 fns** in bytes — a combinatorial explosion that
no type-system-free language can collapse.

Secondary pressures:

- `Matrix[T]` dtype arc (`docs/roadmap/active/matrix-dtype.md`,
  `project_matrix_dtype_design` memory) assumes a meaningful
  element-type parameter. Without Int32 / Float32 / etc. the arc
  has no reasonable `T`.
- MSR: LLMs fluent in Rust / Swift / Go / C all expect
  `Int32` / `UInt8` / `Float32`. The current "only Int64" surface
  forces hallucination-prone workarounds (`n & 0xFFFFFFFF`
  masking, manual endian swaps).
- Performance: `f32` arrays take half the bytes of `f64`; without a
  distinct type the compiler cannot choose the tighter layout.

## Decision

Adopt the **Swift numeric type model**:

```
Int8, Int16, Int32, Int64     // signed, two's complement
UInt8, UInt16, UInt32, UInt64 // unsigned, wrapping on overflow disabled by default
Float32, Float64
```

Plus the platform-independent aliases (so existing code is
untouched):

- `Int` = `Int64`
- `Float` = `Float64`

Swift's model is proven, LLM-familiar (Swift has millions of
training examples with exactly these names), and carries no
legacy baggage (unlike Rust's `usize` / `isize` which would pull
in platform-dependent sizing).

## Type rules

### Literals

Integer and float literals stay untyped at the lexer / AST level.
The checker infers the concrete type from surrounding context:

```almide
let a: Int32 = 42        // 42 coerces to Int32
let b: UInt8 = 0xff      // 0xff coerces to UInt8; out-of-range is an error
let c = 42               // no context → Int (Int64)
bytes.read_u32(buf, 0)   // → UInt32; no annotation needed
```

Out-of-range literals under a concrete type are a compile error
(`UInt8 = 300` → `E-width-overflow`).

### Arithmetic

Same-type binary ops return the same type:

```almide
let x: Int32 = 10
let y: Int32 = 20
let z = x + y    // z: Int32
```

Mixed-width ops are an error. Explicit conversion is required:

```almide
let a: Int32 = 10
let b: Int64 = 20
a + b                    // ✗ type error
a.to_int64() + b         // ✓ Int64
```

No implicit widening. No implicit narrowing. This matches Swift
and avoids silent precision loss.

### Protocol membership

- `FixedWidthInteger`: Int8, Int16, Int32, Int64, UInt8, UInt16,
  UInt32, UInt64. Exposes `.bits`, `.min`, `.max`, `.bit_and`,
  `.bit_or`, etc.
- `BinaryFloatingPoint`: Float32, Float64. Exposes `.infinity`,
  `.is_nan`, `.sqrt`, etc.
- `Numeric` (superset): all of the above.

These let generic stdlib code like `bytes.read[T: FixedWidthInteger]`
stay sane.

### Conversion

Explicit only, via UFCS methods that the checker registers per
type:

```almide
n.to_int32()       // Int → Int32 (panics on out-of-range? or returns Option?)
n.to_int32_lossy() // truncating
n.to_uint8()
f.to_int64()       // Float64 → Int64, truncates toward zero
i.to_float32()     // i32 / i64 → f32 (may lose precision)
```

Decision point: **do we panic, return Option, or saturate on
out-of-range?** Leaning toward Option (`to_int32() -> Option[Int32]`)
for safety + `to_int32_wrapping()` / `to_int32_saturating()` when
explicit.

## Codegen

### Rust target (direct mapping)

| Almide | Rust |
|---|---|
| `Int8` / `Int16` / `Int32` / `Int64` | `i8` / `i16` / `i32` / `i64` |
| `UInt8` / `UInt16` / `UInt32` / `UInt64` | `u8` / `u16` / `u32` / `u64` |
| `Float32` / `Float64` | `f32` / `f64` |

Runtime fn signatures get the precise Rust type instead of the
current `i64` catch-all. Example:

```rust
// Before
pub fn almide_rt_bytes_read_u32_le(b: &[u8], offset: i64) -> i64 { ... }

// After
pub fn almide_rt_bytes_read_u32_le(b: &[u8], offset: i64) -> u32 { ... }
```

### WASM target (lowered)

WASM has only four native value types: `i32`, `i64`, `f32`, `f64`.
Narrower Almide types compile to the next-wider WASM type with
masking on stores:

| Almide | WASM repr | Notes |
|---|---|---|
| `Int8` | `i32` | load `i8_load_s`, store with sign-extend fitness |
| `UInt8` | `i32` | load `i8_load_u`, store with `& 0xff` |
| `Int16` / `UInt16` | `i32` | similar with i16 loads |
| `Int32` / `UInt32` / `Int64` / `UInt64` | `i32` / `i64` | native |
| `Float32` / `Float64` | `f32` / `f64` | native |

Well-known territory; both emscripten and `rustc --target wasm32`
handle this trivially.

## Stdlib integration

### bytes (primary beneficiary)

Post-arc surface for byte IO:

```almide
// Generic read/write — collapses ~70 fns into 4
fn read[T: FixedWidthInteger](b: Bytes, offset: Int, endian: Endian) -> T
fn write[T: FixedWidthInteger](buf: Bytes, value: T, endian: Endian) -> Unit
fn set[T: FixedWidthInteger](buf: Bytes, offset: Int, value: T, endian: Endian) -> Unit
fn append[T: FixedWidthInteger](buf: Bytes, value: T, endian: Endian) -> Unit
```

Plus the float counterparts:

```almide
fn read_float[T: BinaryFloatingPoint](b: Bytes, offset: Int, endian: Endian) -> T
// ... etc
```

Total bytes surface: **~30–40 fns** instead of 126.

### int / float modules

Stay as `Int` (= `Int64`) and `Float` (= `Float64`) first-class
modules. Add typed variants as method-style UFCS:

```almide
let x: Int32 = 42
x.abs()            // Int32 → Int32
x.to_int64()       // Int32 → Int64
x.to_string()      // Int32 → String
```

Each sized type gets its own small stdlib-like module (`int32.*`,
`uint8.*`, etc.) — likely auto-generated from a single declarative
source so we don't re-create the 82-fn explosion on the stdlib
side.

### Matrix[T]

Becomes type-parametric on a `FixedWidthInteger | BinaryFloatingPoint`
bound. `Matrix[Float32]`, `Matrix[Int32]`, etc. Dtype arc gets
its `T` foundation.

## Sub-phases

### Stage 1 — Parser + AST + type system (2 weeks)

- Parser: accept `Int8` / `Int16` / ... / `Float32` / `Float64` as
  new Simple type names.
- AST: no change (they're already `TypeExpr::Simple { name }`).
- `almide-types::Ty`: add `Ty::Int8`, `Ty::Int16`, ..., `Ty::UInt64`,
  `Ty::Float32`. (`Ty::Int` stays as `Int64` alias.)
- Checker:
  - Literal coercion: context-directed, range-checked.
  - Arithmetic: same-type required.
  - `FixedWidthInteger` / `BinaryFloatingPoint` protocols as
    built-in.
- Stdlib: `Int` = `Int64` alias, `Float` = `Float64` alias.
  Round-trip: every existing `.almd` program compiles with no
  change.

### Stage 2 — Codegen (1 week)

- Rust: emit `i32` / `u32` / `f32` etc. Walker `render_type`
  gets new arms.
- WASM: lower `Int8` / `UInt16` etc. to `i32` with mask-on-store
  for narrow writes. `values.rs::ty_to_valtype` extended.
- `BinOp` type dispatch: per-arithmetic operator × per-type
  variant (`AddInt32`, `AddUInt32`, ...). The existing matrix of
  `AddInt` / `AddFloat` extends along a new dimension.

### Stage 3 — Conversion UFCS (1 week)

- `n.to_int32() -> Option[Int32]`, `.to_int32_wrapping()`,
  `.to_int32_saturating()`.
- Float ↔ Int conversions with lossy / checked variants.
- Documentation: `docs/specs/type-system.md` gets a sized-
  numeric section.

### Stage 4 — Stdlib redesign (2-3 weeks)

- `stdlib/bytes.almd` rewritten with generic `read[T]` / `write[T]`
  primitives. The 82 width×endian×op primitives become internal
  `almide_rt_bytes_*` calls dispatched from the generic fn.
- `stdlib/int32.almd`, `stdlib/uint8.almd`, etc. — small modules
  per sized type, auto-generated or handwritten.
- Matrix[T] dtype arc picks up from here.

## Non-goals

- **BigInt / arbitrary precision integers.** Keep the numeric
  tower finite. If users need arbitrary precision they can
  depend on a package.
- **Float16 / Bfloat16.** No native support in Rust stable or WASM;
  revisit when the broader toolchain does.
- **Decimal / fixed-point.** Domain-specific; belongs in a
  package, not the stdlib numeric core.
- **Platform-dependent widths** (`usize`, `isize`). Almide's
  ABI-agnostic design doesn't admit platform variance in the type
  system.

## Risks

- **Existing code regressions from literal inference.** Must keep
  `Int` / `Float` as aliases so `let x = 42` still gives Int64.
  Any change there breaks every program.
- **Operator dispatch combinatorics.** `BinOp` already has
  `AddInt` / `AddFloat` / `AddMatrix` — adding `AddInt8`
  ... `AddUInt64` × each op is ~10 × 8 = 80 new variants. The
  cleaner approach is a single `Add { ty: Ty }` operator, which
  is itself a sub-arc (operator representation refactor).
- **WASM Int64 / UInt64 ops.** WASM has i64 but signed/unsigned
  distinction is in the INSTRUCTION, not the TYPE. We'll encode
  signedness in the op selection.
- **LLM confusion between `Int` and `Int64`.** Docs must be
  explicit: `Int` is a typedef, writing `Int` and `Int64`
  produces identical code.

## Dependencies

- **Blocker for**: `bytes` migration (Stdlib Unification Stage 2c),
  Matrix[T] dtype arc, any future SIMD work.
- **Blocked by**: none. The existing type system + codegen
  architecture already supports adding Ty variants; this arc is
  pure extension.

## Success criteria

- `let x: UInt32 = 0xffffffff` compiles, runs, round-trips through
  WASM.
- `bytes.read[UInt32](buf, 0, .le)` works; the `stdlib/bytes.almd`
  file is under 40 fns.
- `Matrix[Float32]` type-checks; element accessors return Float32.
- MSR baseline: 207+ `.almd` test files pass unchanged (aliases
  ensure zero regression).
- Dojo MSR improvement: LLM hallucination rate on bit-width
  questions ("how do I read a uint32 from bytes") decreases.

## Total scope

**6-7 weeks of focused work**, split across 4 sub-phases. Each
sub-phase is independently mergeable (opt-in new types, existing
code untouched until Stage 4 integrates them).

## Relationship to other arcs

| Arc | Relation |
|---|---|
| `stdlib-declarative-unification.md` Stage 2c (bytes) | **depends on this** — migrate bytes with new generic API |
| `matrix-dtype-design.md` (memory) | **depends on this** — `Matrix[T]` uses sized types as T |
| `mlir-backend-adoption.md` | **enables** — MLIR dialect can tier types naturally once Almide has them |
| `llm-first-language.md` | **aligned** — Swift-style naming is a known good for LLMs |
