# result-family-from-type — layout is a function of the type, names only choose code

**Status**: active (phase 0 landed 2026-08-14; this doc is the arc's constitution)
**Driver**: #1406 and its four documented siblings — one bug class, five incidents.
**Survey evidence**: `../almide-references/RESEARCH.md` (8-compiler survey, file:line
citations; kept outside the repo).

## The disease

The MIR lowering decides a call result's physical layout ("family") from
`(module, func)` NAME tables:

- `is_self_host_result_module_fn` (scalar len-as-tag family, ~27 rows)
- `is_self_host_result_str_module_fn` (heap-Ok cap-as-tag family, ~37 rows)

Five incidents from this one class, all documented in-tree:

1. **#1406-D7** — classify sites see the PRE-routing name (`fan.any_map` covers
   nine pairings; `fan_any_call_name` suffixes at emit), so name rows for the
   routed forms never fire; String-output pairings mis-familied → wall.
2. **C-145** — mono-suffixed `or_else__…` missed every name-keyed decision until
   `base_stdlib_fn_name` existed.
3. **#1144** — a carrier name BEGINNING with `__` split to the empty string;
   every name-keyed decision silently missed; the whole fn walled.
4. **fs.copy/append mis-familied** — listed in the str family for a while; every
   err read back as ok (silent wrong-branch; mod_p5.rs:34-49 records it).
5. **Permanent point-wise maintenance** — every new stdlib pairing needs table
   rows; CLAUDE.md's "extend by matrix, never point-wise" rule is structurally
   violated by the tables' existence.

## The root cause

Layout is not a function of the type. ONE type — `Result[Unit, String]` — has
TWO physical layouts today:

- **len-as-tag** (@4 = 0/1, payload @12): ctor-built via
  `materialize_result_ok` — fs.copy, fs.append, fs.remove, fs.write_bytes,
  fs.write_bytes_raw.
- **cap-as-tag** (tag @16, payload @12, len@4=0 Ok convention): prim-render
  built ($write_text_file family) — fs.write, fs.mkdir_p, fs.rename,
  fs.remove_all.

Because the split is producer-identity, only a name can recover it — that is
WHY the name tables exist. Every other Result type is already consistent:
scalar-Ok (`Int`/`Float`) is always len-as-tag; heap-Ok (String/List/Value/…)
is always cap-as-tag.

## What the surveyed compilers do (one line each)

- rustc: return ABI = `arg_of(sig.output())`; intrinsic name tables (436 arms)
  choose instructions and are debug-asserted against the type-derived layout.
- Swift: one cached `TypeLowering*` per type key (no decl/name field); the one
  name-derived rule (ObjC selector families) folds into the function TYPE at
  import time.
- Zig: `firstParamSRet(cc, return_type, target)`; compiler_rt names are built
  FROM types — name is an output of the type decision, never an input.
- Roc: interned layout stored on every LIR local; the builtin name registry
  carries symbol + RC contract, explicitly never layout.
- Lean 4: type → closed 14-variant IRType; its ONE name-keyed result exception
  (3 Array fns) is duplicated across 3 passes with unenforced "Keep in sync"
  comments — the disease at N=3. Almide is at N=64.
- Koka: `cType :: Type -> CType`; the only name table covers parameter borrows;
  results are ALWAYS owned +1 (an ABI constant — nothing to track).
- Grain: uniform tagged word + a 1-bit Managed flag; freeing reads runtime tags.

**The law (8/8): names choose code; types choose layout.**

## The cure, in phases

### Phase 0 — tactical #1406 (LANDED with this doc)
`returns_foreign_result` can-err seed + `is_fan_any_map`+`is_heap_ok_result`
type-split at the classify sites + C-004 fixture. Proves the type-split shape
on the worst offender. The special case dissolves into the general rule in
Phase 2.

### Phase 1 — make `family = f(Ty)` TRUE: unify `Result[Unit, String]`
Route the ok(())/err(m) ctor rails for `Result[Unit, String]` through the
cap-as-tag materialization (the layout fs.write's prim family already uses:
Ok = len@4=0, @12=0, tag@16=0; Err = len@4=1, @12=msg, tag@16=1). Move the five
ctor-built names from the scalar table to the str table. Drop routing is
UNCHANGED (flat `DropListStr` is exact for both arms in this layout — the
fs.write precedent). After this phase the type→family map is total:

```
result_family(Result[T, E]) = HeapOk (cap-as-tag @16)  if T is heap or Unit
                              Scalar (len-as-tag @4)   otherwise (Int/Float/Bool)
```

Gate: full corpus (418 wasm_cross fixtures, 3-way), ABI probe diff, suites.

### Phase 2 — ONE family function, name tables carry no family
Introduce `result_family(ty: &Ty) -> ResultFamily` next to
`is_heap_ok_result` as THE single decision point. Merge the two name tables
into ONE set meaning only "this call's result is materialized" (union of both
tables). Rewrite the classify sites (bind: binds_p2_c.rs; subject:
tracked_calls.rs + control.rs; the control_p2_b.rs:30/73 heap-ok refinement
dance) to `materialized(name) && family(ty)`. Delete `is_fan_any_map` — the
general rule covers it. The fs.copy incident becomes UNREPRESENTABLE: no row
can put a type in the wrong family, because rows no longer carry family.

### Phase 3 — derive "materialized" from the registry (kill the hand table)
The remaining name set duplicates knowledge the self_host_registry + prim
floors already own. Derive it: a Module call materializes its Result iff the
ROUTED name (`list_heap_call_name` — the single router) links in
`self_host_registry` (ctor-rail layouts are canonical by construction) or is a
prim floor. Land an executable gate that walks every registry-linked
Result-returning fn and asserts the classify sites and `result_family` agree —
the matrix-gate discipline: a gated matrix cannot drift.

### Phase 4 — seed once (Stage-C direction) + close
One `seed_result_read_shape(dst, ty)` entry point used by every site that
tracks a materialized Result (bind / subject / prim), so the knowledge lives in
one function. File the full per-value-repr design (Roc's `Local { layout_idx }`
— makes "untracked" unrepresentable) as its own issue with this doc's survey
as evidence; it is gated on Phases 1–3 and NOT part of this arc. Update
ARCHITECTURE.md; move this doc to done/ with the measured deltas.

## Non-goals

- Physical unification of len-as-tag and cap-as-tag into one block shape
  (Phase 1 only removes the one COLLISION; scalar-Ok stays len-as-tag — that
  split is type-computable, hence harmless).
- Options / non-Result variants (same pattern, separate arc after this one).
- v0 leg changes (v0 rides its own lowering; the 3-way oracle guards it).
