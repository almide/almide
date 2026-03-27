<!-- description: Report all errors at once instead of stopping at the first one -->
<!-- done: 2026-03-13 -->
# Error Recovery

## Why This Is Critical

LLMs are most efficient when they **see all errors at once and fix them in one shot**, not one at a time. The current "stop at first error" behavior unnecessarily increases the dialogue loop with LLMs.

Error recovery is a core element of Almide's "modification survival rate" mission.

## Current State (v0.5.10)

| Component | Recovery | Error Format |
|-----------|----------|-------------|
| Parser (declarations) | `skip_to_next_decl()` — skip to next `fn`/`type`/`test` keyword | Structured `Diagnostic` |
| Parser (statements) | `skip_to_next_stmt()` — sync to next statement boundary, insert `Stmt::Error` | Structured `Diagnostic` |
| Parser (expressions) | Insert `Stmt::Error` on error and continue | Structured `Diagnostic` |
| Type checker | Process all declarations, `Expr::Error` → `Ty::Unknown` to suppress cascading | Structured `Diagnostic` |

### Concrete example of the problem

```almd
fn foo() -> Int = {
  let x = 1 +          // ← parse error here
  let y = x * 2        // ← everything after this is lost
  let z = y + "hello"  // ← type error not reported either
  z
}

fn bar() -> String = {  // ← this is a separate declaration, so it gets parsed
  42                    // ← this type error is reported
}
```

**Current**: Only 1 error reported (`let x` parse error). Remainder of `foo` and type error for `z` are lost.
**Goal**: Report parse error (1) + type errors (2) = 3 errors at once.

## Design Principles

1. **Sync Points at Every Scope Level** — recovery points at each scope level
2. **No Cascading Errors** — suppress secondary errors derived from error nodes
3. **Partial AST is Better Than No AST** — build partial AST even on parse failure and pass to checker
4. **Structured Diagnostics Everywhere** — parser errors output in same `Diagnostic` format as checker

```
Program  → sync on declaration keywords (fn, type, test, impl)
Function → sync on statement boundaries (newline + keyword)
Statement → sync on expression terminators (, ) ] } newline)
Expression → produce ErrorExpr node, continue parsing
```

## Phases

### Phase 1: Structured Parser Errors

Convert parser errors from `String` to `Diagnostic`. Output in same format as checker.

- [x] Change `Parser.errors` from `Vec<String>` → `Vec<Diagnostic>`
- [x] Set filename with `Parser.with_file()`, generate position-aware `Diagnostic` with `diag_error()`
- [x] Unified output with source line display, carets (`^^^`), `display_with_source()`
- [x] `parse_file()` returns `(Program, String)`, eliminating source re-reads
- [x] Add hints to parser errors (e.g., `expected ')' to close function call started at line 5`)
  - `expect_closing()` method: show opening bracket position as secondary span
  - Applied to all major delimiter call sites (`()`, `[]`, `{}`)
- [x] Tests: added parser error snapshot tests (12 tests)

### Phase 2: Statement-Level Recovery

When a parse error occurs in a block, sync to the next statement boundary and continue parsing remaining statements.

- [x] Add statement recovery logic to `parse_brace_expr()`
- [x] `skip_to_next_stmt()`: sync on `let`/`var`/`if`/`match`/`for`/`while`/`do`/`guard` after newline
- [x] Multiple parse errors in one function → all reported
- [x] Statements after error recorded as `Stmt::Error` in AST

### Phase 3: Error AST Nodes

- [x] Add `Expr::Error { span }` / `Stmt::Error { span }` to AST
- [x] Parser generates `Stmt::Error` nodes on error (in-block recovery)
- [x] IR lowering: `Expr::Error` → `IrExprKind::Unit` (type is `Ty::Unknown`)
- [x] IR lowering: `Stmt::Error` → `IrStmtKind::Comment { "/* error */" }`
- [x] Checker: `Expr::Error` → `Ty::Unknown` (suppress cascading errors)
- [x] Checker: `Stmt::Error` → skip (suppress cascading errors)
- [x] Formatter: `Expr::Error` → `/* error */`, `Stmt::Error` → skip

### Phase 4: Statement-Level Expression Recovery

When statement parsing fails inside a block, collect errors, insert `Stmt::Error`, and skip to next statement.

- [x] Insert `Stmt::Error` and continue on error in `parse_brace_expr`
- [x] Sync to next statement boundary with `skip_to_next_stmt()`
- [x] Partial AST passes to checker, type errors also reported simultaneously

### Phase 5: Common Typo Detection

Detect common mistakes and present specific fix suggestions.

- [x] Near-miss keywords: `function`/`func`/`def`/`fun` → hint for `fn`
- [x] Wrong type syntax: `struct`/`class`/`enum`/`data` → hint for `type`
- [x] Wrong operators: `if x = 5` → `Did you mean '=='?`
- [x] Missing delimiters: hints for `)`/`]`/`}`
- [x] Missing `=` before value: `let x value` → `Missing '='`
- [x] Arrow confusion: `fn f() = Int` → `Use '->' for return type`
- [x] `let mut` → `Use 'var'` (existing)
- [x] `<>` generics → `Use []` (existing)

### Phase 6: Checker Continuation on Partial AST

Pass partial AST to checker even with parse errors; report parse errors + type errors together.

- [x] `parse_file` returns parse errors while also returning partial AST
- [x] `compile_with_options` / `cmd_check` combine and report parse errors + checker errors
- [x] `Expr::Error` → `Ty::Unknown` to suppress cascading errors
- [x] Skip IR lowering / codegen when parse errors exist (safety valve)
- [x] Tests: mixed parse errors + type errors → both reported

## Success Criteria

```bash
# For this code:
fn foo() -> Int = {
  let x = 1 +
  let y = "hello" * 2
  y
}

fn bar() -> String = {
  42
}

# Expected error output:
# error[E0001]: unexpected token 'let' — expected expression
#   --> app.almd:2:15
#   |
# 2 |   let x = 1 +
#   |               ^ expected expression after '+'
#
# error[E0002]: cannot apply '*' to String and Int
#   --> app.almd:3:19
#   |
# 3 |   let y = "hello" * 2
#   |                   ^ String does not support arithmetic
#   |
#   = hint: use string.repeat("hello", 2) for repetition
#
# error[E0003]: expected String, found Int
#   --> app.almd:7:3
#   |
# 7 |   42
#   |   ^^ this is Int, but bar() declares return type String

# 3 errors emitted
```

## Reference

| Language | Multi-error | Recovery strategy |
|----------|------------|-------------------|
| **Rust (rustc)** | Yes, all phases | Statement-level sync, error propagation suppression |
| **Go** | Yes, up to 10 | Statement-level sync, `BadExpr`/`BadStmt` nodes |
| **Swift** | Yes | Expression-level recovery, fix-it suggestions |
| **TypeScript** | Yes | Token-level recovery, partial AST |
| **Elm** | One at a time (intentional) | One error, one fix philosophy |
| **Almide (v0.5.10)** | Yes, all phases | Statement + expression level, error AST nodes, partial AST type checking |

## Completion

All 6 phases + parser hints and snapshot tests complete.
