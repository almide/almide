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

## Machinery 2 — List-building (unblocks string.split, list.map result)
Build a `List[T]`: add `prim.alloc_list(n)` (n*8-byte slots, like alloc_str), set rc/len/cap,
`prim.store64` each element/handle. NESTED OWNERSHIP: a List[String] owns its substring Allocs
— the cert must track container-owns-elements (the value-semantics convention, Brick #54).
Count substrings first (2-pass) or grow. Cert-touching (nested owned heap) → adversarial.

## Machinery 3 — Closures (unblocks list.map/filter/fold) — HARDEST
Self-host must INVOKE the closure `f` on each element (`f(x)` where f is a fn-value param).
v1 must lower a call through a function VALUE (not a named callee). The hard frontier; design
last.

## Known writing-idiom gaps (use the workaround now; fix centrally later)
`if` as a BinOp OPERAND (`n + (if c …)`) and a scalar CALL as a call-ARG (`f(g(x))`) and a
Bool CALL as an if-COND (`if is_ws(b) …`) all DON'T lower — bind them first (`let t = …; …t`).
General lowering gaps (scalar-if-in-operand, scalar-call-in-arg/cond) worth a central fix.
