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

- **`busy` has the largest gap: 4.0x (Intel) and 5.1x (AMD) at 1000 iterations**, and Almide is slower there than its own sequential mode (39.6 / 50.6 µs). Each lane's offers come from a `fan { }` arm's thread. Only one offer at a time could hold the resident crew (list.rs `almide_fan_run_tasks`), so the other lane spawned a thread per worker on every offer. That is the item fixed next: see **After** below.
- `nest` comes next at 4.2x / 3.3x. Its outer map has an effect callback (it calls `fan { }`), and an effect `fan.map` runs its elements one after another (#2594). So only the inner maps are parallel, and 4 × 10 elements run as four 10-element offers. That is a language-surface change, not a runtime one.
- `sweep` is within 1.3x of the fastest from 10000 iterations up. Its gap is 1.7–2.2x at 1000, where Rayon's work-stealing join costs less than Almide's dealing and the cost model's choice of 5 workers.
- Zig 0.13's `std.Thread.Pool` costs 110–350 µs per offer here, and it is never the fastest.
- Go's sequential loop is ~10% faster than the others' at high iteration counts. That is code generation, not the parallel runtime, and it is why Go is the fastest at 10000+.

## After: one shared pool, the caller taking blocks too (2026-10-16)

Every parallel offer now posts helper calls to one resident pool (list.rs `almide_fan_deal` / `almide_fan_pool`), whichever thread it comes from, a `fan { }` arm's included. The calling thread takes blocks of its own offer as well. A helper that starts after the blocks ran out takes none. The caller waits only for helpers already inside the offer's gate, so no offer waits on a helper that is busy elsewhere. Helper calls go to the lowest-numbered idle pool threads, so a run of single offers keeps the same warm threads; handing them out in turn cost 4–15% per offer on the AMD machine.

Same runs, same probes; `develop` / `this` are the default (cost model) build of each, and `forced par` is this build with `ALMIDE_FAN_COST_OFF=1`; µs per offer:

### GCE c4-standard-8, Intel Xeon Platinum 8581C

| probe | iters | develop | this | forced par | fastest | develop / fastest | this / fastest | gap before | gap after |
|---|---:|---:|---:|---:|---|---:|---:|---:|---:|
| sweep | 0 | 0.1 | 0.0 | 8.2 | 0.0 go-seq | — | — | +0.0 | -0.1 |
| sweep | 100 | 4.0 | 3.9 | 12.2 | 3.3 go-seq | 1.21x | 1.19x | +0.7 | +0.6 |
| sweep | 300 | 12.3 | 12.1 | 15.8 | 10.3 rayon | 1.20x | 1.18x | +2.0 | +1.8 |
| sweep | 1000 | 34.7 | 21.8 | 31.3 | 15.9 rayon | 2.19x | 1.37x | +18.8 | +5.9 |
| sweep | 3000 | 52.2 | 45.3 | 41.5 | 34.3 rayon | 1.52x | 1.32x | +17.9 | +11.0 |
| sweep | 10000 | 114.7 | 107.5 | 97.9 | 90.3 go | 1.27x | 1.19x | +24.4 | +17.2 |
| sweep | 30000 | 284.0 | 268.4 | 258.4 | 232.0 go | 1.22x | 1.16x | +52.0 | +36.3 |
| nest | 0 | 0.3 | 0.3 | 33.6 | 0.1 rayon-seq | — | — | +0.2 | +0.2 |
| nest | 100 | 15.5 | 15.8 | 48.9 | 13.6 go-seq | 1.14x | 1.16x | +2.0 | +2.2 |
| nest | 1000 | 135.4 | 112.2 | 126.7 | 31.9 rayon | 4.24x | 3.51x | +103.5 | +80.3 |
| nest | 10000 | 461.2 | 427.6 | 398.4 | 215.2 rayon | 2.14x | 1.99x | +246.0 | +212.4 |
| busy | 0 | 0.3 | 0.2 | 18.3 | 0.0 rayon-seq | — | — | +0.3 | +0.2 |
| busy | 100 | 4.2 | 4.1 | 22.7 | 5.9 rayon | 0.70x | 0.70x | -1.7 | -1.8 |
| busy | 1000 | 67.5 | 25.4 | 35.2 | 16.7 rayon | 4.03x | 1.52x | +50.7 | +8.7 |
| busy | 10000 | 185.6 | 138.3 | 156.4 | 101.2 go | 1.83x | 1.37x | +84.4 | +37.1 |

### GCE n2d-standard-8, AMD EPYC 7B13

| probe | iters | develop | this | forced par | fastest | develop / fastest | this / fastest | gap before | gap after |
|---|---:|---:|---:|---:|---|---:|---:|---:|---:|
| sweep | 0 | 0.0 | 0.1 | 24.1 | 0.0 zig-seq | — | — | +0.0 | +0.2 |
| sweep | 100 | 5.0 | 5.1 | 28.6 | 4.6 go-seq | 1.09x | 1.12x | +0.4 | +0.5 |
| sweep | 300 | 16.2 | 15.6 | 35.6 | 14.1 go-seq | 1.15x | 1.10x | +2.1 | +1.4 |
| sweep | 1000 | 42.6 | 39.1 | 43.1 | 25.8 rayon | 1.66x | 1.52x | +16.9 | +13.4 |
| sweep | 3000 | 67.0 | 62.6 | 58.7 | 46.3 rayon | 1.45x | 1.35x | +20.6 | +16.2 |
| sweep | 10000 | 158.0 | 145.7 | 134.7 | 132.3 rayon | 1.19x | 1.10x | +25.7 | +13.4 |
| sweep | 30000 | 370.3 | 371.3 | 348.6 | 327.7 go | 1.13x | 1.13x | +42.6 | +43.6 |
| nest | 0 | 1.1 | 1.1 | 98.4 | 0.0 zig-seq | — | — | +1.6 | +1.5 |
| nest | 100 | 19.3 | 20.8 | 110.2 | 18.2 go-seq | 1.06x | 1.15x | +1.2 | +2.6 |
| nest | 1000 | 164.3 | 129.0 | 170.2 | 49.4 rayon | 3.33x | 2.61x | +114.9 | +79.6 |
| nest | 10000 | 593.5 | 555.0 | 542.3 | 291.4 rayon | 2.04x | 1.90x | +302.1 | +263.6 |
| busy | 0 | 0.0 | 0.4 | 54.5 | 0.0 rayon-seq | — | — | -0.5 | +0.4 |
| busy | 100 | 6.5 | 5.3 | 57.6 | 8.7 go-seq | 0.75x | 0.61x | -2.2 | -3.4 |
| busy | 1000 | 114.1 | 49.9 | 69.3 | 22.6 rayon | 5.05x | 2.21x | +91.5 | +27.3 |
| busy | 10000 | 307.2 | 188.2 | 210.2 | 143.6 go | 2.14x | 1.31x | +163.6 | +44.6 |

**`busy`**: at 1000 iterations Almide is 1.52x Rayon on Intel (was 4.03x) and 2.21x on AMD (was 5.05x). The gap over the fastest fell from 50.7 to 8.7 µs (Intel) and from 91.5 to 27.3 µs (AMD), and at 10000 iterations from 84.4 to 37.1 and from 163.6 to 44.6 µs. Every busy row is under half its old gap.

**No regression elsewhere.** These checks are the same as for the cost model (fan-cost-model.txt: 1000 offers × 10 elements, min of 11, modes interleaved). The default build is at or below develop's on both machines at every iteration count, except rows within 2.5% and sub-µs noise at 0 iterations (Intel 1000 iterations: 31.1 → 25.3 µs; AMD: 38.6 → 32.4). The forced-parallel offer at 0 iterations costs 8.2 µs (Intel) and 24.1 µs (AMD), against 17–36 before. On the benchmarks (min of 15, ms):

| | Intel develop | Intel this | AMD develop | AMD this |
|---|---:|---:|---:|---:|
| small map ×500 | 54.6 | 47.6 | 74.9 | 64.3 |
| spectralnorm 1200 | 13.8 | 13.2 | 22.7 | 20.3 |
| spectralnorm 3000 | 72.9 | 71.6 | 109.6 | 105.2 |

What is left: `nest` (#2594, the effect `fan.map` is sequential) and the wasm leg, whose two `fan { }` arms run one after another.

## The sequential loop: a remainder by a literal (2026-10-17)

From 10000 iterations up the fastest column was Go's, because Go's sequential loop was ~10% faster than Rust's and Almide's (the parallel runtimes split 10 elements over 8 threads the same way, so the loop decides the row). The loop is `acc = (acc * 31 + j) % 1000003`: one dependent chain per iteration, latency-bound. Two things lengthened Almide's chain:

- rustc lowers `checked_rem` by a constant to a magic-number multiply whose sign fix sits on the chain (shift the quotient estimate, add its sign bit, multiply back), and it unrolls the loop with the induction offsets (`j + 1`, `j + 2`) on the chain too;
- with the native build's default `x86-64-v3`, an i128 multiply written in Rust becomes an unsigned `mulx` plus sign fix-ups, not the signed one-operand `imul`.

`x % <literal>` on `Int` (a divisor of 3 or more that is not a power of two) now renders as `almide_rem_const::<D>(x)` (crates/almide-codegen/src/prelude_rem.rs): the same truncated remainder, the quotient from the magic multiply on the signed `imul`, and the sign correction applied to the remainder from the dividend's sign, beside the multiply. A literal divisor of 3 or more is never 0 or -1, so the call cannot abort, as `%` by such a literal never did. A 90M-step chain (`x86-64-v3`, two runs each) measured 290-292 ms against rustc's 364-372 ms and Go's 310 ms on the Intel VM, and 370-371 ms against 454-457 ms and 400-420 ms on the AMD one. The remainder is checked against `%` on 60,000 dividends for 17 divisors and at the multiples of every divisor below 2000 (`prelude_rem.rs` tests).

The probes' sequential column, same method as the tables above (µs per offer, min of 7, implementations interleaved); `develop` is develop at 9e4fbcb2a with `ALMIDE_FAN_SEQUENTIAL=1`, `this` the same with this change:

### GCE c4-standard-8, Intel Xeon Platinum 8581C

| iters | develop seq | this seq | Rayon seq | Go seq | this / fastest other seq |
|---:|---:|---:|---:|---:|---:|
| 0 | 0.1 | 0.0 | 0.0 | 0.1 | — |
| 100 | 4.0 | 3.2 | 3.8 | 3.5 | 0.92x |
| 300 | 12.4 | 10.0 | 11.6 | 10.6 | 0.94x |
| 1000 | 41.6 | 33.2 | 39.4 | 35.6 | 0.93x |
| 3000 | 122.6 | 98.5 | 116.2 | 106.9 | 0.92x |
| 10000 | 402.0 | 326.5 | 383.9 | 349.2 | 0.93x |
| 30000 | 1193.8 | 961.5 | 1138.8 | 1031.9 | 0.93x |

### GCE n2d-standard-8, AMD EPYC 7B13

| iters | develop seq | this seq | Rayon seq | Go seq | this / fastest other seq |
|---:|---:|---:|---:|---:|---:|
| 0 | 0.1 | 0.0 | 0.0 | 0.3 | — |
| 100 | 5.3 | 4.2 | 5.0 | 4.5 | 0.94x |
| 300 | 15.2 | 12.5 | 14.8 | 13.8 | 0.91x |
| 1000 | 50.2 | 41.7 | 49.4 | 45.3 | 0.92x |
| 3000 | 150.5 | 125.1 | 148.9 | 135.4 | 0.92x |
| 10000 | 502.5 | 414.9 | 494.6 | 445.5 | 0.93x |
| 30000 | 1514.5 | 1252.5 | 1482.3 | 1345.2 | 0.93x |

Almide's sequential loop is now 6-9% faster than the fastest other sequential loop at every iteration count on both machines, and 17-21% faster than before. The parallel rows move with it (the elements are this loop); the parallel runtime itself is unchanged here.

## The pool: offers published to polling threads, stealing, a recalibrated cost model (2026-10-17)

With the loop at Go's speed or better, the rest of every gap was the parallel runtime. An offer handed a call to each idle pool thread through its own channel, and the threads slept between offers, so every offer paid the kernel's wake-up path (5-20 µs on these VMs). Each worker also read the thread CPU clock three times (a syscall, ~1.1 µs on the AMD VM), the blocks came from a mutex, and the site history was a mutex-guarded map. And an offer made inside another offer's element found the threads already claimed, so it ran on its caller alone. Now (runtime/rs/src/list.rs):

- **Published, not posted.** An offer is published in one of 64 slots (`ALMIDE_FAN_SLOTS`). A pool thread polls for `ALMIDE_FAN_SPIN` (50 µs) after each offer before it parks, and takes blocks from any published offer with blocks left. An offer wakes parked threads only when fewer than it wants are polling. A polling thread reads one counter, the published offers with blocks left (`ALMIDE_FAN_OPEN_BLOCKS`), and touches nothing else until it is non-zero. A thread that enters after the last block was dealt leaves without the element cleanup, and the caller's handle is cloned only once the caller may sleep. The gate lives in the caller's frame, and blocks are dealt by an atomic counter, still in list order (the ADR-0024 D6 trap wait relies on it).
- **One thread per core.** A pool thread enters an offer only while the threads working on offers are fewer than the cores (`ALMIDE_FAN_RUNNING`), and yields its core while they are not. Two `fan { }` arms each dealing to seven threads had put nine threads on eight cores (busy, 10000 iterations: 1.33x Go on the Intel VM).
- **A waiting caller steals within its own tree.** Once its own blocks are taken, a caller takes blocks of offers made inside blocks of the same outermost offer (`AlmideFanGate::root`), not another arm's. Taking another arm's element delayed this arm's next offer by up to an element (busy 10000: 108 µs against 95 per pair); taking none left a core idle through each outer offer's tail (nest 10000: 220 against 187).
- **The cost model, recalibrated.** A forced-parallel offer of ten empty elements now costs 2.9 µs (Intel) and 2.6 µs (AMD) at W=8, against 8.2 and 24.1 µs before (the table above). `native_offer_ns` / `native_worker_ns` are now 1.5 + 0.7W µs, the envelope of the new calibration rows (fan-cost-model.txt). A parallel offer is timed once in 8 (`ALMIDE_FAN_RESAMPLE_PAR`), the clock read is subtracted from every timed interval, and an estimate from a parallel offer within 2x of the crossover is checked by one sequential offer. A sequential site turns parallel only on two measurements in a row. The site history is a lock-free table.

Same probes and method as above (µs per offer, min of 7, interleaved). `fan-rem` is develop with the remainder change only, `this` adds the pool, and `forced par` is this with `ALMIDE_FAN_COST_OFF=1`:

### GCE c4-standard-8, Intel Xeon Platinum 8581C

| probe | iters | fan-rem | this | forced par | Rayon | Rayon seq | Go | Go seq | fastest | this / fastest |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| sweep | 0 | 0.1 | 0.1 | 2.9 | 9.2 | 0.0 | 3.1 | 0.1 | 0.0 rayon-seq | — |
| sweep | 100 | 3.2 | 3.2 | 8.2 | 8.9 | 4.2 | 7.5 | 3.5 | 3.5 go-seq | 0.93x |
| sweep | 300 | 10.1 | 6.2 | 8.2 | 10.8 | 11.8 | 15.5 | 10.8 | 10.8 go-seq | 0.58x |
| sweep | 1000 | 22.1 | 10.0 | 10.1 | 16.2 | 39.6 | 30.0 | 35.8 | 16.2 rayon | 0.61x |
| sweep | 3000 | 35.6 | 23.8 | 23.3 | 35.4 | 117.3 | 49.9 | 107.1 | 35.4 rayon | 0.67x |
| sweep | 10000 | 92.8 | 72.7 | 72.4 | 116.2 | 385.4 | 91.8 | 348.3 | 91.8 go | 0.79x |
| sweep | 30000 | 226.0 | 214.5 | 211.8 | 271.4 | 1141.5 | 237.3 | 1051.1 | 237.3 go | 0.90x |
| busy | 0 | 0.2 | 0.4 | 6.3 | 4.2 | 0.1 | 3.4 | 0.2 | 0.1 rayon-seq | — |
| busy | 100 | 3.4 | 3.5 | 9.8 | 6.1 | 7.5 | 6.8 | 7.0 | 6.1 rayon | 0.56x |
| busy | 1000 | 24.0 | 12.0 | 12.1 | 17.3 | 79.4 | 21.0 | 71.3 | 17.3 rayon | 0.69x |
| busy | 10000 | 120.2 | 95.9 | 96.1 | 152.3 | 766.8 | 103.8 | 701.2 | 103.8 go | 0.92x |
| nest | 0 | 0.2 | 0.4 | 12.4 | 10.5 | 0.2 | 14.7 | 0.3 | 0.2 rayon-seq | — |
| nest | 100 | 13.2 | 12.7 | 28.2 | 14.2 | 15.4 | 26.5 | 13.8 | 13.8 go-seq | 0.92x |
| nest | 1000 | 109.9 | 39.3 | 38.6 | 32.7 | 155.2 | 67.2 | 143.6 | 32.7 rayon | 1.20x |
| nest | 10000 | 371.0 | 285.7 | 288.3 | 222.5 | 1532.4 | 250.3 | 1412.2 | 222.5 rayon | 1.28x |

### GCE n2d-standard-8, AMD EPYC 7B13

| probe | iters | fan-rem | this | forced par | Rayon | Rayon seq | Go | Go seq | fastest | this / fastest |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| sweep | 0 | 0.5 | 0.0 | 2.6 | 20.2 | 0.3 | 7.2 | 0.6 | 0.3 rayon-seq | — |
| sweep | 100 | 4.6 | 4.7 | 8.2 | 19.6 | 5.0 | 11.3 | 4.9 | 4.9 go-seq | 0.96x |
| sweep | 300 | 13.4 | 9.3 | 9.4 | 20.6 | 15.0 | 22.2 | 14.0 | 14.0 go-seq | 0.67x |
| sweep | 1000 | 41.8 | 12.7 | 14.9 | 31.3 | 49.5 | 49.9 | 45.4 | 31.3 rayon | 0.41x |
| sweep | 3000 | 59.9 | 34.4 | 34.4 | 48.6 | 148.0 | 81.0 | 135.2 | 48.6 rayon | 0.71x |
| sweep | 10000 | 137.0 | 104.2 | 97.8 | 175.1 | 499.7 | 192.2 | 454.9 | 175.1 rayon | 0.59x |
| sweep | 30000 | 343.1 | 312.7 | 306.8 | 484.5 | 1484.6 | 416.2 | 1348.4 | 416.2 go | 0.75x |
| busy | 0 | 1.1 | 1.1 | 4.6 | 4.6 | 0.2 | 5.0 | 1.0 | 0.2 rayon-seq | — |
| busy | 100 | 4.0 | 2.9 | 13.3 | 13.2 | 10.2 | 8.8 | 9.7 | 8.8 go | 0.33x |
| busy | 1000 | 50.4 | 15.5 | 15.7 | 23.3 | 98.4 | 37.6 | 89.7 | 23.3 rayon | 0.67x |
| busy | 10000 | 182.7 | 125.4 | 126.8 | 209.8 | 994.4 | 167.3 | 902.3 | 167.3 go | 0.75x |
| nest | 0 | 2.7 | 0.0 | 13.7 | 30.0 | 1.2 | 28.4 | 2.7 | 1.2 rayon-seq | 0.00x |
| nest | 100 | 18.2 | 18.7 | 33.0 | 24.3 | 21.3 | 45.2 | 19.1 | 19.1 go-seq | 0.98x |
| nest | 1000 | 171.2 | 61.7 | 54.5 | 57.9 | 197.3 | 123.0 | 183.6 | 57.9 rayon | 1.07x |
| nest | 10000 | 476.6 | 368.9 | 371.3 | 324.1 | 1963.0 | 396.8 | 1779.2 | 324.1 rayon | 1.14x |

`sweep` and `busy` are at or under the fastest at every row on both machines. `nest` is not yet: its outer `fan.map` has an effect callback and still runs its four elements one after another (#2594), which the next change addresses.

No regression against develop on the cost model's sweep (fan-cost-model.txt, 1000 offers, min of 11): this is at or under develop at every iteration count on both machines. The benchmarks (min of 15, ms):

| | Intel develop | Intel this | AMD develop | AMD this |
|---|---:|---:|---:|---:|
| small map ×500 | 50.4 | 32.6 | 69.0 | 42.3 |
| spectralnorm 1200 | 13.6 | 12.9 | 22.0 | 20.5 |
| spectralnorm 3000 | 75.5 | 74.9 | 117.1 | 116.5 |
