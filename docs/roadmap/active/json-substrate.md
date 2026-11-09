<!-- description: ADR-0033 implementation plan: JSON documents as certified allocation families, two-pass parse, SWAR/simd128 scanning, fused constructors, exact-size writer, then native and one shared source, with gates, negative controls and targets per phase -->
# JSON substrate (ADR-0033) — phased plan

**Decision:** [ADR-0033](../../adr/0033-json-documents-are-allocation-families.md) (Accepted 2026-11-09; all thirteen owner questions answered ○).
**Base:** branch `json-reborn`. It cuts allocations per jsonround record from 141 to 54 (probe run 42578965891):

| Phase of the program | Allocations per record |
|---|---:|
| build | 27 |
| stringify | ~0 |
| parse | 27 |

**Reference bench:** `research/benchmark/json-world/` (branch `probe/json-world`). It runs jsonround in Almide, serde_json, sonic-rs, simd-json, yyjson and simdjson, native and wasm, timed on CI.

**Order.** The world bench (run 42587113062) shows that the leaders win by arenas and layout, not by SIMD: simdjson without SIMD parses jsonround in 22 ms on wasm, forcing westmere costs it 0.7 ms natively, and yyjson builds 250k values with 29 allocations. So the phases run **0 → 1 → 2 (families) → 4 / 4b (fused construction, whole-collection families, the exact writer) → 3 (scanning) → 5**, then 6 → 7 on native. The phase numbers are kept as names.

**json-reborn (owner Q13).** It does not land alone. Its size and allocation rows are re-measured after phase 2, and the owner rules once on the net movement and the stdlib re-inspection.

**Rule for every phase:** it lands on its own, every gate stays green, and its ratchet protects it.

- Instruction and allocation counts are deterministic, so **they are the ratchet**: the probe's `wi.py` counter plus `ALMIDE_WASM_ALLOC_COUNT`.
- Wall time is measured with `research/benchmark/perf/gce_ab.sh` (alternating A/B on one VM) and judged against its noise floor.

## Benchmarks every phase reports

| Row | What it stresses | Legs |
|---|---|---|
| `jsonround` (50k records) | build → stringify → parse → read | native, wasm |
| `strchurn` | interpolate → split → trim → to_upper → join | native, wasm |
| `wordfreq` | split → map count → top 10 | native, wasm |
| twitter.json, citm_catalog.json, canada.json (the nativejson-benchmark trio) | parse to DOM / stringify throughput: string-heavy, key-heavy, float-heavy | native, wasm |
| `decode` (`User.decode` ×1M, ADR-0013) | Codec decode from a parsed Value | native, wasm |
| size: `crates/almide-wasm/tests/golden/size-baseline*.txt` | bytes per module | wasm |
| retention probe (ADR-0033 D9) | peak memory ÷ live data | native, wasm |

## Phase 0 — ratchets and the world bench

- **Work.**
  - Land json-reborn.
  - Fold `research/benchmark/json-world/` into `research/benchmark/perf/`, with the three corpus files vendored (they have pinned hashes).
  - Add the phase-split jsonround (build / str / full) and its instruction and allocation counts to the CI ratchet, both legs.
  - Add the retention probe.
- **Gate.** The counts are pinned per row, and the GCE noise floor is measured for every timed row (two identical refs, the same way as the ADR-0013 stage 1 floor).
- **Negative control.** A build with one extra allocation per parsed value fails the allocation ratchet. A build with the reborn parser reverted fails the instruction ratchet.
- **Expected gain.** None; this phase measures.
- **Touches `render_native_*` / `runtime/rs`?** No.

## Phase 1 — certify what exists (wasm)

- **Work.**
  - Bring every `prim` site of `stdlib/json_parse.almd` and `value_utils.almd` into the linear-certified class (#4005 A4 route A, `almide-lin-prove`).
  - Move wasm stringify from the emitter-built `$vjson` helper into the self-host writer, certified the same way.
- **Gate.**
  - `almide-anchor-check --census --lin` reports **0 counted sites** for the JSON bodies, as a grow-only ratchet.
  - The structural wall stays OK on every fixture.
- **Negative controls.**
  - Per file, a poisoned `Q` certificate (a string copy past its span; a refusal code written past the cell) is rejected by all three verdicts.
  - A writer certificate held to another artifact reads LIN REJECT.
- **Expected gain.** ±2% on jsonround. The point is that phase 2 builds on certified code.
- **Risks.** The writer's size change may push the size ledger. Hold any raise until it is measured (ruling of 2026-11-08).
- **Touches `render_native_*` / `runtime/rs`?** No.

## Phase 2 — allocation families and the two-pass parse (wasm)

- **Work** (ADR-0033 D1, D2).
  - **Interior blocks.** An interior block's rc word has its top bit set and holds the distance back to its root. `$inc` and `$dec` redirect such a word to the root. The family is freed through its link list. All of this is emitted only where a family producer is linked.
  - **Two-pass parse.** Pass 1 validates and counts; it allocates nothing. Pass 2 carves an exact family under room-checked carves.
  - **Wide objects.** Objects of 16 keys or more get a key index carved inside the family, with a sort fallback.
  - **The clean flag.** Each interior string gets a clean flag.
  - **O(1) reads.** `as_array` / `as_object` / `as_string` share the interior block.
- **Proofs.**
  - `proofs/FamilyRc.v` (new): family-credited RC events give `Temporal`.
  - `StructuralRuntime.v`: the new `$inc` / `$dec` trees, with the double-free trap.
  - `InitCert.v`: carve sites (`site_window` over sub-extents).
  - `check-structural-bytes.sh` grounds the new routine bytes.
- **Gate.**
  - The structural wall is OK, and the carve sites are held by the anchor check.
  - The alias fixtures cover an extracted string or list that is then mutated (copy-on-write must copy) and a sub-value that outlives its document. They run as `spec/wasm_cross` on both legs and cite C-087 / C-400 / C-401.
  - Parse allocations per jsonround record ≤ 1 (one family per document).
- **Negative controls** (`witness-reinject.sh family`):
  - the redirect dropped;
  - the root distance off by 4;
  - a link not released;
  - a COW judge that compares signed (so interior reads as unique);
  - a carve store misplaced;
  - the carve room test loosened by one;
  - pass 2 counting one node more than pass 1 (it must abort, never write).
  - Each is rejected by the kernel, the anchor check, the checked allocator or the fixture, and the drill pins which one.
- **Expected gain.** Parse ~−40 to −55% instructions. jsonround wasm ~−25 to −35% total. ADR falsifier 1: under 25% means revert.
- **Risks.**
  - **Other rc-word readers.** The rc word is read in more places than the three judges, for example the in-place append windows and `@reuse`. Audit every load of `RC.offset` and pin it with a unit test that lists them.
  - **The JS host and the component realloc** must never see an interior block. A Region export must copy out.
- **After the gate (owner Q3).** Re-measure DuckDB-style shape reuse (objects of one shape sharing their key blocks) on top of families. The json-reborn sibling-key trial (17166b602) lost +31% instructions because it read keys through calls; inside a family the shared keys are plain interior blocks, so the trade may flip. It lands only on a measured instruction win with no size raise.
- **Touches `render_native_*` / `runtime/rs`?** No. It touches `crates/almide-wasm` runtime routines and `proofs/`.

## Phase 3 — scanning (wasm)

- **Work** (D3).
  - SWAR in pass 1: plain-ASCII string runs, whitespace, the 8-digit number parse.
  - The internal `__scan_special` op, with simd128 (`v128.load` + `i8x16` compares + `bitmask` + `ctz`) and a scalar tail.
  - Measure canada.json with and without the Eisel–Lemire path (+440 B) for owner question 10.
- **Certificates.**
  - SWAR uses the Bytes room test at width 8.
  - `v128.load` is an access of width 16 on the helper-byte-loop route. Make `BoundsCert.v` width-parametric if it is not.
  - Teach `almide-anchor-check` the `0xFD` prefix.
- **Gate.** The structural wall is OK, and the anchor check holds every `v128.load` to its room test.
- **Negative controls.** `swar-room-one-byte-loose`, `simd-room-15`, `simd-tail-exit-loosened`: each reads ANCHORS REJECT, and each control reads 0 before injection.
- **Expected gain.**
  - twitter / citm parse: −30 to −50%, from long strings.
  - jsonround: −5 to −10% (its strings are short).
  - Size: + a few hundred bytes, only in modules that link the op. It is ledgered.
- **Owner Q5 safeguards (all four are part of the gate).**
  - The SWAR body stays as the fallback; simd128 is never the only route.
  - A build option disables simd128 (the module then links the SWAR body).
  - CI runs the wasm suite a second time with simd128 disabled, byte-identical output.
  - The release note states the simd128 requirement and the option.
- **Risks.**
  - Embedded or old hosts without simd128. Mitigation: the build refuses `--target wasm` on a host profile that declares no simd128 and falls back to the SWAR body.
  - The ratchet for the size growth.
- **Touches `render_native_*` / `runtime/rs`?** No.

## Phase 4 — construction and writing (wasm, plus one shared nanopass)

- **Work** (D4, D5).
  - A nested `value.object` / `value.array` with a static shape, and each derived Codec encoder, becomes one family carve. Computed children become links.
  - Stringify does a counting pass, then one exact allocation.
    - Clean strings are bulk-copied.
    - Ints go through a two-digit table.
    - Floats go through `float.to_string`'s algorithm, from a buffer entry point.
- **Gate.**
  - Build allocations per jsonround record ≤ 5.
  - A stringify result is exactly one allocation.
  - All contract fixtures stay byte-identical (C-356 NaN, C-401 order, the pretty form).
- **Negative controls.**
  - The counting pass short by one byte → the writer's room test aborts (a defined abort), and `anchors` pins the check.
  - A link that is not released → `family-link-not-released`.
- **Expected gain.**
  - Build ~−60% allocations.
  - jsonround wasm ~−15 to −25% total.
  - decode is unchanged.
- **Risks.**
  - If the constructor-fusion nanopass sits in `almide-codegen`, it is shared with native. It needs coordination with the other session.
  - Keep the native render of the fused form identical to today's.
- **Touches `render_native_*` / `runtime/rs`?** Only if the pass is shared: `almide-codegen` passes, a medium collision. Otherwise no.

## Phase 4b — whole-collection families (wasm; ADR-0033 D4b)

- **Work.** A `list.map` / `range |> map` / `filter_map` whose callback result is uniquely owned by the result list carves the list and every block flowing into its elements from one chunked construction arena (a family). Temporaries stay on the heap. Moved-in children become links.
- **Gate.**
  - jsonround **build allocations ≤ 50 for the whole run** (yyjson: 29).
  - Every measurement table reports allocations per phase against yyjson (build 29, stringify 1, parse 2).
  - Alias fixtures: an element taken out and mutated (CoW copies), an element kept after the list is dropped (retains the arena), a fallible callback (refused until the error-edge family drop lands).
- **Negative controls.** `chunk-root-pointer-wrong`, `chunk-not-linked`, `carve-crosses-chunk-end`, plus the phase 2 family drills on the arena producer.
- **Refusals that are not failures.** An escaping callback block, a fallible callback, and the `var` + `for` accumulator form do not fire; each is a fixture that shows the rule does not fire.
- **Touches `render_native_*` / `runtime/rs`?** No for wasm. The ownership fact it reads is computed in shared passes, which it reads without changing.

## Phase 5 — String families (wasm)

- **Work** (D6). `string.split` / `lines` / `words` return one family.
- **Gate.**
  - Allocations per split call ≤ 1, plus the result list.
  - The alias fixtures: a part pushed onto; a part kept after its source is dropped.
- **Negative control.** The family drills of phase 2, re-run on the split producer.
- **Expected gain.**
  - strchurn wasm: −15 to −25%.
  - wordfreq wasm: −5 to −15%.
  - The decision point for owner question 4 (the SSO condition).
- **Risks.** Retention: map keys taken from parts pin the text. Watch the retention probe; falsifier 3 applies.
- **Touches `render_native_*` / `runtime/rs`?** No.

## Phase 6 — the native substrate ⚠ collides with the other session

- **Work.**
  - `AlmideValue` gains a doc-backed view variant (`Rc<Doc>`, node index) over 16-byte nodes in safe Rust.
  - ADR-0013 stage 3 seamless slices in `crates/almide-str`, which doc strings use.
  - SWAR scanning in safe Rust.
  - Two-pass parse and the exact-size writer in `runtime/rs`.
  - The thread-shared flip for docs that cross `fan` (as AlmideStr does).
- **Gate.**
  - Miri and ASan nightly over the JSON corpus.
  - `scripts/check-runtime-unsafe.sh` shows **no new `unsafe` line** outside `almide-str`'s Miri-gated slice code.
  - Native ≤ wasm on every JSON row (`check-perf-ratio.sh`).
  - The native-verified floor does not shrink.
- **Negative controls.**
  - The doc view outliving its doc under a forced early drop → Miri reports it.
  - A slice past its owner's length → refused in `almide-str`'s tests.
- **Expected gain.** Native jsonround ≤ 0.8× serde_json; twitter / citm parse ≥ 1.5× serde_json.
- **Risks.**
  - **High collision**: `runtime/rs/src/value.rs`, `json.rs`, `render_native_value.rs`, `render_native_str.rs`. Start only after the other session's runtime and render work has landed (owner question 12), and rebase onto it, never across it.
  - Stage 3 seamless slices have their own ADR-0013 falsifier.
- **Touches `render_native_*` / `runtime/rs`?** **Yes.**

## Phase 7 — one source for both legs ⚠ collides with the other session

- **Work** (D10).
  - The parser and writer in typed Almide, over a zero-copy byte view of the text.
  - The target-neutral MIR ops `Op::DocAlloc` / `Op::DocCarve` / `Op::DocLink`.
    - wasm renders them to phase 2's carve sequences, with their witnesses.
    - Native renders them to a safe-Rust builder (in C1 where it renders, in v0 otherwise).
  - Delete `runtime/rs/src/json.rs`'s parse and stringify, and the `prim` parser.
- **Gate.**
  - Instruction count on wasm ≤ the `prim` parser + 3%.
  - Native time ≤ phase 6 + 3% (GCE A/B).
  - Every JSON fixture byte-identical on both legs.
  - `proofs/native-wall.sh` certifies the parser's functions in the native-verified floor.
- **Negative controls.** The phase 2 and phase 3 drills on the new render. On native, `witness-reinject.sh native` with a lost carve check.
- **Expected gain.** Neutral on speed. The trusted base shrinks by the hand-written native JSON.
- **Risks.**
  - **High collision**: `almide-mir` lowering and `render_native_*`.
  - The v0 walker's rendering of the typed parser must not regress the phase 6 numbers.
- **Touches `render_native_*` / `runtime/rs`?** **Yes.**

## Targets

Ratios are Almide ÷ reference on the same VM and the same input (lower is better). Throughput targets are fractions of the reference's GB/s.

- The **"now"** cells come from `probe/json-world`, run 42587113062 (GitHub-hosted runner, wasmtime 50.0.1 AOT, rustc 1.97.0, yyjson 0.13.0, simdjson 5.0.5, wasi-sdk 34; median total process time of 15 rounds). "reborn" is json-reborn, "head" is its base.
- The phase columns are targets; each phase gate measures its own.

### jsonround (time, lower is better)

| Leg | Reference | Now | Phase 2 | Phase 4 | End (phase 5 wasm, phase 7 native) |
|---|---|---:|---:|---:|---:|
| wasm | serde_json (wasm32-wasip1) | reborn 175.7 ÷ 151.0 = **1.16×** (head 278.7 ms = 1.85×) | ≤ 1.15× | ≤ 0.85× | **≤ 0.75×** |
| wasm | yyjson (wasi-sdk, simd128) | 175.7 ÷ 64.5 = **2.72×** | — | ≤ 1.3× | **≤ 1.0×** |
| native | serde_json | 148.3 ÷ 160.1 = **0.93×** | — | — | **≤ 0.8×** |
| native | sonic-rs | 148.3 ÷ 139.3 = **1.06×** | — | — | **≤ 1.25×** |
| native | yyjson | 148.3 ÷ 34.3 = **4.32×** | — | — | **≤ 1.25×** (≤ 1.1× if owner question 6 admits SIMD) |

The run's reference rows (ms, total process time; wasm is AOT):

| Implementation | wasm | native |
|---|---:|---:|
| simdjson On-Demand (no SIMD on wasm) | 38.2 | 24.4 |
| simdjson DOM | 50.0 | 32.0 |
| yyjson | 64.5 | 34.3 |
| simd-json (borrowed) | 113.1 | 101.4 |
| sonic-rs | 175.5 | 139.3 |
| serde_json | 151.0 | 160.1 |
| Almide json-reborn | 175.7 | 148.3 |
| Almide head (json-reborn's base) | 278.7 | 148.2 |

Allocations per run (50k records): yyjson 32, simdjson 37–38, sonic-rs 600k, serde_json 1.35M, Almide native 900k, Almide reborn wasm 2.85M (build 1.05M, parse + sum 1.80M), Almide head wasm 7.05M.

### Parse throughput (twitter / citm_catalog / canada)

| Leg | Reference | End |
|---|---|---|
| native | yyjson | ≥ 0.8× (string-heavy rows); ≥ 0.6× on canada, which is float-bound |
| native | simdjson | ≥ 0.6× SWAR-only; ≥ 0.8× with SIMD |
| wasm | yyjson-wasi | ≥ 1.0× |

### strchurn / wordfreq (time ÷ Rust, wasm)

| Row | End target (phase 5) | Note |
|---|---:|---|
| strchurn | ≤ 1.0× | Owner question 4 reopens SSO if it is above 1.3× after phase 5. |
| wordfreq | ≤ 0.9× | — |

**Stringify throughput (native and wasm):** ≥ 0.8× yyjson by the end state.

## Not in this plan

- Lazy / on-demand parsing and any API change (ADR-0033 D8).
- Copy-out on escape (D9). It needs its own ADR if falsifier 3 fires.
- A compact node layout on wasm (Alternative C). It needs falsifier 5.
- German / Umbra strings (Alternative E). It needs owner question 4's condition.

## Progress

### Phase 0 — done (2026-11-09)

- **The world bench** lives in `research/benchmark/perf/json-world/` (`.github/workflows/json-world-bench.yml`, started by a `[world]` commit on a `json-substrate*` branch). It adds the corpus rows (`corpus.sh`): twitter, citm_catalog and canada at a pinned nativejson-benchmark commit, digest-pinned in `.github/pinned-downloads.txt`.
- **The JSON ratchet** is `crates/almide-wasm/tests/json_ratchet.rs` with `golden/json-ratchet.txt`: wasmtime fuel and `$alloc` / `$free` counts per jsonround phase at N = 2,000. The rows only move down, and the update mode refuses to write a raise. It runs in the release `rest` shard.
- **The retention probe** is `json_retention_probe_reports`, printed every run and not pinned. On the json-reborn compiler it reads 5.7 retained bytes per kept byte.
- **Pins** are taken on the json-reborn compiler (probe run 42595010181):

| phase | fuel / record | allocs / record | yyjson allocs, whole run |
|---|---:|---:|---:|
| build | 6,134.9 | 21.00 | 29 |
| stringify | 7,217.9 | 0.00 (2 per run) | 1 |
| parse | 14,144.0 | 30.01 | 2 |
| sum | 1,698.1 | 6.00 | — |

- **Negative controls** (probe runs 42598717788 and 42601172093):
  - The control branch `json-substrate-p0-control` passes.
  - One extra allocation per parsed value fails: parse allocs 60,014 → 78,015, and parse fuel rises.
  - Restoring the pre-reborn parser fails: parse fuel 28.3M → 51.0M, and allocs 60,014 → 172,016.
- **Noise floor**: two compilers with the same compiler source, on one GitHub runner, alternating, n = 15 (probe run 42595010181). wasm AOT head/base = 1.0069, native = 1.0015. Medians differ by up to 1.8× between runner machines, so only same-run ratios are compared.
- **Corpus rows** (run 42601233615; parse ms, wasm AOT / native):
  - twitter: Almide 6.9 / 2.6; serde_json 3.2 / 2.2; yyjson 0.63 / 0.78.
  - citm_catalog: Almide 10.6 / 13.4; serde_json 5.2 / 3.9; yyjson 1.6 / 2.0.
  - canada: Almide **1934** / 28.7; serde_json 10.9 / 12.8; yyjson 4.1 / 3.1.
  - canada on wasm is float.parse's slow path: an 800-digit decimal shifted one bit per pass, taken by every float of 16 or more digits.

### Phase 1 — the writer move measured and withheld

- The self-hosted compact writer (a size pass, one exact String, a room-checked fill; 9198cc27a, then SWAR clean runs in d7235dce0) printed byte-identical output on jsonround.
- It cost **+27% stringify fuel** (7,218 → 9,198 per record) and **+9.4% jsonround wall time** on wasm (probe run 42598565892), so it was reverted (4c0589585).
- Its per-value cost is the size pass's second tree walk and a call per leaf. It returns in phase 4 with the families' clean flag (a clean string is one bulk copy and needs no size scan) and the two-digit table.
