<!-- description: ADR-0033 implementation plan: JSON documents as certified allocation families, two-pass parse, SWAR/simd128 scanning, fused constructors, exact-size writer, then native and one shared source, with gates, negative controls and targets per phase -->
# JSON substrate (ADR-0033) — phased plan

**Decision:** [ADR-0033](../../adr/0033-json-documents-are-allocation-families.md) (Proposed 2026-11-09; twelve ○× questions open).
**Base:** branch `json-reborn`. It cuts allocations per jsonround record from 141 to 54 (probe run 42578965891):

| Phase of the program | Allocations per record |
|---|---:|
| build | 27 |
| stringify | ~0 |
| parse | 27 |

**Reference bench:** `research/benchmark/json-world/` (branch `probe/json-world`). It runs jsonround in Almide, serde_json, sonic-rs, simd-json, yyjson and simdjson, native and wasm, timed on CI.

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

- The **"now"** cells come from `probe/json-world`. They are placeholders until its CI run lands, and are filled in from its results, never estimated.
- The phase columns are targets; each phase gate measures its own.

### jsonround (time, lower is better)

| Leg | Reference | Now | Phase 2 | Phase 4 | End (phase 5 wasm, phase 7 native) |
|---|---|---:|---:|---:|---:|
| wasm | serde_json (wasm32-wasip1) | 1.65× (#4613) / ⟨json-world⟩ | ≤ 1.15× | ≤ 0.85× | **≤ 0.75×** |
| wasm | yyjson (wasi-sdk, simd128) | ⟨json-world⟩ | — | ≤ 1.3× | **≤ 1.0×** |
| native | serde_json | ⟨json-world⟩ | — | — | **≤ 0.8×** |
| native | sonic-rs | ⟨json-world⟩ | — | — | **≤ 1.25×** |
| native | yyjson | ⟨json-world⟩ | — | — | **≤ 1.25×** (≤ 1.1× if owner question 6 admits SIMD) |

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
