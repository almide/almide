# Error Diagnostics [IN PROGRESS]

## Current State (v0.4.12)

What we have:
- Line numbers in errors (`--> file.almd:12`)
- Source line display with line number
- Hint messages with actionable suggestions
- Immutable binding reassignment detection (param vs let distinguished)
- Multiple error reporting (checker continues after first error)
- "Did you mean?" suggestions for undefined functions, variables, and module functions
- Levenshtein distance + substring matching for typo correction

What we lack vs Rust:
- No column numbers
- No caret underline pointing to the problem
- No multi-span errors (showing both definition and usage)
- No color output
- No data-flow warnings (unused mutations, lost return values)

## Comparison with other languages

| Feature | Rust | Go | TypeScript | Python | Almide |
|---------|------|----|------------|--------|--------|
| Line + column | Y | Y | Y | Y | Line only |
| Source line display | Y | N | Y | Y | Y |
| Caret underline | Y | N | Y | Y | N |
| Multi-span (def + use) | Y | N | Y | N | N |
| "did you mean?" | Y | N | Y | N | Y |
| Multiple errors per run | Y | Y | Y | Y | Y (checker) |
| Unused variable warning | Y | Error | Y | N | Y (`_` prefix) |
| Immutable reassign | Y | N/A | N | N | Y (param vs let) |
| Color output | Y | N | Y | Y | N |

### Assessment
Almide's error quality is now between TypeScript and Rust. The remaining gap is primarily visual (carets, color, multi-span) and analysis depth (data-flow).

## Done

### 1.2 Richer immutability errors ✅

```
error: cannot reassign immutable binding 'x'
  --> quicksort.almd:12
  hint: 'x' is a function parameter (immutable). Use a local copy: var x_ = x
```

Parameters and `let` bindings produce different hints. `param_vars` tracking in TypeEnv distinguishes origins.

**Location**: `src/check/statements.rs`, `src/types.rs`

### 1.3 Undefined function/variable suggestions ✅

```
error: undefined function 'string.lenght'
  --> app.almd:5
  hint: Did you mean 'string.len'?
```

Covers:
- Undefined direct functions → suggests from user-defined + builtins
- Undefined variables → suggests from variables in scope
- Undefined module functions → suggests from stdlib module function lists (auto-generated from TOML)

Uses Levenshtein distance (threshold: 40% of name length, min 1, max 3) plus substring containment fallback.

**Location**: `src/check/mod.rs` (suggest_similar, suggest_module_fn, levenshtein), `src/check/calls.rs`, `src/check/expressions.rs`

### Undefined errors now reported ✅

Previously, undefined variables and functions silently returned `Ty::Unknown`. Now they emit proper error diagnostics with suggestions.

## Remaining

### Tier 1 — LLM-critical

#### 1.1 Lost mutation detection
```
warning: 'arr' is modified via list.set but never returned
  --> quicksort.almd:4
  hint: The modified list is discarded. Return it: -> (List[Int], Int)
```
**Why**: This is the #1 cause of silent LLM failures.
**Effort**: Medium. Track `list.set`/`list.swap` call targets, check if they appear in return path.
**See**: [llm-immutable-patterns.md](./llm-immutable-patterns.md)

### Tier 2 — Developer experience

#### 2.1 Column numbers + caret underline
```
error: cannot assign String to variable 'count' of type Int
  --> app.almd:12:15
   |
12 |   count = "hello"
   |           ^^^^^^^ expected Int, found String
```
**Steps**: Add column to Span, format with caret underline.
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
**Effort**: High.

#### 2.3 Color output
ANSI colors for error/warning/hint differentiation.
**Effort**: Low (cosmetic).

### Tier 3 — Advanced analysis

#### 3.1 Unused mutation warning
```
warning: value of list.set is unused
  --> app.almd:10
  hint: list.set returns a new list — assign it: xs = list.set(xs, i, v)
```

#### 3.2 Unreachable code detection

#### 3.3 Type narrowing hints

## Priority

**1.1 Lost mutation** → **2.3 Color** → **2.1 Carets** → **2.2 Multi-span** → **3.x Advanced**
