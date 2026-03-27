<!-- description: Ergonomics issues found via self-tooling, evaluated against design principles -->
<!-- done: 2026-03-17 -->
# 2026 Ergonomics Roadmap

Ergonomics issues discovered through self-tooling (Chrome extension, TextMate generator,
Playground modules), evaluated against SPEC/DESIGN principles.

**Design Principles** (SPEC §0):
1. Canonicity — one way to write each concept
2. Surface Semantics — side effects/errors/Optional are visible in types
3. Vocabulary Economy — minimal vocabulary
4. No Magic — no implicit conversions

---

## P0: Pure function support for `do` blocks + `guard else break/continue` ✅

### Problem

The SPEC explicitly states "`for...in` for collections, `do { guard }` for dynamic loops."
However, the current `do` block only works in Result/Option contexts, leaving
no way to write dynamic loops in pure functions.

As a result, the nightmarish pattern `for _ in 0..len { if done then () else { ... } }` appears everywhere.

### Solution

Do not add a `while` keyword (3 loop types would violate Canonicity).
Instead, extend `do` blocks to work in pure functions and introduce
`guard else break` / `guard else continue`.

```almd
// Pure do block — equivalent to while
var i = 0
do {
  guard i < len else break
  let ch = string.char_at(code, i).unwrap_or("")
  if ch == "\"" then break
  result = result ++ ch
  i = i + 1
}

// guard else break/continue also works inside for loops
for ch in chars {
  guard ch != " " else continue
  result = result ++ ch
}
```

### Alignment with design principles

| Principle | Assessment |
|-----------|------------|
| Canonicity | ○ Unified under `do { guard }` without adding `while`. Early exit always goes through `guard else` |
| Vocabulary Economy | ○ Only 2 new keywords: `break`/`continue`. Cannot be used standalone, always paired with `guard else` |
| Surface Semantics | ○ Loop exit conditions are explicit via `guard` |
| LLM compatibility | ○ LLMs already know `break`/`continue`. `guard else break` is naturally understood |

### Impact

Expected **30-40% code reduction** in highlight.almd and runtime.almd.

---

## P1-a: `unwrap_or` bug fix (type checker) ✅

### Problem

`unwrap_or(opt, default)` exists as a built-in in codegen (calls.rs), but
the type checker (check/) does not recognize it. Results in `undefined function 'unwrap_or'` error.

### Solution

Add built-in function type signatures to check/calls.rs.
Register `unwrap_or: (Option[T], T) -> T`.

### Alignment with design principles

Bug fix. `unwrap_or` is explicitly specified in SPEC §18.

### Impact

After the fix, all of the following patterns work:
```almd
string.index_of(data, needle).unwrap_or(0 - 1)
json.get_string(obj, key).unwrap_or("")
json.get_array(obj, key).unwrap_or([])
string.char_at(code, i).unwrap_or("")
```

This makes the `??` operator unnecessary (maintaining Canonicity: only one way to unwrap).

---

## P1-b: `json.parse` auto `?` insertion bug fix ✅

### Problem

`json.parse(data)` is a pure function returning `Result[Json, String]`, but
codegen automatically inserts `?`, causing compile errors when the caller is not a `Result`-returning function.

It was impossible to handle it locally in a pure function with
`match json.parse(data) { ok(obj) => ..., err(_) => ... }`.

### Solution

Remove `json.parse` from the `result_fns`/`effect_fns` sets in stdlib.
Auto `?` is only inserted inside `effect fn` or `do` blocks.
In regular functions, `Result` is returned as-is and the user explicitly matches on it.

```almd
// Should work in pure functions (after fix)
fn safe_extract(data: String) -> String =
  match json.parse(data) {
    ok(obj) => json.get_string(obj, "text").unwrap_or("")
    err(_) => ""
  }

// Auto ? works inside effect fn + do blocks (existing behavior)
effect fn load(data: String) -> Result[Json, String] = do {
  let obj = json.parse(data)  // auto-? here
  ok(obj)
}
```

### Alignment with design principles

| Principle | Assessment |
|-----------|------------|
| Surface Semantics | ○ `Result` is visible in the type, and the user handles it explicitly |
| No Magic | ○ Stops implicit `?` insertion, encouraging explicit handling |
| Canonicity | ○ Auto-? inside `do`, match everywhere else — unambiguous based on context |

### Impact

A pure SSE parser using `import json` can now be written in sse.almd.
No longer necessary to write a manual JSON string parser.

---

## Not Doing: `??` operator

**Reason**: Once `unwrap_or` is fixed, you can write `opt.unwrap_or(default)` via UFCS.
Adding `??` would create 3 ways to express the same thing, violating Canonicity:
- `unwrap_or(opt, d)` — function call
- `opt.unwrap_or(d)` — UFCS
- `opt ?? d` — operator

From a Vocabulary Economy perspective, a new operator is unnecessary when existing vocabulary suffices.

---

## Not Doing: `while` keyword

**Reason**: The SPEC defines 2 loop forms: `for...in` + `do { guard }`.
Adding `while` would create 3 loop constructs, violating Canonicity.
The same expressiveness is achieved by extending `do` blocks.

---

## Not Doing: `s[i]` index syntax

**Reason**: SPEC §19 explicitly prohibits operator overloading.
`string.char_at(s, i).unwrap_or("")` is sufficient (after `unwrap_or` fix).

---

## Not Doing: `{}` omission in match arms

**Reason**: Same rule as Rust. Consistent with LLMs' existing knowledge.
Also aligns with Canonicity (blocks are always wrapped in `{}`).

---

## Summary

| Item | Decision | Reason |
|------|----------|--------|
| `do` purification + `guard else break/continue` | **Implement** | Completing the SPEC's `do { guard }` design |
| `unwrap_or` type check fix | **Implement** | Bug fix |
| `json.parse` auto `?` fix | **Implement** | Bug fix |
| `??` operator | **Not doing** | Canonicity violation |
| `while` keyword | **Not doing** | Canonicity violation |
| `s[i]` indexing | **Not doing** | Operator overloading prohibited |
| match arm `{}` omission | **Not doing** | Current behavior is consistent with Rust |

3 items to implement. All align with design principles and complete or fix the existing SPEC design.

All 3 items implemented. Tests: `spec/lang/do_block_pure_test.almd`, `spec/stdlib/unwrap_or_test.almd`, `spec/stdlib/json_test.almd`
