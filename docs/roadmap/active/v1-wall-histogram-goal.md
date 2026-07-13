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

## What NOT to do

- No WAT/Rust regex port into the v1 renderer (invariant 2).
- No "close enough" match semantics — v0 is the oracle, byte-for-byte.
- No opening the untracked-match / interp-in-call-arg buckets here (separate
  lowering work, different skill set — keep this goal stdlib-shaped).
- Do not weaken the purity/drift gates to force a link.
