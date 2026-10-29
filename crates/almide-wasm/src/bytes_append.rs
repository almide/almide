//! The k-byte APPEND family (#3294): `append_{u16,i16,u32,i32,i64,f32,f64}_
//! {le,be}` and `append_bool`. Each one used to build a FRESH block
//! of `len + k` and copy the whole payload — the self-host twins (`__bam`)
//! and the inline BE writer alike — so
//! building a buffer by appending was quadratic and ran out of memory at a few
//! hundred KiB. `append_u8` was the one member already amortized: it is
//! `bytes.push`, lowered through `$bytes_push` (#1689).
//!
//! Every member now IS that push: the value's k bytes go through `$bytes_push`
//! one at a time, so the whole family shares its one growth rule (in place
//! while `cap - len >= 1`, else a fresh block at `max(cap * 2, 16)`, the
//! outgrown block freed at rc 1), its receiver protocol (the COW read, which
//! makes a parameter receiver unique too, #3342) and its write-back. No second growth policy
//! exists to drift from list.push's.
//!
//! Byte values are the twins' exactly: integers are the i64's two's-complement
//! bytes (`band(bshr_u(bits, 8i), 255)`), every float member — `append_f32_*`,
//! `append_f64_*` — canonicalizes NaN first through `emit_float_obs_bits`
//! (C-210: 0x7FC00000 / `float.to_bits`; the in-place `set_f*` of bytes_rw.rs
//! share it, #3658).

use almide_ir::IrExpr;
use wasm_encoder::{BlockType, ValType};

use crate::emitter::Emitter;
use crate::*;

/// How a member turns its value into the i64 whose low k bytes it appends.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Bits {
    Int,
    Bool,
    /// f64 bits, NaN collapsed to 0x7FF8000000000000 (`float.to_bits`).
    F64Canon,
    /// f32 bits of the demoted value, NaN collapsed to 0x7FC00000.
    F32Canon,
}

/// The byte order.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Order {
    Le,
    Be,
}

/// The family matrix: `(k, bits, order)` for every fixed-width member, `None`
/// for anything else (`append_u8` / `push` keep their own arm).
pub(crate) fn shape(func: &str) -> Option<(u32, Bits, Order)> {
    let fixed = |stem: &str| -> Option<(u32, Bits)> {
        Some(match stem {
            "u16" | "i16" => (2, Bits::Int),
            "u32" | "i32" => (4, Bits::Int),
            "i64" => (8, Bits::Int),
            "f32" => (4, Bits::F32Canon),
            "f64" => (8, Bits::F64Canon),
            _ => return None,
        })
    };
    if func == "append_bool" {
        return Some((1, Bits::Bool, Order::Be));
    }
    let rest = func.strip_prefix("append_")?;
    let (stem, order) = match (rest.strip_suffix("_le"), rest.strip_suffix("_be")) {
        (Some(s), _) => (s, Order::Le),
        (_, Some(s)) => (s, Order::Be),
        _ => return None,
    };
    let (k, bits) = fixed(stem)?;
    Some((k, bits, order))
}

/// Every member this file lowers: the fixed-width shapes.
pub(crate) fn is_member(func: &str) -> bool {
    shape(func).is_some()
}

impl Emitter<'_> {
    /// One member of the family: the receiver block on the stack (COW-read
    /// like `push`), then `k` calls to `$bytes_push`, then the push arm's
    /// settle and write-back.
    pub(crate) fn lower_bytes_append(&mut self, func: &str, args: &[IrExpr]) -> ArmResult {
        let Some((k, bits, order)) = shape(func) else {
            return unsup(&format!("bytes-append-shape:{func}"));
        };
        let [b, v] = args else {
            return unsup(&format!("bytes-append-arity:{func}"));
        };
        let recv = self.append_open(b)?;
        self.lower_append_bits(v, bits)?;
        let hv = self.hold_i64()?;
        self.f.instructions().local_set(hv);
        for j in 0..k {
            let shift = match order {
                Order::Le => i64::from(8 * j),
                Order::Be => i64::from(8 * (k - 1 - j)),
            };
            let mut i = self.f.instructions();
            i.local_get(hv);
            i.i64_const(shift);
            i.i64_shr_u().call(F_BYTES_PUSH);
        }
        self.release_i64();
        self.append_close(b, recv)
    }

    /// The receiver half `lower_bytes_push` runs: the block on the stack,
    /// made unique by the COW read (a parameter's too, #3342), so the helper
    /// frees the block it outgrows.
    fn append_open(&mut self, b: &IrExpr) -> Result<crate::bytes_recv::BytesRecv, EmitError> {
        let recv = self.bytes_recv("append", b)?;
        self.emit_read_bytes_recv(&recv, b)?;
        Ok(recv)
    }

    /// The final block is on the stack: write it back (`lower_bytes_push`'s
    /// tail).
    fn append_close(&mut self, _b: &IrExpr, recv: crate::bytes_recv::BytesRecv) -> ArmResult {
        self.emit_bytes_writeback(&recv)?;
        if let crate::bytes_recv::BytesRecv::Var { id, global, .. } = &recv {
            self.witness_mut_rebind(*id, *global);
        }
        Ok(None)
    }

    /// The value as the i64 whose low bytes are appended.
    fn lower_append_bits(&mut self, v: &IrExpr, bits: Bits) -> Result<(), EmitError> {
        match bits {
            Bits::Int => {
                self.lower_arg(v, Some(INT), ArgMode::Borrow)?;
            }
            Bits::Bool => {
                self.lower_arg(v, Some(BOOL), ArgMode::Borrow)?;
                self.f.instructions().i64_extend_i32_u();
            }
            Bits::F64Canon | Bits::F32Canon => {
                // A Float32 rides the Float (f64) carrier on this leg.
                self.lower_arg(v, Some(FLOAT), ArgMode::Borrow)?;
                self.emit_float_obs_bits(if bits == Bits::F64Canon { 8 } else { 4 })?;
            }
        }
        Ok(())
    }

    /// The f64 on the stack as the i64 of its observed bits, `width` 8 (f64)
    /// or 4 (the demoted f32, zero-extended). The ONE NaN rule of every bytes
    /// float writer on this leg — append, write and set alike (C-210, #3658):
    /// a NaN of any sign or payload becomes 0x7FF8000000000000 / 0x7FC00000.
    pub(crate) fn emit_float_obs_bits(&mut self, width: u32) -> Result<(), EmitError> {
        let h = self.hold_f64()?;
        let mut i = self.f.instructions();
        i.local_set(h);
        i.local_get(h).local_get(h).f64_ne().if_(BlockType::Result(ValType::I64));
        if width == 8 {
            i.i64_const(0x7FF8_0000_0000_0000_u64 as i64).else_();
            i.local_get(h).i64_reinterpret_f64();
        } else {
            i.i64_const(0x7FC0_0000).else_();
            i.local_get(h).f32_demote_f64().i32_reinterpret_f32().i64_extend_i32_u();
        }
        i.end();
        self.release_f64();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The matrix (#3294): every fixed-width append and writer has a shape,
    /// and the widths and orders are the native runtime's.
    #[test]
    fn every_member_of_the_family_has_its_shape() {
        for (stem, k) in [("u16", 2), ("i16", 2), ("u32", 4), ("i32", 4), ("i64", 8), ("f32", 4), ("f64", 8)] {
            for (sfx, order) in [("le", Order::Le), ("be", Order::Be)] {
                let got = shape(&format!("append_{stem}_{sfx}")).expect(stem);
                assert_eq!((got.0, got.2), (k, order), "append_{stem}_{sfx}");
            }
        }
        assert_eq!(shape("append_f32_le").map(|s| s.1), Some(Bits::F32Canon));
        assert_eq!(shape("append_f64_be").map(|s| s.1), Some(Bits::F64Canon));
        assert_eq!(shape("append_bool").map(|s| (s.0, s.2)), Some((1, Order::Be)));
        for not_fixed in ["append_u8", "push", "append_u24_le", "append_i64"] {
            assert!(shape(not_fixed).is_none(), "{not_fixed}");
        }
        assert!(!is_member("append_u8") && !is_member("push"));
    }
}
