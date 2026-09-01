//! Body lowering: the Emitter and its value/statement machinery.

use std::collections::{HashMap, HashSet};

use almide_ir::{IrExpr, IrExprKind, UnOp, VarId};
use wasm_encoder::{BlockType, Function, ValType};

use crate::*;

// ── body lowering ───────────────────────────────────────────────────────

pub(crate) struct Emitter<'a> {
    pub(crate) pool: &'a mut Pool,
    pub(crate) locals: &'a HashMap<VarId, (u32, SliceTy)>,
    /// Locals below this index are PARAMS (borrowed views of the
    /// caller's blocks): the COW gate exempts them — in-place writes
    /// through a plain Bytes param are the caller-visibility contract
    /// (bytes_param_writeback), exactly the pre-share behavior.
    pub(crate) rc_param_ceiling: u32,
    /// Local indices of the DROPPABLE params (env_shift applied) — the
    /// epilogue's release set, ALSO released at return_call sites: a tail
    /// call REPLACES the frame, so the epilogue never runs there and a
    /// droppable param (a Str accumulator in TCO) leaked every hop
    /// (spec/churn/string_accumulator_churn's grow_tco half OOM'd at the
    /// commissioning). Args are +1'd by rc_arg_guard BEFORE this release,
    /// so a pass-through param survives its own dec.
    pub(crate) rc_droppable_params: Vec<u32>,
    /// Locals the Bind/Assign routes made OWNERS of a droppable block
    /// (RC-3): exactly these get the fall-through epilogue dec. Pattern
    /// and loop binds never enter — they borrow their subject's
    /// interior. BTreeSet: the dec order must be deterministic.
    pub(crate) rc_owned: std::collections::BTreeSet<u32>,
    // NOTE: rc_owned and rc_droppable_params are BOTH dec'd by the
    // epilogue — a local in the two sets at once is a double free. Use
    // rc_own(), never a raw insert (#1770: a mut-param writeback's
    // Assign made the PARAM an "owner", the epilogue dec'd it twice,
    // and the freed-but-returned buffer's freelist link zeroed its
    // first payload word).
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
    /// Top-let globals ((space, VarId) → wasm global index + type): the
    /// fallback when a Var/Assign misses the locals map. Spaced (#1596):
    /// separately-lowered modules each restart VarIds at 0, so the bare
    /// id is ambiguous — lookups pair it with `var_space`.
    pub(crate) globals: &'a HashMap<crate::GVar, (u32, SliceTy)>,
    /// Which VarTable this function's VarIds index (0 = the entry
    /// program, i+1 = `ir.modules[i]`).
    pub(crate) var_space: u32,
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
    /// #1696 phase A: armed by lower_fn when the straightline gate
    /// admits the body — the Bind route and the epilogue record their
    /// RC events here; the certificate is pushed to the witness sink.
    pub(crate) witness: Option<crate::witness::WitnessRecorder>,
    /// Positive while lowering an if/match arm. A droppable PARAM reassigned
    /// under a branch has no sound ownership story (one path frees the
    /// caller's block, the other keeps it — #1688 shipped wrong bytes
    /// silently); the C-132 fold removes the shapes it can prove, and
    /// `lower_assign` WALLS the rest via this counter.
    pub(crate) branch_depth: u32,
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
pub(crate) const HOLD_I32_POOL: u32 = 24;
pub(crate) const HOLD_I64_POOL: u32 = 16;
pub(crate) const HOLD_F64_POOL: u32 = 8;

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




    /// xs[i]: bounds-checked element load. Out of bounds takes the exact
    /// native abort frame — `Error: index out of bounds` on stderr, exit 1
    /// (a bare trap here left stderr EMPTY, the cross-target divergence
    /// develop's xtarget gate caught at the commissioning: stdout and exit
    /// matched, the message did not).
    fn lower_index_access(
        &mut self,
        object: &IrExpr,
        index: &IrExpr,
    ) -> Result<SliceTy, EmitError> {
        let elem = match self.lower(object, None)? {
            SliceTy::List(h) => self.types.el(h),
            other => return Err(EmitError::Unsupported(format!("index-of:{other:?}"))),
        };
        let stride = elem.slot_size();
        let hold = self.hold_i32()?;
        self.f.instructions().local_set(hold);
        self.lower(index, Some(INT))?;
        let idx = self.hold_i64()?;
        self.f.instructions().local_tee(idx);
        let msg = self.pool.intern("index out of bounds");
        // idx < 0 || idx >= count → the message abort
        {
            let mut i = self.f.instructions();
            i.i64_const(0).i64_lt_s();
            i.local_get(idx);
            i.local_get(hold).i32_load(len_memarg()).i32_const(stride as i32).i32_div_u();
            i.i64_extend_i32_u().i64_ge_s();
            i.i32_or().if_(BlockType::Empty);
            i.i32_const(msg as i32);
        }
        self.emit_error_frame_abort();
        let mut i = self.f.instructions();
        i.end();
        // element address: hold + idx*stride, slot at offset PAYLOAD
        i.local_get(hold);
        i.local_get(idx).i32_wrap_i64().i32_const(stride as i32).i32_mul().i32_add();
        self.load_ty_slot(elem, 0);
        self.release_i64();
        self.release_i32();
        Ok(elem)
    }
    /// main's err channel (#1734, the pre-existing structural hole): a
    /// Result-typed expression in MAIN's statement/tail position is the
    /// effect carrier, not a discardable value — the native/interp
    /// contract is `Error: {msg}` on stderr + exit 1 on err, plain
    /// fallthrough on ok. Discarding it swallowed the err (silent exit
    /// 0 — `effect fn main() -> Unit = err("boom")` on the released
    /// 0.61.0). Returns Ok(true) when this handled the expression.
    /// A non-String err payload walls honestly (no message to print).
    pub(crate) fn try_lower_main_err_carrier(&mut self, e: &IrExpr) -> Result<bool, EmitError> {
        use almide_types::types::{Ty, TypeConstructorId};
        if !self.in_main {
            return Ok(false);
        }
        let Ty::Applied(TypeConstructorId::Result, a) = &e.ty else {
            return Ok(false);
        };
        if a.len() != 2 {
            return Ok(false);
        }
        let got = self.lower(e, None)?;
        let SliceTy::Result(_, eh) = got else {
            // Effect-ABI transparency already unwrapped it — nothing to route.
            self.f.instructions().drop();
            return Ok(true);
        };
        if self.types.el(eh) != STR {
            return Err(EmitError::Unsupported("main-err-carrier:non-string-err".into()));
        }
        let hb = self.scr_i32_local;
        let mut i = self.f.instructions();
        i.local_set(hb);
        i.local_get(hb)
            .i32_load(slot_memarg(almide_layout::SUM_TAG))
            .if_(BlockType::Empty);
        i.local_get(hb).i32_load(slot_memarg(almide_layout::SUM_FIELD));
        let _ = i;
        self.emit_error_frame_abort();
        self.f.instructions().end();
        Ok(true)
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
            IrExprKind::MapLiteral { entries } => self.lower_map_literal(e, entries, want)?,
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
                } else if let Some(&(gidx, ty)) = self.globals.get(&(self.var_space, *id)) {
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
                // The expr's own checker type first; the SURROUNDING
                // expectation as fallback — a generic ctor in argument
                // position (FNil inside FCons) has an unresolved own
                // type but a fully-instanced want.
                let hint = slice_ty_of(&e.ty, self.types).or(want);
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
                    self.rc_share_guard(el, elem);
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
            IrExprKind::IndexAccess { object, index } => self.lower_index_access(object, index)?,
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
                self.branch_depth += 1;
                let arms = (|| {
                    self.in_tail = tail;
                    self.lower(then, Some(ty))?;
                    self.f.instructions().else_();
                    self.in_tail = tail;
                    self.lower(else_, Some(ty))
                })();
                self.branch_depth -= 1;
                arms?;
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

