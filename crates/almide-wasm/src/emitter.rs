//! Body lowering: the Emitter and its value/statement machinery.

use std::collections::{HashMap, HashSet};

use almide_ir::{BinOp, IrExpr, IrExprKind, IrStmt, IrStmtKind, UnOp, VarId};
use wasm_encoder::{BlockType, Function, ValType};

use crate::types_table::NamedDef;
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

    /// Statement position: Unit-typed shapes only (blocks, calls, control).
    pub(crate) fn lower_stmt_expr(&mut self, e: &IrExpr) -> Result<(), EmitError> {
        match &e.kind {
            IrExprKind::Block { stmts, expr } => {
                for s in stmts {
                    self.lower_stmt(s)?;
                }
                if let Some(tail) = expr {
                    self.lower_stmt_expr(tail)?;
                }
                Ok(())
            }
            IrExprKind::Call { target, args, .. } => {
                // Unit-position call: a value-returning callee's result is
                // dropped (a bare non-Unit call statement is legal IR).
                if self.lower_call(target, args)?.is_some() {
                    self.f.instructions().drop();
                }
                Ok(())
            }
            // Unit-position `if`: both arms are statement bodies.
            IrExprKind::If { cond, then, else_ } => {
                self.lower(cond, Some(BOOL))?;
                self.f.instructions().if_(BlockType::Empty);
                self.lower_stmt_expr(then)?;
                self.f.instructions().else_();
                self.lower_stmt_expr(else_)?;
                self.f.instructions().end();
                Ok(())
            }
            // `while`: block { loop { !cond → br out; body; br loop } }.
            // Break/Continue would need label-depth tracking — they surface
            // as their own honest reasons until that lands.
            IrExprKind::While { cond, body } => {
                self.f.instructions().block(BlockType::Empty).loop_(BlockType::Empty);
                self.lower(cond, Some(BOOL))?;
                self.f.instructions().i32_eqz().br_if(1);
                for s in body {
                    self.lower_stmt(s)?;
                }
                self.f.instructions().br(0).end().end();
                Ok(())
            }
            IrExprKind::Match { subject, arms } => self.lower_match(subject, arms, None).map(|_| ()),
            // for x in <list> / for i in a..b — extracted for complexity.
            IrExprKind::ForIn { var, var_tuple, iterable, body } => {
                self.lower_forin(*var, var_tuple.as_deref(), iterable, body)
            }
            IrExprKind::Unit => Ok(()),
            other => unsup(&format!("expr:{}", expr_kind_name(other))),
        }
    }

    pub(crate) fn lower_stmt(&mut self, s: &IrStmt) -> Result<(), EmitError> {
        match &s.kind {
            IrStmtKind::Bind { var, value, .. } => {
                let Some(&(idx, declared)) = self.locals.get(var) else {
                    return unsup("bind:unmapped");
                };
                self.lower(value, Some(declared))?;
                // Container value semantics: every bind owns a fresh
                // block, so in-place mutation (push growth, bytes.set_*)
                // can never be observed through aliases. Bytes joined
                // when the snapshot fixture showed `let snap = arena`
                // observing later set_at writes.
                if matches!(
                    declared,
                    SliceTy::List(_)
                        | SliceTy::Map(..)
                        | SliceTy::Set(_)
                        | SliceTy::Scalar(Scalar::Bytes)
                ) {
                    self.f.instructions().call(F_BLOCK_COPY);
                }
                self.f.instructions().local_set(idx);
                Ok(())
            }
            IrStmtKind::Assign { var, value } => {
                let Some(&(idx, declared)) = self.locals.get(var) else {
                    return unsup("assign:unmapped");
                };
                self.lower(value, Some(declared))?;
                if matches!(
                    declared,
                    SliceTy::List(_)
                        | SliceTy::Map(..)
                        | SliceTy::Set(_)
                        | SliceTy::Scalar(Scalar::Bytes)
                ) {
                    self.f.instructions().call(F_BLOCK_COPY);
                }
                self.f.instructions().local_set(idx);
                Ok(())
            }
            IrStmtKind::Expr { expr } => self.lower_stmt_expr(expr),
            // let (a, b) = e — evaluate once, load each bound position.
            IrStmtKind::BindDestructure { pattern, value } => {
                let ty = self.lower(value, None)?;
                let scr = self.scr_i32_local;
                self.f.instructions().local_set(scr);
                self.emit_pattern_binds(pattern, ty, scr)
            }
            IrStmtKind::Comment { .. } => Ok(()),
            other => unsup(&format!("stmt:{}", stmt_kind_name(other))),
        }
    }

    /// A call in any position. Returns the callee's slice return type
    /// (None = Unit). `println`/`eprintln` are the special forms.
    /// `for` loops: a Range iterates Int directly; a List walks its
    /// element array. The loop variable is a pre-collected local.
    pub(crate) fn lower_forin(
        &mut self,
        var: VarId,
        var_tuple: Option<&[VarId]>,
        iterable: &IrExpr,
        body: &[IrStmt],
    ) -> Result<(), EmitError> {
        let Some(&(var_idx, var_ty)) = self.locals.get(&var) else {
            return unsup("bind:unmapped");
        };

                if let IrExprKind::Range { start, end, inclusive } = &iterable.kind {
                    // Range: var runs start..end directly, no list at all.
                    if var_ty != INT {
                        return unsup("forin-range-nonint");
                    }
                    self.lower(start, Some(INT))?;
                    self.f.instructions().local_set(var_idx);
                    self.lower(end, Some(INT))?;
                    let stop = self.hold_i64()?;
                    self.f.instructions().local_set(stop);
                    self.f.instructions().block(BlockType::Empty).loop_(BlockType::Empty);
                    self.f.instructions().local_get(var_idx).local_get(stop);
                    if *inclusive {
                        self.f.instructions().i64_gt_s();
                    } else {
                        self.f.instructions().i64_ge_s();
                    }
                    self.f.instructions().br_if(1);
                    for st in body {
                        self.lower_stmt(st)?;
                    }
                    self.f
                        .instructions()
                        .local_get(var_idx)
                        .i64_const(1)
                        .i64_add()
                        .local_set(var_idx)
                        .br(0)
                        .end()
                        .end();
                    self.release_i64();
                    return Ok(());
                }
                let elem = match self.lower(iterable, None)? {
                    SliceTy::List(h) => self.types.el(h),
                    other => return unsup(&format!("forin-iter:{other:?}")),
                };
                if var_ty != elem {
                    return unsup("forin-var-ty");
                }
                let stride = elem.slot_size();
                let base = self.hold_i32()?;
                let count = self.hold_i32()?;
                let cur = self.hold_i32()?;
                self.f.instructions().local_set(base);
                self.f
                    .instructions()
                    .local_get(base)
                    .i32_load(len_memarg())
                    .i32_const(stride as i32)
                    .i32_div_u()
                    .local_set(count)
                    .i32_const(0)
                    .local_set(cur);
                self.f.instructions().block(BlockType::Empty).loop_(BlockType::Empty);
                self.f.instructions().local_get(cur).local_get(count).i32_ge_u().br_if(1);
                self.f
                    .instructions()
                    .local_get(base)
                    .local_get(cur)
                    .i32_const(stride as i32)
                    .i32_mul()
                    .i32_add();
                self.load_ty_slot(elem, 0);
                self.f.instructions().local_set(var_idx);
                // for (a, b) in pairs — the loop var holds the tuple base;
                // load each position into its destructured local.
                if let Some(tvars) = var_tuple {
                    let SliceTy::Tuple(ti) = elem else {
                        return unsup("forin-tuple-nontuple");
                    };
                    let def = self.types.tuple_def(ti);
                    if def.fields.len() != tvars.len() {
                        return unsup("forin-tuple-arity");
                    }
                    for (tv, (fty, off)) in tvars.iter().zip(def.fields) {
                        let Some(&(tidx, _)) = self.locals.get(tv) else {
                            return unsup("bind:unmapped");
                        };
                        self.f.instructions().local_get(var_idx);
                        self.load_ty_slot(fty, off);
                        self.f.instructions().local_set(tidx);
                    }
                }
                for st in body {
                    self.lower_stmt(st)?;
                }
                self.f
                    .instructions()
                    .local_get(cur)
                    .i32_const(1)
                    .i32_add()
                    .local_set(cur)
                    .br(0)
                    .end()
                    .end();
                self.release_i32();
                self.release_i32();
                self.release_i32();
                Ok(())
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
                let Some(&(idx, ty)) = self.locals.get(id) else {
                    return unsup("var:unmapped");
                };
                self.f.instructions().local_get(idx);
                ty
            }
            IrExprKind::RuntimeCall { symbol, args } => {
                // The slice SYNTAX `xs[a..b]` desugars to this runtime
                // symbol — one impl with `list.slice` (as in native rt).
                if symbol.as_str() == "almide_rt_list_slice" && args.len() == 3 {
                    match self.lower_list_call("slice", args)? {
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

    /// Comparison operators — Int ordering, and equality over ints,
    /// bools, and byte-equal blocks (strings, List[Int/Bool]).
    /// List[String] holds addresses — identity is NOT equality: refused.
    pub(crate) fn lower_cmp(
        &mut self,
        op: BinOp,
        left: &IrExpr,
        right: &IrExpr,
    ) -> Result<SliceTy, EmitError> {
        use BinOp::*;
        if matches!(op, Lt | Gt | Lte | Gte) {
            let lt = self.lower(left, None)?;
            self.lower(right, Some(lt))?;
            let mut i = self.f.instructions();
            match (lt, op) {
                (INT, Lt) => i.i64_lt_s(),
                (INT, Gt) => i.i64_gt_s(),
                (INT, Lte) => i.i64_le_s(),
                (INT, Gte) => i.i64_ge_s(),
                (FLOAT, Lt) => i.f64_lt(),
                (FLOAT, Gt) => i.f64_gt(),
                (FLOAT, Lte) => i.f64_le(),
                (FLOAT, Gte) => i.f64_ge(),
                (other, _) => return unsup(&format!("binop:cmp-{other:?}")),
            };
            return Ok(BOOL);
        }
        let lt = self.lower(left, None)?;
        self.lower(right, Some(lt))?;
        match lt {
            INT => {
                self.f.instructions().i64_eq();
            }
            FLOAT => {
                self.f.instructions().f64_eq();
            }
            BOOL => {
                self.f.instructions().i32_eq();
            }
            // Block byte-equality: strings, and lists whose PAYLOAD bytes
            // ARE the values (Int/Bool elements). Any element that is an
            // address (Str, lists, sums, records) makes byte-compare an
            // identity test, not equality: refused.
            STR => {
                self.f.instructions().call(F_STR_EQ);
            }
            SliceTy::List(h)
                if matches!(
                    self.types.el(h),
                    SliceTy::Scalar(Scalar::Int) | SliceTy::Scalar(Scalar::Bool)
                ) =>
            {
                self.f.instructions().call(F_STR_EQ);
            }
            other => return unsup(&format!("binop:eq-{other:?}")),
        }
        if matches!(op, Neq) {
            self.f.instructions().i32_eqz();
        }
        Ok(BOOL)
    }

    /// Sum-shaped values: constructors and unwraps — split from
    /// `lower_data` for complexity budget. The `want` check happens in
    /// `lower`'s shared tail.
    pub(crate) fn lower_sum(
        &mut self,
        e: &IrExpr,
        want: Option<SliceTy>,
    ) -> Result<SliceTy, EmitError> {
        let got = match &e.kind {
            // Sum constructors — `none`/`ok`/`err` REQUIRE the hint.
            IrExprKind::OptionNone => match want.map_or_else(|| self.infer(e), Ok)? {
                SliceTy::Option(s) => {
                    self.f.instructions().i32_const(almide_layout::NULL_ADDR as i32);
                    SliceTy::Option(s)
                }
                other => return unsup(&format!("ty-mismatch:none-vs-{other:?}")),
            },
            IrExprKind::OptionSome { expr } => {
                let (hty, s) = match want.map_or_else(|| self.infer(e), Ok)? {
                    SliceTy::Option(h) => (SliceTy::Option(h), self.types.el(h)),
                    other => return unsup(&format!("ty-mismatch:some-vs-{other:?}")),
                };
                // The base lives in a HOLD local (stack-disciplined),
                // never the shared tmp: the inner expression can contain
                // its own `some(...)`/`ok(...)` as a SUBEXPRESSION even
                // when the types forbid nested sums — the differential
                // fuzzer falsified the old shared-tmp argument on day one
                // (seed 79: the outer `some` returned the inner block).
                let hold = self.hold_i32()?;
                self.f
                    .instructions()
                    .i32_const(s.slot_size() as i32)
                    .call(F_ALLOC)
                    .local_tee(hold);
                self.lower(expr, Some(s))?;
                self.store_ty_slot(s, almide_layout::OPTION_FIELD);
                self.f.instructions().local_get(hold);
                self.release_i32();
                hty
            }
            IrExprKind::ResultOk { expr } | IrExprKind::ResultErr { expr } => {
                let is_ok = matches!(&e.kind, IrExprKind::ResultOk { .. });
                let (hty, o, er) = match want.map_or_else(|| self.infer(e), Ok)? {
                    SliceTy::Result(o, er) => (SliceTy::Result(o, er), o, er),
                    other => return unsup(&format!("ty-mismatch:result-vs-{other:?}")),
                };
                let side = self.types.el(if is_ok { o } else { er });
                // Hold-local, not shared tmp — same seed-79 lesson as
                // OptionSome above.
                let hold = self.hold_i32()?;
                self.f
                    .instructions()
                    .i32_const(16)
                    .call(F_ALLOC)
                    .local_tee(hold)
                    .i32_const(i32::from(!is_ok))
                    .i32_store(slot_memarg(almide_layout::SUM_TAG));
                self.f.instructions().local_get(hold);
                self.lower(expr, Some(side))?;
                self.store_ty_slot(side, almide_layout::SUM_FIELD);
                self.f.instructions().local_get(hold);
                self.release_i32();
                hty
            }
            // `!` — ABORT form only. In a pure fn returning Option/Result
            // the oracle PROPAGATES instead (#1410 family): refuse those.
            IrExprKind::Unwrap { expr } => {
                if matches!(self.fn_ret, Some(SliceTy::Option(_) | SliceTy::Result(..))) {
                    return unsup("unwrap-propagating");
                }
                match self.lower(expr, None)? {
                    SliceTy::Option(h) => {
                        let et = self.types.el(h);
                        self.f
                            .instructions()
                            .local_tee(self.scr_i32_local)
                            .i32_eqz()
                            .if_(BlockType::Empty)
                            .unreachable()
                            .end()
                            .local_get(self.scr_i32_local);
                        self.load_ty_slot(et, almide_layout::OPTION_FIELD);
                        et
                    }
                    SliceTy::Result(o, _) => {
                        let et = self.types.el(o);
                        self.f
                            .instructions()
                            .local_tee(self.scr_i32_local)
                            .i32_load(slot_memarg(almide_layout::SUM_TAG))
                            .i32_const(0)
                            .i32_ne()
                            .if_(BlockType::Empty)
                            .unreachable()
                            .end()
                            .local_get(self.scr_i32_local);
                        self.load_ty_slot(et, almide_layout::SUM_FIELD);
                        et
                    }
                    other => return unsup(&format!("unwrap-of:{other:?}")),
                }
            }
            // `??` — fallback on none/Err. The fallback branch may clobber
            // the scratch, but the branch that reads the scratch is the
            // exclusive other path.
            IrExprKind::UnwrapOr { expr, fallback } => match self.lower(expr, None)? {
                SliceTy::Option(h) => {
                    let et = self.types.el(h);
                    self.f
                        .instructions()
                        .local_tee(self.scr_i32_local)
                        .i32_eqz()
                        .if_(BlockType::Result(et.val_type()));
                    self.lower(fallback, Some(et))?;
                    self.f.instructions().else_().local_get(self.scr_i32_local);
                    self.load_ty_slot(et, almide_layout::OPTION_FIELD);
                    self.f.instructions().end();
                    et
                }
                SliceTy::Result(o, _) => {
                    let et = self.types.el(o);
                    self.f
                        .instructions()
                        .local_tee(self.scr_i32_local)
                        .i32_load(slot_memarg(almide_layout::SUM_TAG))
                        .i32_const(0)
                        .i32_ne()
                        .if_(BlockType::Result(et.val_type()));
                    self.lower(fallback, Some(et))?;
                    self.f.instructions().else_().local_get(self.scr_i32_local);
                    self.load_ty_slot(et, almide_layout::SUM_FIELD);
                    self.f.instructions().end();
                    et
                }
                other => return unsup(&format!("unwrap-or-of:{other:?}")),
            },
            other => return unsup(&format!("expr:{}", expr_kind_name(other))),
        };
        Ok(got)
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
    /// Record-shaped values: literals, spreads, member reads — split from
    /// `lower_data` for complexity budget.
    pub(crate) fn lower_record(
        &mut self,
        e: &IrExpr,
        want: Option<SliceTy>,
    ) -> Result<SliceTy, EmitError> {
        let got = match &e.kind {
            // Tuple literal: positional record.
            IrExprKind::Tuple { elements } => {
                let ty = want.map_or_else(|| self.infer(e), Ok)?;
                let SliceTy::Tuple(ti) = ty else {
                    return unsup(&format!("ty-mismatch:tuple-vs-{ty:?}"));
                };
                let def = self.types.tuple_def(ti);
                if def.fields.len() != elements.len() {
                    return unsup("tuple-arity");
                }
                let hold = self.hold_i32()?;
                self.f.instructions().i32_const(def.size as i32).call(F_ALLOC).local_set(hold);
                for (el, (fty, off)) in elements.iter().zip(def.fields) {
                    self.f.instructions().local_get(hold);
                    self.lower(el, Some(fty))?;
                    self.store_ty_slot(fty, off);
                }
                self.f.instructions().local_get(hold);
                self.release_i32();
                ty
            }
            // t.0 / t.1 — positional field read.
            IrExprKind::TupleIndex { object, index } => {
                let ty = self.lower(object, None)?;
                let SliceTy::Tuple(ti) = ty else {
                    return unsup(&format!("tuple-index-of:{ty:?}"));
                };
                let def = self.types.tuple_def(ti);
                let Some(&(fty, off)) = def.fields.get(*index) else {
                    return unsup("tuple-index-oob");
                };
                self.load_ty_slot(fty, off);
                fty
            }
            // Record literal: alloc + store each field at its packed offset.
            IrExprKind::Record { name, fields } if name.is_some() => {
                let ty = want.map_or_else(|| self.infer(e), Ok)?;
                let SliceTy::Named(ti) = ty else {
                    return unsup(&format!("ty-mismatch:record-vs-{ty:?}"));
                };
                // A record LITERAL with a variant type is a record-shaped
                // CASE construction (`Scroll { dy: 3 }`).
                if let NamedDef::Variant(v) = &self.types.def(ti) {
                    let Some(cname) = name else {
                        return unsup("record-case-unnamed");
                    };
                    let Some(c) = v.cases.iter().find(|c| c.name == cname.as_str()) else {
                        return unsup("record-case-unknown");
                    };
                    if c.fields.len() != fields.len() {
                        return unsup("record-case-defaults");
                    }
                    let mut slots = Vec::new();
                    for (fname, _) in fields {
                        match c.fields.iter().find(|fi| fi.name == fname.as_str()) {
                            Some(fi) => slots.push((fi.ty, fi.offset)),
                            None => return unsup("record-case-unknown-field"),
                        }
                    }
                    let (size, tag) = (c.size, c.tag);
                    let hold = self.hold_i32()?;
                    self.f
                        .instructions()
                        .i32_const(size as i32)
                        .call(F_ALLOC)
                        .local_tee(hold)
                        .i32_const(tag as i32)
                        .i32_store(slot_memarg(almide_layout::SUM_TAG));
                    for ((_, fexpr), (fty, off)) in fields.iter().zip(slots) {
                        self.f.instructions().local_get(hold);
                        self.lower(fexpr, Some(fty))?;
                        self.store_ty_slot(fty, off);
                    }
                    self.f.instructions().local_get(hold);
                    self.release_i32();
                    return Ok(ty);
                }
                let NamedDef::Record(def) = &self.types.def(ti) else {
                    return unsup("record-of-variant-ty");
                };
                // Defaulted fields: until we verify the checker fills
                // omissions into the literal, a literal that supplies
                // fewer fields than the layout is REFUSED — a missing
                // store would leave header-garbage in the slot.
                if fields.len() != def.fields.len() {
                    return unsup("record-defaults");
                }
                let size = def.size;
                // (name → (offset, ty)) resolved up front to end the borrow.
                let mut slots = Vec::new();
                for (fname, _) in fields {
                    match def.fields.iter().find(|fi| fi.name == fname.as_str()) {
                        Some(fi) => slots.push((fi.ty, fi.offset)),
                        None => return unsup("record-unknown-field"),
                    }
                }
                let hold = self.hold_i32()?;
                self.f.instructions().i32_const(size as i32).call(F_ALLOC).local_set(hold);
                for ((_, fexpr), (fty, off)) in fields.iter().zip(slots) {
                    self.f.instructions().local_get(hold);
                    self.lower(fexpr, Some(fty))?;
                    self.store_ty_slot(fty, off);
                }
                self.f.instructions().local_get(hold);
                self.release_i32();
                ty
            }
            IrExprKind::Record { name: None, .. } => return unsup("record-anon"),
            // {...base, f: v}: copy then overwrite — functional update.
            IrExprKind::SpreadRecord { base, fields } => {
                let ty = self.lower(base, None)?;
                let SliceTy::Named(ti) = ty else {
                    return unsup(&format!("spread-of:{ty:?}"));
                };
                let NamedDef::Record(def) = &self.types.def(ti) else {
                    return unsup("spread-of-variant");
                };
                let mut slots = Vec::new();
                for (fname, _) in fields {
                    match def.fields.iter().find(|fi| fi.name == fname.as_str()) {
                        Some(fi) => slots.push((fi.ty, fi.offset)),
                        None => return unsup("record-unknown-field"),
                    }
                }
                let hold = self.hold_i32()?;
                self.f.instructions().call(F_BLOCK_COPY).local_set(hold);
                for ((_, fexpr), (fty, off)) in fields.iter().zip(slots) {
                    self.f.instructions().local_get(hold);
                    self.lower(fexpr, Some(fty))?;
                    self.store_ty_slot(fty, off);
                }
                self.f.instructions().local_get(hold);
                self.release_i32();
                ty
            }
            // r.field: offset load from the record block.
            IrExprKind::Member { object, field } => {
                let ty = self.lower(object, None)?;
                let SliceTy::Named(ti) = ty else {
                    return unsup(&format!("member-of:{ty:?}"));
                };
                let NamedDef::Record(def) = &self.types.def(ti) else {
                    return unsup("member-of-variant");
                };
                let Some(fi) = def.fields.iter().find(|fi| fi.name == field.as_str()) else {
                    return unsup("record-unknown-field");
                };
                let (fty, off) = (fi.ty, fi.offset);
                self.load_ty_slot(fty, off);
                fty
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
            | IrExprKind::Unwrap { .. }
            | IrExprKind::UnwrapOr { .. }
    )
}
