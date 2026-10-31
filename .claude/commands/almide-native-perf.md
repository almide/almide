# Native Performance Measurement

Measure the sample native CLI; publish only with compiler/build, host, and date provenance.

## Steps

1. Run the measurement script:
   ```bash
   bash research/benchmark/perf/native/measure.sh
   ```

2. Compare with the matching dated measurement, if any, in `docs/benchmarks/README.md` or its linked ledgers; the old front-README section is gone.

3. Update the appropriate source measurement and regenerate derived blocks where applicable; do not edit generated tables by hand.

4. Commit: `Update native performance stats: {brief summary}`

## What It Measures

- **Binary size**: Stripped native binary of a real CLI app (minigit, ~350 LOC)
- **Runtime**: 100 init+add+commit operations against the CLI binary
- **Dependencies**: The script prints a literal zero, not a linkage measurement. Inspect the produced artifact before making static-linkage or dependency claims.

## Source

- CLI app: `research/benchmark/perf/native/cli_app.almd`
- Script: `research/benchmark/perf/native/measure.sh`
