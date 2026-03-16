# Exercise Suite v0.6.0

## Status: 20/23 exercises, 230 tests — Tier 1-4 + Tier 6 complete

## Design Principles

- Each exercise tests a specific language feature tier
- Exercises double as LLM benchmark (CHEATSHEET-only solvable)
- Every exercise includes test blocks — `almide test` runs them directly
- Cross-target: all must compile and pass on `--target rust`, `--target ts`, `--target js`

## Tier 1: Pure Functions — DONE (6 exercises, 96 tests)

| Exercise | Tests | Features |
|---|---|---|
| raindrops | 18 | int modulo, string concat `++`, if/else |
| pangram | 10 | `string.chars`, `list.all`, `string.to_lower` |
| roman-numerals | 19 | `for..in`, `var`, `while`, accumulator |
| scrabble-score | 11 | `match`, `string.chars`, `list.fold` |
| isogram | 13 | `string.to_lower`, `list.unique`, pipes |
| bob | 25 | Multi-branch if/else, `string.trim`, `list.any` |

## Tier 2: Variant Types + Pattern Matching — DONE (4 exercises, 40 tests)

| Exercise | Tests | Features |
|---|---|---|
| calculator | 10 | Recursive record variant, effect fn + do-block, nested match |
| traffic-light | 8 | Simple enum, match, fn composition |
| markdown-renderer | 10 | Tuple variant, string ops, `string.repeat` |
| expression-eval | 12 | Recursive tuple variant, string interpolation |

## Tier 3: Effect fn + Error Handling — DONE (4 exercises, 41 tests)

| Exercise | Tests | Features |
|---|---|---|
| collatz | 8 | `effect fn`, `while`, `guard` |
| hamming | 10 | `effect fn`, `guard`, `list.zip`, tuple access |
| phone-number | 12 | `effect fn`, multi-guard chain, `list.filter` |
| pipeline | 5 | `do` block, multi-function error propagation |
| affine-cipher | 11 | `effect fn`, gcd, mod_inverse, `for` + `while` |

## Tier 4: Row Polymorphism + Generics — DONE (3 exercises, 29 tests)

| Exercise | Tests | Features |
|---|---|---|
| named-things | 12 | `describe[T]`, `set_name[T]`, `names[T]`, mono in lambda |
| sortable | 5 | `sort_by_key[T: { key: Int, .. }]`, cross-type sort |
| data-table | 4 | `pluck_ids[T]`, `find_by_id[T]`, pipe chains |

**Compiler bugs found & fixed during Tier 4:**
- Monomorphization didn't detect TypeVar inside `List[T]`, `Option[T]`
- Monomorphization type extraction: `List[Dog]` → `T=Dog` (was `T=List[Dog]`)
- Monomorphization missing from `--target rust` emit pipeline
- `instantiate_ty` freshened inference vars, breaking lambda param type resolution (root cause)

## Tier 5: Codec + JSON — BLOCKED (runtime gap)

Requires runtime implementations for json.parse/stringify Value roundtrip.
Will be unblocked by stdlib v2 runtime gap work.

## Tier 6: Integration — DONE (2 exercises, 24 tests)

| Exercise | Tests | Features |
|---|---|---|
| todo-app | 10 | Records, variant status, spread update, pipes |
| isbn-verifier | 14 | String parsing, for-enumerate, validation |

## Summary

| Tier | Exercises | Tests | Status |
|---|---|---|---|
| 1. Pure Functions | 6 | 96 | DONE |
| 2. Variants | 4 | 40 | DONE |
| 3. Effect/Error | 4 | 41 | DONE |
| 4. Row Polymorphism | 3 | 29 | DONE |
| 5. Codec | 0 | 0 | BLOCKED (runtime gap) |
| 6. Integration | 2 | 24 | DONE |
| **Total** | **20** | **230** | |

## Compiler Bugs Found via Exercises

| Bug | Found by | Fix |
|---|---|---|
| while loop outer var clone | roman-numerals | use_count: bump outer vars in loop body |
| lambda capture outer var clone | stress test | use_count: bump outer vars in lambda body |
| mono `List[T]` param not detected | named-things | `ty_contains_typevar` recursive check |
| mono type extraction `List[T]→T` | named-things | `extract_typevar_binding` recursive unify |
| mono missing from emit pipeline | named-things | add `monomorphize` to `cmd_emit` |
| inference var freshening in lambda | named-things | `instantiate_ty`: don't freshen `?N` vars |
| mono Unknown binding crash | stress test | skip Unknown bindings in discover |

## Remaining

- **Tier 5 (Codec)**: Blocked on runtime gap. json-config + api-response need Value runtime
- **wasm-smoke**: CI-only exercise, needs WASM target build
- **Cross-target testing**: exercises need TS/JS target verification (CI)
