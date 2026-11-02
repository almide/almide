# Native Performance Measurement

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](almide-native-perf.reader.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](almide-native-perf.reader.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](almide-native-perf.reader.zh-CN.md)

> Non-normative historical reader snapshot of the [canonical English command source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/.claude/commands/almide-native-perf.md), pinned to 35297a612fb29e3f78f6587aba253aa91d51f466. This inert reference is not an agent command, does not authorize executing its instructions, and is not automatically synchronized. Commands below were not executed for this translation; no independent measurement or present-day validation is claimed.

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
