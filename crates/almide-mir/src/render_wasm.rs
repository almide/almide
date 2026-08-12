//! MIR → wasm renderer (the SOVEREIGN target, §1 — wasm is the canonical v1
//! artifact; the Rust renderer is the secondary qualification path).
//!
//! Like the Rust renderer it TRANSLATES the MIR decision and never re-decides
//! it (§3.2). It emits WebAssembly text (WAT, run directly by wasmtime). For the
//! value-semantics subset it uses the SAME copy idiom as the Rust renderer —
//! eager copy-on-`Dup` (a list literal is a heap block; `Dup` copies it) — so
//! the two targets are byte-identical by construction WITHOUT needing SHARING
//! here: no aliasing ⇒ no `rc_inc`, and `MakeUnique` is a no-op (the copy already
//! made the handle unique). What it DOES realize (A1.1b) is the RELEASE: a `Drop`
//! emits `call $rc_dec`, decrementing the refcount cell to 0 — so the binary
//! actually frees at the cell level (`RuntimeModel.balanced_cert_frees_in_memory`)
//! and an already-released cell traps (the double-free sentinel). The remaining
//! RC slices are SHARING (`Dup → rc_inc` + cow, A1.3, for memory efficiency) and
//! PHYSICAL reclamation (a free-list so freed bytes are reused, A1.2); neither is
//! a SAFETY gap (the cell-level frees + sentinel are the safety realization).
//!
//! Heap list layout in linear memory:
//! `[rc: i32 @0][len: i32 @4][cap: i32 @8][data: i64 @12…]`. The `rc` cell at
//! offset 0 is the PHYSICAL realization of `proofs/RuntimeModel.v`'s refcount
//! cell (`read_rc m base` at `RC_OFFSET = 0`): the model that proves leak-freedom
//! now has a concrete byte home. It is initialized to 1 at allocation; the
//! release path that decrements it (`Drop → call $rc_dec`) is the NEXT brick —
//! today the renderer is still eager-copy/Dec-free (no `rc_dec` emitted), so the
//! `eager_copy_refines_safety` safety regime is fully preserved.
//!
//! ⚠ BOOTSTRAP SHORTCUT — DO NOT GROW (see §4.1 of the architecture doc). The
//! hand-written WAT runtime below (`$list_copy` and kin) is the EXACT trap that
//! made v0's wasm emitter a nightmare (a large hand-written wasm surface
//! dual-maintained with native). It exists only to prove the dual-renderer path
//! RUNS — the `$print_int`/`$print_list`/`$list_push`/`$itoa_append`/`$list_len`
//! bootstrap cluster was deleted once real programs printed through the
//! self-hosted runtime instead (#1208). The ideal form shrinks the hand-written
//! wasm to a tiny, total, decision-free, spec-provable MIR-PRIMITIVE mapping,
//! with all of list/string/format in Almide compiled through this same path.
//! Convergence rule: never add another hand-written WAT runtime routine — with
//! ONE principled exception, the proven MEMORY-MODEL primitives (`RC_PRIMITIVE_FNS`,
//! the realization of `RuntimeModel.v`'s `rt_inc`/`rt_dec`). They are a CLOSED set
//! bounded by the PROOF, not by hand-discipline, so they are accounted SEPARATELY
//! from the open-stdlib ratchet the rule guards (the trust spine's own core, not
//! "another stdlib routine"). The ratchet on the open surface stays as strict.

use crate::{
    CallArg, FBinOp, FCmpOp, FUnOp, Init, IntOp, MirFunction, MirProgram, Op, PrimKind, Repr, RtFn,
    ValueId,
};
use std::collections::{BTreeMap, BTreeSet};

// Fixed low-memory addresses (named — no raw literals in the emitted WAT logic).
const NWRITTEN_ADDR: u32 = 0; // i32 scratch for fd_write's bytes-written out-param
const IOVEC_ADDR: u32 = 8; // [buf: i32][len: i32]
                           // The fs.read_text path_open error message — a CONST byte run in the data section
                           // (the `$read_text_file` Err arm copies it into a canonical String).
const RTF_NOTFOUND_ADDR: u32 = 64; // "file not found" message bytes
const RTF_NOTFOUND_LEN: u32 = 14; // len of "file not found"
const RDIR_ERR_ADDR: u32 = 80; // "directory not found" message bytes (fs.list_dir Err)
const RDIR_ERR_LEN: u32 = 19; // len of "directory not found"
const WRITE_ERR_ADDR: u32 = 100; // "write failed" message bytes (fs.write Err) — 100..112
const WRITE_ERR_LEN: u32 = 12; // len of "write failed"
const MKDIR_ERR_ADDR: u32 = 112; // "mkdir failed" message bytes (fs.mkdir_p Err) — 112..124
const MKDIR_ERR_LEN: u32 = 12; // len of "mkdir failed"
const REMOVE_ERR_ADDR: u32 = 124; // "remove failed" message bytes (fs.remove_all Err) — 124..137
const REMOVE_ERR_LEN: u32 = 13; // len of "remove failed"
const DIVZERO_MSG_ADDR: u32 = 144; // "Error: division by zero\n" — 144..169 (__div_trap)
                                   // The explicit-Result main Err protocol ($__main_err) REUSES the div-zero line's bytes:
                                   // its first 7 bytes are "Error: " and its byte 23 is the trailing "\n" — no new data.
const MAIN_ERR_PREFIX_LEN: u32 = 7; // "Error: "
const MAIN_ERR_NL_ADDR: u32 = DIVZERO_MSG_ADDR + 23; // the div-zero line's "\n"
const OVERFLOW_MSG_ADDR: u32 = 176; // "Error: integer overflow\n" — 176..200 (__div_trap)
const BOUNDS_MSG_ADDR: u32 = 208; // "Error: index out of bounds\n" — 208..235 (__div_trap)
const OOM_MSG_ADDR: u32 = 376; // "Error: out of memory\n" — 376..397 ($oom, C-197)
                               // fs errno → native std::io Display strings (240..376, FIXED): path_open errors
                               // map to the EXACT message native std::fs emits, so `err(e)` observes byte-identical
                               // text (C-042 kin).
const FS_ERR_NOENT_ADDR: u32 = 240; // "No such file or directory (os error 2)" — WASI NOENT(44)
const FS_ERR_NOENT_LEN: u32 = 38;
const FS_ERR_ACCES_ADDR: u32 = 280; // "Permission denied (os error 13)" — WASI ACCES(2)
const FS_ERR_ACCES_LEN: u32 = 31;
const FS_ERR_NOTDIR_ADDR: u32 = 312; // "Not a directory (os error 20)" — WASI NOTDIR(54)
const FS_ERR_NOTDIR_LEN: u32 = 29;
const FS_ERR_ISDIR_ADDR: u32 = 344; // "Is a directory (os error 21)" — WASI ISDIR(31)
const FS_ERR_ISDIR_LEN: u32 = 28;
// The bump allocator's DEFAULT start — also the mutable-global slot region's base
// (`crate::MG_SLOT_BASE`, one authoritative value): a program with N mutable
// module-level `var`s shifts its allocator base to `HEAP_BASE + 8*N` so the slots
// are never allocated over (N = 0 keeps every existing module byte-identical).
const HEAP_BASE: u32 = crate::MG_SLOT_BASE;
// The Ok/Err tag of a cap-as-tag `Result[String, String]` lives in the HIGH 32 bits of
// the 1-slot block's element (@16) — the `materialize_result_str` layout `$read_text_file`
// reproduces so the caller's match/`!`/DropListStr reads it identically.
const RTF_TAG_OFFSET: u32 = LIST_HEADER + I32_SIZE; // @16 = the slot's high half

// Field sizes / offsets (derived so the relationships show — no bare literals).
// list = [rc:i32 @0][len:i32 @4][cap:i32 @8][data:i64 @12…].
const I32_SIZE: u32 = 4; // a wasm i32 field is 4 bytes
const LIST_RC_OFFSET: u32 = 0; // the refcount cell — RuntimeModel.v's RC_OFFSET = 0
const LIST_LEN_OFFSET: u32 = LIST_RC_OFFSET + I32_SIZE;
const LIST_CAP_OFFSET: u32 = LIST_LEN_OFFSET + I32_SIZE;
pub(crate) const LIST_HEADER: u32 = LIST_CAP_OFFSET + I32_SIZE; // rc + len + cap
pub(crate) const ELEM_SIZE: u32 = 8; // i64 elements
                                     // A freshly allocated heap block has exactly one owner — the `Alloc`'s +1, the
                                     // initial value of the cell RuntimeModel.v's `exec` starts the fold from.
const RC_INITIAL: i32 = 1;
const PUSH_HEADROOM: u32 = 8; // spare cap so demo pushes never realloc
const IOVEC_LEN_OFFSET: u32 = I32_SIZE; // iovec = [buf:i32 @0][len:i32 @4]

/// ASCII bytes the fs path logic writes. (The itoa/print-list formatter bytes
/// — zero/equals/comma/newline/minus, the digit scratch, and the line-buffer
/// bound — went out with the #1208 prelude deletion: printing is self-hosted.)
const ASCII_SLASH: u32 = 47; // '/' — stripped from an absolute fs.read_text path

/// Render a MIR function to a runnable WAT module string.
pub fn render_wasm(func: &MirFunction) -> String {
    let heap_locals = heap_handle_locals(&func.ops);
    let locals_decl = heap_locals
        .iter()
        .map(|v| format!("(local {} i32)", local(*v)))
        .collect::<Vec<_>>()
        .join(" ");

    let mut body = String::new();
    // Single-function render (test entry): no module table, so FuncRef has no slots
    // and no other function exists to elide-call (empty param_counts ⇒ this path is
    // byte-identical to before).
    let no_slots: BTreeMap<String, u32> = BTreeMap::new();
    let no_param_counts: BTreeMap<String, usize> = BTreeMap::new();
    let reprs = value_reprs_wasm(func);
    // Legacy single-function render: no typed scalar locals and no tree
    // fusion here (empty classification + a fresh Fuser per op keeps this
    // path byte-identical to before).
    let no_floats: BTreeSet<ValueId> = BTreeSet::new();
    let mut no_fuser = Fuser::new();
    for op in &func.ops {
        body.push_str(&render_op(
            op,
            crate::render_wasm::OpTables {
                func_slots: &no_slots,
                param_counts: &no_param_counts,
                masks: &func.heap_slot_masks,
                reprs: &reprs,
                floats: &no_floats,
                tail_call: false,
            },
            &mut no_fuser,
        ));
    }

    format!(
        "{preamble}  (func $main {locals}\n{body}  )\n  (func (export \"_start\") (call $main))\n)\n",
        preamble = preamble(),
        locals = locals_decl,
        body = body,
    )
}

/// The fixed-runtime (preamble) wasm functions a `CallFn` could legitimately name even
/// though they are not `MirFunction`s. In practice no `Op::CallFn` targets these — the
/// runtime helpers are reached via `Op::Call`/`RtFn` (`render_call`) or `Op::Prim`, never
/// by raw name — but they belong to the resolvable set so a (hypothetical) user function
/// or marker that happens to share one of these names is never falsely walled. Derived
/// from the preamble text so it stays in sync with `preamble()` by construction.
fn preamble_func_names() -> BTreeSet<String> {
    let pre = preamble();
    let mut names = BTreeSet::new();
    // Match `(func $name` occurrences; the preamble declares each runtime fn this way.
    for seg in pre.split("(func $").skip(1) {
        let end = seg
            .find(|c: char| !(c.is_alphanumeric() || c == '_'))
            .unwrap_or(seg.len());
        names.insert(seg[..end].to_string());
    }
    names
}

/// The set of wasm function names a rendered module DEFINES (so a `(call $name)` resolves):
/// every `MirFunction` in the program (user-defined + auto-linked self-host + `print_str`)
/// plus the fixed preamble runtime functions. This is the AUTHORITATIVE resolution set —
/// `func_slots` is exactly the program-function half of it.
fn resolvable_call_names(prog: &MirProgram) -> BTreeSet<String> {
    let mut names: BTreeSet<String> = prog.functions.iter().map(|f| f.name.clone()).collect();
    names.extend(preamble_func_names());
    // The mutable-global slot take-accessor: emitted for programs with global slots
    // AND for local SHARED-CELL assigns (cells.rs), which name it over a cell-slot
    // address — mirror the `mg_helpers` emission condition exactly, so the name
    // resolves iff the definition is rendered.
    let uses_mg_take = prog.functions.iter().any(|f| {
        f.ops
            .iter()
            .any(|o| matches!(o, Op::CallFn { name, .. } if name == "__mg_take"))
    });
    if prog.mutable_global_count > 0 || uses_mg_take {
        names.insert("__mg_take".to_string());
    }
    names
}

/// The names of `Op::CallFn` targets that resolve to NOTHING — neither a `MirFunction` in
/// the program nor a preamble runtime function. Each such name, if rendered, would emit a
/// `(call $name)` to an undefined function ⇒ an INVALID wasm module (wasmtime/wat2wasm
/// reject it with "undefined function"). The resolution point where a call name maps to a
/// wasm `$func` is `render_op`'s `Op::CallFn` arm; this is that same lookup, lifted to a
/// pre-render check so it can return a clean reject instead of emitting the dangling call.
///
/// `prim.*` intrinsics never reach here (they are intercepted in lowering → `Op::Prim`);
/// `Op::Call`/`RtFn` runtime calls and `Op::CallIndirect` table dispatch are resolved by
/// their own render arms, not by raw name, so they are out of scope by construction.
pub fn unlinked_call_names(prog: &MirProgram) -> BTreeSet<String> {
    let resolvable = resolvable_call_names(prog);
    let mut missing = BTreeSet::new();
    for f in &prog.functions {
        for op in &f.ops {
            let name = match op {
                Op::CallFn { name, .. } => name.clone(),
                // Generated-drop targets render as literal `(call $__drop_…)`
                // text, not `CallFn` ops — an unresolved one used to sail past
                // this gate and produce an INVALID module instead of a wall
                // (the #881 bare-vs-qualified record-drop mismatch).
                Op::DropVariant { ty, .. } => drop_target_name(ty),
                Op::DropWrapperRec { drop_fn, .. } => drop_target_name(drop_fn),
                _ => continue,
            };
            if !resolvable.contains(&name) {
                crate::trace::trace("ALMIDE_DBG_UNLINKED", || {
                    format!("[unlinked] {} references {}", f.name, name)
                });
                missing.insert(name);
            }
        }
    }
    missing
}

/// The `$__drop_<…>` symbol a drop op's type/fn string renders as (the same
/// dot-sanitization the WAT arms apply).
fn drop_target_name(ty: &str) -> String {
    format!("__drop_{}", ty.replace('.', "_"))
}

/// Resolve an unresolvable generated-drop target to its UNIQUE
/// module-qualified twin. Intra-module code reaches a record type by its BARE
/// name (`View`) while the drop generator keys the QUALIFIED one
/// (`palette.View` → `__drop_palette_View`); without this remap the render
/// emitted a dangling `(call $__drop_View)` — an invalid module, not a wall
/// (#881). An ambiguous bare name (two modules declaring `View`) stays
/// unresolved and walls through `unlinked_call_names` instead of guessing.
fn resolve_drop_alias(target: &str, resolvable: &BTreeSet<String>) -> Option<String> {
    // A candidate twin must keep the SAME family chain (`list_`, `opt_`, …)
    // as the target and differ only by a module qualifier before the type's
    // final segment: `__drop_View` → `__drop_palette_View`,
    // `__drop_list_View` → `__drop_list_palette_View`. A family-crossing
    // match would free a DIFFERENT layout, so the qualifier segment must not
    // itself look like a family marker.
    const FAMILY_MARKERS: &[&str] = &["list_", "opt_", "tup_", "map_", "res", "closure", "value"];
    let t = target.strip_prefix("__drop_")?;
    let bare = t.rsplit('_').next()?;
    if bare.is_empty() {
        return None;
    }
    let fam = &t[..t.len() - bare.len()];
    let suffix = format!("_{bare}");
    let mut hit: Option<&String> = None;
    for cand in resolvable {
        let Some(c) = cand.strip_prefix("__drop_") else {
            continue;
        };
        if c == t {
            continue;
        }
        let Some(rest) = c.strip_prefix(fam) else {
            continue;
        };
        let Some(middle) = rest.strip_suffix(suffix.as_str()) else {
            continue;
        };
        if middle.is_empty() || FAMILY_MARKERS.iter().any(|m| middle.starts_with(m)) {
            continue;
        }
        if hit.is_some() {
            return None;
        }
        hit = Some(cand);
    }
    hit.and_then(|c| c.strip_prefix("__drop_"))
        .map(str::to_string)
}

/// Render a whole MIR program to a WAT module, WALLING any unlinked stdlib/runtime call.
///
/// This is the SOUND, conservative entrypoint: if any `Op::CallFn` names a function that
/// is neither defined in the program (user / auto-linked self-host / `print_str`) nor a
/// preamble runtime function, the module would reference an undefined `$func` (invalid
/// wasm). Rather than emit that dangling call (which passed silently as `Ok` before), this
/// returns [`LowerError::Unsupported`] — a loud, conservative REJECT.
///
/// SOUNDNESS: walling only REMOVES a would-be-emitted module (it never adds a call op), so
/// the MIR call count the corpus gate sees can only DROP — `mir_calls <= ir_calls` is
/// preserved, and caps-verified cannot regress (a walled function is cleanly excluded, not
/// mis-counted). It is strictly more conservative: it can never create a false-green.
/// The deriver burns the SINGLE-mangled cross-module derived-codec name
/// (`almide_rt_varlib_Pigment_encode`) into field-encode call sites, while the
/// DEFINITION carries the DOUBLE mangle (`almide_rt_varlib_varlib_Pigment_encode`
/// — module prefix + qualified type name, observed in the linked IR). Resolve the
/// alias at the render boundary: the burned name is undefined, but re-inserting
/// the module segment hits the defined fn. A module name containing `_` fails the
/// split and simply keeps the conservative wall.
fn resolve_rt_alias(name: &str, resolvable: &BTreeSet<String>) -> Option<String> {
    let rest = name.strip_prefix("almide_rt_")?;
    let (m, _) = rest.split_once('_')?;
    let cand = format!("almide_rt_{m}_{rest}");
    resolvable.contains(&cand).then_some(cand)
}

pub fn try_render_wasm_program(prog: &MirProgram) -> Result<String, crate::lower::LowerError> {
    // Remap aliasable burned names BEFORE the unlinked check (clone only when an
    // alias actually applies — the common path stays zero-copy).
    let resolvable = resolvable_call_names(prog);
    let remapped;
    let prog = if any_call_needs_alias(prog, &resolvable) {
        let mut p = prog.clone();
        remap_burned_names_to_aliases(&mut p, &resolvable);
        remapped = p;
        &remapped
    } else {
        prog
    };
    // Region-specialized allocation (region_alloc.rs, issue #838): rewrite
    // qualifying `consume(produce(...))` windows to bump regions and append
    // the `__rgn_` clones. BEFORE the prune, whose rendered-text reachability
    // scan is what keeps the clones alive.
    let mut regioned = prog.clone();
    crate::region_alloc::apply_region_specialization(&mut regioned);
    let prog = &regioned;

    // Dead-function elimination (#782, generalized): ALWAYS prune to exactly
    // what's reachable from main/exports — not just when a broken call forces
    // the issue. A dead function that happened to be well-formed used to ride
    // along into every module (variant.almd's 3-arm match linked 83 functions,
    // most never on any path from `main`); now only the reachable subset ever
    // reaches the renderer. Whatever `unlinked_call_names` still finds after
    // pruning is, by construction, on a REAL path from main — a genuine wall,
    // never a dangling `(call $…)` for a function nothing would have run.
    let pruned = prune_unreachable_functions(prog);
    let missing = unlinked_call_names(&pruned);
    if !missing.is_empty() {
        let names = missing.into_iter().collect::<Vec<_>>().join(", ");
        return Err(crate::lower::LowerError::Unsupported(format!(
            "unlinked stdlib/runtime call(s) with no wasm definition: {names} — \
             rendering them would emit a dangling `(call $…)` (invalid wasm). \
             Add the callee to the self-host registry or wall the using function."
        )));
    }
    Ok(render_wasm_program(&pruned))
}

/// Whether ANY op names a burned callee/drop target that is unresolvable as spelled but
/// DOES resolve through an alias — the zero-copy test that decides whether the program
/// must be cloned for the remap at all. Extracted verbatim from
/// [`try_render_wasm_program`] (codopsy round-3 sweep, #852).
fn any_call_needs_alias(prog: &MirProgram, resolvable: &BTreeSet<String>) -> bool {
    prog.functions
        .iter()
        .flat_map(|f| f.ops.iter())
        .any(|op| match op {
            Op::CallFn { name, .. } => {
                !resolvable.contains(name) && resolve_rt_alias(name, &resolvable).is_some()
            }
            Op::DropVariant { ty, .. } => {
                !resolvable.contains(&drop_target_name(ty))
                    && resolve_drop_alias(&drop_target_name(ty), &resolvable).is_some()
            }
            Op::DropWrapperRec { drop_fn, .. } => {
                !resolvable.contains(&drop_target_name(drop_fn))
                    && resolve_drop_alias(&drop_target_name(drop_fn), &resolvable).is_some()
            }
            _ => false,
        })
}

/// Render a whole MIR program (functions + `_start` → `main`) to a WAT module.
///
/// This is the raw renderer used by the existing test corpus, which always feeds it
/// fully-linked programs. Callers that may receive an UNLINKED call (the production
/// `render_program` path, the corpus-wall harness) must go through
/// [`try_render_wasm_program`], which walls the dangling-call case cleanly. As a
/// defensive backstop this raw renderer still asserts linkage and panics loudly rather
/// than silently emitting invalid wasm — a regression here is a bug, not a quiet miscompile.
pub fn render_wasm_program(prog: &MirProgram) -> String {
    debug_assert!(
        unlinked_call_names(prog).is_empty(),
        "render_wasm_program fed an unlinked call (use try_render_wasm_program to wall it): {:?}",
        unlinked_call_names(prog)
    );
    // (The module-level print-label data section died with `RtFn::PrintList`,
    // #1208 — no op carries a `CallArg::Label` any more.)
    // Function-table slots by NAME (position in the module) — a FuncRef resolves its
    // referenced function to this index, the same index the `(elem)` table uses.
    let func_slots: BTreeMap<String, u32> = prog
        .functions
        .iter()
        .enumerate()
        .map(|(i, f)| (f.name.clone(), i as u32))
        .collect();
    // Function arity by NAME — a real call always supplies its callee's params,
    // so a caps-accounting elided-call MARKER (an `Op::CallFn` with no dst/args/
    // result NAMING a param-taking function) is distinguishable from a genuine
    // 0-arg void call to a 0-param function. The `Op::CallFn` render uses it to
    // emit NOTHING for the underflowing marker (see that arm).
    let param_counts: BTreeMap<String, usize> = prog
        .functions
        .iter()
        .map(|f| (f.name.clone(), f.params.len()))
        .collect();
    // Constant-fold-through-wrap (render_wasm_peephole.rs): the i64-uniform
    // scalar convention round-trips every literal address/offset a
    // self-hosted `prim.*` caller uses through `i64.const → local.set →
    // local.get → i32.wrap_i64`; when the local's ONE definition is a bare
    // constant, that whole round-trip is itself a compile-time constant —
    // fold it to `i32.const` directly at each use. Per-function (never
    // crosses a call boundary), so it composes with pruning either order.
    let funcs = prog
        .functions
        .iter()
        .map(|f| {
            let body =
                fold_const_wrap_roundtrips(&render_wasm_fn(f, &func_slots, &param_counts));
            strip_region_clone_rc_incs(&f.name, body)
        })
        .collect::<String>();
    // Closure dispatch: when any function makes an indirect (closure) call, emit a module
    // function table whose slot i holds function i (the lambda-lifting convention — a
    // lifted lambda is bound to its slot index), plus ONE closure signature per ARITY that
    // appears (`$closure_fnN` = N i64 params → i64) that `call_indirect` checks against.
    // Gated on CallIndirect presence so non-closure programs render byte-identically (no
    // table, no behavior change). Multi-arity supports fold `(Acc, Int) -> Acc` etc.
    // Each distinct closure SIGNATURE is `(arity, heap_result)`: a closure returning a HEAP
    // value (`(Int) -> Option[Int]` for filter_map, `-> List[Int]` for flat_map) is a wasm
    // i32 result (`$closure_fnN_h`), a scalar result is i64 (`$closure_fnN`). The CallIndirect
    // render picks the matching type by its arg count + result repr.
    // Signature class per CallIndirect: 0 = VOID (a `() -> Unit` closure — the lifted
    // lambda renders with NO result, so the dispatch type must be resultless too: typing
    // it `(result i64)` trapped with "indirect call type mismatch" on the simplest
    // `bench(name, f: () -> Unit)` shape), 1 = scalar i64, 2 = heap i32.
    let sigs: std::collections::BTreeSet<(usize, u8)> = prog
        .functions
        .iter()
        .flat_map(|f| f.ops.iter())
        .filter_map(|op| match op {
            Op::CallIndirect { args, result, .. } => {
                let class = match result {
                    None => 0u8,
                    Some(r) if r.is_heap() => 2,
                    Some(_) => 1,
                };
                Some((args.len(), class))
            }
            _ => None,
        })
        .collect();
    let closure_table = if !sigs.is_empty() {
        let n = prog.functions.len();
        let names = prog
            .functions
            .iter()
            .map(|f| format!("${}", f.name))
            .collect::<Vec<_>>()
            .join(" ");
        let types = sigs
            .iter()
            .map(|(a, class)| {
                let params = if *a == 0 {
                    String::new()
                } else {
                    format!(" (param {})", vec!["i64"; *a].join(" "))
                };
                match class {
                    0 => format!("  (type $closure_fn{a}_v (func{params}))\n"),
                    2 => format!("  (type $closure_fn{a}_h (func{params} (result i32)))\n"),
                    _ => format!("  (type $closure_fn{a} (func{params} (result i64)))\n"),
                }
            })
            .collect::<String>();
        format!("{types}  (table {n} funcref)\n  (elem (i32.const 0) func {names})\n")
    } else {
        String::new()
    };
    // Host wasm IMPORTS for every `@extern(wasm, module, name)` the program calls
    // (an `Op::CallImport`). Each `(import "module" "name" (func $__import_… <sig>))`
    // must precede ALL non-import definitions in the function index space, so it is
    // injected into the preamble's import region — right after the WASI imports,
    // before the first `(memory …)`. Deduped + sorted for host-determinism.
    let extern_imports = render_extern_imports(prog);
    // The bump allocator starts past the mutable-global slot region (byte-identical
    // to the plain preamble when the program has no mutable globals).
    let bump_base = HEAP_BASE + 8 * prog.mutable_global_count;
    let preamble = if extern_imports.is_empty() {
        preamble_with_bump_base(bump_base)
    } else {
        // The preamble begins `(module\n  (import "wasi…` — splice the extern imports
        // in right after the opening `(module\n` so they sit in the import block.
        let pre = preamble_with_bump_base(bump_base);
        match pre.split_once('\n') {
            Some((head, rest)) => format!("{head}\n{extern_imports}{rest}"),
            None => pre,
        }
    };
    // A `Unit` main is a void `(call $main)`. An EXPLICIT `-> Result[Unit, String]`
    // main (porta / almide-grammar CLIs) returns a heap Result block: `_start` reads
    // its tag — Ok is discarded (rc_dec), an Err TRAPs (unreachable) so a failing
    // main is never silently exit-0. (v0 prints `Error: msg` + exit 1; the trap is
    // the honest divergence until the message path is worth a helper — no fixture
    // errs today.) The bare `(call $main)` used to leave the block ON THE STACK —
    // every explicit-Result main was invalid wasm ("values remaining").
    let main_returns = prog
        .functions
        .iter()
        .any(|f| f.name == "main" && f.ret.is_some());
    // EAGER GLOBAL INITS (C-007): when the program carries a synthesized
    // `__global_init` (the abortable top-let initializers — render_program builds
    // it), run it BEFORE `$main` so `let bad = 10 / 0` aborts at startup exactly
    // as native does, even when the global is never used.
    // MUTABLE-GLOBAL init runs FIRST (the slots must hold their declared initializers
    // before any code — `__global_init`'s abort re-evaluations included — can read them).
    let mg_init = if prog.functions.iter().any(|f| f.name == "__mg_init") {
        "    (call $__mg_init)\n"
    } else {
        ""
    };
    let ginit: String = format!(
        "{mg_init}{}",
        if prog.functions.iter().any(|f| f.name == "__global_init") {
            "    (call $__global_init)\n"
        } else {
            ""
        }
    );
    // Stage 1 probe: an in-guest epilogue prints the (consumed, trace) pair to
    // STDERR before _start returns — the CLI shells out to the wasmtime BINARY,
    // so the host cannot read exported globals; the guest reports its own
    // counters in the exact format of the native shim. u64 decimal (the trace
    // hash wraps), buffer on the untouched bump frontier.
    let probe_epilogue = if crate::charge_probe::probe_enabled() {
        "    (call $__probe_print)\n"
    } else {
        ""
    };
    let start = if main_returns {
        // main's Result[Unit, String] is LEN-AS-TAG (scalar Ok): len@4 == 0 ⇒ Ok (discard),
        // len 1 ⇒ Err with the String handle in slot 0's low half (@12). The Err path runs
        // v0's protocol via $__main_err: `Error: <msg>\n` on STDERR + exit 1. (The former
        // @16 read was the cap-as-tag offset — always 0 here, so an erring main silently
        // exited 0.)
        format!(
            "  (func (export \"_start\") (local $r i32)\n{ginit}    (local.set $r (call $main))\n    (if (i32.ne (i32.load (i32.add (local.get $r) (i32.const {LIST_LEN_OFFSET}))) (i32.const 0))\n      (then (call $__main_err (i32.load (i32.add (local.get $r) (i32.const {LIST_HEADER}))))))\n    (call $rc_dec (local.get $r))\n{probe_epilogue})\n"
        )
    } else {
        format!("  (func (export \"_start\")\n{ginit}    (call $main)\n{probe_epilogue})\n")
    };
    let pub_exports: String = prog
        .exports
        .iter()
        .map(|(export_name, internal, param_floats, ret_float)| {
            if param_floats.iter().all(|f| !f) && !matches!(ret_float, Some(true)) {
                // Float-free signature: the internal ABI (i64 scalars, i32 heap
                // handles) IS the public ABI — v0 exports these fns verbatim too.
                return format!("  (export {export_name:?} (func ${internal}))\n");
            }
            // Float-bearing signature: a thin REINTERPRET wrapper presents real f64s
            // (the v0 export ABI) while the internal fn keeps the i64-bits convention.
            // Non-Float params keep the internal wasm valtype (i64 scalar / i32 heap),
            // so the wrapper must read each param's ACTUAL repr, not assume i64.
            let f = prog
                .functions
                .iter()
                .find(|f| f.name == *internal)
                .expect("export names a lowered function (pipeline invariant)");
            let reprs = value_reprs_wasm(f);
            let params: String = f
                .params
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    let wat = if param_floats.get(i).copied().unwrap_or(false) {
                        "f64"
                    } else {
                        wasm_ty(p.repr)
                    };
                    format!(" (param $p{i} {wat})")
                })
                .collect();
            let internal_ret = f
                .ret
                .map(|r| wasm_ty(reprs.get(&r).copied().unwrap_or(SCALAR_REPR)));
            let result = match (ret_float, internal_ret) {
                (Some(true), _) => " (result f64)".to_string(),
                (_, Some(wat)) => format!(" (result {wat})"),
                (_, None) => String::new(),
            };
            let args: String = f
                .params
                .iter()
                .enumerate()
                .map(|(i, _)| {
                    if param_floats.get(i).copied().unwrap_or(false) {
                        format!(" (i64.reinterpret_f64 (local.get $p{i}))")
                    } else {
                        format!(" (local.get $p{i})")
                    }
                })
                .collect();
            let call = format!("(call ${internal}{args})");
            let body = if matches!(ret_float, Some(true)) {
                format!("    (f64.reinterpret_i64 {call})\n")
            } else {
                format!("    {call}\n")
            };
            format!(
                "  (func $__export_{internal}{params}{result}\n{body}  )\n  (export {export_name:?} (func $__export_{internal}))\n"
            )
        })
        .collect();
    // The mutable-global slot TAKE accessor (emitted iff the program has slots): loads
    // the slot's block handle WITHOUT an rc change — the slot's own reference transfers
    // to the caller (the assign path drops it and stores a replacement), which is
    // exactly the fresh-owned CallFn result the ownership certificate models. Reads
    // need no helper: they borrow-then-`Dup` the slot handle inline (`rc_inc`).
    // Emitted for mutable-global slots AND for the local SHARED-CELL assigns
    // (cells.rs), which reuse the same take accessor over a cell-slot address.
    let uses_mg_take = prog.functions.iter().any(|f| {
        f.ops
            .iter()
            .any(|o| matches!(o, Op::CallFn { name, .. } if name == "__mg_take"))
    });
    let mg_helpers = if prog.mutable_global_count > 0 || uses_mg_take {
        "  (func $__mg_take (param $a i64) (result i32)\n    \
         (i32.load (i32.wrap_i64 (local.get $a))))\n"
            .to_string()
    } else {
        String::new()
    };
    // Dead-import/dead-function elimination over the fixed preamble (see
    // render_wasm_dce.rs): drop every WASI import and runtime helper nothing
    // in this program's own rendered code — or a kept helper's own body —
    // transitively reaches. `println`-only programs no longer link
    // `path_open`/`fd_readdir`/`clock_time_get`/etc.
    let used_text = format!("{closure_table}{funcs}{mg_helpers}{start}{pub_exports}");
    let preamble = filter_unreachable_preamble(&preamble, &used_text);
    format!(
        "{preamble}{used_text})
"
    )
}

include!("render_wasm_module_parts.rs");
include!("render_wasm_b.rs");
include!("render_wasm_bce.rs");
include!("render_wasm_c.rs");
include!("render_wasm_dce.rs");
include!("render_wasm_peephole.rs");
include!("render_wasm_switch.rs");

/// The self-hosted stdlib runtime registry: `(call name, impl fn name, Almide source)`.
/// The v1 linker auto-includes an entry when its `call name` is invoked but undefined,
/// renaming the impl fn (Almide names can't hold a dot) to the call name — so
/// `(call $module.func)` resolves AND the caps gate reads it as a known-pure stdlib
/// `module.func`. The single source of truth for the stdlib self-host campaign (§4.1:
/// the runtime self-hosts into Almide; the trusted floor stays the prim ops + checker).
/// The registry itself moved to `almide_types::self_host_registry`, beside the
/// embedded sources it names: the interp oracle reads the SAME table to evaluate
/// the same bodies as the third cross-target vote, and a table owned by one
/// backend would have forced it to either depend on this backend or restate the
/// mapping (the hand-mirrored-bridge drift class). Re-exported here so every
/// in-crate consumer (`crate::render_wasm::self_host_runtime`) is unchanged.
pub use almide_lang::self_host_registry::self_host_runtime;

#[cfg(test)]
mod tests;
