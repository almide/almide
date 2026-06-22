// IrExpr → WASM: type-param helpers, Result/Option inference, loop
// peepholes (split from expressions.rs).
//
// Part 3 of `expressions.rs` — `include!`d at the END of the parent, so it
// shares the parent module's imports. The free fns and `impl FuncCompiler`
// blocks below are moved VERBATIM.

/// Collect type parameter names from a type (Named("X", []) where X is a single-letter or TypeVar).
pub(super) fn collect_type_param_names<'a>(ty: &'a Ty, names: &mut Vec<&'a str>) {
    match ty {
        Ty::Named(name, args) if args.is_empty() && name.len() <= 2 && name.chars().next().map_or(false, |c| c.is_uppercase()) => {
            if !names.contains(&name.as_str()) {
                names.push(name.as_str());
            }
        }
        Ty::TypeVar(name) => {
            if !names.contains(&name.as_str()) {
                names.push(name.as_str());
            }
        }
        Ty::Applied(_, args) => { for a in args { collect_type_param_names(a, names); } }
        Ty::Tuple(elems) => { for e in elems { collect_type_param_names(e, names); } }
        Ty::Fn { params, ret } => {
            for p in params { collect_type_param_names(p, names); }
            collect_type_param_names(ret, names);
        }
        _ => {}
    }
}

/// Substitute type parameters in a type. Named("T", []) → type_args[index of "T"].
pub(super) fn substitute_type_params(ty: &Ty, generic_names: &[&str], type_args: &[Ty]) -> Ty {
    match ty {
        Ty::Named(name, args) if args.is_empty() => {
            // Check if this is a type parameter name
            if let Some(idx) = generic_names.iter().position(|&g| g == name.as_str()) {
                if let Some(concrete) = type_args.get(idx) {
                    return concrete.clone();
                }
            }
            // Also check TypeVar style
            ty.clone()
        }
        Ty::TypeVar(name) => {
            if let Some(idx) = generic_names.iter().position(|&g| g == name.as_str()) {
                if let Some(concrete) = type_args.get(idx) {
                    return concrete.clone();
                }
            }
            ty.clone()
        }
        // Recursively substitute in all other type constructors
        _ => ty.map_children(&|child| substitute_type_params(child, generic_names, type_args)),
    }
}

impl FuncCompiler<'_> {
    /// Resolve the inner type of a ResultOk/ResultErr when inner.ty is Unknown.
    /// Tries: 1) outer expr.ty Result[T,E] args, 2) inner expr IR kind inference.
    pub(super) fn resolve_result_inner_ty(&self, expr: &IrExpr, is_ok: bool) -> Ty {
        use almide_lang::types::constructor::TypeConstructorId;
        // Try from outer Result type
        if let Ty::Applied(TypeConstructorId::Result, args) = &expr.ty {
            let candidate = if is_ok {
                args.first().cloned().unwrap_or(Ty::Unknown)
            } else {
                args.get(1).cloned().unwrap_or(Ty::Unknown)
            };
            if !matches!(candidate, Ty::Unknown) {
                return candidate;
            }
        }
        // Fall back to inferring from inner expr
        let inner = match &expr.kind {
            IrExprKind::ResultOk { expr: e } | IrExprKind::ResultErr { expr: e } => e,
            _ => return Ty::Int,
        };
        self.infer_type_from_expr(inner)
    }

    /// Best-effort type inference from IR expression structure.
    pub(super) fn infer_type_from_expr(&self, expr: &IrExpr) -> Ty {
        if !matches!(expr.ty, Ty::Unknown) {
            return expr.ty.clone();
        }
        match &expr.kind {
            IrExprKind::LitInt { .. } => Ty::Int,
            IrExprKind::LitFloat { .. } => Ty::Float,
            IrExprKind::LitBool { .. } => Ty::Bool,
            IrExprKind::LitStr { .. } => Ty::String,
            IrExprKind::BinOp { op, left, .. } => {
                match op {
                    BinOp::AddInt | BinOp::SubInt | BinOp::MulInt | BinOp::DivInt | BinOp::ModInt
                    | BinOp::PowInt => Ty::Int,
                    BinOp::AddFloat | BinOp::SubFloat | BinOp::MulFloat | BinOp::DivFloat
                    | BinOp::ModFloat | BinOp::PowFloat => Ty::Float,
                    BinOp::Eq | BinOp::Neq | BinOp::Lt | BinOp::Gt | BinOp::Lte | BinOp::Gte
                    | BinOp::And | BinOp::Or => Ty::Bool,
                    BinOp::ConcatStr => Ty::String,
                    BinOp::MulMatrix | BinOp::AddMatrix | BinOp::SubMatrix | BinOp::ScaleMatrix => Ty::Matrix,
                    BinOp::ConcatList => {
                        let lt = self.infer_type_from_expr(left);
                        lt
                    }
                }
            }
            IrExprKind::UnOp { op, .. } => {
                match op {
                    UnOp::NegInt => Ty::Int,
                    UnOp::NegFloat => Ty::Float,
                    UnOp::Not => Ty::Bool,
                }
            }
            IrExprKind::Var { id } => {
                self.var_table.get(*id).ty.clone()
            }
            _ => Ty::Int, // conservative fallback
        }
    }

    /// Try to emit an inverted condition + br_if, avoiding a redundant i32_eqz.
    /// Returns true if successfully handled, false if caller should fall back.
    pub(super) fn try_emit_inverted_br_if(&mut self, cond: &IrExpr, br_depth: u32) -> bool {
        match &cond.kind {
            // k != 0 → emit k; i64.eqz; br_if (break when k == 0)
            IrExprKind::BinOp { op: BinOp::Neq, left, right } => {
                // Special case: x != 0 → i64.eqz
                if matches!(&right.kind, IrExprKind::LitInt { value: 0 }) && matches!(&left.ty, Ty::Int) {
                    self.emit_expr(left);
                    wasm!(self.func, { i64_eqz; br_if(br_depth); });
                    return true;
                }
                // General: x != y → emit eq, br_if (break when equal)
                self.emit_eq(left, right, false); // emit eq (no negate)
                wasm!(self.func, { br_if(br_depth); });
                true
            }
            // x < y → emit x, y, ge_s, br_if (break when x >= y)
            IrExprKind::BinOp { op: BinOp::Lt, left, right } => {
                self.emit_expr(left);
                self.emit_expr(right);
                self.emit_cmp_instruction(&left.ty, CmpKind::Gte);
                wasm!(self.func, { br_if(br_depth); });
                true
            }
            // x > y → emit x, y, le_s, br_if
            IrExprKind::BinOp { op: BinOp::Gt, left, right } => {
                self.emit_expr(left);
                self.emit_expr(right);
                self.emit_cmp_instruction(&left.ty, CmpKind::Lte);
                wasm!(self.func, { br_if(br_depth); });
                true
            }
            // x <= y → emit x, y, gt_s, br_if
            IrExprKind::BinOp { op: BinOp::Lte, left, right } => {
                self.emit_expr(left);
                self.emit_expr(right);
                self.emit_cmp_instruction(&left.ty, CmpKind::Gt);
                wasm!(self.func, { br_if(br_depth); });
                true
            }
            // x >= y → emit x, y, lt_s, br_if
            IrExprKind::BinOp { op: BinOp::Gte, left, right } => {
                self.emit_expr(left);
                self.emit_expr(right);
                self.emit_cmp_instruction(&left.ty, CmpKind::Lt);
                wasm!(self.func, { br_if(br_depth); });
                true
            }
            // x == y → emit neq, br_if
            IrExprKind::BinOp { op: BinOp::Eq, left, right } => {
                self.emit_eq(left, right, true); // emit neq
                wasm!(self.func, { br_if(br_depth); });
                true
            }
            // not(x) → emit x, br_if (no inversion needed)
            IrExprKind::UnOp { op: UnOp::Not, operand } => {
                self.emit_expr(operand);
                wasm!(self.func, { br_if(br_depth); });
                true
            }
            _ => false,
        }
    }

}

/// Check if an expression tree references a specific variable.
fn expr_references_var(expr: &almide_ir::IrExpr, var: almide_ir::VarId) -> bool {
    match &expr.kind {
        IrExprKind::Var { id } => *id == var,
        IrExprKind::BinOp { left, right, .. } => expr_references_var(left, var) || expr_references_var(right, var),
        IrExprKind::UnOp { operand, .. } => expr_references_var(operand, var),
        IrExprKind::Call { args, .. } => args.iter().any(|a| expr_references_var(a, var)),
        IrExprKind::Member { object, .. } => expr_references_var(object, var),
        IrExprKind::If { cond, then, else_ } => expr_references_var(cond, var) || expr_references_var(then, var) || expr_references_var(else_, var),
        _ => false,
    }
}

impl FuncCompiler<'_> {
    /// Detect and emit optimized while loop for string append:
    ///   while i < N { s = s + "x"; i = i + 1 }
    /// Hoists len/cap into locals for zero-reload tight loop.
    fn try_emit_string_append_loop(&mut self, cond: &IrExpr, body: &[almide_ir::IrStmt]) -> bool {
        use almide_ir::{IrStmtKind, BinOp};

        // Match body: exactly 2 statements
        if body.len() != 2 { return false; }

        // Statement 0: s = s + LitStr(1-char)
        let (str_var, byte_val) = if let IrStmtKind::Assign { var, value } = &body[0].kind {
            if let IrExprKind::BinOp { op: BinOp::ConcatStr, left, right } = &value.kind {
                if let (IrExprKind::Var { id }, IrExprKind::LitStr { value: lit }) = (&left.kind, &right.kind) {
                    if *id == *var && lit.len() == 1 {
                        (*var, lit.as_bytes()[0])
                    } else { return false; }
                } else { return false; }
            } else { return false; }
        } else { return false; };

        // Statement 1: i = i + 1
        let counter_var = if let IrStmtKind::Assign { var, value } = &body[1].kind {
            if let IrExprKind::BinOp { op: BinOp::AddInt, left, right } = &value.kind {
                if let (IrExprKind::Var { id }, IrExprKind::LitInt { value: 1 }) = (&left.kind, &right.kind) {
                    if *id == *var { *var } else { return false; }
                } else { return false; }
            } else { return false; }
        } else { return false; };

        // Guard: condition must not reference the string variable (its len is hoisted into a local)
        if expr_references_var(cond, str_var) { return false; }

        // Get local indices
        let str_local = match self.var_map.get(&str_var.0) { Some(&v) => v, None => return false };
        let counter_local = match self.var_map.get(&counter_var.0) { Some(&v) => v, None => return false };

        // Emit optimized loop with hoisted len/cap
        let s = self.scratch.alloc_i32();
        let len = self.scratch.alloc_i32();
        let cap = self.scratch.alloc_i32();

        // Hoist: load len and cap from string header
        wasm!(self.func, {
            local_get(str_local); local_tee(s);
            i32_load(0); local_set(len);
            local_get(s);
            i32_load(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::CAP) as i32 as u32);
            local_set(cap);
            // Loop
            block_empty; loop_empty;
        });
        let _g3 = self.depth_push();
        let break_depth = _g3.saved();
        let _g4 = self.depth_push(); // for loop_empty above (we're inside block+loop)

        // Condition check
        self.emit_expr(cond);
        wasm!(self.func, {
            i32_eqz;
            br_if(self.depth - break_depth - 1);
        });

        // Fast path: len < cap → inline byte store (NO memory read for len/cap)
        wasm!(self.func, {
            local_get(len); local_get(cap); i32_lt_u;
            if_empty;
              local_get(s);
              i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32);
              i32_add;
              local_get(len); i32_add;
              i32_const(byte_val as i32);
              i32_store8(0);
              local_get(len); i32_const(1); i32_add; local_set(len);
            else_;
              // Slow: write len back, grow, reload s/cap
              local_get(s); local_get(len); i32_store(0);
              // new_cap = max(cap*2, 16)
              local_get(cap); i32_const(1); i32_shl; local_tee(cap);
              i32_const(16); i32_lt_u;
              if_empty; i32_const(16); local_set(cap); end;
              // Alloc
              local_get(cap);
              i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32);
              i32_add;
              call(self.emitter.rt.alloc); local_tee(s);
              // Copy old data
              i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add;
              local_get(str_local);
              i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32); i32_add;
              local_get(len);
              memory_copy;
              // Write cap
              local_get(s); local_get(cap);
              i32_store(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::CAP) as i32 as u32);
              // Update str local
              local_get(s); local_set(str_local);
              // Write byte
              local_get(s);
              i32_const(self.emitter.layout_reg.fixed_offset(super::engine::layout::STRING, super::engine::layout::string::DATA) as i32);
              i32_add;
              local_get(len); i32_add;
              i32_const(byte_val as i32);
              i32_store8(0);
              local_get(len); i32_const(1); i32_add; local_set(len);
            end;
            // i++
            local_get(counter_local);
            i64_const(1); i64_add;
            local_set(counter_local);
        });

        // Continue
        wasm!(self.func, { br(0); });

        self.depth_pop(_g4);
        self.depth_pop(_g3);
        wasm!(self.func, { end; end; });

        // Write final len back to memory
        wasm!(self.func, {
            local_get(s); local_get(len); i32_store(0);
        });

        self.scratch.free_i32(cap);
        self.scratch.free_i32(len);
        self.scratch.free_i32(s);
        true
    }

    /// Check if `maybe_mod` is `x % n` with power-of-2 n and `maybe_zero` is `0`.
    /// Returns `(x_expr, n-1)` for emitting `x & (n-1)` instead.
    fn extract_mod_pow2_eq_zero<'b>(maybe_mod: &'b IrExpr, maybe_zero: &'b IrExpr) -> Option<(&'b IrExpr, i64)> {
        if let IrExprKind::LitInt { value: 0 } = &maybe_zero.kind {
            if let IrExprKind::BinOp { op: BinOp::ModInt, left, right } = &maybe_mod.kind {
                if let IrExprKind::LitInt { value: n } = &right.kind {
                    let n = *n;
                    if n > 0 && (n as u64).is_power_of_two() {
                        return Some((left, n - 1));
                    }
                }
            }
        }
        None
    }
}
