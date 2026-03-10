# Stdlib Gaps [DONE]

## Goal
Reduce boilerplate in AI-generated code, improving LOC, token count, and generation time.

## Benchmark Analysis
Analysis of AI-generated code from minigit (3 runs) and miniconf (1 run):
- **Character classification**: AI manually defines `is_digit`, `is_alpha` each time with 60-char lists (40–60 LOC)
- **String character decomposition**: `list.filter(string.split(s, ""), ...)` pattern used 10+ times
- **Magic number slice**: `string.slice(line, 8)` to skip "parent: " etc.
- **Redundant match get**: `match list.get(xs, i) { some(v) => v, none => "" }` appears 64 times
- **Manual indexed loops**: `var i = 0; do { guard i < len; ... }` appears 5–7 times

## Phase 1: String & Character Operations (LOC -40~60)

### 1.1 string.chars
```almide
string.chars("abc") → ["a", "b", "c"]   (* UTF-8 character decomposition *)
```
- Currently AI substitutes with `list.filter(string.split(s, ""), fn(c) => string.len(c) > 0)`
- UFCS: `s.chars()`

### 1.2 string.index_of
```almide
string.index_of("hello world", "world") → some(6)
string.index_of("hello", "xyz") → none
```
- Frequently needed in parser-like code. Used together with `string.slice`

### 1.3 string.repeat
```almide
string.repeat("ab", 3) → "ababab"
```

### 1.4 string.from_bytes
```almide
string.from_bytes([104, 105]) → "hi"
```
- Inverse of `string.to_bytes`

### 1.5 Character predicate functions
```almide
string.is_digit?("3") → true       (* 0-9 *)
string.is_alpha?("a") → true       (* a-zA-Z *)
string.is_alphanumeric?("a") → true
string.is_whitespace?(" ") → true  (* space, tab, newline *)
```
- Placed in `string` module (no separate char module; single-char String is used)
- Prevents AI from writing `["0","1",...,"9"]` every time
- UFCS: `c.is_digit?()`

## Phase 2: List Operations (LOC -20~30)

### 2.1 list.enumerate
```almide
list.enumerate(["a", "b", "c"]) → [(0, "a"), (1, "b"), (2, "c")]
```
- Reduces manual `var i = 0` loops
- Returns `(Int, T)` tuples

### 2.2 list.zip
```almide
list.zip([1, 2], ["a", "b"]) → [(1, "a"), (2, "b")]
```

### 2.3 list.flatten
```almide
list.flatten([[1, 2], [3], [4, 5]]) → [1, 2, 3, 4, 5]
```

### 2.4 list.take / list.drop
```almide
list.take([1, 2, 3, 4], 2) → [1, 2]
list.drop([1, 2, 3, 4], 2) → [3, 4]
```

### 2.5 list.sort_by
```almide
list.sort_by(users, fn(u) => u.name)
```
- Current `list.sort` only supports natural order

### 2.6 list.unique
```almide
list.unique([1, 2, 2, 3, 1]) → [1, 2, 3]
```

## Phase 3: Utility Modules (new)

### 3.1 math module
```almide
import math

math.min(3, 5) → 3
math.max(3, 5) → 5
math.abs(-3) → 3
math.pow(2, 10) → 1024
math.pi → 3.14159...
math.e → 2.71828...
math.sin(x) math.cos(x) math.log(x) math.exp(x) math.sqrt(x)
```
- `math.min`/`math.max` are Int versions. Float versions via `float.min`/`float.max` (TBD)
- Not auto-imported (`import math` required)

### 3.2 random module
```almide
import random

random.int(1, 100) → 42          (* min..max inclusive *)
random.float() → 0.7234          (* 0.0..1.0 *)
random.choice(["a", "b", "c"]) → "b"
random.shuffle([1, 2, 3]) → [3, 1, 2]
```
- effect fn (non-deterministic)
- Rust: no `rand` crate, use `getrandom` or custom xorshift
- Not auto-imported

### 3.3 time module
```almide
import time

time.now() → 1709913600          (* unix timestamp, moved from env.unix_timestamp *)
time.sleep(1000)                 (* ms *)
```
- Compatibility with `env.unix_timestamp` via alias

## Phase 4: Regular Expressions (highest impact)

### 4.1 regex module
```almide
import regex

regex.match?("[0-9]+", "abc123") → true
regex.find("[0-9]+", "abc123def") → some("123")
regex.find_all("[0-9]+", "a1b22c333") → ["1", "22", "333"]
regex.replace("[0-9]+", "a1b2", "X") → "aXbX"
regex.split("[,;]", "a,b;c") → ["a", "b", "c"]
```
- Rust: custom basic regex engine (zero-dependency policy)
- Supports: `.` `*` `+` `?` `[]` `[^]` `\d` `\w` `\s` `^` `$` `|` `()`
- Highest potential LOC reduction (miniconf parser code could shrink by half)
- Also the highest implementation cost

## Implementation Order

```
Phase 1 (string/char)  →  benchmark  →  Phase 2 (list)  →  benchmark  →  Phase 3/4
```

### Phase 1 priority
1. `string.chars` — immediate impact (replaces 10+ `split+filter` patterns)
2. `string.is_digit?` / `string.is_alpha?` — eliminates character classification boilerplate
3. `string.index_of` — simplifies parser code
4. `string.repeat` — low cost, useful to have
5. `string.from_bytes` — symmetry with `to_bytes`

### Per-phase work
1. Add type signatures to `stdlib.rs`
2. Add Rust codegen to `emit_rust/calls.rs`
3. Add TS codegen to `emit_ts/expressions.rs` (where applicable)
4. Add UFCS to `resolve_ufcs_module` in `stdlib.rs`
5. Add tests to `exercises/stdlib-test/`
6. Verify all exercises pass

### Estimated LOC reduction
| Phase | AI-generated code reduction | Benchmark impact |
|-------|----------------------------|-----------------|
| Phase 1 | -40~60 LOC/task | tokens -15~20% |
| Phase 2 | -20~30 LOC/task | tokens -5~10% |
| Phase 3 | new features (expansion, not reduction) | — |
| Phase 4 | -50~80 LOC/task | tokens -20~30% |

## Principles
- Auto-import: Phase 1 and 2 add to existing modules (string, list), already auto-imported
- Phase 3 and 4 are new modules requiring explicit `import`
- Maintain zero-dependency policy (including custom regex implementation)
- Do not add speculatively; measure benchmark impact before moving to the next phase
