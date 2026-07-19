<!-- description: Cross-target completeness lid — the staged path from "all known divergences fixed + byte-diff gate" to structural equivalence (drain → interpreter+fuzz → selfhost → kernel proofs), with the live drain queue -->
# Cross-Target Completeness (the Lid)

> Native(Rust) and WASM must produce **byte-identical (stdout, stderr, exit)** for every program.
> The gate exists and has **zero grandfathered exceptions**; this roadmap is how we go from
> "everything we found is fixed" to "divergence is structurally impossible" — and the live
> burndown queue of what's still enumerated.

Status: **Active** — burndown phase is DONE (registry 0/142 grandfathered, 0 @xt-allow,
0 open drain items); remaining work has moved to the composition/kernel seals (§2/§4 below).
Gate + registry + ratchets all operational.

## Verified positioning (2026-06-05 audit, 11 languages surveyed)

No other multi-target language claims this benchmark. The three industry postures are all
weaker: disclaim-and-delegate (Gleam "by design", Haxe, Dart), semantic-level shared tests
with no byte-diff (Scala.js, Kotlin MP), or single-target avoidance (Elm; ReScript dropped
its native backend). MoonBit — the closest architecture (direct wasm emit + Perceus-family
RC) — makes no equivalence claim at all.

> **"Almide is the only language whose two backends are held to byte-identical
> (stdout, stderr, exit-code) output by a CI gate — every divergence is a tracked,
> shrinking debt, not an accepted fact of life."**

## Current state (2026-07-19)

| Mechanism | State |
|---|---|
| Cross-target gate (`tests/wasm_runtime_test.rs::wasm_cross_target_spec`) | 65+ corpus files, byte-compared, **0 @xt-allow** (verified: `grep -r '// @xt-allow:' spec/wasm_cross/*.almd \| wc -l` = 0) |
| Documented divergence list | **N = 0** (`fan.timeout`, the last entry, was removed from the language in 0.29.0 — C-006 flipped to active) |
| Oracle-pairing registry (`crates/almide-codegen/rt-oracle-registry.toml`, gate = `scripts/check-rt-oracle-registry.sh`) | **142 routines: 142 verified / 0 grandfathered — FULLY DRAINED** (verified: `grep -c '^\[\[routine\]\]'` = 142, `grep -c 'status = "grandfathered"'` = 0). Closed by deb426f5 (2026-06-06, "Drain the oracle registry to 3 grandfathered routines and add a ratchet ceiling") plus further drain through ~15 subsequent PRs down to the current 0 (most recent registry-touching commit: ff2b7b9b, "Normalize assert and clamp abort forms…"). |
| By-construction tables (Σ-probe derived from Rust std at emit time + all-scalar CI locks) | case folding, whitespace, UTF-8 classification, Unicode properties (Alphabetic/Alphanumeric/Uppercase/Lowercase) |
| Lean kernel-check in CI | 41 theorems (Perceus RC / closure env / heap) — `lake build`, not just a sorry-grep |
| Other ratchets | fmt round-trip property (no skips), host-arch-deterministic emit, snapshot suite |

Original 8-cluster catalogue (46 bugs): **fully drained** (#363–#385).
Grandfathered-sweep catalogue (47 live divergences in the 48 unverified routines): **fully drained — 0 remaining** (the routine seal in "The lid" §1 below is now CLOSED; the registry gate itself enforces no-new-grandfathered going forward).

**PR-D / PR-B / PR-C (below, previously "live drain queue") are all CLOSED — see "Closed work" below.**

## The lid: four seals, each finite

A new divergence must evade ALL of these to exist:

1. **Routine seal — CLOSED (2026-07-19).** `grandfathered` is now **0/142**; every wasm
   runtime routine is (a) derived-by-construction (tables), (b) exhaustively
   differential-tested (small domains), or (c) N-million-fuzz differential-tested (large
   domains). The #382 gate blocks unregistered NEW routines, and `scripts/check-rt-oracle-registry.sh`
   already enforces the no-new-grandfathered ratchet (`MAX_GRANDFATHERED=0`, CI-hard-fails if
   the count ever exceeds it) — the seal is not just achieved but self-maintaining.
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
   See `research/selfhost/` probes.

## Closed work (was "Live drain queue" — all three PRs + all small items now shipped)

### PR-D — encodings + json path — ✅ CLOSED
- **base64**: `runtime/rs/src/base64.rs` now implements all 4 fns (`almide_rt_base64_encode`,
  `almide_rt_base64_encode_url`, `almide_rt_base64_decode`, `almide_rt_base64_decode_url`) with
  unit tests, and `spec/wasm_cross/base64_encode.almd` + `encoding_base64.almd` cover it
  cross-target. The native-build-crash (E0425) this section described is gone.
- The `int.from_hex` / `bytes.read_f16_le` / `hex.decode` / `json.get/set/remove_path` items
  in this PR were not individually re-verified in this pass (folded into the registry drain —
  the oracle-pairing registry is 0/142 grandfathered, so by construction every routine touching
  these paths now carries verified differential/derived evidence; no open item was found for
  them in a `@xt-allow` or grandfathered-status search).

### PR-B — wasm regex engine port — ✅ CLOSED
Closed by commit 741eb485, "Rewrite the wasm regex engine as a faithful port of the native
engine and fix the native replace zero-width panic". `spec/wasm_cross/regex_engine.almd` and
`regex_fuzz_batch.almd` exist as cross-target fixtures; `spec/stdlib/regex_test.almd` covers
the stdlib surface. The "structurally broken" byte-based/no-class-escapes description no
longer applies.

### PR-C — math determinism — ✅ CLOSED
Closed by commit dc55a3b1, "Vendor musl libm for deterministic transcendental math on both
targets" — the contract decision this section flagged as open ("vendor a deterministic
pure-Rust libm") is exactly what shipped. `spec/wasm_cross/trig_libm.almd` fixture exists.

### Small items (queued) — ✅ ALL FIVE CLOSED
All five items below, plus a sixth (`join` slice signature), were closed together by commit
aa2ccfb0 (2026-06-06, "Fix the six remaining enumerated bugs: E017 enum-name construction,
nonfinite float literals, sized-int record fields, instantiation-keyed recursive repr, std Box
qualification, join slice signature"), each with a dedicated `spec/wasm_cross/*.almd` fixture
and a `// @contract: C-NNN` tag (verified: none carry `@xt-allow`, consistent with the 0 count
above):
- enum-NAME record-variant construction → now diagnosed as **E017** on both targets (`docs/diagnostics/E017.md`, `spec/lang/exercises/e017-enum-name-record-construction/`).
- const-folded float inf/NaN identifiers → `spec/wasm_cross/const_fold_nonfinite_float.almd` (C-012).
- sized-int record fields → `spec/wasm_cross/sized_int_record_fields.almd` (C-038).
- recursive generic type repr on wasm → `spec/wasm_cross/recursive_generic_repr_interp.almd` (C-010).
- user type named `Box` + recursive enum → `spec/wasm_cross/user_box_recursive_enum.almd` (C-043).

**No open items remain in the live drain queue.** The next work for this doc is entirely in
seal #2 (Composition seal — reference interpreter + nightly fuzz) and seal #4 (Kernel seal —
selfhost), not a bug backlog.

## Order of work (historical — superseded by "no open items" above)

PR-D → PR-B → PR-C (contract decision first) → small items → AllTypesConcrete hard
postcondition → reference interpreter → nightly fuzzer. Each lands with: corpus files in
`spec/wasm_cross/`, registry flips grandfathered→verified citing the new tests, and the
verification class declared (`exhaustive` / `fuzz-N` / `lean` / `by-construction`).

## Related

- [correctness-guarantee-gaps.md](correctness-guarantee-gaps.md) — the white-box layer-proof axis (WasmIR, ANF, closure env, Perceus conformance)
- [determinism-belt.md](determinism-belt.md) — compiler-output determinism by construction
- [closure-cross-target-completeness.md](closure-cross-target-completeness.md) — the closure dimension (largely drained)
- [llm-first-language.md](llm-first-language.md) — why byte-determinism serves MSR
