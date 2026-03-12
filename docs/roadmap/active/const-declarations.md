# Const Declarations [ACTIVE]

## The Problem

Self-tooling and benchmarks exposed a recurring pattern: constant values expressed as zero-argument functions.

```almide
fn PI() -> Float = 3.141592653589793
fn SOLAR_MASS() -> Float = 4.0 * PI() * PI()
fn DAYS_PER_YEAR() -> Float = 365.24
```

This works — LLVM inlines everything — but it's dishonest. These aren't functions. They take no arguments, have no effects, and always return the same value. The `fn` keyword signals "computation" when the reality is "name for a value."

### Why this matters for LLMs

1. **LLMs must remember `()` on every use** — `SOLAR_MASS` vs `SOLAR_MASS()`. Forgetting parentheses is a top-5 LLM error pattern across all languages
2. **Type signature is noise** — `-> Float =` adds tokens with zero information (the type is obvious from the literal)
3. **`fn` implies callable** — LLMs sometimes try to pass these as higher-order functions (`list.map(items, SOLAR_MASS)`) because they look like functions

### Why NOT top-level `let`

```almide
// ❌ Top-level let — rejected
let PI = 3.141592653589793
let sm = solar_mass()  // when is this evaluated? can it fail?
```

- **Evaluation order is ambiguous** — "when does this run?" is not obvious (lazy? eager? module load?)
- **Opens the door to top-level effects** — `let data = fs.read_text("config.json")` at module scope is a footgun
- **Two ways to bind** — `let` inside functions and `let` at top level look the same but behave differently
- **LLMs must learn scoping rules** — "can I reference another top-level let? in what order?" becomes a new error source

Top-level `let` is the wrong abstraction. The real need is narrower: **naming compile-time constants.**

## Design

```almide
const PI = 3.141592653589793
const DAYS_PER_YEAR = 365.24
const SOLAR_MASS = 4.0 * PI * PI
```

### Rules

- `const` declares a module-level named constant
- No parentheses on use: `SOLAR_MASS`, not `SOLAR_MASS()`
- Value must be a compile-time constant expression (same restriction as default field values):
  - Literals: `0`, `3.14`, `"hello"`, `true`, `false`, `none`
  - Empty collections: `[]`
  - Arithmetic on other `const` values: `4.0 * PI * PI`
  - Unary negation: `-1`
- No function calls, no effects, no runtime values
- Type is inferred from the expression (no annotation needed)
- Optional type annotation for clarity: `const PI: Float = 3.14`
- Constants are always visible within the module; `pub const` for cross-module access
- UPPER_SNAKE_CASE by convention (not enforced — LLMs already default to it)

### What the code becomes

Before:
```almide
fn solar_mass() -> Float = 4.0 * 3.141592653589793 * 3.141592653589793

effect fn main() -> Unit = {
  let sm = solar_mass()
  let j_m = 9.54791938424326609e-04 * sm
  // ... use sm everywhere in hot loop
}
```

After:
```almide
const PI = 3.141592653589793
const SOLAR_MASS = 4.0 * PI * PI
const DAYS_PER_YEAR = 365.24

effect fn main() -> Unit = {
  let j_m = 9.54791938424326609e-04 * SOLAR_MASS
  // SOLAR_MASS usable directly — no () needed, no local caching needed
}
```

### Const expressions: what's allowed

| Expression | Allowed | Example |
|-----------|---------|---------|
| Numeric literal | Yes | `42`, `3.14` |
| String literal | Yes | `"hello"` |
| Bool literal | Yes | `true`, `false` |
| `none` | Yes | `none` |
| Empty list | Yes | `[]` |
| Unary negation | Yes | `-1`, `-3.14` |
| Binary arithmetic on consts | Yes | `4.0 * PI * PI` |
| String concat on consts | Yes | `PREFIX ++ ".almide"` |
| Function call | **No** | `math.sqrt(2.0)` |
| Variable reference | **No** | `some_var` |
| `some(x)` / `ok(x)` | **No** | wrapping requires runtime dispatch |

### Why allow `const` arithmetic

`SOLAR_MASS = 4.0 * PI * PI` is the single most common pattern in scientific/benchmark code. Forbidding it forces either:
- Hardcoding `39.4784176...` (loses intent, LLMs can't verify)
- Using `fn` workaround (back to the original problem)

The compiler evaluates `const` expressions at compile time (constant folding). No runtime cost, no evaluation order ambiguity.

## Semantics

- Constants are evaluated at compile time by the checker
- The evaluated value is inlined at every use site
- No runtime allocation, no function call overhead
- Codegen:
  - **Rust**: `const PI: f64 = 3.141592653589793;` (or inlined literal)
  - **TS**: `const PI = 3.141592653589793;` (module-level `const`)
- Constants can reference other constants declared earlier in the same module
- Cross-module: `import foo` then `foo.PI` (same as current function access)

## Impact on LLM accuracy

| Metric | `fn` workaround | `const` |
|--------|----------------|---------|
| Tokens per constant declaration | 8+ (`fn X() -> T = v`) | 3 (`const X = v`) |
| Tokens per use | 2 (`X()`) | 1 (`X`) |
| Parenthesis-forgetting errors | Common | Impossible |
| Callable confusion | Possible | Impossible |
| Evaluation order ambiguity | None (function) | None (compile-time) |

For the n-body benchmark alone, `const` eliminates 12 tokens in declarations and ~40 parentheses in the hot loop.

## Restrictions (explicit non-goals)

- **No `const fn`** — function-level constants remain `let`. No new concept needed.
- **No lazy evaluation** — consts are always compile-time. No `lazy val` or `static`.
- **No mutable statics** — `var` at top level is never allowed.
- **No complex expressions** — `const X = if ... then ... else ...` is not allowed. Keep it simple.

## Tasks

- [ ] AST: add `Decl::Const { name, ty: Option<TypeExpr>, value: Expr, visibility }`
- [ ] Parser: parse `const NAME = expr` and `pub const NAME: Type = expr`
- [ ] Checker: evaluate const expression at compile time, validate compile-time-constant restriction
- [ ] Checker: register constants in scope (accessible without `()`)
- [ ] Checker: allow const-to-const references (ordered)
- [ ] Emit Rust: `const NAME: type = value;` at module level
- [ ] Emit TS: `const NAME = value;` at module level
- [ ] Formatter: preserve `const` declarations
- [ ] Tests: basic consts, const arithmetic, cross-const references, type errors, non-const rejection
