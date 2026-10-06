//! Instance-parallel `fan` chunks (#3003 stage 1, ADR-0011 §D2a): the
//! emitter half. `fan.__par_K(xs, fallback, c…)` (written by `fan_par.rs`)
//! hands the whole map to the host in ONE op and, when the host does not
//! serve it, runs `fallback` — the plain sequential `list.map` over the
//! exported chunk `__fan_site_K`.
//!
//! The op (`OP_FAN_PAR`, `fs_call(74, a, a_len, b, b_len) -> i64`):
//!
//! - `a` is the request, little-endian i64 slots:
//!   `[K, n, m, r, (kind_j, off_j) × r, cap × m, elem × n]` — `n` elements,
//!   `m` captures, `r` result fields (0 = a scalar result). A value is its
//!   raw 64 bits (an f64's bit pattern, a Bool zero-extended); `kind` is
//!   0 i64 / 1 f64 / 2 i32, and `off` is the field's ABSOLUTE offset from a
//!   tuple block's base, so the host reads a child's tuple without knowing
//!   the layout.
//! - `b` is the answer room: `n × max(r, 1)` i64 slots, element-major.
//! - the result is 1 when the host filled `b` (every element ran on some
//!   instance in its own heap), anything else when it did not. Only 1
//!   counts: a host that does not know the op answers 0 (the stock shims),
//!   and so does the embedded host when a child traps — the sequential run
//!   then reproduces the abort exactly (the chunk is pure, so running it
//!   twice is unobservable).

use almide_ir::IrExpr;
use wasm_encoder::{BlockType, ValType};

use crate::emitter::Emitter;
use crate::*;

/// The host op (the first code past 73, `OP_STDERR_RAW`).
pub(crate) const OP_FAN_PAR: i32 = 74;

fn kind_code(t: SliceTy) -> i64 {
    match t.val_type() {
        ValType::I64 => 0,
        ValType::F64 => 1,
        _ => 2,
    }
}

impl Emitter<'_> {
    /// The value on the stack → its raw i64 bits.
    fn par_to_bits(&mut self, t: SliceTy) {
        match t.val_type() {
            ValType::I64 => {}
            ValType::F64 => {
                self.f.instructions().i64_reinterpret_f64();
            }
            _ => {
                self.f.instructions().i64_extend_i32_u();
            }
        }
    }

    /// Raw i64 bits on the stack → the value of type `t`.
    fn par_from_bits(&mut self, t: SliceTy) {
        match t.val_type() {
            ValType::I64 => {}
            ValType::F64 => {
                self.f.instructions().f64_reinterpret_i64();
            }
            _ => {
                self.f.instructions().i32_wrap_i64();
            }
        }
    }

    pub(crate) fn lower_fan_par(&mut self, func: &str, args: &[IrExpr]) -> Result<Lowered, EmitError> {
        let Ok(k) = func[crate::fan_par::PAR_PREFIX.len()..].parse::<i64>() else {
            return unsup(&format!("fan-par-name:{func}"));
        };
        let [xs, fallback, caps @ ..] = args else {
            return unsup("fan-par-arity");
        };
        let SliceTy::List(eh) = self.lower_arg(xs, None, ArgMode::Borrow)? else {
            return unsup("fan-par-xs");
        };
        let elem = self.types.el(eh);
        let hb = self.hold_i32()?;
        self.f.instructions().local_set(hb);
        let mut cap_holds = Vec::with_capacity(caps.len());
        for c in caps {
            let t = self.lower_arg(c, None, ArgMode::Borrow)?;
            self.par_to_bits(t);
            let h = self.hold_i64()?;
            self.f.instructions().local_set(h);
            cap_holds.push(h);
        }
        let Some(out_ty @ SliceTy::List(rh)) = slice_ty_of(&fallback.ty, self.types) else {
            return unsup("fan-par-ret");
        };
        let rty = self.types.el(rh);
        let fields: Vec<(SliceTy, u32)> = match rty {
            SliceTy::Tuple(ti) => self.types.tuple_def(ti).fields,
            _ => Vec::new(),
        };
        let size = match rty {
            SliceTy::Tuple(ti) => self.types.tuple_def(ti).size,
            _ => 0,
        };
        let r = fields.len() as u32;
        let m = caps.len() as u32;
        // the elements start after the header and the captures
        let elems_at = 8 * (4 + 2 * r + m);
        let width = 8 * r.max(1);
        self.note_host_op(OP_FAN_PAR);
        let hn = self.hold_i32()?;
        let ha = self.hold_i32()?;
        let ho = self.hold_i32()?;
        let hi = self.hold_i32()?;
        let hacc = self.hold_i32()?;
        let ht = self.hold_i32()?;
        let estride = elem.slot_size() as i32;
        let rstride = rty.slot_size() as i32;
        {
            let mut i = self.f.instructions();
            i.local_get(hb).i32_load(len_memarg()).i32_const(estride).i32_div_u().local_set(hn);
            // the request block
            i.i32_const(elems_at as i32).local_get(hn).i32_const(8).i32_mul().i32_add().call(F_ALLOC).local_set(ha);
        }
        let request = self.witness_par_room();
        {
            let mut i = self.f.instructions();
            let head: Vec<i64> = [k, 0, i64::from(m), i64::from(r)]
                .into_iter()
                .chain(fields.iter().flat_map(|&(t, off)| [kind_code(t), i64::from(almide_layout::PAYLOAD + off)]))
                .collect();
            for (j, v) in head.into_iter().enumerate() {
                i.local_get(ha);
                if j == 1 {
                    i.local_get(hn).i64_extend_i32_u();
                } else {
                    i.i64_const(v);
                }
                i.i64_store(slot_memarg(8 * j as u32));
            }
            for (j, &h) in cap_holds.iter().enumerate() {
                i.local_get(ha).local_get(h).i64_store(slot_memarg(8 * (4 + 2 * r + j as u32)));
            }
            i.i32_const(0).local_set(hi);
            i.block(BlockType::Empty).loop_(BlockType::Empty);
            i.local_get(hi).local_get(hn).i32_ge_u().br_if(1);
            i.local_get(ha).local_get(hi).i32_const(8).i32_mul().i32_add();
            i.local_get(hb).local_get(hi).i32_const(estride).i32_mul().i32_add();
        }
        self.load_ty_slot(elem, 0);
        self.par_to_bits(elem);
        {
            let mut i = self.f.instructions();
            i.i64_store(slot_memarg(elems_at));
            i.local_get(hi).i32_const(1).i32_add().local_set(hi).br(0).end().end();
            // the answer room
            i.local_get(hn).i32_const(width as i32).i32_mul().call(F_ALLOC).local_set(ho);
        }
        let answer = self.witness_par_room();
        self.witness_par_served(!fields.is_empty());
        {
            let mut i = self.f.instructions();
            i.i32_const(OP_FAN_PAR);
            i.local_get(ha).i32_const(almide_layout::PAYLOAD as i32).i32_add();
            i.local_get(ha).i32_load(len_memarg());
            i.local_get(ho).i32_const(almide_layout::PAYLOAD as i32).i32_add();
            i.local_get(ho).i32_load(len_memarg());
            i.call(F_FS_CALL).i64_const(1).i64_eq();
            i.if_(BlockType::Result(ValType::I32));
            // served: the list from the answer room
            i.local_get(hn).i32_const(rstride).i32_mul().call(F_ALLOC).local_set(hacc);
            i.i32_const(0).local_set(hi);
            i.block(BlockType::Empty).loop_(BlockType::Empty);
            i.local_get(hi).local_get(hn).i32_ge_u().br_if(1);
            i.local_get(hacc).local_get(hi).i32_const(rstride).i32_mul().i32_add();
        }
        if fields.is_empty() {
            self.f.instructions().local_get(ho).local_get(hi).i32_const(8).i32_mul().i32_add().i64_load(slot_memarg(0));
            self.par_from_bits(rty);
            self.store_ty_slot(rty, 0);
        } else {
            self.f.instructions().i32_const(size as i32).call(F_ALLOC).local_set(ht);
            for (j, &(fty, off)) in fields.iter().enumerate() {
                {
                    let mut i = self.f.instructions();
                    i.local_get(ht);
                    i.local_get(ho).local_get(hi).i32_const(width as i32).i32_mul().i32_add();
                    i.i64_load(slot_memarg(8 * j as u32));
                }
                self.par_from_bits(fty);
                self.store_ty_slot(fty, off);
            }
            self.f.instructions().local_get(ht).i32_store(slot_memarg(0));
        }
        {
            let mut i = self.f.instructions();
            i.local_get(hi).i32_const(1).i32_add().local_set(hi).br(0).end().end();
            i.local_get(hacc);
            i.else_();
        }
        // not served: the sequential map, exactly as before the rewrite
        self.witness_par_fallback(fallback);
        self.lower(fallback, Some(out_ty))?;
        {
            let mut i = self.f.instructions();
            i.end();
            i.local_get(ha).call(F_FREE);
            i.local_get(ho).call(F_FREE);
        }
        self.witness_par_close([request, answer]);
        for _ in 0..6 {
            self.release_i32();
        }
        for _ in &cap_holds {
            self.release_i64();
        }
        self.release_i32();
        Ok(Lowered::owned(out_ty))
    }
}
