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
                self.emit_display_at(et, true, ir_arg(ir, 0), path)?;
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
                self.emit_display_at(ot, true, ir_arg(ir, 0), path)?;
                self.append_lit(")");
                self.f.instructions().else_();
                self.append_lit("err(");
                self.f.instructions().local_get(hr);
                self.load_ty_slot(et, almide_layout::SUM_FIELD);
                self.emit_display_at(et, true, ir_arg(ir, 1), path)?;
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
                self.emit_display_at(el, true, ir_arg(ir, 0), path)?;
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
                    self.emit_display_at(fty, true, ir_arg(ir, k), path)?;
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
                    self.f
                        .instructions()
                        .local_get(self.cursor_local)
                        .call(idx)
                        .local_set(self.cursor_local);
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
                self.emit_display_at(SliceTy::List(h), nested, ir, path)?;
                self.append_lit(")");
            }
            // `["k": v, …]` in insertion order; empty is the literal
            // `[:]` (the oracle's map repr).
            SliceTy::Map(kh, vh) => self.emit_display_map(kh, vh, ir, path)?,
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
                for (k, fi) in fields.iter().enumerate() {
                    if k > 0 {
                        self.append_lit(", ");
                    }
                    self.append_lit(&format!("{}: ", fi.name));
                    self.f.instructions().local_get(hb);
                    self.load_ty_slot(fi.ty, fi.offset);
                    let inst = self.types.instance_field_ir(ir, 0, &fi.name);
                    let fir = inst.as_ref().or(fi.ir.as_ref()).or_else(|| record_field_ir(ir, &fi.name));
                    self.emit_display_at(fi.ty, true, fir, path)?;
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
                            self.emit_display_at(f.ty, true, inst.as_ref().or(f.ir.as_ref()), path)?;
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
        let (todo, todo_named): (Vec<(u32, u32)>, Vec<(crate::work::NamedOp, u32)>) = {
            let hs = work.helpers.borrow();
            let bodies = work.display_bodies.borrow();
            let named_bodies = work.named_bodies.borrow();
            let d = hs
                .iter()
                .filter_map(|h| match h {
                    Helper::DisplayNamed { ti, irk } if !bodies.contains_key(&(*ti, *irk)) => Some((*ti, *irk)),
                    _ => None,
                })
                .collect();
            let n = hs
                .iter()
                .filter_map(|h| match h {
                    Helper::NamedOp { op, ti } if !named_bodies.contains_key(&(*op, *ti)) => {
                        Some((*op, *ti))
                    }
                    _ => None,
                })
                .collect();
            (d, n)
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
        let built_ty = build_display_ty_helpers(table, types, work, pool, &mut all_calls)?;
        if todo.is_empty() && todo_named.is_empty() && todo_scan.is_empty() && !built_ty {
            return Ok(all_calls);
        }
        for key in todo {
            match build_one_display_helper(table, types, work, pool, key) {
                Ok((f, calls)) => {
                    all_calls.extend(calls.iter().copied());
                    work.display_bodies
                        .borrow_mut()
                        .insert(key, crate::work::DisplayBuild::Built(f));
                }
                Err(e) => {
                    work.display_bodies.borrow_mut().insert(key, crate::work::DisplayBuild::Failed);
                    return Err(e);
                }
            }
        }
        for (op, ti) in todo_named {
            match build_one_named_helper(table, types, work, pool, op, ti) {
                Ok((f, calls)) => {
                    all_calls.extend(calls.iter().copied());
                    work.named_bodies
                        .borrow_mut()
                        .insert((op, ti), crate::work::DisplayBuild::Built(f));
                }
                Err(e) => {
                    work.named_bodies
                        .borrow_mut()
                        .insert((op, ti), crate::work::DisplayBuild::Failed);
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

/// The local layout an Emitter-built helper body runs on: `params` raw
/// params, then `i32s` i32 locals, then one i64, one f64, then the three
/// hold pools. The bodies below differ ONLY in the width of that param +
/// i32 block, so everything after it is DERIVED here rather than restated
/// per body — the one time a body computed those bases by hand, the
/// 4-param shift put `hold_i32_base` on an f64 slot and only the
/// validator noticed.
#[derive(Clone, Copy)]
struct Shell {
    params: u32,
    i32s: u32,
    cursor: u32,
    tmp_i32: u32,
    scr_i32: u32,
}

impl Shell {
    /// Two raw params, and the three i32 locals after them ARE the
    /// Emitter's cursor / tmp / scratch: `(a, b) -> i32` (the named-op
    /// bodies) and `(block, cursor) -> cursor` (display).
    const PAIR: Shell = Shell { params: 2, i32s: 3, cursor: 2, tmp_i32: 3, scr_i32: 4 };
    /// `(block, stride, off, needle) -> address|0`: four params, and two
    /// of the five i32 locals (4 = p, 5 = end) are the scan's own walk, so
    /// the Emitter's cursor takes the spare slot 8.
    const SCAN: Shell = Shell { params: 4, i32s: 5, cursor: 8, tmp_i32: 6, scr_i32: 7 };
}

/// Build one helper body on `shell`'s local layout: `body` emits into a
/// fresh Emitter, and the closed function comes back with the call set it
/// accumulated. ONE scaffold for every Emitter-built helper in this
/// module.
/// #2758: the frame's witness, armed while a sweep collects, is audited
/// against the closed body (witness_helper.rs).
fn build_helper_body(
    table: &FnTable,
    types: &TypeTable,
    work: &FnWork,
    pool: &mut Pool,
    (shell, hw): (Shell, Option<crate::witness::helper::HelperWitness>),
    body: impl FnOnce(&mut Emitter<'_>) -> Result<(), EmitError>,
) -> Result<(wasm_encoder::Function, std::collections::HashSet<usize>), EmitError> {
    use crate::emitter::{HOLD_F64_POOL, HOLD_I32_POOL, HOLD_I64_POOL};
    use wasm_encoder::ValType;
    let local_decls = [
        (shell.i32s, ValType::I32),
        (1, ValType::I64),
        (1, ValType::F64),
        (HOLD_I32_POOL, ValType::I32),
        (HOLD_I64_POOL, ValType::I64),
        (HOLD_F64_POOL, ValType::F64),
    ];
    // Immediately after the param + i32 block: the i64, then the f64,
    // then the three hold pools in declaration order.
    let scr_i64 = shell.params + shell.i32s;
    let scr_f64 = scr_i64 + 1;
    let holds = scr_f64 + 1;
    let mut f = wasm_encoder::Function::new(local_decls);
    let mut calls = std::collections::HashSet::new();
    let empty_locals = std::collections::HashMap::new();
    let empty_globals = std::collections::HashMap::new();
    let empty_ranges = std::collections::HashMap::new();
    let empty_cells = std::collections::HashSet::new();
    let (recorded, footprint);
    {
        let mut em = Emitter {
            var_space: 0,
            pool,
            locals: &empty_locals,
            rc_param_ceiling: 0,
            tail_release_allowed: false,
            rc_frame_params: Vec::new(),
            tail_consumed: Default::default(),
            loop_back_releasable: Default::default(),
            self_index: None,
            rc_owned: std::collections::BTreeSet::new(),
            owned_ty: std::collections::HashMap::new(),
            owned_call_marks: Default::default(),
            borrowed_temps: Vec::new(),
            exit_ledger: Vec::new(),
            arm_rests: Default::default(),
            borrow_base: 0, // helper bodies lower no arm argument
            table,
            types,
            calls: &mut calls,
            fn_ret: None,
            cursor_local: shell.cursor,
            tmp_i32_local: shell.tmp_i32,
            scr_i32_local: shell.scr_i32,
            scr_i64_local: scr_i64,
            in_main: false,
            build_depth: 0,
            work,
            globals: &empty_globals,
            deferred_ranges: &empty_ranges,
            metered: false,
            cells: &empty_cells,
            moves: Default::default(),
            region_repair: None,
            loop_ctl: None,
            hoisted_counts: HashMap::new(),
            cow_flags: HashMap::new(),
            cow_prejudged: HashSet::new(),
            bounds_facts: None,
            payload_ptrs: HashMap::new(),
            in_tail: false,
            try_see_through: false,
            branch_depth: 0,
            witness: hw.as_ref().map(|h| h.arm()),
            cur_module: None,
            hold_i32_base: holds,
            hold_i32_depth: 0,
            hold_i64_base: holds + HOLD_I32_POOL,
            hold_i64_depth: 0,
            hold_f64_base: holds + HOLD_I32_POOL + HOLD_I64_POOL,
            hold_f64_depth: 0,
            scr_f64_local: scr_f64,
            f: &mut f,
        };
        body(&mut em)?;
        footprint = hw.as_ref().map(|_| crate::witness::helper::footprint(&em));
        recorded = em.witness.take();
    }
    f.instructions().end();
    crate::witness::helper::finish(hw, recorded, footprint, &f);
    Ok((f, calls))
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
    use wasm_encoder::BlockType;
    // #2758: the entry block is lent (param 0); the needle's own RC calls,
    // like every other, are held to the byte audit.
    let hw = crate::witness::helper::HelperWitness::new(format!("<scan:{key:?}>"), "scan", &[0]);
    build_helper_body(table, types, work, pool, (Shell::SCAN, hw), |em| {
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
        Ok(())
    })
}

/// One `(a, b) -> i32` body for a RECURSIVE Named type: deep equality for
/// `NamedOp::Eq`, the total-order verdict for `NamedOp::Cmp`.
///
/// ONE builder, because the two bodies differ by one call (#2172). Both
/// exist for the same reason the display helper does: the emitter inlines
/// a type's shape, so a type that contains itself must become a CALL
/// somewhere. `path` starts with `ti`, so the self-referencing fields see
/// a cycle and call THIS helper's promised index instead of inlining.
fn build_one_named_helper(
    table: &FnTable,
    types: &TypeTable,
    work: &FnWork,
    pool: &mut Pool,
    op: crate::work::NamedOp,
    ti: u32,
) -> Result<(wasm_encoder::Function, std::collections::HashSet<usize>), EmitError> {
    // #2758: both operand blocks are lent (params 0 and 1).
    let hw = crate::witness::helper::HelperWitness::new(format!("<named-op:{op:?}:{ti}>"), "named-op", &[0, 1]);
    build_helper_body(table, types, work, pool, (Shell::PAIR, hw), |em| {
        em.f.instructions().local_get(0).local_get(1);
        match op {
            // `path` starts with `ti`, so a self-referencing field sees the
            // cycle and calls THIS helper's promised index.
            crate::work::NamedOp::Eq => em.emit_named_eq(ti, &mut vec![ti]),
            // The cmp walk needs no path: every `Named` is a call already.
            crate::work::NamedOp::Cmp => em.emit_named_cmp(ti),
            crate::work::NamedOp::EqTy => {
                em.emit_val_eq_level(em.types.el(crate::ETy::from_index(ti as usize)), &mut Vec::new())
            }
            crate::work::NamedOp::CmpTy => em.emit_val_cmp_level(em.types.el(crate::ETy::from_index(ti as usize))),
        }
    })
}

/// One `(block, cursor) -> cursor` display body.
fn build_one_display_helper(
    table: &FnTable,
    types: &TypeTable,
    work: &FnWork,
    pool: &mut Pool,
    (ti, irk): (u32, u32),
) -> Result<(wasm_encoder::Function, std::collections::HashSet<usize>), EmitError> {
    // #2758: the block is lent (param 0); the cursor (param 1) is an i32.
    let hw = crate::witness::helper::HelperWitness::new(format!("<display:{ti}>"), "display", &[0]);
    let ir = work.display_ir(irk);
    build_helper_body(table, types, work, pool, (Shell::PAIR, hw), |em| {
        em.f.instructions().local_get(1).local_set(2);
        em.f.instructions().local_get(0);
        let mut path = vec![ti];
        em.emit_display_named(ti, ir.as_ref(), &mut path)?;
        em.f.instructions().local_get(2);
        Ok(())
    })
}

/// The IR type of field `name` in a record IR type (`{ f: T }`, open or
/// closed) — the anonymous-record shape's leaf types (#3187).
fn record_field_ir<'t>(ir: Option<&'t Ty>, name: &str) -> Option<&'t Ty> {
    match ir? {
        Ty::Record { fields } | Ty::OpenRecord { fields } => {
            fields.iter().find(|(f, _)| f.as_str() == name).map(|(_, t)| t)
        }
        _ => None,
    }
}

/// The `i`-th component of a container's IR type — a List/Option/Set/Map/
/// Result argument or a tuple element — for the display walk (C-372). `None`
/// when the shape does not line up; the leaf then prints as a Float, never
/// guesses Float32.
pub(crate) fn ir_arg(ir: Option<&Ty>, i: usize) -> Option<&Ty> {
    match ir? {
        Ty::Applied(_, args) => args.get(i),
        Ty::Tuple(elems) => elems.get(i),
        _ => None,
    }
}
