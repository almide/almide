//! IrProgram → Almide dialect Module converter.
//!
//! Walks the IR tree and produces SSA-form dialect operations.
//! Each IR expression is lowered to one or more Operations, with
//! the final Operation's result ValueId representing the expression's value.

use almide_base::intern::Sym;
use almide_ir::*;

use crate::{Module, Block, ValueId, IdGen};
use crate::ops::*;
use crate::types::{self, DialectType};

/// Lower an IrProgram into a dialect Module.
pub fn lower_program(program: &IrProgram) -> Module {
    let mut ctx = LowerCtx::new(&program.var_table);

    let functions: Vec<FuncOp> = program.functions.iter()
        .map(|f| ctx.lower_function(f))
        .collect();

    let type_decls: Vec<TypeDeclOp> = program.type_decls.iter()
        .map(|td| lower_type_decl(td))
        .collect();

    let globals: Vec<GlobalOp> = program.top_lets.iter()
        .map(|tl| ctx.lower_top_let(tl))
        .collect();

    Module {
        name: None,
        functions,
        type_decls,
        globals,
    }
}

/// Lowering context — tracks SSA value mapping and generates fresh IDs.
struct LowerCtx<'a> {
    ids: IdGen,
    /// Maps IR VarId → SSA ValueId (current binding).
    var_map: std::collections::HashMap<VarId, ValueId>,
    var_table: &'a VarTable,
    /// Mutable variables: VarId → alloca slot ValueId.
    /// When a var is mutable (Bind with mutability=Var), we emit AllocVar
    /// and track the slot. Reads become LoadVar, writes become StoreVar.
    mutable_slots: std::collections::HashMap<VarId, ValueId>,
}

impl<'a> LowerCtx<'a> {
    fn new(var_table: &'a VarTable) -> Self {
        LowerCtx {
            ids: IdGen::default(),
            var_map: std::collections::HashMap::new(),
            var_table,
            mutable_slots: std::collections::HashMap::new(),
        }
    }

    fn lower_function(&mut self, f: &IrFunction) -> FuncOp {
        let block_id = self.ids.fresh_block();
        let mut args = Vec::new();
        let mut params = Vec::new();

        for p in &f.params {
            let val = self.ids.fresh_value();
            let dty = types::from_ty(&p.ty);
            args.push((val, dty.clone()));
            params.push((p.name, dty));
            self.var_map.insert(p.var, val);
        }

        let (ops, result) = self.lower_expr(&f.body);
        let terminator = Terminator::Return(result);

        FuncOp {
            name: almide_base::intern::sym(&f.name),
            params,
            ret_ty: types::from_ty(&f.ret_ty),
            is_effect: f.is_effect,
            is_test: f.is_test,
            body: vec![Block { id: block_id, args, ops, terminator }],
        }
    }

    fn lower_top_let(&mut self, tl: &IrTopLet) -> GlobalOp {
        let block_id = self.ids.fresh_block();
        let (ops, result) = self.lower_expr(&tl.value);
        let terminator = Terminator::Yield(result);
        let name = self.var_table.get(tl.var).name;
        GlobalOp {
            name,
            ty: types::from_ty(&tl.ty),
            init: vec![Block { id: block_id, args: vec![], ops, terminator }],
        }
    }

    /// Lower an IR expression, returning the operations and the result ValueId.
    fn lower_expr(&mut self, expr: &IrExpr) -> (Vec<Operation>, ValueId) {
        let mut ops = Vec::new();
        let result = self.lower_expr_into(expr, &mut ops);
        (ops, result)
    }

    /// Lower an IR expression, appending operations to `ops`. Returns the result ValueId.
    fn lower_expr_into(&mut self, expr: &IrExpr, ops: &mut Vec<Operation>) -> ValueId {
        let result_ty = types::from_ty(&expr.ty);

        match &expr.kind {
            // ── Literals ──
            IrExprKind::LitInt { value } => self.emit(ops, result_ty, OpKind::ConstInt(*value)),
            IrExprKind::LitFloat { value } => self.emit(ops, result_ty, OpKind::ConstFloat(*value)),
            IrExprKind::LitStr { value } => self.emit(ops, result_ty, OpKind::ConstString(value.clone())),
            IrExprKind::LitBool { value } => self.emit(ops, result_ty, OpKind::ConstBool(*value)),
            IrExprKind::Unit => self.emit(ops, result_ty, OpKind::ConstUnit),

            // ── Variables ──
            IrExprKind::Var { id } => {
                if let Some(slot) = self.mutable_slots.get(id).copied() {
                    // Mutable variable: emit LoadVar
                    self.emit(ops, result_ty, OpKind::LoadVar { slot })
                } else {
                    *self.var_map.get(id).unwrap_or(&ValueId(0))
                }
            }
            IrExprKind::FnRef { name } => {
                // Function references become a callable constant
                self.emit(ops, result_ty, OpKind::ConstString(name.as_str().to_string()))
            }

            // ── Operators ──
            IrExprKind::BinOp { op, left, right } => {
                let lhs = self.lower_expr_into(left, ops);
                let rhs = self.lower_expr_into(right, ops);
                self.emit(ops, result_ty, OpKind::BinOp { op: *op, lhs, rhs })
            }
            IrExprKind::UnOp { op, operand } => {
                let val = self.lower_expr_into(operand, ops);
                self.emit(ops, result_ty, OpKind::UnOp { op: *op, operand: val })
            }

            // ── Control flow ──
            IrExprKind::If { cond, then, else_ } => {
                let cond_val = self.lower_expr_into(cond, ops);
                let then_block = self.lower_to_block(then);
                let else_block = self.lower_to_block(else_);
                self.emit(ops, result_ty, OpKind::IfOp {
                    cond: cond_val,
                    then_region: vec![then_block],
                    else_region: vec![else_block],
                })
            }
            IrExprKind::Match { .. } => self.lower_match_expr(expr, result_ty, ops),
            IrExprKind::Block { stmts, expr } => {
                for stmt in stmts {
                    self.lower_stmt(stmt, ops);
                }
                if let Some(e) = expr {
                    self.lower_expr_into(e, ops)
                } else {
                    self.emit(ops, DialectType::Unit, OpKind::ConstUnit)
                }
            }

            // ── Calls ──
            IrExprKind::Call { .. } => self.lower_call_expr(expr, result_ty, ops),
            IrExprKind::RuntimeCall { symbol, args } => {
                let arg_vals: Vec<ValueId> = args.iter()
                    .map(|a| self.lower_expr_into(a, ops))
                    .collect();
                self.emit(ops, result_ty, OpKind::IntrinsicCallOp { symbol: *symbol, args: arg_vals })
            }

            // ── Collections ──
            IrExprKind::List { elements } => {
                let vals: Vec<ValueId> = elements.iter()
                    .map(|e| self.lower_expr_into(e, ops))
                    .collect();
                self.emit(ops, result_ty, OpKind::ListOp { elements: vals })
            }
            IrExprKind::MapLiteral { entries } => {
                let pairs: Vec<(ValueId, ValueId)> = entries.iter()
                    .map(|(k, v)| {
                        let kv = self.lower_expr_into(k, ops);
                        let vv = self.lower_expr_into(v, ops);
                        (kv, vv)
                    })
                    .collect();
                self.emit(ops, result_ty, OpKind::MapOp { entries: pairs })
            }
            IrExprKind::EmptyMap => self.emit(ops, result_ty, OpKind::EmptyMapOp),
            IrExprKind::Record { name, fields } => {
                let field_vals: Vec<(Sym, ValueId)> = fields.iter()
                    .map(|(n, e)| (*n, self.lower_expr_into(e, ops)))
                    .collect();
                self.emit(ops, result_ty, OpKind::RecordOp { name: *name, fields: field_vals })
            }
            IrExprKind::Tuple { elements } => {
                let vals: Vec<ValueId> = elements.iter()
                    .map(|e| self.lower_expr_into(e, ops))
                    .collect();
                self.emit(ops, result_ty, OpKind::TupleOp { elements: vals })
            }

            // ── Access ──
            IrExprKind::Member { object, field } => {
                let obj = self.lower_expr_into(object, ops);
                self.emit(ops, result_ty, OpKind::MemberOp { object: obj, field: *field })
            }
            IrExprKind::TupleIndex { object, index } => {
                let obj = self.lower_expr_into(object, ops);
                self.emit(ops, result_ty, OpKind::TupleIndexOp { object: obj, index: *index })
            }
            IrExprKind::IndexAccess { object, index } => {
                let obj = self.lower_expr_into(object, ops);
                let idx = self.lower_expr_into(index, ops);
                self.emit(ops, result_ty, OpKind::IndexOp { object: obj, index: idx })
            }
            IrExprKind::MapAccess { object, key } => {
                let obj = self.lower_expr_into(object, ops);
                let k = self.lower_expr_into(key, ops);
                self.emit(ops, result_ty, OpKind::MapAccessOp { object: obj, key: k })
            }

            // ── Result / Option ──
            IrExprKind::ResultOk { expr } => {
                let v = self.lower_expr_into(expr, ops);
                self.emit(ops, result_ty, OpKind::ResultOkOp { value: v })
            }
            IrExprKind::ResultErr { expr } => {
                let v = self.lower_expr_into(expr, ops);
                self.emit(ops, result_ty, OpKind::ResultErrOp { value: v })
            }
            IrExprKind::OptionSome { expr } => {
                let v = self.lower_expr_into(expr, ops);
                self.emit(ops, result_ty, OpKind::OptionSomeOp { value: v })
            }
            IrExprKind::OptionNone => self.emit(ops, result_ty, OpKind::OptionNoneOp),
            IrExprKind::Try { expr } | IrExprKind::Unwrap { expr } => {
                let v = self.lower_expr_into(expr, ops);
                self.emit(ops, result_ty, OpKind::UnwrapOp { value: v })
            }
            IrExprKind::UnwrapOr { expr, fallback } => {
                let v = self.lower_expr_into(expr, ops);
                let fb = self.lower_expr_into(fallback, ops);
                self.emit(ops, result_ty, OpKind::UnwrapOrOp { value: v, fallback: fb })
            }

            // ── Lambda ──
            IrExprKind::Lambda { params, body, .. } => {
                let mut lambda_params = Vec::new();
                for (var_id, ty) in params {
                    let val = self.ids.fresh_value();
                    let dty = types::from_ty(ty);
                    lambda_params.push((val, dty));
                    self.var_map.insert(*var_id, val);
                }
                let body_block = self.lower_to_block(body);
                self.emit(ops, result_ty, OpKind::LambdaOp {
                    params: lambda_params,
                    body: vec![body_block],
                })
            }

            // ── Fan ──
            IrExprKind::Fan { exprs } => {
                let regions: Vec<Vec<Block>> = exprs.iter()
                    .map(|e| vec![self.lower_to_block(e)])
                    .collect();
                self.emit(ops, result_ty, OpKind::FanOp { regions })
            }

            // ── Loops ──
            IrExprKind::ForIn { var, iterable, body, .. } => {
                let iter_val = self.lower_expr_into(iterable, ops);
                let loop_var = self.ids.fresh_value();
                self.var_map.insert(*var, loop_var);
                let body_block = self.lower_stmts_to_block(body);
                self.emit(ops, result_ty, OpKind::ForOp {
                    var: loop_var,
                    iterable: iter_val,
                    body: vec![body_block],
                })
            }
            IrExprKind::While { cond, body } => {
                let cond_block = self.lower_to_block(cond);
                let body_block = self.lower_stmts_to_block(body);
                self.emit(ops, result_ty, OpKind::WhileOp {
                    cond_region: vec![cond_block],
                    body: vec![body_block],
                })
            }

            // ── Codegen-specific nodes: pass through or ignore ──
            IrExprKind::Clone { expr } | IrExprKind::Deref { expr }
            | IrExprKind::Borrow { expr, .. } | IrExprKind::BoxNew { expr }
            | IrExprKind::RcWrap { expr, .. } | IrExprKind::ToVec { expr }
            | IrExprKind::ToOption { expr } => {
                // These are Rust-target codegen artifacts — strip them.
                self.lower_expr_into(expr, ops)
            }

            // Fallback for nodes not yet handled
            _ => self.emit(ops, result_ty, OpKind::ConstUnit),
        }
    }

    fn lower_match_expr(&mut self, expr: &IrExpr, result_ty: DialectType, ops: &mut Vec<Operation>) -> ValueId {
        let IrExprKind::Match { subject, arms } = &expr.kind else { unreachable!() };
        let subj = self.lower_expr_into(subject, ops);
        let dialect_arms = arms.iter().map(|arm| {
            let pattern = self.lower_pattern(&arm.pattern);
            let guard = arm.guard.as_ref().map(|g| {
                let mut guard_ops = Vec::new();
                self.lower_expr_into(g, &mut guard_ops)
            });
            let body_block = self.lower_to_block(&arm.body);
            MatchArm { pattern, guard, body: vec![body_block] }
        }).collect();
        self.emit(ops, result_ty, OpKind::MatchOp { subject: subj, arms: dialect_arms })
    }

    fn lower_call_expr(&mut self, expr: &IrExpr, result_ty: DialectType, ops: &mut Vec<Operation>) -> ValueId {
        let IrExprKind::Call { target, args, .. } = &expr.kind else { unreachable!() };
        let arg_vals: Vec<ValueId> = args.iter()
            .map(|a| self.lower_expr_into(a, ops))
            .collect();
        match target {
            CallTarget::Module { module, func, .. } => {
                let callee = almide_base::intern::sym(&format!("{}.{}", module, func));
                self.emit(ops, result_ty, OpKind::CallOp { callee, args: arg_vals })
            }
            CallTarget::Named { name } => {
                self.emit(ops, result_ty, OpKind::CallOp { callee: *name, args: arg_vals })
            }
            CallTarget::Method { method, .. } => {
                self.emit(ops, result_ty, OpKind::CallOp { callee: *method, args: arg_vals })
            }
            CallTarget::Computed { callee } => {
                let callee_val = self.lower_expr_into(callee, ops);
                self.emit(ops, result_ty, OpKind::ComputedCallOp { callee: callee_val, args: arg_vals })
            }
        }
    }

    /// Lower an expression into a standalone Block.
    fn lower_to_block(&mut self, expr: &IrExpr) -> Block {
        let block_id = self.ids.fresh_block();
        let (ops, result) = self.lower_expr(expr);
        Block { id: block_id, args: vec![], ops, terminator: Terminator::Yield(result) }
    }

    /// Lower a slice of statements into a Block.
    fn lower_stmts_to_block(&mut self, stmts: &[IrStmt]) -> Block {
        let block_id = self.ids.fresh_block();
        let mut ops = Vec::new();
        for stmt in stmts {
            self.lower_stmt(stmt, &mut ops);
        }
        let unit = self.emit(&mut ops, DialectType::Unit, OpKind::ConstUnit);
        Block { id: block_id, args: vec![], ops, terminator: Terminator::Yield(unit) }
    }

    fn lower_stmt(&mut self, stmt: &IrStmt, ops: &mut Vec<Operation>) {
        match &stmt.kind {
            IrStmtKind::Bind { var, mutability, value, .. } => {
                let val = self.lower_expr_into(value, ops);
                if *mutability == almide_ir::Mutability::Var {
                    // Mutable variable: emit AllocVar + StoreVar
                    let dty = types::from_ty(&value.ty);
                    let slot = self.emit(ops, dty.clone(), OpKind::AllocVar { init: val, ty: dty });
                    self.mutable_slots.insert(*var, slot);
                    self.var_map.insert(*var, slot); // slot is the "latest" reference
                } else {
                    self.var_map.insert(*var, val);
                }
            }
            IrStmtKind::Assign { var, value, .. } => {
                let val = self.lower_expr_into(value, ops);
                if let Some(slot) = self.mutable_slots.get(var).copied() {
                    // Mutable: store to existing slot
                    self.emit(ops, DialectType::Unit, OpKind::StoreVar { slot, value: val });
                } else {
                    self.var_map.insert(*var, val);
                }
            }
            IrStmtKind::Expr { expr } => {
                self.lower_expr_into(expr, ops);
            }
            IrStmtKind::Guard { cond, else_, .. } => {
                let cond_val = self.lower_expr_into(cond, ops);
                let else_block = self.lower_to_block(else_);
                let unit = self.emit(ops, DialectType::Unit, OpKind::ConstUnit);
                let then_block = Block {
                    id: self.ids.fresh_block(),
                    args: vec![], ops: vec![],
                    terminator: Terminator::Yield(unit),
                };
                self.emit(ops, DialectType::Unit, OpKind::IfOp {
                    cond: cond_val,
                    then_region: vec![then_block],
                    else_region: vec![else_block],
                });
            }
            _ => {} // IndexAssign, FieldAssign, etc. — TODO
        }
    }

    fn lower_pattern(&mut self, pattern: &IrPattern) -> MatchPattern {
        match pattern {
            IrPattern::Wildcard => MatchPattern::Wildcard,
            IrPattern::Bind { var, .. } => {
                let val = self.ids.fresh_value();
                self.var_map.insert(*var, val);
                MatchPattern::Binding(val)
            }
            IrPattern::Constructor { name, args } => {
                let tag = almide_base::intern::sym(name);
                let bindings = args.iter().map(|p| {
                    if let IrPattern::Bind { var, .. } = p {
                        let val = self.ids.fresh_value();
                        self.var_map.insert(*var, val);
                        val
                    } else {
                        self.ids.fresh_value()
                    }
                }).collect();
                MatchPattern::Variant { tag, bindings }
            }
            IrPattern::Literal { expr } => {
                match &expr.kind {
                    IrExprKind::LitInt { value } => MatchPattern::LitInt(*value),
                    IrExprKind::LitStr { value } => MatchPattern::LitStr(value.clone()),
                    IrExprKind::LitBool { value } => MatchPattern::LitBool(*value),
                    _ => MatchPattern::Wildcard, // non-literal patterns
                }
            }
            IrPattern::Tuple { elements, .. } => {
                MatchPattern::Tuple(elements.iter().map(|p| self.lower_pattern(p)).collect())
            }
            IrPattern::Ok { inner } | IrPattern::Some { inner } => {
                let tag_name = if matches!(pattern, IrPattern::Ok { .. }) { "Ok" } else { "Some" };
                let tag = almide_base::intern::sym(tag_name);
                let inner_pat = self.lower_pattern(inner);
                let binding = match inner_pat {
                    MatchPattern::Binding(v) => v,
                    _ => self.ids.fresh_value(),
                };
                MatchPattern::Variant { tag, bindings: vec![binding] }
            }
            IrPattern::Err { inner } => {
                let tag = almide_base::intern::sym("Err");
                let inner_pat = self.lower_pattern(inner);
                let binding = match inner_pat {
                    MatchPattern::Binding(v) => v,
                    _ => self.ids.fresh_value(),
                };
                MatchPattern::Variant { tag, bindings: vec![binding] }
            }
            IrPattern::None => {
                let tag = almide_base::intern::sym("None");
                MatchPattern::Variant { tag, bindings: vec![] }
            }
            _ => MatchPattern::Wildcard, // Record, List — TODO
        }
    }

    /// Emit an operation, returning its result ValueId.
    fn emit(&mut self, ops: &mut Vec<Operation>, result_ty: DialectType, kind: OpKind) -> ValueId {
        let result = self.ids.fresh_value();
        ops.push(Operation {
            result: Some(result),
            result_ty,
            kind,
        });
        result
    }
}

fn lower_type_decl(td: &IrTypeDecl) -> TypeDeclOp {
    let kind = match &td.kind {
        IrTypeDeclKind::Record { fields } => {
            TypeDeclKind::Record {
                fields: fields.iter().map(|f| (f.name, types::from_ty(&f.ty))).collect(),
            }
        }
        IrTypeDeclKind::Variant { cases, .. } => {
            TypeDeclKind::Variant {
                cases: cases.iter().map(|c| {
                    VariantCase {
                        name: c.name,
                        payload: match &c.kind {
                            IrVariantKind::Unit => vec![],
                            IrVariantKind::Tuple { fields } => fields.iter().map(types::from_ty).collect(),
                            IrVariantKind::Record { fields } => fields.iter().map(|f| types::from_ty(&f.ty)).collect(),
                        },
                    }
                }).collect(),
            }
        }
        IrTypeDeclKind::Alias { target } => TypeDeclKind::Alias(types::from_ty(target)),
    };
    TypeDeclOp { name: td.name, kind }
}
