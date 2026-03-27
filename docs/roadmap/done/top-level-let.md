<!-- description: Allow let bindings at module scope for constant values -->
# Top-Level Let

## The Problem

Constant values require zero-argument functions as a workaround:

```almide
fn pi() -> Float = 3.14159265358979323846
fn solar_mass() -> Float = 4.0 * pi() * pi()
fn days_per_year() -> Float = 365.24
```

This works — LLVM inlines everything — but:
1. **`()` on every use** — `solar_mass()` not `solar_mass`. Forgetting `()` is a top-5 LLM error
2. **`fn` implies computation** — LLMs sometimes pass these as higher-order functions
3. **8+ tokens per declaration** — `fn X() -> T = v` vs `let X = v` (3 tokens)

## Design

Allow `let` at module scope, restricted to constant expressions. No new keyword — same `let` that already exists inside functions.

```almide
let PI = 3.14159265358979323846
let SOLAR_MASS = 4.0 * PI * PI
let DAYS_PER_YEAR = 365.24

effect fn main() -> Unit = {
  let mass = 9.547e-04 * SOLAR_MASS   // no () needed
}
```

Inspired by Swift, which uses `let` uniformly for both local bindings and module-level constants.

### Rules

- `let` at module scope declares a compile-time constant
- Value must be a **constant expression**:
  - Literals: `0`, `3.14`, `"hello"`, `true`, `false`, `none`
  - Empty collections: `[]`
  - References to other top-level `let` values (declared earlier)
  - Arithmetic on constants: `4.0 * PI * PI`
  - String concatenation on constants: `PREFIX ++ ".almide"`
  - Unary negation: `-1`, `-3.14`
- **Not allowed**: function calls, effect expressions, runtime values, `if`, `some()`, `ok()`
- Type inferred from expression; optional annotation: `let PI: Float = 3.14`
- Forward references prohibited — must reference values declared earlier in the same module
- `pub let` for cross-module access (same as `pub fn`)
- UPPER_SNAKE_CASE by convention (not enforced)

### Why not `const`

- `let` is already in the language — zero new concepts for LLMs
- No "should I use `const` or `fn`?" decision
- Swift proves this works: one keyword for both local bindings and module constants
- `const` adds a keyword that solves only the `()` problem — the cost/benefit doesn't justify it

### Why not unrestricted top-level `let`

```almide
let data = fs.read_text("config.json")  // ← this must NOT be allowed
```

- Evaluation order becomes ambiguous ("when does this run?")
- Opens the door to top-level side effects
- Rust codegen would require `lazy_static` / `static` — complexity for no gain

Restricting to constant expressions eliminates all these issues. The compiler can evaluate or inline everything at compile time.

### Constant expressions: what's allowed

| Expression | Allowed | Example |
|-----------|---------|---------|
| Numeric literal | Yes | `42`, `3.14` |
| String literal | Yes | `"hello"` |
| Bool literal | Yes | `true`, `false` |
| `none` | Yes | `none` |
| Empty list | Yes | `[]` |
| Unary negation | Yes | `-1`, `-3.14` |
| Arithmetic on constants | Yes | `4.0 * PI * PI` |
| String concat on constants | Yes | `PREFIX ++ ".almide"` |
| Reference to earlier top-level `let` | Yes | `SOLAR_MASS` (if declared above) |
| Function call | **No** | `math.sqrt(2.0)` |
| Variable reference (local) | **No** | `some_var` |
| `some(x)` / `ok(x)` | **No** | wrapping requires runtime context |
| `if`/`match` | **No** | keep it simple |

## Codegen

### Rust
```rust
// let PI = 3.14  →
const PI: f64 = 3.14;

// let SOLAR_MASS = 4.0 * PI * PI  →
const SOLAR_MASS: f64 = 4.0 * PI * PI;
```

Direct mapping to Rust `const`. Rust's const evaluator handles arithmetic.

### TypeScript
```typescript
// let PI = 3.14  →
const PI = 3.14;

// let SOLAR_MASS = 4.0 * PI * PI  →
const SOLAR_MASS = 4.0 * PI * PI;
```

Direct mapping to JS `const`.

## Impact

### N-body benchmark (before → after)

```almide
// Before: 7 fn declarations, () on every use
fn pi() -> Float = 3.14159265358979323846
fn solar_mass() -> Float = 4.0 * pi() * pi()
fn days_per_year() -> Float = 365.24

let j_mass = 9.54791938424326609e-04 * solar_mass()
let j_vx = -2.76742510726862411e-03 * days_per_year()

// After: clean, no ()
let PI = 3.14159265358979323846
let SOLAR_MASS = 4.0 * PI * PI
let DAYS_PER_YEAR = 365.24

let j_mass = 9.54791938424326609e-04 * SOLAR_MASS
let j_vx = -2.76742510726862411e-03 * DAYS_PER_YEAR
```

### LLM accuracy

| Metric | `fn` workaround | top-level `let` |
|--------|----------------|-----------------|
| Tokens per declaration | 8+ (`fn X() -> T = v`) | 3 (`let X = v`) |
| Tokens per use | 2 (`X()`) | 1 (`X`) |
| Parenthesis errors | Common | Impossible |
| New concepts for LLM | None | None |
| Keyword to learn | None (already knows `fn`) | None (already knows `let`) |

## Tasks

- [x] AST: add `Decl::TopLet { name, ty: Option<TypeExpr>, value: Expr, visibility, span }`
- [x] Parser: parse `let NAME = expr` and `pub let NAME: Type = expr` at module scope
- [x] Checker: validate value is a constant expression
- [x] Checker: register top-level `let` in scope (accessible as a value, not a function)
- [x] Checker: resolve references to earlier top-level `let` values
- [x] Emit Rust: `const NAME: type = value;` at module level
- [x] Emit TS: `const NAME = value;` at module level
- [x] Formatter: preserve top-level `let` declarations
- [x] Tests: basic top-level let, const arithmetic, cross-reference, type errors, non-const rejection
- [x] Update n-body benchmark to use top-level `let`
