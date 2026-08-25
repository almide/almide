//! Binary operator lowering — split from emitter.rs for the
//! complexity budget.

use almide_ir::{BinOp, IrExpr, IrExprKind};
use wasm_encoder::BlockType;

use crate::emitter::Emitter;
use crate::*;

impl Emitter<'_> {
    pub(crate) fn lower_binop(
        &mut self,
        op: BinOp,
        left: &IrExpr,
        right: &IrExpr,
    ) -> Result<SliceTy, EmitError> {
        use BinOp::*;
        match op {
            AddFloat | SubFloat | MulFloat | DivFloat => {
                self.lower(left, Some(FLOAT))?;
                self.lower(right, Some(FLOAT))?;
                let mut i = self.f.instructions();
                match op {
                    AddFloat => i.f64_add(),
                    SubFloat => i.f64_sub(),
                    MulFloat => i.f64_mul(),
                    DivFloat => i.f64_div(),
                    _ => unreachable!(),
                };
                Ok(FLOAT)
            }
            AddInt | SubInt | MulInt => {
                self.lower(left, Some(INT))?;
                self.lower(right, Some(INT))?;
                let mut i = self.f.instructions();
                match op {
                    AddInt => i.i64_add(),
                    SubInt => i.i64_sub(),
                    _ => i.i64_mul(),
                };
                Ok(INT)
            }
            // C-002: wasm's own div/rem semantics DIFFER from the native
            // abort contract — `i64.rem_s` defines `MIN % -1 = 0` (no
            // trap: the silent-divergence case the abort-parity gate
            // caught on activation day), and a raw trap carries no stderr.
            // Guard BOTH operands and abort with the exact native frame
            // ("Error: division by zero" / "Error: integer overflow" +
            // exit 1) before the op, so the op itself can never trap.
            DivInt | ModInt => {
                // LITERAL divisor: strength-reduce (multiply-shift, no
                // guards — the abortable divisors are excluded here).
                // c in {0, -1, MIN} keeps the guarded runtime path; /1
                // is the operand, %1 is zero (operand still evaluated).
                if let IrExprKind::LitInt { value: c } = &right.kind {
                    let c = *c;
                    if !matches!(c, 0 | -1 | i64::MIN) {
                        self.lower(left, Some(INT))?;
                        if c == 1 {
                            if matches!(op, ModInt) {
                                self.f.instructions().i64_const(0).i64_and();
                            }
                            return Ok(INT);
                        }
                        self.emit_const_div(c, matches!(op, DivInt))?;
                        return Ok(INT);
                    }
                }
                self.lower(left, Some(INT))?;
                self.lower(right, Some(INT))?;
                let div0 = self.pool.intern("Error: division by zero");
                let ovf = self.pool.intern("Error: integer overflow");
                let r = self.hold_i64()?;
                let l = self.hold_i64()?;
                let mut i = self.f.instructions();
                i.local_set(r).local_set(l);
                i.local_get(r).i64_eqz().if_(BlockType::Empty);
                i.i32_const(div0 as i32).call(F_EPRINTLN_BLOCK).unreachable().end();
                i.local_get(l).i64_const(i64::MIN).i64_eq();
                i.local_get(r).i64_const(-1).i64_eq();
                i.i32_and().if_(BlockType::Empty);
                i.i32_const(ovf as i32).call(F_EPRINTLN_BLOCK).unreachable().end();
                i.local_get(l).local_get(r);
                match op {
                    DivInt => i.i64_div_s(),
                    _ => i.i64_rem_s(),
                };
                self.release_i64();
                self.release_i64();
                Ok(INT)
            }
            PowInt => self.lower_pow_int(left, right),
            Lt | Gt | Lte | Gte | Eq | Neq => self.lower_cmp(op, left, right),
            // SHORT-CIRCUIT: the right operand must not evaluate (and
            // possibly trap) when the left already decides — an `if`
            // yielding i32, never a strict bitop.
            And => {
                self.lower(left, Some(BOOL))?;
                self.f.instructions().if_(BlockType::Result(ValType::I32));
                self.lower(right, Some(BOOL))?;
                self.f.instructions().else_().i32_const(0).end();
                Ok(BOOL)
            }
            Or => {
                self.lower(left, Some(BOOL))?;
                self.f.instructions().if_(BlockType::Result(ValType::I32)).i32_const(1).else_();
                self.lower(right, Some(BOOL))?;
                self.f.instructions().end();
                Ok(BOOL)
            }
            ConcatStr => {
                self.lower(left, Some(STR))?;
                self.lower(right, Some(STR))?;
                self.f.instructions().call(F_CONCAT);
                if self.metered {
                    // T3-5 dynamic charge: 1 + result_byte_len/16, keyed
                    // on the same result both backends key on.
                    let t = self.tmp_i32_local;
                    let mut i = self.f.instructions();
                    i.local_set(t);
                    i.global_get(G_DET_FUEL);
                    i.i64_const(1);
                    i.local_get(t).i32_load(len_memarg()).i32_const(4).i32_shr_u().i64_extend_i32_u();
                    i.i64_add().i64_sub().global_set(G_DET_FUEL);
                    let _ = i;
                    self.emit_det_cut_check();
                    self.f.instructions().local_get(t);
                }
                Ok(STR)
            }
            // List ++ List: byte-concat of the element arrays IS element
            // concat (same stride both sides).
            ConcatList => {
                let lt = self.lower(left, None)?;
                let SliceTy::List(_) = lt else {
                    return unsup(&format!("concat-list-of:{lt:?}"));
                };
                self.lower(right, Some(lt))?;
                self.f.instructions().call(F_CONCAT);
                Ok(lt)
            }
            other => unsup(&format!("binop:{other:?}")),
        }
    }
}

impl Emitter<'_> {
    /// `x op c` with a LITERAL divisor, |c| >= 2 (c == 1 handled by the
    /// caller; c in {0, -1, i64::MIN} keeps the guarded runtime path):
    /// multiply-shift, no division instruction, no guards needed — the
    /// only abortable divisor values are excluded by construction.
    pub(crate) fn emit_const_div(&mut self, c: i64, is_div: bool) -> Result<(), EmitError> {
        let hx = self.hold_i64()?;
        let scr = self.scr_i64_local;
        {
            let mut i = self.f.instructions();
            i.local_set(hx);
        }
        if c > 0 && (c as u64).is_power_of_two() {
            let k = c.trailing_zeros();
            let mut i = self.f.instructions();
            i.local_get(hx);
            i.local_get(hx).i64_const(63).i64_shr_s().i64_const(64 - i64::from(k)).i64_shr_u();
            i.i64_add().i64_const(i64::from(k)).i64_shr_s();
            if !is_div {
                i.local_set(scr);
                i.local_get(hx);
                i.local_get(scr).i64_const(i64::from(k)).i64_shl();
                i.i64_sub();
            }
            let _ = i;
            self.release_i64();
            return Ok(());
        }
        // General literal divisor: the HD magic sequence measured as a
        // PESSIMIZATION on this class of host (aarch64 sdiv is fast and
        // cranelift has no mulhi; int_loop +60%) — the bounded win is
        // dropping the GUARDS, which a nonzero non-minus-one literal
        // makes provably dead.
        let mut i = self.f.instructions();
        i.local_get(hx).i64_const(c);
        if is_div {
            i.i64_div_s();
        } else {
            i.i64_rem_s();
        }
        let _ = i;
        self.release_i64();
        Ok(())
    }

    /// Wrapping square-multiply, verbatim from the oracle's
    /// int_pow (#895): a negative exponent has no integer result
    /// and aborts on every target; products wrap like `*` does.
    pub(crate) fn lower_pow_int(&mut self, left: &IrExpr, right: &IrExpr) -> Result<SliceTy, EmitError> {

                self.lower(left, Some(INT))?;
                self.lower(right, Some(INT))?;
                let neg = self.pool.intern("Error: negative exponent");
                let e = self.hold_i64()?;
                let b = self.hold_i64()?;
                let acc = self.hold_i64()?;
                let mut i = self.f.instructions();
                i.local_set(e).local_set(b);
                i.local_get(e).i64_const(0).i64_lt_s().if_(BlockType::Empty);
                i.i32_const(neg as i32).call(F_EPRINTLN_BLOCK).unreachable().end();
                i.i64_const(1).local_set(acc);
                i.block(BlockType::Empty).loop_(BlockType::Empty);
                i.local_get(e).i64_eqz().br_if(1);
                i.local_get(e).i64_const(1).i64_and().i64_const(0).i64_ne().if_(BlockType::Empty);
                i.local_get(acc).local_get(b).i64_mul().local_set(acc);
                i.end();
                i.local_get(e).i64_const(1).i64_shr_u().local_set(e);
                i.local_get(e).i64_eqz().i32_eqz().if_(BlockType::Empty);
                i.local_get(b).local_get(b).i64_mul().local_set(b);
                i.end();
                i.br(0).end().end();
                i.local_get(acc);
                let _ = i;
                self.release_i64();
                self.release_i64();
                self.release_i64();
                Ok(INT)
    }
}
