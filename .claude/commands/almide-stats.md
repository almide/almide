# Update README Stats

Measure current quantitative data and update README.md.

## Metrics to Measure

1. **Stdlib functions and modules**
   ```bash
   grep -c '^\[' stdlib/defs/*.toml | awk -F: '{sum += $2} END {print sum " functions across " NR " modules"}'
   ```

2. **Test file counts**
   ```bash
   almide test 2>&1 | grep "All .* passed"           # Rust target
   almide test --target wasm 2>&1 | tail -3           # WASM target
   ```

3. **MSR** — Only if `/almide-msr` was run recently. Otherwise note the last known value.

## README Locations to Update

- Line ~82: `Standard library — N functions across M modules`
- Line ~209: `| Stdlib | N functions across M modules |`
- Line ~210: `| Tests | N test files pass (Rust), M pass (WASM) |`
- Line ~211: `| MSR | N/M exercises pass (...) |`

## Steps

1. Measure all metrics above
2. Compare with current README values
3. Update only values that changed
4. Commit: `Update README stats: {brief summary of changes}`
