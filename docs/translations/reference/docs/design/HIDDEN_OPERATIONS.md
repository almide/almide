# Hidden Operations

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](HIDDEN_OPERATIONS.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](HIDDEN_OPERATIONS.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](HIDDEN_OPERATIONS.zh-CN.md)

> Non-normative reader translation of the [pinned original](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/design/HIDDEN_OPERATIONS.md); the original remains authoritative. This copy is not automatically synchronized.




> Zig's principle: "no hidden control flow, no hidden memory allocations"
> Almide intentionally hides some operations. All of them are documented here.

---

## 1. Automatic clone insertion (Rust target)

### What happens

When a variable of a non-Copy type (String, List, Map, Record, Variant) is used more than once, the compiler automatically inserts `.clone()`.

### Conditions

- `use_count > 1` — IR use-count analysis counts uses of the variable.
- `!is_copy` — no insertion for Copy types such as Int, Float, Bool.
- No clone on the final use (the value is moved).

### Files

- `crates/almide-codegen/src/pass_clone.rs` — clone insertion pass
- `crates/almide-ir/src/use_count.rs` — use-count analysis

### Impact on users

- Performance: unnecessary clones may be inserted (a correctness-first design).
- Optimization: borrow inference (`pass_borrow_inference*.rs`) reduces clones (without changing the behavior of existing code). Alias analysis on the v1 spine side is in `crates/almide-mir/src/alias_safety.rs`.

---

## 2. Error propagation — **not hidden** (auto-`?` has been retired)

This section once documented auto-`?` insertion (implicitly attaching `?` to fallible calls inside an `effect fn`). **Retired by ADR-0008.** Propagation has a single spelling: only postfix `!` lowers to `?`.

```almide
effect fn load() -> String = {
  let text = fs.read_text("file.txt")   // E041 — Result 値のまま、伝搬しない
  let text = fs.read_text("file.txt")!  // これが `?` に落ちる
  text
}
```

A fallible call without `!` is a Result **value**, not control flow:

- A binding without a type annotation → **E041** (`let x = f()`)
- Discarding in statement position → **E042** (must-use: `f()!` or `let _ = f()`)
- The `list.try_*` spelling → **E043** (the callback's `!` is the strategy)

### Files

- `crates/almide-frontend/src/lower/auto_try.rs` — the name is historical. It now inserts `Try` driven by the `!` marker; it does not perform implicit insertion.
- `crates/almide-codegen/src/pass_result_propagation.rs` — signature lifting and reflecting `Try` at call sites
- Detailed specifications: [specs/effect-fn-call-semantics.md](../../../../specs/effect-fn-call-semantics.md), [ADR-0008](../../../../adr/0008-explicit-propagation-only.md)

---

## 3. Runtime embedding

### What happens

The Almide runtime needed by the generated code is embedded. Depending on the features used, external Rust crates or host capabilities may also be required.

### Rust target

The contents of `runtime/rs/src/*.rs` are embedded in the generated `.rs` file (`crates/almide-codegen/build.rs` generates `generated/rust_runtime.rs`).

Included:
- `almide_eq!` / `almide_ne!` macros (deep equality comparison)
- `AlmideConcat` trait (String + List concatenation)
- Stdlib runtime functions declared with `@intrinsic`

### WASM target

The stdlib's self-hosted Almide implementations are linked with user code, and the structural emitter in `almide-wasm` produces Wasm bytes directly. The old MIR→WAT renderer and handwritten WAT preamble route have been retired. New self-hosted bodies are declared with `@implements` and reflected in the generated registry. Host operations such as I/O require import implementations for the chosen execution route. Details: [WASM-OUTPUT.md](../../../../wasm/WASM-OUTPUT.md).

---

## 4. Perceus RC insertion (native MIR / structural Wasm)

### What happens

The v1 MIR (`crates/almide-mir`) automatically inserts reference-counting operations (dup / drop) and uniqueness checks (MakeUnique) for in-place mutation, using the Perceus approach. This MIR is the route for the native renderer and the MIR witness producer; current Wasm uses a separate structural emitter to handle RC/layout.

### Impact on users

- Preserving language semantics is a requirement (native / wasm / interp differential gates check the covered corpus).
- Only performance characteristics are affected. Optimizations such as MakeUnique elision are strengthened with each release.
- Proof: `crates/almide-perceus-belt/` (Lean 4) verifies the RC discipline.

---

## 5. Parallelizing fan (Rust target)

### What happens

On native, `fan { a(), b() }` eligible for parallel execution runs using `std::thread::scope` + `spawn`. Blocks containing `@ordered` operations, for example, execute in arm order, so not every `fan` necessarily uses one thread per arm. The following sketches the parallel route.

```rust
std::thread::scope(|__s| -> Result<_, String> {
    let __fan_h0 = __s.spawn(move || { a() });
    let __fan_h1 = __s.spawn(move || { b() });
    Ok((__fan_h0.join().unwrap()?, __fan_h1.join().unwrap()?))
})?
```

### Impact on users

- External variables are captured by `move` (possibly cloned).
- Capturing `var` is a compile error (E008).
- The compiler/host determines the execution infrastructure and eligibility for parallel execution. Ordinary `fan` joins all siblings and returns the first error in arm order.

### Files

- `crates/almide-codegen/src/pass_fan_lowering.rs`, `pass_rust_lowering_fan.rs` — fan lowering and native parallel routing

---

## Operations that are not hidden

| Operation | Language expression | Why it is not hidden |
|------|-------------|-------------|
| I/O | `effect fn` | Explicit in the type signature |
| Error propagation | `Result[T, E]` | Visible in the type |
| Mutability | `var` vs `let` | Explicit keywords |
| Parallelization | `fan { }` | Explicit syntax |
