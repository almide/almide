# Test Coverage: Systematic Edge Case Tests per Grammar Construct

**Goal**: Every grammar construct in SPEC.md has a dedicated test file with edge cases. Target: 2,500+ test blocks.

**Current**: 117 test files, 1,653 .almd test blocks + 639 Rust unit tests = 2,292 total (92% of target).

## Completed — Phase 1 + 2

### Phase 1: Fill missing constructs (5 files, 47 tests)
- [x] `trait_impl_test.almd` — 13 tests: convention methods, protocols, UFCS
- [x] `newtype_test.almd` — 5 tests: type alias behavior
- [x] `visibility_test.almd` — 6 tests: pub/local/mod, effect fn
- [x] `import_test.almd` — 8 tests: stdlib imports, UFCS without import, math/json
- [x] `while_test.almd` — 10 tests: basic, false, nested, break, accumulator

### Phase 2: Deepen thin coverage (9 files, 77 tests)
- [x] `tuple_test.almd` — 10 tests: creation, destructure, index, nested, match, for
- [x] `range_test.almd` — 10 tests: basic, inclusive, zero, variables, nested
- [x] `pipe_test.almd` — 6 tests: basic, chain, function, list, string
- [x] `lambda_test.almd` — 10 tests: capture, map/filter, fold, block body, chaining
- [x] `heredoc_test.almd` — 10 tests: heredoc, raw, single-quote, escapes, interpolation
- [x] `do_guard_test.almd` — 9 tests: break, accumulator, nested, continue, while guard
- [x] `for_test.almd` — 10 tests: list, range, tuple, map, nested, guard, empty
- [x] `record_spread_test.almd` — 9 tests: creation, spread, equality, conditions
- [x] `map_edge_test.almd` — 10 tests: empty, get, contains, keys, iteration, is_empty

### Phase 2b: Cross-cutting (8 files, 126 tests)
- [x] `generics_test.almd` — 16 tests: generic fn, record, variant, multi-param
- [x] `match_edge_test.almd` — 19 tests: int/string/option/result match, guards, nested
- [x] `string_interp_test.almd` — 15 tests: math, field, call, conditional, multi
- [x] `effect_fn_test.almd` — 15 tests: auto-?, guard, chain, loop, conditional
- [x] `equality_test.almd` — 23 tests: deep == on all types, !=, nested
- [x] `type_annotation_test.almd` — 14 tests: scalar, empty list/map, Option, Result, fn

### Phase 2c: Stdlib module tests (10 files, ~130 tests)
- [x] `math_test.almd` — 16 tests: abs, pow, fpow, pi, e, sqrt, sin, cos, log, sign, fmin/fmax
- [x] `result_test.almd` — 13 tests: is_ok, is_err, unwrap_or, map, match, equality
- [x] `regex_test.almd` — 15 tests: is_match, find, find_all, replace, split, captures
- [x] `random_test.almd` — 14 tests: int range, float range, choice, shuffle
- [x] `value_test.almd` — 10 tests: int, str, bool, float, null, array, object, stringify
- [x] `error_test.almd` — 9 tests: effect fn error patterns, guard, chaining
- [x] `option_extra_test.almd` — 15 tests: map, flat_map, unwrap_or, filter, zip, to_list
- [x] `list_extra_test.almd` — 20 tests: take, drop, enumerate, zip, find, any, all, fold
- [x] `string_extra_test.almd` — 17 tests: pad, trim, repeat, replace, chars, index_of
- [x] `int_extra_test.almd` — 10 tests: parse, abs, min, max, clamp, to_hex, from_hex
- [x] `float_extra_test.almd` — 10 tests: sign, parse, abs, clamp, to_string
- [x] `json_test.almd` — 14 tests: from_string/int/float/bool, object, array, parse, stringify

## Remaining to reach 2,500

~208 tests needed to reach 2,500. Options:
- More stdlib edge cases (datetime, process, fs — requires effect fn / I/O)
- Deeper cross-cutting tests (match inside for inside do, pipe with fan)
- Boundary value tests (Int max/min, deeply nested structures)
- Error message quality tests
- Cross-target tests (verify TS/JS output)
