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
    IrExpr, IrExprKind, IrStmt, IrStmtKind, IrFunction, IrParam,
    IrMatchArm, IrPattern, VarId, VarTable, CallTarget,
};
use almide_lang::types::Ty;

use super::ir::{
    Op, Const, WasmTy, WasmFunc, Local, FuncIdx,
    BinOp as WBinOp, UnOp as WUnOp, LoadKind, StoreKind,
};
use super::layout::{self, LayoutRegistry};
use super::data::DataInterner;
use super::module::SigTable;

/// Lowering context for a single function.
/// Named record type → its fields in declaration order. Lets the engine resolve
/// `Ty::Named("Foo")` to concrete field offsets (the frontend leaves named
/// record types unresolved in the IR).
pub type RecordLayouts = std::collections::HashMap<almide_base::intern::Sym, Vec<(almide_base::intern::Sym, Ty)>>;

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
    /// Shared interner for string literals (data segment).
    pub interner: &'a mut DataInterner,
    /// Shared signature table for call_indirect type indices.
    pub sigs: &'a mut SigTable,
    /// Non-capturing lambdas bound to a `let` (ClosureConversion hoists them);
    /// recorded here so higher-order intrinsics can inline them at the use site.
    pub lambda_binds: std::collections::HashMap<VarId, IrExpr>,
    /// Next scratch local index (for temporaries).
    next_local: u32,
    /// Named record type layouts, for resolving `Ty::Named` field offsets.
    pub record_types: &'a RecordLayouts,
}

impl<'a> LowerCtx<'a> {
    pub fn new(
        params: &[IrParam],
        var_table: &VarTable,
        reg: &'a LayoutRegistry,
        func_idx: &'a dyn Fn(&str) -> Option<FuncIdx>,
        interner: &'a mut DataInterner,
        sigs: &'a mut SigTable,
        record_types: &'a RecordLayouts,
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
            interner,
            sigs,
            lambda_binds: std::collections::HashMap::new(),
            next_local: param_count,
            record_types,
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

    /// Bind a VarId to an existing local (e.g. a lambda parameter aliasing a
    /// loop element). Used by the intrinsic registry's inline-lambda lowering.
    pub fn map_var(&mut self, var: VarId, local: Local) {
        if (var.0 as usize) < self.var_map.len() {
            self.var_map[var.0 as usize] = Some(local);
        }
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
    interner: &mut DataInterner,
    sigs: &mut SigTable,
    record_types: &RecordLayouts,
) -> WasmFunc {
    let mut ctx = LowerCtx::new(&func.params, var_table, reg, func_idx, interner, sigs, record_types);
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
            // Intern into the data segment; the constant is the absolute offset
            // of the String object's header.
            let off = ctx.interner.intern(value);
            vec![Op::Const(Const::I32(off as i32))]
        }

        // ── Variables ──
        IrExprKind::Var { id } => {
            if let Some(local) = ctx.get_var(*id) {
                vec![Op::LocalGet(local)]
            } else if ctx.lambda_binds.contains_key(id) {
                // A bare lambda used as a value (not inlined by a HOF) — the
                // engine has no first-class lambda value yet. Reject honestly.
                vec![Op::Unsupported("lambda-value")]
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
            } else if matches!(op, almide_ir::BinOp::ConcatStr) {
                // String concatenation → runtime call (resolved to __string_concat).
                ops.push(Op::StringConcat);
            } else if matches!(op, almide_ir::BinOp::Eq | almide_ir::BinOp::Neq)
                && matches!(left.ty, Ty::String)
            {
                // String deep equality via runtime; Neq inverts the result.
                if let Some(idx) = (ctx.func_idx)("__string_eq") {
                    ops.push(Op::Call { idx, pops: 2, pushes: 1 });
                    if matches!(op, almide_ir::BinOp::Neq) {
                        ops.push(Op::UnOp(WUnOp::I32Eqz));
                    }
                }
            } else if matches!(op, almide_ir::BinOp::ConcatList) {
                // List concatenation: pass the element width so the runtime can
                // copy raw bytes. (left.ty is List[T].) Reject (→ legacy) on an
                // unresolved element rather than guessing a stride.
                match list_element_ty(&left.ty) {
                    Some(t) if !t.is_unresolved() => {
                        ops.push(Op::Const(Const::I32(wasm_byte_size(&t))));
                        if let Some(idx) = (ctx.func_idx)("__list_concat") {
                            ops.push(Op::Call { idx, pops: 3, pushes: 1 });
                        } else {
                            ops.push(Op::Unreachable);
                        }
                    }
                    _ => ops.push(Op::Unsupported("concat-list-unresolved-elem")),
                }
            }
            // Other None cases (ModFloat, Pow, matrix) have no runtime yet; they
            // leave the stack unbalanced and are caught by the verifier until
            // their runtimes land.
            ops
        }

        // ── Unary operations ──
        IrExprKind::UnOp { op, operand } => {
            // Integer negation has no single WASM instruction — emit `0 - x`.
            if matches!(op, almide_ir::UnOp::NegInt) {
                let mut ops = vec![Op::Const(Const::I64(0))];
                ops.extend(lower_expr(operand, ctx));
                ops.push(Op::BinOp(WBinOp::I64Sub));
                return ops;
            }
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
                let then_ops = lower_expr(then, ctx);
                let else_ops = lower_expr(else_, ctx);
                ops.push(Op::If { ty: ty_to_wasm(&expr.ty), then: then_ops, else_: else_ops });
            } else {
                let then_ops = lower_expr_void(then, ctx);
                let else_ops = lower_expr_void(else_, ctx);
                ops.push(Op::IfVoid { then: then_ops, else_: else_ops });
            }
            ops
        }

        // ── Call / TailCall (no WASM tail-call needed for correctness) ──
        IrExprKind::Call { target, args, .. }
        | IrExprKind::TailCall { target, args } => {
            lower_call(target, args, &expr.ty, ctx)
        }

        IrExprKind::RuntimeCall { symbol, args } => {
            // Declarative stdlib dispatch first (the v2 intrinsic registry).
            if let Some(ops) = super::intrinsics::lower_intrinsic(symbol.as_str(), args, &expr.ty, ctx) {
                return ops;
            }
            // Then the engine's own runtime intrinsics by name.
            let mut ops = Vec::new();
            for arg in args {
                ops.extend(lower_expr(arg, ctx));
            }
            let pops = args.len() as u8;
            let pushes = if matches!(expr.ty, Ty::Unit) { 0 } else { 1 };
            if let Some(idx) = (ctx.func_idx)(symbol.as_str()) {
                ops.push(Op::Call { idx, pops, pushes });
            } else {
                // Stdlib intrinsic not implemented in v2 yet — reject (→ legacy).
                if std::env::var_os("ALMIDE_WASM_V2_DUMP").is_some() {
                    eprintln!("[v2-missing-intrinsic] {}", symbol.as_str());
                }
                ops.push(Op::Unsupported("runtime-call"));
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

            // alloc(data size = LEN(4) + CAP(4) + len * elem_size).
            // The 8-byte alloc header (size/rc) is added by __alloc itself.
            let total = 8 + len * elem_size;
            ops.push(Op::Const(Const::I32(total)));
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

        // ── Empty map → __map_new (Map[Int,Int] only) ──
        IrExprKind::EmptyMap => {
            if super::intrinsics::map_supported(&expr.ty) {
                if let Some(idx) = (ctx.func_idx)("__map_new") {
                    return vec![Op::Call { idx, pops: 0, pushes: 1 }];
                }
            }
            vec![Op::Unsupported("EmptyMap")]
        }

        // ── Tuple literal ──
        IrExprKind::Tuple { elements } => {
            let mut ops = Vec::new();
            let tuple = ctx.alloc_local(WasmTy::I32);
            let mut total_size = 0i32;
            for elem in elements {
                total_size += wasm_byte_size(&elem.ty);
            }
            ops.push(Op::Const(Const::I32(total_size)));
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
            // Fields are laid out in the record TYPE's canonical order at their
            // natural byte widths. Both construction and Member read offsets
            // from the same type, so they always agree. If the type can't be
            // resolved to offsets, reject (→ legacy) rather than collapsing
            // every field to offset 0.
            let Some(total) = record_total_size(&expr.ty, ctx.record_types) else {
                return vec![Op::Unsupported("record-unresolved-type")];
            };
            let mut ops = Vec::new();
            let rec = ctx.alloc_local(WasmTy::I32);
            ops.push(Op::Const(Const::I32(total)));
            ops.push(Op::Alloc);
            ops.push(Op::LocalSet(rec));

            for (name, value) in fields.iter() {
                let Some((off, _)) = record_field_offset(&expr.ty, name.as_str(), ctx.record_types) else {
                    return vec![Op::Unsupported("record-field-missing")];
                };
                ops.push(Op::LocalGet(rec));
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
            // A guessed offset is silent memory corruption, so reject (→ legacy)
            // when the record type can't be resolved to concrete field offsets.
            let Some((offset, _)) = record_field_offset(&object.ty, field.as_str(), ctx.record_types) else {
                return vec![Op::Unsupported("member-unresolved-record")];
            };
            let mut ops = lower_expr(object, ctx);
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
            // Element offset = sum of the byte sizes of all preceding elements
            // (tuples pack at natural width). Reject (→ legacy) if the tuple
            // type is unresolved rather than guess a stride.
            let Ty::Tuple(elems) = &object.ty else {
                return vec![Op::Unsupported("tuple-index-unresolved")];
            };
            let offset = elems.iter().take(*index).map(|t| wasm_byte_size(t)).sum::<i32>();
            let mut ops = lower_expr(object, ctx);
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
            // Each interpolated expression must become a String. We support
            // String parts directly and Int parts via __int_to_string. Other
            // types (Float, Bool, …) have no to_string runtime yet, so the
            // whole interpolation is rejected cleanly rather than concatenating
            // a non-pointer as if it were one.
            let unsupported = parts.iter().any(|p| {
                matches!(p, almide_ir::IrStringPart::Expr { expr }
                    if !matches!(expr.ty, Ty::String | Ty::Int))
            });
            if unsupported {
                return vec![Op::StringInterp { parts: Vec::new() }];
            }
            if parts.is_empty() {
                let off = ctx.interner.intern("");
                return vec![Op::Const(Const::I32(off as i32))];
            }
            // Left-associative concat of all (string-valued) parts.
            let mut ops = Vec::new();
            for (i, part) in parts.iter().enumerate() {
                match part {
                    almide_ir::IrStringPart::Lit { value } => {
                        let off = ctx.interner.intern(value);
                        ops.push(Op::Const(Const::I32(off as i32)));
                    }
                    almide_ir::IrStringPart::Expr { expr: e } => {
                        ops.extend(lower_expr(e, ctx));
                        // Convert Int → String. (String parts are already pointers.)
                        if matches!(e.ty, Ty::Int) {
                            if let Some(idx) = (ctx.func_idx)("__int_to_string") {
                                ops.push(Op::Call { idx, pops: 1, pushes: 1 });
                            }
                        }
                    }
                }
                if i > 0 {
                    ops.push(Op::StringConcat);
                }
            }
            ops
        }

        // ── Option/Result constructors ──
        IrExprKind::OptionSome { expr: inner } => {
            // Option layout: [tag:i32=1][payload]
            let mut ops = Vec::new();
            let opt = ctx.alloc_local(WasmTy::I32);
            ops.push(Op::Const(Const::I32(12))); // tag(4) + payload(4..8) + alloc header
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
            ops.push(Op::Const(Const::I32(12)));
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
            ops.push(Op::Const(Const::I32(12)));
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
            ops.push(Op::Const(Const::I32(12)));
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
            // Check tag. Option: Some = tag != 0 → payload. Result: Ok = tag 0 →
            // payload, so invert the tag test so `then` is always the payload arm.
            ops.push(Op::Load(LoadKind::I32)); // load tag
            let is_result = {
                use almide_lang::types::constructor::TypeConstructorId as TC;
                matches!(&inner.ty, Ty::Applied(TC::Result, _))
            };
            if is_result {
                ops.push(Op::UnOp(WUnOp::I32Eqz)); // tag == 0 (Ok) → take payload
            }
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
            ops.push(Op::If { ty: ty_to_wasm(&expr.ty), then: then_ops, else_: else_ops });
            ops
        }

        // ── Try (`?`): needs error propagation (early return on Err), which
        //    isn't modeled yet — reject so legacy handles it correctly. ──
        IrExprKind::Try { expr: _inner } => vec![Op::Unsupported("Try")],

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
                ops.push(Op::Const(Const::I32(env_size)));
                ops.push(Op::Alloc);
                ops.push(Op::LocalSet(env_ptr));

                // Store captures into env. Each slot is 8 bytes; the value is
                // written at its natural width to match EnvLoad's load width.
                for (i, (vid, cap_ty)) in captures.iter().enumerate() {
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
                    let store_kind = match ty_to_wasm(cap_ty) {
                        WasmTy::I64 => StoreKind::I64,
                        WasmTy::F64 => StoreKind::F64,
                        _ => StoreKind::I32,
                    };
                    ops.push(Op::Store(store_kind));
                }
            }

            // Allocate closure pair
            ops.push(Op::Const(Const::I32(pair_size)));
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

        // ── Lambda ──
        // ClosureConversion should have lifted these into ClosureCreate; a raw
        // Lambda reaching codegen is not something we can lower.
        IrExprKind::Lambda { .. } => vec![Op::Unsupported("Lambda")],

        // ── Range ──
        IrExprKind::Range { start, end, inclusive } => {
            // __range(start, end, inclusive) builds the List[Int].
            let mut ops = lower_expr(start, ctx);
            ops.extend(lower_expr(end, ctx));
            ops.push(Op::Const(Const::I32(if *inclusive { 1 } else { 0 })));
            if let Some(idx) = (ctx.func_idx)("__range") {
                ops.push(Op::Call { idx, pops: 3, pushes: 1 });
            } else {
                ops.push(Op::Unreachable);
            }
            ops
        }

        // ── MapLiteral → __map_new + __map_set per entry ──
        IrExprKind::MapLiteral { entries } => {
            use almide_lang::types::constructor::TypeConstructorId as TC;
            let (key_ty, val_ty) = match &expr.ty {
                Ty::Applied(TC::Map, a) if a.len() == 2 && super::intrinsics::map_supported(&expr.ty) =>
                    (a[0].clone(), a[1].clone()),
                _ => return vec![Op::Unsupported("MapLiteral")],
            };
            let (new_idx, set_idx) = match ((ctx.func_idx)("__map_new"), (ctx.func_idx)("__map_set")) {
                (Some(n), Some(s)) => (n, s),
                _ => return vec![Op::Unsupported("MapLiteral")],
            };
            let kind = if matches!(key_ty, Ty::String) { 1 } else { 0 };
            let widen = |ops: &mut Vec<Op>, ty: &Ty| {
                if matches!(ty_to_wasm(ty), WasmTy::I32) { ops.push(Op::UnOp(WUnOp::I64ExtendI32U)); }
            };
            let m = ctx.alloc_local(WasmTy::I32);
            let mut ops = vec![Op::Call { idx: new_idx, pops: 0, pushes: 1 }, Op::LocalSet(m)];
            for (k, v) in entries {
                ops.push(Op::LocalGet(m));
                ops.extend(lower_expr(k, ctx)); widen(&mut ops, &key_ty);
                ops.extend(lower_expr(v, ctx)); widen(&mut ops, &val_ty);
                ops.push(Op::Const(Const::I32(kind)));
                ops.push(Op::Call { idx: set_idx, pops: 4, pushes: 1 });
                ops.push(Op::LocalSet(m));
            }
            ops.push(Op::LocalGet(m));
            ops
        }

        // ── SpreadRecord: { ...base, f: v } ──
        // Spread preserves the record type, so base and result share field
        // offsets: copy the whole base record, then overwrite the named fields.
        IrExprKind::SpreadRecord { base, fields } => {
            let Some(total) = record_total_size(&expr.ty, ctx.record_types) else {
                return vec![Op::Unsupported("SpreadRecord")];
            };
            let mut ops = Vec::new();
            let out = ctx.alloc_local(WasmTy::I32);
            let base_l = ctx.alloc_local(WasmTy::I32);
            ops.extend(lower_expr(base, ctx));
            ops.push(Op::LocalSet(base_l));
            ops.push(Op::Const(Const::I32(total)));
            ops.push(Op::Alloc);
            ops.push(Op::LocalSet(out));
            // memcpy(out, base, total)
            ops.push(Op::LocalGet(out));
            ops.push(Op::LocalGet(base_l));
            ops.push(Op::Const(Const::I32(total)));
            ops.push(Op::MemoryCopy);
            // overwrite the explicitly-spread fields
            for (name, value) in fields.iter() {
                let Some((off, _)) = record_field_offset(&expr.ty, name.as_str(), ctx.record_types) else {
                    return vec![Op::Unsupported("record-field-missing")];
                };
                ops.push(Op::LocalGet(out));
                if off != 0 {
                    ops.push(Op::Const(Const::I32(off)));
                    ops.push(Op::BinOp(WBinOp::I32Add));
                }
                ops.extend(lower_expr(value, ctx));
                let sk = match ty_to_wasm(&value.ty) {
                    WasmTy::I64 => StoreKind::I64,
                    WasmTy::F64 => StoreKind::F64,
                    _ => StoreKind::I32,
                };
                ops.push(Op::Store(sk));
            }
            ops.push(Op::LocalGet(out));
            ops
        }

        // ── MapAccess (Swiss Table lookup not implemented) ──
        IrExprKind::MapAccess { object: _, key: _ } => vec![Op::Unsupported("MapAccess")],

        // ── OptionalChain (None-check + field access not implemented) ──
        IrExprKind::OptionalChain { expr: _inner, field: _ } => vec![Op::Unsupported("OptionalChain")],

        // ── ToOption (Result → Option conversion not implemented) ──
        IrExprKind::ToOption { expr: _inner } => vec![Op::Unsupported("ToOption")],

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

        // ── Remaining unhandled variants ──
        // A Unit-typed unknown produces nothing; anything else would need a
        // real value, so reject (→ legacy fallback) rather than fake a zero.
        _ => {
            if matches!(expr.ty, Ty::Unit) {
                vec![]
            } else {
                vec![Op::Unsupported("unhandled-expr")]
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
            // A non-capturing lambda bound to a `let` has no runtime value — it
            // is inlined at its use site by the higher-order intrinsics. Record
            // it and emit nothing.
            if matches!(value.kind, IrExprKind::Lambda { .. }) {
                ctx.lambda_binds.insert(*var, value.clone());
                return vec![];
            }
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

        // IndexAssign, MapInsert, FieldAssign, BindDestructure, ListSwap, … are
        // not lowered yet. Emit a marker so the builder rejects (→ legacy
        // fallback) instead of silently dropping the side effect.
        _ => vec![Op::Unsupported("unhandled-stmt")],
    }
}

// ── Call lowering ────────────────────────────────────────────────────

fn lower_call(target: &CallTarget, args: &[IrExpr], ret_ty: &Ty, ctx: &mut LowerCtx) -> Vec<Op> {
    let pushes = if matches!(ret_ty, Ty::Unit) { 0 } else { 1 };

    // Computed (closure) calls have a different calling convention — the env
    // pointer must be the first argument — so they build their own op sequence.
    if let CallTarget::Computed { callee } = target {
        return lower_indirect_call(callee, args, ret_ty, pushes, ctx);
    }

    // Stdlib module calls (`list.filter(xs, f)`) route to the intrinsic registry
    // by their `almide_rt_<module>_<fn>` symbol. The registry lowers args itself
    // so higher-order ops can inline their lambda argument.
    if let CallTarget::Module { module, func, .. } = target {
        let symbol = format!("almide_rt_{}_{}", module.as_str(), func.as_str());
        if let Some(ops) = super::intrinsics::lower_intrinsic(&symbol, args, ret_ty, ctx) {
            return ops;
        }
    }

    let mut ops = Vec::new();
    for arg in args {
        ops.extend(lower_expr(arg, ctx));
    }
    let pops = args.len() as u8;

    match target {
        CallTarget::Named { name, .. } => {
            // Prelude print builtins map to the WASI stdout runtime fns.
            let builtin = match name.as_str() {
                "print" => Some("__print"),
                "println" | "eprintln" => Some("__println"),
                _ => None,
            };
            if let Some(rt_name) = builtin {
                if let Some(idx) = (ctx.func_idx)(rt_name) {
                    ops.push(Op::Call { idx, pops, pushes: 0 });
                    return ops;
                }
            }
            if let Some(idx) = (ctx.func_idx)(name.as_str()) {
                ops.push(Op::Call { idx, pops, pushes });
            } else {
                // Unresolved function (e.g. an unimplemented stdlib fn) — reject
                // so the build falls back to the legacy emitter rather than
                // trapping at runtime.
                ops.push(Op::Unsupported("unresolved-fn"));
            }
        }
        CallTarget::Module { module, func: method, .. } => {
            let full_name = format!("{}_{}", module.as_str(), method.as_str());
            if let Some(idx) = (ctx.func_idx)(&full_name) {
                ops.push(Op::Call { idx, pops, pushes });
            } else if let Some(idx) = (ctx.func_idx)(method.as_str()) {
                ops.push(Op::Call { idx, pops, pushes });
            } else {
                // Stdlib dispatch (e.g. string.len, list.push) is not in v2 yet.
                ops.push(Op::Unsupported("stdlib-call"));
            }
        }
        _ => ops.push(Op::Unsupported("unresolved-call")),
    }
    ops
}

/// Lower an indirect (closure) call.
///
/// Closure pair layout `[table_idx @ 0][env_ptr @ 4]`; lifted lambdas have the
/// convention `(env_ptr, params...) -> ret`. So we push `env_ptr`, then the
/// arguments, then the table index, and `call_indirect` with that signature.
fn lower_indirect_call(
    callee: &IrExpr, args: &[IrExpr], ret_ty: &Ty, pushes: u8, ctx: &mut LowerCtx,
) -> Vec<Op> {
    let cl = ctx.alloc_local(WasmTy::I32);
    let mut ops = lower_expr(callee, ctx);
    ops.push(Op::LocalSet(cl));

    // env_ptr = closure[4]
    ops.push(Op::LocalGet(cl));
    ops.push(Op::Const(Const::I32(4)));
    ops.push(Op::BinOp(WBinOp::I32Add));
    ops.push(Op::Load(LoadKind::I32));

    // arguments
    for arg in args {
        ops.extend(lower_expr(arg, ctx));
    }

    // table_idx = closure[0]
    ops.push(Op::LocalGet(cl));
    ops.push(Op::Load(LoadKind::I32));

    // Signature: (env_ptr: i32, arg types...) -> ret
    let mut sig_params = vec![WasmTy::I32];
    sig_params.extend(args.iter().map(|a| ty_to_wasm(&a.ty)));
    let sig_results = if matches!(ret_ty, Ty::Unit) { vec![] } else { vec![ty_to_wasm(ret_ty)] };
    let sig = ctx.sigs.intern(sig_params, sig_results);

    // call_indirect pops the table index + every parameter (env + args).
    let pops = (args.len() as u8) + 1 /* env */ + 1 /* table idx */;
    ops.push(Op::CallIndirect { sig, pops, pushes });
    ops
}

// ── BinOp / UnOp mapping ─────────────────────────────────────────────

fn lower_binop(op: &almide_ir::BinOp, left_ty: &Ty) -> Option<WBinOp> {
    use almide_ir::BinOp as IrOp;

    // Comparisons are type-dispatched on the operand type: Int → i64,
    // Float → f64, everything else (Bool, pointers) → i32. String/composite
    // deep equality is not a simple binop and falls through to a runtime call.
    let is_float = matches!(left_ty, Ty::Float);
    let is_int = matches!(left_ty, Ty::Int);

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

        IrOp::Eq if matches!(left_ty, Ty::String) => return None, // deep eq → runtime
        IrOp::Neq if matches!(left_ty, Ty::String) => return None,
        IrOp::Eq => if is_int { WBinOp::I64Eq } else if is_float { WBinOp::F64Eq } else { WBinOp::I32Eq },
        IrOp::Neq => if is_int { WBinOp::I64Ne } else if is_float { WBinOp::F64Ne } else { WBinOp::I32Ne },
        IrOp::Lt => if is_float { WBinOp::F64Lt } else if is_int { WBinOp::I64LtS } else { WBinOp::I32LtS },
        IrOp::Lte => if is_float { WBinOp::F64Le } else if is_int { WBinOp::I64LeS } else { WBinOp::I32LeS },
        IrOp::Gt => if is_float { WBinOp::F64Gt } else if is_int { WBinOp::I64GtS } else { WBinOp::I32GtS },
        IrOp::Gte => if is_float { WBinOp::F64Ge } else if is_int { WBinOp::I64GeS } else { WBinOp::I32GeS },

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
        // NegInt is lowered to `0 - x` by the caller (no single WASM i64 neg).
        IrOp::NegInt => return None,
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
        ops.extend(lower_match_arms_with_result(arms, subj_local, &subject.ty, result_ty, ctx));
    } else {
        ops.extend(lower_match_arms_void(arms, subj_local, &subject.ty, ctx));
    }
    ops
}

fn lower_match_arms_with_result(arms: &[IrMatchArm], subj: Local, subj_ty: &Ty, result_ty: &Ty, ctx: &mut LowerCtx) -> Vec<Op> {
    if arms.is_empty() {
        return vec![Op::Unreachable];
    }
    if arms.len() == 1 {
        // Last arm (wildcard) — just emit body
        let mut binds = bind_pattern(&arms[0].pattern, subj, subj_ty, ctx);
        binds.extend(lower_expr(&arms[0].body, ctx));
        return binds;
    }

    let arm = &arms[0];
    let rest = &arms[1..];

    let mut ops = Vec::new();
    // Emit condition for this arm's pattern
    ops.extend(pattern_condition(&arm.pattern, subj, subj_ty, ctx));

    let then_ops = {
        let mut binds = bind_pattern(&arm.pattern, subj, subj_ty, ctx);
        binds.extend(lower_expr(&arm.body, ctx));
        binds
    };
    let else_ops = lower_match_arms_with_result(rest, subj, subj_ty, result_ty, ctx);

    ops.push(Op::If { ty: ty_to_wasm(result_ty), then: then_ops, else_: else_ops });
    ops
}

fn lower_match_arms_void(arms: &[IrMatchArm], subj: Local, subj_ty: &Ty, ctx: &mut LowerCtx) -> Vec<Op> {
    if arms.is_empty() {
        return vec![];
    }
    if arms.len() == 1 {
        let mut binds = bind_pattern(&arms[0].pattern, subj, subj_ty, ctx);
        binds.extend(lower_expr_void(&arms[0].body, ctx));
        return binds;
    }

    let arm = &arms[0];
    let rest = &arms[1..];

    let mut ops = Vec::new();
    ops.extend(pattern_condition(&arm.pattern, subj, subj_ty, ctx));

    let then_ops = {
        let mut binds = bind_pattern(&arm.pattern, subj, subj_ty, ctx);
        binds.extend(lower_expr_void(&arm.body, ctx));
        binds
    };
    let else_ops = lower_match_arms_void(rest, subj, subj_ty, ctx);

    ops.push(Op::IfVoid { then: then_ops, else_: else_ops });
    ops
}

/// Emit a condition check for a pattern. Pushes i32 (0 or 1) onto stack.
fn pattern_condition(pattern: &IrPattern, subj: Local, _subj_ty: &Ty, ctx: &mut LowerCtx) -> Vec<Op> {
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
                IrExprKind::LitStr { value } => {
                    // subj == "literal" via __string_eq
                    let off = ctx.interner.intern(value);
                    let mut v = vec![Op::LocalGet(subj), Op::Const(Const::I32(off as i32))];
                    if let Some(idx) = (ctx.func_idx)("__string_eq") {
                        v.push(Op::Call { idx, pops: 2, pushes: 1 });
                    } else {
                        v.push(Op::Const(Const::I32(1)));
                    }
                    v
                }
                _ => vec![Op::Const(Const::I32(1))], // TODO: float comparison
            }
        }
        IrPattern::Constructor { name: _, .. } => {
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
        // Tuple / Record: irrefutable structurally — the conjunction of the
        // refutable sub-patterns (literals, nested variants) at their slots.
        IrPattern::Tuple { .. } | IrPattern::RecordPattern { .. } => {
            let Some(slots) = sub_slots(pattern, _subj_ty, ctx.record_types) else {
                return vec![Op::Unsupported("pattern-unresolved-type")];
            };
            let mut ops = vec![Op::Const(Const::I32(1))];
            for (off, ty, p) in slots {
                if pattern_irrefutable(&p) { continue; }
                let slot = ctx.alloc_local(ty_to_wasm(&ty));
                ops.extend(load_at(subj, off, &ty, slot));
                ops.extend(pattern_condition(&p, slot, &ty, ctx));
                ops.push(Op::BinOp(WBinOp::I32And));
            }
            ops
        }
        // List: a fixed-arity pattern matches only lists of that exact length,
        // plus any refutable element sub-patterns.
        IrPattern::List { elements } => {
            let Some(slots) = sub_slots(pattern, _subj_ty, ctx.record_types) else {
                return vec![Op::Unsupported("pattern-unresolved-type")];
            };
            let mut ops = vec![
                Op::LocalGet(subj), Op::Load(LoadKind::I32),
                Op::Const(Const::I32(elements.len() as i32)), Op::BinOp(WBinOp::I32Eq),
            ];
            for (off, ty, p) in slots {
                if pattern_irrefutable(&p) { continue; }
                let slot = ctx.alloc_local(ty_to_wasm(&ty));
                ops.extend(load_at(subj, off, &ty, slot));
                ops.extend(pattern_condition(&p, slot, &ty, ctx));
                ops.push(Op::BinOp(WBinOp::I32And));
            }
            ops
        }
    }
}

/// A pattern that always matches (only binds or ignores), so it imposes no
/// runtime condition.
fn pattern_irrefutable(p: &IrPattern) -> bool {
    match p {
        IrPattern::Wildcard | IrPattern::Bind { .. } => true,
        IrPattern::Tuple { elements } => elements.iter().all(pattern_irrefutable),
        IrPattern::RecordPattern { fields, .. } =>
            fields.iter().all(|f| f.pattern.as_ref().map_or(true, pattern_irrefutable)),
        _ => false, // List (length), Literal, Some/None/Ok/Err, Constructor
    }
}

/// Best-effort element/field type for a sub-pattern when the subject type is
/// imprecise: a Bind/Literal carries its own type; composites are pointers.
fn pattern_fallback_ty(p: &IrPattern) -> Ty {
    match p {
        IrPattern::Bind { ty, .. } => ty.clone(),
        IrPattern::Literal { expr } => expr.ty.clone(),
        IrPattern::Tuple { .. } | IrPattern::RecordPattern { .. } | IrPattern::List { .. }
        | IrPattern::Some { .. } | IrPattern::None | IrPattern::Ok { .. }
        | IrPattern::Err { .. } | IrPattern::Constructor { .. } => Ty::Unknown,
        IrPattern::Wildcard => Ty::Unknown,
    }
}

/// The (byte offset, element type, sub-pattern) triples of a composite pattern,
/// using `subj_ty` as the authority for layout (so offsets match construction).
/// The (offset, type, sub-pattern) slots of a composite pattern. Returns `None`
/// when a slot's type can't be resolved to a concrete WASM layout (so the
/// caller rejects → legacy rather than binding at a guessed offset/width).
fn sub_slots(pattern: &IrPattern, subj_ty: &Ty, recs: &RecordLayouts) -> Option<Vec<(i32, Ty, IrPattern)>> {
    match pattern {
        IrPattern::Tuple { elements } => {
            let elem_tys: Vec<Ty> = match subj_ty {
                Ty::Tuple(ts) if ts.len() == elements.len() => ts.clone(),
                _ => elements.iter().map(pattern_fallback_ty).collect(),
            };
            let mut off = 0i32;
            let mut out = Vec::with_capacity(elements.len());
            for (i, p) in elements.iter().enumerate() {
                let ty = elem_tys.get(i)?.clone();
                if ty.is_unresolved() { return None; }
                out.push((off, ty.clone(), p.clone()));
                off += wasm_byte_size(&ty);
            }
            Some(out)
        }
        IrPattern::RecordPattern { fields, .. } => {
            let mut out = Vec::new();
            for f in fields {
                let Some(p) = &f.pattern else { continue };
                // A named field absent from the type, or an unresolved field
                // type, means we can't place it — reject.
                let (off, fty) = record_field_offset(subj_ty, &f.name, recs)?;
                if fty.is_unresolved() { return None; }
                out.push((off, fty, p.clone()));
            }
            Some(out)
        }
        IrPattern::List { elements } => {
            let ety = list_element_ty(subj_ty).filter(|t| !t.is_unresolved())?;
            let es = wasm_byte_size(&ety);
            Some(elements.iter().enumerate().map(|(i, p)| (8 + i as i32 * es, ety.clone(), p.clone())).collect())
        }
        _ => Some(vec![]),
    }
}

/// `subj[offset]` loaded into local `slot` at `ty`'s natural width.
fn load_at(base: Local, offset: i32, ty: &Ty, slot: Local) -> Vec<Op> {
    let wt = ty_to_wasm(ty);
    let mut ops = vec![Op::LocalGet(base)];
    if offset != 0 {
        ops.push(Op::Const(Const::I32(offset)));
        ops.push(Op::BinOp(WBinOp::I32Add));
    }
    ops.push(Op::Load(load_kind_of(wt)));
    ops.push(Op::LocalSet(slot));
    ops
}

/// Bind pattern variables, returning ops that load payloads/sub-components into
/// their locals (emitted at the start of the matched arm body). Recurses through
/// Tuple / Record / List / Some / Ok / Err / Constructor.
fn bind_pattern(pattern: &IrPattern, subj: Local, subj_ty: &Ty, ctx: &mut LowerCtx) -> Vec<Op> {
    match pattern {
        IrPattern::Bind { var, .. } => {
            // The bound variable aliases the subject local directly — no copy.
            if (var.0 as usize) < ctx.var_map.len() {
                ctx.var_map[var.0 as usize] = Some(subj);
            }
            vec![]
        }
        // Tagged-union payloads live at offset 4 (after the i32 tag).
        IrPattern::Some { inner } => {
            let pty = option_arg(subj_ty).unwrap_or_else(|| pattern_fallback_ty(inner));
            destructure_one(inner, subj, 4, &pty, ctx)
        }
        IrPattern::Ok { inner } => {
            let pty = result_args(subj_ty).map(|(o, _)| o).unwrap_or_else(|| pattern_fallback_ty(inner));
            destructure_one(inner, subj, 4, &pty, ctx)
        }
        IrPattern::Err { inner } => {
            let pty = result_args(subj_ty).map(|(_, e)| e).unwrap_or_else(|| pattern_fallback_ty(inner));
            destructure_one(inner, subj, 4, &pty, ctx)
        }
        IrPattern::Constructor { args, .. } => {
            // Constructor args are packed after the tag at their natural widths.
            let mut ops = Vec::new();
            let mut off = 4i32;
            for arg in args.iter() {
                let ty = pattern_fallback_ty(arg);
                ops.extend(destructure_one(arg, subj, off, &ty, ctx));
                off += wasm_byte_size(&ty);
            }
            ops
        }
        IrPattern::Tuple { .. } | IrPattern::RecordPattern { .. } | IrPattern::List { .. } => {
            let Some(slots) = sub_slots(pattern, subj_ty, ctx.record_types) else {
                return vec![Op::Unsupported("pattern-unresolved-type")];
            };
            let mut ops = Vec::new();
            for (off, ty, p) in slots {
                ops.extend(destructure_one(&p, subj, off, &ty, ctx));
            }
            ops
        }
        _ => vec![], // Wildcard, Literal, None
    }
}

/// Load `base[offset]` into a fresh local, then bind `pattern` against it
/// (aliasing for a leaf Bind, recursing for a nested composite/variant).
fn destructure_one(pattern: &IrPattern, base: Local, offset: i32, ty: &Ty, ctx: &mut LowerCtx) -> Vec<Op> {
    match pattern {
        IrPattern::Wildcard | IrPattern::Literal { .. } | IrPattern::None => vec![],
        _ => {
            let slot = ctx.alloc_local(ty_to_wasm(ty));
            let mut ops = load_at(base, offset, ty, slot);
            ops.extend(bind_pattern(pattern, slot, ty, ctx));
            ops
        }
    }
}

/// `Option[T]` → T.
fn option_arg(ty: &Ty) -> Option<Ty> {
    use almide_lang::types::constructor::TypeConstructorId as TC;
    match ty {
        Ty::Applied(TC::Option, a) if !a.is_empty() => Some(a[0].clone()),
        _ => None,
    }
}

/// `Result[T, E]` → (T, E).
fn result_args(ty: &Ty) -> Option<(Ty, Ty)> {
    use almide_lang::types::constructor::TypeConstructorId as TC;
    match ty {
        Ty::Applied(TC::Result, a) if a.len() == 2 => Some((a[0].clone(), a[1].clone())),
        _ => None,
    }
}

// ── Helpers ──────────────────────────────────────────────────────────

/// Byte size for a WASM value type.
pub(super) fn wasm_byte_size(ty: &Ty) -> i32 {
    match ty_to_wasm(ty) {
        WasmTy::I64 | WasmTy::F64 => 8,
        _ => 4,
    }
}

/// The element type of a `List[T]` / `Set[T]` (None if not such a type).
pub(super) fn list_element_ty(ty: &Ty) -> Option<Ty> {
    use almide_lang::types::constructor::TypeConstructorId as TC;
    match ty {
        Ty::Applied(TC::List, args) | Ty::Applied(TC::Set, args) if !args.is_empty() => {
            Some(args[0].clone())
        }
        _ => None,
    }
}

/// LoadKind matching a WASM value type's natural width.
pub(super) fn load_kind_of(wt: WasmTy) -> LoadKind {
    match wt {
        WasmTy::I64 => LoadKind::I64,
        WasmTy::F64 => LoadKind::F64,
        WasmTy::F32 => LoadKind::F32,
        WasmTy::I32 => LoadKind::I32,
    }
}

/// Byte offset (and type) of a named field within a record type, computed by
/// summing the natural widths of preceding fields in declared order. Returns
/// None if the type is not a record or the field is absent.
/// Fields of a record type, resolving `Ty::Named` via the program's type decls.
fn record_fields<'a>(ty: &'a Ty, recs: &'a RecordLayouts) -> Option<&'a [(almide_base::intern::Sym, Ty)]> {
    match ty {
        Ty::Record { fields } | Ty::OpenRecord { fields } => Some(fields.as_slice()),
        Ty::Named(n, _) => recs.get(n).map(|v| v.as_slice()),
        _ => None,
    }
}

fn record_field_offset(ty: &Ty, name: &str, recs: &RecordLayouts) -> Option<(i32, Ty)> {
    let fields = record_fields(ty, recs)?;
    let mut off = 0i32;
    for (fname, fty) in fields {
        if fname.as_str() == name {
            return Some((off, fty.clone()));
        }
        off += wasm_byte_size(fty);
    }
    None
}

/// Total byte size of a record type (sum of field widths), if known.
fn record_total_size(ty: &Ty, recs: &RecordLayouts) -> Option<i32> {
    record_fields(ty, recs).map(|fs| fs.iter().map(|(_, t)| wasm_byte_size(t)).sum())
}

// ── ForIn lowering ───────────────────────────────────────────────────

fn lower_for_in(var: VarId, iterable: &IrExpr, body: &[IrStmt], ctx: &mut LowerCtx) -> Vec<Op> {
    let mut ops = Vec::new();
    // Element type from the iterable: List[T] / Set[T] → T. A List/Set with an
    // unresolved element would mis-stride, so reject (→ legacy). `None` (Range
    // and other Int-yielding iterables) keeps the Int default.
    let elem_ty = match list_element_ty(&iterable.ty) {
        Some(t) if t.is_unresolved() => return vec![Op::Unsupported("forin-unresolved-elem")],
        Some(t) => t,
        None => Ty::Int,
    };
    let elem_wasm = ty_to_wasm(&elem_ty);
    let elem_size = wasm_byte_size(&elem_ty);
    let (elem_load, elem_store) = (load_kind_of(elem_wasm), ()); // store unused here
    let _ = elem_store;

    let list = ctx.alloc_local(WasmTy::I32);
    let idx = ctx.alloc_local(WasmTy::I32);
    let elem = ctx.alloc_local(elem_wasm);

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
    loop_body.push(Op::Const(Const::I32(elem_size)));
    loop_body.push(Op::BinOp(WBinOp::I32Mul));
    loop_body.push(Op::BinOp(WBinOp::I32Add));
    loop_body.push(Op::Load(elem_load));
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
    use super::super::ir::verify_func_stack;
    use super::super::data::DataInterner;
    use super::super::module::SigTable;
    use almide_ir::{IrVisibility, Mutability};
    use almide_base::intern::sym;

    fn empty_var_table() -> VarTable {
        VarTable::new()
    }

    fn no_func_idx(_name: &str) -> Option<FuncIdx> { None }

    fn interner() -> DataInterner { DataInterner::new(16) }

    #[test]
    fn lower_lit_int() {
        let reg = LayoutRegistry::new();
        let vt = empty_var_table();
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
        let wasm_func = lower_function(&func, &vt, &reg, &no_func_idx, &mut interner(), &mut SigTable::new(), &RecordLayouts::new());
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
        let wasm_func = lower_function(&func, &vt, &reg, &no_func_idx, &mut interner(), &mut SigTable::new(), &RecordLayouts::new());
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
        let wasm_func = lower_function(&func, &vt, &reg, &no_func_idx, &mut interner(), &mut SigTable::new(), &RecordLayouts::new());
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
        let wasm_func = lower_function(&func, &vt, &reg, &no_func_idx, &mut interner(), &mut SigTable::new(), &RecordLayouts::new());
        assert!(verify_func_stack(&wasm_func).is_ok(), "{:?}", verify_func_stack(&wasm_func));
    }
}
