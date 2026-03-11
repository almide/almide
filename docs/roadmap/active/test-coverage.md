# Test Coverage [ACTIVE]

Current: 48 test files, 790 test cases. All passing.

## Goal

Reach **1500+ test cases** with systematic coverage of all language features and stdlib functions. Focus on edge cases that LLMs are likely to hit.

## Coverage Targets

| Area | Current | Target | Notes |
|------|---------|--------|-------|
| Language features (lang/) | ~300 | 500 | expressions, control flow, data types, pattern matching |
| Stdlib (stdlib/) | ~400 | 700 | all 203 functions, edge cases |
| Codegen correctness | ~50 | 150 | borrow inference, move analysis, generics emit |
| Error diagnostics | ~40 | 100 | every error path produces correct message |
| Cross-target (Rust + TS) | 0 | 50 | same .almd runs on both targets, same output |

## Priority Areas

### 1. Borrow inference edge cases
- Recursive functions with borrowed params
- Deeply nested call chains (A → B → C, all borrow)
- Mixed borrow/owned params in same function
- Lambda capture with borrowed outer params
- Module-crossing borrow chains

### 2. Generics edge cases
- Recursive generic variants (Tree, LinkedList)
- Generic functions calling generic functions
- Type inference across multiple call sites
- Generic variant + pattern matching exhaustiveness

### 3. Stdlib boundary tests
- Every stdlib function with empty input (`""`, `[]`, `{}`)
- Every stdlib function with large input (1000+ elements)
- Every stdlib function with Unicode input
- Chained operations: `list.map |> list.filter |> list.reduce`

### 4. LLM-likely patterns
- Patterns from benchmark failures
- Common LLM mistakes: wrong arg order, missing import, type mismatch
- Verify error messages guide toward correct fix

## Metrics

Track with `almide test 2>&1 | grep "^test " | wc -l` after each release.

| Version | Test Files | Test Cases |
|---------|-----------|------------|
| v0.5.0 | 48 | 790 |
