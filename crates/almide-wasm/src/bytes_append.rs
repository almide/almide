//! The k-byte APPEND family (#3294): `append_{u16,i16,u32,i32,i64,f32,f64}_
//! {le,be}`, the BE cursor writers (`write_u8/u16_be/u32_be/i64_be/f64_be`,
//! `write_bool`, `write_string_be`) and the Endian-typed writers
//! (`write_uint16/uint32/int32/float32`). Each one used to build a FRESH block
//! of `len + k` and copy the whole payload — the self-host twins (`__bam`,
//! `__bt_append`, `bytes_write_string_be`) and the inline BE writer alike — so
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
//! bytes (`band(bshr_u(bits, 8i), 255)`), `append_f32_*` / `append_f64_*`
//! canonicalize NaN first (C-210: 0x7FC00000 / `float.to_bits`), the BE
//! writer's `write_f64_be` and the typed `write_float32` keep their raw
//! reinterpret, and the typed writers read the byte order off the `Endian`
//! value's tag at run time.

use almide_ir::IrExpr;
use wasm_encoder::{BlockType, MemArg, ValType};

use crate::emitter::Emitter;
use crate::*;

/// How a member turns its value into the i64 whose low k bytes it appends.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Bits {
    Int,
    Bool,
    /// f64 bits, NaN collapsed to 0x7FF8000000000000 (`float.to_bits`).
    F64Canon,
    /// f64 bits as they are (`write_f64_be`).
    F64Raw,
    /// f32 bits of the demoted value, NaN collapsed to 0x7FC00000.
    F32Canon,
    /// f32 bits of the demoted value as they are (`write_float32`).
    F32Raw,
}

/// The byte order: fixed, or read from the trailing `Endian` argument.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Order {
    Le,
    Be,
    Endian,
}

/// The family matrix: `(k, bits, order)` for every fixed-width member, `None`
/// for anything else (`append_u8` / `push` keep their own arm, and
/// `write_string_be` is variable-width — `lower_bytes_write_string_be`).
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
    if let Some(rest) = func.strip_prefix("append_") {
        let (stem, order) = match (rest.strip_suffix("_le"), rest.strip_suffix("_be")) {
            (Some(s), _) => (s, Order::Le),
            (_, Some(s)) => (s, Order::Be),
            _ => return None,
        };
        let (k, bits) = fixed(stem)?;
        return Some((k, bits, order));
    }
    Some(match func {
        "write_u8" => (1, Bits::Int, Order::Be),
        "write_bool" => (1, Bits::Bool, Order::Be),
        "write_u16_be" => (2, Bits::Int, Order::Be),
        "write_u32_be" => (4, Bits::Int, Order::Be),
        "write_i64_be" => (8, Bits::Int, Order::Be),
        "write_f64_be" => (8, Bits::F64Raw, Order::Be),
        "write_uint16" => (2, Bits::Int, Order::Endian),
        "write_uint32" | "write_int32" => (4, Bits::Int, Order::Endian),
        "write_float32" => (4, Bits::F32Raw, Order::Endian),
        _ => return None,
    })
}

/// Every member this file lowers: the fixed-width shapes and `write_string_be`.
pub(crate) fn is_member(func: &str) -> bool {
    func == "write_string_be" || shape(func).is_some()
}

impl Emitter<'_> {
    /// One member of the family: the receiver block on the stack (COW-read
    /// like `push`), then `k` calls to `$bytes_push`, then the push arm's
    /// settle and write-back.
    pub(crate) fn lower_bytes_append(&mut self, func: &str, args: &[IrExpr]) -> ArmResult {
        if let ("write_string_be", [b, s]) = (func, args) {
            return self.lower_bytes_write_string_be(b, s);
        }
        let Some((k, bits, order)) = shape(func) else {
            return unsup(&format!("bytes-append-shape:{func}"));
        };
        let (b, v, endian) = match (args, order) {
            ([b, v], Order::Le | Order::Be) => (b, v, None),
            ([b, v, e], Order::Endian) => (b, v, Some(e)),
            _ => return unsup(&format!("bytes-append-arity:{func}")),
        };
        let recv = self.append_open(b)?;
        self.lower_append_bits(v, bits)?;
        let hv = self.hold_i64()?;
        self.f.instructions().local_set(hv);
        let hle = endian.map(|e| self.lower_endian_is_le(e)).transpose()?;
        for j in 0..k {
            let (le_shift, be_shift) = (i64::from(8 * j), i64::from(8 * (k - 1 - j)));
            let mut i = self.f.instructions();
            i.local_get(hv);
            match (order, hle) {
                (Order::Le, _) => i.i64_const(le_shift),
                (Order::Be, _) => i.i64_const(be_shift),
                (Order::Endian, Some(h)) => {
                    i.local_get(h).if_(BlockType::Result(ValType::I64));
                    i.i64_const(le_shift).else_().i64_const(be_shift).end()
                }
                (Order::Endian, None) => unreachable!("an Endian member always has its flag"),
            };
            i.i64_shr_u().call(F_BYTES_PUSH);
        }
        if hle.is_some() {
            self.release_i32();
        }
        self.release_i64();
        self.append_close(b, recv)
    }

    /// `write_string_be(b, s)`: a u32 BE byte-length prefix, then the
    /// string's UTF-8 bytes — each byte one `$bytes_push`.
    fn lower_bytes_write_string_be(&mut self, b: &IrExpr, s: &IrExpr) -> ArmResult {
        let recv = self.append_open(b)?;
        let hb = self.hold_i32()?;
        self.f.instructions().local_set(hb);
        self.lower_arg(s, Some(STR), ArgMode::Borrow)?;
        let hs = self.hold_i32()?;
        let hn = self.hold_i32()?;
        let hi = self.hold_i32()?;
        let byte = MemArg { offset: u64::from(almide_layout::PAYLOAD), align: 0, memory_index: 0 };
        let mut i = self.f.instructions();
        i.local_tee(hs).i32_load(len_memarg()).local_set(hn);
        i.local_get(hb);
        for j in 0..4u32 {
            i.local_get(hn).i64_extend_i32_u().i64_const(i64::from(8 * (3 - j))).i64_shr_u();
            i.call(F_BYTES_PUSH);
        }
        i.local_set(hb);
        i.i32_const(0).local_set(hi);
        i.block(BlockType::Empty).loop_(BlockType::Empty);
        i.local_get(hi).local_get(hn).i32_ge_u().br_if(1);
        i.local_get(hb);
        i.local_get(hs).local_get(hi).i32_add().i32_load8_u(byte).i64_extend_i32_u();
        i.call(F_BYTES_PUSH).local_set(hb);
        i.local_get(hi).i32_const(1).i32_add().local_set(hi);
        i.br(0).end().end();
        i.local_get(hb);
        self.release_i32();
        self.release_i32();
        self.release_i32();
        self.release_i32();
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
            Bits::F64Raw => {
                self.lower_arg(v, Some(FLOAT), ArgMode::Borrow)?;
                self.f.instructions().i64_reinterpret_f64();
            }
            Bits::F32Raw => {
                // A Float32 rides the Float (f64) carrier on this leg.
                self.lower_arg(v, Some(FLOAT), ArgMode::Borrow)?;
                self.f.instructions().f32_demote_f64().i32_reinterpret_f32().i64_extend_i32_u();
            }
            Bits::F64Canon | Bits::F32Canon => {
                self.lower_arg(v, Some(FLOAT), ArgMode::Borrow)?;
                let h = self.hold_f64()?;
                let mut i = self.f.instructions();
                i.local_set(h);
                i.local_get(h).local_get(h).f64_ne().if_(BlockType::Result(ValType::I64));
                if bits == Bits::F64Canon {
                    i.i64_const(0x7FF8_0000_0000_0000_u64 as i64).else_();
                    i.local_get(h).i64_reinterpret_f64();
                } else {
                    i.i64_const(0x7FC0_0000).else_();
                    i.local_get(h).f32_demote_f64().i32_reinterpret_f32().i64_extend_i32_u();
                }
                i.end();
                self.release_f64();
            }
        }
        Ok(())
    }

    /// `e == LittleEndian`, as an i32 flag held in a local (the caller
    /// releases it): the case tag of the declared `Endian` variant.
    fn lower_endian_is_le(&mut self, e: &IrExpr) -> Result<u32, EmitError> {
        let got = self.lower_arg(e, None, ArgMode::Borrow)?;
        let SliceTy::Named(ti) = got else {
            return unsup(&format!("bytes-endian-ty:{got:?}"));
        };
        let le_tag = match self.types.def(ti) {
            crate::types_table::NamedDef::Variant(v) => v.cases.iter().find(|c| c.name == "LittleEndian").map(|c| c.tag),
            _ => None,
        };
        let Some(le_tag) = le_tag else {
            return unsup("bytes-endian-case");
        };
        let h = self.hold_i32()?;
        let mut i = self.f.instructions();
        i.i32_load(slot_memarg(almide_layout::SUM_TAG)).i32_const(le_tag as i32).i32_eq().local_set(h);
        Ok(h)
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
        for (f, k) in [("write_u8", 1), ("write_bool", 1), ("write_u16_be", 2), ("write_u32_be", 4), ("write_i64_be", 8), ("write_f64_be", 8)] {
            assert_eq!(shape(f).map(|s| (s.0, s.2)), Some((k, Order::Be)), "{f}");
        }
        for (f, k) in [("write_uint16", 2), ("write_uint32", 4), ("write_int32", 4), ("write_float32", 4)] {
            assert_eq!(shape(f).map(|s| (s.0, s.2)), Some((k, Order::Endian)), "{f}");
        }
        for not_fixed in ["append_u8", "push", "write_string_be", "append_u24_le", "append_i64"] {
            assert!(shape(not_fixed).is_none(), "{not_fixed}");
        }
        assert!(is_member("write_string_be") && !is_member("append_u8") && !is_member("push"));
    }
}
