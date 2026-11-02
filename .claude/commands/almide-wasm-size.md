# WASM Binary Size Measurement

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
