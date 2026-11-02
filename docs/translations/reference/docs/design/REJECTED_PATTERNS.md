# Rejected Patterns

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](REJECTED_PATTERNS.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](REJECTED_PATTERNS.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](REJECTED_PATTERNS.zh-CN.md)



> Source: [docs/design/REJECTED_PATTERNS.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/design/REJECTED_PATTERNS.md) (pinned commit `35297a612fb29e3f78f6587aba253aa91d51f466`, Japanese original). This is a non-normative reader snapshot translation, not automatically synchronized. It preserves the source’s dates, scope, and claims; no independent measurements or implementation verification were performed for this translation. Design rationales and historical comparisons—including claims about LLM ability, zero overhead, and optimal execution—are reproduced as source claims, not independently established guarantees. Entries explicitly marked as reversed or currently allowed remain so; this translation does not turn past rejections into current prohibitions.

> Matz: "I should have thought more about the features I took from Perl."
> TypeScript: 10 years of regretting enum and namespace.
>
> Features on this list are **deliberately not adopted**. Read this before proposing them again.

---

## Syntax

### ~~The `??` (nullish coalescing) operator~~ — **Rejection withdrawn (ADR-0005)**

Originally rejected as “a Canonicity violation because `unwrap_or` provides the same functionality.” **ADR-0005 reversed this decision**: `??` is canonical, and its desugaring definition (`a ?? b ≡ unwrap_or_else`) is in [docs/specs/result-option-effect.md §“Operator definitions”](../../../../specs/result-option-effect.md). Instead, the `unwrap_or` function is slated for removal (ADR-0005 D4, almide#1107). Kept as rejection history—the “three spellings” problem is resolved by removing the function, not the operator.

### The `return` keyword

**Reason for rejection**: The implicit last expression is the function's return value. `return` is unnecessary.
- `guard ... else <expr>` expresses early escape structurally.
- Multiple return paths are expressed using `if/then/else` or `match`.

### `s[i]` (string indexing)

**Current API**: Use `string.get(s, i)` to retrieve a character from a string. This does not mean protocol dispatch for existing operators is also forbidden.

### Omitting `{}` in match arms — allowed by current syntax

The old rejection description does not match the current parser. An arm body is an expression, so it can be written as `some(x) => x`. Use `{ ... }` for a block with multiple statements (`crates/almide-syntax/src/parser/compounds.rs::parse_match_arm`).

### The ternary `? :` operator

**Reason for rejection**: `if/then/else` is an expression. A ternary is unnecessary and would require an LLM to learn two conditional-expression syntaxes.

---

## Type system

### null / nil / undefined

**Reason for rejection**: `Option[T]` is the only representation of “a value may be absent.” null produces Go's nil panics and JavaScript's undefined hell.

### implicit type conversion

**Reason for rejection**: Explicit conversion with `int.to_string(n)`, `int.from_saturating(x)`. Implicit conversions cause silent behavioral changes like the `/` operator (int→float) from Python 2→3.

### custom operator syntax — distinct from protocol dispatch

There is no feature for users to define new operator symbols or syntax. Existing operators, however, can dispatch to requirement methods according to declared protocols. For example, `Eq`'s `==` maps to `Type.eq`, and `Numeric`'s `+` to `add`. The old explanation that “all operator overloading is forbidden” does not match the current implementation (`canonicalize/protocols.rs`, `spec/lang/operator_protocol_test.almd`, `spec/stdlib/bigint_operators_test.almd`).

### algebraic effects (an extended effect system)

**Reason for rejection**: Keep `effect fn` as an I/O/side-effect marker. As Gleam succeeded without an effect system, a simple binary classification (pure / effect) is sufficient. The surface syntax `effect[fs]` / `effect[http]` is not adopted. On the other hand, capability category set inference through function values and `[permissions].allow` are implemented ([capability system](../../../../wasm/capability-system.md)).

### Higher-Kinded Types (HKT)

**Reason for rejection**: Complexity that Swift's PAT (Protocol with Associated Types) took 7 years to make ergonomic. There is no evidence that LLMs can use HKT correctly.

### type classes / implicit instances

**Reason for rejection**: Haskell's orphan instances, Scala's implicit resolution—these require global searches and do not fit LLMs' local reasoning well.

### The `trait` / `interface` keywords

**Reason for rejection**: Almide adopts the `protocol` keyword. Reasons:
- `trait` is Rust-specific vocabulary. It evokes associated types, trait objects, and orphan rules, inviting an LLM to import Rust patterns.
- `interface` is widely used in Go/Java/TS, but its meaning is split between Go's implicit satisfaction and Java's explicit `implements`.
- `protocol` is Swift/Python (PEP 544) vocabulary. Almide's existing `: Convention` syntax (`type Dog: Eq`) matches Swift's `: Protocol` syntax.
- It is vocabulary that new languages in 2024–2026 tend to choose.

### `impl` blocks

**Reason for rejection**: Almide implements protocols through convention methods (`fn Type.method()`). An `impl Protocol for Type { ... }` block is unnecessary:
- Convention methods are top-level functions—flat, without nesting.
- Easier for LLMs to generate accurately (no increase in indentation depth).
- Fully consistent with the existing convention-method system.

### Implicit protocol satisfaction (Go-style structural interfaces)

**Reason for rejection**: Go implicitly satisfies an interface when methods exist. Almide requires an explicit declaration, `type Foo: Protocol`:
- An LLM can see the implementation relationship just by reading the code.
- The compiler can report missing methods precisely.
- Consistent with the existing `: Eq, Repr` syntax.

### dynamic dispatch / protocol objects

**Reason for rejection**: Almide protocols are resolved through monomorphization. No vtable or dynamic dispatch is needed:
- Everything expands to concrete types at compile time.
- Zero runtime overhead.
- No boxing like Go interface values is needed.

---

## Runtime and concurrency

### mutable by default

**Reason for rejection**: `let` is immutable, `var` is mutable. Ruby's mutable-by-default led to incompatibility with Ractor. Capturing `var` inside a `fan` block is forbidden.

### async / await keywords

**Reason for rejection**: Concurrency is expressed with `effect fn` + `fan`. async/await brings:
- Python: ecosystem fragmentation from the function-coloring problem.
- Rust: complexity of Pin/Unpin, Send + 'static, and runtime choice.
- Swift: retrofitted after a 7-year delay.

Almide's `fan` expresses only the “intent to run concurrently.” The compiler generates the optimal implementation for each target.

### goroutine / green thread / actor

**Reason for rejection**: Structured concurrency with `fan` is sufficient. Avoids goroutine leaks (Go) and Ractor incompatibility (Ruby). Supervision/Actor is on hold for future consideration.

### Exposing `Future[T]` / `Promise` types

**Reason for rejection**: Do not expose them to users. The return type of `effect fn foo() -> Int` is `Int`, not `Future[Int]`. This prevents LLM confusion with `Future[Future[Int]]`.

---

## Metaprogramming

### macros

**Reason for rejection**: Zig's comptime and Rust's proc macros—powerful, but LLMs cannot generate them correctly. Internal compiler generation and user-written macro syntax are different things. The standard library is currently defined in `stdlib/*.almd`, not the old TOML stdlib definitions.

### monkey patching / open classes

**Reason for rejection**: Ruby's monkey patching brings “developer happiness,” but for an LLM, “methods change somewhere other than their definition” is behavior it cannot analyze.

### reflection / runtime type inspection

**Reason for rejection**: Static types resolve everything at compile time. Runtime type information is unnecessary.

---

## Packages and configuration

### Multiple configuration file formats

**Reason for rejection**: `almide.toml` is the sole configuration. Python took 23 years to go from `setup.py` → `setup.cfg` → `pyproject.toml`.

### Executable build scripts

**Reason for rejection**: `almide.toml` is declarative TOML only. Executable configuration like Python's `setup.py` destroys reproducibility.

---

## Maintaining this list

- **Add**: When considering and rejecting a feature, add it here with the reason.
- **Remove**: Only if the language's direction fundamentally changes (at edition-change scale).
- **Refer**: Point here when a PR or issue proposes “we want this feature.”
