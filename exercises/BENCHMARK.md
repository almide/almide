# Almide LLM Benchmark Results

## Overview

Can an LLM write correct Almide code with **zero prior knowledge**, using only the [CHEATSHEET.md](../CHEATSHEET.md)?

We tested this by having an LLM agent (Claude) solve 14 Exercism-style exercises from scratch, given only the CHEATSHEET as reference. The same exercises were also solved in Python for comparison.

## Results

### Almide (CHEATSHEET only, zero knowledge)

| Exercise | Tests | Passed | Failed | Notes |
|---|---|---|---|---|
| affine-cipher | 16 | 16 | 0 | |
| bob | 25 | 25 | 0 | |
| collatz-conjecture | 6 | 6 | 0 | |
| config-merger | 27 | 27 | 0 | `effect fn` + file I/O error propagation |
| grade-report | 30 | 30 | 0 | Multi-function error propagation |
| hamming | 9 | 9 | 0 | |
| isbn-verifier | 14 | 12 | 2 | Result erasure + fold accumulator limitation |
| isogram | 13 | 13 | 0 | |
| pangram | 10 | 10 | 0 | |
| phone-number | 14 | 14 | 0 | |
| pipeline | 36 | 36 | 0 | `do` block auto error propagation |
| raindrops | 18 | 18 | 0 | |
| roman-numerals | 19 | 19 | 0 | |
| scrabble-score | 11 | 11 | 0 | |
| **Total** | **248** | **246** | **2** | **99.2%** |

### Python (baseline comparison)

| Exercise | Tests | Passed | Notes |
|---|---|---|---|
| affine-cipher | 16 | 16 | |
| bob | 25 | 25 | |
| collatz-conjecture | 6 | 6 | |
| config-merger | 27 | 27 | |
| grade-report | 30 | 30 | |
| hamming | 9 | 9 | |
| isbn-verifier | 14 | 14 | |
| isogram | 13 | 13 | |
| pangram | 10 | 10 | |
| phone-number | 14 | 14 | |
| pipeline | 36 | 36 | |
| raindrops | 18 | 18 | |
| roman-numerals | 19 | 19 | |
| scrabble-score | 11 | 11 | |
| **Total** | **248** | **248** | **100%** |

## Methodology

1. **Exercise design**: Each exercise provides function signatures with `= _` (hole stubs) and test cases. The LLM must implement all functions.
2. **Almide agent**: Given ONLY `CHEATSHEET.md` — no other documentation, no examples of Almide code, no access to the transpiler source.
3. **Python agent**: Standard Python knowledge, no special constraints.
4. **Test runner**: Almide files are transpiled to TypeScript via `src/almide.ts` and tested with `deno test`. Python files are tested with `pytest`.
5. **Single attempt**: Each exercise was solved in one pass without retries or feedback loops.

## Key Findings

### The CHEATSHEET is sufficient
An LLM can write correct Almide code from zero knowledge using only a 340-line quick reference. 246/248 tests pass (99.2%).

### Known limitation: Result erasure + fold accumulator
The 2 failing tests in isbn-verifier involve using `err()` as a fold accumulator value. Due to Result erasure (where `err(e)` compiles to `throw`), error values cannot be accumulated inside `list.fold`. This is a fundamental design trade-off of the erasure approach.

### Almide advantages visible in complex exercises

**`do` block auto error propagation** (pipeline exercise):
```
fn run_pipeline(input: String) -> Result[String, String] = do {
  let pairs = parse_pairs(input)       // auto-unwrap Result
  let pairs2 = validate_keys(pairs)    // error → propagate
  let pairs3 = validate_values(pairs2) // error → propagate
  let pairs4 = transform(pairs3)       // error → propagate
  format_output(pairs4)
}
```
vs Python (implicit exception propagation, no type-level error contract):
```python
def run_pipeline(text):
    pairs = parse_pairs(text)
    pairs = validate_keys(pairs)
    pairs = validate_values(pairs)
    pairs = transform(pairs)
    return format_output(pairs)
```

**`match` with `err(e)` pattern** (grade-report exercise):
```
match parse_student(line) {
  ok(student) => { ... },
  err(e) => err("line " ++ line_num ++ ": " ++ e),  // catch and re-wrap
}
```

**`effect fn` with file I/O error propagation** (config-merger exercise):
```
effect fn merge_and_save(paths: List[String], output: String) -> Result[Int, String] = {
  match merge_files(paths) {       // file read errors propagate automatically
    err(e) => err(e),
    ok(pairs) => {
      save_config(output, pairs);  // file write errors propagate
      ok(list.len(pairs))
    }
  }
}
```
vs Python (exceptions are implicit, no type-level contract):
```python
def merge_and_save(paths, output):
    merged = merge_files(paths)     # FileNotFoundError? ValueError? Unknown
    save_config(output, merged)     # IOError? Unknown
    return len(merged)
```

### Result erasure constraint on test patterns
Due to Result erasure, `let result = fn_returning_err(); match result { ... }` does not work — the error throws at assignment before `match` runs. Instead, match directly on the call: `match fn_returning_err() { ok(v) => ..., err(e) => ... }`. Similarly, use `assert_eq(expr, err("msg"))` to test error cases.

### stdlib note
`string.to_int`, `string.to_upper`, `string.to_lower` are now in the stdlib, reducing boilerplate from earlier exercises.

## Running the benchmark

```bash
# Run a single exercise
bash exercises/run_exercise.sh exercises/pipeline/pipeline.almd

# Run all exercises
for f in exercises/*/*.almd; do
  echo "=== $(basename $(dirname $f)) ==="
  bash exercises/run_exercise.sh "$f"
done
```
