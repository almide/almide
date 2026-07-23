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
//! hand-written WAT runtime below (`$list_copy`/`$itoa_append`/`$print_list`)
//! and the computation baked into the `Push`/`IndexSet`/`Print` MIR ops are the
//! EXACT trap that made v0's wasm emitter a nightmare (a large hand-written wasm
//! surface dual-maintained with native). They exist only to prove the
//! dual-renderer path RUNS. The ideal form shrinks the hand-written wasm to a
//! tiny, total, decision-free, spec-provable MIR-PRIMITIVE mapping, and moves
//! all of list/string/format/RC into Almide compiled through this same path
//! (`Push`/`IndexSet`/`Print` become `Call`s to self-hosted runtime functions).
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
const ITOA_TMP_ADDR: u32 = 32; // reversed-digit scratch (≤ 20 bytes)
// The fs.read_text path_open error message — a CONST byte run in the data section
// (the `$read_text_file` Err arm copies it into a canonical String). Reserved BELOW the
// dynamic print labels so the per-function label writer (which starts at `LABELS_ADDR`)
// never overlaps it.
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
const LABELS_ADDR: u32 = 376; // print labels (the data section) — after ALL fixed messages (incl. fs errno)
// fs errno → native std::io Display strings (240..376, FIXED — placed BEFORE the
// variable-length labels region so labels can never overwrite them): path_open errors
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
const SCRATCH_ADDR: u32 = 768; // the line build buffer
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
const LIST_HEADER: u32 = LIST_CAP_OFFSET + I32_SIZE; // rc + len + cap
const ELEM_SIZE: u32 = 8; // i64 elements
// A freshly allocated heap block has exactly one owner — the `Alloc`'s +1, the
// initial value of the cell RuntimeModel.v's `exec` starts the fold from.
const RC_INITIAL: i32 = 1;
const PUSH_HEADROOM: u32 = 8; // spare cap so demo pushes never realloc
const IOVEC_LEN_OFFSET: u32 = I32_SIZE; // iovec = [buf:i32 @0][len:i32 @4]

// WASI fd_write parameters / numeric base.
const STDOUT_FD: u32 = 1;
const IOVS_COUNT: u32 = 1; // one iovec per write
const DECIMAL_BASE: i64 = 10;

/// ASCII bytes the formatter writes.
const ASCII_ZERO: u32 = 48;
const ASCII_EQUALS: u32 = 61;
const ASCII_COMMA: u32 = 44;
const ASCII_NEWLINE: u32 = 10;
const ASCII_SLASH: u32 = 47; // '/' — stripped from an absolute fs.read_text path

/// The line buffer for printing lives in `[SCRATCH_ADDR, HEAP_BASE)`; one element
/// appends at most a separator comma plus the digits of a u64 (≤ 20). The print
/// loop traps if appending the next element would cross `HEAP_BASE` (the buffer
/// end), so a very long list cannot overflow the line buffer into the heap.
const MAX_I64_DIGITS: u32 = 20; // a u64 is at most 20 decimal digits
const MAX_ELEM_PRINT_BYTES: u32 = 1 + MAX_I64_DIGITS; // comma + digits

/// Render a MIR function to a runnable WAT module string.
pub fn render_wasm(func: &MirFunction) -> String {
    // Heap handles (Alloc/Dup dsts) become i32 list-pointer locals.
    let mut heap_locals: Vec<ValueId> = Vec::new();
    for op in &func.ops {
        match op {
            Op::Alloc { dst, .. } | Op::Dup { dst, .. } => {
                if !heap_locals.contains(dst) {
                    heap_locals.push(*dst);
                }
            }
            _ => {}
        }
    }

    // Labels → data-section offsets (deduplicated).
    let mut label_off: BTreeMap<String, (u32, u32)> = BTreeMap::new();
    let mut data = String::new();
    let mut cursor = LABELS_ADDR;
    for op in &func.ops {
        if let Op::Call { args, .. } = op {
            for a in args {
                if let CallArg::Label(label) = a {
                    if !label_off.contains_key(label) {
                        let len = label.len() as u32;
                        label_off.insert(label.clone(), (cursor, len));
                        data.push_str(&format!("  (data (i32.const {cursor}) {:?})\n", label));
                        cursor += len;
                    }
                }
            }
        }
    }

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
        body.push_str(&render_op(op, &label_off, &no_slots, &no_param_counts, &func.heap_slot_masks, &reprs, &no_floats, &mut no_fuser));
    }

    format!(
        "{preamble}{data}  (func $main {locals}\n{body}  )\n  (func (export \"_start\") (call $main))\n)\n",
        preamble = preamble(),
        data = data,
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
        let end = seg.find(|c: char| !(c.is_alphanumeric() || c == '_')).unwrap_or(seg.len());
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
        f.ops.iter().any(|o| matches!(o, Op::CallFn { name, .. } if name == "__mg_take"))
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
            if let Op::CallFn { name, .. } = op {
                if !resolvable.contains(name) {
                    if std::env::var("ALMIDE_DBG_UNLINKED").is_ok() {
                        eprintln!("[unlinked] {} references {}", f.name, name);
                    }
                    missing.insert(name.clone());
                }
            }
        }
    }
    missing
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
    let needs_alias = prog.functions.iter().flat_map(|f| f.ops.iter()).any(|op| {
        matches!(op, Op::CallFn { name, .. }
            if !resolvable.contains(name) && resolve_rt_alias(name, &resolvable).is_some())
    });
    let remapped;
    let prog = if needs_alias {
        let mut p = prog.clone();
        for f in &mut p.functions {
            for op in &mut f.ops {
                let Op::CallFn { name, .. } = op else {
                    continue;
                };
                if resolvable.contains(name) {
                    continue;
                }
                if let Some(alias) = resolve_rt_alias(name, &resolvable) {
                    *name = alias;
                }
            }
        }
        remapped = p;
        &remapped
    } else {
        prog
    };
    let missing = unlinked_call_names(prog);
    if !missing.is_empty() {
        // DEAD-FUNCTION PRUNE (#782): an UNREACHABLE function carrying an unlinked call
        // (`local fn dead(..) = matrix.qwen3_…(..)` — a native-only intrinsic in a fn
        // main never calls) must not fail the whole module: v0 simply never emitted it.
        // Drop every function that is (a) not a root — `main` / a declared export —
        // AND (b) not REFERENCED by any surviving function (a `CallFn`/`FuncRef` edge,
        // or a `$__drop_<ty>` walker named by a `DropVariant`) AND (c) itself carrying
        // an unlinked call. Iterated to a fixpoint (a dead fn referenced only by
        // another dead fn prunes in a later round). A REACHABLE unlinked call keeps
        // the loud reject below — never a dangling `(call $…)`.
        let mut kept: Vec<MirFunction> = prog.functions.clone();
        loop {
            let resolvable = {
                let mut names: BTreeSet<String> =
                    kept.iter().map(|f| f.name.clone()).collect();
                names.extend(preamble_func_names());
                names.insert("__mg_take".to_string());
                names
            };
            let mut referenced: BTreeSet<String> = BTreeSet::new();
            for f in &kept {
                for op in &f.ops {
                    match op {
                        Op::CallFn { name, .. } => {
                            referenced.insert(name.clone());
                        }
                        Op::FuncRef { name, .. } => {
                            referenced.insert(name.clone());
                        }
                        Op::DropVariant { ty, .. } => {
                            referenced.insert(format!("__drop_{ty}"));
                        }
                        _ => {}
                    }
                }
            }
            let roots: BTreeSet<&str> = std::iter::once("main")
                .chain(prog.exports.iter().map(|(_, internal, _, _)| internal.as_str()))
                .collect();
            let before = kept.len();
            kept.retain(|f| {
                roots.contains(f.name.as_str())
                    || referenced.contains(&f.name)
                    || f.ops.iter().all(|op| {
                        !matches!(op, Op::CallFn { name, .. } if !resolvable.contains(name))
                    })
            });
            if kept.len() == before {
                break;
            }
        }
        let pruned_prog = MirProgram {
            functions: kept,
            exports: prog.exports.clone(),
            mutable_global_count: prog.mutable_global_count,
        };
        let still_missing = unlinked_call_names(&pruned_prog);
        if !still_missing.is_empty() {
            let names = still_missing.into_iter().collect::<Vec<_>>().join(", ");
            return Err(crate::lower::LowerError::Unsupported(format!(
                "unlinked stdlib/runtime call(s) with no wasm definition: {names} — \
                 rendering them would emit a dangling `(call $…)` (invalid wasm). \
                 Add the callee to the self-host registry or wall the using function."
            )));
        }
        return Ok(render_wasm_program(&pruned_prog));
    }
    Ok(render_wasm_program(prog))
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
    // Labels (the data section) are module-level — collect across all functions.
    let mut label_off: BTreeMap<String, (u32, u32)> = BTreeMap::new();
    let mut data = String::new();
    let mut cursor = LABELS_ADDR;
    for func in &prog.functions {
        for op in &func.ops {
            let Op::Call { args, .. } = op else {
                continue;
            };
            for a in args {
                let CallArg::Label(label) = a else {
                    continue;
                };
                if label_off.contains_key(label) {
                    continue;
                }
                let len = label.len() as u32;
                label_off.insert(label.clone(), (cursor, len));
                data.push_str(&format!("  (data (i32.const {cursor}) {:?})\n", label));
                cursor += len;
            }
        }
    }
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
    let param_counts: BTreeMap<String, usize> =
        prog.functions.iter().map(|f| (f.name.clone(), f.params.len())).collect();
    let funcs = prog
        .functions
        .iter()
        .map(|f| render_wasm_fn(f, &label_off, &func_slots, &param_counts))
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
    let start = if main_returns {
        // main's Result[Unit, String] is LEN-AS-TAG (scalar Ok): len@4 == 0 ⇒ Ok (discard),
        // len 1 ⇒ Err with the String handle in slot 0's low half (@12). The Err path runs
        // v0's protocol via $__main_err: `Error: <msg>\n` on STDERR + exit 1. (The former
        // @16 read was the cap-as-tag offset — always 0 here, so an erring main silently
        // exited 0.)
        format!(
            "  (func (export \"_start\") (local $r i32)\n{ginit}    (local.set $r (call $main))\n    (if (i32.ne (i32.load (i32.add (local.get $r) (i32.const {LIST_LEN_OFFSET}))) (i32.const 0))\n      (then (call $__main_err (i32.load (i32.add (local.get $r) (i32.const {LIST_HEADER}))))))\n    (call $rc_dec (local.get $r)))\n"
        )
    } else {
        format!("  (func (export \"_start\")\n{ginit}    (call $main))\n")
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
        f.ops.iter().any(|o| matches!(o, Op::CallFn { name, .. } if name == "__mg_take"))
    });
    let mg_helpers = if prog.mutable_global_count > 0 || uses_mg_take {
        "  (func $__mg_take (param $a i64) (result i32)\n    \
         (i32.load (i32.wrap_i64 (local.get $a))))\n"
            .to_string()
    } else {
        String::new()
    };
    format!("{preamble}{data}{closure_table}{funcs}{mg_helpers}{start}{pub_exports})
")
}

include!("render_wasm_b.rs");
include!("render_wasm_c.rs");

/// The self-hosted stdlib runtime registry: `(call name, impl fn name, Almide source)`.
/// The v1 linker auto-includes an entry when its `call name` is invoked but undefined,
/// renaming the impl fn (Almide names can't hold a dot) to the call name — so
/// `(call $module.func)` resolves AND the caps gate reads it as a known-pure stdlib
/// `module.func`. The single source of truth for the stdlib self-host campaign (§4.1:
/// the runtime self-hosts into Almide; the trusted floor stays the prim ops + checker).
/// (mod declarations kept in THIS physical file — not include!'d — because Rust
/// resolves `mod X;`'s implicit file path from the physical file's own directory,
/// not the logical include! chain: this file is `src/render_wasm.rs`, backing
/// `pub mod render_wasm;`, so `mod registry;` correctly finds `render_wasm/registry.rs`.)
mod registry;
pub use registry::self_host_runtime;

#[cfg(test)]
mod tests;
