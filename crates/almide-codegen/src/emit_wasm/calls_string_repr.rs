//! Recursive Almide-literal repr for compound string interpolation (WASM).
//!
//! `"${[1, 2]}"`, `"${["a": 1]}"`, `"${(1, "x")}"`, … must render a value back
//! to its Almide-literal form, byte-identically with the native target. The walk
//! is driven by the STATIC `Ty` at the interpolation site (WASM carries no
//! runtime type tags), so each contained shape is specialized at emit time and
//! recursion bottoms out at primitives.
//!
//! Contract (mirrors the native `AlmideRepr` impls):
//!   List   → `[a, b, c]`            empty `[]`
//!   Map    → `[k: v, k: v]`         empty `[:]`   (brackets, Swift-style)
//!   Set    → `set.from_list([...])` (no set literal in the language)
//!   Tuple  → `(a, b)`
//!   Option → `some(v)` / `none`
//!   Result → `ok(v)` / `err(e)`
//!   String inside a container → double-quoted + escaped (`__repr_str`)
//!   Int/Bool/Float → same text as bare interpolation (shared rt drivers)
//!
//! Each `emit_repr_value` expects the value on the stack and leaves a string
//! pointer on the stack. Strings are joined with `__concat_str` and interned
//! literal separators (`[`, `, `, `]`, `: `, `some(`, …).

use super::FuncCompiler;
use almide_lang::types::Ty;
use almide_lang::types::constructor::TypeConstructorId;
use super::values;

/// A `Result` cell is `[tag:i32][payload]` — the payload sits one i32 (the tag)
/// in. Mirrors the `ResultOk`/`ResultErr` emit in `expressions.rs`.
const RESULT_PAYLOAD_OFFSET: u32 = 4;

impl FuncCompiler<'_> {
    /// Repr a value of type `ty` already on the stack → string pointer on stack.
    /// Used by `emit_string_part` for compound interpolation parts and by the
    /// recursive container walks below.
    pub(super) fn emit_repr_value(&mut self, ty: &Ty) {
        match ty {
            // ── Primitives: route through the SAME drivers as bare interpolation ──
            Ty::String => {
                wasm!(self.func, { call(self.emitter.rt.repr_str); });
            }
            Ty::Int | Ty::Int64 | Ty::UInt64 => {
                wasm!(self.func, { call(self.emitter.rt.int_to_string); });
            }
            // Sized ints ride in the i32 bucket → widen to i64 for int_to_string.
            Ty::Int8 | Ty::Int16 | Ty::Int32 => {
                wasm!(self.func, { i64_extend_i32_s; call(self.emitter.rt.int_to_string); });
            }
            Ty::UInt8 | Ty::UInt16 | Ty::UInt32 => {
                wasm!(self.func, { i64_extend_i32_u; call(self.emitter.rt.int_to_string); });
            }
            Ty::Float | Ty::Float64 => {
                wasm!(self.func, { call(self.emitter.rt.float_to_string); });
            }
            Ty::Float32 => {
                wasm!(self.func, { f64_promote_f32; call(self.emitter.rt.float_to_string); });
            }
            Ty::Bool => {
                let t = self.emitter.intern_string("true");
                let f = self.emitter.intern_string("false");
                wasm!(self.func, {
                    if_i32; i32_const(t as i32); else_; i32_const(f as i32); end;
                });
            }
            // ── Containers ──
            Ty::Applied(TypeConstructorId::List, _) => {
                let elem_ty = self.list_elem_ty(ty);
                self.emit_repr_list(&elem_ty, "[", "]", "[]");
            }
            Ty::Applied(TypeConstructorId::Set, _) => {
                let elem_ty = self.list_elem_ty(ty);
                // No set literal → constructor form `set.from_list([...])`.
                self.emit_repr_list(&elem_ty, "set.from_list([", "])", "set.from_list([])");
            }
            Ty::Applied(TypeConstructorId::Map, _) => {
                self.emit_repr_map(ty);
            }
            Ty::Applied(TypeConstructorId::Option, args) => {
                let inner = args.first().cloned().unwrap_or(Ty::Int);
                self.emit_repr_option(&inner);
            }
            Ty::Applied(TypeConstructorId::Result, args) => {
                let ok_ty = args.first().cloned().unwrap_or(Ty::Int);
                let err_ty = args.get(1).cloned().unwrap_or(Ty::String);
                self.emit_repr_result(&ok_ty, &err_ty);
            }
            Ty::Tuple(elems) => {
                let elems = elems.clone();
                self.emit_repr_tuple(&elems);
            }
            // Anything else is not a backed compound (records/variants are scoped
            // out — see `ty_needs_repr`); leave the value as-is. Unreachable in
            // practice because the walker only routes backed shapes here.
            _ => {}
        }
    }

    /// List / Set walk: `open` + elem reprs joined by `, ` + `close`.
    /// `empty` is the exact literal for a zero-length collection.
    /// A List and a Set share the `[len:i32][cap:i32][data...]` layout, so one
    /// helper covers both — only the wrapper text differs.
    fn emit_repr_list(&mut self, elem_ty: &Ty, open: &str, close: &str, empty: &str) {
        use super::engine::layout::{LIST, list as ll};
        let data_off = self.emitter.layout_reg.fixed_offset(LIST, ll::DATA) as i32;
        let elem_size = values::byte_size(elem_ty) as i32;
        let open_s = self.emitter.intern_string(open) as i32;
        let close_s = self.emitter.intern_string(close) as i32;
        let empty_s = self.emitter.intern_string(empty) as i32;
        let sep_s = self.emitter.intern_string(", ") as i32;

        let lst = self.scratch.alloc_i32();
        let len = self.scratch.alloc_i32();
        let i = self.scratch.alloc_i32();
        let acc = self.scratch.alloc_i32();
        let concat = self.emitter.rt.concat_str;

        wasm!(self.func, {
            local_set(lst);
            local_get(lst); i32_load(0); local_set(len);
            // Empty → exact literal (`[]`, `[:]` is map-only, `set.from_list([])`).
            local_get(len); i32_eqz;
            if_i32;
              i32_const(empty_s);
            else_;
              i32_const(open_s); local_set(acc);
              i32_const(0); local_set(i);
              block_empty; loop_empty;
                local_get(i); local_get(len); i32_ge_u; br_if(1);
                // separator before every element except the first
                local_get(i); i32_eqz;
                if_empty; else_;
                  local_get(acc); i32_const(sep_s); call(concat); local_set(acc);
                end;
                // acc = acc ++ repr(elem[i])
                local_get(acc);
                // load elem[i] onto stack: addr = lst + data_off + i*elem_size
                local_get(lst); i32_const(data_off); i32_add;
                local_get(i); i32_const(elem_size); i32_mul; i32_add;
            });
            self.emit_load_at(elem_ty, 0);
            self.emit_repr_value(elem_ty);
            wasm!(self.func, {
                call(concat); local_set(acc);
                local_get(i); i32_const(1); i32_add; local_set(i);
                br(0);
              end; end;
              local_get(acc); i32_const(close_s); call(concat);
            end;
        });

        self.scratch.free_i32(acc);
        self.scratch.free_i32(i);
        self.scratch.free_i32(len);
        self.scratch.free_i32(lst);
    }

    /// Map walk over the dense, insertion-ordered entries (compact-ordered-dict):
    /// `[k: v, k: v]`, empty `[:]`. Entry stride `es = ks + vs`; key @ +0, val @ +ks.
    fn emit_repr_map(&mut self, map_ty: &Ty) {
        use super::engine::layout::{SWISS_MAP, map as lm};
        let (ks, vs) = self.map_kv_sizes(map_ty);
        let es = (ks + vs) as i32;
        let (key_ty, val_ty) = match map_ty {
            Ty::Applied(_, args) => (
                args.first().cloned().unwrap_or(Ty::Int),
                args.get(1).cloned().unwrap_or(Ty::Int),
            ),
            _ => (Ty::Int, Ty::Int),
        };
        let cap_off = self.emitter.layout_reg.fixed_offset(SWISS_MAP, lm::CAP);
        let open_s = self.emitter.intern_string("[") as i32;
        let close_s = self.emitter.intern_string("]") as i32;
        let empty_s = self.emitter.intern_string("[:]") as i32;
        let sep_s = self.emitter.intern_string(", ") as i32;
        let kv_s = self.emitter.intern_string(": ") as i32;

        let m = self.scratch.alloc_i32();
        let len = self.scratch.alloc_i32();
        let cap = self.scratch.alloc_i32();
        let eb = self.scratch.alloc_i32();
        let i = self.scratch.alloc_i32();
        let acc = self.scratch.alloc_i32();
        let entry = self.scratch.alloc_i32();
        let concat = self.emitter.rt.concat_str;

        wasm!(self.func, {
            local_set(m);
            local_get(m); i32_load(0); local_set(len);
            local_get(m); i32_load(cap_off); local_set(cap);
        });
        self.emit_dict_entries_base(m, cap);
        wasm!(self.func, {
            local_set(eb);
            local_get(len); i32_eqz;
            if_i32;
              i32_const(empty_s);
            else_;
              i32_const(open_s); local_set(acc);
              i32_const(0); local_set(i);
              block_empty; loop_empty;
                local_get(i); local_get(len); i32_ge_u; br_if(1);
                local_get(i); i32_eqz;
                if_empty; else_;
                  local_get(acc); i32_const(sep_s); call(concat); local_set(acc);
                end;
                // entry = eb + i*es
                local_get(eb); local_get(i); i32_const(es); i32_mul; i32_add;
                local_set(entry);
                // acc ++ repr(key)
                local_get(acc);
                local_get(entry);
        });
        self.emit_load_at(&key_ty, 0);
        self.emit_repr_value(&key_ty);
        wasm!(self.func, {
                call(concat);
                i32_const(kv_s); call(concat); local_set(acc);
                // acc ++ repr(value)   (value at entry + ks)
                local_get(acc);
                local_get(entry); i32_const(ks as i32); i32_add;
        });
        self.emit_load_at(&val_ty, 0);
        self.emit_repr_value(&val_ty);
        wasm!(self.func, {
                call(concat); local_set(acc);
                local_get(i); i32_const(1); i32_add; local_set(i);
                br(0);
              end; end;
              local_get(acc); i32_const(close_s); call(concat);
            end;
        });

        self.scratch.free_i32(entry);
        self.scratch.free_i32(acc);
        self.scratch.free_i32(i);
        self.scratch.free_i32(eb);
        self.scratch.free_i32(cap);
        self.scratch.free_i32(len);
        self.scratch.free_i32(m);
    }

    /// Tuple walk: `(a, b, …)`. Fields laid out sequentially; offset = sum of
    /// preceding field byte sizes (matches `emit_tuple` / `emit_tuple_index`).
    fn emit_repr_tuple(&mut self, elems: &[Ty]) {
        let open_s = self.emitter.intern_string("(") as i32;
        let close_s = self.emitter.intern_string(")") as i32;
        let sep_s = self.emitter.intern_string(", ") as i32;

        let tp = self.scratch.alloc_i32();
        let acc = self.scratch.alloc_i32();
        let concat = self.emitter.rt.concat_str;

        wasm!(self.func, {
            local_set(tp);
            i32_const(open_s); local_set(acc);
        });
        let mut offset = 0u32;
        for (idx, elem_ty) in elems.iter().enumerate() {
            if idx > 0 {
                wasm!(self.func, { local_get(acc); i32_const(sep_s); call(concat); local_set(acc); });
            }
            wasm!(self.func, {
                local_get(acc);
                local_get(tp); i32_const(offset as i32); i32_add;
            });
            self.emit_load_at(elem_ty, 0);
            self.emit_repr_value(elem_ty);
            wasm!(self.func, { call(concat); local_set(acc); });
            offset += values::byte_size(elem_ty);
        }
        wasm!(self.func, { local_get(acc); i32_const(close_s); call(concat); });

        self.scratch.free_i32(acc);
        self.scratch.free_i32(tp);
    }

    /// Option walk: `some(v)` / `none`. WASM repr: null pointer = None, else the
    /// payload is stored at offset 0 of the allocated cell.
    fn emit_repr_option(&mut self, inner_ty: &Ty) {
        let some_s = self.emitter.intern_string("some(") as i32;
        let close_s = self.emitter.intern_string(")") as i32;
        let none_s = self.emitter.intern_string("none") as i32;

        let opt = self.scratch.alloc_i32();
        let concat = self.emitter.rt.concat_str;

        wasm!(self.func, {
            local_set(opt);
            local_get(opt); i32_eqz;
            if_i32;
              i32_const(none_s);
            else_;
              i32_const(some_s);
              local_get(opt);
        });
        self.emit_load_at(inner_ty, 0);
        self.emit_repr_value(inner_ty);
        wasm!(self.func, {
              call(concat);
              i32_const(close_s); call(concat);
            end;
        });

        self.scratch.free_i32(opt);
    }

    /// Result walk: `ok(v)` / `err(e)`. WASM repr: `[tag:i32][payload]`, tag 0 =
    /// ok, 1 = err; payload at offset 4.
    fn emit_repr_result(&mut self, ok_ty: &Ty, err_ty: &Ty) {
        let ok_s = self.emitter.intern_string("ok(") as i32;
        let err_s = self.emitter.intern_string("err(") as i32;
        let close_s = self.emitter.intern_string(")") as i32;

        let res = self.scratch.alloc_i32();
        let concat = self.emitter.rt.concat_str;

        wasm!(self.func, {
            local_set(res);
            // tag == 0 → ok branch
            local_get(res); i32_load(0); i32_eqz;
            if_i32;
              i32_const(ok_s);
              local_get(res);
        });
        self.emit_load_at(ok_ty, RESULT_PAYLOAD_OFFSET);
        self.emit_repr_value(ok_ty);
        wasm!(self.func, {
              call(concat); i32_const(close_s); call(concat);
            else_;
              i32_const(err_s);
              local_get(res);
        });
        self.emit_load_at(err_ty, RESULT_PAYLOAD_OFFSET);
        self.emit_repr_value(err_ty);
        wasm!(self.func, {
              call(concat); i32_const(close_s); call(concat);
            end;
        });

        self.scratch.free_i32(res);
    }
}
