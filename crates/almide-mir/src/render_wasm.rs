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

use crate::{CallArg, Init, IntOp, MirFunction, MirProgram, Op, PrimKind, Repr, RtFn, ValueId};
use std::collections::{BTreeMap, BTreeSet};

// Fixed low-memory addresses (named — no raw literals in the emitted WAT logic).
const NWRITTEN_ADDR: u32 = 0; // i32 scratch for fd_write's bytes-written out-param
const IOVEC_ADDR: u32 = 8; // [buf: i32][len: i32]
const ITOA_TMP_ADDR: u32 = 32; // reversed-digit scratch (≤ 20 bytes)
const LABELS_ADDR: u32 = 64; // print labels (the data section)
const SCRATCH_ADDR: u32 = 512; // the line build buffer
const HEAP_BASE: u32 = 8192; // bump allocator start

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
    for op in &func.ops {
        body.push_str(&render_op(op, &label_off));
    }

    format!(
        "{preamble}{data}  (func $main {locals}\n{body}  )\n  (func (export \"_start\") (call $main))\n)\n",
        preamble = preamble(),
        data = data,
        locals = locals_decl,
        body = body,
    )
}

/// Render a whole MIR program (functions + `_start` → `main`) to a WAT module.
pub fn render_wasm_program(prog: &MirProgram) -> String {
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
    let funcs =
        prog.functions.iter().map(|f| render_wasm_fn(f, &label_off)).collect::<String>();
    // `main` is `Unit` (v0 rejects a non-`Unit` main — it must implement
    // `Termination`), so `_start` discards nothing: a void `(call $main)` matches.
    format!(
        "{preamble}{data}{funcs}  (func (export \"_start\") (call $main))\n)\n",
        preamble = preamble(),
    )
}

/// Render one MIR function with its signature (params, locals, result).
pub fn render_wasm_fn(func: &MirFunction, label_off: &BTreeMap<String, (u32, u32)>) -> String {
    let reprs = value_reprs_wasm(func);
    let params = func
        .params
        .iter()
        .map(|p| format!("(param {} {})", local(p.value), wasm_ty(p.repr)))
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
    let locals_decl = locals.join(" ");
    let mut body = String::new();
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
            _ => body.push_str(&render_op(op, label_off)),
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
        | Op::IntBinOp { dst, .. }
        | Op::Pure { dst, .. } => Some(*dst),
        Op::CallFn { dst, .. } | Op::Call { dst, .. } => *dst,
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
            Op::Const { dst } | Op::ConstInt { dst, .. } | Op::IntBinOp { dst, .. } => {
                m.insert(*dst, SCALAR_REPR);
            }
            // A prim result (a load, fd_write errno, or handle→address) is a scalar i64.
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
            _ => {}
        }
    }
    m
}

fn local(v: ValueId) -> String {
    format!("$v{}", v.0)
}

fn render_op(op: &Op, label_off: &BTreeMap<String, (u32, u32)>) -> String {
    match op {
        // A STRING literal — a heap block `[rc][len][cap][utf8 bytes...]` (same header
        // as a list; len/cap are BYTE counts). $alloc the block, set the header, store
        // each byte. Real DATA reproduced from the MIR (the un-defer, ③ exec slice).
        Op::Alloc { dst, init: Init::Str(string), .. } => {
            let bytes = string.as_bytes();
            let blen = bytes.len() as u32;
            // A String block is sized LIST-COMPATIBLY so the free-list reuses it: `cap` is
            // the ELEMENT count `ceil(blen / ELEM_SIZE)` (rounded up so the bytes fit), and
            // the allocation is `LIST_HEADER + cap*ELEM_SIZE` — exactly what the `$alloc`
            // reuse check recomputes from `cap`. `len` stays the BYTE length (what print
            // reads). Storing `cap = blen` (a byte count) made the reuse formula
            // `LIST_HEADER + blen*ELEM_SIZE` overshoot the real size, so freed String
            // blocks were never reclaimed and a String-allocating loop leaked → OOM.
            let cap_elems = blen.div_ceil(ELEM_SIZE);
            let total = LIST_HEADER + cap_elems * ELEM_SIZE;
            let mut s = format!(
                "    (local.set {d} (call $alloc (i32.const {total})))\n\
                 \x20   (i32.store (i32.add (local.get {d}) (i32.const {LIST_RC_OFFSET})) (i32.const {RC_INITIAL}))\n\
                 \x20   (i32.store (i32.add (local.get {d}) (i32.const {LIST_LEN_OFFSET})) (i32.const {blen}))\n\
                 \x20   (i32.store (i32.add (local.get {d}) (i32.const {LIST_CAP_OFFSET})) (i32.const {cap_elems}))\n",
                d = local(*dst),
            );
            for (i, b) in bytes.iter().enumerate() {
                let off = LIST_HEADER + i as u32;
                s.push_str(&format!(
                    "    (i32.store8 (i32.add (local.get {d}) (i32.const {off})) (i32.const {b}))\n",
                    d = local(*dst),
                ));
            }
            s
        }
        Op::Alloc { dst, init, .. } => {
            let elems: &[i64] = match init {
                Init::IntList(e) => e,
                Init::Opaque | Init::Str(_) => &[],
            };
            let len = elems.len() as u32;
            let cap = len + PUSH_HEADROOM;
            let mut s = format!(
                "    (local.set {d} (call $list_new (i32.const {len}) (i32.const {cap})))\n",
                d = local(*dst)
            );
            for (i, e) in elems.iter().enumerate() {
                s.push_str(&format!(
                    "    (call $list_set (local.get {d}) (i32.const {i}) (i64.const {e}))\n",
                    d = local(*dst)
                ));
            }
            s
        }
        // An alias SHARES the object and bumps its refcount (A1.3-render): dst and
        // src become two handles to the SAME block, rc += 1 — matching the cert's
        // Alias = +1 and exercising the proven rc machine on a shared cell (whereas
        // eager-copy kept every cell at 1). In-place mutation is guarded by cow.
        Op::Dup { dst, src } => format!(
            "    (local.set {d} (local.get {s}))\n    (call $rc_inc (local.get {s}))\n",
            d = local(*dst),
            s = local(*src)
        ),
        // A runtime call → a wasm `call` of the (bootstrap) runtime function.
        Op::Call { dst, func, args, .. } => render_call(*dst, func, args, label_off),
        Op::IntBinOp { dst, op, a, b } => {
            let args = format!("(local.get {}) (local.get {})", local(*a), local(*b));
            // A comparison yields an i32 0/1 → zero-extend to the i64 scalar model.
            let expr = match op {
                IntOp::Add => format!("(i64.add {args})"),
                IntOp::Sub => format!("(i64.sub {args})"),
                IntOp::Mul => format!("(i64.mul {args})"),
                IntOp::Div => format!("(i64.div_s {args})"),
                IntOp::Mod => format!("(i64.rem_s {args})"),
                IntOp::Lt => format!("(i64.extend_i32_u (i64.lt_s {args}))"),
                IntOp::Le => format!("(i64.extend_i32_u (i64.le_s {args}))"),
                IntOp::Gt => format!("(i64.extend_i32_u (i64.gt_s {args}))"),
                IntOp::Ge => format!("(i64.extend_i32_u (i64.ge_s {args}))"),
                IntOp::Eq => format!("(i64.extend_i32_u (i64.eq {args}))"),
                IntOp::Ne => format!("(i64.extend_i32_u (i64.ne {args}))"),
            };
            format!("    (local.set {d} {expr})\n", d = local(*dst))
        }
        Op::CallFn { dst, name, args, .. } => {
            let argstr = args.iter().map(render_arg_wasm).collect::<Vec<_>>().join(" ");
            match dst {
                Some(d) => format!("    (local.set {} (call ${name} {argstr}))\n", local(*d)),
                None => format!("    (call ${name} {argstr})\n"),
            }
        }
        // A release: decrement the refcount cell (RuntimeModel.v's rt_dec). The
        // `$rc_dec` primitive traps if the cell is already 0 — the double-free /
        // use-after-free sentinel. This is the byte the perceus V binds each
        // witness drop to (the leak-freedom realization on the artifact).
        Op::Drop { v } => format!("    (call $rc_dec (local.get {}))\n", local(*v)),
        // COPY-ON-WRITE before an in-place mutation (A1.3-render, refining
        // CowSafety.v): if the block is SHARED (rc > 1), clone it so the mutation
        // touches no alias. The `rc_dec` runs FIRST (rc 2→1 — the alias keeps the
        // original alive, so no temp is needed), then `list_copy` reads the
        // still-live original into a fresh uniquely-owned block. rc == 1 → no-op.
        Op::MakeUnique { v } => format!(
            "    (if (i32.gt_s (i32.load (i32.add (local.get {v}) (i32.const {rc}))) (i32.const 1))\n      (then\n        (call $rc_dec (local.get {v}))\n        (local.set {v} (call $list_copy (local.get {v})))))\n",
            v = local(*v),
            rc = LIST_RC_OFFSET
        ),
        // Still no-ops: Consume MOVES the reference out (the receiver releases it
        // later — no dec at THIS site); Const/Borrow/Pure touch no refcount.
        // A materialized integer constant: set the local to the immediate. (A
        // deferred `Const` renders to nothing — the local keeps the zero default.)
        Op::ConstInt { dst, value } => {
            format!("    (local.set {} (i64.const {value}))\n", local(*dst))
        }
        // A primitive-floor op, hand-mapped INLINE (no preamble func). The MIR is
        // i64-uniform; wrap to i32 at the wasm memory boundary, zero-extend a loaded /
        // returned i32 back to i64. This is the whole trusted floor for raw memory +
        // the fd_write host call — everything else (print_str) is Almide over it.
        Op::Prim { kind, dst, args } => {
            let w = |i: usize| format!("(i32.wrap_i64 (local.get {}))", local(args[i]));
            let body = match kind {
                PrimKind::Handle => format!("(i64.extend_i32_u (local.get {}))", local(args[0])),
                PrimKind::Load { width: 1 } => format!("(i64.extend_i32_u (i32.load8_u {}))", w(0)),
                PrimKind::Load { width: 4 } => format!("(i64.extend_i32_u (i32.load {}))", w(0)),
                PrimKind::Load { .. } => format!("(i64.load {})", w(0)),
                PrimKind::Store { width: 1 } => format!("(i32.store8 {} {})", w(0), w(1)),
                PrimKind::Store { width: 4 } => format!("(i32.store {} {})", w(0), w(1)),
                PrimKind::Store { .. } => format!("(i64.store {} (local.get {}))", w(0), local(args[1])),
                PrimKind::FdWrite => {
                    format!("(i64.extend_i32_u (call $fd_write {} {} {} {}))", w(0), w(1), w(2), w(3))
                }
            };
            match dst {
                Some(d) => format!("    (local.set {} {body})\n", local(*d)),
                None => format!("    {body}\n"),
            }
        }
        // A scalar reassignment of a stable local — the loop-carried state. Reads `src`,
        // writes the var's own local (reusing the same wasm local is legal: read then set).
        Op::SetLocal { local: l, src } => {
            format!("    (local.set {} (local.get {}))\n", local(*l), local(*src))
        }
        Op::Consume { .. }
        | Op::Borrow { .. }
        | Op::Const { .. }
        | Op::Pure { .. }
        // The if- and loop-markers are rendered STATEFULLY by render_wasm_fn (the
        // flat→nested wasm `if`/`else` and `block`/`loop`); render_op never sees them.
        | Op::IfThen { .. }
        | Op::Else { .. }
        | Op::EndIf { .. }
        | Op::LoopStart
        | Op::LoopBreakUnless { .. }
        | Op::LoopEnd => String::new(),
    }
}

fn render_arg_wasm(arg: &CallArg) -> String {
    match arg {
        CallArg::Handle(v) | CallArg::Scalar(v) => format!("(local.get {})", local(*v)),
        CallArg::Imm(n) => format!("(i64.const {n})"),
        CallArg::Label(l) => panic!("label arg {l:?} not valid for a user call"),
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

/// The fixed WAT runtime: WASI import, memory, bump allocator, list ops, integer
/// formatting, and line printing. Addresses are the named constants above.
fn preamble() -> String {
    format!(
        r#"(module
  (import "wasi_snapshot_preview1" "fd_write"
    (func $fd_write (param i32 i32 i32 i32) (result i32)))
  (memory (export "memory") 1)
  (global $bump (mut i32) (i32.const {HEAP_BASE}))
  ;; the free-list head (0 = empty) — physical reclamation (A1.2-render), the
  ;; realization of proofs/FreeList.v. A freed block is pushed here; $alloc reuses
  ;; the head when it is EXACTLY the requested size. The link is stored in the dead
  ;; LEN field (offset 4), NOT the rc cell (offset 0), so the rc cell stays 0 and
  ;; the $rc_dec double-free sentinel still fires on a re-release of a freed block.
  (global $freelist (mut i32) (i32.const 0))

  (func $alloc (param $n i32) (result i32)
    (local $p i32)
    ;; reuse the free-list head iff it is exactly n bytes (FreeList.alloc: a valid
    ;; allocation is the fresh frontier OR a block currently on the free-list).
    (if (i32.ne (global.get $freelist) (i32.const 0))
      (then
        (local.set $p (global.get $freelist))
        (if (i32.eq (i32.add (i32.const {LIST_HEADER})
                             (i32.mul (i32.load (i32.add (local.get $p) (i32.const {LIST_CAP_OFFSET})))
                                      (i32.const {ELEM_SIZE})))
                    (local.get $n))
          (then
            (global.set $freelist
              (i32.load (i32.add (local.get $p) (i32.const {LIST_LEN_OFFSET}))))
            (return (local.get $p))))))
    ;; else bump the frontier (a genuinely fresh block)
    (local.set $p (global.get $bump))
    (global.set $bump (i32.add (local.get $p) (local.get $n)))
    (local.get $p))

  (func $list_new (param $len i32) (param $cap i32) (result i32)
    (local $p i32)
    (local.set $p (call $alloc (i32.add (i32.const {LIST_HEADER})
                                        (i32.mul (local.get $cap) (i32.const {ELEM_SIZE})))))
    (i32.store (i32.add (local.get $p) (i32.const {LIST_RC_OFFSET})) (i32.const {RC_INITIAL}))
    (i32.store (i32.add (local.get $p) (i32.const {LIST_LEN_OFFSET})) (local.get $len))
    (i32.store (i32.add (local.get $p) (i32.const {LIST_CAP_OFFSET})) (local.get $cap))
    (local.get $p))

  ;; release one reference (RuntimeModel.v's rt_dec): trap if the cell is already
  ;; 0 (double-free / use-after-free sentinel), else decrement. At 0 the block is
  ;; FREED — returned to the free-list for physical reuse (A1.2-render, refining
  ;; FreeList.v). The link goes in the dead LEN field; the rc cell stays 0 so a
  ;; re-release of the freed block still hits the sentinel above.
  (func $rc_dec (param $p i32)
    (local $rc i32)
    (local.set $rc (i32.load (i32.add (local.get $p) (i32.const {LIST_RC_OFFSET}))))
    (if (i32.eqz (local.get $rc)) (then (unreachable)))
    (local.set $rc (i32.sub (local.get $rc) (i32.const 1)))
    (i32.store (i32.add (local.get $p) (i32.const {LIST_RC_OFFSET})) (local.get $rc))
    (if (i32.eqz (local.get $rc))
      (then
        (i32.store (i32.add (local.get $p) (i32.const {LIST_LEN_OFFSET})) (global.get $freelist))
        (global.set $freelist (local.get $p)))))

  ;; acquire one reference (RuntimeModel.v's rt_inc): the shared-Dup primitive
  ;; (A1.3-render). Realizes WasmRcDec.rc_inc_prog — proven to compute rt_inc.
  (func $rc_inc (param $p i32)
    (i32.store (i32.add (local.get $p) (i32.const {LIST_RC_OFFSET}))
               (i32.add (i32.load (i32.add (local.get $p) (i32.const {LIST_RC_OFFSET})))
                        (i32.const 1))))

  (func $elem_addr (param $list i32) (param $idx i32) (result i32)
    ;; SAFETY WALL: an out-of-range index would compute an address OUTSIDE the
    ;; block (idx < 0 below it, idx >= cap beyond it) and a $list_set there would
    ;; corrupt memory — the ownership checker accepts (it tracks RC, not bounds),
    ;; so this would be accept-but-unsafe. Trap instead, so OOB is a WALL (a
    ;; controlled halt), never silent corruption (the index-bounds memory-safety
    ;; gate; cap is the block's allocated slot count).
    (if (i32.or (i32.lt_s (local.get $idx) (i32.const 0))
                (i32.ge_s (local.get $idx)
                          (i32.load (i32.add (local.get $list) (i32.const {LIST_CAP_OFFSET})))))
      (then (unreachable)))
    (i32.add (i32.add (local.get $list) (i32.const {LIST_HEADER}))
             (i32.mul (local.get $idx) (i32.const {ELEM_SIZE}))))

  (func $list_set (param $list i32) (param $idx i32) (param $val i64)
    (i64.store (call $elem_addr (local.get $list) (local.get $idx)) (local.get $val)))

  (func $list_get (param $list i32) (param $idx i32) (result i64)
    (i64.load (call $elem_addr (local.get $list) (local.get $idx))))

  (func $list_len (param $list i32) (result i32)
    (i32.load (i32.add (local.get $list) (i32.const {LIST_LEN_OFFSET}))))

  (func $list_copy (param $src i32) (result i32)
    (local $len i32) (local $cap i32) (local $dst i32) (local $i i32)
    (local.set $len (i32.load (i32.add (local.get $src) (i32.const {LIST_LEN_OFFSET}))))
    (local.set $cap (i32.load (i32.add (local.get $src) (i32.const {LIST_CAP_OFFSET}))))
    (local.set $dst (call $list_new (local.get $len) (local.get $cap)))
    (local.set $i (i32.const 0))
    (block $done (loop $loop
      (br_if $done (i32.ge_s (local.get $i) (local.get $len)))
      (call $list_set (local.get $dst) (local.get $i)
                      (call $list_get (local.get $src) (local.get $i)))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br $loop)))
    (local.get $dst))

  (func $list_push (param $list i32) (param $val i64) (result i32)
    (local $len i32)
    (local.set $len (i32.load (i32.add (local.get $list) (i32.const {LIST_LEN_OFFSET}))))
    (call $list_set (local.get $list) (local.get $len) (local.get $val))
    (i32.store (i32.add (local.get $list) (i32.const {LIST_LEN_OFFSET}))
               (i32.add (local.get $len) (i32.const 1)))
    (local.get $list))

  ;; append the decimal digits of a non-negative i64 at $cur; return new cursor
  (func $itoa_append (param $cur i32) (param $v i64) (result i32)
    (local $n i32)
    (if (i64.eqz (local.get $v))
      (then
        (i32.store8 (local.get $cur) (i32.const {ASCII_ZERO}))
        (return (i32.add (local.get $cur) (i32.const 1)))))
    (local.set $n (i32.const 0))
    (block $ddone (loop $dloop
      (br_if $ddone (i64.eqz (local.get $v)))
      (i32.store8 (i32.add (i32.const {ITOA_TMP_ADDR}) (local.get $n))
                  (i32.add (i32.const {ASCII_ZERO})
                           (i32.wrap_i64 (i64.rem_u (local.get $v) (i64.const {DECIMAL_BASE})))))
      (local.set $n (i32.add (local.get $n) (i32.const 1)))
      (local.set $v (i64.div_u (local.get $v) (i64.const {DECIMAL_BASE})))
      (br $dloop)))
    (block $cdone (loop $cloop
      (br_if $cdone (i32.eqz (local.get $n)))
      (local.set $n (i32.sub (local.get $n) (i32.const 1)))
      (i32.store8 (local.get $cur)
                  (i32.load8_u (i32.add (i32.const {ITOA_TMP_ADDR}) (local.get $n))))
      (local.set $cur (i32.add (local.get $cur) (i32.const 1)))
      (br $cloop)))
    (local.get $cur))

  ;; print "<label>=<e0>,<e1>,...\n" to stdout
  (func $print_list (param $list i32) (param $lblptr i32) (param $lbllen i32)
    (local $cur i32) (local $i i32) (local $len i32)
    (local.set $cur (i32.const {SCRATCH_ADDR}))
    (local.set $i (i32.const 0))
    (block $lbldone (loop $lblloop
      (br_if $lbldone (i32.ge_s (local.get $i) (local.get $lbllen)))
      (i32.store8 (local.get $cur)
                  (i32.load8_u (i32.add (local.get $lblptr) (local.get $i))))
      (local.set $cur (i32.add (local.get $cur) (i32.const 1)))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br $lblloop)))
    (i32.store8 (local.get $cur) (i32.const {ASCII_EQUALS}))
    (local.set $cur (i32.add (local.get $cur) (i32.const 1)))
    (local.set $len (call $list_len (local.get $list)))
    (local.set $i (i32.const 0))
    (block $eldone (loop $elloop
      (br_if $eldone (i32.ge_s (local.get $i) (local.get $len)))
      ;; SAFETY WALL: appending an element writes up to a comma + 20 digits; if
      ;; that would cross HEAP_BASE (the line buffer's end), trap rather than
      ;; overflow the buffer into the heap (the print-buffer-overflow gate).
      (if (i32.gt_u (i32.add (local.get $cur) (i32.const {MAX_ELEM_PRINT_BYTES}))
                    (i32.const {HEAP_BASE}))
        (then (unreachable)))
      (if (i32.gt_s (local.get $i) (i32.const 0))
        (then
          (i32.store8 (local.get $cur) (i32.const {ASCII_COMMA}))
          (local.set $cur (i32.add (local.get $cur) (i32.const 1)))))
      (local.set $cur (call $itoa_append (local.get $cur)
                                         (call $list_get (local.get $list) (local.get $i))))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br $elloop)))
    (i32.store8 (local.get $cur) (i32.const {ASCII_NEWLINE}))
    (local.set $cur (i32.add (local.get $cur) (i32.const 1)))
    (i32.store (i32.const {IOVEC_ADDR}) (i32.const {SCRATCH_ADDR}))
    (i32.store (i32.add (i32.const {IOVEC_ADDR}) (i32.const {IOVEC_LEN_OFFSET}))
               (i32.sub (local.get $cur) (i32.const {SCRATCH_ADDR})))
    (drop (call $fd_write (i32.const {STDOUT_FD}) (i32.const {IOVEC_ADDR})
                          (i32.const {IOVS_COUNT}) (i32.const {NWRITTEN_ADDR}))))

  ;; print a scalar integer followed by a newline
  (func $print_int (param $v i64)
    (local $cur i32)
    (local.set $cur (call $itoa_append (i32.const {SCRATCH_ADDR}) (local.get $v)))
    (i32.store8 (local.get $cur) (i32.const {ASCII_NEWLINE}))
    (local.set $cur (i32.add (local.get $cur) (i32.const 1)))
    (i32.store (i32.const {IOVEC_ADDR}) (i32.const {SCRATCH_ADDR}))
    (i32.store (i32.add (i32.const {IOVEC_ADDR}) (i32.const {IOVEC_LEN_OFFSET}))
               (i32.sub (local.get $cur) (i32.const {SCRATCH_ADDR})))
    (drop (call $fd_write (i32.const {STDOUT_FD}) (i32.const {IOVEC_ADDR})
                          (i32.const {IOVS_COUNT}) (i32.const {NWRITTEN_ADDR}))))

"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{verify_ownership, MirParam, MirProgram, ScalarWidth, PLACEHOLDER_LAYOUT};
    use std::process::Command;

    fn heap() -> Repr {
        Repr::Ptr { layout: PLACEHOLDER_LAYOUT }
    }

    /// Same program as the Rust-side test: `fn add(a,b)=a+b` + a `main` calling
    /// it. Both targets must print the same `5` — the dual-renderer thesis for
    /// USER FUNCTIONS (the mechanism that lets the runtime be self-hosted).
    fn add_program() -> MirProgram {
        let scalar = Repr::Scalar { width: ScalarWidth::Double };
        let add = MirFunction {
            name: "add".into(),
            params: vec![
                MirParam { value: ValueId(0), repr: scalar },
                MirParam { value: ValueId(1), repr: scalar },
            ],
            ops: vec![Op::IntBinOp {
                dst: ValueId(2),
                op: IntOp::Add,
                a: ValueId(0),
                b: ValueId(1),
            }],
            ret: Some(ValueId(2)),
            ..Default::default()
        };
        let main = MirFunction {
            name: "main".into(),
            params: vec![],
            ops: vec![
                Op::CallFn {
                    dst: Some(ValueId(0)),
                    name: "add".into(),
                    args: vec![CallArg::Imm(2), CallArg::Imm(3)],
                result: None },
                Op::Call { dst: None, func: RtFn::PrintInt, args: vec![CallArg::Scalar(ValueId(0))] , result: None },
            ],
            ret: None,
            ..Default::default()
        };
        MirProgram { functions: vec![add, main] }
    }

    #[test]
    fn function_call_lowers_and_runs_on_wasm() {
        let prog = add_program();
        if let Some(out) = build_and_run("fncall", &render_wasm_program(&prog)) {
            assert_eq!(out, "5");
        }
    }

    /// A HEAP-returning user call (`fn mk() -> String = "hi"`): the result is a Ptr
    /// (an i32 `$alloc` handle), so the caller's local must be typed i32 — NOT the
    /// scalar i64 default. Regression for `value_reprs_wasm` reading the call op's
    /// `result` repr; typing it i64 made `local.set` reject `$mk`'s i32 handle. Also
    /// pins the `Init::Str` un-defer: the literal's bytes are materialized.
    fn heap_call_program() -> MirProgram {
        let mk = MirFunction {
            name: "mk".into(),
            params: vec![],
            ops: vec![Op::Alloc { dst: ValueId(0), repr: heap(), init: Init::Str("hi".into()) }],
            ret: Some(ValueId(0)),
            ..Default::default()
        };
        let main = MirFunction {
            name: "main".into(),
            params: vec![],
            ops: vec![
                Op::CallFn {
                    dst: Some(ValueId(0)),
                    name: "mk".into(),
                    args: vec![],
                    result: Some(heap()),
                },
                // The returned object (rc 1) is released here — ownership-balanced.
                Op::Drop { v: ValueId(0) },
            ],
            ret: None,
            ..Default::default()
        };
        MirProgram { functions: vec![mk, main] }
    }

    #[test]
    fn heap_returning_call_types_result_as_i32_handle() {
        let prog = heap_call_program();
        let wat = render_wasm_program(&prog);
        // The fix: the call-result local is an i32 handle, never the scalar i64.
        assert!(wat.contains("(local $v0 i32)"), "call-result local must be i32:\n{wat}");
        assert!(!wat.contains("(local $v0 i64)"), "call-result local must NOT be i64:\n{wat}");
        // The Init::Str un-defer materialized the literal's bytes: 'h'=104, 'i'=105.
        assert!(
            wat.contains("(i32.const 104)") && wat.contains("(i32.const 105)"),
            "Init::Str bytes not materialized:\n{wat}"
        );
        // End-to-end: validates, runs clean (exit 0, no output) where wasmtime exists.
        if let Some(out) = build_and_run("heapcall", &wat) {
            assert_eq!(out, "");
        }
    }

    /// Lower real `.almd` SOURCE to a `MirProgram` through the existing frontend
    /// feeder (the same cut point as `examples/render_program.rs`) — for end-to-end
    /// tests over REAL lowering rather than hand-built MIR. Dev-only deps.
    fn lower_source(src: &str) -> MirProgram {
        use almide_frontend::check::Checker;
        use almide_frontend::lower::lower_program;
        use almide_frontend::{canonicalize, ir_link};
        use almide_lang::lexer::Lexer;
        use almide_lang::parser::Parser;
        use almide_optimize::{mono, optimize};
        let tokens = Lexer::tokenize(src);
        let mut prog = Parser::new(tokens).parse().expect("parse");
        let canon = canonicalize::canonicalize_program(&prog, std::iter::empty());
        let mut checker = Checker::from_env(canon.env);
        let _ = checker.infer_program(&mut prog);
        let mut ir = lower_program(&prog, &checker.env, &checker.type_map);
        optimize::optimize_program(&mut ir);
        mono::monomorphize(&mut ir);
        ir_link::ir_link(&mut ir);
        let mut globals: std::collections::HashMap<almide_ir::VarId, almide_lang::types::Ty> =
            std::collections::HashMap::new();
        for tl in &ir.top_lets {
            globals.insert(tl.var, tl.ty.clone());
        }
        for m in &ir.modules {
            for tl in &m.top_lets {
                globals.insert(tl.var, tl.ty.clone());
            }
        }
        let mut functions: Vec<MirFunction> =
            ir.functions.iter().filter_map(|f| crate::lower::lower_function(f, &globals).ok()).collect();
        // Auto-link the self-hosted print_str runtime (the v1 linker step) so a plain
        // `println(…)` program — which lowers to a PrintStr → `(call $print_str)` —
        // resolves, matching how render_program links it. Skip if already defined.
        if !functions.iter().any(|f| f.name == "print_str") {
            let rt = lower_source(include_str!("../../../stdlib/print_str.almd"));
            functions.extend(rt.functions);
        }
        MirProgram { functions }
    }

    /// A scalar-result user call (`let _r = add(2, 3)`) lowered from REAL source is an
    /// EXECUTABLE `CallFn` — immediate args + a bound scalar result — not the pre-
    /// execution `Const` + empty elided marker. Regression for `try_lower_scalar_call`
    /// (the scalar-call execution slice). The swap is adversarially-verified SOUND: the
    /// real CallFn replaces the marker 1:1 (same callee NAME → caps fold unchanged) and
    /// a scalar result registers no ownership object.
    #[test]
    fn scalar_user_call_lowers_to_executable_callfn() {
        let prog = lower_source(
            "fn add(a: Int, b: Int) -> Int = a + b\nfn main() -> Unit = { let _r = add(2, 3) }\n",
        );
        let main = prog.functions.iter().find(|f| f.name == "main").expect("main lowered");
        // A real CallFn to `add`: a bound result repr + both literal args as immediates.
        let (args, result) = main
            .ops
            .iter()
            .find_map(|op| match op {
                Op::CallFn { dst: Some(_), name, args, result } if name == "add" => {
                    Some((args.clone(), *result))
                }
                _ => None,
            })
            .expect("a real CallFn to add with a bound dst");
        assert!(result.is_some(), "the scalar call result repr must be set");
        assert!(
            args.iter().any(|a| matches!(a, CallArg::Imm(2)))
                && args.iter().any(|a| matches!(a, CallArg::Imm(3))),
            "both literal args must be immediates, got {args:?}"
        );
        // ...and NOT also an empty elided marker for add (the call is real, not elided).
        assert!(
            !main.ops.iter().any(|op| matches!(op,
                Op::CallFn { dst: None, name, args, .. } if name == "add" && args.is_empty())),
            "add must not also appear as an empty elided caps marker"
        );
        // End-to-end: renders to a valid module that runs cleanly (where wasmtime is present).
        if let Some(out) = build_and_run("scalar_user_call", &render_wasm_program(&prog)) {
            assert_eq!(out, "");
        }
    }

    /// An `Int` literal in value position materializes its REAL value (`Op::ConstInt`
    /// → `(local.set $dst (i64.const v))`), not the deferred-`Const` zero — the
    /// scalar-value foundation that lets a self-hosted runtime fn compute real
    /// addresses/lengths. Regression: lowering emits ConstInt, render emits the const.
    #[test]
    fn int_literal_materializes_its_value() {
        let prog = lower_source(
            "fn answer() -> Int = 42\nfn main() -> Unit = { let _a = answer() }\n",
        );
        let answer = prog.functions.iter().find(|f| f.name == "answer").expect("answer lowered");
        assert!(
            answer.ops.iter().any(|op| matches!(op, Op::ConstInt { value: 42, .. })),
            "answer must materialize 42 via ConstInt, got {:?}",
            answer.ops
        );
        let wat = render_wasm_program(&prog);
        assert!(wat.contains("(i64.const 42)"), "render must emit the constant:\n{wat}");
        if let Some(out) = build_and_run("int_literal", &wat) {
            assert_eq!(out, "");
        }
    }

    /// Scalar `Int` arithmetic COMPUTES (`fn add(a, b) = a + b` → `IntBinOp{Add}`,
    /// rendered `i64.add` over the param locals) — not the deferred-Const zero. With
    /// the literal-materialization above, this is the foundation a self-hosted runtime
    /// fn needs to compute real addresses (`s + LIST_HEADER`). The add_program test
    /// already proves `i64.add` returns the right value end-to-end.
    #[test]
    fn scalar_arithmetic_computes_via_intbinop() {
        let prog = lower_source(
            "fn add(a: Int, b: Int) -> Int = a + b\nfn main() -> Unit = { let _r = add(2, 3) }\n",
        );
        let add = prog.functions.iter().find(|f| f.name == "add").expect("add lowered");
        assert!(
            add.ops.iter().any(|op| matches!(op, Op::IntBinOp { op: IntOp::Add, .. })),
            "add must compute a+b via IntBinOp, got {:?}",
            add.ops
        );
        let wat = render_wasm_program(&prog);
        assert!(wat.contains("(i64.add"), "render must emit i64.add:\n{wat}");
        if let Some(out) = build_and_run("scalar_arith", &wat) {
            assert_eq!(out, "");
        }
    }

    /// THE PRIM-FLOOR PROOF (sub-slice 1): a hand-built `print_str` MirFunction —
    /// written ENTIRELY over the prim floor (handle / load / store / fd_write) + the
    /// scalar-value foundation (ConstInt / IntBinOp) — reads a heap String's bytes and
    /// writes them + a newline to stdout via a 2-element iovec `fd_write`. main allocs
    /// "hi" and calls it. This proves the prim ops render to valid wasm and ACTUALLY
    /// PRINT, with NO new preamble runtime func (the discipline) — the whole mechanism
    /// for self-hosted print, validated in isolation before the frontend `prim` module.
    #[test]
    fn prim_floor_print_str_prints() {
        // print_str(s: String): writes the string's bytes + "\n" to stdout.
        let print_str = MirFunction {
            name: "print_str".into(),
            params: vec![MirParam { value: ValueId(0), repr: heap() }],
            ops: vec![
                // h = prim.handle(s)  — the block's i64 address
                Op::Prim { kind: PrimKind::Handle, dst: Some(ValueId(1)), args: vec![ValueId(0)] },
                // len = prim.load32(h + 4)   (LIST_LEN_OFFSET)
                Op::ConstInt { dst: ValueId(2), value: 4 },
                Op::IntBinOp { dst: ValueId(3), op: IntOp::Add, a: ValueId(1), b: ValueId(2) },
                Op::Prim { kind: PrimKind::Load { width: 4 }, dst: Some(ValueId(4)), args: vec![ValueId(3)] },
                // data = h + 12   (LIST_HEADER)
                Op::ConstInt { dst: ValueId(5), value: 12 },
                Op::IntBinOp { dst: ValueId(6), op: IntOp::Add, a: ValueId(1), b: ValueId(5) },
                // iovec[0] = { ptr=data @ 8, len @ 12 }
                Op::ConstInt { dst: ValueId(7), value: 8 },
                Op::Prim { kind: PrimKind::Store { width: 4 }, dst: None, args: vec![ValueId(7), ValueId(6)] },
                Op::ConstInt { dst: ValueId(8), value: 12 },
                Op::Prim { kind: PrimKind::Store { width: 4 }, dst: None, args: vec![ValueId(8), ValueId(4)] },
                // "\n" (10) at scratch 512; iovec[1] = { ptr=512 @ 16, len=1 @ 20 }
                Op::ConstInt { dst: ValueId(9), value: 512 },
                Op::ConstInt { dst: ValueId(10), value: 10 },
                Op::Prim { kind: PrimKind::Store { width: 1 }, dst: None, args: vec![ValueId(9), ValueId(10)] },
                Op::ConstInt { dst: ValueId(11), value: 16 },
                Op::ConstInt { dst: ValueId(12), value: 512 },
                Op::Prim { kind: PrimKind::Store { width: 4 }, dst: None, args: vec![ValueId(11), ValueId(12)] },
                Op::ConstInt { dst: ValueId(13), value: 20 },
                Op::ConstInt { dst: ValueId(14), value: 1 },
                Op::Prim { kind: PrimKind::Store { width: 4 }, dst: None, args: vec![ValueId(13), ValueId(14)] },
                // fd_write(stdout=1, iovec@8, count=2, nwritten@0)
                Op::ConstInt { dst: ValueId(15), value: 1 },
                Op::ConstInt { dst: ValueId(16), value: 8 },
                Op::ConstInt { dst: ValueId(17), value: 2 },
                Op::ConstInt { dst: ValueId(18), value: 0 },
                Op::Prim {
                    kind: PrimKind::FdWrite,
                    dst: Some(ValueId(19)),
                    args: vec![ValueId(15), ValueId(16), ValueId(17), ValueId(18)],
                },
            ],
            ret: None,
            declared_caps: vec![crate::Capability::Stdout],
        };
        let main = MirFunction {
            name: "main".into(),
            params: vec![],
            ops: vec![
                Op::Alloc { dst: ValueId(0), repr: heap(), init: Init::Str("hi".into()) },
                Op::CallFn {
                    dst: None,
                    name: "print_str".into(),
                    args: vec![CallArg::Handle(ValueId(0))],
                    result: None,
                },
                Op::Drop { v: ValueId(0) },
            ],
            ret: None,
            ..Default::default()
        };
        let prog = MirProgram { functions: vec![print_str, main] };
        // The prim ops render to valid wasm and print "hi\n" (trimmed to "hi").
        if let Some(out) = build_and_run("prim_print_str", &render_wasm_program(&prog)) {
            assert_eq!(out, "hi");
        }
    }

    /// THE OBSERVABILITY KEYSTONE (sub-slice 2+3): `println("hello")` from SOURCE runs
    /// through v1's SELF-HOSTED print_str — written in ALMIDE over the `prim` floor
    /// (prim.handle/load32/store*/fd_write → Op::Prim, mapped from the bundled `prim`
    /// module), compiled through v1's own lower→MIR→render pipeline — and prints
    /// "hello", byte-matching v0's native println. NO hand-written WAT growth (the
    /// discipline). The self-host runtime vision, realized for print.
    #[test]
    fn selfhosted_print_str_from_source_prints() {
        let src = "fn print_str(s: String) -> Unit = {\n  \
            let h = prim.handle(s)\n  \
            let len = prim.load32(h + 4)\n  \
            let data = h + 12\n  \
            prim.store32(8, data)\n  \
            prim.store32(12, len)\n  \
            prim.store8(512, 10)\n  \
            prim.store32(16, 512)\n  \
            prim.store32(20, 1)\n  \
            let _w = prim.fd_write(1, 8, 2, 0)\n\
            }\n\
            fn main() -> Unit = println(\"hello\")\n";
        let prog = lower_source(src);
        // print_str lowered to real prim-floor ops (not the deferred Const).
        let ps = prog.functions.iter().find(|f| f.name == "print_str").expect("print_str lowered");
        assert!(
            ps.ops.iter().any(|op| matches!(op, Op::Prim { kind: PrimKind::FdWrite, .. })),
            "print_str must reach Op::Prim FdWrite from source, got {:?}",
            ps.ops
        );
        // End-to-end: it prints "hello" (matching v0's native println).
        if let Some(out) = build_and_run("selfhost_println", &render_wasm_program(&prog)) {
            assert_eq!(out, "hello");
        }
    }

    /// SEAMLESS v1=v0: a PLAIN `println` program — byte-identical to what runs on v0,
    /// with NO print_str defined — works on v1 because the self-hosted print_str
    /// runtime is AUTO-LINKED (the v1 linker step). Two printlns include the newline
    /// between them (print_str writes string + "\n" via two single-iovec fd_writes).
    #[test]
    fn plain_println_auto_links_and_prints() {
        let prog = lower_source(
            "fn main() -> Unit = {\n  println(\"line one\")\n  println(\"line two\")\n}\n",
        );
        // print_str was auto-linked (the source did not define it).
        assert!(
            prog.functions.iter().any(|f| f.name == "print_str"),
            "the self-hosted print_str must be auto-linked"
        );
        // build_and_run trims the trailing newline; the MIDDLE newline must remain.
        if let Some(out) = build_and_run("plain_println", &render_wasm_program(&prog)) {
            assert_eq!(out, "line one\nline two");
        }
    }

    /// CONTROL-FLOW EXECUTION + print_int: a recursive itoa (`put_int`) over a SCALAR
    /// `if` — lowered to IfThen/Else/EndIf so ONLY THE TAKEN ARM runs (Div/Mod +
    /// recursion + prim) — prints an integer's decimal digits, byte-matching v0's
    /// `println(int.to_string(12345))`. Proves the `if` EXECUTES (not the old
    /// linearize-both-arms-and-defer), the keystone for control flow + numbers.
    #[test]
    fn scalar_if_executes_print_int() {
        let src = "fn put_int(n: Int, pos: Int) -> Int =\n  \
            if n < 10 then { prim.store8(pos, 48 + n)\n    \
            pos + 1 } else { let p = put_int(n / 10, pos)\n    \
            prim.store8(p, 48 + (n % 10))\n    \
            p + 1 }\n\
            fn write_int(n: Int) -> Unit = { let endp = put_int(n, 512)\n  \
            prim.store8(endp, 10)\n  \
            prim.store32(8, 512)\n  \
            prim.store32(12, endp - 512 + 1)\n  \
            let _w = prim.fd_write(1, 8, 1, 0) }\n\
            fn main() -> Unit = write_int(12345)\n";
        let prog = lower_source(src);
        let put = prog.functions.iter().find(|f| f.name == "put_int").expect("put_int lowered");
        // The `if` is EXECUTABLE control flow (IfThen marker), not the deferred Const.
        assert!(
            put.ops.iter().any(|op| matches!(op, Op::IfThen { .. })),
            "put_int's if must lower to IfThen (executable), got {:?}",
            put.ops
        );
        if let Some(out) = build_and_run("print_int", &render_wasm_program(&prog)) {
            assert_eq!(out, "12345");
        }
    }

    /// FIZZBUZZ — the canonical real program — runs through v1 and byte-matches v0.
    /// Exercises EVERYTHING composed: a chained Unit `if … else if … else …` that
    /// executes ONLY THE TAKEN branch (not all arms), `%`, `==`, comparison, recursion
    /// (write_int → put_int), Div/Mod, the prim floor, println, and self-hosted
    /// print_int. `fizzbuzz(6)` is "Fizz" — proving the nested else-if executes (the
    /// old linearization would print "Fizz\nBuzz\n6").
    #[test]
    fn fizzbuzz_matches_v0() {
        let src = "fn put_int(n: Int, pos: Int) -> Int =\n  \
            if n < 10 then { prim.store8(pos, 48 + n)\n    pos + 1 }\n  \
            else { let p = put_int(n / 10, pos)\n    prim.store8(p, 48 + (n % 10))\n    p + 1 }\n\
            fn write_int(n: Int) -> Unit = { let endp = put_int(n, 512)\n  \
            prim.store8(endp, 10)\n  prim.store32(8, 512)\n  \
            prim.store32(12, endp - 512 + 1)\n  let _w = prim.fd_write(1, 8, 1, 0) }\n\
            fn fizzbuzz(n: Int) -> Unit =\n  \
            if n % 15 == 0 then println(\"FizzBuzz\")\n  \
            else if n % 3 == 0 then println(\"Fizz\")\n  \
            else if n % 5 == 0 then println(\"Buzz\")\n  \
            else write_int(n)\n\
            fn main() -> Unit = fizzbuzz(6)\n";
        let prog = lower_source(src);
        // Only the taken branch runs: fizzbuzz(6) prints exactly "Fizz" (6 % 3 == 0).
        if let Some(out) = build_and_run("fizzbuzz", &render_wasm_program(&prog)) {
            assert_eq!(out, "Fizz");
        }
    }

    #[test]
    fn heap_result_if_returns_the_taken_arm_string() {
        // `if c then "yes" else "no"` RETURNS a String — only the taken arm allocates,
        // returned rc=1 to the caller (per-arm Alloc+Consume balance). label(true)="yes",
        // label(false)="no", byte-matching v0.
        let src = "fn label(c: Bool) -> String = if c then \"yes\" else \"no\"\n\
            fn main() -> Unit = {\n  \
            println(label(true))\n  println(label(false)) }\n";
        let prog = lower_source(src);
        let f = prog.functions.iter().find(|f| f.name == "label").unwrap();
        // It must EXECUTE (IfThen marker), not defer to a single Opaque Alloc.
        assert!(
            f.ops.iter().any(|op| matches!(op, Op::IfThen { .. })),
            "heap-result if must lower to IfThen (executable), got {:?}",
            f.ops
        );
        if let Some(out) = build_and_run("heap_result_if", &render_wasm_program(&prog)) {
            assert_eq!(out, "yes\nno");
        }
    }

    #[test]
    fn string_allocating_loop_reuses_freed_blocks() {
        // A loop that allocates a String literal every iteration must run in BOUNDED
        // memory — each iteration's string is freed (rc_dec) and the free-list REUSES it.
        // 5000 iterations × a 20-byte block would overrun the single 64 KiB page (~2900
        // allocs) if freed String blocks were not reclaimed; before the list-compatible
        // String sizing fix that OOM-trapped. Completing all 5000 lines proves reuse.
        let src = "fn main() -> Unit = {\n  \
            var i = 0\n  \
            while i < 5000 {\n    println(\"x\")\n    i = i + 1\n  } }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("str_loop", &render_wasm_program(&prog)) {
            assert_eq!(out.lines().count(), 5000, "every iteration must print (no OOM)");
            assert!(out.lines().all(|l| l == "x"));
        }
    }

    #[test]
    fn heap_result_match_returns_the_matched_arm_string() {
        // A String-returning `match` over Int literals desugars to a NESTED heap-result
        // `if` and RUNS only the matched arm (each arm Alloc+Consume = "im"). name(0)=zero,
        // name(1)=one, name(7)=other, byte-matching v0.
        let src = "fn name(n: Int) -> String = match n {\n  \
            0 => \"zero\",\n  1 => \"one\",\n  _ => \"other\",\n  }\n\
            fn main() -> Unit = {\n  \
            println(name(0))\n  println(name(1))\n  println(name(7)) }\n";
        let prog = lower_source(src);
        let f = prog.functions.iter().find(|f| f.name == "name").unwrap();
        assert!(
            f.ops.iter().any(|op| matches!(op, Op::IfThen { .. })),
            "heap-result match must lower to nested IfThen (executable), got {:?}",
            f.ops
        );
        if let Some(out) = build_and_run("heap_result_match", &render_wasm_program(&prog)) {
            assert_eq!(out, "zero\none\nother");
        }
    }

    #[test]
    fn match_unit_executes_only_matched_arm() {
        // A Unit `match` over Int literal patterns (+ a `_` catch-all) desugars to a
        // nested `if n == lit then … else …` and EXECUTES: only the matched arm's
        // println runs — byte-identical to v0's match.
        let src = "fn classify(n: Int) -> Unit = match n {\n  \
            0 => println(\"zero\"),\n  \
            1 => println(\"one\"),\n  \
            _ => println(\"other\"),\n  \
            }\n\
            fn main() -> Unit = {\n  \
            classify(0)\n  classify(1)\n  classify(7) }\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("match_unit", &render_wasm_program(&prog)) {
            assert_eq!(out, "zero\none\nother");
        }
    }

    #[test]
    fn scalar_while_loop_runs_n_times() {
        // A real `while i < n { … i = i + 1 }` EXECUTES N iterations (LoopStart/
        // LoopBreakUnless/LoopEnd markers + SetLocal carry the counter) — count_to(4)
        // prints 0..3, byte-matching v0. The string-free body keeps it scalar-state.
        let src = "fn put_int(n: Int, pos: Int) -> Int =\n  \
            if n < 10 then { prim.store8(pos, 48 + n)\n    pos + 1 }\n  \
            else { let p = put_int(n / 10, pos)\n    prim.store8(p, 48 + (n % 10))\n    p + 1 }\n\
            fn write_int(n: Int) -> Unit = { let endp = put_int(n, 512)\n  \
            prim.store8(endp, 10)\n  prim.store32(8, 512)\n  \
            prim.store32(12, endp - 512 + 1)\n  let _w = prim.fd_write(1, 8, 1, 0) }\n\
            fn count_to(n: Int) -> Unit = {\n  \
            var i = 0\n  \
            while i < n {\n    write_int(i)\n    i = i + 1\n  } }\n\
            fn main() -> Unit = count_to(4)\n";
        let prog = lower_source(src);
        // The loop must lower to REAL markers (executes), not the deferred one-iteration form.
        let count_fn = prog.functions.iter().find(|f| f.name == "count_to").unwrap();
        assert!(
            count_fn.ops.iter().any(|op| matches!(op, Op::LoopStart)),
            "count_to's while must lower to LoopStart (executable), got {:?}",
            count_fn.ops
        );
        if let Some(out) = build_and_run("scalar_while", &render_wasm_program(&prog)) {
            assert_eq!(out, "0\n1\n2\n3");
        }
    }

    #[test]
    fn while_loop_accumulates_via_counter() {
        // The loop-carried scalar state truly accumulates: sum 1+2+3+4+5 = 15, computed
        // in the loop and printed once after it. Verifies SetLocal threads `total`/`i`
        // across iterations (not a single modelled iteration).
        let src = "fn put_int(n: Int, pos: Int) -> Int =\n  \
            if n < 10 then { prim.store8(pos, 48 + n)\n    pos + 1 }\n  \
            else { let p = put_int(n / 10, pos)\n    prim.store8(p, 48 + (n % 10))\n    p + 1 }\n\
            fn write_int(n: Int) -> Unit = { let endp = put_int(n, 512)\n  \
            prim.store8(endp, 10)\n  prim.store32(8, 512)\n  \
            prim.store32(12, endp - 512 + 1)\n  let _w = prim.fd_write(1, 8, 1, 0) }\n\
            fn sum_to(n: Int) -> Unit = {\n  \
            var i = 1\n  var total = 0\n  \
            while i <= n {\n    total = total + i\n    i = i + 1\n  }\n  \
            write_int(total) }\n\
            fn main() -> Unit = sum_to(5)\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("while_sum", &render_wasm_program(&prog)) {
            assert_eq!(out, "15");
        }
    }

    #[test]
    fn nested_while_loops_use_distinct_labels() {
        // Two nested loops exercise the per-loop label ids ($brk0/$cont0 vs $brk1/$cont1)
        // and the inner counter reset each outer iteration. grid(2,3) walks r*3+c = 0..5.
        let src = "fn put_int(n: Int, pos: Int) -> Int =\n  \
            if n < 10 then { prim.store8(pos, 48 + n)\n    pos + 1 }\n  \
            else { let p = put_int(n / 10, pos)\n    prim.store8(p, 48 + (n % 10))\n    p + 1 }\n\
            fn write_int(n: Int) -> Unit = { let endp = put_int(n, 512)\n  \
            prim.store8(endp, 10)\n  prim.store32(8, 512)\n  \
            prim.store32(12, endp - 512 + 1)\n  let _w = prim.fd_write(1, 8, 1, 0) }\n\
            fn grid(rows: Int, cols: Int) -> Unit = {\n  \
            var r = 0\n  \
            while r < rows {\n    \
            var c = 0\n    \
            while c < cols {\n      write_int(r * cols + c)\n      c = c + 1\n    }\n    \
            r = r + 1\n  } }\n\
            fn main() -> Unit = grid(2, 3)\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("nested_while", &render_wasm_program(&prog)) {
            assert_eq!(out, "0\n1\n2\n3\n4\n5");
        }
    }

    #[test]
    fn for_in_exclusive_range_executes_each_step() {
        // `for i in 0..n` desugars to the while machinery and RUNS each step: the index
        // is a fresh, mutable local stepped by 1, the end snapshot once. print_range(4)
        // prints 0,1,2,3 (exclusive), byte-matching v0.
        let src = "fn put_int(n: Int, pos: Int) -> Int =\n  \
            if n < 10 then { prim.store8(pos, 48 + n)\n    pos + 1 }\n  \
            else { let p = put_int(n / 10, pos)\n    prim.store8(p, 48 + (n % 10))\n    p + 1 }\n\
            fn write_int(n: Int) -> Unit = { let endp = put_int(n, 512)\n  \
            prim.store8(endp, 10)\n  prim.store32(8, 512)\n  \
            prim.store32(12, endp - 512 + 1)\n  let _w = prim.fd_write(1, 8, 1, 0) }\n\
            fn print_range(n: Int) -> Unit = {\n  \
            for i in 0..n {\n    write_int(i)\n  } }\n\
            fn main() -> Unit = print_range(4)\n";
        let prog = lower_source(src);
        let f = prog.functions.iter().find(|f| f.name == "print_range").unwrap();
        assert!(
            f.ops.iter().any(|op| matches!(op, Op::LoopStart)),
            "for-in must lower to LoopStart (executable), got {:?}",
            f.ops
        );
        if let Some(out) = build_and_run("for_range", &render_wasm_program(&prog)) {
            assert_eq!(out, "0\n1\n2\n3");
        }
    }

    #[test]
    fn for_in_inclusive_range_includes_end() {
        // `for i in 1..=n` is INCLUSIVE (i <= n): sum_range(5) accumulates 1+2+3+4+5 = 15,
        // proving the index threads through and the inclusive bound includes `n`.
        let src = "fn put_int(n: Int, pos: Int) -> Int =\n  \
            if n < 10 then { prim.store8(pos, 48 + n)\n    pos + 1 }\n  \
            else { let p = put_int(n / 10, pos)\n    prim.store8(p, 48 + (n % 10))\n    p + 1 }\n\
            fn write_int(n: Int) -> Unit = { let endp = put_int(n, 512)\n  \
            prim.store8(endp, 10)\n  prim.store32(8, 512)\n  \
            prim.store32(12, endp - 512 + 1)\n  let _w = prim.fd_write(1, 8, 1, 0) }\n\
            fn sum_range(n: Int) -> Unit = {\n  \
            var total = 0\n  \
            for i in 1..=n {\n    total = total + i\n  }\n  \
            write_int(total) }\n\
            fn main() -> Unit = sum_range(5)\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("for_incl", &render_wasm_program(&prog)) {
            assert_eq!(out, "15");
        }
    }

    #[test]
    fn match_scalar_value_selects_matched_arm() {
        // A scalar-result `match` over Int literals computes the matched arm's value
        // (here printed via the self-hosted itoa). pick(1) selects the `1 => 200` arm.
        let src = "fn put_int(n: Int, pos: Int) -> Int =\n  \
            if n < 10 then { prim.store8(pos, 48 + n)\n    pos + 1 }\n  \
            else { let p = put_int(n / 10, pos)\n    prim.store8(p, 48 + (n % 10))\n    p + 1 }\n\
            fn write_int(n: Int) -> Unit = { let endp = put_int(n, 512)\n  \
            prim.store8(endp, 10)\n  prim.store32(8, 512)\n  \
            prim.store32(12, endp - 512 + 1)\n  let _w = prim.fd_write(1, 8, 1, 0) }\n\
            fn pick(n: Int) -> Int = match n {\n  \
            0 => 100,\n  1 => 200,\n  _ => 999,\n  }\n\
            fn main() -> Unit = write_int(pick(1))\n";
        let prog = lower_source(src);
        if let Some(out) = build_and_run("match_scalar", &render_wasm_program(&prog)) {
            assert_eq!(out, "200");
        }
    }

    /// The hand-written WAT runtime is the BOOTSTRAP debt (§4.1). This guard
    /// makes the "never grow" rule MECHANICAL (not a comment): the count may only
    /// ratchet DOWN as the runtime self-hosts into Almide. If you added a
    /// hand-written WAT routine and this fails — STOP: write it in Almide and
    /// call it via `CallFn` instead. v0's wasm emitter rotted because nothing
    /// kept its hand-written surface small; this is that forcing function.
    /// The proven MEMORY-MODEL primitives in the preamble — the wasm realization
    /// of `proofs/RuntimeModel.v`'s `rt_inc`/`rt_dec`. A CLOSED set bounded by the
    /// PROOF (it grows only when the model gains an RC op), NOT by hand-mapping
    /// discipline, so the convergence guard accounts it SEPARATELY from the
    /// open-stdlib ratchet (§4.1): the trust spine's own core is not "another
    /// stdlib routine." The ratchet on the open surface stays exactly as strict.
    const RC_PRIMITIVE_FNS: &[&str] = &["$rc_dec", "$rc_inc"];

    #[test]
    fn handwritten_wasm_runtime_does_not_grow() {
        // The guard is SPLIT by principle: the proven memory-model primitives
        // (RC_PRIMITIVE_FNS — RuntimeModel.v's rt_inc/rt_dec) are a closed set
        // bounded by the PROOF, accounted separately; the OPEN stdlib surface is
        // what the convergence rule (§4.1) ratchets DOWN only.
        let pre = preamble();
        let total = pre.matches("\n  (func $").count();
        let rc_count =
            RC_PRIMITIVE_FNS.iter().filter(|n| pre.contains(&format!("\n  (func {n} "))).count();
        let stdlib_count = total - rc_count;
        // (a) The OPEN stdlib runtime surface — ratchet DOWN only, never raise.
        const BOOTSTRAP_RUNTIME_FN_BASELINE: usize = 11;
        assert!(
            stdlib_count <= BOOTSTRAP_RUNTIME_FN_BASELINE,
            "hand-written stdlib WAT runtime grew to {stdlib_count} funcs (baseline \
             {BOOTSTRAP_RUNTIME_FN_BASELINE}); §4.1 forbids growing it — self-host \
             the new routine in Almide and call it via CallFn"
        );
        // (b) The CLOSED proven-RC-primitive set — present as declared, no more.
        assert!(
            rc_count <= RC_PRIMITIVE_FNS.len(),
            "more RC primitive funcs ({rc_count}) than the proven closed set \
             ({}); an RC primitive must correspond to a RuntimeModel.v op",
            RC_PRIMITIVE_FNS.len()
        );
    }

    fn build_and_run(label: &str, wat: &str) -> Option<String> {
        let dir = std::env::temp_dir().join(format!("almide_mir_wasm_{label}"));
        std::fs::create_dir_all(&dir).unwrap();
        let wat_path = dir.join("m.wat");
        std::fs::write(&wat_path, wat).unwrap();
        match Command::new("wasmtime").arg("run").arg(&wat_path).output() {
            Ok(o) if o.status.code() != Some(127) => {
                assert!(
                    o.status.success(),
                    "wasmtime failed:\n{}\n--- wat ---\n{wat}",
                    String::from_utf8_lossy(&o.stderr)
                );
                Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
            }
            _ => None, // wasmtime unavailable → skip
        }
    }

    /// Run a WAT on wasmtime and report whether it exited cleanly. `None` =
    /// wasmtime unavailable (skip), `Some(true/false)` = ran and exited
    /// success/trap. Unlike `build_and_run` this does NOT assert success — it is
    /// for tests that EXPECT a trap (the double-free sentinel).
    fn run_status(label: &str, wat: &str) -> Option<bool> {
        let dir = std::env::temp_dir().join(format!("almide_mir_wasm_{label}"));
        std::fs::create_dir_all(&dir).unwrap();
        let wat_path = dir.join("m.wat");
        std::fs::write(&wat_path, wat).unwrap();
        match Command::new("wasmtime").arg("run").arg(&wat_path).output() {
            Ok(o) if o.status.code() != Some(127) => Some(o.status.success()),
            _ => None, // wasmtime unavailable → skip
        }
    }

    #[test]
    fn rc_dec_traps_on_double_free() {
        // The double-free CLASS — the one v0 bled on — is now TRAPPED on the real
        // bytes: a second release of an already-0 cell hits the `$rc_dec` sentinel
        // (`unreachable`). This is the runtime backstop for the safety the
        // ownership checker already proves statically.
        let double = format!(
            "{}{}",
            preamble(),
            "  (func $main (local $p i32)\n\
             \u{20}   (local.set $p (call $list_new (i32.const 0) (i32.const 1)))\n\
             \u{20}   (call $rc_dec (local.get $p))\n\
             \u{20}   (call $rc_dec (local.get $p)))\n\
             \u{20} (func (export \"_start\") (call $main))\n)\n"
        );
        if let Some(success) = run_status("doublefree", &double) {
            assert!(!success, "a double `rc_dec` must TRAP (the sentinel), got a clean exit");
        }
        // A SINGLE legitimate release (rc 1 → 0) must NOT trap — the sentinel
        // fires only on the already-freed cell, never on a valid free.
        let single = format!(
            "{}{}",
            preamble(),
            "  (func $main (local $p i32)\n\
             \u{20}   (local.set $p (call $list_new (i32.const 0) (i32.const 1)))\n\
             \u{20}   (call $rc_dec (local.get $p)))\n\
             \u{20} (func (export \"_start\") (call $main))\n)\n"
        );
        if let Some(success) = run_status("singlefree", &single) {
            assert!(success, "a single legitimate free must NOT trap");
        }
    }

    #[test]
    fn freelist_reuses_a_freed_block() {
        // A1.2-render: alloc p1, free p1 (-> the free-list), then alloc p2 of the
        // SAME size. p2 must REUSE p1's freed block (FreeList.alloc reusing a
        // free-list block), so memory is bounded under churn — AND the reused block
        // must be correctly USABLE (re-initialized by list_new, writable, readable).
        // Prints `1` (p1 == p2, reuse happened) then `2` (p2[1] read back) — if the
        // reused block were corrupted the read-back would be wrong.
        let wat = format!(
            "{}{}",
            preamble(),
            "  (func $main (local $p1 i32) (local $p2 i32)\n\
             \u{20}   (local.set $p1 (call $list_new (i32.const 3) (i32.const 3)))\n\
             \u{20}   (call $rc_dec (local.get $p1))\n\
             \u{20}   (local.set $p2 (call $list_new (i32.const 3) (i32.const 3)))\n\
             \u{20}   (call $list_set (local.get $p2) (i32.const 0) (i64.const 1))\n\
             \u{20}   (call $list_set (local.get $p2) (i32.const 1) (i64.const 2))\n\
             \u{20}   (call $list_set (local.get $p2) (i32.const 2) (i64.const 3))\n\
             \u{20}   (call $print_int (i64.extend_i32_s (i32.eq (local.get $p1) (local.get $p2))))\n\
             \u{20}   (call $print_int (call $list_get (local.get $p2) (i32.const 1))))\n\
             \u{20} (func (export \"_start\") (call $main))\n)\n"
        );
        if let Some(out) = build_and_run("reuse", &wat) {
            assert_eq!(out, "1\n2", "second alloc must REUSE the freed block AND be usable");
        }
    }

    #[test]
    fn rc_cell_values_match_the_interpreter_on_wasmtime() {
        // `WasmExec.run_g` PROVES (in Coq, on the grounded bytes): `$rc_inc` takes
        // the rc cell +1 (rt_inc), and a valid `$rc_dec` takes it 1→0 (leak-freedom).
        // Confirm the PRODUCTION engine (wasmtime) computes the same cell values on
        // the renderer's actual `$rc_inc`/`$rc_dec` — grounding the interpreter model
        // against the real engine, so the WasmExec residual shrinks from "trust run_g
        // matches the wasm spec" to "wasmtime matches the spec" (a trusted engine, the
        // same trust level as the wat2wasm byte grounding). `$list_new` inits rc to 1.
        let inc = format!(
            "{}{}",
            preamble(),
            "  (func $main (local $b i32)\n\
             \u{20}   (local.set $b (call $list_new (i32.const 0) (i32.const 1)))\n\
             \u{20}   (call $rc_inc (local.get $b))\n\
             \u{20}   (call $print_int (i64.extend_i32_s (i32.load (local.get $b)))))\n\
             \u{20} (func (export \"_start\") (call $main))\n)\n"
        );
        if let Some(out) = build_and_run("rcinc_cell", &inc) {
            assert_eq!(out, "2", "rc_inc: cell 1→2 (rt_inc) — wasmtime must match run_g");
        }
        let dec = format!(
            "{}{}",
            preamble(),
            "  (func $main (local $b i32)\n\
             \u{20}   (local.set $b (call $list_new (i32.const 0) (i32.const 1)))\n\
             \u{20}   (call $rc_dec (local.get $b))\n\
             \u{20}   (call $print_int (i64.extend_i32_s (i32.load (local.get $b)))))\n\
             \u{20} (func (export \"_start\") (call $main))\n)\n"
        );
        if let Some(out) = build_and_run("rcdec_cell", &dec) {
            assert_eq!(out, "0", "rc_dec: cell 1→0 (leak-freedom) — wasmtime must match run_g");
        }
    }

    #[test]
    fn out_of_bounds_index_traps() {
        // The index-bounds memory-safety WALL: a `$list_set` with idx >= cap would
        // write OUTSIDE the block and corrupt memory (and the ownership checker —
        // which tracks RC, not bounds — would ACCEPT it). `$elem_addr` now traps
        // instead, so OOB is a controlled halt, never silent corruption.
        let oob = format!(
            "{}{}",
            preamble(),
            "  (func $main (local $b i32)\n\
             \u{20}   (local.set $b (call $list_new (i32.const 0) (i32.const 1)))\n\
             \u{20}   (call $list_set (local.get $b) (i32.const 5) (i64.const 9)))\n\
             \u{20} (func (export \"_start\") (call $main))\n)\n"
        );
        if let Some(success) = run_status("oob_idx", &oob) {
            assert!(!success, "an out-of-bounds index must TRAP (the bounds wall), not corrupt memory");
        }
        // An in-bounds index (0 <= idx < cap) must NOT trap.
        let ok = format!(
            "{}{}",
            preamble(),
            "  (func $main (local $b i32)\n\
             \u{20}   (local.set $b (call $list_new (i32.const 0) (i32.const 1)))\n\
             \u{20}   (call $list_set (local.get $b) (i32.const 0) (i64.const 9)))\n\
             \u{20} (func (export \"_start\") (call $main))\n)\n"
        );
        if let Some(success) = run_status("inbounds_idx", &ok) {
            assert!(success, "an in-bounds index must not trap");
        }
    }

    fn value_semantics_mir() -> MirFunction {
        // var a = [1,2,3]; var b = a; a[0] = 9; print a; print b
        let (a, b) = (ValueId(0), ValueId(1));
        MirFunction {
            name: "main".into(),
            ops: vec![
                Op::Alloc { dst: a, repr: heap(), init: Init::IntList(vec![1, 2, 3]) },
                Op::Dup { dst: b, src: a },
                Op::MakeUnique { v: a },
                Op::Call {
                    dst: None,
                    func: RtFn::ListSet,
                    args: vec![CallArg::Handle(a), CallArg::Imm(0), CallArg::Imm(9)],
                result: None },
                Op::Call { dst: None, func: RtFn::PrintList, args: vec![CallArg::Handle(a), CallArg::Label("a".into())] , result: None },
                Op::Call { dst: None, func: RtFn::PrintList, args: vec![CallArg::Handle(b), CallArg::Label("b".into())] , result: None },
                Op::Drop { v: b },
                Op::Drop { v: a },
            ],
            ..Default::default()
        }
    }

    #[test]
    fn alloc_initializes_the_rc_cell_at_offset_zero() {
        // A1.1a: the heap block now carries a refcount cell at offset 0 — the
        // physical home of RuntimeModel.v's `read_rc m base` (RC_OFFSET = 0),
        // initialized to 1 (the `Alloc` +1 the proof's `exec` folds from). The
        // release path that decrements it is the next brick; today the renderer
        // is still Dec-free, so this is purely the foundation relayout.
        let wat = preamble();
        // `$list_new` writes rc = 1 at the rc offset, then len/cap at the shifted
        // offsets — proving the cell exists and is initialized (non-vacuous).
        assert!(
            wat.contains(&format!(
                "(i32.store (i32.add (local.get $p) (i32.const {LIST_RC_OFFSET})) (i32.const {RC_INITIAL}))"
            )),
            "list_new must initialize the rc cell to 1 at RC_OFFSET"
        );
        // The relayout shifted len off offset 0 (where rc now lives): the header
        // is rc + len + cap = 12 bytes, and offsets are derived, not bare.
        assert_eq!(LIST_RC_OFFSET, 0);
        assert_eq!(LIST_LEN_OFFSET, 4);
        assert_eq!(LIST_CAP_OFFSET, 8);
        assert_eq!(LIST_HEADER, 12);
        // The release primitive now EXISTS (A1.1b): the preamble defines `$rc_dec`
        // — the realization of RuntimeModel.v's rt_dec that a `Drop` calls — and it
        // guards against a double-free (it traps on an already-0 cell).
        assert!(wat.contains("(func $rc_dec "), "the rc_dec release primitive must be defined");
        assert!(wat.contains("(unreachable)"), "rc_dec must trap on an already-freed cell");
    }

    #[test]
    fn wasm_runs_value_semantics_matching_rust() {
        let mir = value_semantics_mir();
        assert_eq!(verify_ownership(&mir), Ok(()));
        if let Some(out) = build_and_run("valuesem", &render_wasm(&mir)) {
            assert_eq!(out, "a=9,2,3\nb=1,2,3");
            // The dual-renderer thesis: the SAME MIR on the OTHER target agrees.
            let rust_out = crate::render_rust::render_rust(&mir);
            // (sanity that the two renderers were given the same program)
            assert!(rust_out.contains("v0[0] = 9"));
        }
    }

    #[test]
    fn wasm_push_through_alias_keeps_sibling_independent() {
        // var a = [1]; var b = a; a.push(2); print a; print b → a=[1,2], b=[1]
        let (a, b) = (ValueId(0), ValueId(1));
        let mir = MirFunction {
            name: "main".into(),
            ops: vec![
                Op::Alloc { dst: a, repr: heap(), init: Init::IntList(vec![1]) },
                Op::Dup { dst: b, src: a },
                Op::MakeUnique { v: a },
                Op::Call {
                    dst: Some(a),
                    func: RtFn::ListPush,
                    args: vec![CallArg::Handle(a), CallArg::Imm(2)],
                result: None },
                Op::Call { dst: None, func: RtFn::PrintList, args: vec![CallArg::Handle(a), CallArg::Label("a".into())] , result: None },
                Op::Call { dst: None, func: RtFn::PrintList, args: vec![CallArg::Handle(b), CallArg::Label("b".into())] , result: None },
                Op::Drop { v: b },
                Op::Drop { v: a },
            ],
            ..Default::default()
        };
        assert_eq!(verify_ownership(&mir), Ok(()));
        if let Some(out) = build_and_run("push", &render_wasm(&mir)) {
            assert_eq!(out, "a=1,2\nb=1");
        }
    }
}
