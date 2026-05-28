//! Lowering: AlmideIR → WasmIR.
//!
//! Translates typed IR expressions and statements into stack-machine
//! WasmIR ops. The output is verified by the stack-effect checker
//! before emission to wasm-encoder.
//!
//! This module replaces the hand-written emit_wasm/*.rs files.
//! All layout access goes through LayoutRegistry. All stack effects
//! are declarative. No raw wasm-encoder calls.

use almide_ir::{
    self, IrExpr, IrExprKind, IrStmt, IrStmtKind, IrFunction, IrParam, IrStringPart,
    IrMatchArm, IrPattern, IrVisibility, VarId, VarTable, Mutability, CallTarget, DefId,
};
use almide_lang::types::Ty;
use almide_base::intern::Sym;

use super::ir::{
    self as wir, Op, Const, WasmTy, WasmFunc, Local, FuncIdx, SigIdx,
    BinOp as WBinOp, UnOp as WUnOp, LoadKind, StoreKind, StringPart,
    StackEffect, verify_func_stack,
};
use super::layout::{self, LayoutId, FieldId, LayoutRegistry};

/// Lowering context for a single function.
pub struct LowerCtx<'a> {
    /// Maps VarId → WASM local index.
    var_map: Vec<Option<Local>>,
    /// WASM locals: (index, type). Grows as we allocate.
    locals: Vec<WasmTy>,
    /// Number of params (param locals come first).
    param_count: u32,
    /// Layout registry for memory access.
    pub reg: &'a LayoutRegistry,
    /// Function index lookup: name → FuncIdx.
    /// Provided by the emitter's function registration phase.
    pub func_idx: &'a dyn Fn(&str) -> Option<FuncIdx>,
    /// Next scratch local index (for temporaries).
    next_local: u32,
}

impl<'a> LowerCtx<'a> {
    pub fn new(
        params: &[IrParam],
        var_table: &VarTable,
        reg: &'a LayoutRegistry,
        func_idx: &'a dyn Fn(&str) -> Option<FuncIdx>,
    ) -> Self {
        let mut var_map = vec![None; var_table.len()];
        let mut locals = Vec::new();

        // Map params to WASM locals 0..N-1
        for (i, param) in params.iter().enumerate() {
            let wasm_ty = ty_to_wasm(&param.ty);
            locals.push(wasm_ty);
            if (param.var.0 as usize) < var_map.len() {
                var_map[param.var.0 as usize] = Some(i as Local);
            }
        }

        let param_count = params.len() as u32;

        LowerCtx {
            var_map,
            locals,
            param_count,
            reg,
            func_idx,
            next_local: param_count,
        }
    }

    /// Allocate a new WASM local, returns its index.
    pub fn alloc_local(&mut self, ty: WasmTy) -> Local {
        let idx = self.locals.len() as Local;
        self.locals.push(ty);
        self.next_local = idx + 1;
        idx
    }

    /// Bind a VarId to a WASM local.
    pub fn bind_var(&mut self, var: VarId, ty: &Ty) -> Local {
        let wasm_ty = ty_to_wasm(ty);
        let local = self.alloc_local(wasm_ty);
        if (var.0 as usize) < self.var_map.len() {
            self.var_map[var.0 as usize] = Some(local);
        }
        local
    }

    /// Look up the WASM local for a VarId.
    pub fn get_var(&self, var: VarId) -> Option<Local> {
        self.var_map.get(var.0 as usize).copied().flatten()
    }

    /// Get the non-param locals (for the WASM function definition).
    pub fn non_param_locals(&self) -> Vec<WasmTy> {
        self.locals[self.param_count as usize..].to_vec()
    }
}

// ── Type mapping ─────────────────────────────────────────────────────

/// Map Almide type to WASM value type.
pub fn ty_to_wasm(ty: &Ty) -> WasmTy {
    match ty {
        Ty::Int => WasmTy::I64,
        Ty::Float => WasmTy::F64,
        Ty::Bool => WasmTy::I32,
        Ty::Unit => WasmTy::I32,
        // Heap types are i32 pointers in linear memory
        Ty::String | Ty::Applied(_, _) | Ty::Record { .. } | Ty::Named(_, _)
        | Ty::Fn { .. } | Ty::Unknown | Ty::OpenRecord { .. } | Ty::Tuple(_) => WasmTy::I32,
        _ => WasmTy::I32, // fallback
    }
}

/// Map Almide type to WASM ValType (for wasm-encoder signatures).
pub fn ty_to_valtype(ty: &Ty) -> wasm_encoder::ValType {
    match ty_to_wasm(ty) {
        WasmTy::I32 => wasm_encoder::ValType::I32,
        WasmTy::I64 => wasm_encoder::ValType::I64,
        WasmTy::F32 => wasm_encoder::ValType::F32,
        WasmTy::F64 => wasm_encoder::ValType::F64,
    }
}

// ── Function lowering ────────────────────────────────────────────────

/// Lower an entire IrFunction to WasmIR.
pub fn lower_function(
    func: &IrFunction,
    var_table: &VarTable,
    reg: &LayoutRegistry,
    func_idx: &dyn Fn(&str) -> Option<FuncIdx>,
) -> WasmFunc {
    let mut ctx = LowerCtx::new(&func.params, var_table, reg, func_idx);
    let has_result = !matches!(func.ret_ty, Ty::Unit);
    let body = lower_expr(&func.body, &mut ctx);

    // If void function and body produces a value, add Drop
    let body = if !has_result && expr_produces_value(&func.body) {
        let mut ops = body;
        ops.push(Op::Drop);
        ops
    } else {
        body
    };

    let results = if has_result {
        vec![ty_to_wasm(&func.ret_ty)]
    } else {
        vec![]
    };

    let params: Vec<WasmTy> = func.params.iter().map(|p| ty_to_wasm(&p.ty)).collect();

    WasmFunc {
        name: func.name.as_str().to_string(),
        params,
        results,
        locals: ctx.non_param_locals(),
        body,
    }
}

// ── Expression lowering ──────────────────────────────────────────────

/// Lower an IrExpr to a sequence of WasmIR ops.
/// The ops, when executed, leave the expression's value on the WASM stack
/// (or nothing for Unit-typed expressions).
pub fn lower_expr(expr: &IrExpr, ctx: &mut LowerCtx) -> Vec<Op> {
    match &expr.kind {
        // ── Literals ──
        IrExprKind::LitInt { value } => vec![Op::Const(Const::I64(*value))],
        IrExprKind::LitFloat { value } => vec![Op::Const(Const::F64(*value))],
        IrExprKind::LitBool { value } => vec![Op::Const(Const::I32(if *value { 1 } else { 0 }))],
        IrExprKind::Unit => vec![],  // Unit produces nothing

        IrExprKind::LitStr { value } => {
            // String literals are stored in the data segment.
            // For now, emit a placeholder that the emitter resolves.
            // TODO: proper data segment interning
            vec![Op::Const(Const::I32(0))] // placeholder ptr
        }

        // ── Variables ──
        IrExprKind::Var { id } => {
            if let Some(local) = ctx.get_var(*id) {
                vec![Op::LocalGet(local)]
            } else {
                vec![Op::Const(Const::I32(0))] // unresolved var fallback
            }
        }

        IrExprKind::EnvLoad { env_var, index } => {
            let mut ops = Vec::new();
            if let Some(local) = ctx.get_var(*env_var) {
                ops.push(Op::LocalGet(local));
            } else {
                ops.push(Op::LocalGet(0)); // env_ptr is always local 0 in lifted fns
            }
            // Load from env at offset = index * 8
            let offset = (*index) * 8;
            ops.push(Op::Const(Const::I32(offset as i32)));
            ops.push(Op::BinOp(WBinOp::I32Add));
            let load_kind = match ty_to_wasm(&expr.ty) {
                WasmTy::I64 => LoadKind::I64,
                WasmTy::F64 => LoadKind::F64,
                _ => LoadKind::I32,
            };
            ops.push(Op::Load(load_kind));
            ops
        }

        // ── Binary operations ──
        IrExprKind::BinOp { op, left, right } => {
            let mut ops = lower_expr(left, ctx);
            ops.extend(lower_expr(right, ctx));
            if let Some(wasm_op) = lower_binop(op, &left.ty) {
                ops.push(Op::BinOp(wasm_op));
            }
            ops
        }

        // ── Unary operations ──
        IrExprKind::UnOp { op, operand } => {
            let mut ops = lower_expr(operand, ctx);
            if let Some(wasm_op) = lower_unop(op, &operand.ty) {
                ops.push(Op::UnOp(wasm_op));
            }
            ops
        }

        // ── For-in loop ──
        IrExprKind::ForIn { var, iterable, body, .. } => {
            let mut ops = Vec::new();
            // TODO: proper list/map iteration via ListForEach/MapForEach
            ops
        }

        // ── While loop ──
        IrExprKind::While { cond, body } => {
            let mut ops = Vec::new();
            let mut loop_body = lower_expr(cond, ctx);
            loop_body.push(Op::UnOp(WUnOp::I32Eqz));
            loop_body.push(Op::BrIf(1)); // break out of outer block
            for stmt in body {
                loop_body.extend(lower_stmt(stmt, ctx));
            }
            loop_body.push(Op::Br(0)); // continue loop
            ops.push(Op::Block(vec![Op::Loop(loop_body)]));
            ops
        }

        // ── Block ──
        IrExprKind::Block { stmts, expr: tail } => {
            let mut ops = Vec::new();
            for stmt in stmts {
                ops.extend(lower_stmt(stmt, ctx));
            }
            if let Some(tail) = tail {
                ops.extend(lower_expr(tail, ctx));
            }
            ops
        }

        // ── If/else ──
        IrExprKind::If { cond, then, else_ } => {
            let mut ops = lower_expr(cond, ctx);
            let has_result = !matches!(expr.ty, Ty::Unit);

            if has_result {
                // Coerce bool condition to i32 if needed
                let then_ops = lower_expr(then, ctx);
                let else_ops = lower_expr(else_, ctx);
                ops.push(Op::If { then: then_ops, else_: else_ops });
            } else {
                let then_ops = lower_expr_void(then, ctx);
                let else_ops = lower_expr_void(else_, ctx);
                ops.push(Op::IfVoid { then: then_ops, else_: else_ops });
            }
            ops
        }

        // ── Call ──
        IrExprKind::Call { target, args, .. } => {
            lower_call(target, args, &expr.ty, ctx)
        }

        IrExprKind::RuntimeCall { symbol, args } => {
            let mut ops = Vec::new();
            for arg in args {
                ops.extend(lower_expr(arg, ctx));
            }
            let pops = args.len() as u8;
            let pushes = if matches!(expr.ty, Ty::Unit) { 0 } else { 1 };
            if let Some(idx) = (ctx.func_idx)(symbol.as_str()) {
                ops.push(Op::Call { idx, pops, pushes });
            } else {
                // Unknown runtime function — emit unreachable as placeholder
                ops.push(Op::Unreachable);
            }
            ops
        }

        // ── Perceus RC ──
        IrExprKind::Clone { expr: inner } => {
            // Clone = RcInc + return same pointer
            let mut ops = lower_expr(inner, ctx);
            let tmp = ctx.alloc_local(WasmTy::I32);
            ops.push(Op::LocalTee(tmp));
            ops.push(Op::RcInc);
            ops.push(Op::LocalGet(tmp));
            ops
        }

        // ── Fallback for unimplemented expressions ──
        _ => {
            // TODO: implement remaining IrExprKind variants
            if matches!(expr.ty, Ty::Unit) {
                vec![]
            } else {
                // Push a zero as placeholder
                vec![Op::Const(Const::I32(0))]
            }
        }
    }
}

/// Lower an expression in void context (discard result if any).
fn lower_expr_void(expr: &IrExpr, ctx: &mut LowerCtx) -> Vec<Op> {
    let ops = lower_expr(expr, ctx);
    if expr_produces_value(expr) && !ops.is_empty() {
        let mut result = ops;
        result.push(Op::Drop);
        result
    } else {
        ops
    }
}

/// Does this expression produce a value on the stack?
fn expr_produces_value(expr: &IrExpr) -> bool {
    !matches!(expr.ty, Ty::Unit)
}

// ── Statement lowering ───────────────────────────────────────────────

fn lower_stmt(stmt: &IrStmt, ctx: &mut LowerCtx) -> Vec<Op> {
    match &stmt.kind {
        IrStmtKind::Bind { var, ty, value, .. } => {
            let local = ctx.bind_var(*var, ty);
            let mut ops = lower_expr(value, ctx);
            if expr_produces_value(value) {
                ops.push(Op::LocalSet(local));
            }
            ops
        }

        IrStmtKind::Assign { var, value } => {
            let mut ops = lower_expr(value, ctx);
            if let Some(local) = ctx.get_var(*var) {
                if expr_produces_value(value) {
                    ops.push(Op::LocalSet(local));
                }
            }
            ops
        }

        IrStmtKind::Expr { expr } => {
            lower_expr_void(expr, ctx)
        }

        IrStmtKind::RcInc { var } => {
            if let Some(local) = ctx.get_var(*var) {
                vec![Op::LocalGet(local), Op::RcInc]
            } else {
                vec![]
            }
        }

        IrStmtKind::RcDec { var } => {
            if let Some(local) = ctx.get_var(*var) {
                // TODO: determine layout from var type for proper child dec
                vec![Op::LocalGet(local), Op::RcDec { layout: layout::ALLOC_HEADER }]
            } else {
                vec![]
            }
        }

        IrStmtKind::Guard { cond, else_ } => {
            let mut ops = lower_expr(cond, ctx);
            // Guard: if cond is false (0), execute else_ (which typically returns/exits)
            ops.push(Op::UnOp(WUnOp::I32Eqz)); // invert: 0 → 1 (trigger else)
            let else_ops = lower_expr_void(else_, ctx);
            ops.push(Op::IfVoid { then: else_ops, else_: vec![] });
            ops
        }

        IrStmtKind::Comment { .. } => vec![],

        _ => {
            // TODO: IndexAssign, MapInsert, FieldAssign, BindDestructure, ListSwap, etc.
            vec![]
        }
    }
}

// ── Call lowering ────────────────────────────────────────────────────

fn lower_call(target: &CallTarget, args: &[IrExpr], ret_ty: &Ty, ctx: &mut LowerCtx) -> Vec<Op> {
    let mut ops = Vec::new();

    // Lower arguments
    for arg in args {
        ops.extend(lower_expr(arg, ctx));
    }

    let pops = args.len() as u8;
    let pushes = if matches!(ret_ty, Ty::Unit) { 0 } else { 1 };

    match target {
        CallTarget::Named { name, .. } => {
            if let Some(idx) = (ctx.func_idx)(name.as_str()) {
                ops.push(Op::Call { idx, pops, pushes });
            } else {
                ops.push(Op::Unreachable);
            }
        }
        CallTarget::Module { module, func: method, .. } => {
            let full_name = format!("{}_{}", module.as_str(), method.as_str());
            if let Some(idx) = (ctx.func_idx)(&full_name) {
                ops.push(Op::Call { idx, pops, pushes });
            } else if let Some(idx) = (ctx.func_idx)(method.as_str()) {
                ops.push(Op::Call { idx, pops, pushes });
            } else {
                ops.push(Op::Unreachable);
            }
        }
        CallTarget::Computed { callee } => {
            // Indirect call through closure
            let mut callee_ops = lower_expr(callee, ctx);
            ops.extend(callee_ops);
            // TODO: extract table_idx and env_ptr from closure pair
            ops.push(Op::Unreachable); // placeholder
        }
        _ => {
            ops.push(Op::Unreachable);
        }
    }

    ops
}

// ── BinOp / UnOp mapping ─────────────────────────────────────────────

fn lower_binop(op: &almide_ir::BinOp, _left_ty: &Ty) -> Option<WBinOp> {
    use almide_ir::BinOp as IrOp;

    Some(match op {
        IrOp::AddInt => WBinOp::I64Add,
        IrOp::AddFloat => WBinOp::F64Add,
        IrOp::SubInt => WBinOp::I64Sub,
        IrOp::SubFloat => WBinOp::F64Sub,
        IrOp::MulInt => WBinOp::I64Mul,
        IrOp::MulFloat => WBinOp::F64Mul,
        IrOp::DivInt => WBinOp::I64DivS,
        IrOp::DivFloat => WBinOp::F64Div,
        IrOp::ModInt => WBinOp::I64RemS,

        IrOp::Eq => WBinOp::I32Eq,  // TODO: type-dispatch for deep eq
        IrOp::Neq => WBinOp::I32Ne,
        IrOp::Lt => WBinOp::I64LtS,  // TODO: float dispatch
        IrOp::Lte => WBinOp::I64LeS,
        IrOp::Gt => WBinOp::I64GtS,
        IrOp::Gte => WBinOp::I64GeS,

        IrOp::And => WBinOp::I32And,
        IrOp::Or => WBinOp::I32Or,

        // String/list concat, matrix ops → runtime call (not a simple binop)
        IrOp::ConcatStr | IrOp::ConcatList
        | IrOp::ModFloat | IrOp::PowInt | IrOp::PowFloat
        | IrOp::MulMatrix | IrOp::AddMatrix | IrOp::SubMatrix | IrOp::ScaleMatrix
            => return None,
    })
}

fn lower_unop(op: &almide_ir::UnOp, _operand_ty: &Ty) -> Option<WUnOp> {
    use almide_ir::UnOp as IrOp;

    Some(match op {
        IrOp::NegInt => WUnOp::I64ExtendI32S, // TODO: proper i64 negate (0 - x)
        IrOp::NegFloat => WUnOp::F64Neg,
        IrOp::Not => WUnOp::I32Eqz,
    })
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use almide_base::intern::sym;

    fn empty_var_table() -> VarTable {
        VarTable::new()
    }

    fn no_func_idx(_name: &str) -> Option<FuncIdx> { None }

    #[test]
    fn lower_lit_int() {
        let reg = LayoutRegistry::new();
        let mut vt = empty_var_table();
        let func = IrFunction {
            name: sym("test"),
            params: vec![],
            ret_ty: Ty::Int,
            body: IrExpr { kind: IrExprKind::LitInt { value: 42 }, ty: Ty::Int, span: None, def_id: None },
            is_effect: false, is_async: false, is_test: false,
            generics: None, extern_attrs: vec![], export_attrs: vec![], attrs: vec![],
            visibility: IrVisibility::Private, doc: None, blank_lines_before: 0,
            def_id: None, mutated_params: vec![], module_origin: None,
        };
        let wasm_func = lower_function(&func, &vt, &reg, &no_func_idx);
        assert_eq!(wasm_func.results, vec![WasmTy::I64]);
        assert!(verify_func_stack(&wasm_func).is_ok(), "stack verification failed: {:?}", verify_func_stack(&wasm_func));
    }

    #[test]
    fn lower_arithmetic() {
        let reg = LayoutRegistry::new();
        let vt = empty_var_table();
        // 1 + 2
        let body = IrExpr {
            kind: IrExprKind::BinOp {
                op: almide_ir::BinOp::AddInt,
                left: Box::new(IrExpr { kind: IrExprKind::LitInt { value: 1 }, ty: Ty::Int, span: None, def_id: None }),
                right: Box::new(IrExpr { kind: IrExprKind::LitInt { value: 2 }, ty: Ty::Int, span: None, def_id: None }),
            },
            ty: Ty::Int, span: None, def_id: None,
        };
        let func = IrFunction {
            name: sym("add"), params: vec![], ret_ty: Ty::Int, body,
            is_effect: false, is_async: false, is_test: false,
            generics: None, extern_attrs: vec![], export_attrs: vec![], attrs: vec![],
            visibility: IrVisibility::Private, doc: None, blank_lines_before: 0,
            def_id: None, mutated_params: vec![], module_origin: None,
        };
        let wasm_func = lower_function(&func, &vt, &reg, &no_func_idx);
        assert!(verify_func_stack(&wasm_func).is_ok());
        // Should be: Const(1), Const(2), I64Add
        assert_eq!(wasm_func.body.len(), 3);
    }

    #[test]
    fn lower_void_block_with_bind() {
        let reg = LayoutRegistry::new();
        let mut vt = VarTable::new();
        let var = vt.alloc(sym("x"), Ty::Int, Mutability::Let, None);

        let body = IrExpr {
            kind: IrExprKind::Block {
                stmts: vec![IrStmt {
                    kind: IrStmtKind::Bind {
                        var, ty: Ty::Int, mutability: Mutability::Let,
                        value: IrExpr { kind: IrExprKind::LitInt { value: 10 }, ty: Ty::Int, span: None, def_id: None },
                    },
                    span: None,
                }],
                expr: None,
            },
            ty: Ty::Unit, span: None, def_id: None,
        };
        let func = IrFunction {
            name: sym("void_fn"), params: vec![], ret_ty: Ty::Unit, body,
            is_effect: false, is_async: false, is_test: false,
            generics: None, extern_attrs: vec![], export_attrs: vec![], attrs: vec![],
            visibility: IrVisibility::Private, doc: None, blank_lines_before: 0,
            def_id: None, mutated_params: vec![], module_origin: None,
        };
        let wasm_func = lower_function(&func, &vt, &reg, &no_func_idx);
        assert!(verify_func_stack(&wasm_func).is_ok(), "{:?}", verify_func_stack(&wasm_func));
        assert!(wasm_func.results.is_empty());
    }

    #[test]
    fn lower_if_else() {
        let reg = LayoutRegistry::new();
        let vt = VarTable::new();
        // if true then 1 else 2
        let body = IrExpr {
            kind: IrExprKind::If {
                cond: Box::new(IrExpr { kind: IrExprKind::LitBool { value: true }, ty: Ty::Bool, span: None, def_id: None }),
                then: Box::new(IrExpr { kind: IrExprKind::LitInt { value: 1 }, ty: Ty::Int, span: None, def_id: None }),
                else_: Box::new(IrExpr { kind: IrExprKind::LitInt { value: 2 }, ty: Ty::Int, span: None, def_id: None }),
            },
            ty: Ty::Int, span: None, def_id: None,
        };
        let func = IrFunction {
            name: sym("if_fn"), params: vec![], ret_ty: Ty::Int, body,
            is_effect: false, is_async: false, is_test: false,
            generics: None, extern_attrs: vec![], export_attrs: vec![], attrs: vec![],
            visibility: IrVisibility::Private, doc: None, blank_lines_before: 0,
            def_id: None, mutated_params: vec![], module_origin: None,
        };
        let wasm_func = lower_function(&func, &vt, &reg, &no_func_idx);
        assert!(verify_func_stack(&wasm_func).is_ok(), "{:?}", verify_func_stack(&wasm_func));
    }
}
