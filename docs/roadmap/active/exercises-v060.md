# Exercise Suite v0.6.0

Rewrite exercise suite from scratch for v0.6.0 compiler. All exercises must pass on Rust, TS, and JS targets.

## Design Principles

- Each exercise tests a specific language feature tier
- Exercises double as LLM benchmark (CHEATSHEET-only solvable)
- Every exercise includes test blocks — `almide test` runs them directly
- Cross-target: all must compile and pass on `--target rust`, `--target ts`, `--target js`
- WASM: at least one smoke test exercise must build `--target wasm`

## Tier 1: Pure Functions (string/list/math)

Exercises that test basic computation, stdlib usage, and pattern matching on primitives.

| Exercise | Features Tested | Tests (est.) |
|---|---|---|
| pangram | `string.chars`, `list.all`, `string.to_lower` | 10 |
| raindrops | `int` modulo, string concat `++`, if/else chains | 18 |
| roman-numerals | `for..in`, `var`, accumulator pattern | 19 |
| scrabble-score | `match`, `string.chars`, `list.fold` | 11 |
| isogram | `string.to_lower`, `string.chars`, `list.unique`, duplicate detection | 13 |
| bob | Multi-branch `if/else`, `string.trim`, `string.ends_with`, `string.is_empty` | 25 |

**Target**: 6 exercises, ~96 tests

## Tier 2: Variant Types + Pattern Matching

ADTs, nested match, tuple variants, record variants.

| Exercise | Features Tested | Tests (est.) |
|---|---|---|
| calculator | `type Op = \| Add \| Sub \| Mul \| Div`, `match`, `Result` for div-by-zero | 15 |
| markdown-renderer | Variant with `Heading(Int, String)`, `Paragraph(String)`, `Divider` — the playground example | 10 |
| traffic-light | State machine: `type State = \| Red \| Yellow \| Green`, `fn next`, `fn duration` | 8 |
| expression-eval | Recursive variant: `type Expr = \| Lit(Int) \| Add(Expr, Expr) \| Mul(Expr, Expr)`, recursive eval | 12 |

**Target**: 4 exercises, ~45 tests

## Tier 3: Effect fn + Error Handling

`effect fn`, `do` block, `guard`, `Result` propagation, `match ok/err`.

| Exercise | Features Tested | Tests (est.) |
|---|---|---|
| collatz-conjecture | `effect fn` returning `Result`, input validation with `guard` | 8 |
| hamming | `effect fn`, `err()` for different-length strings, `list.zip` | 10 |
| phone-number | `effect fn`, multi-step validation, `guard` chains, `do` block | 16 |
| pipeline | `do` block auto-unwrap, multi-function error propagation chain | 36 |
| affine-cipher | `effect fn`, modular arithmetic, `guard` for coprime check | 16 |

**Target**: 5 exercises, ~86 tests

## Tier 4: Row Polymorphism + Generics

`OpenRecord`, `T: { field: Type, .. }`, monomorphization, generic functions.

| Exercise | Features Tested | Tests (est.) |
|---|---|---|
| named-things | `fn describe[T: { name: String, .. }]`, multiple record types, monomorphization | 12 |
| sortable | `fn sort_by_field[T: { key: Int, .. }]`, generic sort with structural bound | 10 |
| data-table | `fn pluck[T: { id: Int, .. }]`, `fn update[T: { id: Int, .. }]`, list of records | 15 |

**Target**: 3 exercises, ~37 tests

## Tier 5: Codec + JSON

`encode`, `decode`, `Value`, JSON roundtrip, derive.

| Exercise | Features Tested | Tests (est.) |
|---|---|---|
| json-config | Record encode/decode, `json.stringify`/`json.parse`, field alias | 14 |
| api-response | Nested records, `List[T]` codec, `Option` fields, decode error handling | 18 |

**Target**: 2 exercises, ~32 tests

## Tier 6: Integration (Multi-Feature)

Exercises combining multiple features in realistic scenarios.

| Exercise | Features Tested | Tests (est.) |
|---|---|---|
| todo-app | Records, `List`, `map`, `filter`, variant status, string formatting | 20 |
| isbn-verifier | String parsing, `list.fold`, validation logic, `Bool` return | 14 |
| wasm-smoke | Minimal `fn main`, println — must build `--target wasm` | 2 |

**Target**: 3 exercises, ~36 tests

## Summary

| Tier | Exercises | Tests | Primary Feature |
|---|---|---|---|
| 1. Pure Functions | 6 | ~96 | String/list/math, basic control flow |
| 2. Variants | 4 | ~45 | ADTs, pattern matching, recursive types |
| 3. Effect/Error | 5 | ~86 | effect fn, do block, guard, Result |
| 4. Row Polymorphism | 3 | ~37 | OpenRecord, generics, monomorphization |
| 5. Codec | 2 | ~32 | JSON encode/decode, Value |
| 6. Integration | 3 | ~36 | Multi-feature realistic scenarios |
| **Total** | **23** | **~332** | |

## Implementation Order

1. **Tier 1 + wasm-smoke first** — validates basic codegen on all targets
2. **Tier 2 + 3** — covers the most common Almide patterns
3. **Tier 4** — validates monomorphization (v0.6.0's headline feature)
4. **Tier 5 + 6** — advanced features and integration

## CI Integration

```yaml
# Replace current exercise jobs with:
- name: Run exercises (Rust target)
  run: almide test exercises/
  # No continue-on-error — all must pass

- name: Run exercises (TS target)
  run: |
    for f in exercises/*/*.almd; do
      almide "$f" --target ts > /tmp/out.ts
      deno run --allow-all /tmp/out.ts
    done

- name: WASM smoke
  run: |
    almide build exercises/wasm-smoke/wasm_smoke.almd --target wasm -o /tmp/smoke.wasm
    wasmtime /tmp/smoke.wasm
```

## Success Criteria

- 23 exercises, 330+ tests, 100% pass on Rust + TS + JS
- LLM (CHEATSHEET-only) achieves 90%+ first-attempt pass rate
- wasm-smoke builds and runs on wasmtime
- Remove `continue-on-error` from CI
