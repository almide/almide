<!-- description: Reform ++ concatenation operator for strings and lists -->
<!-- done: 2026-03-18 -->
# Concatenation Operator Reform

**Priority:** High — breaking change candidate before 1.0
**Research:** [docs/research/concat-operators.md](../../research/concat-operators.md)

## Current state

`++` is used for both String and List concatenation (Elm/Haskell style).

```almide
"Hello, " ++ name ++ "!"   // String
[1, 2] ++ [3, 4]           // List
```

## Problem

1. Most String concatenation can be written with interpolation `"Hello, ${name}!"` — `++` is redundant
2. `++` is unfamiliar to LLMs (not used outside Elm/Haskell)
3. JSON construction produces code like `'"' ++ key ++ '":"' ++ val ++ '"'`

## Research conclusions

- **`+` overload (Python/Kotlin/Swift)**: LLMs are familiar, but `1 + "a"` problem
- **Different operators (Gleam `<>`, OCaml `^`/`@`, Elixir `<>`/`++`)**: Clear but LLM learning cost
- **Keep `++` (Elm/Haskell)**: Type-safe, polymorphic. **No language that adopted this has regretted it**
- **In languages with interpolation, concat operator importance decreases** — universal trend across all languages

## Options

| Option | String | List | Compatibility | LLM affinity |
|---|---|---|---|---|
| A. Keep `++` | `++` | `++` | No change | Medium |
| B. `++` for List only, String uses interpolation | Interpolation `"${a}${b}"` | `++` | breaking | High |
| C. Unify to `+` | `+` | `+` | breaking | Highest |
| D. Gleam-style `<>` | `<>` | `++` or `list.concat` | breaking | Medium |

## Recommended: A (keep) or B (List-only)

### Case for A
- **Zero languages that adopted `++` have regretted it** (research conclusion)
- With interpolation, String `++` usage frequency is already low
- Weak justification for taking breaking change risk before 1.0
- LLMs have already learned `++` as "Almide's concatenation operator"

### Case for B
- Forcing String concatenation to interpolation creates **one right way**
- `++` semantics become limited to "List concatenation" — clear
- However, making `"a" ++ "b"` a compile error breaks existing code

## Decision criteria

1. How many places in existing exercises / spec / showcase use `++` for String
2. Whether all of them can be replaced with interpolation
3. Whether it affects LLM first-attempt accuracy

## TODO

- [ ] Count String usage sites of `++`
- [ ] Determine if replaceable with interpolation
- [ ] Decide: A or B
- [ ] (If B) String `++` deprecation warning → error
