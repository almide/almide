<!-- description: Fix UFCS resolution for ambiguous methods on complex expressions -->
<!-- done: 2026-03-14 -->
# UFCS Type Resolution for Ambiguous Methods

## Problem

Ambiguous UFCS methods (`len`, `join`, `contains`, `slice`, `reverse`, `index_of`, `count`) fail when the receiver is a non-trivial expression (member access, function call result, etc.).

```almide
type Group = { words: List[String] }

let g = Group { words: ["if", "else"] }
g.words.len()           // ← compile error: `len` not found
list.len(g.words)       // ← works (qualified call)

g.words.join("|")       // ← compile error
string.join(g.words, "|")  // ← works
```

## Root Cause

The IR lowering pass resolves UFCS via `expr_ty(object)`, which uses span-based `(line, col)` lookup into the checker's `expr_types` map. For member-access expressions like `g.words`, the span lookup returns `Ty::Unknown` — even though the checker knows the type is `List[String]`.

When the type is Unknown and the method is ambiguous (e.g. `len` → string/list/map), UFCS resolution fails silently and falls through to unresolved method emission.

```
g.words.len()
  └─ object = Member(Ident("g"), "words")
     └─ expr_ty → Unknown  ← BUG: should be List[String]
        └─ resolve_ufcs_by_type(Unknown) → None
        └─ candidates = ["string", "list", "map"] (3) → can't pick one
        └─ falls through to Case 3 (unresolved method)
```

## Fix

Enhance `expr_ty` in the lowerer to recursively infer types when the span lookup fails. For `Member` expressions, look up the object type and then resolve the field type from record/struct definitions.

## Affected Methods

All multi-module UFCS methods:
- `len` — string, list, map
- `contains` / `contains?` — string, list, map
- `join` — string, list
- `reverse` — string, list
- `index_of` — string, list
- `count` — string, list
- `slice` — string, list
- `get` / `get_or` / `set` — list, map
- `is_empty?` — list, map
