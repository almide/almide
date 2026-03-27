<!-- description: Visual diagnostic improvements: ANSI colors, multi-line spans -->
# Error Diagnostics — Visual Improvements [ACTIVE]

Tier 2+ items split from the original error-diagnostics roadmap. These improve human developer experience but don't affect LLM survival rate.

## Done

### 2.3 Color output ✅ (v0.4.8)
ANSI colors for error (red), warning (yellow), hint (cyan), source location (blue).
Auto-detected: colors only when stderr is a TTY.

### 2.1 Column numbers + caret underline ✅ (v0.4.11)
```
error: undefined variable 'foo'
  --> app.almd:2:16
  |
2 |   let result = foo + bar
  |                ^^^
```
- `Diagnostic` struct: added `col` and `end_col` fields
- Checker tracks `current_decl_col` alongside `current_decl_line`
- `display_with_source()`: gutter line, source line, caret underline
- Caret width: uses `end_col - col` if set, else `context.len()`, else 1
- Location format: `file:line:col` (was `file:line`)

### 2.2 Multi-span errors ✅ (v0.4.11)
```
error: cannot reassign immutable binding 'count'
  --> app.almd:3:3
   |
2 |   let count = 0
  |   ---- 'count' declared as immutable here
...
3 |   count = 5
  |   ^^^^^^^^^^^
```
- `SecondarySpan` struct: `(line, col, label)`
- `Diagnostic.secondary: Vec<SecondarySpan>`
- `with_secondary()` builder method
- `TypeEnv.var_decl_locs`: tracks variable declaration line/col
- Used in: immutable reassignment, type mismatch on reassignment, if-branch mismatch, match-arm mismatch
- Rendering: secondary spans with `---` dashes + label, `...` ellipsis between distant spans

## Remaining

### 3.1 Unreachable code detection
### 3.2 Type narrowing hints

## Priority

~~2.3 Color~~ → ~~2.1 Carets~~ → ~~2.2 Multi-span~~ → **3.x Advanced**
