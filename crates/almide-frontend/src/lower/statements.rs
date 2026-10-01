// ── Statement lowering ──────────────────────────────────────────

use almide_lang::ast;
use almide_base::intern::sym;
use almide_ir::*;
use crate::types::{Ty, TypeConstructorId, TypeEnv};
use super::LowerCtx;
use super::expressions::lower_expr;

/// `xs = list.set(xs, i, v)` — the value assigned back to the very list it
/// was built from — writes the slot IN PLACE (#2244). `list.set` returns a
/// fresh copy, so the self-assignment in a loop was quadratic (16k sets over
/// 16k elements: 14 s) and nothing at the call site said so. The rewrite is
/// what the writer could have spelled by hand: the index and value are
/// evaluated once, into temporaries, and the write is the language's own
/// `xs[i] = v` statement guarded by the bounds `list.set` itself tolerates
/// (out of range is a no-op, never an abort). Both targets already carry the
/// in-place statement's value semantics — native through the ownership
/// passes (an alias `let ys = xs` is its own copy), wasm through the rc-gated
/// copy-on-write — so no alias analysis is needed here. `None` for every
/// other assignment.
fn lower_self_list_set(ctx: &mut LowerCtx, var: VarId, value: &IrExpr, span: Option<almide_base::Span>) -> Option<IrStmtKind> {
    let IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. } = &value.kind else { return None };
    if module.as_str() != "list" || func.as_str() != "set" || args.len() != 3 {
        return None;
    }
    let IrExprKind::Var { id } = &args[0].kind else { return None };
    if *id != var || ctx.var_table.get(var).ty.is_map() {
        return None;
    }
    let list_ty = args[0].ty.clone();
    let elem_ty = args[2].ty.clone();
    let mk = |kind: IrExprKind, ty: Ty| IrExpr { kind, ty, span, def_id: None };
    let idx_var = ctx.var_table.alloc(sym("__set_i"), Ty::Int, Mutability::Let, span);
    let val_var = ctx.var_table.alloc(sym("__set_v"), elem_ty.clone(), Mutability::Let, span);
    let idx = |mk: &dyn Fn(IrExprKind, Ty) -> IrExpr| mk(IrExprKind::Var { id: idx_var }, Ty::Int);
    let len = mk(IrExprKind::Call {
        target: CallTarget::Module { module: sym("list"), func: sym("len"), def_id: None },
        args: vec![mk(IrExprKind::Var { id: var }, list_ty)],
        type_args: vec![],
    }, Ty::Int);
    let in_range = mk(IrExprKind::BinOp {
        op: BinOp::And,
        left: Box::new(mk(IrExprKind::BinOp {
            op: BinOp::Gte,
            left: Box::new(idx(&mk)),
            right: Box::new(mk(IrExprKind::LitInt { value: 0 }, Ty::Int)),
        }, Ty::Bool)),
        right: Box::new(mk(IrExprKind::BinOp { op: BinOp::Lt, left: Box::new(idx(&mk)), right: Box::new(len) }, Ty::Bool)),
    }, Ty::Bool);
    let write = IrStmt {
        kind: IrStmtKind::IndexAssign {
            target: var,
            index: idx(&mk),
            value: mk(IrExprKind::Var { id: val_var }, elem_ty.clone()),
        },
        span,
    };
    let guarded = mk(IrExprKind::If {
        cond: Box::new(in_range),
        then: Box::new(mk(IrExprKind::Block { stmts: vec![write], expr: None }, Ty::Unit)),
        else_: Box::new(mk(IrExprKind::Unit, Ty::Unit)),
    }, Ty::Unit);
    let stmts = vec![
        IrStmt { kind: IrStmtKind::Bind { var: idx_var, mutability: Mutability::Let, ty: Ty::Int, value: args[1].clone() }, span },
        IrStmt { kind: IrStmtKind::Bind { var: val_var, mutability: Mutability::Let, ty: elem_ty, value: args[2].clone() }, span },
        IrStmt { kind: IrStmtKind::Expr { expr: guarded }, span },
    ];
    Some(IrStmtKind::Expr { expr: mk(IrExprKind::Block { stmts, expr: None }, Ty::Unit) })
}

pub(super) fn lower_stmt(ctx: &mut LowerCtx, stmt: &ast::Stmt) -> IrStmt {
    let span = stmt_span(stmt);
    let kind = match stmt {
        ast::Stmt::Let { name, ty, value, .. } =>
            lower_bind(ctx, name, ty.as_ref(), value, Mutability::Let, span),
        ast::Stmt::Var { name, ty, value, .. } =>
            lower_bind(ctx, name, ty.as_ref(), value, Mutability::Var, span),
        ast::Stmt::LetDestructure { pattern, value, .. } => {
            let ir_val = lower_expr(ctx, value);
            let ir_pat = lower_pattern(ctx, pattern, &ir_val.ty);
            IrStmtKind::BindDestructure { pattern: ir_pat, value: ir_val }
        }
        ast::Stmt::Assign { name, value, .. } => {
            let mut ir_val = lower_expr(ctx, value);
            let var = ctx.lookup_var(name).unwrap_or(VarId(0));
            // #3185: the target's declared type is the literal's slot, as an
            // annotation is for `let` — `V = 254` into a `UInt8` module `var`
            // emitted `254i64` into the `u8` cell (rustc E0277).
            let target_ty = ctx.var_table.get(var).ty.clone();
            coerce_literal_to_sized(&mut ir_val, &target_ty, ctx.env);
            match lower_self_list_set(ctx, var, &ir_val, span) {
                Some(in_place) => in_place,
                None => IrStmtKind::Assign { var, value: ir_val },
            }
        }
        ast::Stmt::IndexAssign { target, path, index, value, .. } => {
            let mut ir_idx = lower_expr(ctx, index);
            let mut ir_val = lower_expr(ctx, value);
            let write = |ctx: &mut LowerCtx, var: VarId| {
                // #3185: the container's key / element type is the slot.
                let slots = index_write_slots(ctx, var);
                if let Some(k) = &slots.0 { coerce_literal_to_sized(&mut ir_idx, k, ctx.env); }
                if let Some(v) = &slots.1 { coerce_literal_to_sized(&mut ir_val, v, ctx.env); }
                if ctx.var_table.get(var).ty.is_map() {
                    IrStmtKind::MapInsert { target: var, key: ir_idx, value: ir_val }
                } else {
                    IrStmtKind::IndexAssign { target: var, index: ir_idx, value: ir_val }
                }
            };
            lower_place_write(ctx, target, path, span, write)
        }
        ast::Stmt::FieldAssign { target, path, field, value, .. } if !path.is_empty() => {
            let mut ir_val = lower_expr(ctx, value);
            let field = *field;
            lower_place_write(ctx, target, path, span, move |ctx, var| {
                coerce_field_write(ctx, var, field, &mut ir_val);
                IrStmtKind::FieldAssign { target: var, field, value: ir_val }
            })
        }
        ast::Stmt::FieldAssign { target, field, value, .. } => {
            let mut ir_val = lower_expr(ctx, value);
            match ctx.lookup_var(target) {
                Some(var) => {
                    coerce_field_write(ctx, var, *field, &mut ir_val);
                    IrStmtKind::FieldAssign { target: var, field: *field, value: ir_val }
                }
                None => {
                    // `m.x = v` where `m` is a MODULE alias, not a local: an
                    // assignment to a cross-module top-let. Resolve through
                    // the same rule the read path uses (one rule, one place)
                    // — the old VarId(0) fallback rendered garbage like
                    // `NUMS.nums = …` (rustc E0425, #505).
                    let ty = ir_val.ty.clone();
                    if let Some((var, _)) = crate::lower::expressions::module_top_let_var(
                        ctx, sym(target), *field, &ty,
                    ) {
                        let target_ty = ctx.var_table.get(var).ty.clone();
                        coerce_literal_to_sized(&mut ir_val, &target_ty, ctx.env);
                        IrStmtKind::Assign { var, value: ir_val }
                    } else {
                        IrStmtKind::FieldAssign { target: VarId(0), field: *field, value: ir_val }
                    }
                }
            }
        }
        ast::Stmt::Guard { cond, else_, .. } => {
            let ir_cond = lower_expr(ctx, cond);
            let ir_else = lower_expr(ctx, else_);
            IrStmtKind::Guard { cond: ir_cond, else_: ir_else }
        }
        // `guard let` binds for the REST of the block, so the enclosing block lowering
        // (lower_block_stmts) restructures it into a match — it never reaches here.
        ast::Stmt::GuardLet { .. } => {
            unreachable!("guard let is desugared by the enclosing block, not lower_stmt")
        }
        ast::Stmt::Expr { expr, .. } => {
            let ir_expr = lower_expr(ctx, expr);
            IrStmtKind::Expr { expr: ir_expr }
        }
        ast::Stmt::Comment { text } => IrStmtKind::Comment { text: text.clone() },
        ast::Stmt::Error { .. } => IrStmtKind::Comment { text: "/* error */".to_string() },
    };

    IrStmt { kind, span }
}

/// Write through an assignment target's place (#3064). A one-level target
/// (`xs[i] = v`, `s.f = v`, empty `path`) is `write` on the root binding
/// itself. A nested one — `o.inner.xs = v`, `o.m[k] = v` — reads each record
/// on the path into a fresh `var`, applies `write` to the innermost, and
/// stores each back into its holder, innermost first:
///
/// ```text
/// { var t1 = o.inner; t1.xs = v; o.inner = t1 }
/// ```
///
/// Only one-level writes reach the backends, so native, wasm and the
/// interpreter carry the nested form with the value semantics each already
/// gives `s.f = v`: an alias of `o` or of `o.inner` taken before the write
/// keeps the old value. The temps are `alloc_fresh`, so two nested writes in
/// one block never share a name (#3049).
/// The (key, element) slot types an `xs[i] = v` / `m[k] = v` write into
/// `var` gives its index and value: a List's element, a Map's key and value.
fn index_write_slots(ctx: &LowerCtx, var: VarId) -> (Option<Ty>, Option<Ty>) {
    use almide_lang::types::constructor::TypeConstructorId as TC;
    match ctx.env.resolve_named(&ctx.var_table.get(var).ty) {
        Ty::Applied(TC::List, args) if args.len() == 1 => (None, args.first().cloned()),
        Ty::Applied(TC::Map, args) if args.len() == 2 => (args.first().cloned(), args.get(1).cloned()),
        _ => (None, None),
    }
}

/// `r.f = v` (#3185): the field's declared type is the value's literal slot,
/// as an annotation is for `let` — a bare `254` into a `UInt8` field emitted
/// `254i64` (rustc E0308).
fn coerce_field_write(ctx: &LowerCtx, var: VarId, field: almide_base::intern::Sym, ir_val: &mut IrExpr) {
    let holder_ty = ctx.var_table.get(var).ty.clone();
    let field_ty = ctx.resolve_field_ty(&holder_ty, field.as_str());
    coerce_literal_to_sized(ir_val, &field_ty, ctx.env);
}

fn lower_place_write(
    ctx: &mut LowerCtx,
    target: &almide_base::intern::Sym,
    path: &[almide_base::intern::Sym],
    span: Option<almide_base::Span>,
    write: impl FnOnce(&mut LowerCtx, VarId) -> IrStmtKind,
) -> IrStmtKind {
    let root = ctx.lookup_var(target).unwrap_or(VarId(0));
    if path.is_empty() {
        return write(ctx, root);
    }
    let mk = |kind: IrExprKind, ty: Ty| IrExpr { kind, ty, span, def_id: None };
    let mut stmts = Vec::new();
    let mut chain: Vec<(VarId, almide_base::intern::Sym, VarId, Ty)> = Vec::new();
    let (mut holder, mut holder_ty) = (root, ctx.var_table.get(root).ty.clone());
    for step in path {
        let step_ty = match ctx.env.resolve_named(&holder_ty) {
            Ty::Record { fields } | Ty::OpenRecord { fields } =>
                fields.iter().find(|(n, _)| n == step).map(|(_, t)| t.clone()).unwrap_or(Ty::Unknown),
            _ => Ty::Unknown,
        };
        let tmp = ctx.var_table.alloc_fresh("__place", step_ty.clone(), Mutability::Var, span);
        let read = mk(IrExprKind::Member {
            object: Box::new(mk(IrExprKind::Var { id: holder }, holder_ty.clone())),
            field: *step,
        }, step_ty.clone());
        stmts.push(IrStmt { kind: IrStmtKind::Bind { var: tmp, mutability: Mutability::Var, ty: step_ty.clone(), value: read }, span });
        chain.push((holder, *step, tmp, step_ty.clone()));
        (holder, holder_ty) = (tmp, step_ty);
    }
    stmts.push(IrStmt { kind: write(ctx, holder), span });
    for (holder, field, tmp, ty) in chain.into_iter().rev() {
        let value = mk(IrExprKind::Var { id: tmp }, ty);
        stmts.push(IrStmt { kind: IrStmtKind::FieldAssign { target: holder, field, value }, span });
    }
    IrStmtKind::Expr { expr: mk(IrExprKind::Block { stmts, expr: None }, Ty::Unit) }
}

/// The source span of a statement.
///
/// A comment has no span of its own: it is attached to whatever follows it, so
/// pointing a diagnostic at the comment would point away from the code.
fn stmt_span(stmt: &ast::Stmt) -> Option<ast::Span> {
    match stmt {
        ast::Stmt::Let { span, .. } | ast::Stmt::Var { span, .. }
        | ast::Stmt::Assign { span, .. } | ast::Stmt::Guard { span, .. }
        | ast::Stmt::GuardLet { span, .. }
        | ast::Stmt::Expr { span, .. } | ast::Stmt::IndexAssign { span, .. }
        | ast::Stmt::FieldAssign { span, .. } | ast::Stmt::LetDestructure { span, .. }
        | ast::Stmt::Error { span, .. } => *span,
        ast::Stmt::Comment { .. } => None,
    }
}

/// Lower a `let` or `var` binding.
///
/// The two differ only in mutability, and the difference was previously two
/// copies of this body — one of which had lost the explanatory comments. One
/// body, `mutability` as the parameter.
fn lower_bind(
    ctx: &mut LowerCtx,
    name: &str,
    ty: Option<&ast::TypeExpr>,
    value: &ast::Expr,
    mutability: Mutability,
    span: Option<ast::Span>,
) -> IrStmtKind {
    let mut ir_val = lower_expr(ctx, value);
    // An explicit `let x: T = ...` annotation wins over the structurally
    // inferred type of the value. Otherwise two nominal record types with
    // identical fields (`Dog` and `Cat`, both `{ name: String }`) collide at
    // codegen, because the value keeps its structural type and
    // `collect_named_records` keys by sorted field names.
    let val_ty = match ty {
        Some(te) => {
            let declared = crate::canonicalize::resolve::resolve_type_expr_in(
                te, Some(&ctx.env.types), ctx.current_module.as_ref().map(|s| s.as_str()));
            override_record_literal_ty(&mut ir_val, &declared, ctx.env);
            ir_val = super::expressions::adapt_fn_value_to_effect_slot(ctx, ir_val, &declared);
            declared
        }
        None => ir_val.ty.clone(),
    };
    let var = ctx.define_var(name, val_ty.clone(), mutability, span);
    // #485: an EXPLICIT `Result[..]` annotation is the only signal that this
    // binding keeps the Result (auto_try must not insert `?`). Un-annotated
    // binds share the same `Bind.ty` shape when the callee itself declares
    // `-> Result[..]`, so the distinction has to be recorded per VarId.
    // ADR-0008 D2 (#1123 N+1): `let _ = f()` is the sanctioned DISCARD — the
    // Result binds dead, nothing propagates, so it keeps the Result too.
    if (ty.is_some() || name == "_") && val_ty.is_result() {
        ctx.annotated_result_vars.insert(var);
    }
    IrStmtKind::Bind { var, mutability, ty: val_ty, value: ir_val }
}

/// Retag an anonymous record literal's IR type with the declared nominal type.
///
/// Record literals are inferred as structural `Ty::Record { fields }`. When
/// assigned to a let with an explicit nominal annotation (e.g. `let d: Dog`),
/// the declared type should win. Otherwise multiple nominal types with
/// identical field shapes (Dog vs Cat, both `{name: String}`) collide at
/// codegen because `collect_named_records` keys by sorted field names.
fn override_record_literal_ty(ir_val: &mut IrExpr, declared: &Ty, env: &TypeEnv) {
    // Nominal record type override — keeps `Dog` / `Cat` distinct even
    // when their structural shapes match.
    if matches!(declared, Ty::Named(_, _)) {
        match &mut ir_val.kind {
            IrExprKind::Record { .. } => {
                if matches!(ir_val.ty, Ty::Record { .. } | Ty::OpenRecord { .. } | Ty::Unknown) {
                    ir_val.ty = declared.clone();
                }
            }
            IrExprKind::Block { expr: Some(inner), .. } => {
                override_record_literal_ty(inner, declared, env);
                if matches!(ir_val.ty, Ty::Record { .. } | Ty::OpenRecord { .. } | Ty::Unknown) {
                    ir_val.ty = declared.clone();
                }
            }
            _ => {}
        }
        // A named record/alias whose structural shape carries sized fields
        // (`type Rec = { b: Int8, n: Int }`) still needs its bare-literal
        // field values narrowed to the sized field types. The nominal retag
        // above only fixes the record's *own* type tag; `coerce_literal_to_sized`
        // resolves the Named type and descends into the field literals.
        coerce_literal_to_sized(ir_val, declared, env);
        return;
    }

    // Sized numeric literal coercion (Stage 1b). When the binding is
    // annotated with a sized integer / float type (`Int32`, `UInt8`,
    // `Float32`, ...) and the value is a bare Int/Float literal whose
    // inferred type is the default `Ty::Int` / `Ty::Float`, rewrite
    // the literal's IR type to the annotation. Codegen reads
    // `expr.ty` for the literal suffix (`42i64` → `42i32`), so this
    // is the single hook that makes `let x: Int32 = 42` emit correct
    // Rust instead of an `i64` / `i32` mismatch.
    coerce_literal_to_sized(ir_val, declared, env);
}

/// Whether `ty` is one of the sized numeric types the Stage 1a/1b
/// literal coercion rule should retype bare literals into.
pub(crate) fn is_sized_numeric(ty: &Ty) -> bool {
    matches!(
        ty,
        Ty::Int8 | Ty::Int16 | Ty::Int32
            | Ty::UInt8 | Ty::UInt16 | Ty::UInt32 | Ty::UInt64
            | Ty::Float32
    )
}

/// Retype a bare Int / Float literal IR node to the sized numeric
/// `declared` type, so codegen emits the right Rust suffix
/// (`42i32` / `3.14f32` / ...). Called from `override_record_literal_ty`
/// (let / var bindings) and `coerce_call_arg_to_sized_param` (fn call
/// sites). No-op when the value isn't a literal of compatible default
/// type — which matches the Stage 1b rule that literals flow into
/// sized slots but named-variable refs don't (they retype instead
/// with an explicit conversion).
///
/// Recurses through container literals so a sized field nested inside a
/// list / tuple / record annotation also coerces: `let a: List[(Int8,
/// Int)] = [(1, 100)]` retypes the `1` element to `Int8` while the type
/// checker only narrowed the *binding* type (the inner literal keeps the
/// default `Ty::Int` in `expr_types`, so codegen would otherwise emit
/// `1i64` against a `Vec<(i8, i64)>` slot — an E0308). The declared type
/// drives the descent; the value literal's own shape must match for any
/// element to be touched.
pub(crate) fn coerce_literal_to_sized(ir_val: &mut IrExpr, declared: &Ty, env: &TypeEnv) {
    use almide_lang::types::constructor::TypeConstructorId;
    // Look through blocks/parenthesized tails: a literal can be wrapped in
    // a single-tail block (e.g. `{ (1, 2) }`) by lowering.
    if let IrExprKind::Block { expr: Some(tail), .. } = &mut ir_val.kind {
        coerce_literal_to_sized(tail, declared, env);
        return;
    }
    // #880: an `if` / `match` carries no literal of its own — its ARMS are the
    // peers that do. The slot's width belongs to each arm the same way it
    // belongs to a block tail, so descend and let every arm coerce on its own
    // (`let v: UInt8 = if b then 1 else u8v` emitted `if … { 1i64 } else { 3u8 }`
    // into a `u8` binding — one arm retyped by the peer join, the other not).
    match &mut ir_val.kind {
        IrExprKind::If { then, else_, .. } => {
            coerce_literal_to_sized(then, declared, env);
            coerce_literal_to_sized(else_, declared, env);
            return;
        }
        IrExprKind::Match { arms, .. } => {
            for arm in arms.iter_mut() {
                coerce_literal_to_sized(&mut arm.body, declared, env);
            }
            return;
        }
        _ => {}
    }
    // Resolve a named type alias to its structural form so a record / sized
    // alias declared via `type Rec = { b: Int8, .. }` (a `Ty::Named`) becomes
    // its `Ty::Record { .. }` / `Ty::Int8` / etc. before the match below.
    // `resolve_named` is a no-op for non-Named types, so the scalar / List /
    // Tuple arms are unaffected.
    let resolved = env.resolve_named(declared);
    let declared = &resolved;
    if is_sized_numeric(declared) {
        retype_scalar_literal(ir_val, declared);
        return;
    }
    match declared {
        // List[T]: every element literal is coerced against T.
        Ty::Applied(TypeConstructorId::List, args) if args.len() == 1 => {
            if let IrExprKind::List { elements } = &mut ir_val.kind {
                for e in elements.iter_mut() {
                    coerce_literal_to_sized(e, &args[0], env);
                }
            }
        }
        // Tuple([t0, t1, ...]): element i is coerced against t_i.
        Ty::Tuple(elem_tys) => coerce_tuple_elements(ir_val, elem_tys, env),
        // Structural record annotation `{ b: Int8, n: Int }`: coerce each
        // field value against its declared field type, matched by name.
        Ty::Record { fields: decl_fields } | Ty::OpenRecord { fields: decl_fields } =>
            coerce_record_fields(ir_val, decl_fields, env),
        // #3060: a `some(..)` / `ok(..)` / `err(..)` payload and a lambda body
        // are value positions of the slot's inner type, and the node's own
        // type is what codegen spells (`Some::<i64>`, `Ok::<i64, String>`,
        // `dyn Fn(i64) -> i64`) — both are narrowed together.
        Ty::Applied(TypeConstructorId::Option, _) | Ty::Applied(TypeConstructorId::Result, _) =>
            coerce_carrier_payload(ir_val, declared, env),
        Ty::Fn { ret, is_effect: false, .. } => coerce_lambda_body(ir_val, ret, env),
        _ => {}
    }
}

/// Whether `inner` is the default numeric type a literal of the sized `slot`
/// starts at (`Int` for the integer widths, `Float` for `Float32`) — or not
/// yet known — so a carrier or fn type built around it may take the slot.
fn is_default_width_of(inner: &Ty, slot: &Ty) -> bool {
    let default = match slot {
        Ty::Int8 | Ty::Int16 | Ty::Int32 | Ty::UInt8 | Ty::UInt16 | Ty::UInt32 | Ty::UInt64 => Ty::Int,
        Ty::Float32 => Ty::Float,
        _ => return inner == slot,
    };
    *inner == default || *inner == *slot || matches!(inner, Ty::Unknown | Ty::TypeVar(_))
}

/// Narrow an Option / Result constructor against its declared carrier: the
/// payload coerces against its slot, and the node takes the declared type when
/// every argument it holds is that slot's default width (`none` included).
fn coerce_carrier_payload(ir_val: &mut IrExpr, declared: &Ty, env: &TypeEnv) {
    use almide_lang::types::constructor::TypeConstructorId;
    let Ty::Applied(ctor, slots) = declared else { return };
    let payload_slot = match (&mut ir_val.kind, ctor, slots.as_slice()) {
        (IrExprKind::OptionSome { expr }, TypeConstructorId::Option, [t])
        | (IrExprKind::ResultOk { expr }, TypeConstructorId::Result, [t, _])
        | (IrExprKind::ResultErr { expr }, TypeConstructorId::Result, [_, t]) => {
            coerce_literal_to_sized(expr, t, env);
            Some((expr.ty.clone(), t.clone()))
        }
        (IrExprKind::OptionNone, TypeConstructorId::Option, [_]) => None,
        _ => return,
    };
    if payload_slot.is_some_and(|(have, want)| have != want) {
        return;
    }
    if let Ty::Applied(own, args) = &ir_val.ty
        && own == ctor
        && args.len() == slots.len()
        && args.iter().zip(slots).all(|(a, s)| is_default_width_of(a, s))
    {
        ir_val.ty = declared.clone();
    }
}

/// Narrow a pure lambda's body against the declared fn type's return, and
/// the lambda's own fn type with it (codegen casts the closure to it).
fn coerce_lambda_body(ir_val: &mut IrExpr, ret: &Ty, env: &TypeEnv) {
    let IrExprKind::Lambda { body, .. } = &mut ir_val.kind else { return };
    coerce_literal_to_sized(body, ret, env);
    if let Ty::Fn { ret: own, .. } = &mut ir_val.ty
        && **own != *ret
        && is_default_width_of(own, ret)
        && coerced_width(body) == Some(ret)
    {
        **own = ret.clone();
    }
}

/// The width a coerced value now yields: its own type, or — for a branch
/// whose node type keeps the peer join — the width its first arm settled on.
fn coerced_width(e: &IrExpr) -> Option<&Ty> {
    match &e.kind {
        IrExprKind::Block { expr: Some(tail), .. } => coerced_width(tail),
        IrExprKind::If { then, .. } => coerced_width(then),
        IrExprKind::Match { arms, .. } => arms.first().and_then(|a| coerced_width(&a.body)),
        _ => Some(&e.ty),
    }
}

/// Retype a bare default-typed literal to a sized numeric slot.
///
/// Only a LITERAL-ONLY expression is retyped. A tree with a non-literal leaf
/// already has whatever width its own operands gave it, and silently widening or
/// narrowing that would change the arithmetic rather than just record the
/// annotation — that half is the checker's to reject (#880).
///
/// "Literal-only" reaches through negation and int arithmetic, not just a bare
/// literal: `let b: Int32 = 5 - 3` has no operand that could have supplied a
/// width, so every node in it is still the default `Int` and the emitted value
/// landed in the `i32` slot as `2i64` — invalid Rust that `check` accepted
/// (#895 follow-on, the fourth shape of #899). The negation case matters for the
/// same reason: the parser produces `-(1)`, not a signed literal, so both the
/// operand and the negation node have to carry the sized type or codegen emits a
/// width mismatch between them.
fn retype_scalar_literal(ir_val: &mut IrExpr, declared: &Ty) {
    let default = match declared {
        Ty::Int8 | Ty::Int16 | Ty::Int32 | Ty::Int64 | Ty::UInt8 | Ty::UInt16 | Ty::UInt32 | Ty::UInt64 => Ty::Int,
        Ty::Float32 | Ty::Float64 => Ty::Float,
        _ => return,
    };
    if is_default_numeric_tree(ir_val, &default) {
        stamp_numeric_tree(ir_val, declared);
    }
}

/// Whether every node of this expression is still the `default` numeric type
/// and every leaf is a literal of it — i.e. nothing in the tree chose a width.
fn is_default_numeric_tree(e: &IrExpr, default: &Ty) -> bool {
    if e.ty != *default {
        return false;
    }
    let (neg_op, ops): (almide_ir::UnOp, &[BinOp]) = match (default, &e.kind) {
        (Ty::Int, IrExprKind::LitInt { .. }) | (Ty::Float, IrExprKind::LitFloat { .. }) => return true,
        (Ty::Int, _) => (
            almide_ir::UnOp::NegInt,
            &[BinOp::AddInt, BinOp::SubInt, BinOp::MulInt, BinOp::DivInt, BinOp::ModInt, BinOp::PowInt],
        ),
        (Ty::Float, _) => (
            almide_ir::UnOp::NegFloat,
            &[BinOp::AddFloat, BinOp::SubFloat, BinOp::MulFloat, BinOp::DivFloat, BinOp::ModFloat, BinOp::PowFloat],
        ),
        _ => return false,
    };
    match &e.kind {
        IrExprKind::UnOp { op, operand } if *op == neg_op => is_default_numeric_tree(operand, default),
        IrExprKind::BinOp { op, left, right } => {
            ops.contains(op)
                && is_default_numeric_tree(left, default)
                && is_default_numeric_tree(right, default)
        }
        _ => false,
    }
}

/// Stamp `declared` on every node of a tree `is_default_numeric_tree` accepted.
/// Whole-tree, because an operator node and its operands must agree on width.
fn stamp_numeric_tree(e: &mut IrExpr, declared: &Ty) {
    e.ty = declared.clone();
    match &mut e.kind {
        IrExprKind::UnOp { operand, .. } => stamp_numeric_tree(operand, declared),
        IrExprKind::BinOp { left, right, .. } => {
            stamp_numeric_tree(left, declared);
            stamp_numeric_tree(right, declared);
        }
        _ => {}
    }
}

/// Coerce a tuple literal's elements positionally.
///
/// A length mismatch is left alone: the checker reports it, and coercing a
/// prefix would leave the value half-retyped behind that diagnostic.
fn coerce_tuple_elements(ir_val: &mut IrExpr, elem_tys: &[Ty], env: &TypeEnv) {
    let IrExprKind::Tuple { elements } = &mut ir_val.kind else { return };
    if elements.len() != elem_tys.len() {
        return;
    }
    for (e, t) in elements.iter_mut().zip(elem_tys.iter()) {
        coerce_literal_to_sized(e, t, env);
    }
}

/// Coerce a record literal's field values, matched by field name.
///
/// Matching by name rather than position is required: a record literal's fields
/// are in source order while the declared fields are in declaration order.
fn coerce_record_fields(ir_val: &mut IrExpr, decl_fields: &[(almide_base::intern::Sym, Ty)], env: &TypeEnv) {
    let IrExprKind::Record { fields, .. } = &mut ir_val.kind else { return };
    for (fname, fvalue) in fields.iter_mut() {
        if let Some((_, fty)) = decl_fields.iter().find(|(n, _)| n == fname) {
            coerce_literal_to_sized(fvalue, fty, env);
        }
    }
}

/// Resolve the declared field types of a named record construction
/// (`Name { ... }`) into a structural `Ty::Record`, so the construction
/// site can narrow bare-literal field values to their sized field types
/// (`coerce_literal_to_sized`). `name` may be either:
///   - a record TYPE name (`type Rec = { a: Int8 }`) — looked up in
///     `env.types` and resolved to its `Ty::Record` shape, or
///   - a record-bearing VARIANT case (`Scroll { dy: Int8 }`) — found in
///     `env.constructors`, whose `VariantPayload::Record` carries the fields.
/// Returns `None` for anonymous records, tuple/unit cases, or unknown names
/// (nothing to coerce against).
pub(crate) fn declared_record_ty(env: &TypeEnv, name: almide_base::intern::Sym, cur_mod: Option<&str>) -> Option<Ty> {
    // Variant case with a record payload takes priority, resolved the way the
    // checker resolves it (`lookup_ctor_written`): only a case visible from
    // this file, never over the file's own same-named type (#2636), and a
    // qualified name inside its module alone (#3176).
    if let Some((_, case)) = env.lookup_ctor_written(name.as_str(), cur_mod) {
        if let crate::types::VariantPayload::Record(fields) = &case.payload {
            return Some(Ty::Record { fields: fields.clone() });
        }
        return None;
    }
    // Record type name: resolve the alias to its structural record form.
    if let Some(ty) = env.types.get(&name) {
        let resolved = env.resolve_named(ty);
        if matches!(resolved, Ty::Record { .. } | Ty::OpenRecord { .. }) {
            return Some(resolved);
        }
    }
    None
}

include!("pattern_lowering.rs");
