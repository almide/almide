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

## Remaining threads

- **Cross-module `@inline_rust` fns** (aes cfb8_encrypt via `import self`) ICE
  on wasm (`no WASM dispatch`) and fail native (the inline template references
  the dep's `native/` module that a cross-module caller doesn't inject). Only
  affects calling such fns from OUTSIDE the package on this shape.
- **svg cross-module render stack overflow on BOTH targets** (`import self as
  svg` + `render(group(...))` recursion) — a separate, target-independent bug.
- almide-web / almide-sqlite need test vectors before they can be verified.
- Handoff step 4 (read_message on the VERIFIED render_program path — wasm JSON
  codec self-host) remains open, unchanged.
