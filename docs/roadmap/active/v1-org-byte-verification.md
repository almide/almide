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

## Fourth pass — the final two threads (2026-07-03)

- **almide-sqlite: verified.** Root causes: a hyphenated package name errored
  in `parse_toml` and the error was SILENTLY swallowed, dropping the
  `[native-deps]` rusqlite injection (opaque E0433 downstream — run.rs/build.rs
  now WARN); the native bindings had drifted from the runtime's `AlmideMap`
  Map repr. Fixed both; wrote `spec/sqlite_test.almd` — 10 tests covering the
  full API against `:memory:` + a file-persistence round-trip. Native host
  package (like porta).
- **almide-web: verified.** `runtime/headless.mjs` — a Node reference host
  implementing the whole import surface (virtual-DOM handle table, captured
  console, deterministic timer/fetch event queue) — runs `spec/host_app.almd`
  and byte-diffs against a pinned expected output (`spec/run_host_test.sh`).
  Every binding, the string-intern protocol, and both callback re-entries are
  exercised; `runtime/web.js`'s DOM TODOs got the real handle-table
  implementation with the same semantics.
- **Handoff step 4: DONE — read_message runs BYTE-IDENTICALLY on the VERIFIED
  render_program path** (stdin Content-Length framing → json.parse → record →
  response build → write_message, diffed against `almide run`). What it took:
  1. `stdlib/json_parse.almd` — a pure-Almide recursive-descent JSON parser
     transcribed clause-for-clause from the native oracle (char positions,
     \u surrogate pairs, lenient trailing separators, Int/Float split), plus
     `json_ctor.almd` (object/array/stringify delegations to value_core).
     Registered in the self-host registry; `json.parse` joined the
     str-result-module predicate so `let v = json.parse(s)!` and `match` both
     track.
  2. **A soundness hole closed**: a `match` over an UNTRACKED Result subject
     fell to the both-arms LINEARIZATION even when arms carried calls — BOTH
     println arms ran (silent miscompile). Call-bearing arms now WALL; pure
     module-call subjects (json.parse) are tracked like Named calls.
  3. **never-err strip fixed two ways**: `can_err` now sees `!` over MODULE
     calls (json.parse errs — parse_and_wrap was misclassified never-err),
     and the bind-position strip is gated to LIFTED (or self-TCO) callees —
     a declared-Result fn builds a REAL Result block, so stripping its `!`
     made consumers read record fields off the Result handle.
  4. Nested-variant payload tracking (`ok(m)` where m: Option[record]) through
     both the statement and value Result-match binds, so porta's
     `match m { some(req)/none }` branches on the tag.
  5. Discarded heap-result calls (`write_message(..)!` as a statement) now
     receive + scope-drop the returned block — a bare void call left it on
     the wasm stack (invalid wasm).
  Gates: corpus-wall TOTAL over 4556 fns; output-parity baseline 126 → **151**
  (+25, the JSON-codec cascade); almide-mir tests 501/6-known; spec 273/273.

## Fifth pass — the output-parity frontier (2026-07-03, same day)

Directive: keep rescuing. Attacked the render_program path's MISMATCH (silent
miscompile) and RUNERR (invalid wasm) classes head-on. **MISMATCH 6 → 0,
RUNERR 8 → 2, parity baseline 151 → 162.** What fell out:

- **Value-model formatting**: stringify's Float arm now renders Rust's raw `{}`
  (strip float.to_string's ".0"); float.parse keeps -0.0's sign (`-1.0 *` not
  `0.0 -`) and caps the pow10 scale at the f64-inf boundary with an
  ACCUMULATOR (tail) recursion — "1e99999" used to exhaust the call stack.
- **Unicode whitespace**: string.trim/trim_start/trim_end decode UTF-8 at the
  boundaries (the full char::is_whitespace 25-codepoint set); int.parse /
  float.parse route through it (C-021 on the MIR path).
- **json.parse rewritten over the prim byte floor** — the char-indexed
  first version was O(n²) (string.get + per-char concat) and timed out on a
  multi-KB glTF; now byte-addressed with a write-cursor string decoder and
  UTF-8 encode for \uXXXX (surrogate pairs included).
- **Lifted lambdas inherit variant_layouts + global_inits** — a custom-ADT
  match inside `list.filter((t) => match t …)` resolved against an EMPTY
  registry and filtered EVERYTHING out (closures_and_variants).
- **Heap-element combinator routing**: filter/get_or/unwrap_or over
  non-String heap payloads (variants, Values) routed to rc-sharing self-hosts
  (`list.filter_rc`, `list.get_or_value`, option/result value-unwrap_or
  variants) instead of the `_str` deep-copy that read a block's length word as
  a byte count (UAF garbage); unshareable combinators route to unregistered
  names = clean walls.
- **Scalar TCO admitted** for destructure-free tail self-recursion — the
  self-host byte-walkers (`__split_fill`, `__chunk_outer`) no longer exhaust
  the stack on large inputs; a tuple-destructure body declines (the loop
  rewrite mishandles it) and keeps real recursion.
- **`Try` joins `Unwrap` everywhere**: the monadic desugar, the never-err
  strip, and a new rewrap (never-err call assigned to an EXPLICIT
  `Result`-typed var re-wraps as `ok(call)`) — the effect_assign_unwrap
  matrix (assign/loop/index/annotated/unannotated) is fully green.
- **`_start` handles an explicit-Result main** (reads the tag, drops the Ok
  block, traps on Err) — every `fn main() -> Result[Unit, String]` CLI
  (porta, almide-grammar) used to emit invalid wasm ("values remaining").
- **CLI dispatch shape** (`match list.get(args,1) { some("cmd") => …, _ => usage }`)
  desugars to the executable two-arm form; bare Named calls resolve into their
  unique linked user module; `string.capitalize` self-hosted. almide-grammar
  now RENDERS + runs its dispatch matrix (output byte-diff still divergent —
  the module-record map leg is the open edge).
- top_let_test exposed **scalar module-globals lowered to Const-0** — const
  (call-free) initializers now materialize their real value, incl. transitive
  const globals (`SOLAR_MASS = 4.0 * PI * PI`); call-bearing inits wall.
- `fan.map` with an all-`ok` lambda rewrites to `list.map` (observably
  identical; fan lambdas cannot capture vars) and defunctionalizes.

Sixth pass (2026-07-03, same day): **almide-grammar wall=0 RESTORED and fully
byte-verified as a CLI** — all four generator modes byte-match `almide run`
under the same argv. Three layers: the Option-String literal dispatch desugar,
bare-Named resolution into the unique linked module, and module type layouts
ALIASED to their bare base names (a bare `Named` reference read a record with
NO layout — fields shifted silently; unique owners only, ambiguity stays
qualified = walls). Also established that v1's `env.args` ALREADY matches v0
(argv[0]-skipping — the earlier mismatch was `wasmtime -- args` passing the
literal `--` into the guest). `Map[Int, String]` / non-String-value heap maps
now WALL cleanly instead of linking the wrong-slot plain/`_str` variants
(map_set_eq: invalid wasm → honest wall).

Seventh pass (2026-07-03, same day): the RECORD-VALUE surface, systematically —
porta's protocol layer is now fully green (real walls 2 → 1; the remainder is
wasmtime-bridge-adjacent by nature). What landed: monadic `match` executes
inside heap-result if-arms (the tail-duplicated `let x = if c then f()! else []`
— resolve_env); EMPTY list literals of admitted classes materialize; STRUCTURAL
record-list literals route through the synthesized anon-record drop (plus the
missing `$__drop_list_anonrec_<hash>` wrapper generation) — this also opens the
`f([{…}])` argument position; `list.get/first/last` SHARE record elements
(elimination-keyed to the layout-identical `_value` accessor); a record `??`
default selects through the handle-level value helper. And the LAST runtime
RUNERR fell: the append-accumulator TCO now admits a PURE-call-wrapped growth
(`string.take(acc + "x", 8)` — tco_deep_recursion_churn's 2M-iteration spin
byte-matches). Parity: match=163, RUNERR 1 (load-flaky), MISMATCH 1 real
(float.parse exact rounding).

Still open on this frontier: the Map repr variants (`_ivh` scalar-key/heap-val,
`_hval` heap-val-non-String — the map_set_eq brick),
`tco_deep_recursion_churn` (a heap accumulator built THROUGH a call —
`string.take(acc + "x", 8)` — needs the general heap back-edge),
float.parse's exact decimal→f64 rounding at the denormal/max boundaries
(a strtod-class brick), the almide-grammar output divergence, and porta's 2
native-FFI walls.

## Sixth pass — org wall 101→0 arc, ceangal 77→21 (2026-07-14)

- **Tail spread/record ctors** (`{ ...n, gap_main: v }` / `text()` returned):
  tail.rs now routes SpreadRecord+Record through the arm-precedent builders;
  `lower_owned_heap_field` gained the nested-spread arm + a `value_or_global`
  Var arm (record globals like `_default`, scalar-only fallback included);
  spread bases may be a Member (`{ ...v._style, … }` borrows via
  `try_lower_heap_field_borrow`). ceangal 77→42.
- **Mutable module-level `var` brick** (scroll bucket + a LIVE miscompile):
  reads/assigns had silently materialized the init / rebound a local
  (`5 3 0` vs native `5 8 8`, probe-confirmed; `inline_pure_call_globals`
  also inlined mutable call-inits — writes invisible). Now each `var` gets a
  linear-memory slot at `MG_SLOT_BASE + 8i` (bump base shifts; N=0 modules
  byte-identical): read = `LoadHandle`+`Dup` (cert `a` = the render's rc_inc),
  assign = build-new → `$__mg_take` (CallFn-owned, cert-honest) → type-routed
  drop → store+Consume; `g[i] = v` = take → MakeUnique(COW) → elem store →
  store-back; synthesized `__mg_init` runs before `__global_init` (walls the
  program if an init can't lower). `__mg_take` excluded from the mir≤ir call
  count (a prim-class injected accessor). ceangal 42→21 (scroll 21→0);
  module_var_test + wasm_cross_pkg now run v1 (the corpus 0 is now HONEST).
- **Float export ABI**: `pub fn` exports with Float params/ret render through a
  `f64.reinterpret_i64` wrapper (`$__export_<fn>`) — the public ABI presents
  real f64s (v0 parity); non-Float params keep their real reprs (i32 heap).
- **Known v0-wasm divergence (recorded, unfixed)**: `let snapshot = g` then
  `g[i] = v` — v0-wasm aliases (snapshot sees the write), native clones
  (`9 9` vs `9 2.5`). v1's COW matches NATIVE; v0 retirement (Phase 3)
  obsoletes the divergent path.
- **Borrow-chain follow-ups (same day)**: `node.children[item.idx]` (index
  whose list is a FIELD of a materialized aggregate) borrows through
  `try_lower_heap_field_borrow`; `line.items[ii].idx` (scalar read off an
  INDEXED element) resolves via an IndexAccess container arm. Together they
  opened the whole Yoga layout core (compute_size / do_layout / resolve_line_
  flex / layout_line / diff_tree) — ceangal 21→14. Fixtures:
  `spec/lang/member_index_borrow_test.almd`.
- **ceangal residual 14 — ALL cross-module-blocked (the per-file classify
  floor)**: read each wall's actual code — every one depends on cross-module
  state the per-file classify cannot resolve (`lay.MARGIN_AUTO` record-field
  globals, `render.list_frame_height` cross-module mutable globals in the
  both-arms-if conditions, `td.todos.get()` through the generic Cell API,
  `v.*`/`lay.*` sibling calls in view_to_node/theme). The per-file wall
  metric for ceangal has BOTTOMED at the cross-module boundary; further
  ceangal progress routes through (a) sibling-resolved classification
  tooling, (b) the generic-module-fn mono/link fix, (c) cross-module mutable
  global bridging in the v1 pipeline (the slot map is same-region today).
- **PRE-EXISTING checker bug (blocks ceangal's own suite, NOT this arc's
  regression)**: `almide test` on ceangal fails the #433 name-pinning
  postcondition (`Node` in build_lines/resolve_line_flex/layout_line/do_layout
  should pin `layout.Node`) — reproduced identically on released v0.28.6.
  Single-file / two-module minimal probes do NOT reproduce; needs ceangal's
  real module graph. Separate checker workstream.
- **PRE-EXISTING build blocker (ceangal, also on v0.28.6)**: `almide build
  src/mod.almd --target wasm` fails IR verify — `call to unknown function
  'cell.get'` (todoapp): the GENERIC module fn (`fn get[T](c: Cell[T])`)
  called via cross-module UFCS never monomorphizes/links. Frontend (mono +
  ir_link) workstream.
- **homullus/almai frontier = the WASI-env runtime brick**: their dominant
  walls (`memory_path`/`memory_dir`/`sessions_dir`/`require_env` …) are
  `match env.get(..)` tails — v1 has no `env.get` lowering, AND v0-wasm ICEs
  outright (`[ICE] emit_wasm: no WASM dispatch for env.get`,
  calls_env.rs:63 — a PANIC, not a wall, on released binaries too). Opening
  this class = env via WASI `environ_get` on BOTH engines (+ a C-contract for
  env observability), then the match-tail machinery applies as-is. The
  non-env homullus walls (value_to_tool_call/value_to_message Record/Value
  shapes) are ordinary lowering bricks.
  **SHIPPED (same day): the WASI-env brick, both engines.** (1) v0-wasm
  `emit_env_call("get")` — environ scan mirroring `emit_wasi_argv_list`; the
  some() is a BOXED cell holding the value String (`unwrap_or`/match load
  `*opt` — the v0-wasm Option convention; the unboxed first draft matched
  but printed empty). (2) The runner + all three test-gate wasmtime spawns
  pass `-S inherit-env=y` (without it every guest lookup was none — the
  silent-divergence trap the design predicted). (3) v1: `PrimKind::EnvGet`
  (Capability::CliArgs — the Env profile's canonical cap, no new id), the
  `$env_get` preamble WAT (4-ALIGNED out-pointers — `$alloc` guarantees no
  alignment, wasmtime traps otherwise — and a ONE-TIME environ snapshot in
  `$env_envp`/`$env_cnt` globals: the per-call re-read leaked its scratch,
  the 120k leak-loop hung), the `env_get.almd` self-host + `prim.env_get`
  declaration, cert `i`-alloc + kernel-verified. (4) C-133 (ALS-R5) +
  `spec/wasm_cross/env_get.almd` (243 equal). almai 7→0, homullus 9→6,
  RATCHET 0 (the fixture itself lowers through v1).

- **homullus residual 6 = external-type artifacts + the glob path-model gap**:
  `ok(Resp{...})` tails lower fine single-module (probe-verified) — the walls
  come from `almai.LLMResponse`/`almai.ToolCall` being an EXTERNAL package's
  types the per-file classify cannot resolve (the same artifact class as
  ceangal's cross-module residue). The one REAL gap: `fs.glob` — native
  matches patterns against ABSOLUTE canonicalized host paths (runtime/rs
  fs.rs `glob_recursive`), which the wasm preopen path model cannot
  reproduce byte-identically; v0-wasm ALSO has no glob dispatch (the same
  latent-ICE class env.get was). Opening it needs a PATH-MODEL contract
  first — out of the byte-parity subset by design until then.
- **Measurement is now the org-arc bottleneck**: per-file classify has
  bottomed on artifacts across ceangal/homullus. The next high-leverage
  brick is sibling/package-RESOLVED classification (classify with the same
  import resolution render_program uses), so org numbers count only real
  lowering gaps.
- **RESOLVED-strict measurement (same day, via render_program — it already
  runs the canonical driver discovery)**: **homullus = 0 walls. ceangal = 8**
  (vs 14 per-file): theme 3 — `let gray_50 = v.rgb(…)` MODULE→MODULE global
  refs still UNBOUND even resolved (the cross-module top-let bridge misses
  call-init globals referenced from a SIBLING module — a real pipeline gap);
  scroll 2 + layout 1 — STRICT-mode scalar-binding/for-in-var value-subset
  walls (permissive classify hid them); render 2 — view_to_node/
  zip_view_rects heap-result-if (real, now measured resolved). The true org
  frontier ≈ ceangal 8 + porta 2 + singles. org-trust-status.sh should adopt
  the resolved sweep as the headline number.
  **The 8 decompose into exactly THREE next bricks**: (1) theme 3 = the
  MODULE→MODULE top-let bridge (`bridge_cross_module_toplets` only maps
  main-side refs — `ir.var_table` with `module_origin`; a sibling module's
  refs live in ITS `m.var_table` and never bridge; per-module region maps
  needed, mind the VarId-collision hazard the shared union tolerates).
  (2) scroll/layout 3 = the STRICT scalar-binding value subset (what shapes
  their bindings actually are is the first probe of that brick). (3) render
  2 = the arm-BLOCK tail-helper lift: zip_view_rects' else arm is a full
  block (global assigns + for-loops + accumulators) returning a record —
  the heap-result-if ARM machinery lowers arm VALUES; the 1792e5d7 dense-
  chain lift must generalize to lift such arm blocks into helper fns.

## Remaining threads

- **wall=0 count 21 → 19 (honesty, not regression)**: the linearization guard
  and the never-err-strip fix surfaced walls in porta (2, native-FFI move-out
  class) and almide-grammar (1, a call-bearing arm over an untracked subject)
  that previously lowered into silently-wrong code paths (both arms running /
  a Result block read as its payload). Both repos' own suites stay green /
  byte-verified on the production (emit_wasm) path; supporting those shapes
  faithfully on the MIR path is the next brick.
- output-parity full-run flakiness: 2–3 baseline files drop only under the
  full-gate machine load and byte-match solo (pre-existing, recorded).
- The 6 almide-mir record-materialization DEBUG tests (another workstream).
