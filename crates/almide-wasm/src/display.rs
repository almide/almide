//! The interpolation DISPLAY engine: one recursive lowering over the
//! type shape producing the oracle's exact repr forms (records, variants,
//! tuples, sums, lists, Rust-Debug string quoting in nested positions).
//! Split from calls.rs for the complexity budget.

use wasm_encoder::BlockType;

use crate::emitter::Emitter;
use crate::types_table::NamedDef;
use crate::*;

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
    pub(crate) fn emit_display_value(
        &mut self,
        got: SliceTy,
        nested: bool,
    ) -> Result<(), EmitError> {
        self.emit_display_at(got, nested, 0)
    }

    fn emit_display_at(
        &mut self,
        got: SliceTy,
        nested: bool,
        depth: u32,
    ) -> Result<(), EmitError> {
        // Emit-time recursion follows the TYPE shape; recursive data
        // types need a runtime-recursive helper — capped honestly.
        if depth > 8 {
            return unsup("display-depth");
        }
        match got {
            INT => {
                self.f.instructions().local_set(self.scr_i64_local);
                self.f
                    .instructions()
                    .local_get(self.cursor_local)
                    .local_get(self.scr_i64_local)
                    .call(F_APPEND_I64)
                    .local_set(self.cursor_local);
            }
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
                // The SAME linked Dragon4 compound form the oracle uses.
                let Some(i) = self.resolve_qualified("float.to_string_compound") else {
                    return unsup("interp-part:Float-unlinked");
                };
                let info = &self.table.infos[i];
                if info.refuse.is_some() || info.ret != Some(STR) {
                    return unsup("interp-part:Float-impl");
                }
                let idx = info.wasm_index;
                self.calls.insert(i);
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
            }
            STR => {
                if nested {
                    // Rust-Debug quoting shares the 5-escape walker.
                    let frags = self.json_frags();
                    let q = self.work.helper(Helper::JsonQuote { frags });
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
                self.emit_display_at(et, true, depth + 1)?;
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
                self.emit_display_at(ot, true, depth + 1)?;
                self.append_lit(")");
                self.f.instructions().else_();
                self.append_lit("err(");
                self.f.instructions().local_get(hr);
                self.load_ty_slot(et, almide_layout::SUM_FIELD);
                self.emit_display_at(et, true, depth + 1)?;
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
                self.emit_display_at(el, true, depth + 1)?;
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
                for (k, (fty, off)) in fields.into_iter().enumerate() {
                    if k > 0 {
                        self.append_lit(", ");
                    }
                    self.f.instructions().local_get(hb);
                    self.load_ty_slot(fty, off);
                    self.emit_display_at(fty, true, depth + 1)?;
                }
                self.append_lit(")");
                self.release_i32();
            }
            SliceTy::Named(ti) => {
                self.emit_display_named(ti, depth)?;
            }
            other => return unsup(&format!("interp-part:{other:?}")),
        }
        Ok(())
    }

    /// Records: `Nm { f: v, g: w }`; variants: `Case(v)` / bare unit
    /// names / record-shaped cases in the record form.
    fn emit_display_named(&mut self, ti: u32, depth: u32) -> Result<(), EmitError> {
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
                    self.append_lit(&format!("{name} {{ "));
                }
                for (k, fi) in fields.iter().enumerate() {
                    if k > 0 {
                        self.append_lit(", ");
                    }
                    self.append_lit(&format!("{}: ", fi.name));
                    self.f.instructions().local_get(hb);
                    self.load_ty_slot(fi.ty, fi.offset);
                    self.emit_display_at(fi.ty, true, depth + 1)?;
                }
                self.append_lit(" }");
                self.release_i32();
            }
            NamedDef::Variant(ref v) => self.display_variant(v, depth)?,
            NamedDef::Excluded => return unsup("interp-part:excluded"),
        }
        Ok(())
    }


    /// Variant display (split from emit_display_named for the complexity budget).
    fn display_variant(&mut self, v: &crate::types_table::VariantDef, depth: u32) -> Result<(), EmitError> {

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
                    if c.fields.is_empty() {
                        self.append_lit(&c.name);
                    } else {
                        self.append_lit(&format!("{}(", c.name));
                        for (j, f) in c.fields.iter().enumerate() {
                            if j > 0 {
                                self.append_lit(", ");
                            }
                            self.f.instructions().local_get(hb);
                            self.load_ty_slot(f.ty, f.offset);
                            self.emit_display_at(f.ty, true, depth + 1)?;
                        }
                        self.append_lit(")");
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
