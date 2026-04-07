# Tiered Test Suite

Run the full Almide test suite in hierarchical order, stopping on first failure tier.

## Steps

1. **cargo test** — Rust compiler unit tests
   ```bash
   cargo test
   ```
   Stop if any test fails.

2. **lang tests** — Language feature tests (Rust target)
   ```bash
   almide test spec/lang/
   ```
   Stop if any test fails.

3. **stdlib tests** — Standard library tests (Rust target)
   ```bash
   almide test spec/stdlib/
   ```
   Stop if any test fails.

4. **integration tests** — Multi-module / integration tests (Rust target)
   ```bash
   almide test spec/integration/
   ```
   Stop if any test fails.

5. **WASM tests** — All tests with WASM target
   ```bash
   almide test --target wasm
   ```
   Report passed/failed/skipped.

6. **Summary** — Report results for each tier:
   - cargo test: pass/fail
   - lang: N files passed
   - stdlib: N files passed
   - integration: N files passed
   - WASM: N passed, N failed, N skipped

## Notes

- Always `make install` before running if compiler source was modified
- If a tier fails, diagnose and fix before proceeding to the next tier
- The WASM tier has some expected skips (exercises requiring fs/io)
