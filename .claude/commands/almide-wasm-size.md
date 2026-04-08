# WASM Binary Size Measurement

Measure WASM binary sizes for standard programs and update the README.

## Steps

1. Run the measurement script:
   ```bash
   bash research/benchmark/perf/wasm-size/measure.sh
   ```

2. Compare with current README values in the "WASM Binary Size" section (~line 183)

3. Update the table if any values changed

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
- No external GC or host runtime dependency
- Built with `almide build --target wasm`
