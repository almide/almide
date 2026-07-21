<!-- description: mut parameter modifier for in-place mutation (Swift inout / Mojo mut) -->
<!-- done: 2026-05-19 -->
# `mut` Parameter Modifier

> **Status: Done** — shipped in v0.18.0.
> **Predecessor**: `@mutating(param)` annotation (shipped in 0.17.10) provides the IR
> infrastructure (`IrFunction.mutated_params`). This roadmap item promotes it to a
> first-class language feature.

## The Problem

```almide
@intrinsic("almide_rt_list_push")
@mutating(xs)                          // ← annotation, not enforced by checker
fn push[A](xs: List[A], x: A) -> Unit = _

let frozen = [1, 2, 3]
list.push(frozen, 4)                   // compiles — silent bug at runtime
```

`@mutating(xs)` is an optimization hint for LICM, not a type-system contract.
The checker does not reject passing a `let` binding to a mutating parameter.

## The Solution

```almide
fn push[A](mut xs: List[A], x: A) -> Unit = _

var data: List[Int] = []
list.push(data, 1)       // OK — var binding

let frozen = [1, 2, 3]
list.push(frozen, 4)     // error: cannot pass immutable binding `frozen` to `mut` parameter `xs`
```

`mut` is a parameter modifier (like Swift's `inout`, Mojo's `mut`). It means:
- The function may mutate this argument in place
- The caller must pass a mutable binding (`var`, not `let`)
- The compiler enforces this at check time

## Design Decisions

### Why `mut` and not `inout`

- Mojo started with `inout`, renamed to `mut` in v24.6 for clarity
- Almide already uses `var` for mutable bindings — `mut` is the natural counterpart
- `mut` is one keyword, self-documenting, LLM-friendly

### Semantics: reference, not copy-in/copy-out

Swift's `inout` uses copy-in/copy-out (may optimize to reference). Almide's `mut`
is a mutable reference — same address, no copy. This matches Almide's RcCow value
semantics: mutation triggers COW, so aliasing is safe without a borrow checker.

### No borrow checker needed

Almide's value types use RcCow (Swift-style COW). Multiple references to the same
list are safe because mutation triggers `Rc::make_mut` (clone-on-write). This means
`mut` parameters don't need Rust-style exclusivity checking — just mutability checking.

### Call-site syntax: implicit

```almide
list.push(data, 1)    // no & needed — checker knows push takes mut xs
```

Unlike Rust (`&mut x`) or Swift (`&x`), the caller does not need special syntax.
The function signature declares the contract; the checker enforces it. This keeps
call sites clean for LLM generation accuracy.

## Implementation Plan

### Phase 1: Parser + AST

- Add `mut` as a parameter modifier in `parser/declarations.rs`
- `ast::Param` gets `is_mut: bool`
- `@mutating(xs)` desugars to `mut xs` in lowering (backward compat)

### Phase 2: Checker

- In `check/calls.rs`, when resolving a call to a function with `mut` param:
  - The argument must be a `var` binding (not `let`, not a temporary)
  - Error: "cannot pass immutable binding `name` to `mut` parameter `param`"
- In `check/infer.rs`, propagate `mut` through UFCS / pipe desugaring

### Phase 3: Lowering

- `mut` param → `IrFunction.mutated_params` (already exists from 0.17.10)
- `@mutating(xs)` is syntactic sugar, emits same IR

### Phase 4: Codegen (Rust target)

- `mut` param → `&mut T` in generated Rust function signature
- Call sites pass `&mut var` automatically
- Removes need for `@inline_rust("...&mut {xs}...")` pattern

### Phase 5: Deprecate `@mutating`

- Warn on `@mutating(xs)` — suggest `mut xs` instead
- Remove after one minor version

## Migration

```almide
// Before (0.17.10)
@intrinsic("almide_rt_list_push")
@mutating(xs)
fn push[A](xs: List[A], x: A) -> Unit = _

// After
@intrinsic("almide_rt_list_push")
fn push[A](mut xs: List[A], x: A) -> Unit = _
```

## References

- [Mojo 24.6: inout → mut rename](https://www.modular.com/blog/hands-on-with-mojo-24-6)
- [Swift SE-0377: Parameter Ownership Modifiers](https://github.com/swiftlang/swift-evolution/blob/main/proposals/0377-parameter-ownership-modifiers.md)
- [Rust References and Borrowing](https://doc.rust-lang.org/book/ch04-02-references-and-borrowing.html)
