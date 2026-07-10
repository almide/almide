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
