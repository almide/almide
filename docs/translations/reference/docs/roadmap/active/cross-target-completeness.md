<a id="cross-target-completeness-the-lid"></a>

# Cross-Target Completeness (the Lid)

[![English](https://img.shields.io/badge/English-24292f?style=flat-square)](cross-target-completeness.md) [![日本語](https://img.shields.io/badge/%E6%97%A5%E6%9C%AC%E8%AA%9E-d0d7de?style=flat-square)](cross-target-completeness.ja.md) [![简体中文](https://img.shields.io/badge/%E7%AE%80%E4%BD%93%E4%B8%AD%E6%96%87-d0d7de?style=flat-square)](cross-target-completeness.zh-CN.md)

> Non-normative reader snapshot; pinned original remains authoritative. No automatic synchronization or independent current execution/validation. This historical catalogue and dated competitive survey are not current defects, guarantees or independently validated comparative claims. “Active”, “live”, and “already closed” below retain source-era language; the opening evidence boundary governs those historical passages. No operational instructions are executed.
> [Source / 原文](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/cross-target-completeness.md)

Source description: Historical cross-target drain strategy, dated catalogue and limits of differential evidence


<a id="historical-drain-queue-and-present-evidence-boundary"></a>

## Historical drain queue and present evidence boundary

The June 2026 catalogue below is not the live defect queue. The v0 runtime
registry it counted has retired; native base64, self-hosted algorithms and
the interpreter/fuzzer now exist. Current operation availability and exception
scope are recorded in [target availability](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/proofs/target-availability.toml)
and [the contract ledger](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/contracts/contracts.toml).

The “four seals” describe an assurance strategy, not a proof that every
possible divergence is impossible. Finite corpus/fuzz agreement cannot
establish that universal statement, and documented host observations and
resource limits remain scoped. The language-comparison survey and its
uniqueness claim are preserved as the author's dated survey, not revalidated
competitive claims. The historical measurements and proposed drains follow.

> Native(Rust) and WASM must produce **byte-identical (stdout, stderr, exit)** for every program.
> The gate exists and has **zero grandfathered exceptions**; this roadmap is how we go from
> "everything we found is fixed" to "divergence is structurally impossible" — and the live
> burndown queue of what's still enumerated.

Status: **Active** — burndown phase. Gate + registry + ratchets all operational.

<a id="verified-positioning-2026-06-05-audit-11-languages-surveyed"></a>

## Verified positioning (2026-06-05 audit, 11 languages surveyed)

No other multi-target language claims this benchmark. The three industry postures are all
weaker: disclaim-and-delegate (Gleam "by design", Haxe, Dart), semantic-level shared tests
with no byte-diff (Scala.js, Kotlin MP), or single-target avoidance (Elm; ReScript dropped
its native backend). MoonBit — the closest architecture (direct wasm emit + Perceus-family
RC) — makes no equivalence claim at all.

> **"Almide is the only language whose two backends are held to byte-identical
> (stdout, stderr, exit-code) output by a CI gate — every divergence is a tracked,
> shrinking debt, not an accepted fact of life."**

<a id="current-state-2026-06-06"></a>

## Current state (2026-06-06)

| Mechanism | State |
|---|---|
| Cross-target gate (`tests/wasm_runtime_cross_target.rs::wasm_cross_target_spec`) | 65+ corpus files, byte-compared, **0 @xt-allow** |
| Documented divergence list | **N = 0** (`fan.timeout`, the last entry, was removed from the language in 0.29.0 — C-006 flipped to active) |
| Oracle-pairing registry — RETIRED with the v0 emitter (#782); `scripts/check-rt-oracle-registry.sh` is a tombstone | superseded by spec/wasm_cross + the interp 3-way oracle (final v0 tally: 76 verified / 42 grandfathered) |
| By-construction tables (Σ-probe derived from Rust std at emit time + all-scalar CI locks) | case folding, whitespace, UTF-8 classification, Unicode properties (Alphabetic/Alphanumeric/Uppercase/Lowercase) |
| Lean kernel-check in CI | 41 theorems (Perceus RC / closure env / heap) — `lake build`, not just a sorry-grep |
| Other ratchets | fmt round-trip property (no skips), host-arch-deterministic emit, snapshot suite |

Original 8-cluster catalogue (46 bugs): **fully drained** (#363–#385).
Grandfathered-sweep catalogue (47 live divergences in the 48 unverified routines): **13 drained, 34 enumerated below**.

<a id="the-lid-four-seals-each-finite"></a>

## The lid: four seals, each finite

A new divergence must evade ALL of these to exist:

1. **Routine seal** — drain `grandfathered` to **0**: every wasm runtime routine is
   (a) derived-by-construction (tables), (b) exhaustively differential-tested (small domains),
   or (c) N-million-fuzz differential-tested (large domains). The #382 gate already blocks
   unregistered NEW routines; add a no-new-grandfathered CI assert when the count hits 0.
2. **Composition seal** — routines being identical doesn't seal compositions (the fan.map
   inline-lambda trap was a CHECKER bug). Components: ConcretizeTypes **hard**
   AllTypesConcrete postcondition (Unknown→i32 silent-miscompile class becomes a build
   error), then a **reference interpreter** over the ~30-node IR core (executable spec;
   3-way differential localizes which backend diverges) + **nightly generative fuzzing**
   (type-directed, ~44k–175k programs/night, auto-minimize, auto-promote to corpus).
   "Zero divergences" may only be declared once the fuzzer runs dry.
3. **Future-code seal** — already closed: cross-target gate + oracle-pairing gate +
   Lean kernel-check + fmt round-trip run on every PR.
4. **Kernel seal** (long horizon) — selfhost the algorithmic stdlib in Almide so both
   targets run the same source (the "two implementations drift" tap closes), shrinking
   the proof surface to ~30 IR ops + RC + layout; prove those (per-op theorems).
   **Selfhost is parked here by decision (2026-06-05) — roadmap-only until called.**
   See `research/spike/v1-mir/` (the former `research/selfhost/` probes were removed).

<a id="live-drain-queue-all-enumerated-coordinates-known"></a>

## Live drain queue (all enumerated, coordinates known)

<a id="pr-d--encodings--json-path-13-divergences-next-up"></a>

### PR-D — encodings + json path (~13 divergences, next up)
- **base64 is broken on BOTH targets**: native build-crashes (E0425 — `almide_rt_base64_encode/decode`
  never existed), wasm returns empty/rejects valid input. Implement native from scratch, fix wasm.
- `int.from_hex`: `0X` prefix, `'_'` skip, repeated-`0x` semantics diverge.
- `bytes.read_f16_le`: wasm renders ±inf as ±f32::MAX (finite!).
- `hex.decode` error-message detail (native has position/length suffixes).
- `json.get/set/remove_path`: negative-index normalization missing on wasm; `set_path`
  Index-on-object reads heap garbage; OOB Index **traps 134** vs native infallible no-op;
  nested propagation of all of the above.

<a id="pr-b--wasm-regex-engine-port-11-divergences-biggest-chip"></a>

### PR-B — wasm regex engine port (11 divergences, biggest chip)
The wasm engine is structurally broken: byte-based (multibyte haystacks → U+FFFD garbage),
`|` treated as a literal, escape atoms only `\d\w\s` (`\.` `\n` `\D` `\W` `\S` and escaped
literals never match), no class escapes (`[\d]` = `{'\', 'd'}`), captures return wrong groups.
Fix = port the native hand-rolled engine (`runtime/rs` `rx_parse_*`, scalar-based) to wasm
emit faithfully + a differential fuzz gate (random patterns × haystacks incl. multibyte).
Consider a Lean **termination theorem** for the backtracking engine (fuel-bounded) — the
property neither exhaustion nor fuzz can establish.

<a id="pr-c--math-determinism-16-divergences-needs-a-contract-decision"></a>

### PR-C — math determinism (16 divergences, needs a contract decision)
wasm polynomial approximations diverge from native libm everywhere (sin(π) off by 8 orders);
outright bugs regardless of contract: `log(0)`=NaN (native -inf), `exp(-745)`=0 (native
subnormal), `fpow(-2, 0.5)`=1.414 (abs bug; native NaN), `fpow(0,-1)`=0 (native inf),
`fpow(2, inf)` **traps 134**, `to_fixed(x, ≥19)` **traps 134**, `to_fixed` rounding wrong
(0.05@1dp → 0.0 vs 0.1).
**Contract recommendation (to be confirmed)**: native libm is itself platform-dependent
(macOS vs glibc last-ulp), so vendor a deterministic pure-Rust libm (musl-port `libm` crate)
into the NATIVE runtime and port the same algorithms to wasm — buys native cross-PLATFORM
determinism too. Precedent: Java `StrictMath`/fdlibm.

<a id="small-items-queued"></a>

### Small items (queued)
- enum-NAME record-variant construction: native rejects (E0574) but **wasm accepts and runs** — frontend should reject on both.
- const-folded float inf/NaN emits `inff64`/`NaNf64` invalid-Rust identifiers (E0425 native build-fail = ICE-leak class).
- sized-int record FIELDS (`M { a: 5 }` with `a: Int8`): native E0308, wasm reads next field as garbage (the #368 class, construction-site residual).
- recursive GENERIC type repr on wasm (`Tree[T]` self-referencing): T payload misrenders (no panic; no spec exercises it).
- user type named `Box` + recursive enum needing Box indirection collides on native (pre-existing).

<a id="order-of-work"></a>

## Order of work

PR-D → PR-B → PR-C (contract decision first) → small items → AllTypesConcrete hard
postcondition → reference interpreter → nightly fuzzer. Each lands with: corpus files in
`spec/wasm_cross/`, registry flips grandfathered→verified citing the new tests, and the
verification class declared (`exhaustive` / `fuzz-N` / `lean` / `by-construction`).

<a id="related"></a>

## Related

- [correctness-guarantee-gaps.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/correctness-guarantee-gaps.md) — the white-box layer-proof axis (WasmIR, ANF, closure env, Perceus conformance)
- [determinism-belt.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/determinism-belt.md) — compiler-output determinism by construction
- [closure-cross-target-completeness.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/closure-cross-target-completeness.md) — the closure dimension (largely drained)
- [llm-first-language.md](https://github.com/almide/almide/blob/35297a612fb29e3f78f6587aba253aa91d51f466/docs/roadmap/active/llm-first-language.md) — why byte-determinism serves MSR
