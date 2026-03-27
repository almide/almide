<!-- description: Unified verb system across all stdlib container types -->
<!-- done: 2026-03-18 -->
# Stdlib API Surface Reform

## Vision

Create a state where the same verb has the same meaning across all stdlib container types, so an LLM learning one verb can apply it to all types.

---

## Design Principles

1. **`map` is the bridge** — usable across List/Map/Option/Result
2. **Argument order is data-first** — natural for both UFCS and `|>` pipe
3. **Predicates have no `?`** — Bool return type is obvious from the type
4. **Conversion is `to_`, construction is `from_`** — `parse` unified to `from_string`
5. **One right answer** — do not create two functions that do the same thing
6. **Direction is `_start` / `_end`** — not left/right but start/end

---

## Implementation Status

### ✅ Step 1: Complete removal of `?` suffix (done)

Renamed 24 functions. All applied, CI green.

### ✅ Step 2: Unify redundant duplicates (mostly done)

| Change | Status |
|---|---|
| `and_then` -> `flat_map` | ✅ Removed |
| `map_values` -> `map.map` | ✅ Unified |
| `char_at` -> `string.get` | ✅ Unified |
| `string.to_int` -> removed | ✅ Removed |
| `string.to_float` -> removed | ✅ Removed |
| `int.parse` → `int.from_string` | 🔲 New name addition needed |
| `float.parse` → `float.from_string` | 🔲 New name addition needed |
| `uuid.parse` → `uuid.from_string` | 🔲 New name addition needed |

Note: `json.parse` maintained as industry standard (`json.from_string` also exists separately).

### 🔲 Step 3: Add verbs for Map

Make Map "a first-class container."

| Function | Status | Description |
|---|---|---|
| `map.map` | ✅ | Transform each (K,V) pair |
| `map.filter` | ✅ | Keep pairs matching predicate |
| `map.fold` | 🔲 | Accumulate with initial value |
| `map.any` | 🔲 | Does any match? |
| `map.all` | 🔲 | Do all match? |
| `map.count` | 🔲 | Count elements matching predicate |
| `map.each` | 🔲 | Side effect on each pair |
| `map.find` | 🔲 | First match by predicate |
| `map.update` | 🔲 | Update key's value with function |

### 🔲 Step 4: Add slice verbs for String

Make String and List slice operations symmetric.

| Function | Status |
|---|---|
| `string.first` | 🔲 |
| `string.last` | 🔲 |
| `string.take` | 🔲 |
| `string.take_end` | 🔲 |
| `string.drop` | 🔲 |
| `string.drop_end` | 🔲 |

### 🔲 Step 5: New Option module

Currently Option operations are scattered. Consolidate into an independent module.

| Function | Status |
|---|---|
| `option.map` | 🔲 |
| `option.flat_map` | 🔲 |
| `option.flatten` | 🔲 |
| `option.unwrap_or` | 🔲 |
| `option.unwrap_or_else` | 🔲 |
| `option.is_some` | 🔲 |
| `option.is_none` | 🔲 |
| `option.to_result` | 🔲 |
| `option.filter` | 🔲 |
| `option.zip` | 🔲 |
| `option.or_else` | 🔲 |

### 🔲 Step 6: Fill List gaps

| Function | Status |
|---|---|
| `list.unique_by` | 🔲 |
| `list.shuffle` | 🔲 |
| `list.window` | 🔲 |
| `list.take_end` | 🔲 |
| `list.drop_end` | 🔲 |

### ✅ Step 7: Delete old names (done)

- `map.from_entries` removed -> unified to `map.from_list` (spec-compliant)
- `parse` maintained (spec principle 3: `parse` = fallible interpretation. `from_string` is out of scope)
- `option.to_list` added (spec-compliant)

---

## Verb Taxonomy (design finalized)

### Transform: map, flat_map, filter, filter_map, flatten
### Aggregate: fold, reduce, scan, sum, product, min, max, count
### Test: any, all, contains, is_empty
### Access: get, first, last, find, find_index, index_of, len
### Slice: take, take_end, take_while, drop, drop_end, drop_while, slice
### Order: sort, sort_by, reverse, shuffle
### Decompose: partition, group_by, chunk, window, zip, unzip, enumerate
### Combine: ++, join, intersperse, merge
### Deduplicate: unique, unique_by, dedup
### Side Effect: each
### Wrapper (Option/Result): map, flat_map, flatten, unwrap_or, unwrap_or_else, map_err, is_some/is_none, is_ok/is_err, to_option, to_result
### Convert: to_string, to_float, to_int, to_hex, from_string, from_chars, from_bytes, from_list, to_list, to_map

---

## Success Criteria

- [x] No `?` suffix on any predicate
- [x] `and_then` -> `flat_map` unified
- [x] `map_values` -> `map.map` unified
- [ ] `map` usable across all container types (List, Map, Option, Result)
- [ ] `flat_map` usable across all container types
- [ ] `filter`, `fold`, `any`, `all`, `each` usable across all collection types (List, Map)
- [ ] Option module exists
- [ ] `parse` -> `from_string` new name addition complete

## Implementation Notes

### from_string addition approach
Do not change TOML function names. Add a new entry `[from_string]` that calls the same runtime function. Keep old name `[parse]` too. Both registered in the arg_transforms table.

### Effort estimate
- Step 2 remaining: 3 functions added (TOML + shared runtime)
- Step 3: 7 functions added (TOML + Rust runtime + TS runtime)
- Step 4: 6 functions added
- Step 5: 11 functions added (new TOML file)
- Step 6: 5 functions added
- Total: 32 functions added

## Files
```
stdlib/defs/*.toml
runtime/rs/src/*.rs
runtime/ts/*.ts
src/stdlib.rs (UFCS mappings)
spec/stdlib/ (tests)
```
