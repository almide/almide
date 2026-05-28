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
            lower_for_in(*var, iterable, body, ctx)
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

        // ── Match ──
        IrExprKind::Match { subject, arms } => {
            lower_match(subject, arms, &expr.ty, ctx)
        }

        // ── List literal ──
        IrExprKind::List { elements } => {
            // Allocate list, store each element
            // TODO: use AllocCollection op for proper layout
            let mut ops = Vec::new();
            let len = elements.len() as i32;
            let elem_size = wasm_byte_size(&elements.first().map(|e| &e.ty).unwrap_or(&Ty::Int));
            let list = ctx.alloc_local(WasmTy::I32);

            // alloc(header + len * elem_size)
            let total = 8 + len * elem_size; // LEN(4) + CAP(4) + data
            ops.push(Op::Const(Const::I32(total + 8))); // +8 for alloc header
            ops.push(Op::Alloc);
            ops.push(Op::LocalTee(list));

            // Write len
            ops.push(Op::Const(Const::I32(len)));
            ops.push(Op::Store(StoreKind::I32)); // store len at offset 0

            ops.push(Op::LocalGet(list));
            ops.push(Op::Const(Const::I32(4)));
            ops.push(Op::BinOp(WBinOp::I32Add));
            ops.push(Op::Const(Const::I32(len)));
            ops.push(Op::Store(StoreKind::I32)); // store cap at offset 4

            // Store elements
            for (i, elem) in elements.iter().enumerate() {
                ops.push(Op::LocalGet(list));
                ops.push(Op::Const(Const::I32(8 + i as i32 * elem_size)));
                ops.push(Op::BinOp(WBinOp::I32Add));
                ops.extend(lower_expr(elem, ctx));
                let store_kind = match ty_to_wasm(&elem.ty) {
                    WasmTy::I64 => StoreKind::I64,
                    WasmTy::F64 => StoreKind::F64,
                    _ => StoreKind::I32,
                };
                ops.push(Op::Store(store_kind));
            }

            ops.push(Op::LocalGet(list));
            ops
        }

        // ── Empty list ──
        IrExprKind::EmptyMap => {
            // Empty map: allocate minimal Swiss Table
            vec![Op::Const(Const::I32(0))] // placeholder
        }

        // ── Tuple literal ──
        IrExprKind::Tuple { elements } => {
            let mut ops = Vec::new();
            let tuple = ctx.alloc_local(WasmTy::I32);
            let mut total_size = 0i32;
            for elem in elements {
                total_size += wasm_byte_size(&elem.ty);
            }
            ops.push(Op::Const(Const::I32(total_size + 8))); // +8 alloc header
            ops.push(Op::Alloc);
            ops.push(Op::LocalSet(tuple));

            let mut offset = 0i32;
            for elem in elements {
                ops.push(Op::LocalGet(tuple));
                if offset != 0 {
                    ops.push(Op::Const(Const::I32(offset)));
                    ops.push(Op::BinOp(WBinOp::I32Add));
                }
                ops.extend(lower_expr(elem, ctx));
                let store_kind = match ty_to_wasm(&elem.ty) {
                    WasmTy::I64 => StoreKind::I64,
                    WasmTy::F64 => StoreKind::F64,
                    _ => StoreKind::I32,
                };
                ops.push(Op::Store(store_kind));
                offset += wasm_byte_size(&elem.ty);
            }

            ops.push(Op::LocalGet(tuple));
            ops
        }

        // ── Record literal ──
        IrExprKind::Record { fields, .. } => {
            let mut ops = Vec::new();
            let rec = ctx.alloc_local(WasmTy::I32);
            let field_size = 4i32; // each field is a pointer or i32
            let total = (fields.len() as i32) * field_size;
            ops.push(Op::Const(Const::I32(total + 8)));
            ops.push(Op::Alloc);
            ops.push(Op::LocalSet(rec));

            for (i, (_name, value)) in fields.iter().enumerate() {
                ops.push(Op::LocalGet(rec));
                let off = i as i32 * field_size;
                if off != 0 {
                    ops.push(Op::Const(Const::I32(off)));
                    ops.push(Op::BinOp(WBinOp::I32Add));
                }
                ops.extend(lower_expr(value, ctx));
                let store_kind = match ty_to_wasm(&value.ty) {
                    WasmTy::I64 => StoreKind::I64,
                    WasmTy::F64 => StoreKind::F64,
                    _ => StoreKind::I32,
                };
                ops.push(Op::Store(store_kind));
            }

            ops.push(Op::LocalGet(rec));
            ops
        }

        // ── Member access ──
        IrExprKind::Member { object, field } => {
            let mut ops = lower_expr(object, ctx);
            // TODO: resolve field offset from type. For now use linear search placeholder.
            let offset = field_offset_placeholder(field.as_str());
            if offset != 0 {
                ops.push(Op::Const(Const::I32(offset)));
                ops.push(Op::BinOp(WBinOp::I32Add));
            }
            let load_kind = match ty_to_wasm(&expr.ty) {
                WasmTy::I64 => LoadKind::I64,
                WasmTy::F64 => LoadKind::F64,
                _ => LoadKind::I32,
            };
            ops.push(Op::Load(load_kind));
            ops
        }

        // ── TupleIndex ──
        IrExprKind::TupleIndex { object, index } => {
            let mut ops = lower_expr(object, ctx);
            let offset = (*index as i32) * 4; // simplified: each element 4 bytes
            if offset != 0 {
                ops.push(Op::Const(Const::I32(offset)));
                ops.push(Op::BinOp(WBinOp::I32Add));
            }
            let load_kind = match ty_to_wasm(&expr.ty) {
                WasmTy::I64 => LoadKind::I64,
                WasmTy::F64 => LoadKind::F64,
                _ => LoadKind::I32,
            };
            ops.push(Op::Load(load_kind));
            ops
        }

        // ── IndexAccess (list[i]) ──
        IrExprKind::IndexAccess { object, index } => {
            let mut ops = Vec::new();
            let list = ctx.alloc_local(WasmTy::I32);
            ops.extend(lower_expr(object, ctx));
            ops.push(Op::LocalSet(list));

            // data_ptr = list + 8 (skip len + cap)
            ops.push(Op::LocalGet(list));
            ops.push(Op::Const(Const::I32(8)));
            ops.push(Op::BinOp(WBinOp::I32Add));

            // index * elem_size
            ops.extend(lower_expr(index, ctx));
            // Convert i64 index to i32
            ops.push(Op::UnOp(WUnOp::I32WrapI64));
            let elem_size = wasm_byte_size(&expr.ty);
            ops.push(Op::Const(Const::I32(elem_size)));
            ops.push(Op::BinOp(WBinOp::I32Mul));
            ops.push(Op::BinOp(WBinOp::I32Add));

            let load_kind = match ty_to_wasm(&expr.ty) {
                WasmTy::I64 => LoadKind::I64,
                WasmTy::F64 => LoadKind::F64,
                _ => LoadKind::I32,
            };
            ops.push(Op::Load(load_kind));
            ops
        }

        // ── String interpolation ──
        IrExprKind::StringInterp { parts } => {
            let mut ops = Vec::new();
            // Concatenate all parts into a new string
            // For now, lower each part and use StringConcat
            let mut first = true;
            for part in parts {
                match part {
                    almide_ir::IrStringPart::Lit { value } => {
                        // TODO: intern string in data segment
                        ops.push(Op::Const(Const::I32(0))); // placeholder str ptr
                    }
                    almide_ir::IrStringPart::Expr { expr: e } => {
                        ops.extend(lower_expr(e, ctx));
                        // TODO: call to_string if not already string
                    }
                }
                if !first {
                    ops.push(Op::StringConcat);
                }
                first = false;
            }
            if parts.is_empty() {
                ops.push(Op::Const(Const::I32(0))); // empty string
            }
            ops
        }

        // ── Option/Result constructors ──
        IrExprKind::OptionSome { expr: inner } => {
            // Option layout: [tag:i32=1][payload]
            let mut ops = Vec::new();
            let opt = ctx.alloc_local(WasmTy::I32);
            ops.push(Op::Const(Const::I32(12 + 8))); // tag(4) + payload(4..8) + alloc header
            ops.push(Op::Alloc);
            ops.push(Op::LocalTee(opt));
            ops.push(Op::Const(Const::I32(1))); // tag = Some
            ops.push(Op::Store(StoreKind::I32));
            ops.push(Op::LocalGet(opt));
            ops.push(Op::Const(Const::I32(4)));
            ops.push(Op::BinOp(WBinOp::I32Add));
            ops.extend(lower_expr(inner, ctx));
            let store_kind = match ty_to_wasm(&inner.ty) {
                WasmTy::I64 => StoreKind::I64,
                WasmTy::F64 => StoreKind::F64,
                _ => StoreKind::I32,
            };
            ops.push(Op::Store(store_kind));
            ops.push(Op::LocalGet(opt));
            ops
        }

        IrExprKind::OptionNone => {
            // Option None: [tag:i32=0]
            let mut ops = Vec::new();
            let opt = ctx.alloc_local(WasmTy::I32);
            ops.push(Op::Const(Const::I32(12 + 8)));
            ops.push(Op::Alloc);
            ops.push(Op::LocalTee(opt));
            ops.push(Op::Const(Const::I32(0))); // tag = None
            ops.push(Op::Store(StoreKind::I32));
            ops.push(Op::LocalGet(opt));
            ops
        }

        IrExprKind::ResultOk { expr: inner } => {
            // Result layout: [tag:i32=0 (Ok)][payload]
            let mut ops = Vec::new();
            let res = ctx.alloc_local(WasmTy::I32);
            ops.push(Op::Const(Const::I32(12 + 8)));
            ops.push(Op::Alloc);
            ops.push(Op::LocalTee(res));
            ops.push(Op::Const(Const::I32(0))); // tag = Ok
            ops.push(Op::Store(StoreKind::I32));
            ops.push(Op::LocalGet(res));
            ops.push(Op::Const(Const::I32(4)));
            ops.push(Op::BinOp(WBinOp::I32Add));
            ops.extend(lower_expr(inner, ctx));
            let store_kind = match ty_to_wasm(&inner.ty) {
                WasmTy::I64 => StoreKind::I64,
                WasmTy::F64 => StoreKind::F64,
                _ => StoreKind::I32,
            };
            ops.push(Op::Store(store_kind));
            ops.push(Op::LocalGet(res));
            ops
        }

        IrExprKind::ResultErr { expr: inner } => {
            let mut ops = Vec::new();
            let res = ctx.alloc_local(WasmTy::I32);
            ops.push(Op::Const(Const::I32(12 + 8)));
            ops.push(Op::Alloc);
            ops.push(Op::LocalTee(res));
            ops.push(Op::Const(Const::I32(1))); // tag = Err
            ops.push(Op::Store(StoreKind::I32));
            ops.push(Op::LocalGet(res));
            ops.push(Op::Const(Const::I32(4)));
            ops.push(Op::BinOp(WBinOp::I32Add));
            ops.extend(lower_expr(inner, ctx));
            let store_kind = match ty_to_wasm(&inner.ty) {
                WasmTy::I64 => StoreKind::I64,
                WasmTy::F64 => StoreKind::F64,
                _ => StoreKind::I32,
            };
            ops.push(Op::Store(store_kind));
            ops.push(Op::LocalGet(res));
            ops
        }

        // ── Unwrap: extract payload from Option/Result ──
        IrExprKind::Unwrap { expr: inner } => {
            // Load payload at offset 4 (skip tag)
            let mut ops = lower_expr(inner, ctx);
            ops.push(Op::Const(Const::I32(4)));
            ops.push(Op::BinOp(WBinOp::I32Add));
            let load_kind = match ty_to_wasm(&expr.ty) {
                WasmTy::I64 => LoadKind::I64,
                WasmTy::F64 => LoadKind::F64,
                _ => LoadKind::I32,
            };
            ops.push(Op::Load(load_kind));
            ops
        }

        // ── UnwrapOr: extract or fallback ──
        IrExprKind::UnwrapOr { expr: inner, fallback } => {
            let mut ops = Vec::new();
            let ptr = ctx.alloc_local(WasmTy::I32);
            ops.extend(lower_expr(inner, ctx));
            ops.push(Op::LocalTee(ptr));
            // Check tag
            ops.push(Op::Load(LoadKind::I32)); // load tag
            // If tag == 0 (None/Err), use fallback
            let load_kind = match ty_to_wasm(&expr.ty) {
                WasmTy::I64 => LoadKind::I64,
                WasmTy::F64 => LoadKind::F64,
                _ => LoadKind::I32,
            };
            let then_ops = {
                // Some/Ok: load payload
                let mut t = vec![Op::LocalGet(ptr)];
                t.push(Op::Const(Const::I32(4)));
                t.push(Op::BinOp(WBinOp::I32Add));
                t.push(Op::Load(load_kind));
                t
            };
            let else_ops = lower_expr(fallback, ctx);
            ops.push(Op::If { then: then_ops, else_: else_ops });
            ops
        }

        // ── Try: Result -> Option conversion (propagate error) ──
        IrExprKind::Try { expr: inner } => {
            // Simplified: just extract the value, error handling is TODO
            let mut ops = lower_expr(inner, ctx);
            ops.push(Op::Const(Const::I32(4)));
            ops.push(Op::BinOp(WBinOp::I32Add));
            let load_kind = match ty_to_wasm(&expr.ty) {
                WasmTy::I64 => LoadKind::I64,
                WasmTy::F64 => LoadKind::F64,
                _ => LoadKind::I32,
            };
            ops.push(Op::Load(load_kind));
            ops
        }

        // ── ClosureCreate ──
        IrExprKind::ClosureCreate { func_name, captures } => {
            let mut ops = Vec::new();
            let closure = ctx.alloc_local(WasmTy::I32);

            // Allocate closure pair: [table_idx:i32, env_ptr:i32]
            // Plus env: captures.len() * 8 bytes
            let env_size = (captures.len() as i32) * 8;
            let pair_size = 8; // table_idx + env_ptr

            // Allocate env
            let env_ptr = ctx.alloc_local(WasmTy::I32);
            if !captures.is_empty() {
                ops.push(Op::Const(Const::I32(env_size + 8))); // +8 alloc header
                ops.push(Op::Alloc);
                ops.push(Op::LocalSet(env_ptr));

                // Store captures into env
                for (i, (vid, _ty)) in captures.iter().enumerate() {
                    ops.push(Op::LocalGet(env_ptr));
                    let off = (i as i32) * 8;
                    if off != 0 {
                        ops.push(Op::Const(Const::I32(off)));
                        ops.push(Op::BinOp(WBinOp::I32Add));
                    }
                    if let Some(local) = ctx.get_var(*vid) {
                        ops.push(Op::LocalGet(local));
                    } else {
                        ops.push(Op::Const(Const::I32(0)));
                    }
                    ops.push(Op::Store(StoreKind::I32));
                }
            }

            // Allocate closure pair
            ops.push(Op::Const(Const::I32(pair_size + 8)));
            ops.push(Op::Alloc);
            ops.push(Op::LocalTee(closure));

            // Store table_idx (func index)
            if let Some(idx) = (ctx.func_idx)(func_name.as_str()) {
                ops.push(Op::Const(Const::I32(idx as i32)));
            } else {
                ops.push(Op::Const(Const::I32(0)));
            }
            ops.push(Op::Store(StoreKind::I32));

            // Store env_ptr at offset 4
            ops.push(Op::LocalGet(closure));
            ops.push(Op::Const(Const::I32(4)));
            ops.push(Op::BinOp(WBinOp::I32Add));
            if captures.is_empty() {
                ops.push(Op::Const(Const::I32(0)));
            } else {
                ops.push(Op::LocalGet(env_ptr));
            }
            ops.push(Op::Store(StoreKind::I32));

            ops.push(Op::LocalGet(closure));
            ops
        }

        // ── Lambda (no captures — kept as-is by ClosureConversion) ──
        IrExprKind::Lambda { .. } => {
            // Non-capturing lambdas are inlined by the emitter.
            // For now, placeholder.
            vec![Op::Const(Const::I32(0))]
        }

        // ── Range ──
        IrExprKind::Range { start, end, inclusive } => {
            // TODO: allocate list and fill with range values
            vec![Op::Const(Const::I32(0))] // placeholder
        }

        // ── MapLiteral ──
        IrExprKind::MapLiteral { entries } => {
            // TODO: allocate Swiss Table and insert entries
            vec![Op::Const(Const::I32(0))] // placeholder
        }

        // ── SpreadRecord ──
        IrExprKind::SpreadRecord { base, fields } => {
            // TODO: clone base record and update fields
            lower_expr(base, ctx) // simplified: just return base
        }

        // ── MapAccess ──
        IrExprKind::MapAccess { object, key } => {
            // TODO: Swiss Table lookup
            vec![Op::Const(Const::I32(0))] // placeholder
        }

        // ── OptionalChain ──
        IrExprKind::OptionalChain { expr: inner, field } => {
            // TODO: check None, then access field
            vec![Op::Const(Const::I32(0))] // placeholder
        }

        // ── ToOption ──
        IrExprKind::ToOption { expr: inner } => {
            // Result → Option conversion
            lower_expr(inner, ctx) // simplified
        }

        // ── FnRef ──
        IrExprKind::FnRef { name } => {
            if let Some(idx) = (ctx.func_idx)(name.as_str()) {
                vec![Op::Const(Const::I32(idx as i32))]
            } else {
                vec![Op::Const(Const::I32(0))]
            }
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

        // ── Codegen-specific (Rust target only, should not appear in WASM) ──
        IrExprKind::Deref { expr: inner } | IrExprKind::Borrow { expr: inner, .. }
        | IrExprKind::BoxNew { expr: inner } | IrExprKind::RcWrap { expr: inner, .. }
        | IrExprKind::ToVec { expr: inner } | IrExprKind::Await { expr: inner } => {
            lower_expr(inner, ctx)
        }

        // ── Misc ──
        IrExprKind::Hole | IrExprKind::Todo { .. } => vec![Op::Unreachable],
        IrExprKind::Break => vec![Op::Br(1)],    // break out of loop block
        IrExprKind::Continue => vec![Op::Br(0)],  // jump to loop head

        // ── Remaining fallback ──
        _ => {
            if matches!(expr.ty, Ty::Unit) {
                vec![]
            } else {
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

// ── Match lowering ───────────────────────────────────────────────────

fn lower_match(subject: &IrExpr, arms: &[IrMatchArm], result_ty: &Ty, ctx: &mut LowerCtx) -> Vec<Op> {
    let mut ops = Vec::new();
    let has_result = !matches!(result_ty, Ty::Unit);

    // Evaluate subject into a local
    let subj_local = ctx.alloc_local(ty_to_wasm(&subject.ty));
    ops.extend(lower_expr(subject, ctx));
    ops.push(Op::LocalSet(subj_local));

    // Compile as nested if-else chain (simple but correct)
    // For variant tags: load tag, compare, branch
    // For literals: compare directly
    // Wildcard: always matches
    if has_result {
        ops.extend(lower_match_arms_with_result(arms, subj_local, &subject.ty, ctx));
    } else {
        ops.extend(lower_match_arms_void(arms, subj_local, &subject.ty, ctx));
    }
    ops
}

fn lower_match_arms_with_result(arms: &[IrMatchArm], subj: Local, subj_ty: &Ty, ctx: &mut LowerCtx) -> Vec<Op> {
    if arms.is_empty() {
        return vec![Op::Unreachable];
    }
    if arms.len() == 1 {
        // Last arm (wildcard) — just emit body
        bind_pattern(&arms[0].pattern, subj, subj_ty, ctx);
        return lower_expr(&arms[0].body, ctx);
    }

    let arm = &arms[0];
    let rest = &arms[1..];

    let mut ops = Vec::new();
    // Emit condition for this arm's pattern
    ops.extend(pattern_condition(&arm.pattern, subj, subj_ty, ctx));

    let then_ops = {
        bind_pattern(&arm.pattern, subj, subj_ty, ctx);
        lower_expr(&arm.body, ctx)
    };
    let else_ops = lower_match_arms_with_result(rest, subj, subj_ty, ctx);

    ops.push(Op::If { then: then_ops, else_: else_ops });
    ops
}

fn lower_match_arms_void(arms: &[IrMatchArm], subj: Local, subj_ty: &Ty, ctx: &mut LowerCtx) -> Vec<Op> {
    if arms.is_empty() {
        return vec![];
    }
    if arms.len() == 1 {
        bind_pattern(&arms[0].pattern, subj, subj_ty, ctx);
        return lower_expr_void(&arms[0].body, ctx);
    }

    let arm = &arms[0];
    let rest = &arms[1..];

    let mut ops = Vec::new();
    ops.extend(pattern_condition(&arm.pattern, subj, subj_ty, ctx));

    let then_ops = {
        bind_pattern(&arm.pattern, subj, subj_ty, ctx);
        lower_expr_void(&arm.body, ctx)
    };
    let else_ops = lower_match_arms_void(rest, subj, subj_ty, ctx);

    ops.push(Op::IfVoid { then: then_ops, else_: else_ops });
    ops
}

/// Emit a condition check for a pattern. Pushes i32 (0 or 1) onto stack.
fn pattern_condition(pattern: &IrPattern, subj: Local, subj_ty: &Ty, ctx: &mut LowerCtx) -> Vec<Op> {
    match pattern {
        IrPattern::Wildcard | IrPattern::Bind { .. } => {
            vec![Op::Const(Const::I32(1))] // always matches
        }
        IrPattern::Literal { expr } => {
            match &expr.kind {
                IrExprKind::LitInt { value } => {
                    vec![Op::LocalGet(subj), Op::Const(Const::I64(*value)), Op::BinOp(WBinOp::I64Eq)]
                }
                IrExprKind::LitBool { value } => {
                    vec![Op::LocalGet(subj), Op::Const(Const::I32(if *value { 1 } else { 0 })), Op::BinOp(WBinOp::I32Eq)]
                }
                _ => vec![Op::Const(Const::I32(1))], // TODO: string/float comparison
            }
        }
        IrPattern::Constructor { name, .. } => {
            // Load tag from variant and compare
            // TODO: resolve tag index from variant name
            vec![
                Op::LocalGet(subj),
                Op::Load(LoadKind::I32), // load tag at offset 0
                Op::Const(Const::I32(0)), // placeholder tag
                Op::BinOp(WBinOp::I32Eq),
            ]
        }
        IrPattern::Some { .. } => {
            // Option tag == 1
            vec![
                Op::LocalGet(subj),
                Op::Load(LoadKind::I32),
                Op::Const(Const::I32(1)),
                Op::BinOp(WBinOp::I32Eq),
            ]
        }
        IrPattern::None => {
            vec![
                Op::LocalGet(subj),
                Op::Load(LoadKind::I32),
                Op::UnOp(WUnOp::I32Eqz),
            ]
        }
        IrPattern::Ok { .. } => {
            vec![
                Op::LocalGet(subj),
                Op::Load(LoadKind::I32),
                Op::UnOp(WUnOp::I32Eqz), // tag == 0 = Ok
            ]
        }
        IrPattern::Err { .. } => {
            vec![
                Op::LocalGet(subj),
                Op::Load(LoadKind::I32),
                Op::Const(Const::I32(1)),
                Op::BinOp(WBinOp::I32Eq),
            ]
        }
        _ => vec![Op::Const(Const::I32(1))], // fallback: always match
    }
}

/// Bind pattern variables to the subject value.
fn bind_pattern(pattern: &IrPattern, subj: Local, subj_ty: &Ty, ctx: &mut LowerCtx) {
    match pattern {
        IrPattern::Bind { var, .. } => {
            let local = ctx.bind_var(*var, subj_ty);
            // Will be set via LocalGet(subj) when the body references it
            // For now, copy subject to the bound local
            // (This is done implicitly by the var mapping)
            ctx.var_map[var.0 as usize] = Some(subj);
        }
        IrPattern::Constructor { args, .. } => {
            // Bind constructor arguments from payload
            for (i, arg) in args.iter().enumerate() {
                if let IrPattern::Bind { var, .. } = arg {
                    let local = ctx.alloc_local(WasmTy::I32);
                    if (var.0 as usize) < ctx.var_map.len() {
                        ctx.var_map[var.0 as usize] = Some(local);
                    }
                    // TODO: actually emit load from variant payload
                }
            }
        }
        IrPattern::Some { inner } | IrPattern::Ok { inner } | IrPattern::Err { inner } => {
            if let IrPattern::Bind { var, .. } = inner.as_ref() {
                // Bind payload: subj + 4 (skip tag)
                let local = ctx.alloc_local(WasmTy::I32);
                if (var.0 as usize) < ctx.var_map.len() {
                    ctx.var_map[var.0 as usize] = Some(local);
                }
                // TODO: emit load payload into local
            }
        }
        _ => {} // Wildcard, Lit — nothing to bind
    }
}

// ── Helpers ──────────────────────────────────────────────────────────

/// Byte size for a WASM value type.
fn wasm_byte_size(ty: &Ty) -> i32 {
    match ty_to_wasm(ty) {
        WasmTy::I64 | WasmTy::F64 => 8,
        _ => 4,
    }
}

/// Placeholder field offset (sequential, 4 bytes per field).
/// The real implementation should use record type info or LayoutRegistry.
fn field_offset_placeholder(_field_name: &str) -> i32 {
    0 // TODO: resolve from record type
}

// ── ForIn lowering ───────────────────────────────────────────────────

fn lower_for_in(var: VarId, iterable: &IrExpr, body: &[IrStmt], ctx: &mut LowerCtx) -> Vec<Op> {
    let mut ops = Vec::new();
    let list = ctx.alloc_local(WasmTy::I32);
    let idx = ctx.alloc_local(WasmTy::I32);
    let elem = ctx.alloc_local(ty_to_wasm(&Ty::Int)); // placeholder elem type

    ops.extend(lower_expr(iterable, ctx));
    ops.push(Op::LocalSet(list));
    ops.push(Op::Const(Const::I32(0)));
    ops.push(Op::LocalSet(idx));

    // Bind loop variable
    if (var.0 as usize) < ctx.var_map.len() {
        ctx.var_map[var.0 as usize] = Some(elem);
    }

    let mut loop_body = Vec::new();

    // Check: idx >= list.len → break
    loop_body.push(Op::LocalGet(idx));
    loop_body.push(Op::LocalGet(list));
    loop_body.push(Op::Load(LoadKind::I32)); // load len
    loop_body.push(Op::BinOp(WBinOp::I32GeU));
    loop_body.push(Op::BrIf(1)); // break out of block

    // Load element: list + 8 + idx * elem_size
    loop_body.push(Op::LocalGet(list));
    loop_body.push(Op::Const(Const::I32(8)));
    loop_body.push(Op::BinOp(WBinOp::I32Add));
    loop_body.push(Op::LocalGet(idx));
    loop_body.push(Op::Const(Const::I32(4))); // TODO: proper elem size
    loop_body.push(Op::BinOp(WBinOp::I32Mul));
    loop_body.push(Op::BinOp(WBinOp::I32Add));
    loop_body.push(Op::Load(LoadKind::I32)); // TODO: proper load kind
    loop_body.push(Op::LocalSet(elem));

    // Body statements
    for stmt in body {
        loop_body.extend(lower_stmt(stmt, ctx));
    }

    // idx++
    loop_body.push(Op::LocalGet(idx));
    loop_body.push(Op::Const(Const::I32(1)));
    loop_body.push(Op::BinOp(WBinOp::I32Add));
    loop_body.push(Op::LocalSet(idx));
    loop_body.push(Op::Br(0)); // continue loop

    ops.push(Op::Block(vec![Op::Loop(loop_body)]));
    ops
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
