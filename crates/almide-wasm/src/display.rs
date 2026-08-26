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
        self.emit_display_at(got, nested, &mut Vec::new())
    }

    fn emit_display_at(
        &mut self,
        got: SliceTy,
        nested: bool,
        path: &mut Vec<u32>,
    ) -> Result<(), EmitError> {
        // Emit-time recursion follows the TYPE SHAPE: a non-recursive
        // shape is a finite DAG and inlines fully; a CYCLE (recursive
        // Named type) is cut at the Named arm below with a call to the
        // runtime-recursive per-type helper.
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
                self.emit_display_at(et, true, path)?;
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
                self.emit_display_at(ot, true, path)?;
                self.append_lit(")");
                self.f.instructions().else_();
                self.append_lit("err(");
                self.f.instructions().local_get(hr);
                self.load_ty_slot(et, almide_layout::SUM_FIELD);
                self.emit_display_at(et, true, path)?;
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
                self.emit_display_at(el, true, path)?;
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
                    self.emit_display_at(fty, true, path)?;
                }
                self.append_lit(")");
                self.release_i32();
            }
            SliceTy::Named(ti) => {
                if path.contains(&ti) {
                    // Recursive type: cut the cycle with the runtime
                    // helper `(block, cursor) -> cursor`. A body that
                    // failed to build refuses THIS caller too.
                    if matches!(
                        self.work.display_bodies.borrow().get(&ti),
                        Some(crate::work::DisplayBuild::Failed)
                    ) {
                        return unsup("display-helper-failed");
                    }
                    let idx = self.work.helper(Helper::DisplayNamed { ti });
                    self.f
                        .instructions()
                        .local_get(self.cursor_local)
                        .call(idx)
                        .local_set(self.cursor_local);
                } else {
                    path.push(ti);
                    self.emit_display_named(ti, path)?;
                    path.pop();
                }
            }
            // A set displays as its constructor call over the element
            // list (`set.from_list([3, 1, 2])`) — the set block IS the
            // element array, so the list walk does the middle.
            SliceTy::Set(h) => {
                self.append_lit("set.from_list(");
                self.emit_display_at(SliceTy::List(h), nested, path)?;
                self.append_lit(")");
            }
            // `["k": v, …]` in insertion order; empty is the literal
            // `[:]` (the oracle's map repr).
            SliceTy::Map(kh, vh) => {
                let (k, v) = (self.types.el(kh), self.types.el(vh));
                let (koff, voff, esz) = crate::collections::entry_layout(k, v);
                let hb = self.hold_i32()?;
                let end = self.hold_i32()?;
                let cur = self.hold_i32()?;
                self.f.instructions().local_set(hb);
                self.f.instructions().local_get(hb).i32_load(len_memarg()).i32_eqz();
                self.f.instructions().if_(BlockType::Empty);
                self.append_lit("[:]");
                self.f.instructions().else_();
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
                self.f.instructions().local_get(cur).i32_const(koff as i32).i32_add();
                self.load_ty_slot_at(k);
                self.emit_display_at(k, true, path)?;
                self.append_lit(": ");
                self.f.instructions().local_get(cur).i32_const(voff as i32).i32_add();
                self.load_ty_slot_at(v);
                self.emit_display_at(v, true, path)?;
                {
                    let mut i = self.f.instructions();
                    i.local_get(cur).i32_const(esz as i32).i32_add().local_set(cur);
                    i.br(0);
                    i.end();
                    i.end();
                }
                self.append_lit("]");
                self.f.instructions().end();
                self.release_i32();
                self.release_i32();
                self.release_i32();
            }
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
                let _ = self.work.helper(Helper::JsonQuote { frags });
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
            other => return unsup(&format!("interp-part:{other:?}")),
        }
        Ok(())
    }

    /// Records: `Nm { f: v, g: w }`; variants: `Case(v)` / bare unit
    /// names / record-shaped cases in the record form.
    pub(crate) fn emit_display_named(&mut self, ti: u32, path: &mut Vec<u32>) -> Result<(), EmitError> {
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
                    self.emit_display_at(fi.ty, true, path)?;
                }
                self.append_lit(" }");
                self.release_i32();
            }
            NamedDef::Variant(ref v) => self.display_variant(v, path)?,
            NamedDef::Excluded => return unsup("interp-part:excluded"),
        }
        Ok(())
    }


    /// Variant display (split from emit_display_named for the complexity budget).
    fn display_variant(&mut self, v: &crate::types_table::VariantDef, path: &mut Vec<u32>) -> Result<(), EmitError> {

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
                        for (j, f) in c.fields.iter().enumerate() {
                            if j > 0 {
                                self.append_lit(", ");
                            }
                            if record_case {
                                self.append_lit(&format!("{}: ", f.name));
                            }
                            self.f.instructions().local_get(hb);
                            self.load_ty_slot(f.ty, f.offset);
                            self.emit_display_at(f.ty, true, path)?;
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

/// The display-helper build phase: bodies for every registered
/// `DisplayNamed` (fixed point — a body may register more). Called after
/// each successful `lower_fn`; a failing body refuses THAT fn, keeping
/// per-fn refusal granularity, and is marked Failed so later callers
/// refuse themselves (assembly stubs the promised index).
pub(crate) fn build_display_helpers(
    table: &FnTable,
    types: &TypeTable,
    work: &FnWork,
    pool: &mut Pool,
) -> Result<std::collections::HashSet<usize>, EmitError> {
    let mut all_calls = std::collections::HashSet::new();
    loop {
        let (todo, todo_eq): (Vec<u32>, Vec<u32>) = {
            let hs = work.helpers.borrow();
            let bodies = work.display_bodies.borrow();
            let eq_bodies = work.eq_bodies.borrow();
            let d = hs
                .iter()
                .filter_map(|h| match h {
                    Helper::DisplayNamed { ti } if !bodies.contains_key(ti) => Some(*ti),
                    _ => None,
                })
                .collect();
            let e = hs
                .iter()
                .filter_map(|h| match h {
                    Helper::NamedEq { ti } if !eq_bodies.contains_key(ti) => Some(*ti),
                    _ => None,
                })
                .collect();
            (d, e)
        };
        let todo_scan: Vec<crate::ETy> = {
            let hs = work.helpers.borrow();
            let scan_bodies = work.scan_bodies.borrow();
            hs.iter()
                .filter_map(|h| match h {
                    Helper::ScanDeep { key } if !scan_bodies.contains_key(key) => Some(*key),
                    _ => None,
                })
                .collect()
        };
        if todo.is_empty() && todo_eq.is_empty() && todo_scan.is_empty() {
            return Ok(all_calls);
        }
        for ti in todo {
            match build_one_display_helper(table, types, work, pool, ti) {
                Ok((f, calls)) => {
                    all_calls.extend(calls.iter().copied());
                    work.display_bodies
                        .borrow_mut()
                        .insert(ti, crate::work::DisplayBuild::Built(f));
                }
                Err(e) => {
                    work.display_bodies.borrow_mut().insert(ti, crate::work::DisplayBuild::Failed);
                    return Err(e);
                }
            }
        }
        for ti in todo_eq {
            match build_one_eq_helper(table, types, work, pool, ti) {
                Ok((f, calls)) => {
                    all_calls.extend(calls.iter().copied());
                    work.eq_bodies.borrow_mut().insert(ti, crate::work::DisplayBuild::Built(f));
                }
                Err(e) => {
                    work.eq_bodies.borrow_mut().insert(ti, crate::work::DisplayBuild::Failed);
                    return Err(e);
                }
            }
        }
        for key in todo_scan {
            match build_one_scan_helper(table, types, work, pool, key) {
                Ok((f, calls)) => {
                    all_calls.extend(calls.iter().copied());
                    work.scan_bodies.borrow_mut().insert(key, crate::work::DisplayBuild::Built(f));
                }
                Err(e) => {
                    work.scan_bodies.borrow_mut().insert(key, crate::work::DisplayBuild::Failed);
                    return Err(e);
                }
            }
        }
    }
}

/// One `(block, stride, off, needle) -> address|0` DEEP scan body: walk
/// the entries comparing the key slot by the type-directed `==`.
fn build_one_scan_helper(
    table: &FnTable,
    types: &TypeTable,
    work: &FnWork,
    pool: &mut Pool,
    key: crate::ETy,
) -> Result<(wasm_encoder::Function, std::collections::HashSet<usize>), EmitError> {
    use crate::emitter::{HOLD_F64_POOL, HOLD_I32_POOL, HOLD_I64_POOL};
    use wasm_encoder::{BlockType, ValType};
    // params 0-3, then FIVE i32s (4=p, 5=end, 6=tmp, 7=scr_i32, 8=spare),
    // one i64 (9 = scr_i64), one f64 (10 = scr_f64), pools from 11 — the
    // 4-param shift once left hold_i32_base on an f64 slot (validator).
    let local_decls = [
        (5, ValType::I32),
        (1, ValType::I64),
        (1, ValType::F64),
        (HOLD_I32_POOL, ValType::I32),
        (HOLD_I64_POOL, ValType::I64),
        (HOLD_F64_POOL, ValType::F64),
    ];
    let mut f = wasm_encoder::Function::new(local_decls);
    let mut calls = std::collections::HashSet::new();
    let empty_locals = std::collections::HashMap::new();
    let empty_globals = std::collections::HashMap::new();
    let empty_ranges = std::collections::HashMap::new();
    let empty_cells = std::collections::HashSet::new();
    {
        let mut em = Emitter {
            pool,
            locals: &empty_locals,
            rc_owned: std::collections::BTreeSet::new(),
            table,
            types,
            calls: &mut calls,
            fn_ret: None,
            cursor_local: 8,
            tmp_i32_local: 6,
            scr_i32_local: 7,
            scr_i64_local: 9,
            in_main: false,
            work,
            globals: &empty_globals,
            deferred_ranges: &empty_ranges,
            metered: false,
            cells: &empty_cells,
            region_repair: None,
            loop_ctl: None,
            in_tail: false,
            cur_module: None,
            hold_i32_base: 11,
            hold_i32_depth: 0,
            hold_i64_base: 11 + HOLD_I32_POOL,
            hold_i64_depth: 0,
            hold_f64_base: 11 + HOLD_I32_POOL + HOLD_I64_POOL,
            hold_f64_depth: 0,
            scr_f64_local: 10,
            f: &mut f,
        };
        // params: 0=block, 1=stride, 2=off, 3=needle; locals 4=p, 5=end
        let (blk, stride, off, needle, p_, end_) = (0u32, 1u32, 2u32, 3u32, 4u32, 5u32);
        let kt = em.types.el(key);
        {
            let mut i = em.f.instructions();
            i.local_get(blk)
                .i32_const(almide_layout::PAYLOAD as i32)
                .i32_add()
                .local_tee(p_);
            i.local_get(blk).i32_load(len_memarg()).i32_add().local_set(end_);
            i.block(BlockType::Empty).loop_(BlockType::Empty);
            i.local_get(p_).local_get(end_).i32_ge_u().br_if(1);
            i.local_get(p_)
                .local_get(off)
                .i32_add()
                .i32_load(wasm_encoder::MemArg { offset: 0, align: 2, memory_index: 0 });
            i.local_get(needle);
        }
        em.emit_val_eq(kt)?;
        {
            let mut i = em.f.instructions();
            i.if_(BlockType::Empty);
            i.local_get(p_).return_();
            i.end();
            i.local_get(p_).local_get(stride).i32_add().local_set(p_);
            i.br(0).end().end();
            i.i32_const(almide_layout::NULL_ADDR as i32);
        }
    }
    f.instructions().end();
    Ok((f, calls))
}

/// One `(a, b) -> i32` deep-equality body for a RECURSIVE Named type —
/// the same scaffold as the display helper; `path` starts with `ti`, so
/// the self-referencing fields call THIS helper's promised index.
fn build_one_eq_helper(
    table: &FnTable,
    types: &TypeTable,
    work: &FnWork,
    pool: &mut Pool,
    ti: u32,
) -> Result<(wasm_encoder::Function, std::collections::HashSet<usize>), EmitError> {
    use crate::emitter::{HOLD_F64_POOL, HOLD_I32_POOL, HOLD_I64_POOL};
    use wasm_encoder::ValType;
    let local_decls = [
        (3, ValType::I32),
        (1, ValType::I64),
        (1, ValType::F64),
        (HOLD_I32_POOL, ValType::I32),
        (HOLD_I64_POOL, ValType::I64),
        (HOLD_F64_POOL, ValType::F64),
    ];
    let mut f = wasm_encoder::Function::new(local_decls);
    let mut calls = std::collections::HashSet::new();
    let empty_locals = std::collections::HashMap::new();
    let empty_globals = std::collections::HashMap::new();
    let empty_ranges = std::collections::HashMap::new();
    let empty_cells = std::collections::HashSet::new();
    {
        let mut em = Emitter {
            pool,
            locals: &empty_locals,
            rc_owned: std::collections::BTreeSet::new(),
            table,
            types,
            calls: &mut calls,
            fn_ret: None,
            cursor_local: 2,
            tmp_i32_local: 3,
            scr_i32_local: 4,
            scr_i64_local: 5,
            in_main: false,
            work,
            globals: &empty_globals,
            deferred_ranges: &empty_ranges,
            metered: false,
            cells: &empty_cells,
            region_repair: None,
            loop_ctl: None,
            in_tail: false,
            cur_module: None,
            hold_i32_base: 7,
            hold_i32_depth: 0,
            hold_i64_base: 7 + HOLD_I32_POOL,
            hold_i64_depth: 0,
            hold_f64_base: 7 + HOLD_I32_POOL + HOLD_I64_POOL,
            hold_f64_depth: 0,
            scr_f64_local: 6,
            f: &mut f,
        };
        em.f.instructions().local_get(0).local_get(1);
        let mut path = vec![ti];
        em.emit_named_eq(ti, &mut path)?;
    }
    f.instructions().end();
    Ok((f, calls))
}

/// One `(block, cursor) -> cursor` display body, Emitter-built with the
/// standard scratch/hold local layout after the two raw params.
fn build_one_display_helper(
    table: &FnTable,
    types: &TypeTable,
    work: &FnWork,
    pool: &mut Pool,
    ti: u32,
) -> Result<(wasm_encoder::Function, std::collections::HashSet<usize>), EmitError> {
    use crate::emitter::{HOLD_F64_POOL, HOLD_I32_POOL, HOLD_I64_POOL};
    use wasm_encoder::ValType;
    let local_decls = [
        (3, ValType::I32),
        (1, ValType::I64),
        (1, ValType::F64),
        (HOLD_I32_POOL, ValType::I32),
        (HOLD_I64_POOL, ValType::I64),
        (HOLD_F64_POOL, ValType::F64),
    ];
    let mut f = wasm_encoder::Function::new(local_decls);
    let mut calls = std::collections::HashSet::new();
    let empty_locals = std::collections::HashMap::new();
    let empty_globals = std::collections::HashMap::new();
    let empty_ranges = std::collections::HashMap::new();
    let empty_cells = std::collections::HashSet::new();
    {
        let mut em = Emitter {
            pool,
            locals: &empty_locals,
            rc_owned: std::collections::BTreeSet::new(),
            table,
            types,
            calls: &mut calls,
            fn_ret: None,
            cursor_local: 2,
            tmp_i32_local: 3,
            scr_i32_local: 4,
            scr_i64_local: 5,
            in_main: false,
            work,
            globals: &empty_globals,
            deferred_ranges: &empty_ranges,
            metered: false,
            cells: &empty_cells,
            region_repair: None,
            loop_ctl: None,
            in_tail: false,
            cur_module: None,
            hold_i32_base: 7,
            hold_i32_depth: 0,
            hold_i64_base: 7 + HOLD_I32_POOL,
            hold_i64_depth: 0,
            hold_f64_base: 7 + HOLD_I32_POOL + HOLD_I64_POOL,
            hold_f64_depth: 0,
            scr_f64_local: 6,
            f: &mut f,
        };
        em.f.instructions().local_get(1).local_set(2);
        em.f.instructions().local_get(0);
        let mut path = vec![ti];
        em.emit_display_named(ti, &mut path)?;
        em.f.instructions().local_get(2);
    }
    f.instructions().end();
    Ok((f, calls))
}
