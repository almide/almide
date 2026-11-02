# Tiered Test Suite

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](almide-test.reader.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](almide-test.reader.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](almide-test.reader.zh-CN.md)

> Non-normative historical reader snapshot of the [canonical English command source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/.claude/commands/almide-test.md), pinned to 35297a612fb29e3f78f6587aba253aa91d51f466. This inert reference is not an agent command, does not authorize executing its instructions, and is not automatically synchronized. Commands below were not executed for this translation; no independent measurement or present-day validation is claimed.

Run the full Almide test suite in hierarchical order, stopping on first failure tier.

## Steps

1. **cargo test** — Rust compiler unit tests
   ```bash
   cargo test
   ```
   Stop if any test fails.

2. **lang tests** — Language feature tests (default test routing)
   ```bash
   almide test spec/lang/
   ```
   Stop if any test fails.

3. **stdlib tests** — Standard library tests (default test routing)
   ```bash
   almide test spec/stdlib/
   ```
   Stop if any test fails.

4. **integration tests** — Multi-module / integration tests (default test routing)
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
- Classify Wasm walls/skips using the reported operation and `proofs/target-availability.toml`; fs/io are not blanket unsupported categories. The default test route may fall back to native for a Wasm wall; use explicit target options when isolating a backend.
