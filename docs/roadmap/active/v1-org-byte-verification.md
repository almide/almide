<!-- description: Org-wide v0==wasm byte-verification sweep and the wasm bug classes it flushed out -->
# Org byte-verification — every repo's own vectors on both targets

Session record (2026-07-02, continuing `v1-porta-read-message-handoff.md`). Goal: the
handoff's steps 1–3 — unblock porta's native build, then widen the byte-match
verification from `wall=0` (lowers) to **the repo's own test vectors running
byte-identically on native and `--target wasm`** across the org.

## Method

For every org repo with tests: `almide test --target native` AND
`almide test --target wasm` must BOTH pass every test. Assertions in the repo's
own suite are the vectors; a pass on both targets is the byte-match. The sweep
script pattern is recorded in this session's history; a repo with no tests
(almide-web, almide-sqlite) cannot be verified this way and stays 🟡.

## porta native build (handoff step 1) — FIXED, 52 → 0 errors

The handoff attributed the porta native block to "toml-dep borrow/clone codegen".
The real decomposition was:

1. **22× E0308 double-wrap** — ResultPropagation Phase 2b (81840f8d) Ok-wrapped
   match-tail arms calling Result-DECLARED effect fns (never sig-lifted → not in
   `lifted_fns`, but already Result-typed). Fixed: a tail whose ty is already
   Result is never wrapped (`b03d71e7`).
2. **28× E0425 extern-fn mismatch** — a module `@extern(rs, ...)` fn emitted
   `use bridge::f as f;` (bare name) while call sites render the flatten prefix
   `almide_rt_<mod>_<f>`. Fixed: the alias carries the prefixed name (`71b22b08`).
3. **Capability E0425 + CapabilitySet E0308** — fixed by cherry-picking the
   develop-side #697/#698/#699 (loop-body Bind ty mangle, TCO shared-mut, TCO
   pre-baked owned params).
4. The auto-? skip-set missed `ok(match parsed { ok/err })` (match behind a
   value wrapper) and any Bind nested below the top level. Fixed with an
   exhaustive-visitor scan applied at every Bind depth (`d5794a86`), in both the
   checker (infer_p5) and lowering (auto_try).

Regression harness: 3 new crossmod-matrix cells + a module-extern native gate
(`6d6adf05`). porta: native build clean, `almide test` 8/8, wasm leg 7/7 (+1
FFI file skipped by design).

## The wasm bug classes the sweep flushed out (all pre-existing, also on develop)

Every one was found by a repo suite trapping/diverging on wasm, minimized to a
pure-stdlib repro, root-caused, fixed, and pinned with a `spec/wasm_cross`
fixture + contract:

| fix | class | contract |
|-----|-------|----------|
| `eb0a0fc3` | string pass-through fast paths (replace/replace_first/pad_start/pad_end/capitalize) returned the INPUT without +1 → pipe chains of no-match links under-flowed the rc (svg escape_attr trap) | C-121 |
| `b78fda19` | record spread byte-copied heap fields without +1, and overrides bypassed `emit_stored_field` (svg doc lost its attrs Map) | C-123 |
| `b17593d2` | value.merge/pick/omit/json.keys allocated the pair list 4 bytes short (no cap word), left cap uninitialized, copied pairs/keys borrowed; value.get/field ok payload borrowed (toml aot silently dropped fields) | C-122 |
| `d4de9c5e` | `Value == Value` compared POINTERS (no deep-eq runtime existed) — `json.get(f,"import") ?? json.null() != json.null()` misclassified every fn as a JS import (almide-wasm-bindgen); + as_array ok payload borrowed | C-124 |
| `9e5927aa` | value.merge dropped a's key positions and mis-handled non-objects vs the native oracle; rewritten position-preserving; both stale `@xt-allow` divergences (value_eq, value_merge) removed | C-103 |
| `86480293` | `bytes.set` stored in place unconditionally (oracle CLONES) — a set through a param clobbered the CALLER's buffer (aes cfb8 NIST vectors wrong); now value-semantic with an AliasCowPass-vetoed `x = bytes.set(x, …)` in-place fast path | C-125 |

Also: rt-oracle registry drift from the v1 file splits repaired (65 entries
repointed, `f121b1ff`) — gate green at 137/137 verified, grandfathered=0.

## Result

All org repos WITH test suites pass both targets: yaml, sha1, toml, svg, rsa,
porta, csv, bigint, base64, aes, almide-wasm-bindgen, almide-lander,
almide-grammar. `BYTE_VERIFIED` in `scripts/org-trust-status.sh` and the
dashboard record the new state. Exclusions: almide-web / almide-sqlite (no
tests — need vectors first), almide-dojo (task-bank fixtures, not a compilable
suite), almide-bindgen (see dashboard).

## Graphics / AI stack spot-check (same day, follow-up)

Recorded in the dashboard's "Graphics / AI stack" section: svg/lumen/homullus
byte-verified (suites, both targets); canvas/wasm-canvas/wasm-webgl/obsid build
clean wasm (browser-hosted — headless run N/A). Three compiler findings:

- **almide-aituber**: wasm emit fails structural validation on develop-v1
  (`type mismatch: expected i32, found i64`) but builds clean on develop
  v0.27.13 — the only v1-vs-develop divergence found; predates the 2026-07-02
  session (reproduced at 59dfd762). Needs a v1-branch bisect.
- **almai**: `[COMPILER BUG] unresolvable bare type name(s) reached codegen` on
  BOTH branches — 8 provider modules define the same type names and bare refs
  can't resolve (#433 class).
- **nn**: unresolved `__tco_tmp_data` (ty=Unknown) on BOTH branches — the TCO
  temp misses type resolution; the build is honestly refused.

## Second pass — the v0 leftovers rescued onto v1 (2026-07-02, same day)

Directive: don't backport to v0; make v1 the branch where these work.

- **nn: 0/13 → 13/13 both targets** (now in `BYTE_VERIFIED`). Five compiler
  fixes, each with the failing shape minimized first: (1) TCO borrow-preserved
  Bytes temps carry their REAL type; the Rust-side "no annotation" intent moved
  to `codegen_annotations.infer_binding_tys` (the old `Ty::Unknown` smuggle was
  rightly refused by the ConcretizeTypes gate); (2) the #653 lambda-param pin
  no longer writes the CALLEE's own unbound generic into the param (nested-HOF
  element types silently defaulted to Int — C-126), while in-scope rigid
  generics (`names[T: Labelled]`) still pin; (3) `AlmideMatrix` got an
  `AlmideRepr` impl (constructor form, the Set precedent); (4) the SIMD
  fast-exp (avx2/neon/simd128) clamps its input to ±708 — the softmax `-1e9`
  mask wrapped the `(k+1023) << 52` exponent bit trick and corrupted whole
  rows (masked attention returned wrong NIST vectors); (5) `unwrap_or` sizes
  its payload from the DEFAULT argument when the chain type is unresolved
  (`list.find |> option.map |> option.unwrap_or` emitted an `if (result i32)`
  block with an `i64.const` arm — C-127).
- **almide-aituber: fixed** — the v1-vs-develop divergence was the missing
  develop-side #717 (recompute if/match/block type after auto-? unwraps an
  effect branch); cherry-picked.
- **almai: CLOSED (2026-07-03).** The nominal/structural fork is resolved by
  the STRUCTURAL-TWIN merge: the checker demonstrably unifies same-base-name,
  same-shape record decls across modules (which nominal name a site lands on
  is an accident of constraint order), so codegen now realizes that semantics
  — the flatten pass groups decls by (base name, shape fingerprint) and maps
  every twin to ONE canonical struct; the bare-ref repair accepts an all-twin
  owner set. Same-name DIFFERENT-shape types keep their distinct mangles (and
  the checker already rejects them inside one package as E020). almai's native
  suite: 56 tests green. Guard cell: `structural_twin_records_flow_both_directions`.
  (Superseded narrative below kept for the record:) The E0063 class is fixed (the flatten
  mangle now remaps name-keyed codegen annotations — default/boxed fields,
  ctor_to_enum — so module-type field DEFAULTS fill). The remaining ~37 errors
  are the root `LLMResponse`/`ToolCall` vs `openai.LLMResponse` etc.
  NOMINAL/STRUCTURAL fork: the checker accepts same-shape records across
  modules, codegen emits distinct Rust structs. Resolving it is a language
  decision (reject in check, or insert conversions); red on develop too.
- Debug aid: `ALMIDE_DUMP_INVALID_WASM=<path>` writes the invalid module
  (name section intact) when structural validation fails, so `wasm-tools
  validate/print` can name the broken function.

## Third pass — the last two known leftovers (2026-07-03)

- **svg cross-module render stack overflow: FIXED.** The overflow was the
  COMPILER's: `unify_structural` on a DIFFERENT-named nominal pair (`El` vs its
  module twin `lib.El`) expanded both sides and recursed into the fields —
  a RECURSIVE type re-reaches the same pair inside `children: List[El]`,
  forever. Equi-recursive guard: an in-progress pair set in the checker; a
  re-encountered pair unifies coinductively. svg renders byte-identically on
  both targets now. Guard cell: `recursive_record_type_cross_module`.
  Debug aid: `ALMIDE_TRACE_PASSES=1` names each pass BEFORE it runs.
- **Cross-module `@inline_rust`: FIXED, both legs.** wasm skipped EVERY
  `@inline_rust` fn as "dispatch-only" — but a user package's fn carries a
  real Almide body as its portable implementation (the attr is a native-only
  optimization); now only Hole-bodied declarations skip, so wasm compiles the
  body. Native pasted the template's bare struct tokens (`Cfb8State { .. }`)
  into the post-mangle world (E0422); StdlibLowering now requalifies a
  package template's own type tokens to the canonical dotted names and the
  flatten pass rewrites dotted tokens inside templates to the flat struct
  names. aes cfb8 NIST F.3.7 passes cross-module, byte-identical on both
  targets. Guard cell: `inline_rust_with_fallback_body_cross_module`.

## Remaining threads

- almide-web / almide-sqlite need test vectors before they can be verified.
- Handoff step 4 (read_message on the VERIFIED render_program path — wasm JSON
  codec self-host) remains open, unchanged.
