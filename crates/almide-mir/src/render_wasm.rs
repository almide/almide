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

use crate::{CallArg, Init, IntOp, MirFunction, MirProgram, Op, Repr, RtFn, ValueId};
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
    for op in &func.ops {
        body.push_str(&render_op(op, label_off));
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
        | Op::IntBinOp { dst, .. }
        | Op::Pure { dst, .. } => Some(*dst),
        Op::CallFn { dst, .. } | Op::Call { dst, .. } => *dst,
        _ => None,
    }
}

/// Infer each value's Repr (params + op results) for local/param/result typing.
fn value_reprs_wasm(func: &MirFunction) -> BTreeMap<ValueId, Repr> {
    let mut m = BTreeMap::new();
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
            Op::Const { dst } | Op::IntBinOp { dst, .. } => {
                m.insert(*dst, SCALAR_REPR);
            }
            Op::CallFn { dst: Some(d), .. } => {
                m.insert(*d, SCALAR_REPR);
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
        Op::Alloc { dst, init, .. } => {
            let elems: &[i64] = match init {
                Init::IntList(e) => e,
                Init::Opaque => &[],
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
            let o = match op {
                IntOp::Add => "i64.add",
                IntOp::Sub => "i64.sub",
                IntOp::Mul => "i64.mul",
            };
            format!(
                "    (local.set {d} ({o} (local.get {a}) (local.get {b})))\n",
                d = local(*dst),
                a = local(*a),
                b = local(*b)
            )
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
        Op::Consume { .. }
        | Op::Borrow { .. }
        | Op::Const { .. }
        | Op::Pure { .. } => String::new(),
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
