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
const HEAP_BASE: u32 = 8192; // bump allocator start
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
    for op in &func.ops {
        body.push_str(&render_op(op, &label_off, &no_slots, &no_param_counts, &func.heap_slot_masks));
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
pub fn try_render_wasm_program(prog: &MirProgram) -> Result<String, crate::lower::LowerError> {
    let missing = unlinked_call_names(prog);
    if !missing.is_empty() {
        let names = missing.into_iter().collect::<Vec<_>>().join(", ");
        return Err(crate::lower::LowerError::Unsupported(format!(
            "unlinked stdlib/runtime call(s) with no wasm definition: {names} — \
             rendering them would emit a dangling `(call $…)` (invalid wasm). \
             Add the callee to the self-host registry or wall the using function."
        )));
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
    let sigs: std::collections::BTreeSet<(usize, bool)> = prog
        .functions
        .iter()
        .flat_map(|f| f.ops.iter())
        .filter_map(|op| match op {
            Op::CallIndirect { args, result, .. } => {
                Some((args.len(), result.map(|r| r.is_heap()).unwrap_or(false)))
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
            .map(|(a, heap)| {
                let params = if *a == 0 {
                    String::new()
                } else {
                    format!(" (param {})", vec!["i64"; *a].join(" "))
                };
                let (suffix, res) = if *heap { ("_h", "i32") } else { ("", "i64") };
                format!("  (type $closure_fn{a}{suffix} (func{params} (result {res})))\n")
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
    let preamble = if extern_imports.is_empty() {
        preamble()
    } else {
        // The preamble begins `(module\n  (import "wasi…` — splice the extern imports
        // in right after the opening `(module\n` so they sit in the import block.
        let pre = preamble();
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
    let ginit = if prog.functions.iter().any(|f| f.name == "__global_init") {
        "    (call $__global_init)\n"
    } else {
        ""
    };
    let start = if main_returns {
        format!(
            "  (func (export \"_start\") (local $r i32)\n{ginit}    (local.set $r (call $main))\n    (if (i32.ne (i32.load (i32.add (local.get $r) (i32.const 16))) (i32.const 0))\n      (then unreachable))\n    (call $rc_dec (local.get $r)))\n"
        )
    } else {
        format!("  (func (export \"_start\")\n{ginit}    (call $main))\n")
    };
    format!("{preamble}{data}{closure_table}{funcs}{start})
")
}

/// The `(import …)` declarations for every distinct `@extern(wasm, module, name)`
/// host function the program calls (an [`Op::CallImport`]). The import signature is
/// the import's wasm valtypes (`abi`/`result_abi`, mapped from the declared Almide
/// types at lowering), so the declared `(func (param …) (result …))` matches exactly
/// what the call site supplies. Deduped by symbol + sorted (host-deterministic). A
/// program with no host import renders the empty string (byte-identical to before).
fn render_extern_imports(prog: &MirProgram) -> String {
    let mut decls: BTreeMap<String, String> = BTreeMap::new();
    for f in &prog.functions {
        for op in &f.ops {
            if let Op::CallImport { module, name, abi, result_abi, .. } = op {
                let sym = import_symbol(module, name);
                let params = if abi.is_empty() {
                    String::new()
                } else {
                    format!(
                        " (param {})",
                        abi.iter().map(|a| a.wat()).collect::<Vec<_>>().join(" ")
                    )
                };
                let result = result_abi
                    .map(|r| format!(" (result {})", r.wat()))
                    .unwrap_or_default();
                decls.entry(sym.clone()).or_insert_with(|| {
                    format!(
                        "  (import {module:?} {name:?} (func ${sym}{params}{result}))\n"
                    )
                });
            }
        }
    }
    decls.into_values().collect()
}

/// Render one MIR function with its signature (params, locals, result).
pub fn render_wasm_fn(
    func: &MirFunction,
    label_off: &BTreeMap<String, (u32, u32)>,
    func_slots: &BTreeMap<String, u32>,
    param_counts: &BTreeMap<String, usize>,
) -> String {
    let reprs = value_reprs_wasm(func);
    // A LIFTED LAMBDA (`__lambda_*`) is dispatched through the function table against the uniform
    // i64 closure signature (`$closure_fnN`), so its params MUST all be i64. A HEAP param (a Ptr)
    // is received as an i64 raw param and NARROWED to its Ptr value local at entry (the dual of the
    // CallIndirect's `i64.extend_i32_u` widen); a scalar param is already i64. Regular functions
    // keep their natural per-repr signature.
    let is_lambda = func.name.starts_with("__lambda_");
    let mut lambda_narrow = String::new();
    let mut lambda_heap_locals: Vec<String> = Vec::new();
    let params = func
        .params
        .iter()
        .map(|p| {
            if is_lambda && p.repr.is_heap() {
                lambda_heap_locals.push(format!("(local {} i32)", local(p.value)));
                lambda_narrow.push_str(&format!(
                    "    (local.set {v} (i32.wrap_i64 (local.get {v}_raw)))\n",
                    v = local(p.value)
                ));
                format!("(param {}_raw i64)", local(p.value))
            } else if is_lambda {
                format!("(param {} i64)", local(p.value))
            } else {
                format!("(param {} {})", local(p.value), wasm_ty(p.repr))
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    let result = func
        .ret
        .map(|r| format!(" (result {})", wasm_ty(reprs.get(&r).copied().unwrap_or(SCALAR_REPR))))
        .unwrap_or_default();
    // locals = values defined in the body that are not params (first-def order).
    let mut seen: BTreeSet<ValueId> = func.params.iter().map(|p| p.value).collect();
    let mut locals = Vec::new();
    for op in &func.ops {
        if let Some(d) = defined_value(op) {
            if seen.insert(d) {
                let ty = wasm_ty(reprs.get(&d).copied().unwrap_or(SCALAR_REPR));
                locals.push(format!("(local {} {ty})", local(d)));
            }
        }
    }
    // A recursive List[String] drop needs two i32 scratch locals (loop index + length); they
    // are function-wide (DropListStr ops never nest) and only declared when one is present.
    // `DropResultListStr` (Result[List[String], String]) also loops the Ok payload list with
    // $dlsi/$dlsn, so it joins this gate.
    if func.ops.iter().any(|op| matches!(op,
        Op::DropListStr { .. } | Op::DropResultListStrInt { .. } | Op::DropResultListStr { .. })) {
        locals.push("(local $dlsi i32) (local $dlsn i32)".to_string());
    }
    // DropResultListStrInt reuses the List[List[String]] scratch ($dlli = tuple handle, $dllinner =
    // the inner List handle) for its nested Ok-tuple List free; `DropResultListStr` reuses just $dlli
    // (the Ok payload List handle — no inner $dllinner, its payload is the direct list). Declare them
    // when no DropListListStr did.
    // `DropListIntStr` (List[(Int,String)]) loops with $dlli/$dlln/$dllinner too (no $dlsi/$dlsn —
    // its per-element free is a single rc_dec of the tuple's String slot, not a nested loop).
    if func.ops.iter().any(|op| matches!(op,
        Op::DropResultListStrInt { .. } | Op::DropResultListStr { .. } | Op::DropListIntStr { .. }
        | Op::DropListStrInt { .. }))
        && !func.ops.iter().any(|op| matches!(op, Op::DropListListStr { .. }))
    {
        locals.push("(local $dlli i32) (local $dlln i32) (local $dllinner i32)".to_string());
    }
    // A recursive `List[List[String]]` drop is a NESTED loop: the OUTER loop over the rows needs its
    // own index/length/inner-handle scratch (`$dlsi`/`$dlsn` serve the INNER cell loop). It also uses
    // the inner-loop locals, so declare those too when no plain DropListStr already did.
    if func.ops.iter().any(|op| matches!(op, Op::DropListListStr { .. })) {
        locals.push("(local $dlli i32) (local $dlln i32) (local $dllinner i32)".to_string());
        if !func.ops.iter().any(|op| matches!(op,
            Op::DropListStr { .. } | Op::DropResultListStr { .. })) {
            locals.push("(local $dlsi i32) (local $dlsn i32)".to_string());
        }
    }
    // A lifted lambda's heap params become i32 value locals (narrowed from their i64 raw params).
    locals.extend(lambda_heap_locals);
    let locals_decl = locals.join(" ");
    // The heap-param narrowing runs first, before any body op reads the Ptr value local.
    let mut body = lambda_narrow;
    // The if-markers (IfThen/Else/EndIf) render to a NESTED wasm `if`/`else` — a
    // stateful reconstruction of the flat marker stream. A scalar `if` is an
    // expression `(local.set $dst (if (result i64) cond (then …val) (else …val)))`;
    // each arm leaves its value on the stack. Only the taken arm executes.
    let mut if_stack: Vec<Option<ValueId>> = Vec::new(); // the result dst per open if
    let arm_val = |v: &Option<ValueId>| {
        v.map(|v| format!("      (local.get {})\n", local(v))).unwrap_or_default()
    };
    // The loop-markers (LoopStart/LoopBreakUnless/LoopEnd) reconstruct the standard
    // wasm while shape `(block $brk (loop $cont … (br_if $brk (eqz cond)) … (br $cont)))`.
    // A unique id per loop keeps nested loops' labels distinct; the stack tracks which
    // open loop a break/back-edge closes.
    let mut loop_ctr: u32 = 0;
    let mut loop_stack: Vec<u32> = Vec::new();
    for op in &func.ops {
        match op {
            Op::LoopStart => {
                let id = loop_ctr;
                loop_ctr += 1;
                loop_stack.push(id);
                body.push_str(&format!("    (block $brk{id}\n    (loop $cont{id}\n"));
            }
            Op::LoopBreakUnless { cond } => {
                let id = *loop_stack.last().expect("LoopBreakUnless outside a loop");
                body.push_str(&format!(
                    "    (br_if $brk{id} (i64.eqz (local.get {})))\n",
                    local(*cond)
                ));
            }
            Op::LoopEnd => {
                let id = loop_stack.pop().expect("LoopEnd without LoopStart");
                // unconditional back-edge to the loop top, then close `loop` and `block`.
                body.push_str(&format!("    (br $cont{id})\n    ))\n"));
            }
            Op::IfThen { cond, dst } => {
                if_stack.push(*dst);
                // The result type follows the dst repr: a heap-result `if` yields an i32
                // handle, a scalar one an i64 (value_reprs_wasm fixed dst from the arm val).
                let res = match dst {
                    Some(d) => format!(
                        " (result {})",
                        wasm_ty(reprs.get(d).copied().unwrap_or(SCALAR_REPR))
                    ),
                    None => String::new(),
                };
                let set = dst.map(|d| format!("(local.set {} ", local(d))).unwrap_or_default();
                body.push_str(&format!(
                    "    {set}(if{res} (i64.ne (local.get {c}) (i64.const 0))\n      (then\n",
                    c = local(*cond),
                ));
            }
            Op::Else { val } => {
                body.push_str(&format!("{}      )\n      (else\n", arm_val(val)));
            }
            Op::EndIf { val } => {
                let dst = if_stack.pop().expect("EndIf without IfThen");
                // close: else-arm value, `)` else, `)` if, and `)` local.set if scalar.
                let close = if dst.is_some() { "))\n" } else { ")\n" };
                body.push_str(&format!("{}      ){close}", arm_val(val)));
            }
            _ => body.push_str(&render_op(op, label_off, func_slots, param_counts, &func.heap_slot_masks)),
        }
    }
    let tail = func.ret.map(|r| format!("    (local.get {})\n", local(r))).unwrap_or_default();
    format!("  (func ${} {params}{result} {locals_decl}\n{body}{tail}  )\n", func.name)
}

const SCALAR_REPR: Repr = Repr::Scalar { width: crate::ScalarWidth::Double };

fn wasm_ty(repr: Repr) -> &'static str {
    if repr.is_heap() {
        "i32"
    } else {
        "i64"
    }
}

/// The value an op defines (binds), if any.
fn defined_value(op: &Op) -> Option<ValueId> {
    match op {
        Op::Alloc { dst, .. }
        | Op::Dup { dst, .. }
        | Op::Const { dst }
        | Op::ConstInt { dst, .. }
        | Op::FuncRef { dst, .. }
        | Op::IntBinOp { dst, .. }
        | Op::Pure { dst, .. } => Some(*dst),
        Op::CallFn { dst, .. } | Op::Call { dst, .. } => *dst,
        Op::CallImport { dst, .. } => *dst,
        Op::CallIndirect { dst, .. } => *dst,
        Op::Prim { dst, .. } => *dst,
        Op::IfThen { dst, .. } => *dst,
        _ => None,
    }
}

/// Infer each value's Repr (params + op results) for local/param/result typing.
fn value_reprs_wasm(func: &MirFunction) -> BTreeMap<ValueId, Repr> {
    let mut m = BTreeMap::new();
    // The `if`-result `dst` repr follows the ARM values (a heap-result `if` yields an i32
    // handle, a scalar one an i64): seed `dst` scalar at `IfThen`, then OVERWRITE it from
    // the arm value's repr at `EndIf`. The stack pairs each `EndIf` with its `IfThen` dst.
    let mut if_result_stack: Vec<Option<ValueId>> = Vec::new();
    for p in &func.params {
        m.insert(p.value, p.repr);
    }
    for op in &func.ops {
        match op {
            Op::Alloc { dst, repr, .. } => {
                m.insert(*dst, *repr);
            }
            Op::Dup { dst, src } => {
                let r = m.get(src).copied().unwrap_or(Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT });
                m.insert(*dst, r);
            }
            Op::Const { dst }
            | Op::ConstInt { dst, .. }
            | Op::FuncRef { dst, .. }
            | Op::IntBinOp { dst, .. } => {
                m.insert(*dst, SCALAR_REPR);
            }
            // A `LoadHandle` result is a heap PTR (i32 handle); an `ArgsGetList` result is a
            // freshly-allocated heap `List[String]` PTR; a `ReadTextFile` result is a
            // freshly-allocated heap `Result[String, String]` PTR; a `ReadDir` result is a
            // freshly-allocated heap `Result[List[String], String]` PTR — all keep Ptr repr (no
            // i64 zero-extend). Every other prim result (a load, fd_write errno, or
            // handle→address) is a scalar i64.
            Op::Prim {
                dst: Some(dst),
                kind: PrimKind::LoadHandle
                    | PrimKind::ArgsGetList
                    | PrimKind::ReadLine
                    | PrimKind::ReadNBytes
                    | PrimKind::ReadTextFile
                    | PrimKind::ReadDir
                    | PrimKind::WriteTextFile
                    | PrimKind::MakeDir
                    | PrimKind::RemoveAll,
                ..
            } => {
                m.insert(*dst, Repr::Ptr { layout: crate::PLACEHOLDER_LAYOUT });
            }
            Op::Prim { dst: Some(dst), .. } => {
                m.insert(*dst, SCALAR_REPR);
            }
            // An `if` result: seed scalar, recorded on the stack; the real repr (scalar
            // i64 or heap-result i32) is fixed from the arm value at the matching `EndIf`.
            Op::IfThen { dst, .. } => {
                if_result_stack.push(*dst);
                if let Some(dst) = dst {
                    m.insert(*dst, SCALAR_REPR);
                }
            }
            Op::EndIf { val: Some(v) } => {
                if let Some(Some(dst)) = if_result_stack.pop() {
                    if let Some(r) = m.get(v).copied() {
                        m.insert(dst, r);
                    }
                }
            }
            Op::EndIf { val: None } => {
                if_result_stack.pop();
            }
            // A call's result repr is the callee's RETURN repr, carried on the op
            // (`result`) — the same field the ownership analysis reads to know a call
            // hands back a heap object. A String/List-returning call is a Ptr (i32),
            // NOT a scalar; typing it i64 mismatched `$alloc`'s i32 handle.
            Op::CallFn { dst: Some(d), result, .. } => {
                m.insert(*d, result.unwrap_or(SCALAR_REPR));
            }
            // An indirect (closure) call's result repr is likewise carried on the op.
            Op::CallIndirect { dst: Some(d), result, .. } => {
                m.insert(*d, result.unwrap_or(SCALAR_REPR));
            }
            _ => {}
        }
    }
    m
}

fn local(v: ValueId) -> String {
    format!("$v{}", v.0)
}

/// The wasm `$func` symbol an `@extern(wasm, module, name)` IMPORT is declared and
/// called under. Mangled `$__import_<module>_<name>` so it cannot collide with a
/// user/runtime function of the same bare `name` (the wrapper fn keeps its own
/// name and `(call $__import_…)`s this). Single source for the import declaration
/// (render_wasm_program), the call render (`render_op`), and the translation-
/// validation pattern.
pub fn import_symbol(module: &str, name: &str) -> String {
    format!("__import_{module}_{name}")
}


fn render_arg_wasm(arg: &CallArg) -> String {
    match arg {
        CallArg::Handle(v) | CallArg::Scalar(v) => format!("(local.get {})", local(*v)),
        CallArg::Imm(n) => format!("(i64.const {n})"),
        CallArg::Label(l) => panic!("label arg {l:?} not valid for a user call"),
    }
}

/// Render one `Op::CallImport` arg, COERCED from its i64-uniform / i32-heap MIR
/// local to the import-signature valtype `ty`. A scalar MIR local is i64: an `F64`
/// import param reads the f64 BITS it holds (`f64.reinterpret_i64`), an `I32` Bool
/// param narrows (`i32.wrap_i64`), an `I64` param passes through. A heap handle is
/// already an i32 pointer for an `I32` param. An immediate matches the valtype's
/// constant form.
fn render_import_arg_wasm(arg: &CallArg, ty: crate::WasmAbi) -> String {
    use crate::WasmAbi;
    match arg {
        CallArg::Handle(v) => match ty {
            // A heap handle is an i32 pointer — exactly the `I32` import valtype.
            WasmAbi::I32 => format!("(local.get {})", local(*v)),
            // A heap handle to an i64/f64 param is a type error the lowering never emits.
            _ => format!("(local.get {})", local(*v)),
        },
        CallArg::Scalar(v) => match ty {
            WasmAbi::I64 => format!("(local.get {})", local(*v)),
            WasmAbi::F64 => format!("(f64.reinterpret_i64 (local.get {}))", local(*v)),
            WasmAbi::I32 => format!("(i32.wrap_i64 (local.get {}))", local(*v)),
        },
        CallArg::Imm(n) => match ty {
            WasmAbi::I64 => format!("(i64.const {n})"),
            WasmAbi::F64 => format!("(f64.reinterpret_i64 (i64.const {n}))"),
            WasmAbi::I32 => format!("(i32.const {n})"),
        },
        CallArg::Label(l) => panic!("label arg {l:?} not valid for a host import call"),
    }
}

fn render_call(
    dst: Option<ValueId>,
    func: &RtFn,
    args: &[CallArg],
    label_off: &BTreeMap<String, (u32, u32)>,
) -> String {
    match (func, args) {
        (RtFn::ListSet, [CallArg::Handle(t), CallArg::Imm(idx), CallArg::Imm(val)]) => format!(
            "    (call $list_set (local.get {t}) (i32.const {idx}) (i64.const {val}))\n",
            t = local(*t)
        ),
        (RtFn::ListPush, [CallArg::Handle(t), CallArg::Imm(val)]) => {
            // push may move the buffer → rebind the handle local (dst == target).
            let target = dst.unwrap_or(*t);
            format!(
                "    (local.set {d} (call $list_push (local.get {t}) (i64.const {val})))\n",
                d = local(target),
                t = local(*t)
            )
        }
        (RtFn::PrintList, [CallArg::Handle(v), CallArg::Label(label)]) => {
            let (off, len) = label_off[label];
            format!(
                "    (call $print_list (local.get {v}) (i32.const {off}) (i32.const {len}))\n",
                v = local(*v)
            )
        }
        (RtFn::PrintInt, [CallArg::Scalar(v)]) => {
            format!("    (call $print_int (local.get {}))\n", local(*v))
        }
        (RtFn::PrintStr, [CallArg::Handle(v)]) => {
            format!("    (call $print_str (local.get {}))\n", local(*v))
        }
        _ => panic!("malformed runtime call {func:?} with args {args:?}"),
    }
}

/// The self-hosted stdlib runtime registry: `(call name, impl fn name, Almide source)`.
/// The v1 linker auto-includes an entry when its `call name` is invoked but undefined,
/// renaming the impl fn (Almide names can't hold a dot) to the call name — so
/// `(call $module.func)` resolves AND the caps gate reads it as a known-pure stdlib
/// `module.func`. The single source of truth for the stdlib self-host campaign (§4.1:
/// the runtime self-hosts into Almide; the trusted floor stays the prim ops + checker).
mod registry;
pub use registry::self_host_runtime;


#[cfg(test)]
mod tests;

include!("render_wasm_p2.rs");
include!("render_wasm_p3.rs");
