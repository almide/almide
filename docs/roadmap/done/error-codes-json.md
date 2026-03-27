<!-- description: Structured error codes (E001-E010) and JSON diagnostic output -->
<!-- done: 2026-03-17 -->
# Error Codes + JSON Output [DONE — 1.0 Phase II]

## Implemented

- [x] Error code system: E001-E010 (type mismatch, undefined function/variable, arg count/type, effect isolation, fan restrictions, assign to immutable, non-exhaustive match)
- [x] `almide check --json`: structured JSON output, 1 diagnostic per line
- [x] `almide check --explain E001`: detailed explanation per error code
- [x] `almide test --json`: structured test results, 1 file per line
- [x] Check speed: 14ms (debug) / 25ms (release) for 298 lines — well under 1 second for 500 lines
