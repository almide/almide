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
  **PARTIAL (2026-07-10, runtime/rs/src/regex.rs lines 1–260 read):**
  - The engine is **CHAR-based** (`Vec<char>` — Unicode scalar values): `.`
    and classes match one CHAR; the Almide port must decode UTF-8 (the 108
    non-ASCII corpus patterns make this load-bearing).
  - AST: `Lit(char) | Dot | Class(Vec<(char,char)>, negated) | AnchorStart |
    AnchorEnd | Group(alts, capture_idx_1based)`; reps `(min, max)` from
    `* + ?` ONLY (no `{n,m}` — matches the corpus scout).
  - BACKTRACKING, GREEDY (`rx_match_rep`: try one more first, then the rest
    once `count >= min`), with the ZERO-WIDTH GUARD `consumed > 0 || count ==
    0` (prevents `(a*)*`-style hangs — port this exact guard).
  - Alternation: alts split on top-level/group `|`; EMPTY alternatives are
    natural (an empty alt seq matches zero-width) — the corpus `a|`/`|a`/
    `a||b` edges fall out of this.
  - `rx_find_at`: leftmost scan `i in start..=len`, per-alt order preference;
    `is_match` = find anywhere; `full_match` = top-level alts from 0 AND
    `end == chars.len()` (checked AFTER the alt match — NOT per-alt anchored);
    `find` returns the matched SUBSTRING (`Option<String>`), positions are
    char-index internal only.
  - **API semantics COMPLETE (lines 260–406)**: `find_all` — repeat leftmost;
    zero-width advance = `end + 1` (skip one char). `replace` — the
    replacement is a PLAIN LITERAL (NO `$n` group refs!); scan loop: emit
    `chars[pos..start]`, emit rep; zero-width → ALSO emit the char at `end`
    and advance `end + 1` (at end-of-string: just advance — the
    `replace_empty_match_at_end_no_panic` regression). `replace_first` — one
    find from 0, splice, else the input unchanged. `split` — zero-width match
    AT the current pos → push ONE char and advance 1; else push
    `chars[pos..start]`, `pos = end`; on no-match push the tail (NO
    trailing-empty suppression: `split(",", "a,")` = ["a", ""]).
    `captures` — `ncap == 0 → None`; else the FIRST match's groups,
    an unmatched group = "" (not None). The v0 unit tests at the file's end
    are the exact parity oracle set (`x*`/"ab"→"-a-b-", `b?`/"abc"→"-a--c-",
    `a*`/"aaa"→"--", "本a" multibyte boundaries, the empty-alternation
    family) — port them VERBATIM into the engine's spec tests first.
- Check how v0 wasm exposes regex (per-call WAT emit? a compiled NFA?) — for
  UNDERSTANDING only (invariant 2).

**STAGES 1+2 SHIPPED (2026-07-10, bcc02de4)**: `stdlib/regex_engine.almd` — the
full byte-address/char-decode backtracking engine (UTF-8 on the fly, v0
quirk-for-quirk: first-win alternation, atomic group boundary, zero-width rep
guard, anchor-quantifier-ignored, in-class escape expansions) + SEVEN APIs
linked: is_match, full_match, find, find_all, replace, replace_first, split.
Parity probes v0-identical on 40 oracle cases (incl. `-a-b-`/`--`/`-本-a-`
zero-width replaces, empty-field splits, `(a|ab)c`→F group quirk, `[あ-ん]+`).
mir 583, spec 283/283, gate 36 rows, corpus PCC+kernel ACCEPT.
**Histogram insight**: the corpus walls are MULTI-BLOCKER — opening
find/replace moved their functions to the NEXT blocker (captures 49 remains,
plus match-over-untracked-subject for `match regex.find(...)` shapes — the
fan.map-style subject hoist + seed for self-host Option-returning fns is the
unlock), so walled-real stays 296 until the LAST blocker per function falls.
**STAGE 3 SHIPPED (2026-07-10, d13c0163): the regex FAMILY is COMPLETE — all
8 APIs self-hosted** (`regex.captures` via the `_c` capture-threading matcher:
caller-owned Int-list buffer passed as a raw data address, v0's clone-save/
restore at every alternative and greedy-extension choice point, group ordinal
= pre-order '(' count, unmatched group = ""). `regex.find`/`captures` are also
seeded (`is_self_host_option_module_fn`) + subject-hoisted
(`desugar_match_subject_hoist`) so `match regex.find(...) { some/none }`
executes. Parity: capture pairs `10|25`, `bob|host`, the `(a|)` empty group,
ncap-0 → NONE, `(a)(b)?` unmatched-group-"" — all v0-identical. The regex
buckets are GONE from the histogram (381 → 0). Walled-real stays 296: the
regex-test FUNCTIONS are multi-blocked — their residual blockers are the
generic buckets (assert-arg shapes → match-untracked 33 / interp-in-call-arg
30). **bytes.append_u8 SHIPPED (48d700c3)**: the same statement-rewrite as
bytes.push (`buf = bytes.append(buf, x)` — the self-hosted functional append);
parity incl. loop pushes. Its bucket is gone (50 → 0).
**THE ROAD TO DOUBLE DIGITS, decomposed (2026-07-10 diagnosis — per-function
wall names via `WALL_NAMES=1 classify_corpus`)**: the 291 walled-real fns
spread thin over ~60 files (top file = 15); they decompose into SIX brick-
scale design pieces, NOT linkage gaps:
1. **Nested-variant-payload matches** (the match-untracked 33 bucket's core):
   `match e { err(Overflow(msg)) => … }` — a ctor pattern INSIDE ok/err/some
   (the Camp-4 frontier); also the cross-module `mod.Type.method()` Codec
   roundtrips. **MATCH side SHIPPED (0b98c23e)**: `group_option_result_arms`
   now admits nested user-ctor columns (`err(Overflow(msg))` regroups to
   `err($q) => match $q { Overflow(msg) => … }`), walls 296 → 292. The
   CONSTRUCTION side **SHIPPED (030e9d85)**: `err(<variant ctor>)` for
   `Result[T_scalar, <user variant>]` — `try_lower_result_err_variant_ctor`
   (len-as-tag via `materialize_opt_str_some`; rich payloads route to the
   GENERATED `$__drop_res_<V>` wrapper drop, flat payloads keep the exact
   flat drop), wired at bind / arm-chain / tail. NOT self-tracked at bind —
   the corpus PCC gate caught the call-arg double-track (`idd`) on first
   contact and it was root-fixed (ctor arms leave tracking to callers). The
   regroup was RESTRICTED to Option/Result subject columns (user-variant
   nested arms keep the #610 refinement machinery — regrouping shadowed it).
   Net +3 corpus fns (by-name diff, zero newly-walled); v0-identical on
   rich/flat err paths and the ok path. (The resolved design, for the
   record:) **LAYOUT (seed_variant_param
   audit)**: the reader seeds Result[scalar, heap] as **LEN-AS-TAG**
   (`materialized_results` + `heap_elem_lists`; Err = len 1 + payload HANDLE
   at slot 0, bound BORROWED by the err arm — existing machinery), so the
   err-variant ctor must materialize exactly that: build the variant block
   (`try_lower_variant_ctor`), store its handle at slot 0, len = 1 (the
   `materialize_result_str(value_ok=false)`-family len-as-tag builder), and
   DETACH the variant's own scope-end drop (moved into the Result). The ONE
   NEW piece is the DROP of a RICH-variant Err payload: the seed's flat
   `DropListStr` rc_decs slot 0 only — leaking the variant's own heap fields
   (`Overflow(String)`) — so a rich payload needs a len-as-tag wrapper
   recursion (a `reserr:<V>` sibling of `optrec:`/`resrec:` in
   `variant_drop_handles` whose render recurses into slot 0 via
   `$__drop_<V>` when len == 1); a FLAT variant payload (`DivZero`) is exact
   under the existing flat drop. Adversarial probes required on: ok-path
   scalar read, err-path rich/flat payloads, the leak loop, and the
   `Result[Int,Int]` both-scalar sibling (recorded misread gap — do NOT
   regress it).
2. **Interp repr coverage** (the interp-in-call-arg 30 bucket +
   compound_repr_interp's 15). **PARTIAL (32232105)**: literal record/tuple
   parts now HOIST to statement-level binds
   (`desugar_interp_literal_aggregate_hoist` — a Block in call-arg walls, so
   the binds prepend to the enclosing statement), opening the heterogeneous-
   tuple class; walls 293 → 290. REMAINING sub-family (diagnosed): VARIANT
   reprs (`${Overflow("x")}` — tuple/record-payload/nullary/recursive/generic)
   and recursive/anonymous-record reprs — needs **GENERATED repr sources**
   symmetric to the generated drops (`generate_variant_repr_sources` emitting
   per-ADT `__repr_<V>` Almide fns, v0's compound Display as the byte oracle),
   plus nested list-of-maps and the annotated empty map. **VARIANT REPRS
   SHIPPED**: `generate_variant_repr_sources` (lower/mod.rs, symmetric to the
   generated drops) emits per-variant `__repr_<V>` Almide fns for the
   FIXPOINT-emittable set (ctor fields all Int/Bool/String/emittable-variant —
   covers nullary, tuple-payload, NESTED and RECURSIVE variants), with shared
   `__repr_quote`/`__repr_esc_*` string-escape helpers (v0's Display escape
   set `\" \\ \n \r \t`, byte-oracled via od). Leaf routing: `interp_part_leaf`
   routes a `Ty::Named` part with `resolve_aggregate == None` to a
   `CallTarget::Named __repr_<drop_fn_ident>` call — previously ALL such parts
   hit the catch-all unlinked `compound.to_string` (wall), so the change is
   STRICTLY opening (a non-emittable variant leaves `__repr_` unlinked = the
   same honest wall). Injected in pipeline.rs + the tests_part1 lower_source
   drops block. Parity: `Overflow("x")`/`DivZero`/`Pair(3, true)`/record,
   escapes, `Wrap(A(3))` nested, recursive `Node(Leaf(1), …)` — all
   v0-identical. **Walls 290 → 272 (−18)**; mir 583, spec 283/283, gate,
   corpus PCC + kernel oracle all green.
2b. **List[Option/Result] LITERALS SHIPPED (walls 272 → 262, −10 by-name, zero
   newly-walled)**: `try_lower_record_list_literal_as` gained TWO ctor element
   classes via the shared `lenlist_elem_class` (lower/mod.rs — the classifier
   the injection pre-scan and the builder BOTH consult): `Flat`
   (`Option[scalar]` — per-element `rc_dec` of `DropListStr`/`heap_elem_lists`
   is exact) and `LenLoop` (`Option[String]`, `Result[scalar,String]`,
   `Result[String,String]` — owned handle slots under len-as-tag, freed by the
   GENERATED `$__drop_list_lenlist` source, `variant_drop_handles` key
   `list_lenlist`). Elements lower via `try_lower_option_ctor` (ctors) or
   `lower_owned_heap_field` (Var/call). TYPE-driven caller-side routing added
   at ALL TEN `is_heap_elem_list_ty` registration sites (`is_lenlist_list_ty`
   checked first — a call-returned `List[Result[_,String]]` bound by a caller
   would otherwise take the flat drop and LEAK each Err payload);
   `copy_heap_drop_class` propagates via variant_drop_handles. StringInterp
   payload arms added beside every ConcatStr piece arm (`err("bad ${id}")`,
   `some("v=${x}")`). **The PCC ownership gate caught the never-err-LIFTED
   effect-call element on first contact** (`[step(), step()]`,
   autotry_construction: the lift rewrites the call type to the RAW payload,
   so a scalar landed in a handle slot — invalid wasm + an unacquired `m`
   witness): non-ctor ctor-class elements now REQUIRE `e.ty == elem_ty`
   (decline → honest wall). option.collect opens with the literals
   (self-hosted already); result.collect self-host is the follow-up. Parity:
   bind/tail/match/leak-loop probes v0-identical; full ladder green.
2c. **Unit-main unwrap + the main-err protocol SHIPPED (walls 262 → 251, −12
   opened / +1 honest)**: THREE pieces. (i) Option-`!` in the effect-unwrap
   desugar: `build_unwrap_match` now builds Option-POLARITY arms (`none =>
   fail, some(x) => cont` — Option's len-as-tag is opposite Result's, so the
   Ok/Err skeleton fired the fail arm on success), gated to SCALAR Some
   payloads; this opened the sized_conversion family
   (`int.to_int8_checked(42)!.to_int64()` chains). (ii) The UNIT-MAIN failure
   protocol (v0 oracle: `Error: <msg>` stderr + exit 1): main is void (the
   err arm value was silently DISCARDED — erring mains exited 0), so a
   `unit_main` flag threads through `desugar_effect_unwrap`/`desugar_all`
   (same tree on the lowering AND count-gate sides) and the fail arms become
   `{ let $m = "Error: " + e + "\n"; let $h = prim.handle($m); prim.die($h) }`
   (the SPLIT form — a nested `prim.handle(<Var>)` declines inside a match
   arm), plus `desugar_unit_main_err_arms` for the FRONTEND auto-? residue
   (`err(e) => err(e)` arms built before the MIR desugar sees an Unwrap; a
   user cannot type `err(e)` as a Unit arm, so every such arm IS the auto-?
   artifact). (iii) The RETURNING-main `_start` protocol: the old Err check
   read @16 (the cap-as-tag offset — always 0 under len-as-tag, so an erring
   explicit-Result main silently exited 0); now len@4 != 0 routes to the new
   `$__main_err` preamble helper (three-span STDERR write reusing the
   div-zero line's "Error: " head + "\n" tail, then proc_exit(1)) — C-035's
   v1 realization, added to TERMINATION_FLOOR_FNS. Failure-path probes:
   unwrap-none main and auto-?-err main both `Error: <msg>` + exit 1,
   v0-identical. One main newly-walled (cross_module_unit_effect — its
   previously-"open" lowering silently swallowed the err; now an honest
   heap-result-match-returned wall). Full ladder green.
2d. **Borrowed-param ctor payloads SHIPPED (walls 251 → 244, −7, zero
   newly-walled)**: `err(msg)` / `ok(s)` / `some(s)` of a BORROWED PARAM
   (`effect fn fail_with(msg: String) = err(msg)` — the fan-family tail
   ctors) now Dup the param's handle into a fresh CO-OWNED ref (cert `a`)
   and move THAT into the wrapper — the borrow-then-Dup discipline the
   spread-record copy proves; all four Var piece arms in binds_p4 gained the
   sibling case. Parity + PCC ownership ACCEPT.
2e. **Nested-BUILTIN pattern regroup SHIPPED (walls 244 → 238, −6, zero
   newly-walled)**: `group_option_result_arms`' column classifiers admitted
   nested `IrPattern::Constructor` (user ctors) but NOT the nested BUILTIN
   wrappers — `some(some(n))` / `some(ok(v))` / `ok(none)` (the
   match_exhaustive nested-Option/Result class) failed `scalar_col` and the
   whole regroup bailed. `scalar_col`/`is_nested_ctor` now admit
   Some/None/Ok/Err with plain inners; the regrouped inner match over the
   seeded payload bind lowers through the ordinary Option/Result machinery.
   Parity on Option[Option[Int]] / Option[Result[Int,String]] /
   Result[Option[Int],String] probes; full ladder green.
2f. **Scalar-tuple literal match SHIPPED (walls 238 → 237)**:
   `desugar_scalar_tuple_literal_match` — a match over a TUPLE literal of
   scalar components with literal/bind/wildcard tuple arms (`match (a, b) {
   (true, true) => "tt", … }`, bool_pair) rewrites to the PROVEN hoist +
   if-chain form (components hoisted once, first-match = chain order, last
   arm = the unconditional else — sound by frontend exhaustiveness). Small
   yield (most remaining tuple-subject matches carry VARIANT components —
   `(some(a), some(b))` — the next extension).
2g. **`?` bridge + str-str `??` SHIPPED (walls 237 → 231, −6, zero
   newly-walled)**: `desugar_to_option_calls` rewrites `r?` over
   `Result[Int, String]` into the SELF-HOST bridge call `result.to_option(r)`
   — a REAL IR Call node, so every position lowers through the proven
   Module-call machinery and the caps `mir == ir` count sees it on both sides
   BY CONSTRUCTION (the desugar-before-both discipline; ToOption was
   previously fully deferred → strict-value wall). And the `??` Var gate now
   admits a `materialized_results_str` Var for `Result[String, String]`
   ONLY (routing to the existing `result.str_unwrap_or` helper — any other
   _str-set shape would misread the len-as-tag String branch); the
   classify-side `value_operand_lowers` Var case credits the +1 (the mir>ir
   breach the corpus gate caught on first run — fixed same stage). NOTE: a
   blanket "??/?-to-Match" desugar was BUILT AND REVERTED first: the by-name
   diff showed it net-negative (the existing UnwrapOr executable subset
   handles Value/json/base64 shapes BETTER than the match form) — decline-
   point extension beats blanket rewriting.
2h. **Defaulted variant-record fields SHIPPED (walls 231 → 216, −15, zero
   newly-walled — the session's best single lever)**: `IrFieldDecl.default`
   (the declared default EXPR) now rides `VariantLayouts.ctor_field_defaults`
   (populated in `build_variant_layouts`), and `try_lower_variant_ctor`'s
   record-ctor arm FILLS an omitted defaulted field with the declared expr —
   evaluated at construction exactly as v0 does — instead of declining
   (`Rect { width, height }` with `color = ""`, the default_fields family).
   Gated CALL-FREE via the new `expr_contains_call` (a call-bearing default
   would add a MIR call the counted IR lacks — mir>ir). Parity on
   omit-all/override-one/override-all/empty-list-default probes.
2i. **Trailing-wildcard + record-pattern regroup SHIPPED (walls 216 → 213,
   −3, zero newly-walled)**: `group_option_result_arms` now (a) admits a
   TRAILING `_` catch-all (`_ => assert(false)` — the codec-roundtrip class):
   its body duplicates into each multi-arm bucket's inner fallback AND stays
   the outer last arm (an `ok(<unmatched ctor>)` falls through the INNER
   match, an `err(_)` through the OUTER; duplication is count-safe — both
   sides read this tree); (b) admits nested `RecordPattern` columns
   (`ok(Tag { name, c })`) with all-plain field patterns. RESIDUAL (recorded):
   the codec-roundtrip family itself still walls one level deeper —
   `try_lower_result_match` does not yet bind a heap-Ok USER-VARIANT payload
   (`Result[Shape, String]`'s `ok($q) => match $q { <variant arms> }`; the
   payload var needs the custom-variant seed) — the next design piece for
   the untracked-subject bucket (~7 codec fns).
2j. **Wildcard-as-err SHIPPED (walls 213 → 206, −7, zero newly-walled) — the
   codec-roundtrip family OPENS end-to-end**: `try_lower_result_match`'s arm
   parser admits a top-level `_` catch-all as the non-Ok arm (tag != 0 ⇒ the
   wildcard body, binding nothing — positionally `err(_)` once Ok holds the
   other arm). With 2i's regroup, `match Shape.decode(Shape.encode(X)) {
   ok(<ctor pattern>) …, _ => assert(false) }` now lowers: the Named-call
   subject materializes inline (already tracked), the regrouped inner
   custom-variant match dispatches TYPE-driven off the borrowed @12 payload
   handle (no extra seed needed — the ok(<variant>) construction side
   already lowered). cd1/cd2/cd3 probes v0-identical.
2k. **`some(<pure module call>)` payloads SHIPPED (walls 206 → 205)**: the
   OptionSome heap piece matches (bind position in binds_p4 AND the
   heap-result-arm position in control_p4) admit a PURE Module call yielding
   a String payload (`some(string.slice(s, 4, n))` — the parse_tag tail-if
   family) via lower_pure_module_value_call, moved into the Some slot.
2l. **Cross-module top-let bridge SHIPPED (walls 205 → 204 net; +3 opened /
   2 honest)**: main-program references to a sibling module's top-let
   (`toplib.SYSTEM`) carry MAIN-side VarIds while the globals union keyed
   MODULE-side ids — unrelated regions that COLLIDE (main VarId(2) resolved
   an unrelated module entry's init) or miss (unbound). New
   `bridge_cross_module_toplets` aliases main-side var-table ids by NAME +
   TYPE onto the module top-let's (ty, init), skipping ambiguous names;
   composition order = module union (fallback) → bridge (overrides
   collisions) → main's own top-lets (win). A global whose init is ANOTHER
   global (`let DIRECT = letlib.GREETING`, #632) recurses through
   value_or_global. The 2 newly-walled init_order shapes (`letlib.welcome()`
   / `list.len(...)` call-inits) previously "lowered" by resolving the WRONG
   colliding global — silent latent miscompiles, now honest walls. Module
   variant-layout defaults also union (`ctor_field_defaults.extend` was
   missing in both pipeline and classify).
2m. **testing.assert_* self-host SHIPPED (walls 204 → 198, −6, zero
   newly-walled) — UNDER 200**: `stdlib/testing_assert.almd` (assert_gt/lt/
   approx/contains/some/ok — each PURE-OR-HALT: the comparison is pure, a
   failure aborts via prim.die, the same class as the div-zero trap), on the
   `is_pure_fn_in_impure_module` per-fn whitelist (the `testing` MODULE stays
   impure-plain) + PURE_MODULES for the file-level drift gate. TYPED SIGS
   ONLY: `assert_some` = Option[String] (len-as-tag), `assert_ok` =
   Result[String,String] (cap-as-tag@16) — an off-signature call site is
   renamed `_x` by `desugar_offtype_testing_asserts` (count-invariant), so a
   different instantiation walls honestly instead of misreading a block.

2n. **fan.settle / fan.any value positions SHIPPED (walls 198 → 188, −10,
   zero newly-walled)**: `desugar_fan_race_any` gained POSITION-LIMITED
   (bind-value / block-tail) rewrites — `fan.settle([thunks])` becomes the
   results LIST LITERAL (the 2b lenlist machinery materializes it);
   `fan.any([thunks])` becomes the first-Ok chain VALUE (`match t0 { ok($x)
   => ok($x), err(_) => <next … err("fan.any: all candidates failed")> }`).
   Position-limiting matters: an `!`-wrapped `fan.any(…)!` must stay for the
   effect-unwrap desugar (whose match shape the PRE-order inliner handles) —
   the first, any-position version left a match-over-match and REGRESSED
   fan_any_allfail/fan_race_any_wasm (by-name diff caught it; reworked).

2o. **Record-variant repr FORM FIX (correctness, walls unchanged)**: adversarial
   probing of the shipped 2a generator found a WRONG-BYTES latent: a
   RECORD-variant case (`Tag { name: String, n: Int }`) rendered
   tuple-style `Tag("hi", 3)` where v0 renders `Tag { name: "hi", n: 3 }` —
   no corpus in-profile shape exercised it, so parity never saw it. The
   generator now carries field names per case and emits the brace form
   (name-prefixed fields, ` }` closer). vr/vr2/vr3/vr4 probes v0-identical.

2p. **Named-record reprs SHIPPED (walls 188 → 185, −3, zero newly-walled)**:
   the generator emits `__repr_rec_<R>` for the record fixpoint (fields Int/
   Bool/String/emittable-variant/emittable-record/`List[<emittable record>]`
   — the recursion that renders `Node { val: 1, kids: [Node { … }] }` and
   the mutually-recursive A/B shapes) plus the `__repr_list_rec_<R>` element
   loop; `interp_part_leaf`'s non-expandable NAMED-record branch routes to
   it (an unemitted record leaves the call unlinked = the same honest wall,
   same single call node). ANONYMOUS records stay walled (the sorted-field
   hash-name variant is the residual sub-piece). The early `emittable
   variants empty → bail` now also considers records.

2q. **Anonymous-record reprs SHIPPED (walls 185 → 182, −3, zero
   newly-walled)**: `collect_interp_anon_records` scans interp parts for
   structural `Ty::Record` shapes; the generator emits `__repr_anonrec_<hash>`
   per shape (scalar/String fields) reading slots at SOURCE index while
   concatenating in SORTED-name order (v0 sorts anon fields; the v1 block
   lays them in source order). `interp_part_leaf` routes anon records to it
   UNCONDITIONALLY — the inline display_aggregate expansion reads structural
   order and would emit WRONG bytes for an unsorted literal. The
   `interp_synthetic_call_names` mirror was updated to the NEW leaf decision
   tree (anon → 1 repr call; non-expandable Named → `__repr_rec_<R>`) — the
   corpus mir>ir gate caught the drift on first run (3 fns), fixed same
   stage. Oracle: `{ zebra: 1, apple: 2, mango: 3 }` → `{ apple: 2, mango:
   3, zebra: 1 }` v0-identical.

2r. **Camp-4 opener: merge-based tail Result match SHIPPED (walls 182 → 181,
   the machinery compounds)**: THREE pieces open the `compute` class
   (`match safe_div(a,b) { ok(v) => ok(int.to_string(v)), err(DivideByZero)
   => ok("infinity"), err(e) => err(e) }` — v0-identical end-to-end):
   (i) `try_lower_result_match_value` — a TAIL-value match over a len-as-tag
   Result subject with HEAP-result arms: the subject materializes as an
   owned tracked temp (freed by the scope epilogue AFTER the merge
   move-out), each arm binds its payload as a BORROW (scalar copy @12 for
   Ok; the slot-0 HANDLE + param_values for a heap Err), arms construct
   fresh results via lower_heap_result_arm (which Dups borrowed payloads),
   and the IfThen/Else/EndIf merge + release-parity sweep carries the value
   out (the released-merge cert shape heap-result-if already proves).
   (ii) `VariantArmKind::BindAll` — a BINDER catch-all (`e => err(e)`)
   matches any tag and binds the WHOLE subject as a borrow (the regrouped
   fall-through arm; previously "walled for now").
   (iii) the heap-result-arm Match case now routes CUSTOM-variant subjects
   (`match $q { DivideByZero => …, e => … }` over the borrowed err payload)
   through try_lower_custom_variant_match — which already accepted heap
   results over borrowed subjects (the recursive-to_string precedent).
   Only 1 corpus fn opened directly (multi-blocker as usual) but the
   machinery unblocks the 23-bucket's core shape for future compounding.

2s. **List-subject tail-value match SHIPPED (walls 181 → 179)**:
   `try_lower_list_match_value` — the len-tag TWIN of the Result opener for
   `match list.filter(xs, f) { [] => None, ys => list.get(ys, 0) }`. Gate:
   heap result, exactly one `[]` arm + one catch-all (`_` | bind-all), no
   guards. tag = len@4 with INVERTED polarity vs Result (THEN = non-empty).
   The bind-all var ALIASES the owned subject temp (arm calls borrow it; an
   arm MOVE-OUT `ys => ys` is compensated by the release-parity sweep with a
   drop on the empty side — probe lm3 verified). Opened `first_even` and
   `quicksort` (Block arm body + closure captures + recursion + `??` all
   compose). Probes lm1–lm4 v0-identical, incl. param-var subjects (Dup'd)
   and multi-use binds. The heap-result-match bucket (18, ALL single-blocker
   — the largest single-reason family) is now: multi-arm list patterns
   (`describe` — element reads + guards), tuple subjects (`zip_first`),
   custom-variant value matches with fn-value arms (`tree_fold`), single-ctor
   heap-payload move-out (`unwrap_html`), branch_lift synthetics.

2t. **Scalar-subject guard-match desugar SHIPPED (walls 179 → 177)**:
   `desugar_scalar_guard_match` (mod_p6) — `match weight(p) { w if w <= 1 =>
   "envelope", w if w <= 10 => "box", _ => "freight" }` rewrites to a hoisted
   scalar temp + an `if` chain BEFORE lowering (added to BOTH chains:
   `desugar_all` + `lower_body_into`, desugar-before-both). Gate: scalar
   subject, every non-final arm a GUARDED Bind/Wildcard, final an unguarded
   catch-all. All arm bind vars alias the one temp at block top (scalar
   copies — guards need their var in scope before the chain); guards/bodies
   appear exactly once (count-invariant). Opened `shipping_label` and
   `Temp.classify`; heap-result guard matches now ride the proven
   heap-result-`if` machinery in every position (tail/bind/arg). Probes
   gm1–gm2 v0-identical (call subjects evaluated once, final-arm binds,
   let-bound position).

2u. **Tuple-subject 2-arm variant match desugar SHIPPED (walls 177 → 176)**:
   `desugar_tuple_variant_match` (desugar_match.rs) — `match (s, 0) { (Full(x),
   _) => …, (Empty, _) => … }` rewrites to per-component temps (Var components
   used direct — keeps a borrowed param BORROWED for the brick-4 heap-result
   gate) + NESTED single-subject matches; the catch-all body duplicates into
   each conditional component's wildcard arm (branch-exclusive; VarId
   uniqueness guarded by `introduces_binder` when >1 conditional component;
   the last arm must be `_`/non-binding — frontend exhaustiveness). Opened
   `payload` (probe tv1: payload + zip_first shapes v0-identical on v0; tv2
   isolates the remaining gap). REMAINING GAP (zip_first): a NESTED
   heap-payload Option match INSIDE an arm body (`match a { some(x) => match
   b { some(y) => …, _ => none }, _ => none }`) — the om2 single-match path
   (heap-payload Some at tail) does not fire inside `lower_heap_result_arm`
   arm-body context. r5 `classify` (3 arms) needs the arm-matrix
   generalization (Maranget-style specialization) — a later brick.

2v. **(Int,String)-tuple Some payload in arm context SHIPPED (walls 176 → 175)**:
   `lower_heap_result_arm` gains an `OptionSome` case for an `(Int, String)`
   TUPLE payload (`some((a, b))` — the zip_first merge arm after the
   tuple-variant desugar): `lower_owned_heap_field` constructs/Dups the tuple,
   `materialize_opt_int_str_some` wraps it (recursive `$__drop_list_int_str`
   drop, Consumes the piece), per-arm `im` balance. Opened
   `zip_first__Int_String` — the 2u + 2v pieces compose end-to-end (tv1/tv2
   v0-identical). Known non-corpus gap: a SCALAR-tuple Some payload
   (`some((Int, Int))`) in arm context (probe tv3 nested_scalar).

2w. **Transparent-newtype ERASURE SHIPPED (walls 175 → 173)**:
   `erase_transparent_newtypes` (lower/newtype_erase.rs) — a whole-program
   pre-lowering pass (post-ir_link, in BOTH pipeline + classify =
   desugar-before-both) erasing non-generic `Alias` type decls: `Ty::Named`
   tags substitute to the inner ty everywhere (exprs, params/ret, binds,
   patterns, lambda params, top-lets, OTHER type decls' fields; alias chains
   fixpoint-resolved), the 1-arg ctor CALL becomes its payload expr, the
   1-arg ctor PATTERN becomes its inner pattern, and a match reduced to one
   bare-Bind arm folds to `{ let s = subject; body }`. Sound because the
   frontend already rejects every wrapper-observing op (direct print,
   arithmetic) — by IR time the newtype is purely nominal, exactly v0's
   `#[repr(transparent)]` story. Opened `unwrap_html` + the opaque-in-list
   test fn (probe uh1 v0-identical). This is the miniature of the
   http.response opaque-nominal migration.

2x. **Funcref call arm SHIPPED (walls 173 → 171)**: `lower_heap_result_arm`
   gains a Computed-callee case for a KNOWN funcref (`closure_value_of` — a
   fn-typed param / lifted lambda): `emit_closure_call` + Consume +
   drop_arm_locals, the tail-position machinery (tail.rs) ported per-arm with
   the Named-call arm's `im` balance. Opened `tree_fold__String_String`
   (recursive self-calls as merge args + funcref arms — probe tf1
   v0-identical) AND `option_chain__Int_Int` (`some(v) => f(v)`). The
   heap-result-match bucket is now 9.

2y. **Bindless `[]`-column tuple specialization SHIPPED (walls 171 → 170)**:
   `desugar_tuple_empty_list_match` — an N-arm tuple-of-lists match whose tests
   are all bindless `[]` patterns (`([], []) / ([], _) / (_, []) / _` — the
   regression `classify` shape) specializes on the first conditional column
   recursively (mini decision tree, trivial because `[]` binds nothing): each
   level is a 2-arm `[] / _` match over one hoisted component — exactly the
   `try_lower_list_match_value` subset. First-match pruning after an all-`_`
   row; duplicated bodies (a row with an `_` column reaches both branches)
   must not introduce binders. PLUS: `lower_heap_result_arm`'s Match case now
   also tries `try_lower_list_match_value` (the nested inner match — same
   no-extra-Consume convention as the recursive Match case). Probe cl1 all-4
   branches v0-identical. Opened regression `classify`; heap-result-match
   bucket now 8 (r5 classify needs fieldless-CTOR columns — tag reads have no
   IR-level test node, a later brick; `describe` needs the len-group +
   element-load desugar; `pick`/nested_boxed need depth-2 ctor patterns).

2z. **json_path self-host SHIPPED (walls 170 → 166)**: `stdlib/json_path.almd`
   per the design below — rep List[String] ("f<name>"/"i<int>"), get_path over
   value.get / value.as_array / GENERIC list.get (typed variants like
   list.get_str are v1-lowering names, NOT frontend fns — the almd must use
   the generic and let type-directed routing pick the variant; `json.*` needs
   an import so value.* is the self-host-internal spelling). Registry maps
   json.root/field/index/get_path; "json_path" in PURE_MODULES; "get_path" in
   the json tracked-subject arm; the eraser's new SELF-HOST REP table erases
   `Named("JsonPath")` → List[String] (guarded on the program not declaring
   its own JsonPath). Probe jp1: 8 cases v0-identical (field / nested / index /
   NEGATIVE index wraps len+i / OOB / missing key / index-on-non-array / root
   identity). Opened all 4 json_path_test fns. p_set (set_path) remains.

**NEXT PIECES DIAGNOSED (at …→175→173→171→170→166, 2026-07-11):**
- **json_path family (REMAINING: p_set / set_path)**: the untracked-subject bucket's
  biggest coherent sub-family (json_path_test ×4 + json_path_edges p_set).
  `json.root/field/index/get_path` are Rust intrinsics over the opaque nominal
  `JsonPath` (undeclared in the checker — just `Ty::Named("JsonPath", [])`
  from the stdlib sigs). Self-host plan:
  (1) `stdlib/json_path.almd` — rep = `List[String]` of segments root-first,
  `"f<name>"` / `"i<int>"`; `root()=[]`, `field(p,n)=p+["f"+n]`,
  `index(p,i)=p+["i"+int.to_string(i)]`; `get_path` walks via the PROVEN
  json.get / json.as_array / list.get_value path. v0 oracle semantics
  (runtime/rs/src/json.rs almide_json_get_path): a field step on a non-object
  → none; an index step wraps NEGATIVE i as len+i, misses OOB. Segment decode:
  `string.take/drop(seg,1)` + `int.parse` (all self-hosted already).
  (2) registry entry mapping json.root/field/index/get_path; purity module
  "json_path" into PURE_MODULES (sorted); "get_path" into the
  is_self_host_option_module_fn "json" arm (tracked subject).
  (3) THE KEY PIECE: teach `erase_transparent_newtypes` a SELF-HOST REP table
  (`"JsonPath"` → `List[String]`) so every `Ty::Named("JsonPath")` bind/param
  erases to List[String] and the drop routing (heap_elem_list str) is correct —
  the self-host OWNS the rep, the eraser publishes it. JsonPath is opaque to
  user code (only these fns consume it), so the rep swap is unobservable.
  set_path (p_set) is the follow-up (needs Value rebuild — value.merge /
  list.set_value precedents exist in value_core).
- **fan.settle / fan.any / fan.timeout over literal thunk lists (7)**: extend
  the `desugar_fan_race` inline pattern (mod_p6 ~3677) — on wasm the fan
  combinators are DETERMINISTIC (sequential), so `settle([t0,t1,…])` inlines
  to building the results `List[Result[…]]` (NOW materializable — the 2b
  lenlist stage), `any` to a first-ok if-chain. `fan.map` over a captured
  closure value stays walled (opaque fn-value arg).
- **http.response builders (6)**: PURE data constructors on the blanket-
  impure `http` module — but `HttpResponse` is an OPAQUE NOMINAL type backed
  by Rust (`@intrinsic`), so the self-host needs the value_core pattern
  (redefine as a bundle almd record `{status: Int, body: String, headers:
  List[(String,String)]}` + rewrite the builder section of stdlib/http.almd
  to almd bodies, keeping serve/network intrinsic). A whole-module migration
  — the "opaque nominal stdlib type self-host" design piece.
- **zlib.compress/deflate (6)**: a real DEFLATE port — large; candidates for
  the permanent-wall netting if not undertaken.
- **compute-style Result-arm tail matches (~part of 23)**: `match
  safe_div(a,b) { ok(v) => ok(int.to_string(v)), err(DivideByZero) =>
  ok("infinity"), err(e) => err(e) }` at tail returning
  `Result[String, MathError]` — heap-Ok + VARIANT-err (cap-as-tag with
  variant payload) — needs the resvar sibling of the reserr work on BOTH
  construction (`err(e)` re-wrap of a bound variant payload) and match sides.
- **result.collect / partition / collect_map (5)**: self-host returning
  `Result[List[Int], List[String]]` — needs the cap-as-tag Result-of-two-
  lists drop design (recorded in 2b's revert note: the err-List arm was
  reverted pending an exact `__drop_res_errlenlist`-class drop).
- **random (7) / http network (rest)**: permanent walls (entropy/network not
  byte-verifiable) — net out of the double-digit target.
3. **JsonPath subsystem** (~144 rows): heap JsonPath repr + get/set_path
   traversal.
4. **Unicode range tables** (string.is_alpha/is_lower/is_upper ~70 rows):
   generated range-table .almd (ASCII would silently diverge).
5. **Families**: result.collect (10), sized_conversion (9), fan_value (9),
   default_fields (9), pattern_test (8), zlib (6), matrix.shape (26),
   float.to_fixed (24).
6. **Honest permanent walls**: random (7), http (6+) — entropy/network are
   not byte-verifiable; the double-digit target nets these out.
Each stage ships with the same discipline: v0 parity probe → mir tests →
spec → gate + corpus (PCC + kernel oracle) → push at green.

(Historical note — the buckets already OPENED in this arc:)
- **json path family** (root 46 / field 41 / index 29 / set_path 28): a
  JsonPath DSL SUBSYSTEM — needs a heap JsonPath representation + the
  get_path/set_path traversal over Value; design piece, not a linkage gap.
- **generic lowering buckets**: match-over-untracked-subject with call-bearing
  arms (33) and string-interpolation in call-arg position (30) — these hold
  the regex/bytes test functions hostage after their primary buckets opened.
- **string.is_alpha/is_lower/is_upper (~70 combined)**: v0 uses FULL UNICODE
  char properties (`is_alphabetic()`), so an ASCII self-host would silently
  diverge — the honest route is a GENERATED range-table .almd (binary search
  over the alphabetic/uppercase/lowercase range lists derived from the same
  Unicode data Rust's std uses).
- matrix.shape (26 — the Matrix subsystem), float.to_fixed (24 — decimal
  formatting, the float_to_string self-host family's sibling).

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

- [x] Every regex.* corpus call site either EXECUTES v0-byte-identically or
      walls on a RECORDED unsupported feature (regex family opened 2026-07-10).
- [x] Engine edge-case suite green (greediness, empty match, anchors, split
      empties — the scouted list), on BOTH targets.
- [x] json.root/field/index + bytes.append_u8 buckets opened or their real
      blocker recorded (json_path self-hosted at 2z, walls 170→166).
- [ ] Histogram deltas recorded; corpus PCC (binary + kernel oracle) ACCEPT
      throughout; pushed at all-green; Trust Spine green (ongoing per stage).

## ENDGAME: walled-real → 0 (set 2026-07-11, at 166)

The target is ZERO — no allowlist, no "permanent wall" netting. Every corpus
function lowers, witnesses, and kernel-ACCEPTs. The 166 decompose into FIVE
campaigns (wall LINES below; fns often multi-blocker so campaigns compound):

**A. Call-argument materialization (~35 lines — the biggest mechanical seam).**
string-interp-in-arg (7), List arg (7), ResultErr arg (5), concat-in-arg (3),
`??`-in-arg (3+1), Fan arg (4), EmptyMap arg (2), method/computed-in-arg (4),
Match-in-arg (1), OptionSome arg (1), heap-result-other-in-arg (2). ONE
systematic fix: generalize the ANF lift (`desugar_callarg_heap_if` precedent)
— EVERY non-trivial heap arg lifts to a let-temp, then the existing bind
machinery lowers it. Work the desugar once, verify per-shape.
DIAGNOSIS UPDATE (2026-07-12, probes ia1/ia2): the interp-in-arg 7 are NOT an
ANF-lift problem — `${g(n)}` (String call part) and `${list.min([ints])}`
(Option call part) ALREADY lower in-arg. The 7 decompose into THREE deeper
families the wall message groups together: (i) `list_total_order` — typed
`list.min/max` over String/tuple/nested-list/Option elements = the TOTAL-ORDER
comparator self-host (lexicographic tuples/lists, none<some — a real project,
the list_sort_float registry precedent × the whole lattice); (ii)
`sized_int_record_fields` + `drain_smalls` (4) — Int8/16/32 record-field READS
+ negative sized-int literal repr inside interps; (iii)
`compound_repr_interp` (3) — literal-aggregate parts `${[1,2,3]}`, map parts
`${["a":1]}`, and EMPTY-map parts (desugar_interp_literal_aggregate_hoist
declines these today).

**B. Let-bound / returned heap-result forms (~25 lines).** let-bound variant
match (5), tail variant match (4), heap-result match remainder (8: multi-arm
list patterns `describe`, fieldless-ctor tuple columns `r5 classify` — needs a
tag-test IR node or a ctor-eq desugar, depth-2 ctor patterns `pick`,
branch_lift synthetics), let-bound match/if with the scope-end-drop question
(2+1: the merged dst needs a drop-class registration — design the
"merged-result drop class" once), heap Record return (3), move-out other (3).

**C. Loop control flow (6 lines).** while/for-in with break/continue: real
loop lowering with exit branches (wasm `br` out of the loop block; the cert
stays a flat fold if the loop body remains per-iteration balanced and the
break edge carries no live heap — scout the TCO rewrite precedent).

**D. Effectful/impure module calls (~40 lines) — TWO distinct halves.**
- D1 PURE-ON-IMPURE (open WITHOUT capability machinery, the testing_assert
  precedent): http.url_decode(4)/response(3)/json(1)/redirect(1)/
  with_headers(1) — pure builders/codecs on the blanket-impure http module
  (the opaque-nominal HttpResponse migrates via the SELF-HOST REP table, the
  JsonPath precedent); datetime.parse_iso (3); zlib.compress/level/gzip/
  deflate (6 — the DEFLATE port, pure and deterministic); testing.
  assert_throws (2). ≈ 22 lines.
- D2 GENUINELY EFFECTFUL: random.shuffle/choice (7), process.* (7), fs.stat
  (3), env.* (3), http.serve (1), fan.timeout (2). The capability system
  already PROVES the bound (caps witness = one of the three kernel-proven
  properties); the missing brick is LOWERING an effectful call with its
  declared capability to the WASI-shim import — v1 then matches v0-WASM
  behavior exactly (including v0-wasm's own error paths where native-only).
  Design piece: "capability-declared effectful call brick".

**E. Singles & bugs (~15 lines).** module-level globals with computed inits
(5 — a run-once init-fn brick); HOF opaque fn-value args (5: list.map/
option.flat_map/fan.map/filter_map/or_else over captured closures — the
funcref closure-table machinery from 2x extends); ADT brick 5 recursive ctor
heap fields (3); RawPtr Repr (2 — FFI decls); `use of unbound var` (2 —
smells like a real lowering BUG: diagnose first, these may be latent
miscompiles being walled honestly); LitInt heap bind (2); effect-fn error
model non-`ok(x)` pattern (1); never-err lift residue.

Waypoints: **<150** (A opened), **<120** (B+C), **<100 二桁** (D1), **<60**
(D2 design lands), **0** (E swept + loop-until-dry re-classify). Every stage
keeps the invariant ladder: v0 byte parity probes → mir tests → spec suite →
gate.sh → corpus-wall.sh (PCC + kernel ACCEPT) → by-name diff (zero
newly-walled) → push at green.

E1. **unbound-var diagnosis + cross-module toplet fixes SHIPPED (166 → 163)**:
   the two "use of unbound var" walls were NOT miscompiles (honest walls) but
   lowering gaps in the cross-module top-let machinery (#486/#502 shapes,
   probe xm1): (i) `lower_bind`'s heap Var-ALIAS arm used strict `value_for`
   — now `value_or_global`, so `let x = toplib.SYSTEM` materializes the
   global's cached const-init copy and Dups it; (ii) a RECORD-literal heap
   global (`let CFG = Cfg { name: "c" }`) materializes through
   `try_lower_record_construct` (allocs + stores, zero CallFn — the count
   gate stays exact) + `materialized_aggregates` registration; (iii) the
   Var-alias Dup PROPAGATES `materialized_aggregates` (the alias denotes the
   same block), so `{ ...x, override }` spreads over rebound globals read
   real slots. Mid-diagnosis a WRONG-BYTES intermediate (a spread over an
   unregistered-materialized base fell to Opaque and printed empty instead of
   walling) confirmed the register-then-spread order matters — the shipped
   form gates on registration. Opened both #486/#502 tests + the record
   top-let member-access test.

A1. **Empty-map call args SHIPPED (163 → 160)**: `lower_call_args` intercepts
   `[:]` / empty `MapLiteral` args before the deferred-Opaque arm — the SAME
   layout-agnostic 0-length block an empty-map BIND builds
   (`try_lower_scalar_list_slots(&[])`, now pub(crate)) via
   `materialized_call_arg` (live-tracked, ty-routed drop). Opened
   `frequencies` (the `fold(xs, [:], …)` seed), the ascription test, AND the
   `${emap}` interp part (compound_repr — the part now reads a real len-0
   block). Note: a probe main aggregating fold results still links
   `list.fold_hacc` (heap-accumulator fold variant, unimplemented) — a
   defunc-family follow-up, recorded here.

A2. **Pure fan blocks SHIPPED (160 → 156)**: `desugar_fan_block` (desugar_fan.rs,
   both chains) — `fan { e1; e2 }` over PLAIN Named calls rewrites to the tuple
   `(e1, e2)` (v0's wasm emission IS the sequential fallback, contract C-004's
   determinism family). KEY FINDING: the checker types EVERY fan expr as a
   PHANTOM `Result[T, String]` even for a plain callee whose runtime value is
   the raw T — the desugar strips the phantom to the Ok type for direct Named
   calls (probe fb1: 3 chained fan blocks with captures + staged deps,
   v0-identical). A Module/Method/Computed thunk (really fallible) stays
   declined — its auto-unwrap + Err early-return is a later brick. Opened all
   4 fan_test fns.

B1. **Scalar-tuple Some ctor SHIPPED (156 → 154)**: `try_lower_option_ctor`
   gains `some((1, 2))` — an ALL-SCALAR tuple literal payload: the flat tuple
   block (`try_lower_scalar_tuple_construct`) moves into the 1-element Option
   via `materialize_opt_str_some`; the payload owns no inner heap so
   DropListStr's flat slot-0 free is exact. With the ctor materialized, the
   let-bound `match x { some((a, b)) => a + b, none => 0 }` composes through
   the EXISTING tuple-payload desugar + destructure (probe nt1 v0-identical).
   Opened both pattern_test nested-tuple fns. The let-bound variant-match
   bucket's remainder: if_let over Result (frontend if-let desugar shape) and
   the option-of-variant none case — separate sub-shapes.

B2. **Result wildcard arm in the value match SHIPPED (154 → 151)**: the
   let-bound variant value match rejected `match x { Ok(v) => A, _ => B }`
   (the frontend's if-let desugar) twice over: (i) a Wildcard arm was only
   admitted for OPTION subjects; (ii) a Result Err CTOR bind reuses the
   Some(string) machinery (`materialize_opt_str_some` inserts
   `materialized_options`), so the subject is BOTH option- and result-tracked
   and the Wildcard got eaten by the Option else-side arm → slot collision →
   rollback (found via a temporary DBG_VVM arm-fill trace, removed). Fixes:
   the Option Wildcard arm gates on `!is_result` (Result semantics win on
   double-tracking), and a new Result Wildcard arm takes whichever slot the
   ctor arm did NOT fill (ambiguous wildcard-first rejects). Probes
   il1/il2/il3 + nt1 all v0-identical. Opened both if_let_test fns +
   guard_let's unwrap_res.

NEXT (diagnosed at 151, probe ov1): **Option-of-variant ctors** — the
codegen_patterns none case needs BOTH: (i) `let x: Option[Msg] = none` (an
empty len-0 block + materialized_options + the variant payload class), and
(ii) `some(Number(7))` — Some wrapping a CUSTOM-VARIANT payload: today the
inner ctor defers to an unlinked `$Number` CallFn (the render wall catches it
honestly — classify counts some_case open but the program cannot render);
the fix is try_lower_variant_ctor for the payload + a 1-element Option whose
drop routes to the RECURSIVE `$__drop_Msg` (the materialize_opt_aggregate_some
/ DropWrapperRec pattern). Also diagnosed: `unannotated_unwraps` (the #485
implicit auto-unwrap of a lifted Result on plain assign in an unannotated
effect fn), `nested_unwrap` (`o!` over an OPTION in an effect fn — the
none→error propagation model), `is_balanced` (fold with an Option[List]
accumulator — defunc family). Tail variant bucket = these + the fold shape.

B3. **Option-of-variant ctors SHIPPED (151 → 150)**: three pieces (probe ov1,
   none/some subjects + inner variant dispatch v0-identical): (i)
   `some(Number(7))` — Some wrapping a CUSTOM-VARIANT ctor payload builds the
   variant block (`try_lower_variant_ctor`) and moves it into the 1-element
   Option, drop-routed by the payload's own discipline (recursive variant →
   `optrec:<T>` → `$__drop_<T>`; flat variant → the Some(string) shape whose
   flat slot-0 free is exact). Previously the inner ctor deferred to an
   unlinked `$Number` CallFn (honest render wall, but the fn counted open —
   now it lowers for real). (ii) `let x: Option[Msg] = none` — a HEAP-payload
   OptNone also registers `heap_elem_lists` so the downstream match ADMITS
   its Some-arm payload bind (len-0 DropListStr is drop-equivalent). (iii)
   `heap_or_scalar_bind` admits `optrec:`-tracked subjects (the resrec
   precedent). Opened the codegen_patterns none case. NOTE: a botched patch
   emptied binds_p4.rs mid-stage — restored from HEAD (own commit) and
   re-applied atomically; the build error was loud, nothing shipped broken.

NEXT (diagnosed at 150, probe uu1): **the unannotated-effect-fn lift** — the
#485 shape desugars to a TAIL `match eff() { err(e) => err(e), ok(v) => v }`
whose tail.ty is the RAW scalar (Int), while the arms build Result blocks:
the LIFTED signature exists only in the frontend's arm bodies, not in the
tail/ret types the lowering dispatches on — so the scalar-tail path takes it
and dies on the Err-ctor arm. The fix is fn-LEVEL: when `is_effect` and the
declared ret is non-Result, the MIR ret (repr + tail result_ty + the CALLER
convention `f()!`) must use the lifted `Result[ret, String]` — the effect-fn
error-model frontier proper, NOT an arm-local patch (a speculative ok-wrap
fallback in try_lower_result_match_value was tried and reverted: the dispatch
never reaches it because is_heap_ty(tail.ty) gates first).

B4. **Record defaults + scalar field ANF lift SHIPPED (150 → 146 — the <150
   WAYPOINT falls)**: (i) plain-record field DEFAULTS ride
   `ctor_field_defaults` keyed by the record TYPE name (build_variant_layouts
   gains a Record branch), and `try_lower_record_construct` fills omitted
   slots from them (CALL-FREE defaults only — the count-gate discipline), so
   `AllDefault()` paren-empty ctors materialize; (ii) `f().x` — a SCALAR
   field on a call result in ARG position ANF-lifts to a synthetic temp
   (`fresh_synth_var` + lower_bind, the tail.rs heap-extraction discipline)
   and loads the real slot. Probe pc1 v0-identical. Opened the 3
   record_paren_ctor fns + codec_p0's unknown-ignored (compound). Also
   REVERTED-BY-DESIGN: a speculative ok-wrap fallback for the
   unannotated-effect-fn lift (see the NEXT note above — fn-level, not
   arm-local).

A3. **List[List[String]] literals + string-key sort_by honest wall SHIPPED
   (146 → 144)**: (i) a new `ListStr` element class in the record-list-literal
   builder — each inner `List[String]` literal builds through the str-list
   builder and the outer list drop routes `list_list_str_lists` (the recursive
   list-of-list-str free); a type-rewritten (never-err-lifted) element
   declines, the ctor-class guard. (ii) CORRECTNESS: opening the literal
   EXPOSED a latent mis-route — a STRING-key `list.sort_by` has no registered
   typed variant and fell to the generic scalar impl (probe ll1: wasm
   "indirect call type mismatch" TRAP in __sb_init). It now routes to the
   unlinkable `list.sort_by_str_key_x` (the `_x` honest-wall convention) —
   fail-closed at render, never a trap. NOTE the metric nuance: sort_by-test
   and list_total_order count OPEN at classify (lower succeeds) while the
   RENDER wall holds the `_x` boundary — FORBIDDEN=0 still proves no dangling
   call escapes. Follow-up recorded: registered sort_by_str_key / min/max
   typed variants over heap-elem lists = the total-order comparator family.

A4. **Err(List[String]) ctor SHIPPED (144 → 139)**: the both-heap ResultErr
   arm gains a `List[String]` LITERAL payload case — the inner list builds
   fresh-owned (`try_lower_str_list_literal`), `materialize_result_str` wraps
   it, and the drop RECLASSIFIES from the flat `heap_elem_lists` (which would
   free slot-0 as a String, leaking the inner list's elements) to
   `list_list_str_lists` (the recursive list-of-list-str free). Opened all 5
   result_collect_test fns at classify (probe rc1: v0 oracle `eq|eq|eq`); the
   `result.collect` CALL itself stays a dotted-call render wall.
   **DESIGN CONFIRMED for the render-side completion**: the self-host
   (filter_str-style prim passes: outer slots @12+8i, elem tag @4, Err String
   deep-copied via string.repeat·1, Ok ints via load64/store64 into
   alloc_list, len patched @4) is straightforward EXCEPT the caller-side
   call-result drop for `Result[List[Int], List[String]]` needs a TAG-AWARE
   generated drop (`$__drop_res_intlist_strlist`: tag=err → slot-0 recursive
   list-of-str free; tag=ok → slot-0 flat block free — freeing the Ok side
   recursively would rc_dec garbage int "handles" = UNSOUND). This is exactly
   the "exact drop" the 2b revert was pending — a LENLIST_DROP_SRC-style
   generated source + program_uses gate + drop_op_for arm.

**CRITICAL FINDING — FIXED (2026-07-12, the linearization wall)**: root cause
= `lower_branch`'s If arm linearized CALL-BEARING arms when every real-branch
path (try_lower_unit_if etc.) had declined the condition — the render then
RUNS BOTH arms (rc4's double print). Fix: the If arm now WALLS on a
call-bearing arm exactly like the untracked-subject match rule (call-free
arms stay linearizable — double-evaluation without effects is unobservable).
The mir unit test that pinned the OLD contract
(unit_if_with_effect_arms_linearizes_balanced) now pins the WALL. Corpus
impact: +1 honest wall (deep_eq_heap main — a previously silently
double-executing shape), 139→140. ORIGINAL FINDING (for the record):
`let e: Result[Int, String] = err("a"); println(if e == err("a") then "eq"
else "ne")` — v0 prints ONE line (eq), v1 prints TWO (eq|ne): the println
executes twice, the second with a wrong value. INDEPENDENT of the in-flight
result.collect work (none of its gates match this program) — a SHIPPED latent
wrong-behavior in the v1 line that no corpus/spec shape happens to exercise.
Suspects: the let-bound err-ctor bind + `==`-with-err-ctor-arg + println(if)
continuation — a tail-duplication (desugar_heap_branches /
desugar_let_bound_heap_branch family) whose arms BOTH execute. Plan: bisect
rc4 against past commits to find the introducing stage, root-cause, fix, add
a spec/wasm_cross fixture pinning the shape (+ siblings: ok-ctor, option
some/none ctor eq forms).

SHIPPED (after the linearization fix unblocked it): the result.collect
render-side stage — probe rc3 (`result.collect` + `result.is_err`) is
v0-identical end-to-end; rc1/rc2's remaining gap is the SEPARATE heap-Result
`==`-condition piece (now an HONEST wall via the linearization fix, next
stage: try_lower_unit_if × lower_heap_eq_typed_materialized). Original
in-flight note (the rc1 MISMATCH was the linearization bug, not this stage):
the result.collect render-side
stage is ON DISK but NOT shipped — probe rc1 MISMATCHES: v0 prints 3 lines
(eq|eq|eq), v1 prints those PLUS 11 extra ne/eq lines — a print-multiplying
shape (a both-arms linearization or a tail-duplication running the untaken
side's effects) somewhere in `println(if result.collect(..) == err([..]) …)`.
Pieces on disk: RES_ILSL_DROP_SRC + program_uses gate (drop_sources.rs),
pipeline injection, binds_p2 call-result registration (res_ilsl +
materialized_results_str — SUSPECT: the results_str tracking may route the
`==` or a linearization wrongly for this type), ("result","collect") in
is_self_host_result_module_fn, "result_collect" in PURE_MODULES, the registry
entry, stdlib/result_collect.almd (filter_str-style two-pass prim impl).
Debug order: (1) minimize rc1 to one println; (2) check whether the extra
prints come from a statement-match linearization admitted by the new
tracking; (3) verify the eq path for Result[List,List] (slot-0 is a LIST
handle — a string-compare of it is garbage). Ship only at full parity.

B5. **Heap-eq unit-if conditions SHIPPED (140 held)**: `try_lower_unit_if`'s
   cond fallback now routes a heap `==`/`!=` through `lower_heap_eq_cond`
   (rollback-safe typed materialized eq — String/Value/List[scalar|Value]/
   Option/Result[scalar,String]), so `println(if e == err("a") …)` (rc4)
   EXECUTES one arm with the correct value instead of walling. Zero corpus
   delta (deep_eq_heap's operands are outside the eq type coverage —
   extending lower_heap_eq_typed_materialized to deep records/lists and
   Result[List,List] is the follow-up that reopens it + rc1/rc2 shapes).

D1a. **http.url_decode self-host SHIPPED (140 → 136)**: the FIRST
   pure-on-impure D1 piece — `stdlib/http_url_decode.almd` (percent-decode
   over the prim floor: '+' → space, strict `i+2 < n` boundary + hex-validity
   passthrough matching v0's percent_decode byte-for-byte; the decoded bytes
   go through the SELF-HOSTED `string.from_bytes` for v0's from_utf8_lossy
   semantics — public-sig delegation, no internals). Wiring: registry,
   PURE_MODULES "http_url_decode", and a NEW "http" arm in
   is_pure_fn_in_impure_module (url_decode only — the network fns stay
   walled). Probe ud1: 8 edges v0-identical (multibyte, bad hex, lone '%',
   lossy U+FFFD, empty). Opened all 4 http_url_decode_test fns.

IN-FLIGHT (UNCOMMITTED): datetime.parse_iso self-host —
`stdlib/datetime_parse_iso.almd` + purity ("parse_iso" whitelist,
"datetime_parse_iso" module) + registry, ON DISK. Probe pi1: cases 1-3
v0-identical, case 4 ("2024-XX-15…" — the filter_map path where int.parse
ERRS and the part drops) TRAPS in rc_dec inside datetime.parse_iso (an
ownership bug in the self-host's own v1 lowering — suspect: the `match
int.parse(p) { ok/err }` inside __dpi_nums with the `acc + [v]` continuation,
or the err("…") ctor after the len check double-freeing a borrowed piece).
BISECT COMPLETE (dn1–dn6, all plain USER code — this is a LATENT SHIPPED
rc_dec TRAP, fail-stop not wrong-bytes): dn4 reproduces WITHOUT the self-host
— `fn nums(parts, i, acc: List[Int]) -> List[Int]` (tail-recursive
accumulator: ok-arm `nums(.., acc + [v])`, err-arm PASS-THROUGH `nums(..,
acc)`) called TWICE in a fn whose 3-arm heap-result if chain follows; the
FIRST err arm traps in rc_dec. Controls that PASS: dn1 (same nums, caller
uses the result immediately), dn3 (ONE list + 2-arm chain), dn5 (split-built
List[String] locals, no nums), dn6 (non-recursive user fn returning
List[Int]). ⇒ the trap needs the TCO'd mixed-arm accumulator (reassign in one
arm, pass-through in the other) × a caller with a multi-arm heap-result chain
after — suspect the TCO loop's acc-reassign drop discipline leaves a freed
block the caller's chain compensation re-drops. NEXT: dump dn4's MIR
(DBG_LOWER_FN=nums + f), inspect try_tco_rewrite's reassign path (mod_p5) for
the skip-arm drop imbalance. Ship parse_iso only after this is fixed (its
almd hits the same shape).

E2+D1b. **Scalar-Ok arm frame FIX + parse_iso SHIPPED (136 → 133)**: the dn4
   bisect landed — the `ResultOk(scalar)` arm case in lower_heap_result_arm
   was the ONLY sibling without a per-arm frame: a `?? 0` operand inside
   `ok(list.get(date, 0) ?? 0 + …)` materialized its Option temp into
   live_heap_handles, LEAKED it to the function scope end, and the teardown's
   unconditional rc_dec read an UNINITIALIZED local when the err path ran
   (rc_dec(0) trap — the yaml parse_number class; fail-stop, never wrong
   bytes). Fixed with arm_mark + drop_arm_locals (10fcddd9). That unblocked
   `stdlib/datetime_parse_iso.almd` (dd7e7218) — trim + strip-all-Z + split-T
   + filter_map-parse halves + exact err strings, delegating to the
   self-hosted datetime.from_parts / int.parse; probe pi1 6 edges
   v0-identical. Opened all 3 datetime_test parse_iso fns.

D1c. **http.response family self-host SHIPPED (133 → 127)**: the opaque
   nominal HttpResponse migrates via the SELF-HOST REP table (the JsonPath
   precedent) — rep = `List[String]` `[int.to_string(status), body, k1, v1,
   …]`, insertion-ordered like v0's Vec pushes, so NO new drop class. Eight
   fns in `stdlib/http_response.almd`: response/json/redirect (default
   headers exactly v0's), with_headers (map.keys + map.get iteration —
   insertion order), status (head-replace + list.drop), body, get_header
   (pair scan → Option), set_header (upsert via generic list.set — typed
   routing picks set_str; `list.set_str` is a LOWERING name the frontend
   rejects). Purity: the http arm widens to the 8 pure fns (serve/network
   stay walled); http.get_header joins the tracked-subject predicate. Probe
   hr1: 7 cases v0-identical. Opened all 6 http_response_test fns.

B6. **Record-destructure match desugar SHIPPED (127 → 126)**: `match f {
   Flags { ok: o, err: e, .. } => B, _ => C }` over a PLAIN-RECORD subject
   rewrites to the unconditional destructure `{ let o = f.ok; …; B }` —
   gated on the pattern NAME equalling the subject's Named type (a variant
   CASE pattern carries the case name), all later arms bare Wildcards, and
   plain Bind/Wildcard fields; the dead `_` arm drops on both sides. Probe
   sk1 v0-identical. Opened the soft-keyword-field test.

C1. **Conditional while breaks SHIPPED (126 → 122)**: `try_lower_scalar_while`
   now executes break shapes with the EXISTING marker vocabulary (no new op):
   `if c then <rest> else break` (the guard-else-break desugar) →
   `LoopBreakUnless(c)` + <rest> emitted linearly (the br already exited on
   the broken path, exactly like the loop-head cond); `if c then break else
   ()` → `LoopBreakUnless(1 - c)`; a BARE `break` statement (a const-folded
   `if true then break`) → `LoopBreakUnless(0)`. SAFETY: any UNRECOGNIZED
   break/continue in a body statement now ERRS the attempt (lower_stmt
   silently swallows a bare Break — the pre-gate that guarded that is
   replaced by the per-stmt check), so the fallback walls honestly. Probes
   wb1/wb2 v0-identical (guard-else-break ×2 shapes, const-folded break,
   mid-body conditional break). Opened all 4 while-family walls. for-in
   guard-break/continue remains (2 walls — the for machinery's sibling
   extension).

C2. **For-loop conditional breaks SHIPPED (122 → 121)**: both scalar for
   machines (`try_lower_scalar_for_range` / `_for_list`) route body
   statements through the while-break handler (`lower_while_body_stmt`) and
   drop their `body_breaks_or_continues` pre-gates — guard-else-break,
   if-then-break, folded bare breaks and guard-continue FILTERS (the guard
   desugar's continue elimination) all execute (probe fb2 v0-identical:
   filter-sum, break-sum, range break). Opened `for guard else break`; the
   codegen_loop_guard "for with guard continue filtering" test has a further
   blocker — DIAGNOSED: `var odds: List[Int] = []` + `odds = odds + [i]` (a
   HEAP-ACCUMULATOR reassign) × the guard-continue filter-if whose THEN arm
   nests the break-guard: the stmt is `if c then <block-with-breaks> else ()`
   (else is unit, not break), so the conditional-break handler defers and the
   per-stmt break check honestly aborts. Opening it needs (i) a FILTER-IF
   form in the loop-body handler (`IfThen(c); recurse; EndIf` — WAT labels
   resolve $brk through nesting) AND (ii) the heap-accumulator for machinery
   to accept it — the br must not skip the per-iteration acc-reassign frees
   (design the early-exit × Option-C slot interplay before coding).

A5. **from_codepoint global const-fold SHIPPED (121 → 119 — the <120 WAYPOINT
   falls)**: `const_global_init` folds `string.from_codepoint(<int literal>)`
   (`let NL = string.from_codepoint(10)` — the stringify-escape test globals)
   to its one-char `Init::Str` at lowering time — zero calls injected (the
   count gate stays exact); an invalid codepoint keeps walling. Probe nl1
   v0-identical. Opened both json_stringify_escape tests (their concat-in-arg
   wall chained from the computed NL/TAB globals).

B7. **Free-fn UFCS resolution SHIPPED (119 → 117)**: `desugar_method_calls`
   resolves a surviving Method on a NON-Named, non-record receiver
   (`3.double()`, `"hello".exclaim()`, `xs.sum_all()`) to the free fn with
   the receiver prepended — the checker already resolved stdlib UFCS to
   Module calls and type-checked the rest, so the free fn exists (a
   genuinely-missing one is caught by the render's unlinked wall). A RECORD
   receiver stays deferred (a fn-FIELD call needs the Computed-callee brick —
   record_fn_field's 2 walls). The mir test that pinned the old
   Method-walls contract now pins the resolution. Probe uf1 v0-identical
   (basic + chain). Opened both function_test UFCS fns.

B8. **Record fn-field call desugar SHIPPED (117 held, an enabler)**: a Method
   on a STRUCTURAL-record receiver whose name is a FN FIELD rewrites to the
   Computed call through the Member read (`h.run("x")` → `(h.run)("x")`,
   field ty from the record's own fields — count-invariant). Probe ff1 shows
   the REMAINING blockers for record_fn_field's 2 walls: (i) `make_handler`
   returns a record with a CAPTURING-CLOSURE field (`run: (x) => n + ":" + x`)
   — the record ctor with a closure field is unbuilt (heap-result Record
   return), and (ii) the Member-callee closure call needs the field's closure
   block loaded from the record slot (the funcref machinery × record slots).
   Both are the "closures in record slots" piece — design next.

V0 BUG FOUND (2026-07-13, probe si3 — needs a GitHub issue, EMU gh cannot
create one): `int.to_string(o.b)` where `b: Int16` is a record field →
"codegen produced invalid Rust" (E0308: expected i64, found i16). The C-038
construction-site narrowing stores the declared width but the int.to_string
CALL SITE never widens back (`o.b as i64`). `almide check` passes — the
check-passes/build-fails class (#739 sibling). Repro:
`type Outer = { a: UInt8, b: Int16 }; let o = Outer { a: 200, b: 30000 };
println(int.to_string(o.b))`.

DIAGNOSIS (v1, the sized-int interp walls — si1/si2): even the single-field
`"a=${o.a}"` (UInt8 Member part) walls at the interp-in-arg position — the
part's narrow-int to_string routing/concat operand is the decline point
(NOT the nested member). Next: trace desugar_string_interp's synthetic
to_string name for narrow int parts and the concat operand's slot load.

B9. **Sized-int interp display SHIPPED (117 → 114)**: `interp_to_string_call`
   (the interp desugar's leaf dispatch — the ONE `display_leaf_call` defers
   to) treats Int8/16/32/64 + UInt8/16/32 like Int (the v1 scalar is a
   uniform i64, widened at the literal/load, so int.to_string prints the
   exact value incl. negatives; UInt64 stays excluded — above i64::MAX would
   misprint). Probes si1/si2/si5 v0-identical (flat + nested records, 4-part
   interp). Opened both drain_smalls fns + sized_int_record_fields main.
   SECOND v0 BUG in this family (probe si6, needs an issue — EMU gh cannot
   file): a COMPUTED sized-int field value (`N { a: 0 - 5 }`, a: Int8) emits
   `-5i64` unwidened into the i8 field — invalid Rust E0308 ×3; the C-038
   construction-site narrowing only covers bare literals. v1 handles the
   same program CORRECTLY (`neg=-5 -300 -100000`).

NEXT (diagnosed at 114): **hash_protocol Map keys (4)** — the map literal
desugars to `map.from_list([(k, v), …])`; a BOOL key makes the list
`List[(Bool, String)]` (no tuple element class) and from_list has no
bool-key variant. Bool→Int retyping is WRONG (map.keys display: true/false
vs 0/1) — the piece is a bool-key map variant family (map_bkv: the int-key
machinery + bool display) plus record/variant keys via the hash protocol
(bigger). **depth-2 ctor patterns (pick, 2-3 walls)** — the chain emitter
needs nested-tag conds with FALLTHROUGH (a wrong-outer-ctor payload slot can
hold a raw Int whose truncated address could OOB-trap on a naive AND — the
inner tag load must sit UNDER the outer match; single-outer-ctor subjects
(pick's Wrap) skip the outer test entirely — that sub-shape first).

B10. **Depth-2 single-outer ctor patterns SHIPPED (114 → 113)**:
   `try_lower_custom_variant_match` strips ONE pattern layer when the outer
   type has a SINGLE ctor and every arm is `TheCtor(<inner ctor pattern>)`
   (`match o { Wrap(A(n)) => …, Wrap(B(m)) => … }` — pick): the outer always
   matches, so the dispatch handle becomes the payload's slot-1 handle (a
   BORROW in param_values — the subject's recursive drop owns it; loaded only
   under the guaranteed-matching outer, so no wrong-ctor garbage read
   exists) and the inner layout drives parse_variant_arms unchanged. Probe
   pk1 v0-identical. Opened `pick`. Multi-outer-ctor depth-2 (r5 main,
   nested_boxed classify) still needs the fallthrough design.

B11. **Fixed-length list-pattern match desugar SHIPPED (113 → 112)**:
   `desugar_list_pattern_match` (desugar-before-both) rewrites a match over a
   `List[scalar]` whose arms are fixed-length list patterns (elements ∈
   Bind/Wildcard/Literal, final arm an unguarded Wildcard) into a hoisted
   `let $t = subject; let $len = list.len($t)` plus a len==k if-chain: arms
   group by length in first-occurrence order, each group loads element temps
   `$e_i = $t[i]` ONCE under its len test, literal elements become `==` conds
   ANDed with the arm guard, binds alias the element temps, and a group's
   first unconditional arm terminates it (else the catch-all body duplicates
   in — `introduces_binder`-gated for VarId uniqueness under duplication).
   Probe de1 (the exact `describe` shape: `[] / [0] / [n] if n>0 / [_] /
   [a,b] / _`) v0-byte PARITY on all 6 branches. Opened
   `regression_v0_11_test :: describe`; zero newly-walled vs walls-pk.txt.
   Ladder: mir 583 / spec 283 / GATE OK / FORBIDDEN 0 / CORPUS WALL OK.

B12. **Depth-2 multi-outer ctor fallthrough SHIPPED (112 → 107)**: two composed
   pieces. (i) `group_option_result_arms` User columns go DEEP (`deep_col`:
   arbitrary ctor nesting, shallow record sub-patterns) + payload field tys
   fall back to the variant-layout registry (`lookup_ctor`) when no
   Bind/Literal names the column (`Box(Some(n))/Box(None)`); layouts threaded
   explicitly (`desugar_heap_branches`/`desugar_all` now take
   `&VariantLayouts` — no thread_local). (ii) NEW
   `desugar_tuple_variant_match_deep` — Maranget-lite column specialization
   for the N-arm tuple-of-variants matches the 2-arm desugar declines:
   specialize the leftmost conditional column per ctor head (fresh payload
   binds = new columns), Bind/Wildcard rows join every head branch (Bind
   substituted by the component ref — no duplicate binder), `_` default
   OMITTED when heads cover the type (a dead default would embed a
   non-exhaustive inner match), first-match pruning, `introduces_binder` gate
   on >1-branch bodies, 50k node cap. Probes mo1 (sum + classify, 7 branches
   incl. depth-2 + boxed bind), r5c (3-arm tuple-of-colors), pk2 (B10
   no-regression), ar1 (record-variant inner) all v0-byte PARITY. Opened
   nested_boxed sum/classify, nested_ctor area/opt, r5 classify. op1 NOTE:
   `opt` opened fn-level but `Box(Some(8))` CONSTRUCTION still walls
   ("heap/recursive field — ADT brick 5"): a custom-variant ctor with a
   BUILTIN-heap (Option) payload cannot materialize — recursive-variant
   payloads work (mo1's `Node(Leaf(5), Leaf(7))` in arg position lowered).
   That ctor gap is the next Campaign B piece and also blocks
   nested_ctor/r5 mains (ctor-in-arg). Ladder: mir 583 / classify 107 zero
   newly-walled / spec 283 / GATE OK / FORBIDDEN 0 / CORPUS WALL OK.

B13. **Option[scalar] ctor fields + in-arg tuple-variant matches SHIPPED
   (107 → 105)**: three composed pieces. (i) A custom-variant ctor field of
   type `Option[scalar]` (`Box(Some(8))`) now CONSTRUCTS: the 0-or-1 len-tag
   block owns no children, so its free is one flat rc_dec — mirrored in ALL
   THREE drop authorities in the same change (the drop generator's field
   loop, `variant_needs_recursive_drop`'s supported_heap, and the
   VariantLayouts twin) plus the ctor field admission
   (`try_lower_option_ctor` / `lower_owned_heap_field`). Option[heap]/Result
   payloads stay walled (owned children a flat free would leak). (ii)
   `VariantArmKind::Ctor` binds now carry the FIELD TY, and an Option/Result
   payload bind seeds its read-shape (`seed_variant_param`) — the inner
   `match $f { Some(n)/None }` executes instead of walling on a STRICT-mode
   scalar destructure (classify counted `opt` open but the strict render
   walled it — the permissive/strict split, now closed). (iii) The 2-arm +
   deep tuple-variant desugars also run INSIDE the heap-branches fixpoint,
   AFTER the call-arg lift and BEFORE the let-bound tail-duplication —
   `println(match (Red, Green) {…})` lifts to a let first, then compiles as
   a binder-free VALUE match (duplication-gate-safe); order matters both
   ways (before the lift: a Block-in-call-arg wall; after the duplication:
   binder-carrying duplicated bodies decline). Probes op1/so1 opened +
   PARITY, all six prior probes still PARITY, and BOTH fixtures
   nested_ctor_pattern.almd + r5_wasm_tuple_variant_pattern.almd are
   end-to-end v0-byte PARITY (mains opened). Ladder: mir 583 / classify 105
   zero newly-walled / spec 283 / GATE OK / FORBIDDEN 0 / CORPUS WALL OK.

E3. **@extern(c) native-root reclassification (105 → 102)**: classify's
   `compute_native_ffi_set` root-(a) matched only `@extern(rust/rs)`;
   `@extern(c, "m", "sqrt")` (extern_c_test — header: "wasm:skip —
   @extern(c) not available in WASM") is the SAME structural class (a
   C-library link no wasm module can satisfy), so its 3 fns
   (c_sqrt/c_floor/c_ceil) now count walled_native_ffi, not walled_real.
   Metric-only (no lowering change); corpus FORBIDDEN 0 / CORPUS WALL OK
   re-verified. DIAGNOSIS while here: bool-key maps are NOT a thin
   "route-Bool-as-Int" piece — even Map[Int,String] `m[1]` get walls outside
   the assert_eq shape (bk2/bk4 probes: `??`-in-call-arg + match-over-get
   wall; the hash_protocol int test passes only via its typed-assert path).
   The map-key family needs the get/from_list machinery widened per
   key/value class first; record/variant keys additionally need the hash
   protocol proper (tag/field-wise hash + eq, not handle identity).

B14. **Option-`?` identity desugar SHIPPED (102 → 99 — waypoint <100
   CROSSED)**: `?` is the to-Option CONVERSION (not `!`-propagation), so
   `Option[T]?` converts to itself — `desugar_to_option_calls` now replaces
   the ToOption node by its operand when `expr.ty == e.ty` is Option, in any
   position (count-invariant: ToOption is not a counted call). Probe qo1
   (`o?`/`none?` in match-subject position + tail `list.first(xs)?`) 4
   branches v0-byte PARITY. Opened result_option_matrix's two `? Option →
   identity` tests + unwrap_operators try_first. Ladder: mir 583 / classify
   99 zero newly-walled / spec 283 / GATE OK / FORBIDDEN 0 / CORPUS WALL OK.
   Next waypoint <60. Remaining 99 概観: record_fn_field 5 (closures in
   record slots), hash_protocol 4 (map key classes + hash protocol), random
   7 + zlib 6 + process/fs/env ~12 (D2 capability brick), fan 4,
   cross-module 7 (call-init globals brick + #412/#484), compound/deep-eq +
   repr interp mains ~8, heap-acc loop C-tail (find_factor, guard-continue
   filtering, map_fold_heap_acc), branch_lift synths 3, singles.

E4. **process/zlib native-root reclassification (99 → 86)**: root-(b)'s
   enumerated no-wasm set gains `zlib.*` (v0's emit_wasm has NO zlib runtime
   at all; fixture header "wasm:skip — OS/native-only") and
   `process.spawn|kill|is_alive|exec_status|env` (v0's calls_process.rs
   implements exactly exit/stdin_lines/args; WASI preview1 has no
   child-process API; fixture headers declare these native-only). 13 walls
   (process_ext 4 + process_exec_status 3 + zlib 6) → walled_native_ffi.
   `random` is deliberately NOT reclassified: v0's emit_wasm DOES implement
   it over WASI random_get (calls_random.rs — its fixture's wasm:skip header
   is stale), so random_test's 7 stay REAL — the implementable D2 slice
   (prim entropy import + Fisher-Yates/choice self-hosts + Entropy cap
   witness), alongside fs.stat (v0 wasm calls_fs_p3.rs "stat" exists),
   env_extra, fs_preopen, process_args (v0 wasm "args" exists). Metric-only;
   corpus FORBIDDEN 0 / CORPUS WALL OK re-verified.

D2a. **random.choice / random.shuffle self-host SHIPPED (86 → 79)**: the
   D2 slice with a real WASI floor — random_choice.almd (empty → `none`,
   else delegate to the generic `list.get` at a `__rc_rand` index) and
   random_shuffle.almd (Fisher–Yates on a COW copy — `var ys = xs` +
   `list.set`, source untouched; in-bounds `?? 0`/`?? ""` are dead
   fallbacks), each with its own prim.random_get entropy helper so the
   transitive cap_witness carries Entropy exactly like random.int. Wiring:
   `is_admitted_effectful` (calls.rs), element-typed routing in
   `list_heap_call_name` (scalar → flat impl, String → `_str`, else the
   unlinked `random.<fn>_x` wall), registry entries ×4,
   `is_self_host_option_module_fn` "random"/choice, IMPURE_PLAIN drift-gate
   justifications ×2. Probe rn2 (all 7 walled shapes: choice
   empty/single/from_list, shuffle empty/single/preserves-elements/-length)
   v0-byte PARITY — the outputs are invariant-based so parity is exact
   despite entropy. random_test's whole walled set opened. Ladder: mir 583 /
   classify 79 zero newly-walled / spec 283 / purity drift gate OK / GATE OK
   / FORBIDDEN 0 / CORPUS WALL OK.

D2b. **fs.stat self-host SHIPPED (79 → 76) + LATENT fs.exists regression
   FOUND & FIXED**: while wiring the new prim the probes exposed that
   `prim.path_exists` was NEVER DECLARED in stdlib/prim.almd — every program
   calling fs.exists through the v1 render path walled with "type errors:
   undefined function 'prim.path_exists'" (an honest wall, v0 fallback — but
   fs.exists's original opening had silently regressed). Both prims are now
   declared (path_exists + the new path_filestat). The fs.stat pieces:
   PrimKind::PathFilestat (args = [bufaddr, path], dst = raw errno;
   Capability::FsRead in certificate.rs), the `$path_filestat_q` WAT bridge
   (host writes the 64-byte WASI filestat into the SELF-HOST's own scratch
   Bytes — field reads stay Almide: filetype@16, size@32, mtim@48 ns→s;
   accounted in the CLOSED WASI_FLOOR_FNS set, not the open ratchet),
   stdlib/fs_stat.almd (structural-record Ok payload in fs.almd's field
   order), admission + registry + result-str tracking, and THREE gate
   widenings: (i) `effect_unwrap_admitted` admits record-Ok Results
   (Ty::Record + non-variant Named — control_p2's HOLE-1 resrec/flat
   machinery already handles both; layouts threaded through
   desugar_effect_unwrap), (ii) VariantArmKind (B13) already seeds, (iii)
   NEW FileStat entry in the SELF-HOST REP table (newtype_erase): the
   BUNDLED fs module's type decls never reach record_layouts, so
   Named(FileStat) erases to the structural record and `meta.size` member
   reads resolve without a registry. Probes fst1–fst7 (direct `!`, helper-fn
   `!`, payload bind, member read, err path, statement match) ALL v0-byte
   PARITY; mir test updates: the walled-example test now uses env.temp_dir
   (fs.stat genuinely opened), `$path_filestat_q` documented in the WASI
   floor set. Ladder: mir 583 / classify 76 zero newly-walled / spec 283 /
   GATE OK / FORBIDDEN 0 / CORPUS WALL OK.

D2c. **env.os / env.temp_dir self-host SHIPPED (76 → 74)**: on the wasm
   target these are COMPILE-TIME CONSTANTS (v0's calls_env.rs folds os() →
   "wasi", temp_dir() → "/tmp"), so the v1 self-hosts are the same one-line
   constants — no host reach, no capability, admitted per-fn via purity.rs's
   new "env" arm (the effectful env fns stay walled/cap-admitted as before).
   env_os.almd + env_temp_dir.almd + registry + PURE_MODULES; drift gate OK.
   Probe ev1 v0-byte PARITY (len>0 invariants — the per-target constant
   values are never printed by the corpus). The durably-walled test example
   moved to http.serve (env.temp_dir opened out from under it). Ladder: mir
   583 / classify 74 zero newly-walled / spec 283 / GATE OK / CORPUS WALL OK.

D2d. **process.args self-host SHIPPED (74 → 73)**: `$args_get_list` is now
   PARAMETERIZED by `$skip` (1 = env.args argv[1..], 0 = process.args
   argv[0..] — std::env::args includes the program path, C-096): ONE WAT
   bridge serves both prims, no host-floor growth. New
   PrimKind::ArgsGetListFull (render `(call $args_get_list (i32.const 0))`,
   CliArgs in certificate.rs, Ptr dst repr in render_wasm's prim repr
   classification — MISSING it produced the i64/i32 invalid-wasm the probe
   caught), prim.args_get_list_full decl, process_args.almd (a PLAIN fn
   matching stdlib/process.almd's plain `fn args()`), admission + registry +
   IMPURE_PLAIN justification. The process_args.almd fixture itself is
   end-to-end v0-byte PARITY. Ladder: mir 583 / classify 73 zero
   newly-walled / spec 283 / purity gate OK / GATE OK / CORPUS WALL OK.

B15. **Unit-discard `!` normalization SHIPPED (73 → 72)**: `let _ =
   fs.write(p, s)!` — the frontend gives `_` a REAL VarId, so the
   unwrap-desugars built `ok($v: Unit)` and the statement result-match
   parser declined the Unit-typed bind (fs_preopen_resolve's blocker). Both
   arm builders (desugar_let_unwrap's Target::Single and
   desugar_effect_unwrap's ok_pat) now normalize a UNIT-typed Ok bind whose
   var the continuation NEVER references into the Wildcard arm (exactly the
   bare-stmt `!` shape, which already lowers) — gated by a VarUse read-scan
   so a genuinely-read unit var keeps its bind. Probes fp1 + the
   fs_preopen_resolve fixture (write/read/exists/alloc-churn ×2 rounds,
   run with --dir=/) end-to-end v0-byte PARITY. Ladder: mir 583 / classify
   72 zero newly-walled / spec 283 / GATE OK / CORPUS WALL OK.

C3. **Mid-body conditional break with pre-break statements SHIPPED
   (72 → 71)**: `if c then { A…; break } else B` (find_factor's
   `if n % i == 0 then { result = i; break } else { i = i + 1 }`) —
   `lower_while_body_stmt` now CAPTURES the (call-free pure-scalar) cond
   once, lowers the ordinary unit `if` with the trailing break STRIPPED
   (`strip_trailing_break_expr`: break as block tail or last stmt; both arms
   then break-free, so the statement-if machinery branches the arm assigns),
   and emits LoopBreakUnless(1 − captured) AFTER — the capture keeps the
   break test the value the branch dispatched on even when an arm mutates
   the cond's operands. No calls added (cond gated call-free), so mir == ir
   holds without count-gate changes. Probe ff4 (guard-else-err + while +
   assign-then-break, 4 branches incl. the composite factor walk) v0-byte
   PARITY; probes ff2/ff3 confirmed guard+while alone already lowered.
   Ladder: mir 583 / classify 71 zero newly-walled / spec 283 / GATE OK /
   CORPUS WALL OK. Remaining loop-guard C-tail: the heap-acc guard-continue
   filter (`odds = odds + [i]` under 2 guards) — the heap-acc loop family.

B16. **Scalar-scalar Result `err(<scalar>)` ctor SHIPPED (71 → 69)**:
   `Result[Int, Int]` (match_container_literal's `ck(err(404))`) had a
   scalar-Ok materializer but NO scalar-Err twin — and the len-1 Err tag
   makes the DropListStr convention rc_dec slot 0, which for a RAW SCALAR
   payload is the rc_dec-trap class, so the new
   `materialize_result_err_scalar` (result_ctors.rs) keeps the same
   len-as-tag block but is deliberately NOT heap_elem_lists-tracked: the
   flat Op::Drop frees the block exactly (neither arm owns children). Ctor
   arm gated to BOTH sides scalar so the heap-err layouts keep their
   existing arms. Probes mc1–mc4 bisected the fixture (the concat chain and
   some(String-literal) args already lowered; ok(0)/err(404) ctor args were
   the gap); match_container_literal.almd is now end-to-end v0-byte PARITY,
   and result.or_else's ok-passthrough test opened as a bonus (same ctor
   class). Ladder: mir 583 / classify 69 zero newly-walled / spec 283 /
   GATE OK / CORPUS WALL OK.

B17. **Option-`!` heap payload SHIPPED (69 → 68)**: the effect-unwrap
   desugar's scalar-only gate on Option payloads dated from before the heap
   Some-bind discipline existed — a heap payload now binds as a @12 BORROW
   over the tracked subject (heap_elem_lists + the B13 seeds), so the gate
   drops to arity-1 only (`list.get(chunks, 0)!` with a Matrix payload —
   matrix_misc's blocker). An untracked/unliftable shape still walls
   honestly at the match layer. Probe mm1 (matrix.from_lists →
   split_cols_even → two `!`-extractions → matrix.get arithmetic) v0-byte
   PARITY. Ladder: mir 583 / classify 68 zero newly-walled / spec 283 /
   GATE OK / CORPUS WALL OK.

E5. **http.serve native-root reclassification (68 → 67)**: root-(b) gains
   `http.serve` — a TCP listener with NO v0 wasm form (the fixture header:
   "wasm:skip — http.serve is native-only"; effect_intrinsic_tail's
   `_serve_demo` exists only to pin the never-started codegen path). Same
   no-wasm class as net.*. Metric-only; CORPUS WALL OK re-verified.
   Batch diagnosis of the remaining test-fn walls while here: list.map with
   a FIRST-CLASS fn value (3 — transform/closure-factory/apply_all__A: the
   caps gate walls an opaque Fn-typed var even though 5c possible-callee
   sets could bound it), heap-result Match in call-arg (codegen_patterns
   tuples), list.filter_map unliftable closure (extract_click_positions),
   never-err effect-fn structured match (protocol_edge — the error-model
   frontier), generic variant ctor with heap field (type_system Node).

B18. **First-class fn values into pure combinators SHIPPED (67 → 62)**: a
   Fn-typed VAR argument (`fn transform(xs, f, pred) = xs |> list.map(f) |>
   list.filter(pred)`) now passes its closure BLOCK by handle — the
   self-host combinator CallIndirects it exactly like a lifted lambda's
   block (the 5c possible-callee rows bound the witness). Capability-sound:
   a PURE combinator can only receive a PURE closure (the frontend's effect
   typing), so the callback contributes no host capability of its own; a
   lifted lambda's caps were folded at its creation site. An UNBOUND
   Fn-typed var still walls (unresolved-function-value; the pinning mir
   test updated to the new contract). Opened 5: transform + closure-factory
   + apply_all__A (the targeted 3) PLUS option.flat_map chains and fan.map
   over a captured closure value (bonuses — same admission). Probe tf1
   (map+filter through fn params) v0-byte PARITY and the
   protocol_ufcs_inferred_lambda fixture is END-TO-END PARITY. Ladder: mir
   583 / classify 62 zero newly-walled / spec 283 / GATE OK / CORPUS WALL
   OK (the caps corpus re-verified with the new handle args).

B19. **Option[List[scalar]] `??` SHIPPED (62 → 58 — waypoint <60 CROSSED)**:
   `map.get(groups, "0") ?? []` (the group_by class) — the `??` machinery
   had liststr/value/listvalue payload routes but no FLAT scalar-element
   list sibling. New `option_listint_unwrap_or` (value_core.almd — the
   liststr shape with a flat rc drop, scalar elements own nothing) +
   `is_option_listscalar_ty` + both routing sites (the `??` operator's
   helper table in control_p3 AND the pipe-form option.unwrap_or rename in
   list_heap_call_name). Opened 4: group_by test, `?? preserves type: list`,
   list_chunk_windows main (END-TO-END v0-byte PARITY — the nested
   `list.get(list.get(list.chunk(...)) ?? [], 0) ?? neg(1)` chain), and
   map_insertion_order main (fn-level; render still walls honestly on a
   later for-in-over-map blocker — FORBIDDEN 0 holds). group_by itself
   stays render-unlinked (no self-host yet) — the fn-level open is honest
   (fallback, no bytes shipped). Ladder: mir 583 / classify 58 zero
   newly-walled / spec 283 / GATE OK / FORBIDDEN 0 / CORPUS WALL OK.
   Next waypoint: 0.

B20. **Closures in record slots SHIPPED (58 → 54)**: the B8-diagnosed
   "closures-in-record-slots" piece, four coordinated edits. (i)
   CONSTRUCTION: `lower_owned_heap_field` gets a Lambda arm (lift_lambda —
   the full 本命 capture machinery — builds the self-describing closure
   block; the record Consumes it into its slot). (ii) DROP: the record drop
   generator's field loop gets a `Ty::Fn` arm freeing the slot via the
   generated `__drop_closure` (a flat rc_dec would leak the captured env);
   `record_field_needs_recursive_drop` already classified Fn fields
   recursive (is_heap), so `__drop_<Handler>` existed but leaked. (iii)
   DIRECT CALL `h.run("hello")`: desugar_method_calls resolves a
   NAMED-record receiver whose method is a declared FN FIELD to the
   Computed(Member) rewrite (previously it fabricated an undefined
   `Handler.run` Named call — record_layouts now threaded through
   desugar_method_calls/desugar_all); the new `closure_block_of_mut` loads
   the slot borrow (param_values + closure_values) and the call-arg + bind
   sites dispatch through it (`is_fn_member_callee` for match-arm guards).
   (iv) EXTRACTION `let f = h.run; f("world")`: a Fn-typed heap extraction
   joins closure_values. Probes rf1–rf6 all v0-byte PARITY except
   mock_source, which stays honestly walled on the KNOWN nested-heap
   capture ratchet (a List[String] capture — the closure-env ratchet's
   documented remainder), yet its two TESTS opened fn-level (the ctor and
   match lower; the lambda lift declines inside mock_source only). Opened
   4: make_handler, direct-field-call, field-access-then-call,
   Result-returning-fn tests. Ladder: mir 583 / classify 54 zero
   newly-walled / spec 283 / GATE OK / FORBIDDEN 0 / CORPUS WALL OK.

A6. **Pure call-init globals inline at use sites SHIPPED (54 → 51)**:
   `let BANNER = make_banner()` (the #632 / C-077 family) could not
   materialize at a use site under the count discipline (a Var reference =
   0 IR calls; injecting the CallFn breaches mir == ir), and an eager
   `__init_globals` prologue is a whole new subsystem. But v0 NATIVE
   globals are LAZY statics — every use evaluates the initializer's value —
   so for a transitively-PURE initializer, substituting the init EXPRESSION
   at each use site is byte-equivalent (v0-wasm's dependency-sorted eager
   init is pinned observably equal by C-077). New program-level pass
   `inline_pure_call_globals` (newtype_erase.rs), run right after the
   newtype erasure in BOTH the pipeline and classify IR construction —
   desugar-before-both by construction, so the counts stay 1:1. Gates:
   call-bearing init only; transitive purity (non-effect Named callees +
   pure Module calls, no RuntimeCall, Method/Computed declines);
   REGION-LOCAL substitution (main↔main, module↔module — the VarId
   numbering regions can collide, and the bridge owns cross-module reads);
   self-substitution skip + bounded fixpoint for chains. Opened 3:
   r5_mod_global_init_order main (END-TO-END v0-byte PARITY — the full
   BANNER/APP_NAME/ITEMS/FIRST dependency shape) and BOTH
   cross_module_init_order #632 tests (the module-side substitution feeds
   the existing name bridge). Ladder: mir 583 / classify 51 zero
   newly-walled / spec 283 / GATE OK / FORBIDDEN 0 / CORPUS WALL OK.

HOTFIX (51 held): **B19 shipped with a corpus caps-gate breach** — the
   listint `??` route emitted one synthetic option.listint_unwrap_or CallFn
   with NO matching count credit, so every such site was mir 2 > ir 1
   (WALL BREACH on 3 fns). The B19/B20/A6 ladders MISSED it because the
   corpus-wall check grepped `FORBIDDEN|CORPUS WALL OK | head -2` — on
   failure the FORBIDDEN line still prints, and head-2 hid the absence of
   the OK line (B19's own run was green; the breach surfaced when its
   opened fns' counts entered the comparison). Fix: credit
   `is_option_listscalar_ty` in BOTH the `??` count arms (classify),
   mirroring the liststr/listvalue credits. LADDER RULE UPDATE: always grep
   the FAIL lines too (`CORPUS WALL OK|WALL BREACH|WALL GATE FAIL`) — an
   OK-only grep is a false-green vector. Also probed and REVERTED a bare
   tail-Option-`!` desugar (it regressed unwrap_option_some — the
   lifted-mix `some(x) => raw x` arm is the error-model frontier, same
   class as protocol_edge; recorded, not shipped).

D2e. **fan.timeout literal-thunk inline SHIPPED (51 → 49)**: v0's WASM leg
   has NO timeout — calls_p4.rs calls the thunk INLINE ("just call fn"), so
   `fan.timeout(ms, () => body)` desugars to `body` (the fan.race
   head-settle precedent), the ms arg gated CALL-FREE (nothing effectful or
   counted is discarded), a non-literal thunk declining honestly. Wired
   into desugar_fan_race_any's shared visitor (heap-branches fixpoint —
   desugar-before-both). Probe ft1 (`fan.timeout(5000, () => succeed(42))`
   + `== ok(42)` + `?? -1`) v0-byte PARITY. Opened both fan.timeout tests.
   Ladder (with the CORRECTED corpus grep incl. FAIL lines): mir 583 /
   classify 49 zero newly-walled / spec 283 / GATE OK / CORPUS WALL OK.

B21. **Scalar-key (String-value) tuple lists SHIPPED (49 → 48)**: the
   `List[(Int, String)]` literal machinery's key gate widens from Ty::Int
   to ANY non-heap scalar (`[(true, "yes"), (false, "no")]` — the bool-key
   map literal's from_list argument): the (scalar @12, String @20) slot
   layout and the `$__drop_list_int_str` per-tuple String rc_dec are
   identical for every scalar key, so the two literal-materialization
   gates (binds.rs bind-position + calls_p2 call-arg) widen with no drop
   change. Probe bk6 (bool tuple list + destructuring find) v0-byte PARITY;
   the hash_protocol bool-keys test opened fn-level (map.from_list stays
   render-unlinked for non-String keys — the honest fallback, FORBIDDEN 0).
   Record/variant Map keys remain the hash-protocol-proper frontier.
   Ladder: mir 583 / classify 48 zero newly-walled / spec 283 / GATE OK /
   CORPUS WALL OK.

B22. **Map[Int,String] from_list + display (48 held, an enabler)**:
   map_ivh.almd gains `map_from_list_ivh` (set-fold over (Int,String) pairs
   — duplicate key keeps first position with last value, map_set_ivh's
   replace-in-place) and `map_to_string_ivh` (`[10: "x", 20: "y"]` / `[:]`
   — raw int keys, quoted values via the replace-chain escapes, backslash
   FIRST). Routing: the ivh admission keys `from_list` on the RESULT type
   (its first arg is the pairs List), the interp display table gains
   (Int, String) → map.to_string_ivh, and an already-suffixed synthesized
   display name passes through VERBATIM (the first probe exposed
   `to_string_ivh_ivh_wall` — the suffix machinery re-suffixed a suffixed
   name; the pass-through arm is the general guard). Probe cr4
   (`"imap=${imap}"`) v0-byte PARITY. compound_repr_interp main still
   walls on its LONG display tail (List[Map], Map[String,List[Int]],
   List[Option] parts, Map[Int,Float] …) — each needs its own display
   self-host; probes cr1–cr6 show list/smap/set/tuple parts already lower.
   Ladder: mir 583 / classify 48 zero delta / spec 283 / purity gate OK /
   GATE OK / CORPUS WALL OK.

B23. **Display tail continued (48 held, enablers)**: `${List[Option[Int]]}`
   (list_to_string_lo — the list_to_string_ll composed pattern, each element
   through its own `"${o}"` interp) and `Map[Int, Float]` from_list +
   `${fmap}` display (map_if.almd — build via the GENERIC scalar map.set,
   display composed from map.keys/values + list.zip + the float interp
   part). Routing: scalar-scalar from_list keyed on the RESULT ty (Float
   values only — Map[Int,Int] from_list stays unlinked), to_string_if
   passes through verbatim (the B22 suffix guard pattern). Probes cp2 + cp5
   v0-byte PARITY; cp1/cp3 confirmed Result-interp and List[String]-escape
   / List[Float] parts already lower. compound_repr_interp main's residue:
   `${Map[String, List[Int]]}` (the (String, List[Int]) heap-heap pairs
   list literal blocks FIRST — cp4) and the List[Map…] nesting — the hval
   from_list/display family, next. Ladder: mir 583 / classify 48 zero
   delta / spec 283 / purity OK / GATE OK / CORPUS WALL OK.

B24. **hval from_list/display self-hosts + an ownership lesson (48 held)**:
   map_hval.almd gains from_list (set-fold) + to_string_hval
   (`["xs": [1, 2, 3]]` — quoted keys, list values through their own
   interp), routing + registry wired; probe cp4 reached v0-byte PARITY
   THROUGH a call-arg literal widening (flat-2-tuple elements in the VIEW
   materializer) that the PCC OWNERSHIP CHECKER then REJECTED over the
   corpus (30420 objects) — the view builder stores RAW handles with NO rc
   events (elements owned elsewhere; plain block-only Drop), and a
   fresh-owned tuple element pushed live + a ty-driven recursive
   DropListStrStr re-track double-frees. The widening is REVERTED (gate
   green again; accept ⟹ safe held exactly as designed); the self-hosts
   stay (sound, registered, reachable once the pairs-literal materializes
   through an OWNERSHIP-CORRECT path — the documented follow-up: a
   dedicated owned-tuple-list builder like try_lower_record_list_literal,
   NOT the view). classify 48 / mir 583 / spec 283 / CORPUS WALL OK.

B25. **Owned-route (String, List[scalar]) pairs literal SHIPPED (48 held)**:
   the B24 follow-up done right — the two OWNED-builder gates widen to the
   flat-second-slot class ((String, String|List[scalar]) tuples): the
   `lower_owned_heap_field` (String,String)-tuple arm and
   `try_lower_record_list_literal_as`'s StrStr element class. Each tuple
   materializes fresh-owned via try_lower_tuple_construct and is CONSUMED
   into the owned list (per-element `i…m` + list `i…d` — the record-list
   balanced shape), drop = DropListStrStr (both-slot rc_dec, a full free
   for any flat slot). The PCC ownership gate ACCEPTS (30,368 objects) —
   the same corpus that REJECTED the raw-handle view widening: the checker
   discriminated the unsound and the sound encoding of the SAME feature
   exactly as designed. Probe cp4 v0-byte PARITY through the owned route.
   The List[Map[…]] nesting (cp6) is the remaining compound-repr depth.
   Ladder: mir 583 / classify 48 / spec 283 / GATE OK / CORPUS WALL OK.

B26. **List[Map] nesting SHIPPED (48 → 47)**: the compound-repr depth-2
   nest `${[["a": [1, 2]], ["b": [3]]]}` — three pieces in map_hval.almd +
   wiring: `__drop_list_map_hval` (per-element `__drop_map_hval`, the
   rc-guarded hval free), `list_to_string_lmh` (each map through its own
   `"${m}"` interp — the ll composed pattern), and the owned builder's new
   ListElemDrop::MapHval class (each element a from_list_hval call result
   MOVED in; drop via variant_drop_handles "list_map_hval"). Display table:
   List[Map[String, List[Int]]] → list.to_string_lmh. Probe cp6 v0-byte
   PARITY; the nested-list-of-maps TEST opened. compound_repr_interp main's
   last blocker is one MORE level (`List[Map[String, List[Option[Int]]]]` —
   the `deep` line). Ladder: mir 583 / classify 47 zero newly-walled /
   spec 283 / GATE OK / CORPUS WALL OK.

B27. **RawPtr / linear-memory bridge SHIPPED (47 → 45)**: the #440 / C-062
   family. (i) `repr_of` gains Ty::RawPtr → Scalar Double (a raw address in
   the uniform i64 slot — never a tracked handle). (ii) NEW identity prim
   casts `prim.ptr_to_int` / `int_to_ptr` (declared in prim.almd; the
   lowering emits NO op — the operand ValueId passes through, a pure
   type-level hat swap). (iii) stdlib/bytes_rawptr.almd self-hosts the four
   bridge fns: as_ptr/as_mut_ptr = handle+12, from_raw_ptr = fresh
   alloc_bytes + byte-copy loop, copy_to_ptr = min(len, cap) write-through
   + count. Debug notes: prim calls decline in ARG position (hoist to
   lets), and a UNIT-returning helper bound to `let _c` walls on
   repr_of(Unit) — the ivh helpers' Int-return convention is the pattern.
   The bytes_rawptr fixture is END-TO-END v0-byte PARITY (read side, write
   side, capacity clamp); both walls opened. Ladder: mir 583 / classify 45
   zero newly-walled / spec 283 / purity OK / GATE OK / CORPUS WALL OK.

NEXT PIECES DIAGNOSED (at 45):
   **eq mains (3)** — deep_eq_heap main needs exactly two NEW eq classes in
   `lower_heap_eq_typed_materialized`: (a) a (String, Int) TUPLE eq —
   composable in RUST MIR (load slot handles, CallFn string.eq on slot0,
   IntBinOp Eq on slot1, And) with NO self-host; (b) a small-variant eq
   (`Tagged("x") == Tagged("y")` — Tagged(String)|Empty): tag eq AND a
   tag-guarded string.eq of the String field (an IfThen/Else merge). Its
   List[String]-literal eq (case 1) should already route via list.eq_str +
   literal materialization. value_deep_eq/compound_eq add Set/(Int,Int)
   list eq + list.contains/index_of over nested operands on top.
   **deep line (1)** — the full map_hvo round (Map[String, List[Option
   [Int]]]): value-aware map drop (per-value element loop — __drop_map_hval
   の flat all-slot rc_dec では Option 要素をリーク), from_list/set/display
   (to_string_hvo → "${v}" routes list.to_string_lo), outer
   list_to_string_lmo + __drop_list_map_hvo, pairs-tuple class StrLo (a
   NEW __drop_list_str_lo — DropListStrStr would leak the value list's
   Option elements), and a lower_owned_heap_field arm for lenlist List
   literals in tuple slots. ~1 wall per full round — LOW yield; do after
   the eq family. Self-host規約 (B27): prim calls hoist to lets; helpers
   return Int (never Unit) for `let _c =` binds.

B28. **(String, Int) tuple eq (45 held, an enabler)**: the first of the two
   diagnosed deep_eq classes — composed directly in MIR (string.eq over the
   slot-0 handles AND an i64 compare of slot 1; borrowed materialized
   operands, no self-host). Probe te1 both branches v0-byte PARITY.
   deep_eq_heap main still needs the small-variant eq (Tagged(String) —
   the tag-guarded field compare, an IfThen/Else merge) — next.
   Ladder: mir 583 / classify 45 / spec 283 / CORPUS WALL OK.

B29. **Small-variant eq + (String, scalar) tuple literals SHIPPED
   (45 → 44)**: the second diagnosed deep_eq class — a custom variant whose
   every ctor carries ≤1 field (scalar or String) composes eq directly in
   MIR: tag-eq AND a tag-dispatched field-compare chain (String fields via
   a borrowed string.eq, scalar an i64 compare, fieldless ctors true; all
   values scalar Bools so the nested IfThen/Else/EndIf merges carry no
   ownership). Plus the (String, <scalar>) TUPLE LITERAL arm in
   lower_owned_heap_field (te1 used vars; the fixture uses literals).
   Probes ve1 (4 variant branches incl. cross-ctor and fieldless) and the
   whole deep_eq_heap fixture END-TO-END v0-byte PARITY (List[String]
   literal eq confirmed already lowering via le1). Ladder: mir 583 /
   classify 44 zero newly-walled / spec 283 / GATE OK / CORPUS WALL OK.

DIAGNOSIS (at 44): **value_deep_eq / compound_eq mains — the 5-chain
   ceiling.** Every individual piece of value_deep_eq lowers with PARITY
   (probes vv1–vv5, vv7: Value eq over call operands incl. null/int/cross/
   str/float/array/object/parse-vs-built, and the ??-let chains). The main
   walls ONLY when ≥5 call-arg heap-`if` lifts chain in one block (vc4
   PARITY, vc5 walls with "heap-result if bound to a let/var" — the
   innermost lifted let is left unduplicated). NOT the
   MAX_DESUGARED_NODES cap (tested at 4×: same wall). Suspect:
   `desugar_let_bound_heap_branch` (or the callarg-lift fixpoint
   interaction) declines at nesting depth 5 — instrument its decline path
   next (DBG env on the desugar chain, diff the depth-4 vs depth-5
   desugared trees). Opening this ceiling likely opens value_deep_eq main,
   compound_eq main, and part of the compound-repr mains (their println
   chains exceed 5 lifts too).

B30. **The 5-chain ceiling OPENED (44 → 43)**: branch_lift (the shared
   almide-optimize pass) gains a DENSE-CHAIN region — a Block holding >3
   statements that contain a heap-result `if`/`match` ANYWHERE (counted
   BEFORE the MIR ANF lift turns call-arg branches into the let chain the
   bounded-duplication gate refuses at rest > 3). Inside the region every
   heap-result `if` EXPRESSION lifts in place to a tail-helper call
   (bottom-up, one helper per if — chain-length immune, no 2^n
   duplication; pre-both, so the caps counts stay 1:1 by construction).
   The helper shape (tail heap-result if over a heap-eq cond) was
   probe-verified first (hl1 PARITY). Probes vc5 / vv8 PARITY and the
   value_deep_eq fixture is END-TO-END v0-byte PARITY (the C-124 Value
   deep-eq contract, all 15 lines). compound_eq / compound_repr mains keep
   their separate List-materialization blockers. Ladder: optimize 11 +
   mir 583 / classify 43 zero newly-walled / spec 283 / GATE OK /
   CORPUS WALL OK.

B31. **All-scalar tuple lists via the OWNED route (43 held, an enabler)**:
   ListElemDrop::ScalarAggregate in try_lower_record_list_literal_as —
   each `(1, 2)` element materializes fresh-flat via
   try_lower_scalar_tuple_construct and is CONSUMED in; drop =
   heap_elem_lists (per-element rc_dec IS the full free for inline-scalar
   blocks). The raw-handle VIEW would double-free this shape (the B24
   trap); the owned route passes the PCC ownership gate (30,070 objects
   ACCEPT). Probes ce1 (list.contains over List[(Int,Int)]) and ce2
   (set.from_list over scalar-tuple pairs — the SET machinery already
   handles tuple elements!) v0-byte PARITY. compound_eq main's residue:
   the List[List[Int]]-literal ARG (list-literal elements) and the
   tuple-key Map literal — next. Ladder: mir 583 / classify 43 / spec 283 /
   ownership ACCEPT / CORPUS WALL OK.

B32. **list.unique/dedup over flat-block heap elements + List[List[scalar]]
   literal ARGs (43 held, an enabler)**: stdlib/list_hshare.almd gains
   `list_unique_hshare` / `list_dedup_hshare` — over-allocate n slots,
   keep-scan with slot-wise structural eq (`__uh_eq_at` prim loads),
   rc_inc-SHARE kept elements in (`__uh_acquire`, whitelisted in
   coown_names.rs), patch the final len via `prim.store32(oh+4, k)`. Plus
   the List[List[scalar]] LITERAL element class in
   try_lower_record_list_literal_as (ScalarAggregate route,
   try_lower_scalar_list_slots per element). LESSON: "if over an
   unresolvable condition" self-host walls were the rc_inc CALLER-NAME
   GATE cascading up — fix is the whitelist + shaping the setter like the
   proven `__ivh_set_append` (const-result arms) + branchless `k+1-seen`
   advancement, not fighting the condition. Probes ce1–ce3 v0-byte PARITY
   (contains + unique over List[List[Int]]). Ladder: mir 583 / classify 43
   zero newly-walled / spec 283 / GATE OK / ownership 30,070 ACCEPT /
   CORPUS WALL OK.

SAFETY. **CRITICAL correctness fix: `_str`/`_skv` heap-search dispatch was
   silently WRONG for non-String heap elements (43 held — a soundness fix, not
   a wall-count move)**: adversarial probing (`list.contains(lt, (1,9))` where
   `lt` holds `(1,2)`) found v1 answering `T` where v0 answers `F` — a
   CONFIRMED silent wrong-bytes bug, live on develop since B25/B29/B31 opened
   tuple/nested-list literal construction (the dispatch itself is older,
   2026-06-16, but was unreachable dead code until those literals could lower).
   Root cause: `list.contains`/`index_of`, the whole `set.*` algebra
   (from_list/contains/union/…), and `map.*_str`/`_skv` (heap-KEY lookup) all
   routed ANY `is_heap_ty` element to the byte-level `__str_eq`/`__skv_eq`
   family — correct only for an actual String (whose `len` field IS a byte
   count); for a tuple/nested-list block `len` is a SLOT count, so `__str_eq`
   compares only the object's first `len` BYTES — a false-positive collision
   past ~2 bytes for any two elements sharing a leading Int. `set.from_list`
   over tuples was worse: narrowing the guard without an explicit wall
   fallthrough re-links the BARE name against the Int-typed generic
   (set_core.almd) — silent POINTER-IDENTITY comparison, the old pre-C-015
   bug, resurrected. Map's heap-key `_str`/`_skv` fallthrough instead produced
   an i32/i64 invalid-wasm crash — louder, but still not the honest wall this
   repr gate exists to guarantee. Fix: (1) new `is_flat_scalar_block_ty` gate
   (mod.rs) — all-scalar tuple / `List[scalar]`, the exact shape B32's
   `__uh_eq` compares correctly (length as ELEMENT count + raw i64-slot
   compare); (2) `list.contains`/`index_of` route String→`_str`, flat-scalar→
   NEW `list_contains_hshare`/`list_index_of_hshare` (list_hshare.almd, reuses
   `__uh_eq`), anything else → explicit UNREGISTERED `_x` wall (never the bare
   name); (3) same narrowing + explicit `_x`/`_key_wall` fallthrough for the
   whole `set.*` family and `map.*`'s key-heap branches; all/any/fold/count
   (list AND set) stay unguarded — pure closure-passthrough, no internal
   eq/copy, safe for any heap element by construction. Verified: probes T/F on
   deliberately-mismatched tuple/list/record elements now WALL cleanly
   (`unlinked stdlib/runtime call`) instead of silently answering wrong or
   crashing; cq1/cq2 (the correct flat-scalar path) stay v0-byte PARITY.
   Ladder: mir 583 / classify 43 zero newly-walled (compound_eq main was
   ALREADY walled upstream at the tuple-key Map LITERAL stage — this bug was
   never actually exercised by the spec corpus, only by hand-written
   adversarial probes) / spec 283 / GATE OK / CORPUS WALL OK. **Lesson: a
   dispatch guard removal is not automatically a safe wall — the bare
   fallthrough name can silently RE-LINK against a differently-typed generic
   self-host (same low-level ABI width) instead of hitting the unlinked-call
   check. Any narrowed guard must route its excluded cases to an explicit
   UNREGISTERED name.** This was caught BEFORE opening the tuple-key Map
   literal (the next planned piece) — had that landed first, compound_eq's
   main would have run its list.contains/set.from_list lines with silently
   wrong results before ever reaching the still-walled Map section.

E6. **testing.assert_throws reclassified native-root (43 → 41)**: `catch_unwind`
   over a WASM `unreachable` trap has no unwind mechanism in the WASI MVP ABI
   — v0's OWN emit_wasm has no wasm form for `assert_throws` either (native-
   only, same class as E4's process/zlib and E5's http.serve), independently
   documented by CHANGELOG.md and `wasm_dispatch_coverage_test.rs`, and the
   fixture header says so verbatim ("wasm:skip — WASM cannot catch panics").
   Added `(m == "testing" && fname == "assert_throws")` to
   `compute_native_ffi_set`'s enumerated no-wasm root set. The render step's
   own wall (capability-gate Unsupported) is unchanged and correct — this
   only fixes classify's accounting, moving 2 entries from REAL to
   native-root. Ladder: mir 583 / classify 41 zero newly-walled / spec 283 /
   GATE OK / CORPUS WALL OK.

B33. **Variant ctor `List[String]` field opened — ADT brick 5 extension (41 → 40)**:
   `Node("root", ["a","b","c"])` (a variant ctor with a String field AND a
   `List[String]` field) walled because the drop generator's field loop
   (`generate_variant_drop_sources`) had no case for a `List[String]` ctor
   field — the record generator already supports this shape (freed via the
   generic `__drop_list_str`), the VARIANT generator just never grew the
   mirror. Fixed by widening the SAME 3 drop-authority sites this class
   always needs together: `variant_needs_recursive_drop` (mod.rs) and
   `VariantLayouts::needs_recursive_drop` (mod_p2.rs)'s `supported_heap`
   predicates now admit `List[String]`; `generate_variant_drop_sources`'s
   field loop emits `__drop_list_str(f{idx})` for it;
   `ctor_list_field_drop_freeable` (binds_p3.rs, the construction-side
   admission) now returns `true` for `List[String]` too. **Sharing gotcha**:
   `__drop_list_str` was previously emitted INLINE by
   `generate_record_drop_sources` only (gated by its own local
   `need_list_str`) — naively adding the same inline emission to the variant
   generator would double-define the fn (a compile error) the moment a SINGLE
   program has both a record and a variant needing it. Extracted the helper
   to a shared `LIST_STR_DROP_SRC` const + `program_uses_list_str_drop_field`
   scan (drop_sources.rs), emitted ONCE in `pipeline.rs`'s two-pass drop
   injection (the `LENLIST_DROP_SRC`/`RES_ILSL_DROP_SRC` precedent) — both
   generators now only REFERENCE the name, never define it. **Second
   fallout**: `render_wasm/tests_part1.rs`'s `lower_source` test helper hand-
   duplicates this same two-pass injection (a SEPARATE copy from
   pipeline.rs's `source_to_ir_with`, predating this session) — missed the
   shared-const wiring on the first pass, breaking 3 unit tests with
   "unlinked call: __drop_list_str"; fixed by mirroring the same
   `list_str_drop` gate there too. **Lesson: a two-pass drop-injection
   pattern that exists in more than one place (production pipeline.rs + this
   test helper) must be patched in ALL copies — the exact `desugar-before-
   both` lesson from earlier this session, generalized beyond desugar
   passes.** nd1 probe (Leaf/Node variant, String + List[String] fields)
   v0-byte PARITY. Ladder: mir 583 / classify 40 zero newly-walled / spec 283
   / GATE OK / CORPUS WALL OK.

B34. **(String, Int) / (Int, String) tuple list literals (40 held, an enabler)**:
   `["k0": 1, "k1": 2]` (a `[key: value]` map-literal desugar with a scalar
   value) as a call ARGUMENT desugars to `map.from_list([("k0",1),…])` — a
   `List[(String, Int)]` literal `try_lower_record_list_literal_as` had no
   class for (only `(String,String)`/`(String,List[scalar])` StrStr and
   all-scalar ScalarAggregate existed). The needed drop machinery already
   existed and is ALREADY used elsewhere: `Op::DropListStrInt` /
   `Op::DropListIntStr` (calls_p2.rs's `+`/concat-operator dispatch and
   binds.rs both already route this exact tuple shape via
   `variant_drop_handles = "list_str_int"`/`"list_int_str"`) — this fix is
   purely wiring the SAME established Op to the LIST-LITERAL classifier,
   which was the one path that hadn't grown it. New `ListElemDrop::StrInt` /
   `IntStr` variants; materialize via the GENERAL masked-tuple builder
   `try_lower_tuple_construct` (already proven — same fn (String,Int)/
   (Int,String) construction already uses via `lower_owned_heap_field`'s
   dispatch, binds_p4.rs:187-215). si1 probe (map literal + a List[(Int,
   String)] literal) v0-byte PARITY. **Scope note**: this does NOT open
   map_fold_heap_acc (entry in the 40) — that fixture's literal now
   materializes fine, but its `main` hits a SEPARATE, already-known gap
   (`map.fold` over a HEAP accumulator, `map.fold_hacc` unlinked — the
   previously-diagnosed "fold_hacc" family, LOW yield, deferred). Does NOT
   help hash_protocol_test (needs `(Record, String)` / `(Variant, String)`
   keys) or generic_chain_unwrap_or (needs `(String, <custom variant>)`) —
   confirmed by direct probe, different tuple shapes, out of this fix's
   scope. Ladder: mir 583 / classify 40 (unchanged — a verified capability
   completion, not a corpus-exercised path yet) / spec 283 / GATE OK /
   CORPUS WALL OK.

B35. **Heap-result `match` as a call argument (40 held, an enabler)**: the
   `If`-in-call-arg dispatch (calls_p2.rs) already had a dedicated arm
   (`try_lower_heap_result_if`) but `Match` fell straight to the generic
   fallback wall — an established asymmetry (`f(if c then a else b)` worked,
   `f(match x {...})` never did). Fixed by adding a dedicated `Match` arm
   that desugars the match to an equivalent if/else-if chain via the
   EXISTING, PROVEN `desugar_match_to_if` (already used at tail/bind
   positions), then lowers the resulting `If` through the SAME existing
   `try_lower_heap_result_if` path — no new lowering machinery. Probe mt4
   (`println(match x { n if n>3 => "big", n => "small" })`, WITH a guard)
   v0-byte PARITY. **Does NOT fully open** `codegen_patterns_test`'s "match
   arms returning tuples" (the tuple-PATTERN-LET desugar,
   `let (label,len) = match {...}`): with a guard it still declines earlier
   (inside `desugar_match_to_if`/`build_match_chain` for this exact
   subject+guard shape — undiagnosed); without a guard it progresses PAST
   this fix into a SEPARATE later wall ("scalar destructure component
   outside the value subset") — a different gap in the tuple-destructuring
   mechanism itself. Both are out of this fix's scope. Ladder: mir 583 /
   classify 40 (unchanged — verified capability completion, message text on
   the one still-walled entry changed to reflect the NEW, narrower blocker)
   / spec 283 / GATE OK / CORPUS WALL OK.

DIAGNOSIS (at 40): **the remaining 40 were triaged in full** (a fork read
   every fixture at its wall site against the current lower/*.rs code). Full
   per-entry breakdown lives in the triage transcript; the load-bearing
   findings:
   - **UNTRACKED-subject match linearization** (control.rs:304, "cannot take
     the both-arms linearization") is a HARD/DEEP, shared root cause across
     ≥5 entries (bidirectional_type_test, option_result_symmetry_test,
     fan_pure_thunks, json_path_edges, + likely more) — the both-arms
     linearization is unsound for a call-bearing arm; opening this needs REAL
     per-arm branching over an untracked (non-Option/Result/variant) subject,
     not a narrow admission widening. Do not attempt piecemeal.
   - **Cross-module variant registry gaps** (#412/#631/#484,
     crossmod_variant_payload_test) — `VariantLayouts` is never populated for
     a FOREIGN module's ctors; HARD/DEEP, a registry-merge project of its own.
   - **Generics + monomorphization** (generic_fn_in_inferred_lambda's
     `List[Box[Int]]`) — tried widening `try_lower_record_list_literal_as`
     with an `is_flat_variant_ty` arm (binds_p3.rs): WORKED for a concrete
     flat variant (`IBox = IB(Int)`, probe bx2 PARITY) but FAILED for the
     generic case — `VariantLayouts` stores the UNRESOLVED generic field type
     (`T`, not `Int`), so `is_flat_variant_ty`'s `!is_heap_ty(fty)` check
     sees a bare type-variable and returns false regardless of the concrete
     instantiation. REVERTED (zero corpus benefit + incomplete + unexercised
     code is itself a risk this session's `_str`-dispatch bug proved). Fixing
     for real needs a mono-aware registry lookup, not a narrow arm.
   - **`map.find`'s Option[(String,Int)] payload — CONFIRMED HARD, a NEAR-
     MISS SOUNDNESS TRAP (map_insertion_order.almd, branch_lift_synth_0)**:
     traced the FULL admission chain — `is_self_host_option_module_fn`
     (mod_p4.rs) is missing `"map" => "find"` (a one-line whitelist gap), and
     `control.rs`'s `is_self_host_option_call` handler already GENERICALLY
     seeds `materialized_options` + `heap_elem_lists` for ANY `Option[heap]`
     subject via `is_heap_elem_list_ty` — so the WIRING looks trivial. It is
     NOT: `heap_elem_lists` routes to the FLAT (no-mask) `Op::DropListStr`,
     which does a BLIND blind `rc_dec` of the payload slot (Option's `len@4`
     doubles as its 0/1 tag, so the "loop" runs 0-or-1 times — the len-as-tag
     trick, intentional). For a `(String,Int)` TUPLE payload, that blind
     rc_dec only decrements the TUPLE's OWN refcount — if it hits 0 the
     tuple's memory frees WITHOUT recursively freeing the tuple's OWN String
     field = **a LEAK**, the exact class of bug the (Value,scalar)-tuple
     precedent in binds_p4.rs (~L216-229) already had to special-case via
     `variant_drop_handles = "value_tuple"` (swapping the flat mask for a
     recursive `$__drop_value_tuple`). No `Op::DropOption<Tuple>` analogue
     exists yet (only the LIST-of-tuples `DropListStrInt`/`DropListIntStr`
     this session's B34 wired up). **Do NOT just add "find" to the
     whitelist** — it would ship a real (if narrower-than-wrong-bytes) leak.
     The correct fix needs a NEW `Op::DropOptionStrInt` (mirroring
     `DropResultStrInt`'s shape but len-as-tag instead of cap-as-tag) wired
     through the full authority chain (Op def in lib.rs, render_wasm_p2.rs
     emission, mod_p3.rs cascade, certificate.rs, render_rust.rs,
     translation_validation.rs) PLUS the admission site
     (`is_self_host_option_call`'s handler, routing to
     `variant_drop_handles` instead of `heap_elem_lists` for a tuple-with-
     heap-field payload) — a real, careful, multi-file brick, not a
     one-liner. Same likely applies to `pattern_test`'s branch_lift_synth_4
     (Result[String,String] match — the STANDALONE match already works,
     probe rss1 PARITY; the fixture-specific failure needs the DENSE branch_
     lift context reproduced, not yet isolated) and `control_flow_test`'s
     branch_lift_synth_3 — re-diagnose with this SAME lens (check for a
     similar blind-flat-drop trap) before touching either.
   - map_fold_heap_acc's residue (after B34) is the separate, previously-
     diagnosed `map.fold_hacc` self-host gap (LOW yield, deferred).
   **Lesson reinforced**: an admission-chain gap that LOOKS like "just add
   the callee to a whitelist" must be checked against what DROP the
   resulting tracked value gets routed to — a flat/masked drop is only sound
   when the payload owns no further heap children one level down. This is
   the THIRD time this exact class of trap has surfaced this session (the
   `_str`-dispatch wrong-bytes bug, the Map/Set key `_x` wall fixes, and now
   this near-miss) — always trace the drop, not just the tag-read, before
   wiring a new admission.

B36. **`List[<Fn>]` literal construction opened (40 held, an enabler)**:
   `[(x: Int) => x + 1, (x: Int) => x * 2]` (a list of non-capturing lambdas,
   #623's closure-parameter shape) had no `ListElemDrop` class —
   `try_lower_record_list_literal_as` walled at the literal itself. Fixed by
   (1) a new `Closure` class, materialized per element via the EXISTING,
   PROVEN `lift_lambda` (the same call-argument lambdas already use), and
   (2) a NEW generated helper `$__drop_list_closure`
   (`LIST_CLOSURE_DROP_SRC`) that recurses into the EXISTING, PROVEN
   `$__drop_closure` per element — required rather than a blind per-element
   `rc_dec`, since the LIST's TYPE (`List[(Int)->Int]`) does not preclude a
   CAPTURING element even though this fixture's elements happen not to
   capture (a blind rc_dec would leak a capturing element's captured heap
   slots — the same trap class documented in the DIAGNOSIS entry above).
   Gated on a new precise scanner `program_uses_closure_list` (mirroring
   `program_uses_closures`'s shape) rather than piggy-backing the broader
   closures gate, so a program with closures but no closure LIST pays no
   dead drop routine. Wired in BOTH `pipeline.rs` and the
   `render_wasm/tests_part1.rs` test-helper's mirrored two-pass injection
   (the B33 lesson, applied proactively this time). VERIFIED via a 10,000×
   leak-loop (construct + scope-end-drop a 2-closure list per iteration,
   no `list.map` needed to isolate the drop itself): completed instantly
   with matching v0/v1 output (20000) — no OOM/hang, which would be the
   signature of a leak at this iteration count. **Does NOT fully open**
   `call_closure_lambda_param.almd` (needs a SEPARATE gap: `list.map` CALLING
   a closure stored in a list element — "list.map with an
   unliftable/closure-list higher-order argument", a different HOF-over-
   closure-elements capability) nor `fan_var_thunk_list.almd` (progressed to
   a SEPARATE gap: `fan.race`/`settle` over a VAR-bound thunk list — not
   inline — never reaches the HOF dispatch since `is_higher_order` only
   recognizes a bare `Fn`-typed/Lambda argument, not a `List[Fn]`-typed Var;
   and even fixing that predicate alone would not suffice unless
   `try_lower_defunc_list_hof` also has a race/settle/any dispatch arm —
   untraced, likely another real gap, not just a predicate widening).
   Ladder: mir 583 / classify 40 (unchanged — both affected fixtures'
   wall MESSAGE changed to reflect the new, narrower blocker; a verified,
   leak-loop-proven capability completion) / spec 283 / GATE OK /
   CORPUS WALL OK.

B37. **(String,Int)/(Int,String) widened to any scalar (40 → 39) + a newly-
   discovered PRE-EXISTING invalid-wasm bug flagged**: B34's `StrInt`/`IntStr`
   tuple-list classification was Int-specific; confirmed via
   `Op::DropListStrInt`/`DropListIntStr`'s WAT emission (render_wasm_p2.rs)
   that the render NEVER reads the non-String slot's contents — it is
   scalar-type-agnostic by construction — so widening the classifier guard
   from `matches!(tys[1], Ty::Int)` to `!is_heap_ty(&tys[1])` (any scalar)
   was a pure, zero-risk completion, not a new mechanism. Probes oue2/oue3
   (`["k0":true,"k1":false]` as a bare bind and as an Option payload)
   v0-byte PARITY. classify moved `option_unwrap_or_else_heap.almd` OUT of
   the WALLED-REAL bucket — **but this is a bucket transition, not a full
   open**: `render_program`'s STRICT check still rejects the fixture, now
   at a DIFFERENT, separately-tracked site (`map.to_string_x` unlinked — no
   self-host display for `Map[String,Bool]`, the interp/self-host-gap
   bucket classify counts separately from WALLED REAL, per its own
   `count_interp_sites`/`would_wall_callees` accounting — NOT a measurement
   bug, an intentional bucket split this campaign has never included).
   **More importantly, isolating the fixture surfaced a PRE-EXISTING,
   UNRELATED correctness bug**: `option.unwrap_or_else(some(map_literal),
   fallback)` — for BOTH `Map[String,Int]` (which predates this session
   entirely, via binds_p4.rs's existing precedent) and `Map[String,Bool]`
   — compiles to INVALID WASM (`type mismatch: expected i32, found i64`),
   confirmed via a minimal repro (oue4) with NO Unsupported wall printed —
   an escaped miscompile at the WHOLE-PROGRAM render step, not a clean
   `LowerError`. **NOT a regression from this session** (verified: this
   exact shape was NEVER reachable before — the corpus's only fixture
   exercising it, `option_unwrap_or_else_heap.almd`, has been walled
   upstream this entire project). `CORPUS WALL OK` / `FORBIDDEN: 0` hold
   because the SHIPPED corpus's copy of this fixture still walls overall
   (via the SEPARATE map-display gap) — this bug is latent, not shipped,
   but is now ONE gap closer to being reachable. Needs its own
   investigation before `option.unwrap_or_else` is ever wired for a Map
   payload — likely a missing/wrong self-host dispatch arm for
   `option.unwrap_or_else` keyed on a Map-typed Option (mod_p4.rs's
   `list_heap_call_name`-style dispatch family), analogous to
   `option.listint_unwrap_or`'s existing List[Int]-specific variant.
   Ladder: mir 583 / classify 39 zero newly-walled / spec 283 / GATE OK /
   CORPUS WALL OK.

DIAGNOSIS (at 39): **two further root causes pinned by direct probing**,
   both confirmed genuine (not quick fixes) — recorded so a future session
   doesn't re-derive them:
   - **`lift_lambda`'s `List[String]` capture ratchet (binds.rs ~L75-97,
     `one_level_exact`) blocks `record_fn_field_test`'s `mock_source`
     (entry 20) and likely more.** Isolated via probes rfc2 (non-capturing
     closure record field, WORKS) → rfc3 (scalar-capturing, WORKS) → rfc4
     (`List[String]`-capturing, WALLS) → rfc5/rfc6 (the SAME
     `List[String]` capture returned DIRECTLY, not via a record — WALLS
     identically, confirming this is `lift_lambda`'s OWN capture-type gate,
     not a record- or tail-position-specific bug). The record CONSTRUCTION
     side, the record DROP generator (`Ty::Fn` field arm), and the
     TAIL-position Record-return path are ALL already fully wired for a
     closure field (confirmed by reading each) — the ONLY gap is
     `lift_lambda` declining a `List[String]` (or `Value`/variant/heap-field-
     record) capture, exactly as the code's OWN comment documents
     ("nested-heap capture... still defers — honest wall, recorded in the
     goal file" — this IS that record). Fixing it needs a THIRD closure-env
     capture category (current: closure_caps [recursive `$__drop_closure`],
     heap_caps [flat `rc_dec`, one-level-exact only], scalar_caps
     [untouched]) — a `nested_heap_caps` category freed via the
     TYPE-SPECIFIC recursive drop (`$__drop_list_str` for `List[String]`,
     analogous to B33's variant-field extension), which means widening the
     packed env header (currently `nh | nc<<16`, two fields) to a third
     count AND updating both construction (which slot group each capture
     lands in) and `$__drop_closure_loop`'s per-slot dispatch to a 3-way
     split. A real, bounded, but NOT small brick — touches the closure
     representation used everywhere (including this session's B36 List[
     Closure] literals), so it needs its own careful session, not a
     drive-by widening.
   - **Tail-position `Result`/`Option` match needing error-propagation in
     the non-Ok/Some arm is a shared root cause across ≥3 entries**
     (`effect_assign_unwrap_test::unannotated_unwraps`,
     `nested_match_option_string_test::is_balanced`,
     `result_option_matrix_test::nested_unwrap` — all three share the
     IDENTICAL wall text "variant (Option/Result) match in tail position
     outside the executable subset... a Const-0 would silently pick a
     wrong arm", from `tail.rs:1082` when `try_lower_variant_value_match`
     declines for a TAIL match whose non-Ok/Some arm must construct an
     early-return/error value rather than a plain scalar expression, e.g.
     `let v = declared_result(); v` in an un-annotated effect fn — the
     auto-`?` desugar produces a tail `match { ok(x)=>x, err(e)=>
     <propagate> }` whose Err arm doesn't fit `try_lower_variant_value_match`'s
     current admitted-arm shapes). Likely the SAME family as entry 1
     (`find_first_even`'s "needs early-return propagation" — that one is a
     LOOP-carried case, these three are the plain-tail case, probably an
     easier subset of the same missing mechanism). Not attempted — probe
     uw2 confirmed the repro but the actual FIX (giving
     `try_lower_variant_value_match` or its tail-position caller a real
     error-arm-propagation path) needs design work, not a quick widening.
   Both are legitimate next targets for a session with fresh context
   budget; neither is safe to rush.

B38. **Closure `List[String]` capture ratchet closed — 3rd env-header
   category (39 → 38)**: contrary to the DIAGNOSIS entry above (which
   flagged this as substantial-but-not-safe-to-rush), on reflection the
   change was fully scoped after tracing every touch point, so it was
   attempted carefully with heavy verification. `lift_lambda` (binds.rs)
   gains a THIRD capture class — `nested_heap_caps` (a `List[String]`
   capture) alongside the existing `closure_caps`/`heap_caps`/
   `scalar_caps` — widening the packed env header from 2 fields
   (`n_heap | n_closure<<16`) to 3 (`n_heap | n_nested_heap<<16 |
   n_closure<<32`, still one i64). ALL boundary checks touched
   consistently: the prologue's LoadHandle-vs-scalar-Load split, the
   construction Dup+store split, and the header packing itself (4
   call sites in binds.rs, verified by grepping every remaining
   `n_heap`/`n_closure` reference after the edit). `CLOSURE_DROP_SRC`
   (drop_sources.rs) gains a 3-way `__drop_closure_loop` dispatch:
   closure_caps → recursive `__drop_closure` (unchanged), nested_heap_caps
   → `__drop_list_str` (B33's generic per-element String-list free — NOT
   the flat `rc_dec` a one-level-exact heap capture gets, which would leak
   each captured String — the exact bug class this session's `_str`-
   dispatch fix and the `map.find` near-miss both already caught, this
   time caught BEFORE shipping), heap_caps → flat `rc_dec` (unchanged).
   `LIST_STR_DROP_SRC`'s injection gate widened to fire whenever
   `program_uses_closures` is true (conservative — a closure's captures
   aren't known without re-running `lift_lambda`'s own free-vars scan, so
   this may occasionally include an unused routine rather than risk a
   missing one), mirrored in BOTH pipeline.rs and the tests_part1.rs
   test-helper (the B33 lesson, applied again). **Verification depth
   matched the risk** (this touches the closure representation used
   throughout the compiler, including B36's List[Closure] literals): probe
   rfc4 (a `List[String]`-capturing closure stored as a record field,
   returned in tail position — exactly `record_fn_field_test::mock_source`'s
   shape) v0-byte PARITY; a DEDICATED 10,000× leak-loop (`clsleak.almd` —
   construct + immediately call a closure capturing a fresh `List[String]`
   each iteration) completed instantly with matching v0/v1 output (30000),
   no OOM/hang; `gate.sh`'s kernel-proven ownership checker ACCEPTs the
   real `closure_capture.almd`/`closure_heap_capture.almd` corpus fixtures.
   `record_fn_field_test::mock_source` fully disappeared from classify's
   output (checked: absent from EVERY bucket, not a B37-style bucket
   transition). Ladder: mir 583 / classify 38 zero newly-walled / spec 283
   / GATE OK / CORPUS WALL OK.

B39. **Flat record/variant Map keys opened — tuple-pair classifier
   generalized past String (38 → 35)**: `hash_protocol_test`'s
   `Map[Color, String]` (`Color = {r,g,b: Int}`, all-scalar) and
   `Map[Direction, Int]` (`Direction = North|South|East|West`, all-
   nullary) — a `[key: value]` map literal over a user Hash-key type.
   Traced `Op::DropListStrStr`'s self-host body (`__ssdrop_list`,
   value_core.almd) and confirmed it is PURELY handle-based (`rc_dec` of
   the raw slot0/slot1 handles, no byte/length interpretation — unlike the
   `_str`-dispatch bug this session already fixed), so it is exact for
   ANY pair of ONE-LEVEL-EXACT heap values, not just two Strings. New
   helper `is_flat_heap_tuple_slot` (binds_p3.rs) — scalar (vacuously
   flat) OR String OR List[scalar] OR a FLAT record (`aggregate_field_tys`
   all-scalar, gated behind `record_or_anon_drop_type_name` already being
   `None` so a RECURSIVE-drop record never reaches it) OR a flat variant
   (`is_flat_variant_ty`). Widened the list-literal classifier's
   StrStr/StrInt/IntStr guards (binds_p3.rs) AND the actual per-element
   MATERIALIZER (binds_p4.rs's `lower_owned_heap_field` Tuple arms — a
   SEPARATE gate that also needed the same widening, discovered when the
   classifier alone didn't move the wall) from `Ty::String`-specific to
   `is_flat_heap_tuple_slot`. **Caught my own bug before shipping**: the
   first draft of the StrStr guard used `is_heap_ty(a) || is_heap_ty(b)`
   (OR) instead of AND — `Op::DropListStrStr` unconditionally `rc_dec`s
   BOTH slots, so a `(Direction, Int)` pair with the OR guard would have
   `rc_dec`'d the raw Int VALUE as if it were a pointer — caught by
   re-deriving the exact semantics before testing, fixed to AND
   (StrInt/IntStr's guards were already correctly AND'd). Probes hp2
   (isolated `List[(Color,String)]` literal) and hp3 (the Map wrapping it
   — reaches the ALREADY-WALLED `_key_wall`/`_hval_wall` from B37's Map-
   key safety fix, confirming no invalid-wasm regression) + a dedicated
   10,000× leak-loop (`fhleak.almd`, both Color-key and Direction-key
   pairs) — instant completion, matching v0/v1 output (30000), no
   OOM/hang. All 3 named `hash_protocol_test` entries fully vanished from
   classify (confirmed absent from every bucket, not a message
   transition). `compound_repr_records_interp` advanced to a DIFFERENT,
   still-walled site (net zero for that one — a message transition, not a
   regression). Ladder: mir 583 / classify 35 zero newly-walled / spec
   283 / GATE OK / CORPUS WALL OK.

DIAGNOSIS (at 35): **B39's flat-heap generalization incidentally opened the
   LITERAL half of two more fixtures, pinning down the SAME single missing
   piece as the highest-leverage remaining target**:
   - `compound_eq.almd`'s `Map[(Int,Int), String]` literal now MATERIALIZES
     (verified via probe ce_tk1 — `aggregate_field_tys` already handles
     `Ty::Tuple`, so an all-scalar tuple key was already covered by
     `is_flat_heap_tuple_slot` without extra work). But `map.from_list`/
     `map.get`/`map.insert`/`map.len` over it hit B37's OWN
     `_key_wall`/`_hval_wall` safety wall (correctly — no working map
     family exists for a non-String key yet, so this is an honest wall,
     not a regression).
   - `generic_chain_unwrap_or.almd`'s `[("x", ValInt(64)), …]` — a
     `(String, <RECURSIVE-drop variant>)` tuple (`ValStr(String)` is
     another ctor of the SAME variant type, so it is NOT flat) — is a
     DIFFERENT shape than B39 covers (B39 only handles FLAT
     records/variants, one-level-exact). This needs the
     `(Value, scalar)`-tuple PRECEDENT (binds_p4.rs ~L216-232: swap
     `record_masks` for `variant_drop_handles`, routing to a type-specific
     recursive drop instead of a blind flat `rc_dec`) generalized to ANY
     recursive-drop variant, which needs a NEW generated helper
     `$__drop_list_str_<V>` PARAMETRIZED by the variant name (unlike
     B33's `LIST_STR_DROP_SRC`, a single static const — this would need a
     generator function emitting one helper per DISTINCT (String, V)
     shape actually used, mirroring `generate_variant_drop_sources`'s
     per-type loop). Also has a SEPARATE, unrelated `get_alignment`
     "scalar tail outside the value subset" wall.
   **The single highest-leverage next piece is a working `Map[<heap-key>,
   V>]` family** (from_list/get/insert/len/contains at minimum) for
   record/variant/tuple keys — needs type-directed KEY EQUALITY (the `==`
   composition B28/B29 already built for tuples/small-variants this
   session is the right building block) plus a hash/scan self-host
   (mirroring `map_skv.almd`'s shape but keyed on structural eq instead of
   `__str_eq`). This would open `compound_eq` fully and is a substantial,
   multi-file brick — properly scoped for a dedicated session, not a
   drive-by extension (unlike B38/B39, which turned out tractable on
   inspection, this one requires genuinely new self-host machinery, not
   just wiring existing pieces).

B40. **`List[Closure]` as a HOF data argument opened (35 → 34)**:
   `call_closure_lambda_param.almd`'s `list.map(fns, (f) => f(10))` (`fns:
   List[(Int)->Int]`) walled at TWO stacked, independent gaps, both fixed:
   (1) `lift_lambda` (binds.rs) never inserted a Fn-TYPED PARAMETER (as
   opposed to a Fn-typed CAPTURE) into `closure_values` — so `f(10)` inside
   the lifted `(f) => f(10)` body couldn't dispatch as a closure call,
   `lift_lambda` returned `None`. Fixed by mirroring `bind_params`'s
   IDENTICAL 3-line Fn-param arm (mod_p3.rs) into `lift_lambda`'s own param
   loop — confirmed by reading `bind_params` that this was always the
   intended parity, just never carried over when `lift_lambda` grew its
   own separate param-binding loop. (2) A SEPARATE, STALE guard
   (binds_p2.rs's `data_arg_has_fn`, the bind-position HOF faithfulness
   check) explicitly walled ANY DATA argument whose type contains `Fn`
   ANYWHERE — with a comment stating "`fns: List[(Int)->Int]` — a list of
   closures **the v1 model cannot represent**" — TRUE when written, FALSE
   now: B36 (this session) shipped exactly that representation
   (`List[<Fn>]` literals + a generated per-element `$__drop_list_closure`).
   Narrowed the guard to exclude the specific `List[Fn]` shape (a Fn
   buried in some OTHER shape — a record/tuple field, `List[List[Fn]]` —
   stays walled; only the now-proven-representable direct case is
   excluded). **Neither fix alone was sufficient** — isolating showed (1)
   makes a STANDALONE lifted closure-calling-its-param lambda work, but
   the FULL HOF call still declined at gate (2) until BOTH landed.
   Verified: ccl2 (the isolated `list.map` reproduction) AND the FULL
   `call_closure_lambda_param.almd` fixture (both the `List[(Int)->Int]`
   and `List[(String)->String]` halves) v0-byte PARITY (`11 20 hi!`); a
   dedicated 10,000× leak-loop (construct 2 closures + `list.map` over
   them each iteration) completed instantly with matching output (20000),
   no OOM/hang. Ladder: mir 583 / classify 34 zero newly-walled / spec
   283 / GATE OK / CORPUS WALL OK.

B41. **`map.find` self-hosted end-to-end — the confirmed near-miss soundness
   trap from the earlier DIAGNOSIS is now CLOSED, not just avoided (34 →
   33)**: `map.find` turned out to not merely need drop-routing (my earlier
   diagnosis) — the CALL ITSELF was unlinked (no v1 self-host existed at
   all; the earlier "UNTRACKED subject" wall MASKED this, since a
   lowering-time `Err` short-circuits before the render-time link check
   ever runs). Built `map_find_skv`/`__skv_find_loop`/`__skv_find_at`/
   `__skv_find_some`/`__skv_find_none` (stdlib/map_skv.almd), modeled
   directly on the PROVEN `list_find_str` shape from list_str.almd — **hit
   the SAME "heap-result if" lowering trap TWICE while writing it**: (1) a
   single recursive fn with `if hit then {let...; Some(...)} else recurse`
   declined ("a block with lets as a heap-result-if's arm does not lower")
   — fixed by splitting into `_at` (holds the lets, tail is a bare
   two-arm-call `if`) and `_loop` (the bounds check), mirroring
   `list_str.almd`'s own documented precedent exactly; (2) `Some((kc, v))`
   — a `(String, Int)` tuple Some-payload — had NO admission arm in
   `try_lower_option_ctor` at all (only all-scalar tuples were admitted,
   B31) — added one, reusing `try_lower_tuple_construct` + the SAME mask-
   swap pattern the existing `(Value, scalar)` tuple case already
   establishes (remove the flat `heap_elem_lists` routing,
   `variant_drop_handles.insert(obj, "opt_str_int")` instead). Wired:
   `is_self_host_option_module_fn` gains `"map" => "find"`; a new
   `program_calls_map_find` scanner gates a new generated
   `OPT_STR_INT_DROP_SRC` (`$__drop_opt_str_int` — Some recurses into the
   tuple's own last-ref check + frees its String slot, None frees
   nothing, the wrapper always frees — the SAME "blind flat rc_dec leaks
   the tuple's String" trap the DIAGNOSIS entry predicted, now fixed
   rather than avoided) mirrored in pipeline.rs AND tests_part1.rs (the
   B33 lesson, third time applied); `control.rs`'s `is_self_host_option_call`
   handling detects an `Option[(String, <scalar>)]` subject and layers
   the SAME `variant_drop_handles` override on top of the existing
   `heap_elem_lists` bind-admission tracking (both coexist — cascade order
   in `drop_op_for` picks the variant-handle route first); `mod_p4.rs`'s
   map dispatch admits `"find"` into the `_skv` family. Verified with
   escalating adversarial depth given the stakes (this is the EXACT shape
   the earlier DIAGNOSIS flagged as a near-miss): mf1 (bare match, hit),
   mf2 (destructuring `let (k,v) = pair` — `map_insertion_order`'s ACTUAL
   shape — both hit AND miss paths) v0-byte PARITY; a dedicated 10,000×
   leak-loop (fresh 3-entry map + `map.find` + destructure per iteration)
   completed instantly with matching output (50005000), no OOM/hang.
   `map_insertion_order.almd`'s `branch_lift_synth_0` entry (the map.find
   match specifically) fully vanished from classify; the FULL fixture
   still doesn't compile (an unrelated for-in-loop wall elsewhere in the
   same file — out of scope here). Ladder: mir 583 / classify 33 zero
   newly-walled / spec 283 / GATE OK / CORPUS WALL OK.

B42. **Tail-position variant constructor calls opened — a general gap, not
   just `list.map(Wrap)` (33 held, an enabler)**: investigating
   `variant_ctor_fn_test`'s `list.map(Wrap)` (a bare constructor passed as a
   first-class function value) with debug instrumentation showed the
   FRONTEND already desugars `Wrap` into a synthetic `(x) => Wrap(x)`
   lambda — so `lift_lambda` (already proven for real lambdas) should have
   handled it. It didn't, because the REAL bug was one layer deeper and far
   more general: `tail.rs`'s `IrExprKind::Call{target: Named{name}}` arm
   (any function-call result returned directly) unconditionally emits a
   plain `Op::CallFn` — with NO check for whether `name` is a registered
   variant CONSTRUCTOR (which has no real top-level wasm function —
   `try_lower_variant_ctor` inlines its block construction at every call
   site — so the plain `CallFn` route always produces an unlinked call).
   Confirmed independently of closures: `fn make(x: Int) -> Boxed = Wrap(x)`
   (an ordinary top-level function, zero relation to HOFs or lambdas) ALSO
   walled. Fixed with a new guarded arm before the generic one, dispatching
   to `try_lower_variant_ctor` — which turned out to already have the
   right ownership shape for tail position (it does NOT itself push its
   result into `live_heap_handles`, leaving that to the caller — so
   returning it directly IS the "move out, don't scope-end-drop" tail
   needs, no extra bookkeeping required). Verified narrow (tc1: a direct
   `Wrap(x)` tail return) AND the original target (wr1: `list.map(Wrap)`,
   PARITY `3`) AND a HEAP-FIELD ctor case (`Wrap(List[String])`) via a
   dedicated 10,000× leak-loop — instant completion, matching output
   (20000), no OOM/hang. **Does not fully open**
   `variant_ctor_fn_test.almd`'s "constructor in list.map" test — a
   SEPARATE, THIRD layer remains: `match list.get(xs,1) ?? Empty {...}`
   (a custom-variant `??` fallback in a match-subject position) hits its
   own distinct wall ("non-lowerable `??` with a heap result in a call-
   argument position", calls_p2.rs) — undiagnosed, out of scope here. Zero
   classify movement (this specific corpus entry needs all three layers;
   the fix is still a genuine, leak-loop-verified closure of a real,
   previously-undiscovered gap — `fn wrapper(x) = Ctor(x)`-shaped
   functions were silently unlinkable before this). Ladder: mir 583 /
   classify 33 zero newly-walled / spec 283 / GATE OK / CORPUS WALL OK.

B43. **`Option[<custom variant>] ?? <ctor fallback>` opened — closes
   `variant_ctor_fn_test`'s THIRD layer, B42's diagnosed follow-up (33 →
   32)**: `try_lower_option_unwrap_or` (control_p3.rs) had no branch for a
   custom-variant Option payload — every existing arm covers Value/List/
   String/record payloads or a bare scalar, so `list.get(xs,1) ?? Empty`
   fell all the way to the final scalar fallback, which reads the payload
   as a raw `Load{width:8}` scalar — wrong for a variant HANDLE (walled,
   not mis-valued, per the existing gate: `matches!(fallback.ty, Ty::String)`
   already excludes non-scalar fallbacks from that path, and no other arm
   claimed it, so it correctly stayed a wall rather than emitting a corrupt
   read). Added a new branch, inserted right after the `value_unwrap_helper`
   block and before the String-specific `??` handling: gated to `expr.ty`
   being `Option[<named type registered in variant_layouts>]` AND the
   fallback being a call/record-construct whose name is a registered ctor
   for that SAME type (so a mismatched-type fallback still declines, not
   silently miscompiles). Built via the SAME `Op::IfThen`/`Else`/`EndIf`
   heap-result-if skeleton the scalar fallback below already proves, but
   with heap-shaped arms: Some → `LoadHandle` @12 (BORROW — the source
   list/Option keeps ownership) then `Dup` to a fresh OWNED reference (the
   same borrowed-param `Some(p)` precedent used throughout this file); None
   → `try_lower_variant_ctor(fallback)` (already a fresh owned value, no
   Dup needed) — both arms end up uniformly owned, matching this file's
   established merge discipline. Verified with a standalone repro (vc1.almd:
   `list.map(Wrap)` then both a Some hit AND an out-of-bounds/empty-list
   None miss) — v0 native and v1-via-wasmtime both printed `3 / 2 / none /
   none` (byte-identical, all three arms of the fallback logic exercised);
   a dedicated 2,000,000× leak-loop (fresh 3-elem Wrap list + hit-match +
   fresh empty list + miss-match per iteration) under a 16MB wasmtime
   memory cap completed in ~190ms with the correct accumulated value
   (6000000), confirming the Dup/ctor-fallback merge doesn't leak even at
   200× the standard 10,000× stress multiple. `variant_ctor_fn_test.almd`'s
   "constructor in list.map" entry fully vanished from classify (all three
   B42-diagnosed layers now closed). Ladder: mir 583 / classify 32 zero
   newly-walled (one entry closed, `variant_ctor_fn_test.almd`) / spec 283
   / GATE OK / CORPUS WALL OK (FORBIDDEN=0).

B44. **`unwrap_never_err_call_types` regression fixed for List/Record/Tuple
   CONSTRUCTION positions — a real, previously-undiscovered v1-only bug (32 →
   31)**: the largest remaining wall bucket (6 entries, "non-empty List[heap]
   literal with nested-ownership elements") turned out to NOT be one deep
   mechanism — it was several UNRELATED walls sharing the same generic error
   text (autotry_construction/compound_repr_*/generic_chain_unwrap_or/
   generic_fn_in_inferred_lambda). Isolated `autotry_construction.almd`'s
   `[step(), step()]: List[Result[Int, String]]` (a never-err effect fn's
   call kept as Result in a list-literal position — the file's own C-068
   contract: "construction positions are target-directed, a Result-typed
   element must KEEP its Result"). `almide-frontend`'s `auto_try.rs` gets
   this exactly right (confirmed by instrumenting the frontend directly: the
   list element's type is `Result[Int,String]` immediately after
   `insert_auto_try` and stays that way through `optimize`/`mono`/`ir_link`/
   `erase_transparent_newtypes`/`inline_pure_call_globals` — traced with a
   temporary per-stage type-printer in `pipeline.rs`). The type flip to raw
   `Int` happens ONE step later, inside `inline_mutual_tail_recursion`
   (mod_p2.rs) — a PROGRAM-level pre-pass whose DOCSTRING says it only
   touches mutually-recursive sibling PAIRS, but which actually computes
   `stripped: Vec<IrFunction> = fns.iter().map(|f| { ...; unwrap_never_err_
   call_types(...); rewrap_never_err_into_result_targets(...); ... })` — i.e.
   applies `unwrap_never_err_call_types` (which blindly rewrites ANY
   never-err lifted call's type from `Result[T,_]` back to raw `T`,
   regardless of WHERE the call sits) to **every function in the program**,
   including ones with no mutual-recursion partner at all (like `main`), and
   the RETURN VALUE uses this same `stripped` set for every non-inlined fn
   too. The existing "undo" pass, `rewrap_never_err_into_result_targets`,
   only re-wraps the two cases its docstring names — a `let`/`var` BIND or
   ASSIGN whose declared type is Result (the #485 rule) — it has NO coverage
   for a call sitting inside a List literal element, a Record field, or a
   Tuple slot, so those three C-068 construction positions were silently
   getting their Result-ness stripped by this pre-pass, walling immediately
   downstream (the registered list/record/tuple drop expects an owned Result
   handle in that slot, gets a bare scalar, declines rather than corrupt).
   **This is a genuine v1-only regression of the ALREADY-FIXED-AT-FRONTEND
   C-068 bug** — v0/native has never had this problem (confirmed:
   `almide run` on all repros gives correct output) since v0 never runs
   `inline_mutual_tail_recursion` at all. Fixed by extending
   `rewrap_never_err_into_result_targets` with a THIRD covered position: a
   new `visit_expr_mut` override (alongside its existing `visit_stmt_mut`)
   that walks List/Tuple/Record CONSTRUCTION exprs and re-wraps any raw
   never-err call sitting in a slot whose OWN target type is Result — List's
   elem type from the list expr's own `Ty::Applied(List,[T])`, Tuple's slot
   types positionally from `Ty::Tuple`, Record's field types from the
   record's own structural `Ty::Record`/`Ty::OpenRecord` OR (for a NAMED
   record) a `record_layouts` lookup by type name — mirroring exactly the
   `field_tys`/`elem_is_result` logic `auto_try.rs` already uses at the
   frontend, just re-applied post-strip. Required threading a new
   `record_layouts: &RecordLayouts` parameter through the function's single
   call site (already available in the caller). Verified with 4 standalone
   repros (scratchpad vc1-style): the bare List[Result] shape (byte-parity
   `list [42, 42]` v0==v1-wasmtime); Record-field + Tuple-slot shapes via
   `match` consumption (byte-parity `42/42/9`); a combined leak-loop
   exercising all three construction positions together in a `while` loop
   at 50× the standard stress multiple (500,000 iterations under a 16MB
   wasmtime memory cap, 51ms, correct accumulated value `88500000`, no
   OOM/hang — this pre-pass runs on EVERY function unconditionally so a
   pervasive-mechanism-grade leak check was warranted). **Investigation
   note**: my first two leak-loop attempts appeared to still wall — turned
   out to be a STALE `target/release/examples/render_program-*` binary not
   yet rebuilt after the fix (the debug binary I'd tested against WAS
   current); re-running `cargo build --release` for that example before
   retesting resolved it — the fix was correct the whole time. Only
   `autotry_construction.almd`'s specific classify entry closed this round
   (the other 5 files in the original 6-entry bucket wall for SEPARATE,
   unrelated reasons — compound_repr_* needs deeper nested-container repr
   work, generic_chain_unwrap_or/generic_fn_in_inferred_lambda are the
   already-diagnosed generics/monomorphization frontier — confirming the
   "6-entry bucket" was several coincidentally-identically-worded walls, not
   one mechanism). Ladder: mir 583 / classify 31 zero newly-walled (one
   entry closed) / spec 283 / GATE OK / CORPUS WALL OK (FORBIDDEN=0).

B45. **`branch_lift.rs`'s dense-region lift widened from `If`-only to
   `If`|simple-pattern-`Match` — closes the 2-entry "heap-result match bound
   to a let/var" bucket (31 → 29, a 2-for-1)**: diagnosed via a fork —
   `alias_combinator_rc.almd`/`codec_decode_errors.almd` both share the
   IDENTICAL shape: 5+ chained `let X = <call>; println(match X {...})`
   statement-pairs in one straight-line block, where each match's arms
   interpolate into a heap `String`. `println(match X {...})` puts a
   heap-result `Match` in a CALL-ARGUMENT position; the MIR ANF-lift turns
   this into a synthetic `let $tmp = match X {...}; println($tmp)` — and
   with 5+ such pairs in one block, the bind-position `Match` handling in
   `binds_p2.rs` (a narrow 2-case subset: tuple-unwrap_or desugar output,
   single-arm tuple-destructure) declines outright — no sound per-arm-alloc
   scope-end-drop encoding exists there for a dense chain. This is EXACTLY
   the class `branch_lift.rs` (B30, commit `1792e5d7`) was built to solve
   for `If` — a dense (>3 heap-branch) straight-line block's density SCAN
   (`stmt_holds_heap_if`, at the top of `visit_expr_mut`) already counted
   BOTH `If` and `Match` toward the threshold, but the actual LIFT TRIGGER
   (`self.dense_depth > 0 && matches!(expr.kind, IrExprKind::If{..})`) only
   ever fired for `If` — `Match` was scanned-for but never wired into the
   lift itself, a pure oversight from B30's original scope. Fixed by
   widening the trigger to ALSO fire for a `Match` whose arms are ALL
   simple patterns (`Some`/`None`/`Ok`/`Err`/`Bind`/`Wildcard` — mirroring
   the EXISTING stmt-level Bind-arm gate a few lines below, which already
   uses this exact same subset for the `let $tmp = match {...}` case, for
   the identical reason: a literal-pattern match desugars to an `if subject
   == lit` chain that DUPLICATES the subject's calls — an unpredictable
   `mir > ir` count — and a custom-variant/tuple/list/record-pattern match
   can still wall inside the tail handler, so lifting it would just
   relocate the wall into a dead helper). `lift_bind_value` (the
   capture/helper-synthesis machinery) is kind-agnostic — it lifts whatever
   `&mut IrExpr` it is given, so no other change was needed; the lifted
   Match's TAIL position is already proven ("renders it for both scalar AND
   heap payloads") by `try_lower_variant_value_match` since B30. Verified: a
   300-repetition straight-line stress repro (1500 total heap-result
   `Match`-in-call-arg sites, 300 synthesized `branch_lift_synth_N`
   helpers) — v0 native and v1-via-wasmtime byte-identical over all 1500
   output lines, completed in 786ms under a 16MB wasmtime memory cap (no
   leak/double-free across 300 independently-synthesized helpers). A
   SEPARATE variant (the same dense chain wrapped inside a `while` LOOP,
   rather than straight-line) still declines with a DIFFERENT, pre-existing
   wall text ("in a call-argument position outside the executable subset")
   — confirmed this is NOT a regression (pre-fix, the expr-level lift never
   existed for `Match` at all, loop or not, so this shape was already
   broken before B45; out of scope here, a distinct follow-up). classify:
   `codec_decode_errors.almd` fully vanished from every bucket (confirmed
   end-to-end PARITY: identical stdout on both targets);
   `alias_combinator_rc.almd` advanced past this wall to a DIFFERENT,
   unrelated pre-existing gap (`list.push` unlinked self-host call — a
   bucket transition, not a full open, tracked separately from WALLED
   REAL). Ladder: mir 583 / optimize 11 / classify 29 zero newly-walled (two
   entries closed) / spec 283 / GATE OK / CORPUS WALL OK (FORBIDDEN=0).

B46. **`unit_main` die-on-error gate narrowed to the VOID convention only —
   closes `cross_module_unit_effect_test.almd::main` (29 → 28)**: diagnosed
   via a fork — the 5-entry "tail-position heap-result if/match" bucket had
   TWO tractable, narrow, unrelated bugs bundled under identical wall text.
   `mod_p3.rs`'s `lower_body_into` gated `desugar_effect_unwrap`'s
   `unit_main` flag (which routes a `!`-desugar's Err/None arm to
   `build_main_die_line` — an abort/exit-1 helper call — INSTEAD OF the
   normal `err(e)`/`none` reconstruction, the "void main" convention) purely
   on `self.fn_name == "main"`, with NO check on `main`'s DECLARED return
   type. A `main` that legitimately declares `-> Result[Unit, String]` (a
   REAL Result-returning main the caller inspects, `cross_module_unit_
   effect_test`'s regression-test shape) got the die-protocol body anyway,
   producing an ill-typed match `tail.rs`/`try_lower_variant_value_match`
   can't lower. `LowerCtx` already carries EXACTLY the right flag for this
   distinction — `decl_ret_is_result: bool` (set at construction from
   `func.ret_ty`, consumed already by `tail.rs:396`'s Result[Unit] tail-
   voiding gate) — just never wired into the `unit_main` computation at
   `mod_p3.rs`'s two call sites (`desugar_effect_unwrap` and
   `desugar_unit_main_err_arms`). Fixed: `let unit_main = self.fn_name ==
   "main" && !self.decl_ret_is_result;`, threaded to both sites. Verified:
   a standalone `main() -> Result[Unit, String]` with a real `!`-propagated
   Err PARITY-matched v0 byte-for-byte (stdout `after ok\nError: boom\n`,
   exit 1, both targets); the EXISTING void-`main() -> Unit` die-on-error
   convention re-verified unchanged (same output/exit, confirming no
   regression to the common case); a 200-chained-`!`-unwrap stress repro
   (each constructing the now-correctly-gated continuation) completed under
   a 16MB wasmtime cap in 50ms with matching output, no leak (this fix is
   low-blast-radius — gated to functions literally named `main` — so a
   lighter stress bound than the pervasive-mechanism 10,000× standard is
   appropriate; 2000 chained unwraps hit a PRE-EXISTING, unrelated Rust
   compiler stack-overflow in `desugar_effect_unwrap_inner`'s recursion,
   not a v1/wasm leak — noted as a possible future recursion-depth item,
   out of scope here).
   **DIAGNOSIS — a real correctness bug FOUND, not shipped**: the fork also
   identified `effect_if_branch_unwrap_test.almd::handler` as tractable — a
   missing `IrExprKind::ResultOk { expr }` arm in `heap_result_arm.rs`'s
   `lower_heap_result_arm` (a redundant `ok(e)` wrapper the frontend's
   target-directed coercion, B44, doesn't reach for `if`/`match` ARM
   positions specifically, only Bind/List/Record/Tuple). Implemented the
   arm (strip + recurse when `expr.ty == *result_ty`, mirroring the
   existing `Unwrap` identity-arm) — it compiled clean and DID close
   `handler`'s wall — but an end-to-end parity probe (`if c then { match
   fetch(p) {...} } else { ok([...]) }`, `fetch: -> List[String]`) caught a
   REAL wrong-bytes bug it exposed: v0 prints `a,b` / `empty`, v1 printed
   `0 ` / `empty` — the FIRST (match) arm, not the ResultOk arm, is wrong.
   Root cause (traced, not yet fixed): `control_p2.rs`'s subject-tracking
   dispatch (`try_lower_variant_value_match`'s `materialized_results_str`
   branch, ~line 365) routes ANY `Result[<heap-Ok>, String]` NAMED-call
   subject to the CAP-AS-TAG @16 read (`materialize_result_str`'s repr —
   correct for a SELF-HOST helper like `value.as_string`) — but a user
   `effect fn fetch(p) -> List[String]` is a LIFTED/auto-wrapped effect fn,
   whose underlying WASM ABI is the ordinary LEN-AS-TAG @4 layout (the SAME
   convention every scalar-Ok effect-fn Result uses) — the dispatch cannot
   tell these two REPR CONVENTIONS apart from `subject.ty` alone (both are
   `Result[<heap>, String]` at the IR level), reads the WRONG tag offset,
   and returns garbage. This is a genuine, previously-unreachable REPR
   MISMATCH class bug (NOT a Camp-4 payload-shape gap) — was NEVER
   observable before because `handler`'s whole `if` always declined (due to
   the OTHER arm's missing ResultOk handling), so the buggy match-arm path
   was compiled but never actually reachable/tested end-to-end. **Reverted
   the `heap_result_arm.rs` ResultOk-arm change** (never committed) rather
   than ship a change that unlocks a reachable wrong-bytes path — `handler`
   stays honestly walled. Fixing the repr-mismatch itself needs a
   discriminator between "self-host cap-as-tag str-result" and "lifted-
   effect-fn len-as-tag Result" at the SUBJECT-tracking dispatch site
   (`control_p2.rs` ~L300-407) — scoped as a real follow-up, NOT a
   drive-by; the ResultOk-arm fix can be re-applied once that's sound (it
   is itself correct in isolation, just currently unsafe to enable given
   what it makes reachable). Ladder (B46 alone, heap_result_arm.rs
   reverted): mir 583 / classify 28 zero newly-walled (one entry closed) /
   spec 283 / GATE OK / CORPUS WALL OK (FORBIDDEN=0).

CORRECTION (post-B46): the repr-mismatch root-cause guess above (a
   `control_p2.rs` subject-tracking dispatch confusing cap-as-tag vs
   len-as-tag for a NAMED call) was **WRONG** — traced further and found
   the ACTUAL bug, one layer earlier. Built the discriminator sketched
   above (a new `LIFTED_EFFECT_FNS` thread_local, populated by
   `inline_mutual_tail_recursion`, consulted in `control_p2.rs`'s subject
   dispatch) and it compiled clean — but `DBG`-instrumenting
   `try_lower_variant_value_match` showed it is **NEVER CALLED** for the
   `mixedok2.almd` repro (`handler2() = match fetch(p) {ok(ls)=>ls,
   err(e)=>[...]}` alone, no outer `if`): `fetch` is a NEVER-ERR lifted
   effect fn (body only ever `ok(...)`s), so `rewrite_never_err_effect_match`
   (mod_p2.rs, runs BEFORE any match-dispatch code) already rewrote the
   whole match into `{ let ls = fetch(p); ls }` — a bare bind + tail Var,
   bypassing `control_p2.rs` entirely. The discriminator fix was **reverted
   as ineffective-for-this-bug** (harmless in isolation, but doesn't touch
   the actual defect — not worth carrying dead code).
   Reading the GENERATED WAT for `fetch` pinned the REAL bug: `fetch`'s own
   BODY (`= ok(["a", "b"])`) compiles to a REAL cap-as-tag `materialize_
   result_str` WRAPPER block (rc@0, len@4=1, cap@8=1, slot0@12=handle-to-
   the-actual-list, slot0's HIGH 32 bits @16=0-as-Ok-tag) — NOT the raw
   `List[String]` `fetch`'s OWN DECLARED signature promises callers. `tail.
   rs`'s dispatch for a bare `IrExprKind::ResultOk` tail (`try_lower_
   option_ctor(tail, &tail.ty)`, ~L580) builds this wrapper because `tail.
   ty` (the `ok([...])` EXPRESSION's own type, assigned by the checker
   following normal `ok()`-construction typing) is `Result[List[String],
   String]` — DIFFERENT from `func.ret_ty` (`List[String]`, the function's
   DECLARED/lowered-signature type, preserved as-is for a lifted fn per
   `lifted_effect_fn_names`'s filter). `auto_try.rs`'s `insert_try_body`
   only strips a tail wrapper when `fn_returns_result` is true (via
   `strip_tail_try`, which targets an auto-inserted `Try` node specifically)
   — for a NON-Result-declared effect fn (`fn_returns_result=false`,
   our case) NOTHING strips an EXPLICIT `ok(...)` sugar wrapper at the
   function's own tail, even when its payload already matches the declared
   return type. `handler2`/`main` then use `fetch`'s returned WRAPPER
   POINTER as if it were the raw list (per `fetch`'s promised signature),
   reading garbage — the `0` output. **Root cause is at the FRONTEND
   level** (`auto_try.rs`'s `insert_try_body`, NOT anywhere in
   almide-mir/control_p2.rs) — a genuinely new stripping rule needed: a
   tail-position explicit `ok(x)`/`err(e)` in a NON-Result-declared effect
   fn body, whose payload type already matches the DECLARED return, must
   collapse to just `x` (mirroring `strip_top_try`'s role for the
   IMPLICIT/auto-`?` case, but for the EXPLICIT sugar case, which that
   function does not touch). Scope: touches `auto_try.rs`'s tail handling
   broadly (every non-Result effect fn whose body EVER uses explicit `ok`/
   `err` sugar, not just `fetch`-shaped ones) — a real, dedicated-session
   item, not a drive-by. `effect_if_branch_unwrap_test.almd::handler`
   stays walled (correctly — no fix landed); NO code changed by this
   correction pass (both attempted fixes were reverted before commit).

B47. **All-scalar tuple `Some((x, y))` admitted as a heap-result MATCH/IF ARM
   value — closes `extract_click_positions` (28 → 27)**:
   `codegen_variant_record_test.almd`'s `list.filter_map(events, (e) => match
   e { Click{x,y,..} => some((x,y)), _ => none })` walled with "unliftable/
   closure-list higher-order argument" — traced to the LAMBDA's own body
   failing to lower when lifted as a standalone fn: `match e {Click{x,y,..}
   => some((x,y)), _ => none}` in TAIL position hit the generic "heap-result
   match outside the executable subset" fallback. `try_lower_custom_variant_
   match` (ADT brick 4, tag@slot0 dispatch with heap-result arms) delegates
   each arm's body to `lower_heap_result_arm` (`heap_result_arm.rs`) — which
   had an `OptionSome{expr} if is_heap_ty(&expr.ty)` fallback arm whose
   INNER `match &expr.kind` only covers `Var`/`Named`-call/pure-String-
   Module-call payloads, no `Tuple` case — so `some((x,y))` (`expr.kind =
   Tuple{[x,y]}`, `expr.ty = Tuple([Int,Int])`, itself heap per
   `is_heap_ty`) matched the OUTER guard but fell to `_ => return None`
   inside, declining. The exact fix ALREADY EXISTED one file over —
   `binds_p4.rs`'s `try_lower_option_ctor` (the BIND-position `let x = some
   ((a,b))` entry point, established by B31) has precisely this arm: build
   the flat tuple via `try_lower_scalar_tuple_construct`, wrap via
   `materialize_opt_str_some` (flat drop is exact — the payload owns no
   inner heap). `heap_result_arm.rs`'s ARM-position mirror had simply never
   been added — the SAME "two sibling functions, one got the fix, the other
   didn't" shape as several earlier stages this campaign. Ported the
   identical arm (same guard, same builders) into `heap_result_arm.rs`,
   checked before the generic `is_heap_ty` fallback. Verified: `click_pos`
   (the isolated lambda body) and the full `extract_click_positions` both
   PARITY-matched v0 (`2` / `10,20` / `30,40`, byte-identical via wasmtime);
   a dedicated 10,000× leak-loop (fresh `Click`/`KeyPress` construct +
   `click_pos` + match-consume per iteration, mixing the hit AND miss arms)
   completed in 13ms under a 16MB cap with the correct accumulated value
   (100010000), no leak. `extract_click_positions` fully vanished from
   every bucket. Ladder: mir 583 / classify 27 zero newly-walled (one entry
   closed) / spec 283 / GATE OK / CORPUS WALL OK (FORBIDDEN=0).

B48. **`(String, <scalar>)` tuple `Some((k, v))` admitted as a heap-result
   MATCH/IF ARM value too — a safety-net enabler, zero classify delta (27
   held)**: while porting B47's all-scalar-tuple arm, spotted a SECOND
   `try_lower_option_ctor` (binds_p4.rs) tuple case never mirrored into
   `heap_result_arm.rs` — `Some((k, v))` for a `(String, <scalar>)` tuple
   (map.find's own `__skv_find_some(k, v) = Some((kc, v))` shape, B41).
   Unlike the all-scalar case, this payload has ONE heap slot (the String),
   so it needs the RECURSIVE `$__drop_opt_str_int` drop (`variant_drop_
   handles = "opt_str_int"`, B41's own generated helper) — a flat
   `DropListStr` (what the bare `is_heap_ty` fallback below it would use)
   only frees the tuple's OWN refcount and LEAKS the String, the exact
   near-miss class B41's DIAGNOSIS caught for the bind position. Since
   `heap_result_arm.rs` had NO `Tuple` case in its `OptionSome` handling at
   all (confirmed by B47), this ARM-position shape was walling honestly
   (never reaching the leak) — but porting the drop-routing NOW closes the
   gap defensively before any future corpus fixture exercises it in arm
   position. Ported `try_lower_tuple_construct` + `materialize_opt_str_some`
   + the `opt_str_int` mask-swap verbatim (same builders B41 already
   proved). Verified: a standalone custom-variant match (`match item {A{k,v}
   => some((k,v)), B{..} => none}`) PARITY-matched v0 byte-for-byte (`hi,42`
   / `none`); a dedicated 10,000× leak-loop (fresh `A`/`B` construct + `pick`
   + match-consume, hit AND miss arms) completed in 11ms under a 16MB cap
   with the correct accumulated value (50005000), no leak. classify: zero
   delta (no CURRENT corpus fixture exercises this exact arm-position shape
   yet) — shipped anyway as a proactive safety fix, mirroring the B33/B36/
   B46-class "verified enabler, no immediate wall closed" precedent this
   campaign has used before for genuine correctness gaps. Ladder: mir 583 /
   classify 27 zero newly-walled (zero closed too — expected) / spec 283 /
   GATE OK / CORPUS WALL OK (FORBIDDEN=0).

B49. **`some(<custom variant ctor>)` admitted as a heap-result MATCH/IF ARM
   value too — third and final `try_lower_option_ctor` twin ported, zero
   classify delta (27 held)**: completing the sweep started by B47/B48 —
   diffed EVERY `IrExprKind::` arm in `binds_p4.rs`'s `try_lower_option_ctor`
   (the BIND-position ctor materializer) against `heap_result_arm.rs`'s
   `lower_heap_result_arm` (the ARM-position twin) and found ONE more
   uncovered case: `some(Number(7))` — Some wrapping a CUSTOM-VARIANT
   constructor call (the option-of-variant shape). `heap_result_arm.rs` had
   NO handling for `OptionSome{expr: Call{Named{name}}} if ctor_to_type.
   contains_key(name)` at all — a match/if arm producing this shape would
   have either fallen to the generic `is_heap_ty` fallback (whose inner
   `match` has no ctor-call case, declining honestly) or, worse, the LATER
   generic `Call{Named}` arm (which assumes a REAL wasm fn exists — a ctor
   has none, `try_lower_variant_ctor` inlines its block construction at
   every call site, so that route would emit an UNLINKED call). Ported the
   identical logic (build via `try_lower_variant_ctor`, route the drop by
   the payload's OWN discipline — `needs_recursive_drop` selects
   `materialize_opt_aggregate_some`/"optrec:<Type>" for a variant with heap
   fields, `materialize_opt_str_some` for a flat one), checked BEFORE the
   generic Named-call arm so a ctor never reaches it. Verified BOTH payload
   classes: a flat ctor (`Number(Int)`, no heap fields) and a recursive-drop
   ctor (`Tag(String)`) — both custom-variant matches (`match item {A{v}=>
   some(Number(v)), B=>none}` / the `Tag(String)` twin) PARITY-matched v0
   byte-for-byte on both targets; a dedicated 10,000× leak-loop (fresh `A`/
   `B` construct + `pick` + a NESTED match consuming the recursive-drop
   `Tag(String)` payload, hit AND miss arms) completed in 12ms under a 16MB
   cap with the correct accumulated value (60000, hello's `string.len`×hits
   + 1×misses), no leak — the recursive-drop path is the higher-risk one
   (an extra heap field to free correctly) so it got the dedicated stress
   test, not just the flat case. classify: zero delta (no current fixture
   exercises this shape in arm position) — same B33/B36/B46/B48-class
   proactive-safety shipping rationale. This closes the FULL `try_lower_
   option_ctor` ↔ `lower_heap_result_arm` twin-coverage sweep — every
   `OptionSome` payload shape the bind position handles, the arm position
   now handles identically. Ladder: mir 583 / classify 27 zero newly-walled
   / spec 283 / GATE OK / CORPUS WALL OK (FORBIDDEN=0).

B50. **`auto_try.rs` explicit-`ok(...)`-sugar stripping — the ACTUAL fix for
   the B46 CORRECTION's diagnosed bug, closes `handler` (27 → 26) AND fixes
   a REAL pre-existing wrong-bytes class**: the B46 CORRECTION traced
   `fetch(p) -> List[String] = ok(["a", "b"])`'s bug to `auto_try.rs`'s
   `insert_try_body`: for a NON-Result-declared effect fn (`fn_returns_
   result = false`), NOTHING strips an EXPLICIT tail-position `ok(x)` sugar
   wrapper — `strip_tail_try` only handles the AUTO-INSERTED `Try` node
   (the implicit `?` machinery), not a user-WRITTEN `ok(...)`. The checker
   types `ok(x)` as `Result[T,_]` by its normal construction rule
   regardless of the enclosing fn's declared return, so this survives
   `insert_try` untouched. But the function's WASM signature is built from
   its DECLARED type (`repr_of(func.ret_ty)`, `List[String]` here) — a
   compiled tail that still returns a REAL `materialize_result_str`
   wrapper object type-checks at the ABI level (both are opaque `i32`
   pointers) but points at the WRONG block shape; `handler2`/`main` then
   read the wrapper as if it were the raw list = garbage (`0` instead of
   `a,b`, confirmed via the generated WAT in the CORRECTION). Added
   `strip_tail_result_ok_sugar` — mirrors `strip_tail_try`'s exact
   recursive shape (Block-tail / both `If` arms / every `Match` arm, so a
   wrapper nested inside a branch like `handler`'s `else { ok([...]) }` is
   reached too, not just a bare-tail `ok(...)`) but strips `ResultOk`
   UNCONDITIONALLY (no `inner.ty.is_result()` guard — the node ITSELF, at
   this position, is always the redundant sugar) and forces every
   traversed level's `.ty` to the function's OWN declared `ret_ty` (so a
   stripped `If`/`Match` doesn't disagree with its now-raw-typed children).
   **CAUGHT A REAL BUG IN MY OWN FIRST DRAFT before shipping**: an
   UNCONDITIONAL version (stripping `ok(x)` at every non-Result-declared
   fn's tail with no further gate) broke `validate(n) -> Int = if n>0 then
   ok(n) else err("negative")` — this file's OWN header comment explicitly
   documents this as a no-regress guard: "must still type (and run) as a
   Result" (its callers `match validate(5) {ok(n)=>n, err(e)=>...}` — a
   GENUINE can-err lifted fn, so its callers need a REAL Result read; my
   strip touched the `ok(n)` THEN-arm but correctly left the untouched
   `err(...)` ELSE-arm alone, producing a type-mismatched `If` that then
   walled with "scalar tail outside the value subset"). Added
   `body_never_constructs_err` — a LOCAL scan (no transitive `!`-
   propagation-through-callees analysis needed, unlike almide-mir's
   `compute_can_err` — just "does this body's OWN AST ever construct
   `err(...)` anywhere") gating the strip: only fires when the answer is
   NO. `fetch` (only ever `ok(...)`) qualifies; `validate` (has a genuine
   `err(...)` branch) does not and is left completely untouched — its
   PRE-EXISTING (already-correct, already-tested) machinery keeps handling
   it exactly as before. **This is a SHARED-FRONTEND change (almide-
   frontend, consumed by BOTH v0 native/Rust codegen AND v1 MIR/WASM)** —
   the highest-blast-radius change this whole campaign has made, so the
   verification bar was raised accordingly: rebuilt the `almide` CLI itself
   (v0) and re-ran `fetch`/`handler` + `validate` + both `unitmain` repros
   directly through `almide run` (byte-identical to pre-fix expectations on
   ALL of them — `validate` confirmed completely unaffected: `5`/`-1`);
   ran the FULL `almide test` suite (283 files, 0 failed, confirming zero
   v0/native regressions program-wide, not just the two touched files);
   ran `cargo test --workspace` (every crate, not just almide-mir/
   almide-frontend — zero failures anywhere) in addition to the standard
   mir/frontend suites; a dedicated 10,000× leak-loop mixing BOTH the
   never-err path (`fetch`) and the can-err path (`validate`) in the same
   loop body completed in 11ms under a 16MB cap with the correct
   accumulated value (35001), no leak. `effect_if_branch_unwrap_test.almd`'s
   `handler` AND `fetch` fully vanished from every classify bucket;
   `validate` was NEVER walled by this change (confirmed present in neither
   the before nor after wall list). Ladder: mir 583 / frontend 11 / mir+
   frontend+workspace all-green / classify 26 zero newly-walled (one entry
   closed) / spec 283 (0 failed) / GATE OK / CORPUS WALL OK (FORBIDDEN=0).

DIAGNOSIS (at 26, two REVERTED dead-end attempts, no code shipped):
   **`protocol_edge_test.almd`'s "match over a never-err effect-fn call with
   a non-`ok(x)` Ok pattern"** (`assert_eq(e.log_info()!, "started")`,
   `e.log_info() -> String = e.message` a never-err lifted effect fn) —
   TWO fix attempts, both reverted, neither closed it (both were zero
   classify-delta and the second didn't even reach its own target code
   path — confirmed via `DBG_`-gated eprintln, cleanly removed before
   revert):
   1. Widened `mod_p2.rs`'s `rewrite_never_err_effect_match` to also
      handle an `ok(_)` WILDCARD Ok pattern (minting a fresh throwaway
      `var` via a new `next_var: &mut u32` param), since its OWN inline
      comment already correctly documents Wildcard as unhandled (the
      function's TOP doc-comment claiming otherwise is STALE — a real,
      small doc-drift bug, still unfixed). Built, verified safe (zero
      newly-walled), but zero corpus impact: `rewrite_never_err_effect_
      match` runs ONCE, early, in `inline_mutual_tail_recursion`
      (pipeline.rs pre-pass) — but THIS test's `!` is nested in a
      CALL-ARGUMENT (`assert_eq(e.log_info()!, ...)`), which only becomes
      a `let x = f()!` shape via `desugar_callarg_unwrap`, and the match
      that shape THEN produces is built by `desugar_effect_unwrap`
      (desugar_unwrap.rs) — which runs LATER, per-function, inside the
      main lowering loop (`lower_body_into`, mod_p3.rs). The pre-pass has
      already finished by the time this match exists, so it can never
      rewrite it — a genuine TIMING gap, not a pattern-coverage gap.
   2. Attempted the SAME never-err short-circuit INLINE in `desugar_
      unwrap.rs`'s `desugar_effect_unwrap_inner` (build the raw let-bind
      directly instead of ever constructing the err/ok match, checking
      `NEVER_ERR_LIFTED_FNS` at construction time instead of post-hoc).
      Compiled clean but a `DBG_`-gated eprintln at the exact check site
      NEVER FIRED for this file — meaning `desugar_effect_unwrap_inner`'s
      per-statement loop never even reaches the point my check lives at,
      for this specific test. Root cause NOT further traced (ran out of
      budget on this specific item) — candidates: `desugar_callarg_
      unwrap` may not actually produce a `let tmp = e.log_info()!` BIND
      shape for a call-arg inside `assert_eq(...)` the way `fetch`/
      `handler`'s repros assumed (maybe test-block compilation or
      `assert_eq` itself takes a different desugar path entirely); or the
      ORDER `desugar_callarg_unwrap` / `desugar_effect_unwrap` run in
      relative to EACH OTHER in `lower_body_into`'s chain doesn't actually
      compose the way assumed (need to re-verify the actual mod_p3.rs
      desugar sequence, not assume it from the file's own doc comments).
   **Next session**: before attempting fixes again, trace with a debug
   print in `desugar_callarg_unwrap` itself (not just its downstream
   consumer) to confirm what shape `assert_eq(e.log_info()!, "started")`
   ACTUALLY becomes, at EACH desugar stage, for THIS specific test file —
   don't assume the transform chain from reading doc comments alone (both
   reverted attempts did, and both were wrong about where the relevant
   code runs). classify unaffected either way (still 26, both attempts
   fully reverted before commit — nothing unverified shipped).

B51. **The THIRD attempt found the real function and closes `protocol_edge_
   test.almd` (26 → 25)** — following the DIAGNOSIS note's own advice
   (trace empirically, don't infer from doc comments): built a MUCH
   smaller repro (`type Event={message:String}; effect fn Event.log_info
   (e)->String=e.message; test{...assert_eq(e.log_info()!,"started")}`)
   and iterated debug prints across it, MUCH faster than re-testing the
   full spec file each time. Found `desugar_effect_unwrap_inner`'s stmt-
   loop (both PREVIOUS attempts' target) is only ever called with `let e =
   Event{...}` for this test — it correctly declines (no `Unwrap` there)
   and falls to `desugar_tail_effect_unwrap`, which only recurses into
   `Block`/`If`/`Match` tails — NOT a bare `Call` (`assert_eq(...)`), so it
   never reaches the argument at all. The REAL match-builder is a
   DIFFERENT, similarly-named function in the SAME file —
   `desugar_let_unwrap` (not `desugar_effect_unwrap_inner`) — called from
   `desugar_heap_branches`'s OWN internal fixpoint (`desugar_branch.rs`),
   AFTER `desugar_callarg_unwrap` (in that SAME fixpoint) lifts the
   call-arg `!` into a real `let tmp = e.log_info()!` statement. Ported
   the IDENTICAL never-err short-circuit into `desugar_let_unwrap` instead
   — compiled, but STILL walled. One more debug pass revealed the ACTUAL
   remaining gap: at the point `desugar_let_unwrap` runs, `e.log_info()`'s
   `CallTarget` is STILL `Method { object, method }` — UNRESOLVED — because
   `desugar_method_calls` (the outer `lower_body_into` step that resolves
   Method → Named) never recurses into an `Unwrap`-wrapped call-argument
   position either, so it hasn't had a chance to run yet for THIS specific
   nesting. `NEVER_ERR_LIFTED_FNS` is keyED by the DECLARED fn's own name
   (`"Event.log_info"` for a UFCS `effect fn Event.log_info(..)` def) —
   which is EXACTLY the `Sym` a `CallTarget::Method{method}` carries, so
   checking `method.as_str()` against the SAME set (in addition to the
   already-checked `Named{name}` case) closes the gap WITHOUT needing
   method resolution to run first. Verified: the ORIGINAL bare-function
   (non-method) never-err call-arg-unwrap repro STILL parity-matches
   (confirms no regression to the case that WAS already working); a NEW
   method-syntax non-test-block repro (`println(e.log_info()! + "!")`)
   PARITY-matches v0 byte-for-byte on both targets; a genuinely CAN-ERR
   UFCS method (`Item.check(i) -> Int = if i.n>0 then ok(i.n) else
   err("negative")`, called via `match i.check() {ok/err}`) in the SAME
   program confirms the can-err path is completely untouched (`5`/`-1`,
   matching B50's `validate` safety story exactly — the SAME never-err
   local-scan-style discipline). A while-loop-wrapped leak-loop hit an
   UNRELATED pre-existing wall ("scalar binding outside the value
   subset") — switched to a 500-repetition STRAIGHT-LINE stress repro
   (matching the B45 precedent for when loop-wrapping hits unrelated
   walls): completed in 52ms under a 16MB cap with the correct
   accumulated value (3500), no leak. `protocol_edge_test.almd`'s
   `__test_almd_protocol with effect fn` fully vanished from classify.
   **Lesson for this whole 3-attempt arc**: when a wall's SOURCE COMMENT
   references a mechanism ("`rewrite_never_err_effect_match`... the rare
   residue"), don't assume that NAMED function is the one to fix — THREE
   different functions in TWO files (`rewrite_never_err_effect_match` in
   mod_p2.rs, `desugar_effect_unwrap_inner` AND `desugar_let_unwrap` both
   in desugar_unwrap.rs) all independently implement variations of the
   SAME "never-err lifted call → plain bind" pattern for DIFFERENT desugar
   ENTRY SHAPES (pre-pass over an existing match / stmt-level `!` / a
   call-arg-lifted `!`) — empirically trace which one ACTUALLY constructs
   the match for YOUR specific repro before touching any of them. Ladder:
   mir 583 / classify 25 zero newly-walled (one entry closed) / spec 283 /
   GATE OK / CORPUS WALL OK (FORBIDDEN=0).

B52. **Two-layer fix for `codegen_patterns_test.almd`'s "match arms
   returning tuples" (25 → 24) — both layers caught by an adversarial
   probe that classify_corpus alone could NOT see**: `let (label, len) =
   match x {s if string.len(s)>3 => ("long",string.len(s)), s => ("short",
   string.len(s))}` walled with "heap-result match in a call-argument
   position" (`calls_p2.rs`). Traced (debug print at the exact decline
   site) to a shape-mismatch: `desugar_match_to_if` wraps its OUTPUT in a
   `Block` (hoisted `let` bindings preceding the `If`) whenever the
   subject isn't one of the freely-substitutable kinds `build_match_
   chain`'s `subject_pure` admits (`Var`/`LitInt`/`LitBool`/`LitFloat` —
   NOTABLY missing `LitStr`; an EARLIER inlining pass had already
   propagated `let x = "hello"`'s literal value into the match subject
   position, since `x` is single-use, so by the time this code runs the
   subject is a bare `LitStr`, not a `Var`). `calls_p2.rs`'s consumer only
   ever pattern-matched a BARE `IrExprKind::If`, declining outright on the
   Block-wrapped form. Fixed by unwrapping the Block generically (lower
   its hoisted `let`s via `self.lower_stmt`, THEN extract the inner `If`)
   — a GENERAL fix, not LitStr-specific: ANY subject needing the hoist
   (not just literals) now works in this position.
   **A first end-to-end parity probe (v0 `long/5/short/2` vs v1) caught a
   SECOND, layered bug** classify_corpus's per-function isolation never
   exercises (render_program SKIPS `test{}` blocks entirely — this session
   confirmed AGAIN that a test-block "not walled by classify" is NOT the
   same guarantee as "produces correct bytes"; only a hand-written
   non-test-block equivalent run through wasmtime actually proves it): the
   materialized tuple rendered, but destructuring `len` (the SCALAR
   component) fell to the generic container-grain `bind_pattern` fallback
   and hit STRICT mode's `Const-0` wall. Traced to `lower_destructure`
   (binds_p2.rs)'s PRECISE tuple-field-extraction seeding, gated on
   `live_heap_handles.contains(&subj)` — but my Match-arm fix (and the
   PRE-EXISTING sibling `If` arm right above it) returned a bare
   `CallArg::Handle(dst)`, never pushing `dst` into `live_heap_handles` at
   all, so the gate always failed and the precise seeding never ran.
   `materialized_call_arg` (calls_p4.rs) is the EXISTING helper every
   OTHER heap call-arg case in this same function already routes through
   — it does exactly this tracking (`live_heap_handles.push`) PLUS seeds
   `record_masks`/`variant_drop_handles` from `aggregate_field_tys` for a
   Tuple/Record `a.ty`. Routed my Match-arm's result through it instead of
   the bare `CallArg::Handle`. Verified: BOTH the hit (`"long"`/`5`) and
   miss (`"short"`/`2`) arms parity-match v0 byte-for-byte on both
   targets; a dedicated 10,000× leak-loop (fresh string + match + tuple
   destructure, both arms exercised each iteration, 20,000 total
   materializations) completed in 11ms under a 16MB cap with the correct
   accumulated value (160000), no leak.
   **DIAGNOSIS — NOT fixed, deliberately out of scope**: the PRE-EXISTING
   sibling `If` arm (line ~488, right above my Match-arm edit) has the
   IDENTICAL `live_heap_handles`-tracking gap — confirmed via a standalone
   `let (a,b) = if c then (...) else (...)` repro (no Match at all), which
   STILL hits the same destructure wall after this fix. Left UNCHANGED
   (not touched by this stage) because it is PRE-EXISTING, already-relied-
   upon code for OTHER passing corpus entries, and changing its
   `live_heap_handles` tracking behavior needs its OWN careful regression
   pass (verifying nothing that currently works starts double-freeing)
   rather than a drive-by extension riding on this stage's momentum — a
   real, scoped follow-up (route the `If` arm through `materialized_call_
   arg` too, exactly like this stage did for `Match`, then re-verify the
   FULL corpus + a dedicated leak-loop before shipping). classify shows no
   entry for this shape currently (unexercised by any corpus fixture), so
   it is a latent gap, not a regression risk today. Ladder: mir 583 /
   classify 24 zero newly-walled (one entry closed) / spec 283 / GATE OK /
   CORPUS WALL OK (FORBIDDEN=0).

B53. **Closed B52's own scoped follow-up: the sibling `If` arm in
   `calls_p2.rs`'s `lower_call_args` had the IDENTICAL `live_heap_handles`-
   tracking gap as the Match arm, latent (not a corpus regression) but a
   real wrong-behavior trap for the exact shape B52's DIAGNOSIS note called
   out — `let (a,b) = if c then (...) else (...)` in a call-argument
   position**. Before touching PRE-EXISTING, already-relied-upon code, ran
   two double-free safety probes: (a) the UNFIXED If-arm's ordinary
   (non-destructure) call-arg use — `println(if c then "yes" else "no")`,
   10,000 iterations under a 4MB wasmtime cap — no leak; (b) B52's own
   Match-arm fix under the analogous ordinary-value shape, same cap/count —
   also no leak. Both confirmed `materialized_call_arg`'s `live_heap_
   handles.push` doesn't conflict with whatever cleanup already handles the
   untracked case, so applied the IDENTICAL routing to the If arm (bare
   `CallArg::Handle(dst)` → `materialized_call_arg(dst, repr, &a.ty)`).
   Verified: `let (label, len) = if string.len(x) > 3 then (...) else (...)`
   (previously walled "scalar destructure component outside the value
   subset") now renders and matches v0 byte-for-byte (`long`/`5`); a
   combined 10,000-iteration leak-loop exercising BOTH the plain If-arm
   call-arg AND the destructuring If-arm call-arg together, under a 4MB
   cap, completed with the correct accumulated value (50000), no leak/trap.
   Ladder: mir 583 / classify 24 zero newly-walled zero closed (expected —
   classify has no fixture for this shape, matching B52's own DIAGNOSIS) /
   spec 283 / GATE OK / CORPUS WALL OK (FORBIDDEN=0).

DIAGNOSIS — `nested_unwrap` (`result_option_matrix_test.almd`) reverted, NOT
   fixed, a genuine regression caught before shipping: `{ let r:
   Result[Option[Int],String] = ok(some(42)); let o = r!; o! }` walls
   "variant match in tail position" because the SECOND `!` (`o!`, an Option
   unwrap) sits in TAIL position, which `desugar_tail_effect_unwrap`
   (desugar_unwrap.rs) has NEVER handled (only Block/If/Match — a bare
   `Unwrap` tail falls through to `_ => None`) — the doc comment at line 14
   explicitly punts this to "tail.rs pass-through", which is sound ONLY for
   a RESULT operand (same repr as the fn's own return) but WRONG for Option
   (opposite tag polarity: Some=len1/None=len0 vs Result's Err=len1/Ok=
   len0) — so nothing ever built the required none/some match for the
   Option case, hence the wall. **Attempted fix**: add an `Unwrap{expr}`
   arm to `desugar_tail_effect_unwrap` (gated on `expr.ty` being
   `Option[_]`) that calls the EXISTING `build_unwrap_match` helper — this
   correctly desugars `o!` into `match o { none=>err("none"), some(v)=>v }`
   (confirmed via debug trace: the new arm DOES fire and DOES build the
   right structural shape). **Two problems surfaced, in order**: (1) the
   OUTER match's Err-arm String bind still declines — `try_lower_variant_
   value_match`'s Camp-4 sub-case-1 gate (`heap_elem_lists` insertion,
   control_p2.rs) requires the subject Result's Ok type to be NON-heap
   (`!is_heap_ty(&a[0])`) before admitting a flat-drop String Err bind;
   here Ok = `Option[Int]`, which classifies as heap (variants are
   heap-repr even with a scalar payload) — a genuinely UNHANDLED case
   (`Result[<flat-heap Ok>, String]`) needing either a widened gate (only
   safe if `Option[Int]`'s own drop is provably flat — a single rc_dec, no
   nested heap) or a new drop-routing entry; NOT attempted (correctness of
   a flat rc_dec for the Ok-side handle vs a dedicated recursive drop
   wasn't verified — this needs its own careful pass, not a rushed
   extension). (2) SEPARATELY, and more seriously: **the fix regressed
   `unwrap_option_some`** (classify showed 1 newly-walled entry) — `{ let
   o: Option[Int] = some(10); o! }`, a BARE tail Option-unwrap with NO
   preceding stmt-level unwrap, was previously PASSING via tail.rs's raw
   pass-through (empirically proven working — the test asserts `unwrap_
   option_some()! == 10`). The new desugar arm intercepts it too and
   builds `match o { none=>err("none"), some(v)=>v }`, but types the
   reconstructed `ResultErr` node at `tail.ty` (`o!`'s own evaluated type,
   `Int` — the UNWRAPPED scalar, since NO other unwrap in this function
   establishes Result-wrapping) instead of a genuine `Result[Int,String]`
   — a type-mismatched `ResultErr{..}, ty: Int` node that `lower_scalar_
   arm` cannot lower as a scalar value, rolling back the whole match →
   NEW wall. The root confusion: `tail.ty`/`body.ty` at a bare function
   tail is the function's DECLARED scalar type, not automatically
   Result-wrapped — stmt-position `!` (the proven, shipped path) evidently
   relies on the type CHECKER already having assigned `body.ty` as
   Result-wrapped for THOSE functions (a fallible operation NOT in tail
   position needs true early-return, which only a Result ABI supports);
   tail-position `!` apparently does NOT get this treatment from the
   checker (by design — pass-through doesn't NEED it for Result operands),
   so `tail.ty` legitimately stays the bare scalar type there. Reusing
   `tail.ty` as the "enclosing fn's real Result type" is FALSE precisely in
   this case. **Reverted cleanly** (`git checkout --`, confirmed classify
   matches the B53 baseline exactly, zero diff). A real fix needs the
   function's TRUE compiled return type threaded down independently of
   `tail.ty` (not derivable locally at the point `desugar_tail_effect_
   unwrap` sees a bare Unwrap tail) — likely requires either passing the
   fn's `ret_ty`/a `does-this-fn-need-result-wrapping` flag through the
   whole `desugar_tail_effect_unwrap` recursion, or restricting the new
   case to ONLY fire when already nested inside another match/if arm that
   is ITSELF known to need Result-wrapping (i.e., not at the true top-level
   bare-tail position) — unexplored, next attempt should start there.
   **Current 24, unchanged** (no commit made).

DIAGNOSIS — the ROOT CAUSE shared by (at least) `unannotated_unwraps`
   (effect_assign_unwrap_test.almd), `nested_unwrap` (result_option_matrix_
   test.almd), and plausibly `is_balanced` (nested_match_option_string_
   test.almd) — all three "variant match in tail position" walls — traced
   via debug instrumentation directly in `try_lower_variant_value_match`
   (control_p2.rs, temporarily added then FULLY REVERTED — `git checkout
   --`, confirmed classify matches the B53 baseline exactly). Repro:
   `effect fn declared_result() -> Result[Int,String] = ok(7)` /
   `effect fn unannotated_unwraps() -> Int = { let v = declared_result();
   v }`. This is the STMT-position `Try`-node auto-`?` path (the SAME
   mechanism the doc comment atop desugar_unwrap.rs calls "PROVEN... VERIFIED
   to byte-match... porta.start's every shape") — the stmt loop correctly
   fires, both arms of the reconstructed `match declared_result() { err(e)
   => err(e), ok(v) => v }` parse and bind cleanly (confirmed via trace: no
   rollback anywhere in subject-tracking or arm-pattern-parsing). The ACTUAL
   failure is later: `try_lower_variant_value_match` receives `result_ty =
   tail.ty = Int` (the WHOLE MATCH's `.ty`, which `build_unwrap_match` set
   to `body.ty.clone()` — and `body.ty` here is the function's DECLARED
   sugar type, `Int`, NOT a Result-wrapped type) — so `heap_res =
   is_heap_ty(Int) = false`, routing BOTH arms through `lower_scalar_arm`
   (the SCALAR path). The Err arm's body is `ResultErr{Var{e}}` (a node
   that structurally CONSTRUCTS a heap Result — conceptually never a
   "scalar value" regardless of its `.ty` annotation) — `lower_scalar_arm`
   has no case for `ResultErr` and falls to its default `lower_scalar_
   value`/`try_lower_scalar_call` bucket, which declines → arm lowering
   fails → `try_lower_variant_value_match` rolls back → falls to tail.rs's
   final "variant match in tail position" wall. **Why "porta.start"/`sum_
   parse` work but this doesn't**: those precedents are declared EXPLICITLY
   `-> Result[Int,String]` (not a bare scalar), so their `body.ty` IS
   ALREADY the wrapped type — `result_ty` reaching `try_lower_variant_
   value_match` is correctly `Result[Int,String]`, `heap_res=true`, and
   BOTH arms route through `lower_heap_result_arm` (which correctly handles
   the "one arm raw-scalar implicit-Ok, other arm explicit ResultErr"
   asymmetry — the actual proven case). `unannotated_unwraps` is declared
   `-> Int` (auto-wrap sugar: the source-level type is scalar, but the
   REAL compiled ABI must be `Result[Int,String]` since `main`'s call site
   `unannotated_unwraps()!` type-checks — the checker treats the CALL as
   unwrappable, proving the true ABI is Result-shaped — even though `func.
   ret_ty` AND `func.body.ty` both stay `Int` throughout, matching the
   user-facing sugar). **This is a genuinely unhandled case, not a
   regression** — the STMT-position mechanism has apparently ONLY ever been
   proven for functions ALREADY declared with a Result/Option return; the
   "declared scalar, auto-wrapped due to an internal propagating unwrap"
   case has no corpus precedent and no infrastructure. **A real fix needs**
   a "real ABI return type" for a function, DISTINCT from `func.ret_ty`
   (which stays the declared sugar type for WASM signature purposes via
   whatever ALREADY reconciles this at the callee/caller boundary for
   OTHER auto-wrap cases — unidentified in this pass, needs its own
   investigation) — threaded into `build_unwrap_match`'s reconstructed
   arm/match typing INSTEAD of `body.ty` whenever the declared type isn't
   already Result/Option, so `try_lower_variant_value_match` correctly
   selects the heap-result (`lower_heap_result_arm`) path instead of the
   scalar path. High blast radius (signature generation, `repr_of`, the
   call-site unwrap machinery) — deliberately NOT attempted this session;
   scope it as its own investigation before touching. **Current 24,
   unchanged** (fully reverted, zero diff).

DIAGNOSIS — `map_fold_heap_acc.almd`'s "List argument cannot be faithfully
   materialized" wall is the compound_repr_* cluster in disguise, NOT an
   independent bug. Bisected (no source edits made — `git status` clean
   throughout) down to a single minimal repro with NO `map.fold` involved
   at all: `let m: Map[String, Map[String, String]] = ["k0": ["k0": "x"]]`
   — a bare bind of a NESTED Map literal (a Map whose VALUE type is itself
   a Map) — walls on its own, used or not. A Map is represented internally
   as a "paired-slot List" (per the existing comment in calls_p2.rs), so
   this is STRUCTURALLY the same shape the already-documented "non-empty
   List[heap] literal with nested-ownership elements (a heap-field record/
   tuple, a list, a call result) cannot be faithfully materialized" wall
   covers (`compound_repr_interp.almd`/`compound_repr_records_interp.almd`/
   `compound_repr_recursive_interp.almd`/`generic_chain_unwrap_or.almd`/
   `generic_fn_in_inferred_lambda.almd` — 5 of the current 24 entries) —
   just reached via Map-literal construction instead of List-literal
   construction. `map_fold_heap_acc.almd`'s ACTUAL fold-with-heap-
   accumulator logic (the file's own stated purpose per its header
   comment) is unaffected — ALL of its `map.fold` calls over flat
   (non-nested) Map/List shapes render fine in isolation (verified: the
   first three `map.fold` lines of the file, extracted alone, lower past
   this specific wall — they hit a SEPARATE, unrelated "unlinked map.fold_
   hacc" self-host-registry gap instead, likely just needing all 5 of the
   file's functions present for correct registry linking, not investigated
   further here). The ONLY line that hits "List argument cannot be
   faithfully materialized" is the `map.get_or(["k0": ["k0": "x"]],
   "missing", y3)` sub-expression's nested map-literal argument. Given
   this is the SAME "generics/monomorphization frontier" gap already
   scoped for the compound_repr_* cluster (not a scoped, safe fix — it
   needs the nested-heap-element container-literal construction work,
   not a decline-point extension), no fix attempted. Recommend: when the
   compound_repr_* cluster is eventually tackled, re-classify_corpus
   afterward — `map_fold_heap_acc` likely closes as a side effect (it may
   even be worth ADDING to that cluster's fixture list, since it's the
   only entry currently exercising the Map-literal path instead of the
   List-literal path — same construction machinery, different literal
   syntax). **Current 24, unchanged** (zero source edits made or
   reverted).

B107. **Closed 2 of 3 "heap-result match/if outside the executable subset"
   entries — the SAME `Block`-wrapping gap B52 fixed for the call-argument
   consumer, but at a DIFFERENT consumer site (tail.rs's own heap-result
   Match handler) that had never been touched (26 → 24 in this narrower
   cluster; 24 → 22 corpus-wide)**: `branch_lift_synth_3`/`branch_lift_
   synth_4` are NOT source identifiers — they're SYNTHESIZED tail-helper
   functions `almide-optimize/branch_lift.rs` creates by lifting a
   let-bound heap `if`/`match` (Some/None/Ok/Err/Bind/Wildcard arms only)
   out of its enclosing scope into a fresh top-level function, so the
   proven tail-position lowering can render it. Bisected the ACTUAL source
   trigger via a greedy block-removal script over `control_flow_test.almd`
   (60 top-level test/fn blocks, iteratively drop-and-recheck) since the
   synthesized name carries no back-reference to source — converged on
   `test "match with string guard" { let result = match s { x if string.
   contains(x,"world") => "has world", _ => "no world" }; assert_eq(...) }`
   (a GUARDED Bind pattern + Wildcard fallback, String subject, OUT of any
   loop — eligible because `visit_stmt_mut`'s fire condition is `loop_depth
   > 0 OR all-arms-are-{Some,None,Ok,Err,Bind,Wildcard}`, unconditionally
   admitting this out-of-loop shape too). Traced via debug instrumentation
   in `tail.rs`'s heap-result `Match` handler (added then fully removed):
   `desugar_match_to_if` returned a `Block`-wrapped result (hoisted `let`s
   before the `If`), not a bare `If` — the EXACT same `subject_pure` gap
   B52 diagnosed (`Var`/`LitInt`/`LitBool`/`LitFloat` only, missing
   `LitStr` — a single-use `let s = "hello world"` gets constant-propagated
   to a bare `LitStr` subject upstream) — but this consumer (the tail
   position's OWN heap-result Match dispatch) only ever matched a bare
   `IrExprKind::If`, declining outright on the Block-wrapped form, exactly
   like `calls_p2.rs` did before B52. Applied the IDENTICAL generic fix:
   unwrap the `Block` (lower its hoisted `let`s via `self.lower_stmt`, then
   extract the inner `If`) before calling `try_lower_heap_result_if` — not
   LitStr-specific, any subject needing the hoist now works here too.
   Verified: a hand-written non-test-block equivalent (both a direct-tail
   `fn classify(s) = match s {...}` AND a let-bound form that actually
   triggers the branch-lift, confirmed via `branch_lift_synth_0` appearing
   in the WAT) matches v0 byte-for-byte on both inputs (`has world`/`no
   world`); a dedicated 10,000-iteration leak-loop (fresh string match
   every iteration) under a 4MB wasmtime cap completed with the correct
   accumulated value (90000), no leak. **`wrap_lists` (playground_default.
   almd) is a DIFFERENT root cause, NOT fixed by this change** — its wall
   is a bare `IrExprKind::If` (not a `Match`) directly as the function's
   own tail (`if result.in_ul then result.out + ["</ul>"] else result.out`,
   both arms Member-access + list-concat), never touching `desugar_match_
   to_if` at all — `try_lower_heap_result_if` itself declines on this
   shape for an unrelated reason, not investigated further this stage (out
   of scope — this stage targeted the `Match`-Block-unwrap gap
   specifically). Ladder: mir 583 / classify 22 zero newly-walled (2
   closed) / spec 283 / GATE OK / CORPUS WALL OK (FORBIDDEN=0). **Current
   22**.

DIAGNOSIS — the 4-entry "match over an UNTRACKED subject with a
   call-bearing arm" cluster, per-entry findings (nothing shipped, one
   near-miss reverted after a correctness check caught it):
   `json_path_edges.almd :: p_set` (`fn p_set(label, r: Result[Value,
   String]) = match r { Ok(v) => println(..json.stringify(v)..), Err(e) =>
   .. }`): found and CONFIRMED a real twin-function drift — `try_lower_
   result_match`'s (control_p2.rs) Err-arm heap-bind admission only checks
   `heap_elem_lists.contains(&subj)`, while its value-position sibling
   `try_lower_variant_value_match`'s `heap_or_scalar_bind` (same file,
   ~463-479) ALSO admits `value_result_lists`/`value_result_results`/
   `resrec:`/`optrec:` — for `Result[Value,String]` (routed to `value_
   result_results` by `seed_variant_param`, since `Op::DropResultValue`
   needs the RECURSIVE Value drop, not flat `DropListStr`), the statement-
   position gate is strictly narrower, so the match falls through to the
   both-arms linearization wall. Widening the Err-bind gate to match the
   twin's admission set (`|| value_result_lists.contains(&subj) ||
   value_result_results.contains(&subj)`) DOES make `p_set` render (no
   longer walled) — but an adversarial parity check caught it BEFORE
   shipping: `json.as_int(v)` on the Ok arm's bound payload returned `none`
   (fallback -999) instead of the real value 5 — a SILENT WRONG VALUE, not
   a wall. `Op::DropResultValue`'s drop semantics were independently
   verified sound (frees Err-String OR recursively drops the Ok Value at
   the subject's own scope-end, same discipline `heap_elem_lists` already
   relies on) — so the DROP side isn't the problem; the OK-arm's payload
   BIND (`try_lower_result_match`'s plain `LoadHandle @12` + `param_values.
   insert`) evidently doesn't set up whatever ADDITIONAL tracking a `Value`
   payload specifically needs for `json.as_int`/`json.stringify` to read it
   correctly (unlike a plain String payload, which the SAME LoadHandle
   path already handles correctly for `heap_elem_lists`-tracked subjects —
   `Value` is a tagged dynamic union, not a flat byte buffer, so it likely
   needs the SAME extra seeding `try_lower_result_match`'s nested-variant-
   payload branch (lines ~110-131) does for Option/Result Ok binds, just
   generalized to a bare `Value` bind too — NOT investigated to a fix
   within this session's remaining budget). **Reverted cleanly** (`git
   checkout --` on control_p2.rs only — this fork's sole edit; unrelated
   concurrent in-progress edits from a parallel session in drop_sources.rs/
   pipeline.rs/render_wasm/tests_part1.rs were left untouched per git
   safety rules). A real fix needs the SAME extra Value-read seeding
   `try_lower_variant_value_match`'s twin already does correctly (worth
   diffing the two functions' Ok-bind paths line-by-line, not just the
   Err-bind gate) — next attempt should start there, and MUST re-run this
   exact `json.as_int` parity check before shipping.
   `bidirectional_type_test.almd :: "structured error - overflow variant"`
   (`let e: Result[Int, MathError] = err(Overflow("too big")); match e {
   ok(_)=>.., err(Overflow(msg))=>.., err(_)=>.. }`): DIFFERENT and likely
   DEEPER shape — `MathError` is a CUSTOM VARIANT as the Err type (not the
   pervasive `Result[_, String]` convention every admission gate in this
   codebase assumes), AND the Err arm nests a CTOR pattern (`Overflow(msg)`)
   inside the `err(..)` pattern. Not investigated beyond reading the
   source — genuinely out of scope for a quick pass (would need a new
   Err-payload-is-a-registered-variant drop routing, separate from every
   existing `Result[_,String]`-shaped gate).
   `option_result_symmetry_test.almd :: "option.collect_map all some"`
   (`let c: Option[List[Int]] = option.collect_map([1,2,3], (x)=>some(x*2));
   match c { some(vs)=>.., none=>.. }`): `option.collect_map` is a SELF-
   HOST stdlib fn (`stdlib/option.almd`), not a `@intrinsic`/registry
   entry — whether `is_self_host_option_call` (control.rs:1011) recognizes
   a call to a self-hosted Almide-defined function (vs. only intrinsics)
   was NOT checked; this is plausibly a simpler, different gap than
   `p_set`'s (subject-tracking recognition for a CALL, not a param) — worth
   checking first in a future attempt, quick to rule in/out.
   `fan_pure_thunks.almd :: main`: not investigated this pass (budget
   spent on `p_set`'s deeper dive) — `fan.race`/`fan.any`/`fan.settle`
   results feeding a `println("...${r}...")` interpolation are the likely
   subject; may share EITHER the `p_set` gap (if some arm's bound payload
   needs the same extra Value-style seeding) or be its own shape — unknown.
   **Current 22, unchanged this stage** (all 4 entries still open).

## What NOT to do

- No WAT/Rust regex port into the v1 renderer (invariant 2).
- No "close enough" match semantics — v0 is the oracle, byte-for-byte.
- No opening the untracked-match / interp-in-call-arg buckets here (separate
  lowering work, different skill set — keep this goal stdlib-shaped).
- Do not weaken the purity/drift gates to force a link.
