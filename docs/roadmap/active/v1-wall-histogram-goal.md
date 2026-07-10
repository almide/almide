<!-- description: GOAL PROMPT — v1 wall histogram: self-host the regex family (381 walls), then json/bytes tail -->
# GOAL PROMPT — v1 wall histogram: the regex family, then the json/bytes tail

> **Read first**: `proofs/corpus-wall.sh` output's Unsupported histogram (the
> roadmap this goal executes), the self-host linkage pattern
> (`stdlib/fan_map.almd` + `crates/almide-mir/src/render_wasm/registry.rs` +
> `purity.rs` — registry name, PURE_MODULES drift gate, typed routing with
> unlinkable-variant `_x` walls), v0's wasm regex for SEMANTICS ONLY
> (`crates/almide-codegen/src/emit_wasm/rt_regex.rs` / `rt_regex_p2.rs`,
> `runtime/rs/src/regex.rs`), and the self-host constraints recorded in
> [[project_v1_mir_trust_spine]]-era notes: **bundled-module public-sig fns
> only are callable; self-host modules cannot call each other's internals**.

## Context (2026-07-10, commit `7b91dcac`)

Corpus: 4,745 in-profile / 306 walled real. The histogram's dominant buckets:

| bucket | walls |
|---|---|
| regex.is_match / find / replace / captures / full_match / split / replace_first / find_all | **381** |
| json.root / json.field / json.index | ~116 |
| bytes.append_u8 | 50 |
| match over an UNTRACKED subject with a call-bearing arm | 33 |
| string interpolation in call-arg position | 30 |

The v1 trust-spine ethos: stdlib rides as SELF-HOSTED `.almd` (the code then
carries its own ownership/names/caps certificates through the proven checker —
zero trusted runtime growth), linked by registry name with typed routing.
`fan.map`'s 4-variant routing is the house pattern.

## The goal (one line)

> **Open the regex family's 381 walls with a SELF-HOSTED Almide regex engine
> (v0-byte-identical per function, feature-gated by the corpus's REAL
> patterns, honest walls beyond), then sweep the json.root/field/index and
> bytes.append_u8 tail — driving walled-real from 306 toward the double
> digits, with every opened function's witness proven.**

## Non-negotiable invariants

1. **Honest wall over silent miscompile, always**: a pattern feature the
   engine does not implement must fail CLOSED (unlinked `_x` wall or an
   explicit runtime reject matching v0's behavior) — never a wrong match
   result. Byte-parity vs v0 (`almide run` on both targets) per opened
   function BEFORE commit; deferred-Opaque is the known silent-miscompile
   breeding ground (the computed-list lesson) — gate first, emit second.
2. **Zero new trusted runtime in v1**: the engine is `.almd` self-host (its
   own PCC certs), NOT a WAT port of `rt_regex.rs` (that would grow the
   renderer contract the A1 work zeroed). v0's implementations are the
   SEMANTIC ORACLE only.
3. **Registry discipline**: PURE_MODULES drift gate (file must exist), typed
   routing per (pattern-arg, subject) signature, self-host fns need public
   type sigs (internals are not callable cross-module — inline helpers with a
   distinctive prefix, the `__rts_*` convention).
4. Tiered testing (lang → stdlib → integration), stop on first red; corpus
   histogram re-measured per stage and the delta recorded here.
5. Commit per stage at all-green (English, one line, no prefix).

## Sub-tasks (in order — each independently shippable)

**0 — SCOUT (do first, record findings here).**
- Extract the corpus's ACTUAL regex patterns: grep `spec/` (and the exercises
  the corpus includes) for `regex.` call sites; classify the pattern strings
  by feature (literals, `.`, `[...]`/`[^...]`, `*`/`+`/`?`, `^`/`$`, groups
  `(...)`, alternation `|`, escapes `\d\w\s`, `{n,m}`). The feature set the
  corpus USES is the stage-1 scope — record the histogram of features.
  **DONE (2026-07-10): 270 unique literal patterns in spec/. Feature counts:
  alternation 164, `+` 132, class-escapes (`\d\w\s…`) 123, charclass 117,
  `*` 111, non-ASCII text 108 (UTF-8 correctness is load-bearing!), `.` 108,
  `?` 104, anchors 96, negated charclass 35, groups/captures 19,
  `{n,m}` counted repetition ZERO — the stage-1 scope is the full basic
  alphabet WITHOUT counted repetition. Adversarial alternation edges are
  IN-CORPUS (`a|`, `|a`, `a||b`, `a|||` — empty alternatives) and must match
  v0's semantics exactly.**
- Read `runtime/rs/src/regex.rs` + `rt_regex.rs` for the exact SEMANTICS v0
  implements (greediness, empty-match advance, capture numbering, replace `$n`
  syntax, split edge cases — empty pattern, trailing empty fields). These
  edge cases are where parity dies; list them as test cases up front.
- Check how v0 wasm exposes regex (per-call WAT emit? a compiled NFA?) — for
  UNDERSTANDING only (invariant 2).

**1 — the engine core (`stdlib/regex_engine.almd` or split files).**
A backtracking matcher over the scouted feature set: `__re_match_at(pattern,
text, pos) -> Int` (match end or −1) style helpers with public-sig entry
points. Byte-level string ops only (string.len / prim loads or the existing
string API — mind the self-host callable-surface constraint). Determinism and
TERMINATION by construction (fuel or structural descent on (pos, pattern) —
an adversarial `(a*)*` must not hang the build; record the strategy).
Ship `regex.is_match` + `regex.full_match` first (Bool — simplest routing):
registry link, typed gate, fixtures, parity, corpus delta.

**2 — positions and pieces**: `regex.find` (first match, Option/position
semantics — mirror v0's return type exactly), `regex.find_all`,
`regex.captures` (group extraction — scout v0's capture representation first).

**3 — writers**: `regex.replace` / `replace_first` (with v0's `$n`/literal
replacement semantics) and `regex.split` (empty-field edge cases from the
scout list).

**4 — the tail sweep**: `json.root` / `json.field` / `json.index` (~116 — scout
what they lower to today; likely a value.* linkage gap, much smaller than
regex) and `bytes.append_u8` (50 — likely a MakeUnique/push-in-place shape;
check the existing bytes.set machinery). Each: same parity + cert discipline.

**5 — re-measure**: corpus-wall histogram before/after per stage; update this
file and certificate-format-v1.md's coverage note. Target: walled-real 306 →
double digits after regex, further after the tail.

## Verification ladder (per stage)

```
almide test spec/stdlib/ && almide test  # parity first (both targets)
cargo test -q -p almide-mir
proofs/gate.sh && proofs/corpus-wall.sh  # PCC + kernel oracle + histogram
cargo test -q
```

## Exit criteria

- [ ] Every regex.* corpus call site either EXECUTES v0-byte-identically or
      walls on a RECORDED unsupported feature (list the residue here).
- [ ] Engine edge-case suite green (greediness, empty match, anchors, split
      empties — the scouted list), on BOTH targets.
- [ ] json.root/field/index + bytes.append_u8 buckets opened or their real
      blocker recorded.
- [ ] Histogram deltas recorded; corpus PCC (binary + kernel oracle) ACCEPT
      throughout; pushed at all-green; Trust Spine green.

## What NOT to do

- No WAT/Rust regex port into the v1 renderer (invariant 2).
- No "close enough" match semantics — v0 is the oracle, byte-for-byte.
- No opening the untracked-match / interp-in-call-arg buckets here (separate
  lowering work, different skill set — keep this goal stdlib-shaped).
- Do not weaken the purity/drift gates to force a link.
