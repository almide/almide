# Fan floor ledger: Almide's parallel offers against Rayon, Go and Zig

How much of a parallel offer's cost is the language's? This ledger measures `fan` against three other parallel runtimes on the same programs and machines. The largest gap it shows is the next thing to fix. The sources and the runner are in [research/benchmark/fan-floor/](../../research/benchmark/fan-floor/). The cost model's own ledger is [fan-cost-model.txt](./fan-cost-model.txt).

## The probes

Every program computes `work(s, i, iters)`: `iters` steps of `acc = (acc * 31 + j) % 1000003`. It runs in OFFERS of 10 elements, and each offer's result seeds the next, so offers cannot overlap. An element costs about 4–5 ns per iteration on these machines (iters 1000 ≈ 4–5 µs).

| probe | one offer | measured as |
|---|---|---|
| `sweep` | one 10-element parallel map | t(1010 offers) − t(10 offers), per offer |
| `nest` | a 4-element parallel map whose element is itself a 10-element parallel map | t(210) − t(10), per outer offer |
| `busy` | two independent lanes at once, each running its own sequence of 10-element maps (Almide: two `fan { }` arms) | t(510) − t(10), per pair of offers |

The difference of two runs cancels process start and, on wasm, compilation. Each figure is the minimum of 7 runs, with every implementation run in turn on each repetition so they see the same machine state. Every implementation printed the same result for every row.

The implementations:

| column | what |
|---|---|
| Almide native | `almide build --release`, the default (the cost model chooses sequential or parallel per offer) |
| native seq | the same binary, `ALMIDE_FAN_SEQUENTIAL=1`. Two `fan { }` arms still run at once, so `busy` seq is one lane's time |
| Almide wasm | `almide run --target wasm`, the default. Shown for reference only: it is a wasm program, and the reference implementations are native code, so it is not ranked against them |
| Rayon | Rust 1.x + `rayon` 1.10.0: `into_par_iter().map().collect()`, nested par_iter, `rayon::join` for the lanes |
| Go | Go 1.22.2: a goroutine per element and a `sync.WaitGroup` per offer, two goroutines for the lanes |
| Zig | Zig 0.13.0: `std.Thread.Pool` + `spawnWg` + `waitAndWork`, two `std.Thread`s for the lanes |
| … seq | each language's plain loop |

`fastest` is the smallest of the six reference columns. `Almide / fastest` compares it with Almide native, and `—` marks a row where the fastest is under 1 µs (no parallel work to compare).

## Results, 2026-10-16 (Almide develop at 594a5eb81)

All figures are µs per offer.

### GCE c4-standard-8, Intel Xeon Platinum 8581C

| probe | iters | Almide native | native seq | Almide wasm | Rayon | Rayon seq | Go | Go seq | Zig | Zig seq | fastest | Almide / fastest |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|---:|
| sweep | 0 | 0.1 | 0.0 | 1.8 | 8.9 | 0.0 | 2.8 | 0.0 | 137.6 | 0.1 | 0.0 go-seq | — |
| sweep | 100 | 4.0 | 3.8 | 5.7 | 8.4 | 3.6 | 7.1 | 3.3 | 143.9 | 4.6 | 3.3 go-seq | 1.21x |
| sweep | 300 | 12.3 | 11.7 | 12.0 | 10.3 | 11.2 | 14.8 | 10.3 | 151.4 | 14.3 | 10.3 rayon | 1.20x |
| sweep | 1000 | 34.7 | 39.7 | 42.9 | 15.9 | 37.9 | 30.0 | 34.4 | 169.3 | 47.7 | 15.9 rayon | 2.19x |
| sweep | 3000 | 52.2 | 120.3 | 65.2 | 34.3 | 113.8 | 49.4 | 103.1 | 171.1 | 143.1 | 34.3 rayon | 1.52x |
| sweep | 10000 | 114.7 | 397.6 | 123.1 | 114.2 | 379.9 | 90.3 | 344.3 | 178.1 | 474.6 | 90.3 go | 1.27x |
| sweep | 30000 | 284.0 | 1192.4 | 283.5 | 273.9 | 1138.8 | 232.0 | 1030.2 | 360.3 | 1420.7 | 232.0 go | 1.22x |
| nest | 0 | 0.3 | 0.1 | 13.2 | 10.6 | 0.1 | 14.6 | 0.8 | 453.9 | 0.2 | 0.1 rayon-seq | — |
| nest | 100 | 15.5 | 15.2 | 20.7 | 13.9 | 14.5 | 26.2 | 13.6 | 469.5 | 18.5 | 13.6 go-seq | 1.14x |
| nest | 1000 | 135.4 | 159.2 | 162.1 | 31.9 | 152.4 | 64.5 | 136.8 | 595.8 | 190.7 | 31.9 rayon | 4.24x |
| nest | 10000 | 461.2 | 1591.0 | 489.6 | 215.2 | 1517.5 | 239.8 | 1381.6 | 652.7 | 1900.4 | 215.2 rayon | 2.14x |
| busy | 0 | 0.3 | 0.0 | 2.3 | 4.4 | 0.0 | 3.4 | 0.3 | 235.7 | 0.1 | 0.0 rayon-seq | — |
| busy | 100 | 4.2 | 3.7 | 8.9 | 5.9 | 7.3 | 6.7 | 6.9 | 247.2 | 9.3 | 5.9 rayon | 0.70x |
| busy | 1000 | 67.5 | 39.6 | 82.8 | 16.7 | 75.6 | 20.4 | 68.5 | 302.5 | 95.2 | 16.7 rayon | 4.03x |
| busy | 10000 | 185.6 | 397.9 | 252.2 | 147.8 | 758.8 | 101.2 | 688.1 | 316.3 | 949.8 | 101.2 go | 1.83x |

### GCE n2d-standard-8, AMD EPYC 7B13

| probe | iters | Almide native | native seq | Almide wasm | Rayon | Rayon seq | Go | Go seq | Zig | Zig seq | fastest | Almide / fastest |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|---:|
| sweep | 0 | 0.0 | 0.0 | 6.3 | 14.7 | 0.0 | 4.6 | 0.2 | 348.8 | 0.0 | 0.0 zig-seq | — |
| sweep | 100 | 5.0 | 4.7 | 19.2 | 17.3 | 4.7 | 9.9 | 4.6 | 392.5 | 5.9 | 4.6 go-seq | 1.09x |
| sweep | 300 | 16.2 | 14.7 | 27.4 | 18.8 | 14.9 | 17.2 | 14.1 | 414.9 | 17.3 | 14.1 go-seq | 1.15x |
| sweep | 1000 | 42.6 | 49.5 | 66.0 | 25.8 | 49.1 | 36.3 | 44.9 | 431.0 | 57.0 | 25.8 rayon | 1.66x |
| sweep | 3000 | 67.0 | 149.6 | 81.0 | 46.3 | 146.9 | 65.4 | 134.2 | 475.7 | 170.6 | 46.3 rayon | 1.45x |
| sweep | 10000 | 158.0 | 502.3 | 170.0 | 132.3 | 491.5 | 141.8 | 445.2 | 476.5 | 567.5 | 132.3 rayon | 1.19x |
| sweep | 30000 | 370.3 | 1501.2 | 407.1 | 397.6 | 1471.0 | 327.7 | 1338.6 | 545.5 | 1688.8 | 327.7 go | 1.13x |
| nest | 0 | 1.1 | 0.0 | 5.6 | 19.2 | 0.5 | 19.7 | 1.4 | 1397.8 | 0.0 | 0.0 zig-seq | — |
| nest | 100 | 19.3 | 18.7 | 92.7 | 22.4 | 19.7 | 32.0 | 18.2 | 1551.9 | 22.4 | 18.2 go-seq | 1.06x |
| nest | 1000 | 164.3 | 201.2 | 245.4 | 49.4 | 196.1 | 88.6 | 179.4 | 1547.0 | 227.4 | 49.4 rayon | 3.33x |
| nest | 10000 | 593.5 | 2022.9 | 631.1 | 291.4 | 1962.6 | 348.1 | 1787.0 | 1942.8 | 2265.5 | 291.4 rayon | 2.04x |
| busy | 0 | 0.0 | 0.0 | 20.7 | 4.3 | 0.0 | 4.7 | 0.4 | 573.5 | 0.1 | 0.0 rayon-seq | — |
| busy | 100 | 6.5 | 4.8 | 25.0 | 8.7 | 9.3 | 8.9 | 8.7 | 647.8 | 11.3 | 8.7 go-seq | 0.75x |
| busy | 1000 | 114.1 | 50.6 | 129.7 | 22.6 | 98.2 | 31.0 | 89.9 | 758.5 | 112.8 | 22.6 rayon | 5.05x |
| busy | 10000 | 307.2 | 502.2 | 344.3 | 180.2 | 976.5 | 143.6 | 888.3 | 931.8 | 1132.4 | 143.6 go | 2.14x |

## Reading it

- **`busy` has the largest gap: 4.0x (Intel) and 5.1x (AMD) at 1000 iterations**, and Almide is slower there than its own sequential mode (39.6 / 50.6 µs). Each lane's offers come from a `fan { }` arm's thread. Only one offer at a time could hold the resident crew (list.rs `almide_fan_run_tasks`), so the other lane spawned a thread per worker on every offer. That is the item fixed next, and the fix's own section will be appended here.
- `nest` comes next at 4.2x / 3.3x. Its outer map has an effect callback (it calls `fan { }`), and an effect `fan.map` runs its elements one after another (#2594). So only the inner maps are parallel, and 4 × 10 elements run as four 10-element offers. That is a language-surface change, not a runtime one.
- `sweep` is within 1.3x of the fastest from 10000 iterations up. Its gap is 1.7–2.2x at 1000, where Rayon's work-stealing join costs less than Almide's dealing and the cost model's choice of 5 workers.
- Zig 0.13's `std.Thread.Pool` costs 110–350 µs per offer here, and it is never the fastest.
- Go's sequential loop is ~10% faster than the others' at high iteration counts. That is code generation, not the parallel runtime, and it is why Go is the fastest at 10000+.
