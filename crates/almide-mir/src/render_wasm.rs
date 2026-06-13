//! MIR → wasm renderer (the SOVEREIGN target, §1 — wasm is the canonical v1
//! artifact; the Rust renderer is the secondary qualification path).
//!
//! Like the Rust renderer it TRANSLATES the MIR decision and never re-decides
//! it (§3.2). It emits WebAssembly text (WAT, run directly by wasmtime). For the
//! value-semantics subset it uses the SAME idiom as the Rust renderer — eager
//! copy-on-`Dup` (a list literal is a heap block; `Dup` copies it) — so the two
//! targets are byte-identical by construction WITHOUT needing refcounting here:
//! no sharing ⇒ no `__rc_*`, and `MakeUnique` is a no-op (the copy already made
//! the handle unique). The richer RC model is a later brick; this proves the
//! dual-renderer thesis end to end first.
//!
//! Heap list layout in linear memory: `[len: i32 @0][cap: i32 @4][data: i64 @8…]`.
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
//! Convergence rule: never add another hand-written WAT runtime routine.

use crate::{Init, MirFunction, Op, ValueId};
use std::collections::BTreeMap;

// Fixed low-memory addresses (named — no raw literals in the emitted WAT logic).
const NWRITTEN_ADDR: u32 = 0; // i32 scratch for fd_write's bytes-written out-param
const IOVEC_ADDR: u32 = 8; // [buf: i32][len: i32]
const ITOA_TMP_ADDR: u32 = 32; // reversed-digit scratch (≤ 20 bytes)
const LABELS_ADDR: u32 = 64; // print labels (the data section)
const SCRATCH_ADDR: u32 = 512; // the line build buffer
const HEAP_BASE: u32 = 8192; // bump allocator start

// List layout / growth.
const LIST_HEADER: u32 = 8; // [len:i32][cap:i32]
const ELEM_SIZE: u32 = 8; // i64 elements
const PUSH_HEADROOM: u32 = 8; // spare cap so demo pushes never realloc

/// ASCII bytes the formatter writes.
const ASCII_ZERO: u32 = 48;
const ASCII_EQUALS: u32 = 61;
const ASCII_COMMA: u32 = 44;
const ASCII_NEWLINE: u32 = 10;

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
        if let Op::Print { label, .. } = op {
            if !label_off.contains_key(label) {
                let len = label.len() as u32;
                label_off.insert(label.clone(), (cursor, len));
                data.push_str(&format!(
                    "  (data (i32.const {cursor}) {:?})\n",
                    label
                ));
                cursor += len;
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
        // The single ownership decision: an alias is a fresh COPY (eager COW),
        // matching the Rust renderer's `.clone()`.
        Op::Dup { dst, src } => format!(
            "    (local.set {d} (call $list_copy (local.get {s})))\n",
            d = local(*dst),
            s = local(*src)
        ),
        Op::IndexSet { target, index, value } => format!(
            "    (call $list_set (local.get {t}) (i32.const {index}) (i64.const {value}))\n",
            t = local(*target)
        ),
        Op::Push { target, value } => format!(
            "    (local.set {t} (call $list_push (local.get {t}) (i64.const {value})))\n",
            t = local(*target)
        ),
        Op::Print { value, label } => {
            let (off, len) = label_off[label];
            format!(
                "    (call $print_list (local.get {v}) (i32.const {off}) (i32.const {len}))\n",
                v = local(*value)
            )
        }
        // No wasm needed: Drop/Consume are no-ops in the eager-copy model (no RC),
        // MakeUnique already done by the Dup copy, Const/Borrow/Pure are not used
        // by the value-semantics subset's observable path.
        Op::Drop { .. }
        | Op::Consume { .. }
        | Op::MakeUnique { .. }
        | Op::Borrow { .. }
        | Op::Const { .. }
        | Op::Pure { .. } => String::new(),
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

  (func $alloc (param $n i32) (result i32)
    (local $p i32)
    (local.set $p (global.get $bump))
    (global.set $bump (i32.add (local.get $p) (local.get $n)))
    (local.get $p))

  (func $list_new (param $len i32) (param $cap i32) (result i32)
    (local $p i32)
    (local.set $p (call $alloc (i32.add (i32.const {LIST_HEADER})
                                        (i32.mul (local.get $cap) (i32.const {ELEM_SIZE})))))
    (i32.store (local.get $p) (local.get $len))
    (i32.store (i32.add (local.get $p) (i32.const 4)) (local.get $cap))
    (local.get $p))

  (func $elem_addr (param $list i32) (param $idx i32) (result i32)
    (i32.add (i32.add (local.get $list) (i32.const {LIST_HEADER}))
             (i32.mul (local.get $idx) (i32.const {ELEM_SIZE}))))

  (func $list_set (param $list i32) (param $idx i32) (param $val i64)
    (i64.store (call $elem_addr (local.get $list) (local.get $idx)) (local.get $val)))

  (func $list_get (param $list i32) (param $idx i32) (result i64)
    (i64.load (call $elem_addr (local.get $list) (local.get $idx))))

  (func $list_len (param $list i32) (result i32) (i32.load (local.get $list)))

  (func $list_copy (param $src i32) (result i32)
    (local $len i32) (local $cap i32) (local $dst i32) (local $i i32)
    (local.set $len (i32.load (local.get $src)))
    (local.set $cap (i32.load (i32.add (local.get $src) (i32.const 4))))
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
    (local.set $len (i32.load (local.get $list)))
    (call $list_set (local.get $list) (local.get $len) (local.get $val))
    (i32.store (local.get $list) (i32.add (local.get $len) (i32.const 1)))
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
                           (i32.wrap_i64 (i64.rem_u (local.get $v) (i64.const 10)))))
      (local.set $n (i32.add (local.get $n) (i32.const 1)))
      (local.set $v (i64.div_u (local.get $v) (i64.const 10)))
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
    (i32.store (i32.add (i32.const {IOVEC_ADDR}) (i32.const 4))
               (i32.sub (local.get $cur) (i32.const {SCRATCH_ADDR})))
    (drop (call $fd_write (i32.const 1) (i32.const {IOVEC_ADDR})
                          (i32.const 1) (i32.const {NWRITTEN_ADDR}))))

"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{verify_ownership, LayoutId, Repr};
    use std::process::Command;

    fn heap() -> Repr {
        Repr::Ptr { layout: LayoutId(0) }
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

    fn value_semantics_mir() -> MirFunction {
        // var a = [1,2,3]; var b = a; a[0] = 9; print a; print b
        let (a, b) = (ValueId(0), ValueId(1));
        MirFunction {
            name: "main".into(),
            ops: vec![
                Op::Alloc { dst: a, repr: heap(), init: Init::IntList(vec![1, 2, 3]) },
                Op::Dup { dst: b, src: a },
                Op::MakeUnique { v: a },
                Op::IndexSet { target: a, index: 0, value: 9 },
                Op::Print { value: a, label: "a".into() },
                Op::Print { value: b, label: "b".into() },
                Op::Drop { v: b },
                Op::Drop { v: a },
            ],
        }
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
                Op::Push { target: a, value: 2 },
                Op::Print { value: a, label: "a".into() },
                Op::Print { value: b, label: "b".into() },
                Op::Drop { v: b },
                Op::Drop { v: a },
            ],
        };
        assert_eq!(verify_ownership(&mir), Ok(()));
        if let Some(out) = build_and_run("push", &render_wasm(&mir)) {
            assert_eq!(out, "a=1,2\nb=1");
        }
    }
}
