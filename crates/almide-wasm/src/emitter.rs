//! Body lowering: the Emitter and its value/statement machinery.

use std::collections::{HashMap, HashSet};

use almide_ir::{BinOp, IrExpr, IrExprKind, UnOp, VarId};
use wasm_encoder::{BlockType, Function, ValType};

use crate::*;

// ── body lowering ───────────────────────────────────────────────────────

pub(crate) struct Emitter<'a> {
    pub(crate) pool: &'a mut Pool,
    pub(crate) locals: &'a HashMap<VarId, (u32, SliceTy)>,
    pub(crate) table: &'a FnTable,
    pub(crate) types: &'a TypeTable,
    pub(crate) calls: &'a mut HashSet<usize>,
    /// The containing function's return slice type — `!` PROPAGATES (not
    /// aborts) in pure fns returning Option/Result, so those are refused.
    pub(crate) fn_ret: Option<SliceTy>,
    pub(crate) cursor_local: u32,
    pub(crate) tmp_i32_local: u32,
    /// Match/unwrap subject scratch. Shared across nesting levels — safe
    /// because a subject is only read during its own tests, which finish
    /// before any nested match/unwrap in a SELECTED arm's body runs.
    pub(crate) scr_i32_local: u32,
    pub(crate) scr_i64_local: u32,
    /// Lowering `main`: a propagated `!` error ABORTS (the interp's
    /// main-level Flow::Return(Err) contract — "Error: {msg}" + exit 1).
    pub(crate) in_main: bool,
    /// Function-value work: funcref-table entries, call_indirect types,
    /// lifted lambdas (W-1/W-2).
    pub(crate) work: &'a FnWork,
    /// Top-let globals (VarId → wasm global index + type): the fallback
    /// when a Var/Assign misses the locals map.
    pub(crate) globals: &'a HashMap<VarId, (u32, SliceTy)>,
    /// Deferred head-only range binds (C-238): VarId → (start local,
    /// end local, inclusive). No block exists for these vars.
    pub(crate) deferred_ranges: &'a HashMap<VarId, (u32, u32, bool)>,
    /// One-shot tail-position marker: set by `lower_tail`, TAKEN at
    /// `lower`'s entry so it never leaks into operand lowering. A direct
    /// call in tail position with a matching return type emits
    /// `return_call` — constant stack for deep (incl. mutual) recursion.
    pub(crate) in_tail: bool,
    /// The module this function belongs to (None = entry program) —
    /// intra-module Named calls resolve module-qualified FIRST.
    pub(crate) cur_module: Option<&'a str>,
    pub(crate) hold_i32_base: u32,
    pub(crate) hold_i32_depth: u32,
    pub(crate) hold_i64_base: u32,
    pub(crate) hold_i64_depth: u32,
    pub(crate) hold_f64_base: u32,
    pub(crate) hold_f64_depth: u32,
    pub(crate) scr_f64_local: u32,
    pub(crate) f: &'a mut Function,
}

/// Hold-pool sizes: nesting deeper than this is refused, never corrupted.
pub(crate) const HOLD_I32_POOL: u32 = 12;
pub(crate) const HOLD_I64_POOL: u32 = 4;
pub(crate) const HOLD_F64_POOL: u32 = 4;

impl Emitter<'_> {
    pub(crate) fn hold_i32(&mut self) -> Result<u32, EmitError> {
        if self.hold_i32_depth >= HOLD_I32_POOL {
            return unsup("hold-depth-i32");
        }
        let idx = self.hold_i32_base + self.hold_i32_depth;
        self.hold_i32_depth += 1;
        Ok(idx)
    }

    pub(crate) fn release_i32(&mut self) {
        self.hold_i32_depth -= 1;
    }

    pub(crate) fn hold_i64(&mut self) -> Result<u32, EmitError> {
        if self.hold_i64_depth >= HOLD_I64_POOL {
            return unsup("hold-depth-i64");
        }
        let idx = self.hold_i64_base + self.hold_i64_depth;
        self.hold_i64_depth += 1;
        Ok(idx)
    }

    pub(crate) fn release_i64(&mut self) {
        self.hold_i64_depth -= 1;
    }

    pub(crate) fn hold_f64(&mut self) -> Result<u32, EmitError> {
        if self.hold_f64_depth >= HOLD_F64_POOL {
            return unsup("hold-depth-f64");
        }
        let idx = self.hold_f64_base + self.hold_f64_depth;
        self.hold_f64_depth += 1;
        Ok(idx)
    }

    pub(crate) fn release_f64(&mut self) {
        self.hold_f64_depth -= 1;
    }

    /// The lambda body's captured OUTER locals (VarIds are unique within
    /// a function context, so any Var resolving through the enclosing
    /// locals map that is not a lambda param is a capture).
    pub(crate) fn captured_vars(
        &self,
        params: &std::collections::HashSet<VarId>,
        body: &IrExpr,
    ) -> Vec<(VarId, SliceTy)> {
        struct Scan<'x> {
            locals: &'x HashMap<VarId, (u32, SliceTy)>,
            params: &'x std::collections::HashSet<VarId>,
            out: Vec<(VarId, SliceTy)>,
        }
        impl almide_ir::visit::IrVisitor for Scan<'_> {
            fn visit_expr(&mut self, e: &IrExpr) {
                if let IrExprKind::Var { id } = &e.kind
                    && !self.params.contains(id)
                    && let Some(&(_, ty)) = self.locals.get(id)
                    && !self.out.iter().any(|(v, _)| v == id)
                {
                    self.out.push((*id, ty));
                }
                almide_ir::visit::walk_expr(self, e);
            }
        }
        let mut sc = Scan { locals: self.locals, params, out: Vec::new() };
        almide_ir::visit::IrVisitor::visit_expr(&mut sc, body);
        sc.out
    }

    /// `[raw value]` -> `[ok(..) Result block]` (the effect-fn return wrap).
    pub(crate) fn wrap_ok(&mut self, raw: SliceTy, ret: SliceTy) -> Result<(), EmitError> {
        let SliceTy::Result(o, _) = ret else {
            return unsup("effect-wrap-non-result");
        };
        let side = self.types.el(o);
        if side != raw {
            return unsup("effect-wrap-ty-mismatch");
        }
        let hv = self.hold_val(raw)?;
        let hb = self.hold_i32()?;
        self.f.instructions().local_set(hv);
        self.f
            .instructions()
            .i32_const(16)
            .call(F_ALLOC)
            .local_tee(hb)
            .i32_const(0)
            .i32_store(slot_memarg(almide_layout::SUM_TAG));
        self.f.instructions().local_get(hb).local_get(hv);
        self.store_ty_slot(raw, almide_layout::SUM_FIELD);
        self.f.instructions().local_get(hb);
        self.release_i32();
        self.release_val(raw);
        Ok(())
    }

    /// The main-level / pure-fn abort frame for a failed `!`: the exact
    /// native contract — `Error: {msg}` on stderr, exit 1. The message
    /// block address is on the stack.
    pub(crate) fn emit_error_frame_abort(&mut self) {
        let prefix = self.pool.intern("Error: ");
        let hm = self.tmp_i32_local;
        self.f.instructions().local_set(hm);
        self.f.instructions().i32_const(prefix as i32).local_get(hm).call(F_CONCAT);
        self.f
            .instructions()
            .call(F_EPRINTLN_BLOCK)
            .i32_const(1)
            .call(F_EXIT_IMPORT)
            .unreachable();
    }

    /// A hold from the pool matching the slice type's wasm value type.
    pub(crate) fn hold_val(&mut self, ty: SliceTy) -> Result<u32, EmitError> {
        match ty.val_type() {
            ValType::I64 => self.hold_i64(),
            ValType::F64 => self.hold_f64(),
            _ => self.hold_i32(),
        }
    }

    pub(crate) fn release_val(&mut self, ty: SliceTy) {
        match ty.val_type() {
            ValType::I64 => self.release_i64(),
            ValType::F64 => self.release_f64(),
            _ => self.release_i32(),
        }
    }




    /// Lower `e` in TAIL position (the value becomes the function's
    /// return): direct calls become `return_call`.
    pub(crate) fn lower_tail(
        &mut self,
        e: &IrExpr,
        want: Option<SliceTy>,
    ) -> Result<SliceTy, EmitError> {
        self.in_tail = true;
        self.lower(e, want)
    }

    pub(crate) fn lower(&mut self, e: &IrExpr, want: Option<SliceTy>) -> Result<SliceTy, EmitError> {
        let tail = std::mem::take(&mut self.in_tail);
        let got = match &e.kind {
            IrExprKind::LitInt { value } => {
                self.f.instructions().i64_const(*value);
                INT
            }
            // `["k": v, …]` — a map literal. Desugars to the SAME
            // insertion-ordered upsert `map.from_list` runs (last write
            // wins on duplicate keys, the interp's insert-per-entry
            // semantics) by synthesizing the pairs list.
            IrExprKind::MapLiteral { entries } => {
                let ty = want.map_or_else(|| self.infer(e), Ok)?;
                let SliceTy::Map(kh, vh) = ty else {
                    return unsup(&format!("ty-mismatch:map-literal-vs-{ty:?}"));
                };
                let (kt, vt) = (self.types.el(kh), self.types.el(vh));
                let _ = (kt, vt);
                let (k_ty, v_ty) = match &e.ty {
                    Ty::Applied(TypeConstructorId::Map, a)
                        if a.len() == 2 =>
                    {
                        (a[0].clone(), a[1].clone())
                    }
                    other => return unsup(&format!("map-literal-ty:{}", ty_name(other))),
                };
                let pair_ty = Ty::Tuple(vec![k_ty, v_ty]);
                let list_ty = Ty::Applied(
                    TypeConstructorId::List,
                    vec![pair_ty.clone()],
                );
                let pairs = IrExpr {
                    kind: IrExprKind::List {
                        elements: entries
                            .iter()
                            .map(|(k, v)| IrExpr {
                                kind: IrExprKind::Tuple {
                                    elements: vec![k.clone(), v.clone()],
                                },
                                ty: pair_ty.clone(),
                                span: e.span,
                                def_id: None,
                            })
                            .collect(),
                    },
                    ty: list_ty,
                    span: e.span,
                    def_id: None,
                };
                match self.lower_map_call("from_list", &[pairs], Some(ty))? {
                    Some(t) => t,
                    None => return unsup("map-literal-unit"),
                }
            }
            IrExprKind::EmptyMap => {
                let ty = want.map_or_else(|| self.infer(e), Ok)?;
                let SliceTy::Map(..) = ty else {
                    return unsup(&format!("ty-mismatch:empty-map-vs-{ty:?}"));
                };
                // An empty map block: zero entries, same shape map.new emits.
                self.f.instructions().i32_const(0).call(F_ALLOC);
                ty
            }
            // Unit as a VALUE (an effect ok payload, a Unit bind).
            IrExprKind::Unit => {
                self.f.instructions().i32_const(0);
                SliceTy::Unit
            }
            IrExprKind::LitFloat { value } => {
                self.f.instructions().f64_const((*value).into());
                FLOAT
            }
            IrExprKind::LitBool { value } => {
                self.f.instructions().i32_const(i32::from(*value));
                BOOL
            }
            IrExprKind::LitStr { value } => {
                let base = self.pool.intern(value);
                self.f.instructions().i32_const(base as i32);
                STR
            }
            IrExprKind::Var { id } => {
                if let Some(&(idx, ty)) = self.locals.get(id) {
                    self.f.instructions().local_get(idx);
                    ty
                } else if let Some(&(gidx, ty)) = self.globals.get(id) {
                    self.f.instructions().global_get(gidx);
                    ty
                } else {
                    return unsup("var:unmapped");
                }
            }
            IrExprKind::FnRef { name } => self.lower_fn_ref(e, name, want)?,
            IrExprKind::Lambda { params, body, .. } => {
                self.lower_lambda_value(e, params, body, want)?
            }
            IrExprKind::RuntimeCall { symbol, args } => {
                // The slice SYNTAX `xs[a..b]` desugars to this runtime
                // symbol — one impl with `list.slice` (as in native rt).
                if symbol.as_str() == "almide_rt_list_slice" && args.len() == 3 {
                    match self.lower_list_call("slice", args, None)? {
                        Some(t) => t,
                        None => return unsup("rt:list-slice-unit"),
                    }
                } else {
                    return unsup(&format!("rt:{}", symbol.as_str()));
                }
            }
            IrExprKind::Call { target, args, .. } => {
                let hint = slice_ty_of(&e.ty, self.types);
                match self.lower_call_at(target, args, tail, hint)? {
                    Some(ty) => ty,
                    // A void call in value position under a Unit want:
                    // materialize the unit value.
                    None if want == Some(SliceTy::Unit) => {
                        self.f.instructions().i32_const(0);
                        SliceTy::Unit
                    }
                    None => return unsup("call-unit-in-value"),
                }
            }
            IrExprKind::UnOp { op, operand } => match op {
                UnOp::NegInt => {
                    self.f.instructions().i64_const(0);
                    self.lower(operand, Some(INT))?;
                    self.f.instructions().i64_sub();
                    INT
                }
                UnOp::Not => {
                    self.lower(operand, Some(BOOL))?;
                    self.f.instructions().i32_eqz();
                    BOOL
                }
                UnOp::NegFloat => {
                    self.lower(operand, Some(FLOAT))?;
                    self.f.instructions().f64_neg();
                    FLOAT
                }
            },
            IrExprKind::BinOp { op, left, right } => self.lower_binop(*op, left, right)?,
            IrExprKind::Block { stmts, expr } => {
                for s in stmts {
                    self.lower_stmt(s)?;
                }
                let Some(t) = expr else { return unsup("expr:Block-no-tail") };
                self.in_tail = tail;
                self.lower(t, want)?
            }
            IrExprKind::If { .. } | IrExprKind::Match { .. } => {
                self.lower_control(e, want, tail)?
            }
            // Value-position interpolation: build in the line buffer, then
            // capture as a real block; the cursor global restores so the
            // buffer region is reusable (and nested builds stay sound).
            IrExprKind::StringInterp { parts } => {
                let start = self.lower_interp_build(parts)?;
                self.f
                    .instructions()
                    .local_get(start)
                    .local_get(self.cursor_local)
                    .call(F_BUF_TO_BLOCK)
                    .local_get(start)
                    .global_set(G_LINE_CURSOR);
                // The build clobbered the shared cursor local — restore it
                // to the captured region's start, or an ENCLOSING display
                // append lands past the inner bytes and doubles the text
                // (fuzz seeds 1/7/9/… the day the display engine landed).
                self.f
                    .instructions()
                    .local_get(start)
                    .local_set(self.cursor_local);
                self.release_i32();
                STR
            }
            _ => self.lower_data(e, want)?,
        };
        if let Some(w) = want
            && got != w
        {
            return unsup(&format!("ty-mismatch:{got:?}-vs-{w:?}"));
        }
        Ok(got)
    }

    /// Data-shaped values: sum constructors, unwraps, records, lists.
    /// (Split from `lower` for complexity budget; the `want` check happens
    /// in `lower`'s shared tail.)
    pub(crate) fn lower_data(
        &mut self,
        e: &IrExpr,
        want: Option<SliceTy>,
    ) -> Result<SliceTy, EmitError> {
        let got = match &e.kind {
            _ if is_sum_shape(&e.kind) => self.lower_sum(e, want)?,
            _ if matches!(
                &e.kind,
                IrExprKind::Record { .. }
                    | IrExprKind::SpreadRecord { .. }
                    | IrExprKind::Member { .. }
                    | IrExprKind::Tuple { .. }
                    | IrExprKind::TupleIndex { .. }
            ) => self.lower_record(e, want)?,
            // Range in VALUE position materializes the real List[Int]
            // (the front end types it Applied(List, [Int])). Span follows
            // native list_range: end.saturating_sub(start).max(0) — the
            // saturation is real i64-overflow detection, so (i64::MIN, 3)
            // is the C-197 die, not an empty list. Past the wasm leg's own
            // structural bound the same "Error: out of memory" + exit 1
            // fires BEFORE the allocator (success between the two legs'
            // bounds is the contracted divergence, runtime/rs list.rs).
            IrExprKind::Range { start, end, inclusive } => {
                let hty = SliceTy::List(self.types.intern(INT));
                if let Some(w) = want
                    && w != hty
                {
                    return unsup(&format!("ty-mismatch:range-vs-{w:?}"));
                }
                self.lower(start, Some(INT))?;
                let hs = self.hold_i64()?;
                self.f.instructions().local_set(hs);
                self.lower(end, Some(INT))?;
                let he = self.hold_i64()?;
                let hd = self.hold_i64()?;
                let hb = self.hold_i32()?;
                let hc = self.hold_i32()?;
                let msg = self.pool.intern("out of memory");
                // Block bytes must fit a positive i32: span*8 + header.
                const RANGE_CAP: i64 = ((i32::MAX - 16) / 8) as i64;
                {
                    let mut i = self.f.instructions();
                    i.local_set(he);
                    if *inclusive {
                        i.local_get(he).i64_const(1).i64_add().local_set(he);
                    }
                    // d = he - hs (wrapping); true span positive iff
                    // he > hs; positive overflow iff sign(he)!=sign(hs)
                    // and sign(d)!=sign(he) — then any past-cap value
                    // stands in for the saturated span.
                    i.local_get(he).local_get(hs).i64_sub().local_set(hd);
                    i.i64_const(RANGE_CAP + 1);
                    i.local_get(hd);
                    i.local_get(he).local_get(hs).i64_xor();
                    i.local_get(he).local_get(hd).i64_xor();
                    i.i64_and().i64_const(0).i64_lt_s();
                    i.select();
                    i.i64_const(0);
                    i.local_get(he).local_get(hs).i64_gt_s();
                    i.select();
                    i.local_set(hd);
                    i.local_get(hd).i64_const(RANGE_CAP).i64_gt_s();
                    i.if_(BlockType::Empty);
                    i.i32_const(msg as i32);
                }
                self.emit_error_frame_abort();
                {
                    let mut i = self.f.instructions();
                    i.end();
                    i.local_get(hd)
                        .i64_const(8)
                        .i64_mul()
                        .i32_wrap_i64()
                        .call(F_ALLOC)
                        .local_set(hb);
                    // fill ascending: payload[k] = start + k
                    i.local_get(hb)
                        .i32_const(almide_layout::PAYLOAD as i32)
                        .i32_add()
                        .local_set(hc);
                    i.block(BlockType::Empty).loop_(BlockType::Empty);
                    i.local_get(hd).i64_const(0).i64_le_s().br_if(1);
                    i.local_get(hc).local_get(hs).i64_store(MemArg {
                        offset: 0,
                        align: 2,
                        memory_index: 0,
                    });
                    i.local_get(hs).i64_const(1).i64_add().local_set(hs);
                    i.local_get(hc).i32_const(8).i32_add().local_set(hc);
                    i.local_get(hd).i64_const(1).i64_sub().local_set(hd);
                    i.br(0);
                    i.end();
                    i.end();
                    i.local_get(hb);
                }
                self.release_i32();
                self.release_i32();
                self.release_i64();
                self.release_i64();
                self.release_i64();
                hty
            }
            // List literal: alloc, then store each element through a hold
            // local (kept live across element lowering — the pool makes
            // nesting safe by construction).
            IrExprKind::List { elements } => {
                let (hty, elem) = match want.map_or_else(|| self.infer(e), Ok)? {
                    SliceTy::List(h) => (SliceTy::List(h), self.types.el(h)),
                    other => return unsup(&format!("ty-mismatch:list-vs-{other:?}")),
                };
                let stride = elem.slot_size();
                let hold = self.hold_i32()?;
                self.f
                    .instructions()
                    .i32_const((elements.len() as u32 * stride) as i32)
                    .call(F_ALLOC)
                    .local_set(hold);
                for (i, el) in elements.iter().enumerate() {
                    self.f.instructions().local_get(hold);
                    self.lower(el, Some(elem))?;
                    self.store_ty_slot(elem, i as u32 * stride);
                }
                self.f.instructions().local_get(hold);
                self.release_i32();
                hty
            }
            // xs[i]: bounds-checked element load. Out of bounds aborts on
            // the oracle — the trap lands in the abort-parity bucket.
            IrExprKind::IndexAccess { object, index } => {
                let elem = match self.lower(object, None)? {
                    SliceTy::List(h) => self.types.el(h),
                    other => return unsup(&format!("index-of:{other:?}")),
                };
                let stride = elem.slot_size();
                let hold = self.hold_i32()?;
                self.f.instructions().local_set(hold);
                self.lower(index, Some(INT))?;
                let idx = self.hold_i64()?;
                self.f.instructions().local_tee(idx);
                // idx < 0 || idx >= count → trap
                let mut i = self.f.instructions();
                i.i64_const(0).i64_lt_s();
                i.local_get(idx);
                i.local_get(hold).i32_load(len_memarg()).i32_const(stride as i32).i32_div_u();
                i.i64_extend_i32_u().i64_ge_s();
                i.i32_or().if_(BlockType::Empty).unreachable().end();
                // element address: hold + idx*stride, slot at offset PAYLOAD
                i.local_get(hold);
                i.local_get(idx).i32_wrap_i64().i32_const(stride as i32).i32_mul().i32_add();
                self.load_ty_slot(elem, 0);
                self.release_i64();
                self.release_i32();
                elem
            }
            other => return unsup(&format!("expr:{}", expr_kind_name(other))),
        };
        Ok(got)
    }

    pub(crate) fn store_ty_slot(&mut self, t: SliceTy, payload_relative: u32) {
        let m = slot_memarg(payload_relative);
        match t.val_type() {
            ValType::I64 => self.f.instructions().i64_store(m),
            ValType::F64 => self.f.instructions().f64_store(m),
            _ => self.f.instructions().i32_store(m),
        };
    }

    pub(crate) fn load_ty_slot(&mut self, t: SliceTy, payload_relative: u32) {
        let m = slot_memarg(payload_relative);
        match t.val_type() {
            ValType::I64 => self.f.instructions().i64_load(m),
            ValType::F64 => self.f.instructions().f64_load(m),
            _ => self.f.instructions().i32_load(m),
        };
    }

    // ── inference (non-emitting) ────────────────────────────────────────

    /// Non-emitting slice-type resolution — used where wasm needs a block
    /// type before an arm is lowered. Reads the CHECKER's annotation
    /// (`IrExpr.ty`), the authoritative type on every node; an unmappable
    /// annotation is an honest reason, and `lower`'s own result is still
    /// verified against the hint afterwards (defense in depth).
    pub(crate) fn infer(&self, e: &IrExpr) -> Result<SliceTy, EmitError> {
        match slice_ty_of(&e.ty, self.types) {
            Some(t) => Ok(t),
            None => unsup(&format!("infer-ty:{}", ty_name(&e.ty))),
        }
    }

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
    /// Value-position control flow (`if`, `match`) — the block type comes
    /// from the hint or the checker's annotation. Split from `lower` for
    /// complexity budget; the `want` check happens in `lower`'s tail.
    pub(crate) fn lower_control(
        &mut self,
        e: &IrExpr,
        want: Option<SliceTy>,
        tail: bool,
    ) -> Result<SliceTy, EmitError> {
        let got = match &e.kind {
            // Value-position `if`: the arm type comes from the hint or is
            // inferred WITHOUT emitting (wasm wants the block type up
            // front), then both arms are lowered against it.
            IrExprKind::If { cond, then, else_ } => {
                self.lower(cond, Some(BOOL))?;
                let ty = match want {
                    Some(w) => w,
                    None => self.infer(e)?,
                };
                self.f.instructions().if_(BlockType::Result(ty.val_type()));
                self.in_tail = tail;
                self.lower(then, Some(ty))?;
                self.f.instructions().else_();
                self.in_tail = tail;
                self.lower(else_, Some(ty))?;
                self.f.instructions().end();
                ty
            }
            IrExprKind::Match { subject, arms } => {
                let ty = match want {
                    Some(w) => w,
                    None => self.infer(e)?,
                };
                self.lower_match_at(subject, arms, Some(ty), tail)?;
                ty
            }
            other => return unsup(&format!("expr:{}", expr_kind_name(other))),
        };
        Ok(got)
    }
}

impl Emitter<'_> {
}

/// Kinds `lower_sum` owns (constructors and unwraps).
fn is_sum_shape(k: &IrExprKind) -> bool {
    matches!(
        k,
        IrExprKind::OptionNone
            | IrExprKind::OptionSome { .. }
            | IrExprKind::ResultOk { .. }
            | IrExprKind::ResultErr { .. }
            | IrExprKind::Unwrap { .. }
            | IrExprKind::UnwrapOr { .. }
    )
}

impl Emitter<'_> {
    /// A named fn as a VALUE (split from lower for the complexity budget).
    fn lower_fn_ref(
        &mut self,
        e: &IrExpr,
        name: &almide_base::intern::Sym,
        want: Option<SliceTy>,
    ) -> Result<SliceTy, EmitError> {

                let ty = want.map_or_else(|| self.infer(e), Ok)?;
                let SliceTy::Fn(sig) = ty else {
                    return unsup(&format!("fnref-vs-{ty:?}"));
                };
                let def = self.types.fn_sig_def(sig);
                let resolved = self
                    .cur_module
                    .and_then(|m| self.table.by_name.get(&format!("{m}.{}", name.as_str())))
                    .or_else(|| self.table.by_name.get(name.as_str()))
                    .copied();
                let Some(idx) = resolved else {
                    return unsup(&format!("fnref:{name}"));
                };
                let info = &self.table.infos[idx];
                if info.refuse.is_some() {
                    return unsup("fnref-refused-target");
                }
                if info.params != def.params {
                    return unsup("fnref-param-mismatch");
                }
                let entry = if info.ret == def.ret {
                    TableEntry::Fn(idx)
                } else {
                    match (info.ret, def.ret) {
                        (Some(raw), Some(SliceTy::Result(o, er)))
                            if def.effect
                                && self.types.el(o) == raw
                                && self.types.el(er) == STR =>
                        {
                            TableEntry::Adapter { target: idx, raw }
                        }
                        _ => return unsup("fnref-ret-mismatch"),
                    }
                };
                let slot = self.work.slot(entry);
                // Fn value = closure BLOCK [slot@0]; capture-free blocks
                // are pool statics (dedup by content, zero runtime alloc).
                let block = self.pool.intern_block(&(slot).to_le_bytes());
                self.f.instructions().i32_const(block as i32);
                Ok(ty)
    }

    /// A lambda as a VALUE (split from lower for the complexity budget).
    fn lower_lambda_value(
        &mut self,
        e: &IrExpr,
        params: &[(VarId, almide_types::types::Ty)],
        body: &IrExpr,
        want: Option<SliceTy>,
    ) -> Result<SliceTy, EmitError> {

                let ty = want.map_or_else(|| self.infer(e), Ok)?;
                let SliceTy::Fn(sig) = ty else {
                    return unsup(&format!("lambda-vs-{ty:?}"));
                };
                let def = self.types.fn_sig_def(sig);
                if params.len() != def.params.len() {
                    return unsup("lambda-arity");
                }
                let param_ids: std::collections::HashSet<VarId> =
                    params.iter().map(|(v, _)| *v).collect();
                let mut captured = self.captured_vars(&param_ids, body);
                captured.sort_by_key(|v| v.0);
                let ps: Vec<(VarId, SliceTy)> = params
                    .iter()
                    .map(|(v, _)| *v)
                    .zip(def.params.iter().copied())
                    .collect();
                let effect_raw = if def.effect {
                    match def.ret {
                        Some(SliceTy::Result(o, _)) => Some(self.types.el(o)),
                        _ => None,
                    }
                } else {
                    None
                };
                // Closure block layout: [slot:i32][captures packed...].
                let widths: Vec<u32> = std::iter::once(4)
                    .chain(captured.iter().map(|(_, t)| t.slot_size()))
                    .collect();
                let (offsets, size) = almide_layout::pack_fields(&widths);
                let captures: Vec<(VarId, SliceTy, u32)> = captured
                    .iter()
                    .zip(offsets.iter().skip(1))
                    .map(|(&(v, t), &off)| (v, t, off))
                    .collect();
                let j = self.work.register_lambda(LiftedLambda {
                    params: ps,
                    ret: def.ret,
                    effect_raw,
                    body: body.clone(),
                    captures: captures.clone(),
                });
                let slot = self.work.slot(TableEntry::Lambda(j));
                if captures.is_empty() {
                    let block = self.pool.intern_block(&(slot).to_le_bytes());
                    self.f.instructions().i32_const(block as i32);
                } else {
                    let hb = self.hold_i32()?;
                    self.f
                        .instructions()
                        .i32_const(size as i32)
                        .call(F_ALLOC)
                        .local_tee(hb)
                        .i32_const(slot as i32)
                        .i32_store(slot_memarg(0));
                    for (v, t, off) in &captures {
                        let (idx, _) = self.locals[v];
                        self.f.instructions().local_get(hb).local_get(idx);
                        self.store_ty_slot(*t, *off);
                    }
                    self.f.instructions().local_get(hb);
                    self.release_i32();
                }
        Ok(ty)
    }
}
