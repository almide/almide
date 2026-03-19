# Compiler Bugs and Gaps Found During Test Coverage Push

Discovered while writing 386+ new test blocks across 32 test files (v0.8.4).

## Codegen Bugs

### Record spread generates broken Rust
- `Config { ...base, debug: true }` → generates `c.clone()` instead of proper spread
- **Impact**: Spread syntax is parsed and type-checked but produces incorrect Rust output
- **Workaround**: Manual field copy `Config { host: base.host, port: base.port, debug: true }`

### result.is_ok / result.is_err arg decoration
- UFCS `r.is_ok()` generates `almide_rt_result_is_ok(r)` but runtime expects `&Result`
- **Impact**: Can't use UFCS for is_ok/is_err on Result values
- **Workaround**: Use pattern matching `match r { ok(_) => true err(_) => false }`

### math.pow signature/runtime mismatch
- Checker signature: `(Int, Int) -> Int`
- Runtime function: `(f64, f64) -> f64`
- **Impact**: `math.pow(2, 10)` type-checks but fails to compile
- **Workaround**: Use `math.fpow(2.0, 10.0)` for float exponentiation

### value.as_int / value.as_string / value.as_bool arg decoration
- UFCS on Value extractors generates incorrect borrow patterns
- **Impact**: Can't extract typed values from Value type via UFCS
- **Workaround**: Use `json.stringify` and parse the output, or `match` on Value

### error.message / error.context arg decoration
- Same borrow pattern issue as value extractors
- **Impact**: Can't use error module functions via UFCS
- **Workaround**: Pattern match on Result to extract error strings

## Missing Runtime Functions

| Function | Checker | Runtime | Status |
|---|---|---|---|
| `result.flat_map` | Has signature | No runtime fn | Missing |
| `result.unwrap_or_else` | Has signature | No runtime fn | Missing |
| `result.to_option` | Has signature | No runtime fn | Missing |
| `result.to_err_option` | Has signature | No runtime fn | Missing |
| `float.is_nan` | Has signature | No runtime fn | Missing |
| `float.is_infinite` | Has signature | No runtime fn | Missing |
| `json.null` | Has signature | No runtime fn | Missing |

## Parser/Language Gaps

### unsafe block not implemented
- `unsafe` is a reserved keyword (lexer) but not parsed as an expression
- Parser rejects `unsafe { ... }` with "Expected expression"

### newtype constructor not implemented
- `type UserId = newtype Int` creates a type alias
- `UserId(42)` constructor syntax doesn't work — generates invalid Rust
- **Workaround**: Use as type alias only (`let id: UserId = 42`)

### trait/impl block methods not registered
- `trait T { fn f... }` + `impl T for X { fn f... }` is parsed
- But impl methods are NOT accessible via `X.f()` or UFCS
- **Workaround**: Use convention methods (`fn X.f(...)`) instead of impl blocks

### pub on type declarations not supported
- `pub type Point = { ... }` rejected — `pub` only works on `fn`
- Types are always module-public by default

## Priority

1. **Record spread codegen** — High (commonly expected syntax)
2. **result.is_ok/is_err UFCS** — High (common pattern)
3. **Missing runtime functions** — Medium (7 functions)
4. **math.pow sig mismatch** — Medium (easy fix)
5. **value/error arg decoration** — Low (workarounds exist)
6. **unsafe/newtype/trait-impl** — Low (language design decisions needed)
