//! The interpolation DISPLAY engine: one recursive lowering over the
//! type shape producing the oracle's exact repr forms (records, variants,
//! tuples, sums, lists, Rust-Debug string quoting in nested positions).
//! Split from calls.rs for the complexity budget.

use wasm_encoder::BlockType;

use crate::emitter::Emitter;
use crate::types_table::NamedDef;
use crate::*;

include!("display_map.rs");
include!("display_depth.rs");

impl Emitter<'_> {
    /// Append a static fragment to the line buffer.
    fn append_lit(&mut self, text: &str) {
        let base = self.pool.intern(text);
        self.f
            .instructions()
            .local_get(self.cursor_local)
            .i32_const((base + almide_layout::PAYLOAD) as i32)
            .i32_const(text.len() as i32)
            .call(F_APPEND_COPY)
            .local_set(self.cursor_local);
    }

    /// One `${part}` (or a nested position inside one): the value is on
    /// the stack; append its ORACLE display form to the line buffer and
    /// update the cursor local. `nested` = the Rust-Debug nesting rule
    /// (strings quote+escape inside containers, bare at the top).
    /// `ir` is the value's IR type when the caller has it: the slot type
    /// cannot tell a Float32 from a Float (both ride the f64 slot), and a
    /// Float32 — at the top or nested at any depth — prints its own shortest
    /// binary32 digits (C-372, #3081).
    pub(crate) fn emit_display_value(
        &mut self,
        got: SliceTy,
        nested: bool,
        ir: Option<&Ty>,
    ) -> Result<(), EmitError> {
        // The top level of a part: a runtime-backed value read straight out
        // of a frame variable hands its repr a share of that variable.
        let src = self.display_src.take();
        if let Some(i) = self.declared_repr_fn(ir) {
            return self.emit_declared_repr(i, src);
        }
        self.emit_display_at(got, nested, ir, &mut Vec::new())
    }

    /// The float on the stack, appended through the linked compound printer
    /// `key` names: `float.to_string_compound` for a Float, and for a
    /// Float32 `float32.to_string_compound` — the f32 shortest form native's
    /// f32 Display prints (#3079), which the widened f64 carrier would not.
    pub(crate) fn emit_float_display(&mut self, key: &str) -> Result<(), EmitError> {
        let Some(i) = self.resolve_qualified(key) else {
            return unsup("interp-part:Float-unlinked");
        };
        let info = &self.table.infos[i];
        if info.refuse.is_some() || info.ret != Some(STR) {
            return unsup("interp-part:Float-impl");
        }
        let idx = info.wasm_index;
        self.calls.insert(i);
        // The formatted block this call hands over is copied into the line
        // and released below (#2973): an owned temporary, born and released
        // here (`id`).
        self.witness_discard();
        self.f
            .instructions()
            .call(idx)
            .local_set(self.tmp_i32_local);
        self.f
            .instructions()
            .local_get(self.cursor_local)
            .local_get(self.tmp_i32_local)
            .i32_const(almide_layout::PAYLOAD as i32)
            .i32_add()
            .local_get(self.tmp_i32_local)
            .i32_load(len_memarg())
            .call(F_APPEND_COPY)
            .local_set(self.cursor_local);
        // #2973: the formatted text is this site's own block — the
        // bytes are copied into the line, so its credit ends here.
        let dec = self.dec_fn_of(STR);
        self.f.instructions().local_get(self.tmp_i32_local).call(dec);
        Ok(())
    }

    /// The i64 slot on the stack, appended as decimal digits. A `UInt64`
    /// reads the slot UNSIGNED (C-179, #3187): a pattern in the upper half
    /// (negative as an i64) prints as its `div_u 10` quotient — back in the
    /// signed-safe band — then the `rem_u 10` digit, the one-split
    /// `uint64.to_string` makes. Every other width prints the slot signed.
    fn emit_int_display(&mut self, unsigned: bool) {
        let (cur, x) = (self.cursor_local, self.scr_i64_local);
        let mut i = self.f.instructions();
        i.local_set(x);
        if unsigned {
            i.local_get(x).i64_const(0).i64_lt_s().if_(BlockType::Empty);
            i.local_get(cur).local_get(x).i64_const(10).i64_div_u().call(F_APPEND_I64).local_set(cur);
            i.local_get(x).i64_const(10).i64_rem_u().local_set(x);
            i.end();
        }
        i.local_get(cur).local_get(x).call(F_APPEND_I64).local_set(cur);
    }

    /// One inlined level of `got`'s display; components recurse through
    /// `emit_display_at` (display_depth.rs), which may outline them.
    fn emit_display_level(
        &mut self,
        got: SliceTy,
        nested: bool,
        ir: Option<&Ty>,
        path: &mut Vec<u32>,
    ) -> Result<(), EmitError> {
        // Emit-time recursion follows the TYPE SHAPE: a non-recursive
        // shape is a finite DAG and inlines fully; a CYCLE (recursive
        // Named type) is cut at the Named arm below with a call to the
        // runtime-recursive per-type helper.
        match got {
            INT => self.emit_int_display(matches!(ir, Some(Ty::UInt64))),
            BOOL => {
                self.f.instructions().local_set(self.tmp_i32_local);
                self.f
                    .instructions()
                    .local_get(self.cursor_local)
                    .local_get(self.tmp_i32_local)
                    .call(F_APPEND_BOOL)
                    .local_set(self.cursor_local);
            }
            FLOAT => {
                // The SAME linked Schubfach compound form the oracle uses —
                // at binary32 for a Float32 (C-372).
                if matches!(ir, Some(Ty::Float32)) {
                    self.emit_float_display("float32.to_string_compound")?;
                } else {
                    self.emit_float_display("float.to_string_compound")?;
                }
            }
            STR => {
                if nested {
                    // Rust-Debug quoting: the 5-escape repr walker (not
                    // JSON's — #2802 gave only JSON the control escapes).
                    let frags = self.json_frags();
                    let q = self.work.helper(Helper::ReprQuote { frags });
                    self.f.instructions().local_set(self.tmp_i32_local);
                    self.f
                        .instructions()
                        .local_get(self.cursor_local)
                        .local_get(self.tmp_i32_local)
                        .call(q)
                        .local_set(self.cursor_local);
                } else {
                    self.f.instructions().local_set(self.tmp_i32_local);
                    self.f
                        .instructions()
                        .local_get(self.cursor_local)
                        .local_get(self.tmp_i32_local)
                        .i32_const(almide_layout::PAYLOAD as i32)
                        .i32_add()
                        .local_get(self.tmp_i32_local)
                        .i32_load(len_memarg())
                        .call(F_APPEND_COPY)
                        .local_set(self.cursor_local);
                }
            }
            SliceTy::Option(h) => {
                let et = self.types.el(h);
                let ho = self.hold_i32()?;
                self.f.instructions().local_set(ho);
                self.f.instructions().local_get(ho).i32_eqz().if_(BlockType::Empty);
                self.append_lit("none");
                self.f.instructions().else_();
                self.append_lit("some(");
                self.f.instructions().local_get(ho);
                self.load_ty_slot(et, almide_layout::OPTION_FIELD);
                self.disp_last(")", |em| em.emit_display_at(et, true, ir_arg(ir, 0), path))?;
                self.append_lit(")");
                self.f.instructions().end();
                self.release_i32();
            }
            SliceTy::Result(o, e) => {
                let (ot, et) = (self.types.el(o), self.types.el(e));
                let hr = self.hold_i32()?;
                self.f.instructions().local_set(hr);
                self.f
                    .instructions()
                    .local_get(hr)
                    .i32_load(slot_memarg(almide_layout::SUM_TAG))
                    .i32_eqz()
                    .if_(BlockType::Empty);
                self.append_lit("ok(");
                self.f.instructions().local_get(hr);
                self.load_ty_slot(ot, almide_layout::SUM_FIELD);
                self.disp_last(")", |em| em.emit_display_at(ot, true, ir_arg(ir, 0), path))?;
                self.append_lit(")");
                self.f.instructions().else_();
                self.append_lit("err(");
                self.f.instructions().local_get(hr);
                self.load_ty_slot(et, almide_layout::SUM_FIELD);
                self.disp_last(")", |em| em.emit_display_at(et, true, ir_arg(ir, 1), path))?;
                self.append_lit(")");
                self.f.instructions().end();
                self.release_i32();
            }
            SliceTy::List(h) => {
                let el = self.types.el(h);
                let stride = el.slot_size() as i32;
                let hb = self.hold_i32()?;
                let end = self.hold_i32()?;
                let cur = self.hold_i32()?;
                self.f.instructions().local_set(hb);
                self.append_lit("[");
                {
                    let mut i = self.f.instructions();
                    i.local_get(hb)
                        .i32_const(almide_layout::PAYLOAD as i32)
                        .i32_add()
                        .local_set(cur);
                    i.local_get(cur)
                        .local_get(hb)
                        .i32_load(len_memarg())
                        .i32_add()
                        .local_set(end);
                    i.block(BlockType::Empty).loop_(BlockType::Empty);
                    i.local_get(cur).local_get(end).i32_ge_u().br_if(1);
                    i.local_get(cur)
                        .local_get(hb)
                        .i32_const(almide_layout::PAYLOAD as i32)
                        .i32_add()
                        .i32_ne()
                        .if_(BlockType::Empty);
                }
                self.append_lit(", ");
                self.f.instructions().end();
                self.f.instructions().local_get(cur);
                self.load_ty_slot_at(el);
                self.disp_off(|em| em.emit_display_at(el, true, ir_arg(ir, 0), path))?;
                {
                    let mut i = self.f.instructions();
                    i.local_get(cur).i32_const(stride).i32_add().local_set(cur);
                    i.br(0);
                    i.end();
                    i.end();
                }
                self.append_lit("]");
                self.release_i32();
                self.release_i32();
                self.release_i32();
            }
            SliceTy::Tuple(id) => {
                let fields = self.types.tuple_def(id).fields;
                let hb = self.hold_i32()?;
                self.f.instructions().local_set(hb);
                self.append_lit("(");
                let n = fields.len();
                for (k, (fty, off)) in fields.into_iter().enumerate() {
                    if k > 0 {
                        self.append_lit(", ");
                    }
                    self.f.instructions().local_get(hb);
                    self.load_ty_slot(fty, off);
                    self.disp_part((k + 1 == n).then_some(")"), |em| em.emit_display_at(fty, true, ir_arg(ir, k), path))?;
                }
                self.append_lit(")");
                self.release_i32();
            }
            SliceTy::Named(ti) => {
                if path.contains(&ti) || self.display_outlines(ti, path) {
                    // Recursive type (or one too deep for the hold pool):
                    // cut it with the runtime helper `(block, cursor) ->
                    // cursor`. A body that failed to build refuses THIS
                    // caller too.
                    let irk = self.work.display_ir_key(ir);
                    if matches!(
                        self.work.display_bodies.borrow().get(&(ti, irk)),
                        Some(crate::work::DisplayBuild::Failed)
                    ) {
                        return unsup("display-helper-failed");
                    }
                    let idx = self.work.helper(Helper::DisplayNamed { ti, irk });
                    self.emit_display_call(ti, idx)?;
                } else {
                    path.push(ti);
                    self.emit_display_named(ti, ir, path)?;
                    path.pop();
                }
            }
            // A set displays as its constructor call over the element
            // list (`set.from_list([3, 1, 2])`) — the set block IS the
            // element array, so the list walk does the middle.
            SliceTy::Set(h) => {
                self.append_lit("set.from_list(");
                self.disp_off(|em| em.emit_display_at(SliceTy::List(h), nested, ir, path))?;
                self.append_lit(")");
            }
            // `["k": v, …]` in insertion order; empty is the literal
            // `[:]` (the oracle's map repr).
            SliceTy::Map(kh, vh) => self.disp_off(|em| em.emit_display_map(kh, vh, ir, path))?,
            // A Value displays as its compact JSON — the SAME serializer
            // json.stringify uses (one repr, two spellings). The $vjson
            // helper appends AT THE DISPLAY CURSOR in place — the
            // stringify capture path scratches from G_LINE_CURSOR and
            // would clobber the interpolation already in the buffer.
            SliceTy::Value => {
                let Some(fi) = self.resolve_qualified("float.to_string") else {
                    return unsup("interp-part:Value-float-unlinked");
                };
                let info = &self.table.infos[fi];
                if info.refuse.is_some() || info.ret != Some(STR) {
                    return unsup("interp-part:Value-float-impl");
                }
                let float_idx = info.wasm_index;
                self.calls.insert(fi);
                let frags = self.json_frags();
                let ctrl = self.json_ctrl_frags();
                let _ = self.work.helper(Helper::JsonQuote { frags, ctrl });
                let vj = self.work.helper(Helper::JsonValue { float_to_string: float_idx, frags });
                let hv = self.hold_i32()?;
                self.f.instructions().local_set(hv);
                self.f
                    .instructions()
                    .local_get(self.cursor_local)
                    .local_get(hv)
                    .call(vj)
                    .local_set(self.cursor_local);
                self.release_i32();
            }
            // `${()}` / `ok(())`: the unit value (an i32 0) shows as `()`,
            // native's Debug of the unit (#2747).
            SliceTy::Unit => {
                self.f.instructions().drop();
                self.append_lit("()");
            }
            // A Bytes displays as the list of its byte values, `[97, 98]` —
            // the oracle's form (a Bytes is a List of Ints there) and native's
            // Debug of the byte vector. The block is byte-packed (len = byte
            // count), so the walk is the List one at stride 1, each byte read
            // unsigned and printed through the Int path.
            SliceTy::Scalar(Scalar::Bytes) => {
                let hb = self.hold_i32()?;
                let end = self.hold_i32()?;
                let cur = self.hold_i32()?;
                self.f.instructions().local_set(hb);
                self.append_lit("[");
                {
                    let mut i = self.f.instructions();
                    i.local_get(hb)
                        .i32_const(almide_layout::PAYLOAD as i32)
                        .i32_add()
                        .local_set(cur);
                    i.local_get(cur)
                        .local_get(hb)
                        .i32_load(len_memarg())
                        .i32_add()
                        .local_set(end);
                    i.block(BlockType::Empty).loop_(BlockType::Empty);
                    i.local_get(cur).local_get(end).i32_ge_u().br_if(1);
                    i.local_get(cur)
                        .local_get(hb)
                        .i32_const(almide_layout::PAYLOAD as i32)
                        .i32_add()
                        .i32_ne()
                        .if_(BlockType::Empty);
                }
                self.append_lit(", ");
                self.f.instructions().end();
                self.f
                    .instructions()
                    .local_get(cur)
                    .i32_load8_u(wasm_encoder::MemArg { offset: 0, align: 0, memory_index: 0 })
                    .i64_extend_i32_u();
                self.emit_int_display(false);
                {
                    let mut i = self.f.instructions();
                    i.local_get(cur).i32_const(1).i32_add().local_set(cur);
                    i.br(0);
                    i.end();
                    i.end();
                }
                self.append_lit("]");
                self.release_i32();
                self.release_i32();
                self.release_i32();
            }
            other => return unsup(&format!("interp-part:{other:?}")),
        }
        Ok(())
    }

    /// Records: `Nm { f: v, g: w }`; variants: `Case(v)` / bare unit
    /// names / record-shaped cases in the record form.
    /// `ir` is the record's own IR type when the caller has it: an
    /// ANONYMOUS record shape is interned by its slot types, so its fields
    /// carry no IR type of their own, and a UInt64 / Float32 field reads its
    /// digits from the enclosing `{ f: T }` type instead (#3187).
    pub(crate) fn emit_display_named(
        &mut self,
        ti: u32,
        ir: Option<&Ty>,
        path: &mut Vec<u32>,
    ) -> Result<(), EmitError> {
        let name = self.types.name_of(ti);
        match self.types.def(ti) {
            NamedDef::Record(def) => {
                let hb = self.hold_i32()?;
                self.f.instructions().local_set(hb);
                let mut fields: Vec<_> = def.fields.clone();
                if name.is_empty() {
                    // Anonymous record shape: "{ f: v }", fields in NAME
                    // order (the oracle's structural display).
                    self.append_lit("{ ");
                    fields.sort_by(|a, b| a.name.cmp(&b.name));
                } else {
                    // Both the entry program's shadow of a stdlib-owned name
                    // (`self.X`, #1828) and a module record (`m.Cfg`, #1836) show
                    // the last segment the source declares.
                    self.append_lit(&format!("{} {{ ", almide_ir::declared_type_name(&name)));
                }
                let n = fields.len();
                for (k, fi) in fields.iter().enumerate() {
                    if k > 0 {
                        self.append_lit(", ");
                    }
                    self.append_lit(&format!("{}: ", fi.name));
                    self.f.instructions().local_get(hb);
                    self.load_ty_slot(fi.ty, fi.offset);
                    let inst = self.types.instance_field_ir(ir, 0, &fi.name);
                    let fir = inst.as_ref().or(fi.ir.as_ref()).or_else(|| record_field_ir(ir, &fi.name));
                    self.disp_part((k + 1 == n).then_some(" }"), |em| em.emit_display_at(fi.ty, true, fir, path))?;
                }
                self.append_lit(" }");
                self.release_i32();
            }
            NamedDef::Variant(ref v) => self.display_variant(v, ir, path)?,
            NamedDef::Excluded => return unsup("interp-part:excluded"),
        }
        Ok(())
    }


    /// Variant display (split from emit_display_named for the complexity budget).
    fn display_variant(
        &mut self,
        v: &crate::types_table::VariantDef,
        ir: Option<&Ty>,
        path: &mut Vec<u32>,
    ) -> Result<(), EmitError> {

                let hb = self.hold_i32()?;
                self.f.instructions().local_set(hb);
                for (k, c) in v.cases.iter().enumerate() {
                    let last = k + 1 == v.cases.len();
                    if !last {
                        self.f
                            .instructions()
                            .local_get(hb)
                            .i32_load(slot_memarg(almide_layout::SUM_TAG))
                            .i32_const(c.tag as i32)
                            .i32_eq()
                            .if_(BlockType::Empty);
                    }
                    // Tuple cases carry synthetic "0","1",… field names;
                    // a real name means a RECORD-shaped case, which the
                    // oracle displays in the record form (C-008:
                    // `Rect { w: 3, h: 4 }`, not `Rect(3, 4)`).
                    let record_case =
                        c.fields.first().is_some_and(|f| f.name.parse::<usize>().is_err());
                    if c.fields.is_empty() {
                        self.append_lit(&c.name);
                    } else {
                        self.append_lit(&format!("{}{}", c.name, if record_case { " { " } else { "(" }));
                        let n = c.fields.len();
                        for (j, f) in c.fields.iter().enumerate() {
                            if j > 0 {
                                self.append_lit(", ");
                            }
                            if record_case {
                                self.append_lit(&format!("{}: ", f.name));
                            }
                            self.f.instructions().local_get(hb);
                            self.load_ty_slot(f.ty, f.offset);
                            let inst = self.types.instance_field_ir(ir, c.tag as usize, &f.name);
                            let fir = inst.as_ref().or(f.ir.as_ref());
                            let close = if record_case { " }" } else { ")" };
                            self.disp_part((j + 1 == n).then_some(close), |em| em.emit_display_at(f.ty, true, fir, path))?;
                        }
                        self.append_lit(if record_case { " }" } else { ")" });
                    }
                    if !last {
                        self.f.instructions().else_();
                    }
                }
                for _ in 0..v.cases.len().saturating_sub(1) {
                    self.f.instructions().end();
                }
                self.release_i32();
        Ok(())
    }
}

#[path = "display_helpers.rs"]
mod helpers;
pub(crate) use helpers::{build_display_helpers, ir_arg};
use helpers::{build_helper_body, record_field_ir, Shell};
