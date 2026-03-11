# Error Diagnostics [IN PROGRESS]

## Current State (v0.4.11)

What we have:
- Line numbers in errors (`--> file.almd:12`)
- Hint messages with actionable suggestions
- Immutable binding reassignment detection
- Multiple error reporting (checker continues after first error)

What we lack vs Rust:
- No column numbers
- No source line display with caret pointing to the problem
- No multi-span errors (showing both definition and usage)
- No "did you mean?" suggestions for typos
- No data-flow warnings (unused mutations, lost return values)

## Comparison with other languages

| Feature | Rust | Go | TypeScript | Python | Almide |
|---------|------|----|------------|--------|--------|
| Line + column | Y | Y | Y | Y | Line only |
| Source line display | Y | N | Y | Y | N |
| Caret underline | Y | N | Y | Y | N |
| Multi-span (def + use) | Y | N | Y | N | N |
| "did you mean?" | Y | N | Y | N | N |
| Multiple errors per run | Y | Y | Y | Y | Y (checker) |
| Unused variable warning | Y | Error | Y | N | Y (`_` prefix) |
| Immutable reassign | Y | N/A | N | N | Y (v0.4.11) |
| Lost mutation warning | N | N | N | N | N |
| Color output | Y | N | Y | Y | N |

### Assessment
Almide's error quality is between Go and TypeScript. The gap to Rust is primarily visual (source display, carets) and intelligence (data-flow analysis). For LLM consumers, the hint text matters more than visual formatting.

## Roadmap

### Tier 1 — LLM-critical (highest ROI for modification survival rate)

#### 1.1 Lost mutation detection [NEW]
```
warning: 'arr' is modified via list.set but never returned
  --> quicksort.almd:4
  hint: The modified list is discarded. Return it: -> (List[Int], Int)
```
**Why**: This is the #1 cause of silent LLM failures. mutable-algorithm ports look correct but produce wrong results.
**Effort**: Medium. Track `list.set`/`list.swap` call targets, check if they appear in return path.
**See**: [llm-immutable-patterns.md](./llm-immutable-patterns.md)

#### 1.2 Richer immutability errors
```
error: cannot reassign immutable binding 'arr'
  --> quicksort.almd:12
  hint: 'arr' is a function parameter (immutable). Try: var arr_ = arr
```
Currently we say "Use 'var' instead of 'let'" which is confusing for parameters.
**Effort**: Low. Distinguish param vs let in error message.

#### 1.3 Undefined function/variable suggestions
```
error: undefined function 'lenght'
  --> app.almd:5
  hint: Did you mean 'list.len'?
```
LLMs frequently misspell or use wrong module prefixes. Levenshtein distance on known symbols.
**Effort**: Medium.

### Tier 2 — Developer experience

#### 2.1 Source line display
```
error: cannot assign String to variable 'count' of type Int
  --> app.almd:12:15
   |
12 |   count = "hello"
   |           ^^^^^^^ expected Int, found String
```
**Steps**:
1. Retain source text in compiler (or re-read file on error)
2. Add column to Span
3. Format with caret underline

**Effort**: Medium-high.

#### 2.2 Multi-span errors
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
**Effort**: High. Needs Span on variable definitions + lookup in error formatter.

#### 2.3 Color output
ANSI colors for error/warning/hint differentiation. Most terminals support it.
**Effort**: Low (cosmetic).

### Tier 3 — Advanced analysis

#### 3.1 Unused mutation warning
```
warning: value of list.set is unused
  --> app.almd:10
  hint: list.set returns a new list — assign it: xs = list.set(xs, i, v)
```
For when someone writes `list.set(xs, i, v)` as a statement (discarding return).

#### 3.2 Unreachable code detection
```
warning: unreachable code after guard
  --> app.almd:12:3
```

#### 3.3 Type narrowing hints
```
error: cannot use 'x' of type 'Int | None' as 'Int'
  hint: Use guard: guard x != none else err("x is none")
```

## Priority

**1.1 Lost mutation** → **1.2 Richer immutability** → **1.3 Suggestions** → **2.1 Source display** → **2.3 Color** → **2.2 Multi-span** → **3.x Advanced**

The first two items directly improve LLM modification survival rate. Visual improvements (2.x) matter more for human developers.
