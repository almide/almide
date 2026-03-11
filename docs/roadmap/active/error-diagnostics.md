# Error Diagnostics — Visual Improvements [PLANNED]

Tier 2+ items split from the original error-diagnostics roadmap. These improve human developer experience but don't affect LLM survival rate.

## Remaining

### 2.1 Column numbers + caret underline
```
error: cannot assign String to variable 'count' of type Int
  --> app.almd:12:15
   |
12 |   count = "hello"
   |           ^^^^^^^ expected Int, found String
```
**Steps**: Add column to Span, format with caret underline.
**Effort**: Medium-high.

### 2.2 Multi-span errors
```
error: cannot assign String to variable 'count' of type Int
  --> app.almd:12:15
   |
 8 |   var count = 0
   |       ----- declared as Int here
   |
12 |   count = "hello"
   |           ^^^^^^^ found String
```
**Effort**: High.

### 2.3 Color output
ANSI colors for error/warning/hint differentiation.
**Effort**: Low (cosmetic).

### 3.1 Unreachable code detection
### 3.2 Type narrowing hints

## Priority

**2.3 Color** → **2.1 Carets** → **2.2 Multi-span** → **3.x Advanced**
