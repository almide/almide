# Error Recovery [ACTIVE]

## The Problem

LLMs generate code with multiple errors at once. Current compiler behavior:

1. **Parser**: Declaration-level recovery exists (`skip_to_next_decl`), but a single typo inside a function body loses the entire function — all subsequent errors in that function go unreported
2. **Parser errors are plain strings** — no source line display, no carets, no secondary spans. Checker errors use structured `Diagnostic` with colored source rendering, but parser errors don't
3. **No statement-level sync** — a missing `)` in a let binding skips to the next `fn` declaration, losing all remaining statements in the block
4. **No expression-level recovery** — partial expressions are discarded entirely instead of producing an error node in the AST

For LLM-generated code, reporting all errors in a single pass is critical: the LLM can fix them all at once instead of iterating one error at a time.

## Current State

| Component | Recovery | Error Format |
|-----------|----------|-------------|
| Parser (declarations) | `skip_to_next_decl()` — skips to next `fn`/`type`/`test` keyword | Plain string with line:col |
| Parser (statements) | None — error bubbles to declaration level | Plain string |
| Parser (expressions) | None — first error aborts expression | Plain string |
| Type checker | Full — processes all declarations, collects all `Diagnostic` | Structured `Diagnostic` with source, hints, secondary spans |

## Design

### Principle: Sync Points at Every Scope Level

```
Program  → sync on declaration keywords (fn, type, test, impl)     [DONE]
Function → sync on statement boundaries (newline + indent level)   [NEW]
Statement → sync on expression terminators (, ) ] } newline)       [NEW]
Expression → produce ErrorExpr node, continue parsing              [NEW]
```

### Error Node in AST

Add `Expr::Error` and `Stmt::Error` variants so the parser can produce a partial AST with placeholder nodes where errors occurred. The checker skips error nodes silently (no cascading errors).

## Phases

- [ ] Phase 1: Structured parser errors — convert parser errors from `String` to `Diagnostic` with source line rendering, hints, and secondary spans
- [ ] Phase 2: Statement-level recovery — after a parse error inside a block, sync to the next statement boundary (newline at same or lower indent) instead of skipping to next declaration
- [ ] Phase 3: Error AST nodes — add `Expr::Error` / `Stmt::Error` to AST so partial blocks are preserved. Checker treats error nodes as `Ty::Unknown` without emitting cascading diagnostics
- [ ] Phase 4: Expression-level recovery — on unexpected token inside an expression, emit `Expr::Error` and skip to a safe recovery point (closing delimiter, comma, newline)
- [ ] Phase 5: Common typo suggestions — detect near-miss keywords (`funcion` → `fn`), missing delimiters (`if cond { ... ` → missing `}`), and wrong operators (`=` → `==` in conditions)
- [ ] Phase 6: Checker continuation on partial AST — ensure checker handles partial/error-containing ASTs gracefully, suppressing cascading errors from error nodes
