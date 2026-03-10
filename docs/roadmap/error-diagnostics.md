# Error Diagnostics [PLANNED]

## Source Location (Span)
The current Diagnostic only has a context field; line and column numbers are missing.

```
// current
error: list element at index 1 has type String but expected Int
  in list literal
  hint: All list elements must have the same type

// improved
error: list element at index 1 has type String but expected Int
  --> src/main.almd:15:12
   |
15 |   [1, "hello", 3]
   |       ^^^^^^^ expected Int, found String
  hint: All list elements must have the same type
```

### Implementation Steps
1. Add line/column numbers to Token (lexer.rs)
2. Add Span fields to AST Expr/Stmt nodes
3. Checker passes Span to Diagnostic
4. display() shows the source line

## Improved Parse Errors
The current parser stops at the first error. Reporting multiple errors at once is preferable.

## Unused Variable Warnings
```
warning: unused variable `temp`
  --> src/main.almd:10:7
  hint: prefix with _ to suppress: `_temp`
```

## Unreachable Code Detection
```
warning: unreachable code after guard
  --> src/main.almd:12:3
```

## Priority
Source location > unused variable warnings > improved parse errors > unreachable detection
