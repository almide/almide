# Error Diagnostics [DONE]

## Implemented

### LLM-critical diagnostics (Tier 1)

#### 1.1 Lost mutation detection ✅
```
warning: return value of list.set() is unused
  --> quicksort.almd:4
  hint: list.set() returns a new list — assign it: xs = list.set(xs, ...)
```

Detects when immutable update functions (`list.set`, `list.swap`, `list.push`, `list.sort`, `map.set`, `string.replace`, etc.) are called as statements with the return value discarded. Covers both module calls (`list.set(xs, ...)`) and UFCS (`xs.push(...)`).

**Location**: `src/check/statements.rs`

#### 1.2 Richer immutability errors ✅
```
error: cannot reassign immutable binding 'x'
  hint: 'x' is a function parameter (immutable). Use a local copy: var x_ = x
```
vs
```
error: cannot reassign immutable binding 'count'
  hint: Use 'var count = ...' instead of 'let count = ...' to declare a mutable variable
```

Parameters and `let` bindings produce different, targeted hints.

**Location**: `src/check/statements.rs`, `src/types.rs` (param_vars tracking)

#### 1.3 "Did you mean?" suggestions ✅
```
error: undefined function 'string.lenght'
  hint: Did you mean 'string.len'?
```

Covers undefined functions, variables, and module functions. Uses Levenshtein distance + substring containment. Module function candidates auto-generated from TOML via build.rs.

**Location**: `src/check/mod.rs`, `src/check/calls.rs`, `src/check/expressions.rs`, `build.rs`

### Baseline diagnostics

- Line numbers in errors (`--> file.almd:12`) ✅
- Source line display ✅
- Hint messages with actionable suggestions ✅
- Multiple error reporting (checker continues after first error) ✅
- Undefined variables and functions now emit proper errors ✅
