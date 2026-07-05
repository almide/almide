# v1 stdlib self-host — the machinery phase (Option / List-building / closures)

Status: **~28 functions self-hosted + executing = v0 (corpus-wall ACCEPT 13139/4083/3582
every commit). The Option match-execution machinery is DONE; the campaign is now in clean
batch-production (string/list scalar/read predicates) with the Option-returning fns reusing
the materialized-Option layout.** Goal `/goal`: run until ALL ~381 v0 stdlib fns execute
byte-for-byte. This records the de-risked designs so harder slices are implemented from a
settled plan.

## Done (self-hosted + executing = v0) — ~28 fns
- **core**: int.to_string, print_str(println)
- **string**: len, repeat, is_empty, slice, trim, starts_with, ends_with, contains, count,
  index_of (Option, codepoint), last_index_of (Option, codepoint)
- **math**: abs, max, min
- **list**: len, is_empty, sum, get_or, get (Option), first (Option), last (Option),
  contains, index_of (Option), product, max (Option), min (Option)
- **map** (`Map[String,String]` = map_str): new, set, get, keys, values, len, …, and **entries**
  (`map_entries_str` → `List[(String,String)]`, 2026-06-21) — builds rc-shared (key,value) tuples,
  freed by `$__drop_list_str_str` (the new `(String,String)` tuple-list: `DropListStrStr`, the
  `str_str_elem` concat case, the tuple-element defunc-map + `let (k,v)=pair` destructure). The svg
  render_attrs lever ([[v1-records-svg]]).

The string pattern: read header
(`handle(s)+4`=byte-len, `+12`=data) via prim.load8/32, build via prim.alloc_str+store8,
recursion for loops. The list pattern: `+4`=element count, `+12`=8-byte i64 slots
(prim.load64). Registry groups by file: `self_host_runtime() = &[(source, &[(impl_fn,
call_name)])]`; auto-link renames impl→call (keeps the caps gate reading a known-pure
`module.func`); `lower_source` dedups by name (recursively-linked print_str copies).

## Machinery 1 — Option (unblocks list.get / first / last)
**STATUS: match-execution CORE DONE** (commit "Execute match over a materialized Option…").
A `match opt { Some(x) => …, None => … }` over a DIRECTLY-bound `Some(scalar)`/`None`
now EXECUTES the taken arm only (= v0), byte-matching: `Init::OptSome { payload }` (a
1-element list, render via `list_new(1,…)` + `list_set 0 payload`); `None` stays
`Init::Opaque` (len0). `try_lower_option_ctor` (binds.rs) materializes + records the dst
in a new `materialized_options: HashSet<ValueId>`; `try_lower_variant_match` (control.rs)
reads `len` as the tag (`prim.Handle`+`Load{4}`@+4), extracts `data[0]` (`Load{8}`@+12)
on the Some branch, wraps the per-arm frames in `IfThen`/`Else`/`EndIf`. The
`materialized_options` GATE is the soundness key — only a tracked subject is read by len,
so a non-materialized Opaque Option keeps the linearized fallback; AND because both arms
are per-arm-balanced, even a gate miss is at worst a wrong-arm CORRECTNESS bug, never a
memory bug. Cert-neutral (Alloc = i init-agnostic; markers no-op; scalar prims):
corpus-wall ACCEPT UNCHANGED 13139/4083/3582. Tests: `variant_match_over_a_materialized_
option_executes` (Some(42)/None → 42/none), `option_allocating_loop_matches_bounded`
(3000-iter materialize+match+free = bounded, no leak). SCALAR payload + UNIT arms only.
**NAMED FNS DONE — list.get / list.first / list.last self-hosted** (commits "Self-host
list.get…" + "Self-host list.first and list.last…"). CALL-RESULT tracking landed:
`is_self_host_option_module_fn(module,func)` (the SINGLE source shared by binds.rs's
bound-var path and control.rs's direct-subject path) marks a self-host Option call's dst
materialized, so BOTH `let o = list.get(xs,i); match o` AND `match list.get(…)` execute.
The impls (stdlib/list_get.almd) return `Some(scalar)`/`None` through TAIL-materialized
helpers (`__opt_some_at`/`__opt_none_int`, the tail.rs OptionSome/None hook); list.get's
body is a heap-result-if with those CALL arms (already handled). get(1)=Some(20)/get(5)=
None, first([10,20,30])=Some(10)/last=Some(30)/first([])=None all byte-match v0; a
2000-iter list.get-and-match loop is bounded (no leak). corpus-wall ACCEPT unchanged.
**NEXT**: (a) scalar-RESULT arms (`let s = match o { Some(x)=>…, None=>…}`). (b) heap
payload (Some[String] — the element-alias refinement). (c) `??`/`!` over a materialized
Option (the unwrap operators, deferred today). (d) string.split / list.map (the harder
List-building / closure machinery — Machinery 2/3).

**Layout (DE-RISKED): Option = a 0-or-1-element LIST block** — reuse the existing list layout
`[rc][len@4][cap@8][data@12]`. `None` = a 0-element list (len=0). `Some(x)` = a 1-element list
(len=1, `data[0]`=x). No new block kind, no new Init. The ownership cert is UNCHANGED: a
construction is still ONE `Alloc` (`i`), identical to today's Opaque — only the tag(len)+
payload(data[0]) stores are added (no ownership delta).
- CONSTRUCT: intercept `IrExprKind::OptionSome{inner}` → `Alloc` a 1-slot list + store
  `data[0]=inner` (like a `[inner]` list literal); `OptionNone` → a 0-slot list (`[]`). At the
  bind/tail/arm positions (where they're Opaque today, binds.rs catch-all).
- DESTRUCTURE: `match opt { Some(x) => A, None => B }` → read `len` (load32 `handle(opt)+4`);
  if `len != 0` then bind `x = data[0]` (load64 `+12`) + A else B. Extend the match lowering
  (`desugar_match_to_if` / `lower_branch`) for `Some(bind)`/`None` patterns → a tag-test if.
- SCOPE FIRST to SCALAR payload (Option[Int]): `x` is a scalar (no ownership) → clean. A HEAP
  payload (Option[String]) makes `x` ALIAS the Option's element (a `Dup`, container-grain) —
  the aliasing complexity, a follow-up. Gate the materialize+match to scalar payload; keep
  heap-payload Options on today's Opaque/linearize.
- ADVERSARIAL: this changes the corpus cert for Option[Int] fns (Opaque→materialized,
  linearize→execute). Ownership must stay 13139 (Alloc `i` both ways; match arms per-arm-
  balanced both ways) — verify + spawn refuters (a Some-arm binding a borrowed param must
  still REJECT).

## Machinery 2 — List-building — FLOOR DONE; List[Int] flowing; List[String] BLOCKED
`Init::DynList` + `prim.alloc_list(n)` + `prim.store64` landed (cert i, like DynStr). DONE
(List[Int], i64 value slots, fully sound + loop-bounded): **list.reverse / take / drop /
slice / range / repeat**. The string builders (slice/trim/reverse/replace/replace_first/
pad/trim_start/trim_end) similarly use prim.alloc_str.

**THE List[String] BLOCKER (critical for split/lines/map):** `Op::Drop` is a FLAT
`rc_dec` on the block — it does NOT recursively free heap ELEMENTS. So a function that
CREATES NEW strings and stores them in a list (string.split, string.lines, list.map's
result) LEAKS those strings on the list's drop → a loop OOMs → violates the loop-bounded
rule. (list.reverse/repeat over a List[String] are FINE — they only ALIAS existing element
handles, create no new strings, so nothing leaks.) **string.split etc. need NESTED-HEAP RC
first: a Drop that recursively rc_dec's a list's heap elements** — cert-touching (the cert
must track container-owns-elements, the value-semantics convention / Brick #54) and the
Drop/render must know the element type (List[Int] → no element drop; List[String] →
per-element drop). This is the next big machinery slice; until it lands, only List[Int]
construction and List[String] aliasing are admissible.
Count substrings first (2-pass) or grow. Cert-touching (nested owned heap) → adversarial.

## Machinery 3 — Closures (unblocks map/filter/fold/reduce/scan/find/each/sort_by …) — HARDEST
Self-host must INVOKE the closure `f` on each element (`f(x)` where f is a fn-value param).
**KEY FINDING (investigated): v1's wasm render has NO function table / `call_indirect` /
`elem` at all** — so this is a from-scratch build of the entire indirect-call surface, the
largest remaining slice. Current state: `IrExprKind::Lambda { params, body, lambda_id }`
lowers to `Alloc{Opaque}` (not a real function); a `Computed { callee }` call (`(g)(x)`)
is WALLED by `is_higher_order` (calls.rs:193) → deferred. Required pieces, in order:
1. **Lambda → a real MirFunction + a table slot.** Each lambda body becomes its own wasm
   `func` (lower its body like any fn); register it in a new wasm `(table funcref)` + `(elem)`
   indexed by `lambda_id`. The closure VALUE = the table index (a scalar i64) for a NON-
   capturing lambda; a capturing one also needs an env block (the `ClosureCreate`/`EnvLoad`
   path) carrying the captures, so the value = (table_idx, env_ptr) — do NON-capturing FIRST.
2. **`Computed`-call → `call_indirect`.** `f(x)` where `f` is a fn-value local lowers to
   `(call_indirect (type $sig) (args…) (local.get f))`. Needs a `(type)` per arity/sig.
3. **Un-wall higher-order + CAPS.** Remove the `is_higher_order` wall for the admitted case;
   the invoked closure's capabilities are UNKNOWN, so the calling fn must be tainted
   caps-UNVERIFIED (honest, like an elided call) — NOT claimed caps-safe. This keeps caps
   SOUND (`used ⊆ declared` can't be proven, so don't claim it) — verify caps stays ≥ 3582
   (the closure-using corpus fns were already caps-unverified, so no regression expected).
4. **Ownership/discipline:** the table/elem are STATIC module structure (not handwritten
   RUNTIME WAT) so the `handwritten_wasm_runtime_does_not_grow` baseline is untouched IF the
   lambda funcs + table render inline (no new `$rc_*`-style runtime helpers). The closure
   value (a scalar table_idx) carries no ownership; a capturing env IS a heap object (i/d).
5. **Then self-host list.map/filter** (alloc_list result + `f(elem)` per slot) — but a
   List[String] result still needs Machinery-2's nested-heap RC, so map over List[Int]→
   List[Int] lands first.
ADVERSARIAL: cert-touching (un-walls higher-order). The soundness rests on the caps taint
(never claim a closure call is caps-safe) — spawn refuters trying to get an undeclared
Stdout closure to pass caps. Multi-turn; implement the infrastructure (1+2) before any
stdlib fn, with corpus-wall caps ACCEPT verified at each step.

**THE SOUNDNESS CRUX (found while investigating — the one place an accept-but-unsafe hole
hides):** today a `Computed` call is ELIDED, so `count_ir_calls` sees ir>mir and TAINTS the
fn caps-UNVERIFIED (honest — closure caps unknown). If `call_indirect` is emitted as a
clean MIR op that maps 1:1 to the ir Computed node, that taint VANISHES and the fn becomes
caps-VERIFIED — but the closure may reach Stdout, so the caps witness would MISS it =
accept-but-unsafe = a hole in a PROVEN property (the trust spine's core). So `CallIndirect`
must NOT be caps-clean: either keep the ir>mir taint (don't count it 1:1) OR have
`cap_witness` treat a `CallIndirect` as using EVERY capability (conservative `used ⊇ all`),
so `used ⊆ declared` only holds for a fn that DECLARES all caps — i.e. a CallIndirect fn is
never silently caps-verified. corpus-wall `caps` must stay 3582 (the closure-using corpus
fns are already caps-unverified, so a correct impl keeps them so — a DROP below 3582 or a
spurious RISE both signal the taint is wrong). This caps taint is the single highest-risk
line of the whole campaign; implement + adversarially verify it FIRST, in isolation.

## Known writing-idiom gaps (use the workaround now; fix centrally later)
`if` as a BinOp OPERAND (`n + (if c …)`) and a scalar CALL as a call-ARG (`f(g(x))`) and a
Bool CALL as an if-COND (`if is_ws(b) …`) all DON'T lower — bind them first (`let t = …; …t`).
General lowering gaps (scalar-if-in-operand, scalar-call-in-arg/cond) worth a central fix.
