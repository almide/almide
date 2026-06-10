# WASM Reference-Count Frees: the Ownership-Discipline Drain

**Status:** in progress (2026-06-07). Branch `perceus-belt-hard-error`, worktree
`xtarget-compound-display`. All work LOCAL/uncommitted pending the full two-gate
green.

## Mission

WASM is the memory frontier. Native Rust gets the borrow checker for free; the
WASM backend has historically **leaked all heap by design** — `compile_rc_inc` /
`compile_rc_dec` (`emit_wasm/runtime.rs`) were deliberate no-ops (`if ptr <
heap_start return`, reading the legacy `heap_start_global` field that stays `0`,
so the guard is always true). The whole Perceus belt has been verifying the RC
balance of NO-OPS. The goal: **activate frees** so WASM reclaims heap (churn
O(1), not O(n)) **without** introducing a single double-free, and keep
native↔WASM byte-identical.

This is the heart of "the trust layer for machine-written software": the same
program, run native or in a WASM sandbox, frees memory correctly and produces
identical bytes.

## The discovery

Flipping the rc guard to the correct `HEAP_START_GLOBAL_IDX` (=4) activates the
fully-built free-list machinery (alloc walk+reuse, rc_dec push). It immediately
exposed a **class of pre-existing latent double-frees** the no-op model had
masked: anywhere a heap value is *borrowed/aliased* or *moved into a container*,
the simplified Perceus inserts a scope-end `Dec` that frees a value something
else still owns. Activating frees turns each into a free-list cycle (silent hang)
or — with the **double-free sentinel guard** added to `rc_dec` (stamps a freed
block rc=0, traps `unreachable` on a second Dec) — a loud WASM trap caught by the
cross-target gate.

## Root cause: the simplified Perceus has no ownership/move discipline

Real Perceus (Koka) tracks *ownership*: a value is dup'd (Inc) when a new owner is
created, dropped (Dec) at an owner's last use, and **moved** (ownership
transferred, no separate drop) when consumed by a constructor/return. Almide's
WASM Perceus was a simplification: it Dec's *every* heap local at scope end and
Inc's *only* `Var`/`Clone`/`Deref` binds. That is wrong in two directions:

1. **Borrowed/aliased values bound to a Dec'd local under-count** — the local's
   scope-end Dec frees a value its source still holds (extraction aliases,
   env-loads, direct element accessors).
2. **Values moved into a container are over-Dec'd** — the container takes the
   value by reference (no copy/Inc), then Perceus Dec's the source local too,
   deep-freeing the container's contents.

Every bug in this drain is one facet of this single gap.

## The completion bar = BOTH gates (the key lesson)

`almide test spec/ --target wasm` (the test-block assertion corpus) is
**necessary but NOT sufficient**. Test blocks exercise value assertions but skip
whole shapes. The **`cargo test --release --test wasm_runtime_test`** gate (the
~100 `spec/wasm_cross/*.almd` `@contract` fixtures, byte-identical stdout/stderr/
exit) exercises shapes the test corpus never does — e.g. **compound set/map keys**
(`compound_eq.almd`, contract C-015). The real done-bar is:

- `almide test spec/ --target wasm` = clean (assertions), AND
- `cargo test --release --test wasm_runtime_test` = clean (byte-identical), AND
- churn benchmark O(1) (the leak gate — see below), AND
- native `almide test spec/` unregressed.

"240/0 on the test corpus" was an early false summit; the cargo gate found
`compound_eq` still diverging.

## Mechanisms implemented (each a facet of the one discipline)

All reuse one classifier — `yields_borrowed_alias(e)` in `pass_perceus.rs`
(exhaustive `match`, no wildcard = total; a new `IrExprKind` must be classified
deliberately). ALIAS forms acquire a reference; FRESH forms own theirs already.
The leak/crash asymmetry sets the safe default: a **missing** Inc on an alias =
double-free (crash); an **extra** Inc on a fresh value = leak (safe) — so unknown
forms default FRESH only where proven, else ALIAS.

| # | Mechanism | Where | Fixed |
|---|-----------|-------|-------|
| 1 | `yields_borrowed_alias` + **VDecl alias-Inc-after-bind** (subsumes old Var/Clone/Deref Rule-1; covers Member/Index/TupleIndex/MapAccess/OptionalChain, Match/If/Block tails, Unwrap/Try/ToOption/UnwrapOr peels) | `pass_perceus.rs` ChainHead::VDecl | hamming, default_fields, result_option_matrix |
| 2 | **`emit_stored_field`** — constructor/value-builder dup of alias args | value str/array/object (`calls_value.rs`), Record/List/Tuple (`collections.rs`), OptionSome/ResultOk/ResultErr (`expressions.rs`) | codec/variant cluster (11), capture_clone, protocol_advanced, try_parse_list |
| 3 | **fold accumulator dup** — `fold(xs,seed,f)` move-returns its seed; dup an alias seed | `calls_list_closure2.rs:1046,1155` | option.collect-empty, is_balanced-empty |
| 4 | **EnvLoad-borrow exclusion** — a closure body's `EnvLoad`-bound local borrows an env capture the env owns; exclude from scope-end Dec | `collect_heap_vdecls` | `>>` composition chains (compose_test) |
| 5 | **`is_alias_returning_runtime_call`** — `list.get_or`/`map.get_or` return the element pointer directly (alias) | `pass_perceus.rs` | protocol_extreme decode |

Mechanism #2/#3/#5 emit `call rt.rc_inc` **directly** (not an IR `RcInc` node).

## Verification status (2026-06-07)

- ✅ `almide test spec/ --target wasm`: **240 passed / 0 failed** (8 wasm:skip).
- ✅ native `almide test spec/`: **248/248**.
- ✅ churn (2M iters of {build Codec record + encode + value.stringify}): peak RSS
  **7.46 MB** = O(1), no leak.
- ❌ cargo `wasm_runtime_test` gate: **`compound_eq.almd` (C-015) diverges** —
  `set.contains(sr, {record})` returns F (should T) **and** a `rc_dec` sentinel
  trap at `main` exit. (Full diverging list pending the gate run.)

## Second layer: the COLLECTION RUNTIME (found by the cargo gate, 2026-06-08)

The expression/constructor/closure drain above is verified clean by BOTH the test
corpus AND a 2M-iter churn. But the `cargo wasm_runtime_test` gate (byte-identical,
the part `almide test` cannot reach) exposes a **second layer**: the WASM
collection runtime copies heap element/key/value POINTERS into new structures
without dup'ing them, so the source's scope-end Dec deep-frees what the new
structure now holds. A full streaming byte-sweep of the 100 fixtures (stdout-only;
`control_flow`/`cross_module_spread` were md5 false-positives = native-side
compiler *warnings* on stderr, not divergence) found **4 real divergences**, all
WASM `rc_dec`-sentinel traps / corruption under frees-on:

| Fixture | Op(s) | Sub-class | Status |
|---------|-------|-----------|--------|
| `compound_eq` (C-015) | `set.from_list`/`set.insert`/`map.from_list`/`map.insert` over record/tuple keys | heap-element/key RC | **set.from_list FIXED** (stdout now identical); exit still 134 (set.insert/map ops remain) |
| `list_float_total_order` | `list.sort_by` over `List[R]` | heap-element RC | open |
| `alias_cow` | `var b = a; a[0]=…` list COW | list-BACKING / COW RC | open (all values correct; double-free only at main-exit teardown) |
| `list_count_index_truncation` | `list.take/drop/slice` huge indices (2³²…) | NUMERIC (likely u32 index truncation → OOB → corruption), maybe NOT frees-related | open / triage |

### The fix tool + WHY a piecemeal drain CORRUPTS (attempted + REVERTED 2026-06-08)
`emit_elem_copy_owned(ty)` = `emit_elem_copy` plus, for a heap element, a
stack-neutral `rc_inc` (`i32_load; call rc_inc; i32_store`), at **SHARE points
only** (copy OUT of a still-live source), NOT intra-structure MOVE points (grow +
abandon → an extra Inc leaks). Applying it to `set.from_list` (`calls_set.rs`
append-from-xs) DID fix `compound_eq`'s stdout value (set.contains(record) F→T)
and kept the corpus at 240/0. **BUT** a `set.from_list` + `set.insert` churn loop
then revealed the trap of piecemeal collection fixes: native `sink=30` vs **wasm
`sink=31`** at 10 iters (one set's dedup length wrong), → **OOB at 1.5M iters**.
A set op has MULTIPLE share/move points; fixing some but not all leaves a residual
double-free that reuses freed memory → corrupts the element-equality the *next*
op's dedup depends on → wrong length → eventually OOB. **Lesson: the collection
drain must fix ALL of a structure's share/move points ATOMICALLY and gate on a
per-structure CHURN loop (1M+ iters), not just stdout/corpus.** The piecemeal
changes were **REVERTED** to the validated known-good (240/0 + records-churn
7.43 MB). The tool/approach is sound; the *increment* was wrong.

### The remaining collection drain (mechanical, but per-site share/move judgement)
~15-20 `emit_elem_copy`/`emit_elem_copy_sized` sites across `calls_set.rs` (lines
240,310,355,392,470,536,609 — insert/union/intersect/diff/…) and `calls_map.rs`
(the dict path: `emit_dict_put_entry` + `emit_elem_copy_sized` at 102/510/519 —
note `emit_elem_copy_sized` takes a SIZE not a Ty, so it needs heap-ness threaded
in or an owned variant keyed on `key_ty`/val-ty). Plus `list.sort_by`
(`calls_list_closure.rs:941`) and the slice/take/drop family. Each: classify the
copy as SHARE (source survives → owned) vs MOVE (source abandoned → plain), then
swap. The leak gate for these is **the cargo gate / a 2M-churn**, NOT the test
corpus — verify both after each batch.

### COW (`alias_cow`) is its own sub-problem
`var b = a` (list aliasing) + `a[0]=…` (copy-on-write) — all VALUES are already
byte-identical; only the main-exit teardown double-frees the COW'd backing. Lives
in `AliasCowPass` + the IndexAssign/var-alias RC, not the element-copy helper.
Separate, careful fix.

## Remaining work

1. **Compound set/map keys** (`compound_eq`, C-015). `set.from_list`/`map.from_list`
   move record/tuple elements into the structure by reference; the input list's
   scope-end Dec then deep-frees the moved-in elements (and dedup'd duplicates).
   Same move-discipline gap as #2, in the set/map runtime (`calls_set.rs`,
   `calls_map.rs`). Apply the dup-on-store / move-exclusion there.
2. **Run the full cargo gate** and drain every diverging fixture (the test corpus
   misses these shapes by construction).
3. **Verifier-invisibility (follow-up, load-bearing for the belt):** mechanisms
   #2/#3/#5 emit `rc_inc` inside the emitter, NOT as IR `RcInc` nodes, so the
   perceus-belt verifier **cannot see them** — the churn benchmark is currently
   their only leak gate. Extend the verifier to MODEL constructor/builder/HOF
   dups (treat `value_object`/`Record`/`fold`/… as dup'ing their alias args) so
   they are *certified*, not just runtime-checked. Until then the belt's
   "we certify RC balance" claim has a hole.
4. **Re-land** M1 (rc guard flip) + M2 (TCO flatten + `tco_managed_params`) +
   sentinel guard + mechanisms #1–#5 + the compound fix as ONE coherent commit,
   only after BOTH gates + churn are green.

## Trap log (environment)

- `almide test spec/ --target wasm` runs every test block on wasmtime; the cargo
  `wasm_runtime_test` gate is the byte-identical contract gate. **Neither alone is
  the bar — both are.**
- The sentinel guard converts double-free hangs → fast traps, so the corpus run
  terminates instead of spinning.
- **Orphan `rustc` hazard:** the cargo gate compiles each fixture's native side
  via `rustc`; a previous session left 3-day-old `rustc` orphans (PPID=1) spinning
  at 99% CPU on the shared build scratch `/tmp/claude-501/almide-build/`, starving
  the gate for 47h. Check `pgrep rustc` + kill stale PPID=1 rustc before timing the
  gate. A `perl -e 'alarm N'` wrapper around `cargo test` kills cargo but leaves
  the test binary detached/running — background the gate instead.


## 2026-06-10/11 — third campaign: FULL DRAIN under the flag

Branch `true-perceus`. The activation is now env-gated (`ALMIDE_WASM_FREES=1`
at emit) with the **entire quadruple bar green under the flag**:
native corpus 264/264 · wasm corpus (flag off) 264/264 · wasm corpus
(flag ON) 264/264 · cargo byte gate (flag ON) 67/67 · 2M-iteration record
churn correct + flat RSS (13.3 MB ≈ leak-mode baseline).

All 14 frees-ON divergences drained (mechanism → fix):
- record typed-drop glue sorted fields BY NAME vs declaration-order layout → de-sorted (2 lines)
- heap_restore now resets the free list; rc_inc/rc_dec gained dead-zone guards (ptr ≥ heap_ptr)
- double-free sentinel (rc=0 stamp → second dec traps) + rc_inc resurrection trap + absolute walk cap (1M steps)
- string.join len==1 returned the element pointer raw → gated inc
- tuple-payload variant ctor stores → emit_stored_field
- mechanism #6: return-alias dup in insert_rc_ops (callee-side owned returns); unwrap_or runtime calls classified alias
- **ordering fix**: a VDecl alias-Inc on a Block value now hoists INSIDE the block (after the tail bind, before temp decs) — the late Inc was resurrecting payloads freed by the temp's typed dec
- emitter-level ownership retired under frees (is_single_use_var → false): in-place list reuse / raw rc_dec double-owned blocks vs IR decs
- list family: 15 emit_elem_copy→owned + min/max + get-box + remove_at OOB inc + set/insert/push stored-field + sort post-build dup walk + filter post-loop dup + enumerate + update replaced-release; list.repeat per-slot inc (the for+concat-push → repeat rewrite exposed it)
- concat: call-site SHARE dup walk over the fresh result's elements
- map family: emit_elem_copy_sized(size, dup) + dict_recap dup Option<(K,V)> (set/merge=Some, grow=None) + remove survivor dup + update full-table dup with old-value capture-release and not-found input inc + map.map key inc
- json/Value: value_get / get_typed(get_string|get_array) / as_type(tag 4|5) interior-pointer incs; set_path kept-key / unchanged-pair / path-key / no-op cur_built / unchanged-elem incs; remove_path 7 no-op alias returns + survivor sites; set_path new_val via stored-field
- emit_elem_copy_owned gated on is_heap_type (Int32/UInt32/Float32 4-byte-scalar landmine defused)
- free-list size-sanity bound removed (false-positived on legit freed nodes; cap+sentinel+resurrection traps cover the classes)

REMAINING before default-ON: Stage C (TCO loop reclamation — M2 re-land; loops
currently leak per iteration, bounded), committed churn fixture family
(per-structure 1M+ gates), Stage D PIN (fs scratch by-construction) + C-042
unlock, flag-ON full bar ×3 consecutive, then default flip + C-041 revision +
new reclamation contract + perf suite (Stage G).
