<!-- description: Systematic language feature test suite for regression detection -->
<!-- done: 2026-03-11 -->
# Language Test Suite

A systematic test suite for Almide language features. Located in `lang/`.

## Goal

Regression detection when modifying the compiler. With tests covering all features, refactoring can be done with confidence.

## Structure

One file per category. Each file consists of `test "..." { assert_eq(...) }` blocks.

```
exercises/lang-test/
  expr_test.almd           Basic expressions (arithmetic, comparison, logic, string)
  control_flow_test.almd   Control flow (if, match, for, do/guard)
  data_types_test.almd     Data types (record, tuple, variant, Option, Result)
  function_test.almd       Functions (pure fn, effect fn, lambda, recursion, UFCS)
  variable_test.almd       Variables (let, var, assignment, index assign, field assign)
  pattern_test.almd        Pattern matching (literal, constructor, guard, nested)
  operator_test.almd       Operators (++, |>, ==, !=, comparison, bitwise)
  type_system_test.almd    Types (type alias, generics, newtype, variant)
  error_test.almd          Error handling (ok/err, effect fn, auto-?, do block)
  string_test.almd         Strings (interpolation, heredoc, escapes)
  scope_test.almd          Scope (shadowing, closure, nested blocks)
  edge_cases_test.almd     Edge cases (empty collections, large numbers, boundary values)
```

## Categories

### 1. expr_test — Basic Expressions
- [ ] Integer arithmetic (+, -, *, /, %)
- [ ] Floating-point arithmetic
- [ ] Operator precedence (`2 + 3 * 4 == 14`)
- [ ] Unary minus (`-x`)
- [ ] Comparison operators (<, >, <=, >=, ==, !=) Int/String/Bool
- [ ] Logical operators (and, or, not)
- [ ] Short-circuit evaluation (and/or)
- [ ] String concatenation (++)
- [ ] List concatenation (++)
- [ ] Parenthetical grouping

### 2. control_flow_test — Control Flow
- [ ] if/then/else (returns value)
- [ ] Nested if
- [ ] if/then/else with block
- [ ] match with literal patterns (Int, String, Bool)
- [ ] match with Option (some/none)
- [ ] match with Result (ok/err)
- [ ] match with wildcard (_)
- [ ] match with guard conditions
- [ ] for...in list
- [ ] for...in with tuple destructuring (enumerate, zip)
- [ ] for...in with var accumulation
- [ ] do { guard } loop
- [ ] guard else break inside do block
- [ ] Nested for/do

### 3. data_types_test — Data Types
- [ ] Record literal and field access
- [ ] Nested records
- [ ] Record spread ({ ...base, field: v })
- [ ] Tuple literal and index access (.0, .1)
- [ ] Tuple destructuring (let (a, b) = ...)
- [ ] Variant type definition and construction
- [ ] Variant with zero args, tuple, and record payloads
- [ ] Option[T] creation and decomposition
- [ ] Result[T, E] creation and decomposition
- [ ] List[T] basic operations
- [ ] Map[K, V] basic operations

### 4. function_test — Functions
- [ ] Simple function definition and call
- [ ] Multiple arguments
- [ ] No-argument functions
- [ ] Recursion (factorial, fibonacci)
- [ ] Mutual recursion
- [ ] Higher-order functions (function as argument)
- [ ] Functions returning functions
- [ ] Lambda (fn(x) => expr)
- [ ] Lambda with block body
- [ ] Lambda in map/filter/fold
- [ ] Closure (capturing outer variables)
- [ ] UFCS (`x.f(y)` == `f(x, y)`)
- [ ] UFCS chaining (`x.f().g()`)

### 5. variable_test — Variables
- [ ] let (immutable binding)
- [ ] let with type annotation
- [ ] var (mutable binding) and reassignment
- [ ] index assign (xs[i] = v)
- [ ] field assign (r.f = v)
- [ ] var in for loop
- [ ] let shadowing (rebinding with same name)

### 6. pattern_test — Pattern Matching
- [ ] Literal patterns (Int, String, Bool)
- [ ] Identifier patterns (binding)
- [ ] Wildcard (_)
- [ ] some(x) / none patterns
- [ ] ok(x) / err(e) patterns
- [ ] Constructor patterns (user-defined variant)
- [ ] Tuple patterns ((a, b))
- [ ] Record patterns ({ field1, field2 })
- [ ] Nested patterns (some((a, b)))
- [ ] Guarded patterns (pattern if condition)

### 7. operator_test — Operators
- [ ] ++ string concatenation
- [ ] ++ list concatenation
- [ ] |> pipe operator
- [ ] |> chaining
- [ ] == / != deep equality (lists, records)
- [ ] ^ exponentiation
- [ ] Bitwise operations (int.band, int.bor, int.bxor, int.bshl, int.bshr, int.bnot)

### 8. type_system_test — Type System
- [ ] type alias (basic types)
- [ ] type alias (generic types)
- [ ] Variant type definition
- [ ] Variant with multiple constructors
- [ ] Generic variant
- [ ] Newtype (with deriving)
- [ ] Generic record type

### 9. error_test — Error Handling
- [ ] ok(value) / err(error) construction
- [ ] match on Result
- [ ] auto-? inside effect fn (error propagation)
- [ ] Automatic Result unwrap inside do block
- [ ] guard else err(...)
- [ ] effect fn chain (multiple fallible operations)

### 10. string_test — Strings
- [ ] String interpolation (${expr})
- [ ] Expression inside interpolation (${x + 1})
- [ ] Function call inside interpolation (${f(x)})
- [ ] Heredoc ("""...""")
- [ ] Heredoc indent stripping
- [ ] Escape sequences (\n, \t, \\, \")

### 11. scope_test — Scope
- [ ] Block scope
- [ ] Shadowing in nested blocks
- [ ] for loop variable scope
- [ ] Scope of bindings inside match arms
- [ ] Closure variable capture

### 12. edge_cases_test — Edge Cases
- [ ] Empty list operations (len, get, map, filter)
- [ ] Empty string operations
- [ ] Empty map operations
- [ ] list.get out of bounds → none
- [ ] Division by zero
- [ ] Large integers
- [ ] Nested compound types (List[List[Int]], Map[String, List[Int]])

## Implementation Order

1. expr_test + control_flow_test (foundation)
2. data_types_test + variable_test (data)
3. function_test + pattern_test (functions and patterns)
4. operator_test + type_system_test (operators and types)
5. error_test + string_test (errors and strings)
6. scope_test + edge_cases_test (scope and edge cases)

## Principles

- 1 test = verify 1 behavior
- Test names concisely describe what is verified in English
- Use assert_eq as the default, assert only for condition checks
- Each file can be run independently with `almide test file.almd`
- Run all files in CI
