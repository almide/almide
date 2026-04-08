# Native Performance Measurement

Measure native binary size and runtime performance, then update the README.

## Steps

1. Run the measurement script:
   ```bash
   bash research/benchmark/perf/native/measure.sh
   ```

2. Compare with current README values in the "Native Performance" section (~line 195)

3. Update the table if any values changed

4. Commit: `Update native performance stats: {brief summary}`

## What It Measures

- **Binary size**: Stripped native binary of a real CLI app (minigit, ~350 LOC)
- **Runtime**: 100 init+add+commit operations against the CLI binary
- **Dependencies**: Always 0 (Almide compiles to a single static binary)

## Source

- CLI app: `research/benchmark/perf/native/cli_app.almd`
- Script: `research/benchmark/perf/native/measure.sh`
