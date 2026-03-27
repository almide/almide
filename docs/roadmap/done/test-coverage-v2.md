<!-- description: Cross-target test coverage status (Rust/WASM/TS) -->
# Test Coverage

**Current**: 129 test files, 2,042 .almd test blocks (Rust target). All 129 pass on WASM target too.

## Cross-target coverage

| Target | Files | Pass | Fail | Skip |
|--------|------:|-----:|-----:|-----:|
| Rust   | 130   | 130  | 0    | 0    |
| WASM   | 130   | 130  | 0    | 0    |
| TS     | 130   | ~127 | ~3   | 0    |

## Remaining

- Cross-target parity tests (TS target)
- Deeper cross-cutting tests (match inside for inside do, pipe with fan)
- Boundary value tests (Int max/min, deeply nested structures)
- Error message quality tests
- More stdlib edge cases (datetime, process, fs — requires effect fn / I/O)
