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
        // A runtime-sized OWNED String of `len` bytes: round the byte length up to
        // ELEM_SIZE (list-compatible so the free-list reuses it), $alloc, set rc=1 + the
        // byte len + the element cap. The data is left UNINITIALIZED for the caller to fill
        // via `prim.store8` (the self-host `int.to_string` builder). Cert: one `Alloc` = i,
        // init-agnostic — a fresh owned object, no checker change.
        Op::Alloc { dst, init: Init::DynStr { len }, .. } => {
            let wlen = format!("(i32.wrap_i64 (local.get {}))", local(*len));
            // round byte len up to ELEM_SIZE: (len + ELEM_SIZE-1) & ~(ELEM_SIZE-1)
            let rounded = format!(
                "(i32.and (i32.add {wlen} (i32.const {add})) (i32.const {mask}))",
                add = ELEM_SIZE - 1,
                mask = -(ELEM_SIZE as i32),
            );
            format!(
                "    (local.set {d} (call $alloc (i32.add (i32.const {LIST_HEADER}) {rounded})))\n\
                 \x20   (i32.store (i32.add (local.get {d}) (i32.const {LIST_RC_OFFSET})) (i32.const {RC_INITIAL}))\n\
                 \x20   (i32.store (i32.add (local.get {d}) (i32.const {LIST_LEN_OFFSET})) {wlen})\n\
                 \x20   (i32.store (i32.add (local.get {d}) (i32.const {LIST_CAP_OFFSET})) (i32.shr_u {rounded} (i32.const {shift})))\n",
                d = local(*dst),
                shift = ELEM_SIZE.trailing_zeros(),
            )
        }
        // A materialized `Some(payload)`: a 1-element list (len=1) whose `data[0]` holds
        // the scalar payload. `None` is the 0-element list (`Init::Opaque`, len=0). A
        // variant `match` reads `len` as the tag and `data[0]` as the payload. Cert: one
        // `Alloc` = i, init-agnostic (no checker change).
        Op::Alloc { dst, init: Init::OptSome { payload }, .. } => {
            let cap = 1 + PUSH_HEADROOM;
            format!(
                "    (local.set {d} (call $list_new (i32.const 1) (i32.const {cap})))\n\
                 \x20   (call $list_set (local.get {d}) (i32.const 0) (local.get {p}))\n",
                d = local(*dst),
                p = local(*payload),
            )
        }
        // A runtime-sized OWNED `List[Int]` of `len` i64 slots: $alloc `LIST_HEADER +
        // len*ELEM_SIZE` bytes, set rc=1 + len + cap (= the element count). Elements are
        // left UNINITIALIZED for the caller to fill via `prim.store64`. The list-building
        // sibling of `DynStr`. Cert: one `Alloc` = i, init-agnostic — no checker change.
        Op::Alloc { dst, init: Init::DynList { len }, .. } => {
            let wlen = format!("(i32.wrap_i64 (local.get {}))", local(*len));
            let bytes = format!("(i32.mul {wlen} (i32.const {ELEM_SIZE}))");
            format!(
                "    (local.set {d} (call $alloc (i32.add (i32.const {LIST_HEADER}) {bytes})))\n\
                 \x20   (i32.store (i32.add (local.get {d}) (i32.const {LIST_RC_OFFSET})) (i32.const {RC_INITIAL}))\n\
                 \x20   (i32.store (i32.add (local.get {d}) (i32.const {LIST_LEN_OFFSET})) {wlen})\n\
                 \x20   (i32.store (i32.add (local.get {d}) (i32.const {LIST_CAP_OFFSET})) {wlen})\n",
                d = local(*dst),
            )
        }
        Op::Alloc { dst, init, .. } => {
            let elems: &[i64] = match init {
                Init::IntList(e) => e,
                Init::Opaque
                | Init::Str(_)
                | Init::DynStr { .. }
                | Init::OptSome { .. }
                | Init::DynList { .. } => &[],
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
                IntOp::And => format!("(i64.and {args})"),
                IntOp::Or => format!("(i64.or {args})"),
                IntOp::Xor => format!("(i64.xor {args})"),
                IntOp::Shl => format!("(i64.shl {args})"),
                IntOp::Shr => format!("(i64.shr_s {args})"),
            };
            format!("    (local.set {d} {expr})\n", d = local(*dst))
        }
        // CallIndirect render needs the module function table + per-arity `(type)` — wired
        // in a later closures-machinery slice. No lowering emits this op yet (dead path), so
        // a WAT comment placeholder keeps render TOTAL without affecting any real program.
        Op::CallIndirect { .. } => {
            String::from("    ;; call_indirect — closure machinery: function table not yet wired\n")
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

/// The self-hosted stdlib runtime registry: `(call name, impl fn name, Almide source)`.
/// The v1 linker auto-includes an entry when its `call name` is invoked but undefined,
/// renaming the impl fn (Almide names can't hold a dot) to the call name — so
/// `(call $module.func)` resolves AND the caps gate reads it as a known-pure stdlib
/// `module.func`. The single source of truth for the stdlib self-host campaign (§4.1:
/// the runtime self-hosts into Almide; the trusted floor stays the prim ops + checker).
pub fn self_host_runtime() -> &'static [(&'static str, &'static [(&'static str, &'static str)])] {
    &[
        (include_str!("../../../stdlib/int_to_string.almd"), &[("int_to_string", "int.to_string")]),
        (include_str!("../../../stdlib/string_len.almd"), &[("string_len", "string.len")]),
        (include_str!("../../../stdlib/string_repeat.almd"), &[("string_repeat", "string.repeat")]),
        (include_str!("../../../stdlib/string_is_empty.almd"), &[("string_is_empty", "string.is_empty")]),
        (
            include_str!("../../../stdlib/math_int.almd"),
            &[
                ("math_abs", "math.abs"),
                ("math_max", "math.max"),
                ("math_min", "math.min"),
                ("math_sign", "math.sign"),
                ("math_pow", "math.pow"),
                ("math_factorial", "math.factorial"),
                ("math_choose", "math.choose"),
            ],
        ),
        (
            include_str!("../../../stdlib/list_modify.almd"),
            &[
                ("list_set", "list.set"),
                ("list_swap", "list.swap"),
                ("list_insert", "list.insert"),
                ("list_remove_at", "list.remove_at"),
            ],
        ),
        (include_str!("../../../stdlib/list_len.almd"), &[("list_len", "list.len")]),
        (include_str!("../../../stdlib/list_is_empty.almd"), &[("list_is_empty", "list.is_empty")]),
        (include_str!("../../../stdlib/list_sum.almd"), &[("list_sum", "list.sum")]),
        (include_str!("../../../stdlib/list_sort.almd"), &[("list_sort", "list.sort")]),
        (include_str!("../../../stdlib/list_unique.almd"), &[("list_unique", "list.unique")]),
        (include_str!("../../../stdlib/list_dedup.almd"), &[("list_dedup", "list.dedup")]),
        (
            include_str!("../../../stdlib/list_intersperse.almd"),
            &[("list_intersperse", "list.intersperse")],
        ),
        (
            include_str!("../../../stdlib/int_wrap.almd"),
            &[
                ("int_wrap_add", "int.wrap_add"),
                ("int_wrap_mul", "int.wrap_mul"),
                ("int_to_u32", "int.to_u32"),
                ("int_to_u8", "int.to_u8"),
            ],
        ),
        (
            include_str!("../../../stdlib/int_sized.almd"),
            &[
                ("int_to_int8", "int.to_int8"),
                ("int_to_int16", "int.to_int16"),
                ("int_to_int32", "int.to_int32"),
                ("int_to_int64", "int.to_int64"),
            ],
        ),
        (include_str!("../../../stdlib/string_slice.almd"), &[("string_slice", "string.slice")]),
        (
            include_str!("../../../stdlib/string_is_digit.almd"),
            &[("string_is_digit", "string.is_digit")],
        ),
        (
            include_str!("../../../stdlib/string_take_drop.almd"),
            &[
                ("string_take", "string.take"),
                ("string_take_end", "string.take_end"),
                ("string_drop", "string.drop"),
                ("string_drop_end", "string.drop_end"),
            ],
        ),
        (
            include_str!("../../../stdlib/string_to_bytes.almd"),
            &[("string_to_bytes", "string.to_bytes")],
        ),
        (
            include_str!("../../../stdlib/string_trim.almd"),
            &[
                ("string_trim", "string.trim"),
                ("string_trim_start", "string.trim_start"),
                ("string_trim_end", "string.trim_end"),
            ],
        ),
        (include_str!("../../../stdlib/string_reverse.almd"), &[("string_reverse", "string.reverse")]),
        (
            include_str!("../../../stdlib/string_replace.almd"),
            &[
                ("string_replace", "string.replace"),
                ("string_replace_first", "string.replace_first"),
            ],
        ),
        (
            include_str!("../../../stdlib/string_pad.almd"),
            &[("string_pad_start", "string.pad_start"), ("string_pad_end", "string.pad_end")],
        ),
        (include_str!("../../../stdlib/list_get_or.almd"), &[("list_get_or", "list.get_or")]),
        (
            include_str!("../../../stdlib/int_bitcount.almd"),
            &[
                ("int_pop_count", "int.pop_count"),
                ("int_count_trailing_zeros", "int.count_trailing_zeros"),
                ("int_count_leading_zeros", "int.count_leading_zeros"),
                ("int_bit_width", "int.bit_width"),
                ("int_log2_floor", "int.log2_floor"),
                ("int_log2_ceil", "int.log2_ceil"),
                ("int_next_power_of_two", "int.next_power_of_two"),
                ("int_prev_power_of_two", "int.prev_power_of_two"),
            ],
        ),
        (
            include_str!("../../../stdlib/int_bits.almd"),
            &[
                ("int_band", "int.band"),
                ("int_bor", "int.bor"),
                ("int_bxor", "int.bxor"),
                ("int_bshl", "int.bshl"),
                ("int_bshr", "int.bshr"),
                ("int_bnot", "int.bnot"),
                ("int_byte_swap", "int.byte_swap"),
                ("int_bit_reverse", "int.bit_reverse"),
            ],
        ),
        (include_str!("../../../stdlib/int_hex.almd"), &[("int_to_hex", "int.to_hex")]),
        (
            include_str!("../../../stdlib/int_scalar.almd"),
            &[
                ("int_abs", "int.abs"),
                ("int_min", "int.min"),
                ("int_max", "int.max"),
                ("int_clamp", "int.clamp"),
            ],
        ),
        (
            include_str!("../../../stdlib/list_get.almd"),
            &[("list_get", "list.get"), ("list_first", "list.first"), ("list_last", "list.last")],
        ),
        (
            include_str!("../../../stdlib/list_search.almd"),
            &[("list_contains", "list.contains"), ("list_index_of", "list.index_of")],
        ),
        (include_str!("../../../stdlib/list_reverse.almd"), &[("list_reverse", "list.reverse")]),
        (
            include_str!("../../../stdlib/list_make.almd"),
            &[("list_range", "list.range"), ("list_repeat", "list.repeat")],
        ),
        (
            include_str!("../../../stdlib/list_take_drop.almd"),
            &[
                ("list_take", "list.take"),
                ("list_drop", "list.drop"),
                ("list_slice", "list.slice"),
            ],
        ),
        (
            include_str!("../../../stdlib/list_fold.almd"),
            &[
                ("list_product", "list.product"),
                ("list_max", "list.max"),
                ("list_min", "list.min"),
            ],
        ),
        (
            include_str!("../../../stdlib/string_search.almd"),
            &[
                ("string_starts_with", "string.starts_with"),
                ("string_ends_with", "string.ends_with"),
                ("string_contains", "string.contains"),
                ("string_count", "string.count"),
                ("string_index_of", "string.index_of"),
                ("string_last_index_of", "string.last_index_of"),
            ],
        ),
    ]
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
mod tests;
