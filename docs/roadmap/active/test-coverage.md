# Test Coverage: Systematic Edge Case Tests per Grammar Construct

**Goal**: Every grammar construct in SPEC.md has a dedicated test file with edge cases. Target: 2,500+ test blocks (currently 1,487).

**Status**: 649 test blocks in `spec/lang/`, ~838 in exercises/stdlib/showcases.

## Current Coverage

### Well-covered (20+ tests, edge cases included)
- [x] if/then/else — `control_flow_test.almd`
- [x] match/patterns — `pattern_test.almd` + `control_flow_test.almd`
- [x] let/var/scope — `variable_test.almd` + `scope_test.almd`
- [x] functions — `function_test.almd`
- [x] operators — `operator_test.almd`
- [x] strings/interpolation — `string_test.almd` + `interpolation_edge_test.almd`
- [x] error/Result/effect — `error_test.almd`
- [x] type system — `type_system_test.almd`
- [x] codec/derive — 8 dedicated test files
- [x] fan concurrency — `fan_test.almd` + `fan_map_test.almd` + `fan_race_test.almd`
- [x] top-level let — `top_let_test.almd`
- [x] default/named args — `default_args_test.almd` + `named_args_test.almd`
- [x] single-quote strings — `single_quote_test.almd`

### Thin coverage (< 10 dedicated tests, needs edge cases)
- [ ] **while loops** — ~3 tests in control_flow_test. Needs: break, continue, nested while, while with mutation, while false (zero iterations), infinite loop guard
- [ ] **map literals** — 11 tests. Needs: empty map typed, nested maps, map with complex keys, map iteration order, map spread
- [ ] **tuple** — ~5 in data_types. Needs: tuple destructure, tuple index, nested tuples, tuple in match, tuple as return, single-element ambiguity
- [ ] **range** — ~3 in expr_test. Needs: 0..0, negative range, range in for, range with variables, ..= inclusive, range of large numbers
- [ ] **pipe** — ~3 in expr_test. Needs: multi-pipe chain, pipe with lambda, pipe with module fn, pipe into method, pipe Result propagation
- [ ] **heredoc/raw strings** — ~3 in block_comment. Needs: heredoc indentation, heredoc interpolation, raw string with special chars, triple-quote edge cases
- [ ] **lambda edge cases** — ~5 in function_test. Needs: lambda capture var, lambda in map/filter chain, lambda returning lambda, multi-line lambda body, lambda type annotation
- [ ] **do/guard** — ~10 across files. Needs: guard with ok/err, guard else break, nested do, do as expression value, guard in while
- [ ] **for...in** — ~10 in control_flow. Needs: for with break/continue, for tuple destructure, for over range, for over map, nested for, for with var mutation
- [ ] **record/spread** — ~15 across files. Needs: spread with override, spread type inference, nested records, record update syntax, empty record

### Missing entirely (no dedicated test file)
- [ ] **trait/impl** — No tests at all. Needs: trait declaration, impl for type, trait method call, multiple traits, trait with generics
- [ ] **newtype** — No tests. Needs: newtype declaration, newtype wrapping/unwrapping, newtype in match
- [ ] **unsafe** — No tests. Needs: unsafe block syntax, unsafe in effect fn
- [ ] **import/module** — No spec tests (only tested via almide-grammar integration). Needs: import stdlib, import self, import alias, import submodule, unused import warning
- [ ] **visibility (pub/mod/local)** — No tests. Needs: pub fn access, local fn restriction, mod fn access, default visibility

## Plan

### Phase 1: Fill missing (0 → basic)
Create dedicated test files for the 5 missing constructs:
1. `trait_impl_test.almd`
2. `newtype_test.almd`
3. `unsafe_test.almd`
4. `import_module_test.almd` (what's testable without multi-file)
5. `visibility_test.almd` (what's testable in single file)

### Phase 2: Deepen thin coverage
Add edge case tests to existing files or create new files:
6. `while_test.almd` — extract and expand from control_flow_test
7. `tuple_test.almd` — extract and expand from data_types_test
8. `range_test.almd` — extract and expand from expr_test
9. `pipe_test.almd` — extract and expand from expr_test
10. `map_edge_test.almd` — expand map_literal_test
11. `heredoc_test.almd` — expand from block_comment_raw_string_test
12. `lambda_test.almd` — extract and expand from function_test
13. `do_guard_test.almd` — expand from do_block_pure_test
14. `for_test.almd` — extract and expand from control_flow_test
15. `record_spread_test.almd` — expand from edge_cases_test

### Phase 3: Cross-cutting edge cases
16. Interaction tests (e.g., match inside for inside do, pipe with fan, lambda capturing while var)
17. Error message quality tests (verify hints are correct)
18. Boundary values (Int max/min, empty string, empty list, deeply nested)

## Target
- Phase 1: +50 tests (5 files × 10 tests)
- Phase 2: +150 tests (10 files × 15 tests)
- Phase 3: +50 tests
- **Total**: 649 + 250 = ~900 in spec/lang/, ~1,750 total (closer to 2,500 goal)
