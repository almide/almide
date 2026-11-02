# WASM Binary Size Measurement

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](almide-wasm-size.reader.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](almide-wasm-size.reader.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](almide-wasm-size.reader.zh-CN.md)

> Non-normative historical reader snapshot of the [canonical English command source](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/.claude/commands/almide-wasm-size.md), pinned to 35297a612fb29e3f78f6587aba253aa91d51f466. This inert reference is not an agent command, does not authorize executing its instructions, and is not automatically synchronized. Commands below were not executed for this translation; no independent measurement or present-day validation is claimed.

Measure WASM binary sizes for these samples; keep measurements distinct from the generated Hello, world baseline.

## Steps

1. Run the measurement script:
   ```bash
   bash research/benchmark/perf/wasm-size/measure.sh
   ```

2. Compare with matching dated entries in `docs/benchmarks/README.md` and linked ledgers; the old front-README section is gone.

3. For the published Hello, world baseline use `scripts/gen-readme-stats.sh --measure`; keep other sample results separately dated and do not overwrite generated blocks.

4. Commit: `Update WASM binary sizes: {brief summary}`

## Source Files

Located in `research/benchmark/perf/wasm-size/`:
- `hello.almd` — Hello World
- `fizzbuzz.almd` — FizzBuzz
- `fibonacci.almd` — Fibonacci
- `closure.almd` — Closure (higher-order function)
- `variant.almd` — Variant (algebraic data type + pattern matching)

## Notes

- All binaries are self-contained (allocator, string handling, runtime included)
- No external GC is required for guest ownership; executing host operations still requires a compatible Wasm runtime and the artifact's imports.
- Built with `almide build --target wasm`
