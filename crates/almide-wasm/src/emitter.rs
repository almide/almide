//! Body lowering: the Emitter and its value/statement machinery.

use std::collections::{HashMap, HashSet};

use almide_ir::{IrExpr, IrExprKind, UnOp, VarId};
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
    /// USER code: loop heads / entries / dyn ops charge the deterministic
    /// meter (ALS-DT2). Pool bodies and synthesized helpers never charge.
    pub(crate) metered: bool,
    /// C-319 shared-cell vars: the local holds a one-slot heap cell's
    /// ADDRESS; reads load through it, writes store through it.
    pub(crate) cells: &'a std::collections::HashSet<VarId>,
    /// C-320: Some((saved_local, depth_entry_local)) when this fn is a
    /// region ARM — a cut here runs the exit bookkeeping its early
    /// return would otherwise skip (guarded by depth > depth-at-entry,
    /// so a post-exit cut never double-books).
    pub(crate) region_repair: Option<(u32, u32)>,
    /// Some((extra, break_delta)) inside a loop body: `extra` counts the
    /// statement-walker labels opened since the loop's continue target,
    /// so `continue` = br(extra) and `break` = br(extra + break_delta).
    /// Suspended (None) under structures whose labels the walker does
    /// not track (match arms) — a Continue there walls honestly.
    pub(crate) loop_ctl: Option<(u32, u32)>,
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
/// (16: the string scanners — lines/chars/pad — hold up to 7 at once and
/// sit inside expression contexts already holding several.)
pub(crate) const HOLD_I32_POOL: u32 = 16;
pub(crate) const HOLD_I64_POOL: u32 = 8;
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

    /// Store the value on the stack into `var`'s storage: a plain local
    /// set, or a store through the C-319 cell address.
    pub(crate) fn emit_store_var(
        &mut self,
        id: VarId,
        idx: u32,
        ty: SliceTy,
    ) -> Result<(), EmitError> {
        if self.cells.contains(&id) {
            let hv = self.hold_val(ty)?;
            self.f.instructions().local_set(hv);
            self.f.instructions().local_get(idx).local_get(hv);
            self.store_ty_slot(ty, 0);
            self.release_val(ty);
        } else {
            self.f.instructions().local_set(idx);
        }
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




    /// The WASM-level Result type of a Named callee (the IR `ty` on the
    /// call is the RAW ok type — the effect ABI is a backend layer), or
    /// None when the operand is not a resolvable Named call. Feeds the
    /// Try/Unwrap tail see-through guard.
    fn effect_tail_callee_ret(&self, e: &IrExpr) -> Option<SliceTy> {
        let IrExprKind::Call {
            target: almide_ir::CallTarget::Named { name }, ..
        } = &e.kind
        else {
            return None;
        };
        let name = name.as_str();
        // Mirror lower_call_at's dispatch order: a ctor wins the name.
        if self.types.ctors.contains_key(name) {
            return None;
        }
        let i = self
            .cur_module
            .and_then(|m| self.table.by_name.get(&format!("{m}.{name}")))
            .or_else(|| self.table.by_name.get(name))
            .copied()?;
        let info = &self.table.infos[i];
        if info.refuse.is_some() {
            return None;
        }
        info.ret
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
                // A Float32-typed literal narrows AT BIRTH (C-182) —
                // the widened carrier holds the f32-representable value.
                let v = if matches!(e.ty, almide_types::types::Ty::Float32) {
                    *value as f32 as f64
                } else {
                    *value
                };
                self.f.instructions().f64_const(v.into());
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
                    if self.cells.contains(id) {
                        self.load_ty_slot(ty, 0);
                    }
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
            IrExprKind::Fan { exprs } => self.lower_fan_block(exprs)?,
            IrExprKind::RuntimeCall { symbol, args } => {
                // The slice SYNTAX `xs[a..b]` desugars to this runtime
                // symbol — one impl with `list.slice` (as in native rt).
                if symbol.as_str() == "almide_rt_list_slice" && args.len() == 3 {
                    match self.lower_list_call("slice", args, None)? {
                        Some(t) => t,
                        None => return unsup("rt:list-slice-unit"),
                    }
                } else if let Some(t) = self.lower_budget_prim(symbol.as_str(), args)? {
                    t
                } else {
                    return unsup(&format!("rt:{}", symbol.as_str()));
                }
            }
            // TCO sees THROUGH `Try{Call self}` / `Unwrap{Call self}`
            // (#557 / C-069): when the callee's Result type IS this
            // effect fn's Result, propagate-err-or-rewrap-ok is the
            // identity, so the call happens in TRUE tail position
            // (return_call, O(1) stack). Restricted to Named calls —
            // the one arm guaranteed to honor `tail` when ret == fn_ret,
            // so the un-taken wrap path after us is provably dead.
            IrExprKind::Try { expr } | IrExprKind::Unwrap { expr }
                if tail
                    && matches!(self.fn_ret, Some(SliceTy::Result(..)))
                    && self.effect_tail_callee_ret(expr) == self.fn_ret =>
            {
                self.in_tail = true;
                self.lower(expr, self.fn_ret)?;
                // The Named arm return_calls (ret == fn_ret by the guard),
                // so control NEVER returns here — the stack is polymorphic
                // and the dead wrap path after us only needs to type-check.
                // Report the surrounding expectation, not the Result.
                match want.or_else(|| slice_ty_of(&e.ty, self.types)) {
                    Some(t) => t,
                    None => return unsup("try-tail-untyped"),
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
            IrExprKind::Range { start, end, inclusive } => {
                self.lower_range_value(start, end, *inclusive, want)?
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
            // m[k]: exactly map.get — a miss is `none`, never an abort
            // (the interp's map_lookup contract).
            IrExprKind::MapAccess { object, key } => {
                let args = [(**object).clone(), (**key).clone()];
                match self.lower_map_call("get", &args, want)? {
                    Some(t) => t,
                    None => return unsup("map-access-void"),
                }
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
            // `?` and `!` are ONE oracle marker (eval_try_unwrap) and
            // lower_sum handles Try | Unwrap in one arm — the routing
            // predicate omitting Try was the entire ×15 "expr:Try" wall.
            | IrExprKind::Try { .. }
            | IrExprKind::Unwrap { .. }
            | IrExprKind::UnwrapOr { .. }
            | IrExprKind::ToOption { .. }
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
                // A C-319 cell travels as its 4-byte ADDRESS.
                let widths: Vec<u32> = std::iter::once(4)
                    .chain(captured.iter().map(|(v, t)| {
                        if self.cells.contains(v) { 4 } else { t.slot_size() }
                    }))
                    .collect();
                let (offsets, size) = almide_layout::pack_fields(&widths);
                let captures: Vec<(VarId, SliceTy, u32, bool)> = captured
                    .iter()
                    .zip(offsets.iter().skip(1))
                    .map(|(&(v, t), &off)| (v, t, off, self.cells.contains(&v)))
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
                    for (v, t, off, is_cell) in &captures {
                        let (idx, _) = self.locals[v];
                        self.f.instructions().local_get(hb).local_get(idx);
                        if *is_cell {
                            // the local already holds the cell address
                            self.f.instructions().i32_store(slot_memarg(*off));
                        } else {
                            self.store_ty_slot(*t, *off);
                        }
                    }
                    self.f.instructions().local_get(hb);
                    self.release_i32();
                }
        Ok(ty)
    }
}
