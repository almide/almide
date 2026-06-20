// ── Expression lowering ─────────────────────────────────────────

use almide_lang::ast;
use almide_base::intern::sym;
use almide_ir::*;
use crate::types::{Ty, TypeConstructorId};
use super::LowerCtx;
use super::calls::{lower_call, lower_call_target};
use super::statements::lower_stmt;
use super::statements::lower_pattern;
use super::types::resolve_type_expr;

pub(super) fn lower_expr(ctx: &mut LowerCtx, expr: &ast::Expr) -> IrExpr {
    let ty = ctx.expr_ty(expr);
    let span = expr.span;

    match &expr.kind {
        // ── Literals ──
        ast::ExprKind::Int { raw, .. } => {
            let value = if raw.starts_with("0x") || raw.starts_with("0X") {
                i64::from_str_radix(&raw[2..].replace('_', ""), 16).unwrap_or(0)
            } else if raw.starts_with("0b") || raw.starts_with("0B") {
                i64::from_str_radix(&raw[2..].replace('_', ""), 2).unwrap_or(0)
            } else if raw.starts_with("0o") || raw.starts_with("0O") {
                i64::from_str_radix(&raw[2..].replace('_', ""), 8).unwrap_or(0)
            } else {
                raw.replace('_', "").parse::<i64>().unwrap_or(0)
            };
            ctx.mk(IrExprKind::LitInt { value }, ty, span)
        }
        ast::ExprKind::Float { value, .. } => ctx.mk(IrExprKind::LitFloat { value: *value }, ty, span),
        ast::ExprKind::String { value, .. } => ctx.mk(IrExprKind::LitStr { value: value.clone() }, ty, span),
        ast::ExprKind::Bool { value, .. } => ctx.mk(IrExprKind::LitBool { value: *value }, ty, span),
        ast::ExprKind::Unit => ctx.mk(IrExprKind::Unit, Ty::Unit, span),

        // ── Variables ──
        ast::ExprKind::Ident { name, .. } => {
            if let Some(var_id) = ctx.lookup_var(name) {
                // The type checker fills `ty` from `expr_types`. Names bound
                // by record / variant patterns can land here as `Unknown`
                // because the checker doesn't propagate the variant case's
                // field types into the binding occurrence (the pattern
                // lowering puts the field type into VarTable, but the
                // checker's `expr_types` for the bare identifier reference
                // hasn't been re-resolved post-pattern). Promote to the
                // VarTable type only when the checker truly has nothing
                // (`Unknown`) and the VarTable has a fully concrete type —
                // we never want to fold a `TypeVar` from a generic body's
                // VarTable into a call-site IR expression, since that would
                // confuse mono's binding discovery.
                let resolved = if matches!(ty, Ty::Unknown) {
                    let vt_ty = ctx.var_table.get(var_id).ty.clone();
                    if !vt_ty.contains_unknown() && !vt_ty.contains_typevar() {
                        vt_ty
                    } else { ty }
                } else { ty };
                ctx.mk(IrExprKind::Var { id: var_id }, resolved, span)
            } else if let Ty::Fn { params: param_tys, ret } = &ty {
                // Function/top-let used as a value → eta-expand to lambda
                // so borrow insertion handles param types correctly (e.g. String → &str).
                // Use the type (not env.functions) to detect: module-scoped functions
                // have their bare names removed after type checking (restore_keys).
                let params: Vec<(VarId, Ty)> = param_tys.iter().enumerate().map(|(i, pt)| {
                    let vid = ctx.var_table.alloc(sym(&format!("_fn_arg{}", i)), pt.clone(), Mutability::Let, None);
                    (vid, pt.clone())
                }).collect();
                let call_args: Vec<IrExpr> = params.iter().map(|(vid, pt)| {
                    ctx.mk(IrExprKind::Var { id: *vid }, pt.clone(), span)
                }).collect();
                let ret_ty = ret.as_ref().clone();
                let body = ctx.mk(IrExprKind::Call {
                    target: CallTarget::Named { name: sym(name) },
                    args: call_args, type_args: vec![],
                }, ret_ty, span);
                ctx.mk(IrExprKind::Lambda {
                    params, body: Box::new(body), lambda_id: None,
                }, ty, span)
            } else if ctx.env.functions.contains_key(&sym(name)) {
                ctx.mk(IrExprKind::FnRef { name: sym(name) }, ty, span)
            } else {
                ctx.mk(IrExprKind::Var { id: VarId(0) }, ty, span) // error recovery
            }
        }
        ast::ExprKind::TypeName { name, .. } => {
            // Variant constructor used as value (e.g., Red)
            if let Some((_, case)) = ctx.env.lookup_ctor(&sym(name)) {
                if let crate::types::VariantPayload::Tuple(param_tys) = &case.payload {
                    if !param_tys.is_empty() && matches!(&ty, Ty::Fn { .. }) {
                        // Constructor with payload as function value → generate lambda
                        // Use instantiated types from `ty` (type checker output) instead
                        // of raw `case.payload` (which may contain unresolved TypeVars
                        // for generic constructors like Box[T]).
                        let inst_params = match &ty {
                            Ty::Fn { params: ip, .. } => ip.clone(),
                            _ => param_tys.clone(),
                        };
                        let params: Vec<(VarId, Ty)> = inst_params.iter().enumerate().map(|(i, pt)| {
                            let vid = ctx.var_table.alloc(sym(&format!("_ctor_arg{}", i)), pt.clone(), Mutability::Let, None);
                            (vid, pt.clone())
                        }).collect();
                        let ctor_args: Vec<IrExpr> = params.iter().map(|(vid, pt)| {
                            ctx.mk(IrExprKind::Var { id: *vid }, pt.clone(), span)
                        }).collect();
                        let ret_ty = match &ty {
                            Ty::Fn { ret, .. } => ret.as_ref().clone(),
                            _ => ty.clone(),
                        };
                        let body = ctx.mk(IrExprKind::Call {
                            target: CallTarget::Named { name: sym(name) },
                            args: ctor_args, type_args: vec![],
                        }, ret_ty, span);
                        return ctx.mk(IrExprKind::Lambda {
                            params, body: Box::new(body), lambda_id: None,
                        }, ty, span);
                    }
                }
                ctx.mk(IrExprKind::Call {
                    target: CallTarget::Named { name: sym(name) },
                    args: vec![], type_args: vec![],
                }, ty, span)
            } else if let Some(var_id) = ctx.lookup_var(name) {
                ctx.mk(IrExprKind::Var { id: var_id }, ty, span)
            } else if let Some(Ty::ConstParam { name: pname, ty: param_ty }) = ctx.env.types.get(&sym(name)).cloned() {
                // Const param reference: look up existing VarId or allocate one.
                // The const param is treated as an implicit function parameter at runtime.
                let var_id = if let Some(vid) = ctx.const_param_vars.get(&pname) {
                    *vid
                } else {
                    let vid = ctx.var_table.alloc(pname, *param_ty.clone(), Mutability::Let, None);
                    ctx.const_param_vars.insert(pname, vid);
                    vid
                };
                ctx.mk(IrExprKind::Var { id: var_id }, *param_ty, span)
            } else {
                ctx.mk(IrExprKind::Var { id: VarId(0) }, ty, span)
            }
        }

        // ── Collections ──
        ast::ExprKind::List { elements, .. } => {
            let elems = elements.iter().map(|e| lower_expr(ctx, e)).collect();
            ctx.mk(IrExprKind::List { elements: elems }, ty, span)
        }
        ast::ExprKind::MapLiteral { entries, .. } => {
            let pairs = entries.iter().map(|(k, v)| (lower_expr(ctx, k), lower_expr(ctx, v))).collect();
            ctx.mk(IrExprKind::MapLiteral { entries: pairs }, ty, span)
        }
        ast::ExprKind::EmptyMap => ctx.mk(IrExprKind::EmptyMap, ty, span),
        ast::ExprKind::Tuple { elements, .. } => {
            let elems: Vec<IrExpr> = elements.iter().map(|e| lower_expr(ctx, e)).collect();
            // Type-checker fills `ty` from `expr_types`; for a tuple whose
            // element exprs depend on a pattern-bound name, that ty can be
            // `Tuple([Unknown, ..])` even when the lowered elements now
            // carry concrete types (see the same fix on `Ident`). Rebuild
            // the tuple ty from the lowered elements when the checker's ty
            // is unresolved so downstream `Some(tuple)` / `List[tuple]`
            // chains get a clean propagation path.
            let resolved_ty = if ty.has_unresolved_deep()
                && elems.iter().all(|e| !e.ty.has_unresolved_deep())
            {
                Ty::Tuple(elems.iter().map(|e| e.ty.clone()).collect())
            } else { ty };
            ctx.mk(IrExprKind::Tuple { elements: elems }, resolved_ty, span)
        }

        // ── Records ──
        ast::ExprKind::Record { name, fields, .. } => {
            let fs = fields.iter().map(|f| (f.name, lower_expr(ctx, &f.value))).collect();
            // Constructor name resolution:
            //  - A struct (Record-type) literal is pinned to its qualified canonical
            //    name `mod.Type` (#433) — bare `Config` in module M → `M.Config`, a
            //    cross-module `dep.Config` stays qualified — so codegen names the
            //    right (mangled) struct. (`is_struct` distinguishes it from a variant.)
            //  - A variant constructor keeps the bare ctor name: the expr's type pins
            //    the module and both backends resolve it by name + type (#412).
            let ctor_name = (*name).map(|n| {
                let s = n.as_str();
                let is_struct = |key: &str| matches!(ctx.env.types.get(&sym(key)), Some(crate::types::Ty::Record { .. }));
                if let Some((m, base)) = s.rsplit_once('.') {
                    if !almide_lang::stdlib_info::is_bundled_module(m) && is_struct(s) {
                        return n; // user-module struct: keep qualified for mangling
                    }
                    return sym(base); // stdlib / variant: strip (existing #412 behavior)
                }
                if let Some(m) = ctx.current_module {
                    let qual = format!("{}.{}", m.as_str(), s);
                    if is_struct(&qual) {
                        return sym(&qual);
                    }
                }
                n
            });
            let mut rec = ctx.mk(IrExprKind::Record { name: ctor_name, fields: fs }, ty, span);
            // Narrow bare integer/float literals in sized fields to their
            // declared field type (`{ a: Int8 }` ← `a: 5` must emit `5i8`, not
            // `5i64`). Inference leaves the literal at the default `Ty::Int`
            // even with no binding annotation, so the construction site itself
            // — driven by the declared struct/case field types — is the only
            // place this is guaranteed to run. Without it native rustc rejects
            // `M { a: 5i64 }` (E0308) and WASM writes the wrong byte width into
            // the field, corrupting the next field. Mirrors the let/var path
            // in `override_record_literal_ty`.
            if let Some(decl) = name.and_then(|n| super::statements::declared_record_ty(ctx.env, n)) {
                super::statements::coerce_literal_to_sized(&mut rec, &decl, ctx.env);
            }
            rec
        }
        ast::ExprKind::SpreadRecord { base, fields, .. } => {
            let ir_base = lower_expr(ctx, base);
            let fs = fields.iter().map(|f| (f.name, lower_expr(ctx, &f.value))).collect();
            ctx.mk(IrExprKind::SpreadRecord { base: Box::new(ir_base), fields: fs }, ty, span)
        }

        // ── Operators ──
        ast::ExprKind::Binary { op, left, right, .. } => {
            let mut l = lower_expr(ctx, left);
            let mut r = lower_expr(ctx, right);
            // Sized Numeric Types (Stage 1c): when one operand is a
            // sized type and the other is a bare Int/Float literal,
            // retype the literal so the resulting BinOp has matching
            // operand widths. Mirrors the fn-arg and let-binding
            // coercion rules — the same authoritative-context rule
            // applies to any pairing where one side locks the width.
            super::statements::coerce_literal_to_sized(&mut r, &l.ty, ctx.env);
            super::statements::coerce_literal_to_sized(&mut l, &r.ty, ctx.env);
            // Resolve operand types: if expr.ty is Unknown, try VarTable lookup
            let left_ty = if l.ty == Ty::Unknown {
                if let IrExprKind::Var { id } = &l.kind { ctx.var_table.get(*id).ty.clone() } else { l.ty.clone() }
            } else { l.ty.clone() };
            let left_ty = &left_ty;
            // Operator protocol: dispatch == / != to convention methods if available
            if op == "==" || op == "!=" {
                if let Some(eq_fn) = ctx.find_convention_fn(left_ty, "eq") {
                    let call = ctx.mk(IrExprKind::Call {
                        target: CallTarget::Named { name: eq_fn },
                        args: vec![l, r], type_args: vec![],
                    }, Ty::Bool, span);
                    if op == "!=" {
                        return ctx.mk(IrExprKind::UnOp { op: UnOp::Not, operand: Box::new(call) }, Ty::Bool, span);
                    }
                    return call;
                }
            }
            let right_ty = if r.ty == Ty::Unknown {
                if let IrExprKind::Var { id } = &r.kind { ctx.var_table.get(*id).ty.clone() } else { r.ty.clone() }
            } else { r.ty.clone() };
            let right_ty = &right_ty;
            let bin_op = match (op.as_str(), left_ty, right_ty) {
                ("+", Ty::String, _) | ("+", _, Ty::String) => BinOp::ConcatStr,
                ("+", Ty::Applied(TypeConstructorId::List, _), _) | ("+", _, Ty::Applied(TypeConstructorId::List, _)) => BinOp::ConcatList,
                // Matrix operators
                ("+", Ty::Matrix, Ty::Matrix) => BinOp::AddMatrix,
                ("-", Ty::Matrix, Ty::Matrix) => BinOp::SubMatrix,
                ("*", Ty::Matrix, Ty::Matrix) => BinOp::MulMatrix,
                ("*", Ty::Matrix, Ty::Float) | ("*", Ty::Float, Ty::Matrix) => BinOp::ScaleMatrix,
                ("*", Ty::Matrix, Ty::Int) | ("*", Ty::Int, Ty::Matrix) => BinOp::ScaleMatrix,
                // Float dispatch covers canonical `Float` plus the sized
                // `Float32`. Any other numeric type (Int / Int8 ... /
                // UInt64) takes the Int path. The *width* of the
                // arithmetic op (i32_add vs i64_add vs f32_add vs
                // f64_add) is resolved at WASM emit time from the
                // operand's valtype; Rust codegen emits plain `a + b`
                // and lets rustc pick.
                ("+", Ty::Float, _) | ("+", _, Ty::Float)
                | ("+", Ty::Float32, _) | ("+", _, Ty::Float32) => BinOp::AddFloat,
                ("+", _, _) => BinOp::AddInt,
                ("-", Ty::Float, _) | ("-", _, Ty::Float)
                | ("-", Ty::Float32, _) | ("-", _, Ty::Float32) => BinOp::SubFloat,
                ("-", _, _) => BinOp::SubInt,
                ("*", Ty::Float, _) | ("*", _, Ty::Float)
                | ("*", Ty::Float32, _) | ("*", _, Ty::Float32) => BinOp::MulFloat,
                ("*", _, _) => BinOp::MulInt,
                ("/", Ty::Float, _) | ("/", _, Ty::Float)
                | ("/", Ty::Float32, _) | ("/", _, Ty::Float32) => BinOp::DivFloat,
                ("/", _, _) => BinOp::DivInt,
                ("%", Ty::Float, _) | ("%", _, Ty::Float)
                | ("%", Ty::Float32, _) | ("%", _, Ty::Float32) => BinOp::ModFloat,
                ("%", _, _) => BinOp::ModInt,
                ("^", Ty::Float, _) | ("^", _, Ty::Float)
                | ("^", Ty::Float32, _) | ("^", _, Ty::Float32) => BinOp::PowFloat,
                ("^", _, _) => BinOp::PowInt,
                ("==", _, _) => BinOp::Eq, ("!=", _, _) => BinOp::Neq,
                ("<", _, _) => BinOp::Lt, (">", _, _) => BinOp::Gt,
                ("<=", _, _) => BinOp::Lte, (">=", _, _) => BinOp::Gte,
                ("and", _, _) => BinOp::And, ("or", _, _) => BinOp::Or,
                _ => BinOp::AddInt,
            };
            ctx.mk(IrExprKind::BinOp { op: bin_op, left: Box::new(l), right: Box::new(r) }, ty, span)
        }
        ast::ExprKind::Unary { op, operand, .. } => {
            // #660: fold a unary minus into an int-literal parse so `i64::MIN`
            // (`-9223372036854775808`) is representable. Lowering the operand
            // first parses the bare magnitude `9223372036854775808`, which
            // overflows `i64` → `unwrap_or(0)`, and negating 0 yields 0.
            if op.as_str() == "-" {
                if let ast::ExprKind::Int { raw, .. } = &operand.kind {
                    let clean = raw.replace('_', "");
                    let parsed = if let Some(h) = clean.strip_prefix("0x").or_else(|| clean.strip_prefix("0X")) {
                        i64::from_str_radix(&format!("-{}", h), 16)
                    } else if let Some(b) = clean.strip_prefix("0b").or_else(|| clean.strip_prefix("0B")) {
                        i64::from_str_radix(&format!("-{}", b), 2)
                    } else if let Some(o) = clean.strip_prefix("0o").or_else(|| clean.strip_prefix("0O")) {
                        i64::from_str_radix(&format!("-{}", o), 8)
                    } else {
                        format!("-{}", clean).parse::<i64>()
                    };
                    if let Ok(value) = parsed {
                        return ctx.mk(IrExprKind::LitInt { value }, ty.clone(), span);
                    }
                }
            }
            let o = lower_expr(ctx, operand);
            let un_op = match (op.as_str(), &o.ty) {
                ("not", _) => UnOp::Not,
                ("-", Ty::Float) => UnOp::NegFloat,
                _ => UnOp::NegInt,
            };
            ctx.mk(IrExprKind::UnOp { op: un_op, operand: Box::new(o) }, ty, span)
        }

        // ── Control flow ──
        ast::ExprKind::If { cond, then, else_, .. } => {
            let c = lower_expr(ctx, cond);
            let t = lower_expr(ctx, then);
            let e = lower_expr(ctx, else_);
            ctx.mk(IrExprKind::If { cond: Box::new(c), then: Box::new(t), else_: Box::new(e) }, ty, span)
        }
        ast::ExprKind::Match { subject, arms, .. } => {
            let s = lower_expr(ctx, subject);
            // Resolve subject type: if the expression type disagrees with VarTable
            // (e.g., expr_types says Int but VarTable says Result[Int, String]),
            // prefer VarTable for container types needed by Ok/Err/Some/None patterns.
            let subject_ty = if let IrExprKind::Var { id } = &s.kind {
                let vt_ty = &ctx.var_table.get(*id).ty;
                if matches!(vt_ty, Ty::Applied(_, _)) && !matches!(&s.ty, Ty::Applied(_, _)) {
                    vt_ty.clone()
                } else {
                    s.ty.clone()
                }
            } else {
                s.ty.clone()
            };
            // Fix subject Var's type if it was wrong
            let s = if subject_ty != s.ty {
                IrExpr { ty: subject_ty.clone(), ..s }
            } else { s };
            let ir_arms = arms.iter().map(|arm| {
                ctx.push_scope();
                let pat = lower_pattern(ctx, &arm.pattern, &subject_ty);
                let guard = arm.guard.as_ref().map(|g| lower_expr(ctx, g));
                let body = lower_expr(ctx, &arm.body);
                ctx.pop_scope();
                IrMatchArm { pattern: pat, guard, body }
            }).collect();
            ctx.mk(IrExprKind::Match { subject: Box::new(s), arms: ir_arms }, ty, span)
        }
        ast::ExprKind::IfLet { name, scrutinee, then, else_ } => {
            // Swift-style implicit-unwrap if-let desugars to a 2-arm match on the
            // scrutinee's Option/Result: `name` binds the inner value in the Some/Ok
            // arm; the wildcard arm is the else branch. The wrapper (Some vs Ok) is
            // chosen from the (now-inferred) scrutinee type.
            let s = lower_expr(ctx, scrutinee);
            let subject_ty = if let IrExprKind::Var { id } = &s.kind {
                let vt_ty = &ctx.var_table.get(*id).ty;
                if matches!(vt_ty, Ty::Applied(_, _)) && !matches!(&s.ty, Ty::Applied(_, _)) {
                    vt_ty.clone()
                } else {
                    s.ty.clone()
                }
            } else {
                s.ty.clone()
            };
            let s = if subject_ty != s.ty { IrExpr { ty: subject_ty.clone(), ..s } } else { s };
            let inner = ast::Pattern::Ident { name: *name };
            let bind_pat = match &subject_ty {
                Ty::Applied(TypeConstructorId::Result, _) => {
                    ast::Pattern::Ok { inner: Box::new(inner) }
                }
                _ => ast::Pattern::Some { inner: Box::new(inner) },
            };
            ctx.push_scope();
            let pat1 = lower_pattern(ctx, &bind_pat, &subject_ty);
            let body1 = lower_expr(ctx, then);
            ctx.pop_scope();
            let arm1 = IrMatchArm { pattern: pat1, guard: None, body: body1 };
            ctx.push_scope();
            let pat2 = lower_pattern(ctx, &ast::Pattern::Wildcard, &subject_ty);
            let body2 = lower_expr(ctx, else_);
            ctx.pop_scope();
            let arm2 = IrMatchArm { pattern: pat2, guard: None, body: body2 };
            ctx.mk(IrExprKind::Match { subject: Box::new(s), arms: vec![arm1, arm2] }, ty, span)
        }
        ast::ExprKind::Block { stmts, expr, .. } => {
            ctx.push_scope();
            let body = lower_block_body(ctx, stmts, expr.as_deref(), &ty, span);
            ctx.pop_scope();
            body
        }

        ast::ExprKind::Fan { exprs, .. } => {
            let ir_exprs: Vec<IrExpr> = exprs.iter().map(|e| lower_expr(ctx, e)).collect();
            ctx.mk(IrExprKind::Fan { exprs: ir_exprs }, ty, span)
        }

        // ── Loops ──
        ast::ExprKind::ForIn { var, var_tuple, iterable, body, .. } => {
            let ir_iter = lower_expr(ctx, iterable);
            ctx.push_scope();
            let elem_ty = match &ir_iter.ty {
                Ty::Applied(TypeConstructorId::List, args) if args.len() == 1 => args[0].clone(),
                Ty::Applied(TypeConstructorId::Map, args) if args.len() == 2 => Ty::Tuple(vec![args[0].clone(), args[1].clone()]),
                _ => Ty::Unknown,
            };
            // For a tuple-destructure loop (`for (i, x) in …`) the loop var is a
            // synthetic holder for each tuple — the real user bindings are the
            // `var_tuple` components. Give it no span so the unused-variable check
            // never flags it (it is never used directly, only destructured, and it
            // inherits the first element's name → a spurious "unused 'i'"). A plain
            // `for x in …` keeps its span so a genuinely unused `x` is still flagged.
            let loop_var_span = if var_tuple.is_some() { None } else { span.clone() };
            let var_id = ctx.define_var(var, elem_ty.clone(), Mutability::Let, loop_var_span);
            let tuple_vars = var_tuple.as_ref().map(|names| {
                let tys = match &elem_ty {
                    Ty::Tuple(tys) => tys.clone(),
                    _ => vec![],
                };
                names.iter().enumerate().map(|(i, n)| {
                    let ty = tys.get(i).cloned().unwrap_or(Ty::Unknown);
                    ctx.define_var(n, ty, Mutability::Let, None)
                }).collect()
            });
            let ir_body: Vec<IrStmt> = body.iter().map(|s| lower_stmt(ctx, s)).collect();
            ctx.pop_scope();
            ctx.mk(IrExprKind::ForIn { var: var_id, var_tuple: tuple_vars, iterable: Box::new(ir_iter), body: ir_body }, ty, span)
        }
        ast::ExprKind::While { cond, body, .. } => {
            let ir_cond = lower_expr(ctx, cond);
            ctx.push_scope();
            let ir_body: Vec<IrStmt> = body.iter().map(|s| lower_stmt(ctx, s)).collect();
            ctx.pop_scope();
            ctx.mk(IrExprKind::While { cond: Box::new(ir_cond), body: ir_body }, ty, span)
        }
        ast::ExprKind::Break => ctx.mk(IrExprKind::Break, Ty::Unit, span),
        ast::ExprKind::Continue => ctx.mk(IrExprKind::Continue, Ty::Unit, span),
        ast::ExprKind::Range { start, end, inclusive, .. } => {
            let s = lower_expr(ctx, start);
            let e = lower_expr(ctx, end);
            ctx.mk(IrExprKind::Range { start: Box::new(s), end: Box::new(e), inclusive: *inclusive }, ty, span)
        }

        // ── Calls ──
        ast::ExprKind::Call { callee, args, named_args, type_args, .. } => {
            lower_call(ctx, callee, args, named_args, type_args.as_ref(), ty, span)
        }

        // ── Pipe: desugar `a |> f(b)` → `f(a, b)` ──
        ast::ExprKind::Pipe { left, right, .. } => {
            lower_pipe(ctx, left, right, ty, span)
        }

        // ── Compose: desugar `f >> g` → `(x) => g(f(x))` ──
        ast::ExprKind::Compose { left, right, .. } => {
            let ir_left = lower_expr(ctx, left);
            let ir_right = lower_expr(ctx, right);
            // Extract types: left is Fn[A] -> B, right is Fn[B] -> C
            let (param_ty, mid_ty) = match &ir_left.ty {
                Ty::Fn { params, ret } => (
                    params.first().cloned().unwrap_or(Ty::Unknown),
                    *ret.clone(),
                ),
                _ => (Ty::Unknown, Ty::Unknown),
            };
            let ret_ty = match &ir_right.ty {
                Ty::Fn { ret, .. } => *ret.clone(),
                _ => Ty::Unknown,
            };
            ctx.push_scope();
            let param_var = ctx.define_var("__compose_x", param_ty.clone(), Mutability::Let, span.clone());
            let param_ref = ctx.mk(IrExprKind::Var { id: param_var }, param_ty.clone(), span.clone());
            // f(x)
            let f_call = ctx.mk(IrExprKind::Call {
                target: CallTarget::Computed { callee: Box::new(ir_left) },
                args: vec![param_ref], type_args: vec![],
            }, mid_ty, span.clone());
            // g(f(x))
            let g_call = ctx.mk(IrExprKind::Call {
                target: CallTarget::Computed { callee: Box::new(ir_right) },
                args: vec![f_call], type_args: vec![],
            }, ret_ty.clone(), span.clone());
            ctx.pop_scope();
            let lambda_id = Some(ctx.next_lambda_id());
            let lambda_ty = Ty::Fn { params: vec![param_ty.clone()], ret: Box::new(ret_ty) };
            ctx.mk(IrExprKind::Lambda {
                params: vec![(param_var, param_ty)],
                body: Box::new(g_call),
                lambda_id,
            }, lambda_ty, span)
        }

        // ── Lambda ──
        ast::ExprKind::Lambda { params, body, .. } => {
            ctx.push_scope();
            // Get lambda type from checker to resolve inferred param types
            let lambda_param_tys: Vec<Ty> = match &ty {
                Ty::Fn { params: ptys, .. } => ptys.clone(),
                _ => vec![],
            };
            let ir_params: Vec<(VarId, Ty)> = params.iter().enumerate().map(|(i, p)| {
                let param_ty = p.ty.as_ref().map(|te| resolve_type_expr(te))
                    .or_else(|| lambda_param_tys.get(i).cloned())
                    .unwrap_or(Ty::Unknown);
                let var = ctx.define_var(&p.name, param_ty.clone(), Mutability::Let, None);
                (var, param_ty)
            }).collect();
            let ir_body = lower_expr(ctx, body);
            ctx.pop_scope();
            let lambda_id = Some(ctx.next_lambda_id());
            ctx.mk(IrExprKind::Lambda { params: ir_params, body: Box::new(ir_body), lambda_id }, ty, span)
        }

        // ── Access ──
        ast::ExprKind::Member { object, field, .. } => {
            // Module function used as first-class value: `string.len` →
            // lowered to a wrapper lambda `(x) => string.len(x)`. This lets
            // user code write `list.map(xs, string.len)` without a manual
            // eta expansion.
            if let ast::ExprKind::Ident { name: mod_name, .. } = &object.kind {
                if let Ty::Fn { params, ret } = &ty {
                    let resolved_mod_for_fn = ctx.env.import_table.resolve(mod_name)
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| mod_name.to_string());
                    let is_module_fn = crate::stdlib::lookup_sig(mod_name, field).is_some()
                        || ctx.env.functions.contains_key(&sym(&format!("{}.{}", resolved_mod_for_fn, field)))
                        || ctx.env.user_modules.contains(&sym(mod_name))
                        || ctx.env.import_table.aliases.contains_key(&sym(mod_name));
                    if is_module_fn {
                        return eta_expand_module_fn(ctx, *mod_name, *field, params.clone(), (**ret).clone(), span);
                    }
                }
                // Cross-module top-level `let` access: `utils.CATEGORY_ORDER`.
                if let Some((var_id, def_id)) = module_top_let_var(ctx, *mod_name, *field, &ty) {
                    if let Some(def_id) = def_id {
                        return ctx.mk_def(IrExprKind::Var { id: var_id }, ty, span, def_id);
                    }
                    return ctx.mk(IrExprKind::Var { id: var_id }, ty, span);
                }

                // Cross-module variant constructor as value: dispatch.Never, binary.ImportFunc
                if let Some((type_name, case)) = ctx.env.lookup_ctor(field) {
                    let resolved = ctx.env.import_table.aliases.get(mod_name).copied()
                        .unwrap_or(*mod_name);
                    let qualified = format!("{}.{}", resolved.as_str(), type_name.as_str());
                    if ctx.env.types.contains_key(&sym(&qualified)) {
                        // Constructor with payload as function value → generate lambda
                        if let crate::types::VariantPayload::Tuple(ref param_tys) = case.payload {
                            if !param_tys.is_empty() && matches!(&ty, Ty::Fn { .. }) {
                                let params: Vec<(VarId, Ty)> = param_tys.iter().enumerate().map(|(i, pt)| {
                                    let vid = ctx.var_table.alloc(sym(&format!("_ctor_arg{}", i)), pt.clone(), Mutability::Let, None);
                                    (vid, pt.clone())
                                }).collect();
                                let ctor_args: Vec<IrExpr> = params.iter().map(|(vid, pt)| {
                                    ctx.mk(IrExprKind::Var { id: *vid }, pt.clone(), span)
                                }).collect();
                                let ret_ty = match &ty {
                                    Ty::Fn { ret, .. } => ret.as_ref().clone(),
                                    _ => ty.clone(),
                                };
                                let body = ctx.mk(IrExprKind::Call {
                                    target: CallTarget::Named { name: *field },
                                    args: ctor_args, type_args: vec![],
                                }, ret_ty, span);
                                return ctx.mk(IrExprKind::Lambda {
                                    params, body: Box::new(body), lambda_id: None,
                                }, ty, span);
                            }
                        }
                        // No-payload constructor: emit as Call
                        return ctx.mk(IrExprKind::Call {
                            target: CallTarget::Named { name: *field },
                            args: vec![], type_args: vec![],
                        }, ty, span);
                    }
                }
            }
            let obj = lower_expr(ctx, object);
            ctx.mk(IrExprKind::Member { object: Box::new(obj), field: *field }, ty, span)
        }
        ast::ExprKind::TupleIndex { object, index, .. } => {
            let obj = lower_expr(ctx, object);
            ctx.mk(IrExprKind::TupleIndex { object: Box::new(obj), index: *index }, ty, span)
        }
        ast::ExprKind::IndexAccess { object, index, .. } => {
            // Range index → slice desugaring
            if let ast::ExprKind::Range { start, end, inclusive } = &index.kind {
                let obj = lower_expr(ctx, object);
                let start_expr = lower_expr(ctx, start);
                let end_expr = lower_expr(ctx, end);
                let end_final = if *inclusive {
                    // ..= inclusive: end + 1 for exclusive slice
                    ctx.mk(IrExprKind::BinOp {
                        op: BinOp::AddInt,
                        left: Box::new(end_expr),
                        right: Box::new(ctx.mk(IrExprKind::LitInt { value: 1 }, Ty::Int, span)),
                    }, Ty::Int, span)
                } else {
                    end_expr
                };
                let symbol = if matches!(obj.ty, Ty::Bytes) {
                    "almide_rt_bytes_slice"
                } else {
                    "almide_rt_list_slice"
                };
                ctx.mk(IrExprKind::RuntimeCall {
                    symbol: sym(symbol),
                    args: vec![obj, start_expr, end_final],
                }, ty, span)
            } else {
                let obj = lower_expr(ctx, object);
                let idx = lower_expr(ctx, index);
                if obj.ty.is_map() {
                    ctx.mk(IrExprKind::MapAccess { object: Box::new(obj), key: Box::new(idx) }, ty, span)
                } else {
                    ctx.mk(IrExprKind::IndexAccess { object: Box::new(obj), index: Box::new(idx) }, ty, span)
                }
            }
        }

        // ── String interpolation ──
        ast::ExprKind::InterpolatedString { parts, .. } => {
            let ir_parts = parts.iter().map(|part| match part {
                ast::StringPart::Lit { value } => IrStringPart::Lit { value: value.clone() },
                ast::StringPart::Expr { expr } => {
                    let mut ir_expr = lower_expr(ctx, expr);
                    // Operator protocol: dispatch to an EXPLICIT user `repr` only.
                    // An auto-derived `repr` is intentionally NOT used here — the
                    // record/variant instead falls through to the codegen
                    // `AlmideRepr` impl (the canonical literal form with quoted
                    // strings), so an auto-derived and a plain record interpolate
                    // byte-identically. An explicit `fn X.repr` still wins.
                    if let Some(repr_fn) = ctx.find_explicit_convention_fn(&ir_expr.ty, "repr") {
                        ir_expr = ctx.mk(IrExprKind::Call {
                            target: CallTarget::Named { name: repr_fn },
                            args: vec![ir_expr], type_args: vec![],
                        }, Ty::String, None);
                    }
                    IrStringPart::Expr { expr: ir_expr }
                }
            }).collect();
            ctx.mk(IrExprKind::StringInterp { parts: ir_parts }, Ty::String, span)
        }

        // ── Result / Option ──
        ast::ExprKind::Some { expr, .. } => {
            let inner = lower_expr(ctx, expr);
            ctx.mk(IrExprKind::OptionSome { expr: Box::new(inner) }, ty, span)
        }
        ast::ExprKind::Ok { expr, .. } => {
            let inner = lower_expr(ctx, expr);
            ctx.mk(IrExprKind::ResultOk { expr: Box::new(inner) }, ty, span)
        }
        ast::ExprKind::Err { expr, .. } => {
            let inner = lower_expr(ctx, expr);
            ctx.mk(IrExprKind::ResultErr { expr: Box::new(inner) }, ty, span)
        }
        ast::ExprKind::None => ctx.mk(IrExprKind::OptionNone, ty, span),
        ast::ExprKind::Try { expr, .. } => {
            let inner = lower_expr(ctx, expr);
            ctx.mk(IrExprKind::Try { expr: Box::new(inner) }, ty, span)
        }
        ast::ExprKind::Await { expr, .. } => {
            let inner = lower_expr(ctx, expr);
            ctx.mk(IrExprKind::Await { expr: Box::new(inner) }, ty, span)
        }

        // expr! — keep as Unwrap (distinct from auto-? Try)
        ast::ExprKind::Unwrap { expr, .. } => {
            let inner = lower_expr(ctx, expr);
            ctx.mk(IrExprKind::Unwrap { expr: Box::new(inner) }, ty, span)
        }
        // expr ?? fallback — lower to match: ok(v)/some(v) → v, else → fallback
        ast::ExprKind::UnwrapOr { expr, fallback, .. } => {
            let inner = lower_expr(ctx, expr);
            let fb = lower_expr(ctx, fallback);
            // For now, use a dedicated UnwrapOr node if it exists, otherwise fallback to Call
            ctx.mk(IrExprKind::UnwrapOr { expr: Box::new(inner), fallback: Box::new(fb) }, ty, span)
        }
        // expr? — lower to ToOption
        ast::ExprKind::ToOption { expr, .. } => {
            let inner = lower_expr(ctx, expr);
            ctx.mk(IrExprKind::ToOption { expr: Box::new(inner) }, ty, span)
        }
        // expr?.field — keep as IR node for target-specific rendering
        ast::ExprKind::OptionalChain { expr: inner_expr, field, .. } => {
            let inner = lower_expr(ctx, inner_expr);
            ctx.mk(IrExprKind::OptionalChain { expr: Box::new(inner), field: *field }, ty, span)
        }

        // ── Misc ──
        ast::ExprKind::Paren { expr, .. } => lower_expr(ctx, expr),
        ast::ExprKind::TypeAscription { expr, ty: ascribed_te } => {
            // The ascription pins the inner expression's type (`[]: List[Int]`).
            // Lower the inner expr, then adopt the ascribed type when the inner
            // came back less resolved — an empty collection literal otherwise
            // carries an unresolved element type, which codegen renders as an
            // uninferable `Vec::<_>::new()` (native E0282) under `almide_repr`.
            // The annotation's own `TypeExpr` is the authoritative source: the
            // checker's resolved type-map entry for the ascription can still be
            // an unresolved `List[?]` when nothing outside the annotation
            // constrained the element.
            let mut inner = lower_expr(ctx, expr);
            if inner.ty.has_unresolved_deep() {
                let ascribed = resolve_type_expr(ascribed_te);
                if !ascribed.has_unresolved_deep() {
                    inner.ty = ascribed;
                } else if !ty.has_unresolved_deep() {
                    inner.ty = ty;
                }
            }
            inner
        }
        ast::ExprKind::Hole => ctx.mk(IrExprKind::Hole, ty, span),
        ast::ExprKind::Todo { message, .. } => ctx.mk(IrExprKind::Todo { message: message.clone() }, ty, span),
        ast::ExprKind::Error => ctx.mk(IrExprKind::Unit, Ty::Unknown, span),
        ast::ExprKind::Placeholder => ctx.mk(IrExprKind::Unit, Ty::Unknown, span),
    }
}

/// Lower a block body (stmts + optional tail), desugaring `guard let`. A `guard let
/// name = scrutinee else { alt }` binds `name` for the REST of the block, so everything
/// after it (the remaining stmts + the tail) becomes the Some/Ok arm of a match on the
/// scrutinee, and `alt` the wildcard arm. Statements before the guard stay as block
/// stmts. Recurses so multiple guard-lets nest. Without a guard-let it lowers normally.
/// The caller owns the block scope (push/pop around this).
fn lower_block_body(
    ctx: &mut LowerCtx,
    stmts: &[ast::Stmt],
    tail: Option<&ast::Expr>,
    ty: &Ty,
    span: Option<ast::Span>,
) -> IrExpr {
    if let Some(i) = stmts.iter().position(|s| matches!(s, ast::Stmt::GuardLet { .. })) {
        let pre: Vec<IrStmt> = stmts[..i].iter().map(|s| lower_stmt(ctx, s)).collect();
        let (name, scrutinee, else_) = match &stmts[i] {
            ast::Stmt::GuardLet { name, scrutinee, else_, .. } => (*name, scrutinee, else_),
            _ => unreachable!(),
        };
        let s = lower_expr(ctx, scrutinee);
        let subject_ty = if let IrExprKind::Var { id } = &s.kind {
            let vt_ty = &ctx.var_table.get(*id).ty;
            if matches!(vt_ty, Ty::Applied(_, _)) && !matches!(&s.ty, Ty::Applied(_, _)) {
                vt_ty.clone()
            } else {
                s.ty.clone()
            }
        } else {
            s.ty.clone()
        };
        let s = if subject_ty != s.ty { IrExpr { ty: subject_ty.clone(), ..s } } else { s };
        let inner = ast::Pattern::Ident { name };
        let bind_pat = match &subject_ty {
            Ty::Applied(TypeConstructorId::Result, _) => {
                ast::Pattern::Ok { inner: Box::new(inner) }
            }
            _ => ast::Pattern::Some { inner: Box::new(inner) },
        };
        // Some/Ok arm: bind name, then the rest of the block (recurse for nested guards).
        ctx.push_scope();
        let pat1 = lower_pattern(ctx, &bind_pat, &subject_ty);
        let rest = lower_block_body(ctx, &stmts[i + 1..], tail, ty, span);
        ctx.pop_scope();
        let arm1 = IrMatchArm { pattern: pat1, guard: None, body: rest };
        // Wildcard arm: the else branch (must diverge).
        ctx.push_scope();
        let pat2 = lower_pattern(ctx, &ast::Pattern::Wildcard, &subject_ty);
        let alt = lower_expr(ctx, else_);
        ctx.pop_scope();
        let arm2 = IrMatchArm { pattern: pat2, guard: None, body: alt };
        let match_expr =
            ctx.mk(IrExprKind::Match { subject: Box::new(s), arms: vec![arm1, arm2] }, ty.clone(), span);
        ctx.mk(IrExprKind::Block { stmts: pre, expr: Some(Box::new(match_expr)) }, ty.clone(), span)
    } else {
        let ir_stmts: Vec<IrStmt> = stmts.iter().map(|s| lower_stmt(ctx, s)).collect();
        let ir_expr = tail.map(|e| Box::new(lower_expr(ctx, e)));
        ctx.mk(IrExprKind::Block { stmts: ir_stmts, expr: ir_expr }, ty.clone(), span)
    }
}

/// Lower pipe expression, unwrapping postfix operators (??, !, ?) on the RHS
/// so the pipe targets the inner Call. e.g. `xs |> list.find(p) ?? fallback`
/// becomes `list.find(xs, p) ?? fallback` rather than treating `??` as part of the pipe target.
fn lower_pipe(ctx: &mut LowerCtx, left: &ast::Expr, right: &ast::Expr, ty: Ty, span: Option<ast::Span>) -> IrExpr {
    match &right.kind {
        // Transparent postfix: pipe into inner, then wrap with the operator
        ast::ExprKind::UnwrapOr { expr: inner, fallback, .. } => {
            // The inner pipe result is Option[ty] or Result[ty, _]; codegen needs the wrapper
            // type on the piped expression to generate correct match (Some/None vs Ok/Err).
            // Use the checker's resolved type for the inner expression.
            let inner_checked_ty = ctx.expr_ty(inner);
            let is_wrapper = inner_checked_ty.is_option()
                || matches!(inner_checked_ty, Ty::Applied(TypeConstructorId::Result, _));
            let inner_ty = if is_wrapper {
                inner_checked_ty
            } else {
                Ty::Applied(TypeConstructorId::Option, vec![ty.clone()])
            };
            let piped = lower_pipe(ctx, left, inner, inner_ty, span.clone());
            let ir_fallback = lower_expr(ctx, fallback);
            ctx.mk(IrExprKind::UnwrapOr { expr: Box::new(piped), fallback: Box::new(ir_fallback) }, ty, span)
        }
        ast::ExprKind::Unwrap { expr: inner, .. } => {
            // Use the checker's resolved type for the inner expression.
            // This preserves the actual error type (e.g., List[String] from result.collect)
            // instead of hardcoding String.
            let inner_checked_ty = ctx.expr_ty(inner);
            let inner_ty = if inner_checked_ty.is_result() || inner_checked_ty.is_option() {
                inner_checked_ty
            } else {
                Ty::result(ty.clone(), Ty::String)
            };
            let piped = lower_pipe(ctx, left, inner, inner_ty, span.clone());
            ctx.mk(IrExprKind::Unwrap { expr: Box::new(piped) }, ty, span)
        }
        ast::ExprKind::Try { expr: inner, .. } => {
            let piped = lower_pipe(ctx, left, inner, ty.clone(), span.clone());
            ctx.mk(IrExprKind::ToOption { expr: Box::new(piped) }, ty, span)
        }

        // Direct pipe targets
        ast::ExprKind::Call { callee, args, type_args, .. } => {
            let ir_left = lower_expr(ctx, left);
            let mut all_args = vec![ir_left];
            all_args.extend(args.iter().map(|a| lower_expr(ctx, a)));
            let target = lower_call_target(ctx, callee);
            let ta = type_args.as_ref().map(|tas| tas.iter().map(|t| resolve_type_expr(t)).collect()).unwrap_or_default();
            let resolved_ty = if matches!(ty, Ty::Unknown) {
                if let CallTarget::Named { name } = &target {
                    ctx.env.functions.get(name).map(|f| f.ret.clone()).unwrap_or(ty)
                } else { ty }
            } else { ty };
            ctx.mk(IrExprKind::Call { target, args: all_args, type_args: ta }, resolved_ty, span)
        }
        ast::ExprKind::Ident { .. } | ast::ExprKind::Member { .. } => {
            let ir_left = lower_expr(ctx, left);
            let target = lower_call_target(ctx, right);
            ctx.mk(IrExprKind::Call { target, args: vec![ir_left], type_args: vec![] }, ty, span)
        }
        // `a |> (n) => body` — INLINE the immediately-applied lambda to `{ let n = a; body }`.
        // A pipe RHS lambda is applied exactly once, so binding its single param to the piped value
        // and evaluating the body is identical on BOTH targets — and it avoids a Computed-callee
        // call, which v1 MIR cannot lower as a first-class closure (it silently mis-lowered
        // `5 |> (n) => n * n` to 0). Multi-param / zero-param lambdas keep the Computed-call form.
        ast::ExprKind::Lambda { params, body, .. } if params.len() == 1 => {
            let ir_left = lower_expr(ctx, left);
            let p = &params[0];
            let param_ty = p
                .ty
                .as_ref()
                .map(|te| resolve_type_expr(te))
                .unwrap_or_else(|| ctx.expr_ty(left));
            ctx.push_scope();
            let var = ctx.define_var(&p.name, param_ty.clone(), Mutability::Let, span.clone());
            let ir_body = lower_expr(ctx, body);
            ctx.pop_scope();
            let bind = IrStmt {
                kind: IrStmtKind::Bind {
                    var,
                    mutability: Mutability::Let,
                    ty: param_ty,
                    value: ir_left,
                },
                span: span.clone(),
            };
            ctx.mk(IrExprKind::Block { stmts: vec![bind], expr: Some(Box::new(ir_body)) }, ty, span)
        }
        _ => {
            let ir_left = lower_expr(ctx, left);
            let ir_right = lower_expr(ctx, right);
            ctx.mk(IrExprKind::Call {
                target: CallTarget::Computed { callee: Box::new(ir_right) },
                args: vec![ir_left], type_args: vec![],
            }, ty, span)
        }
    }
}

/// Eta-expand a module function reference (`string.len`, `list.map`, ...)
/// into a lambda that calls it. Used when the reference appears in value
/// position rather than as a callee, e.g. `xs |> list.map(string.len)`.
fn eta_expand_module_fn(
    ctx: &mut LowerCtx,
    module: almide_base::intern::Sym,
    field: almide_base::intern::Sym,
    params: Vec<Ty>,
    ret_ty: Ty,
    span: Option<ast::Span>,
) -> IrExpr {
    ctx.push_scope();
    let mut param_vars: Vec<(VarId, Ty)> = Vec::with_capacity(params.len());
    for (i, pt) in params.iter().enumerate() {
        let name = format!("__eta_{}", i);
        let var = ctx.define_var(&name, pt.clone(), Mutability::Let, span.clone());
        param_vars.push((var, pt.clone()));
    }
    let args: Vec<IrExpr> = param_vars.iter()
        .map(|(var, pt)| ctx.mk(IrExprKind::Var { id: *var }, pt.clone(), span.clone()))
        .collect();
    // For stdlib modules (e.g. `string`) use CallTarget::Module so codegen
    // picks the stdlib runtime function. For user convention methods
    // (`Type.method`) use CallTarget::Named with the dotted key.
    let mod_name = module.as_str();
    let target = if crate::stdlib::is_stdlib_module(mod_name)
        || crate::stdlib::is_any_stdlib(mod_name)
        || ctx.env.user_modules.contains(&module)
        || ctx.env.import_table.aliases.contains_key(&module)
    {
        let resolved = ctx.env.import_table.aliases.get(&module).copied().unwrap_or(module);
        CallTarget::Module { module: resolved, func: field, def_id: ctx.def_map.get(&sym(&format!("{}.{}", resolved, field))).copied() }
    } else {
        CallTarget::Named { name: sym(&format!("{}.{}", module, field)) }
    };
    let call = ctx.mk(IrExprKind::Call {
        target, args, type_args: vec![],
    }, ret_ty.clone(), span.clone());
    ctx.pop_scope();
    let lambda_id = Some(ctx.next_lambda_id());
    let lambda_ty = Ty::Fn {
        params: params.clone(),
        ret: Box::new(ret_ty),
    };
    ctx.mk(IrExprKind::Lambda {
        params: param_vars,
        body: Box::new(call),
        lambda_id,
    }, lambda_ty, span)
}

/// Resolve `mod.NAME` against the cross-module top-let table and build the
/// synthetic use-site Var: CLEAN uppercase name in the IR, `module_origin`
/// carrying the (versioned) module for emit-time prefixing. ONE rule shared
/// by every syntactic position that references a module top-let — reads
/// (`Member`) and assignment lvalues (`m.x = v`, #505); a position that
/// re-derives this resolution is a #500-class hole waiting to happen.
pub(super) fn module_top_let_var(
    ctx: &mut LowerCtx,
    mod_name: almide_base::intern::Sym,
    field: almide_base::intern::Sym,
    ty: &Ty,
) -> Option<(VarId, Option<almide_ir::DefId>)> {
    let resolved_mod = ctx.env.import_table.resolve(&mod_name)
        .map(|s| s.to_string())
        .unwrap_or_else(|| mod_name.to_string());
    let qual_let_key = format!("{}.{}", resolved_mod, field);
    if !ctx.env.top_lets.contains_key(&sym(&qual_let_key)) {
        return None;
    }
    // Use the versioned module name if available (e.g. "snaidhm_v0.web.gpu")
    // to match the constant definition generated by lower_module. Exact
    // match first, then walk up parent segments to the package root (only
    // root modules have pkg_id → versioned name).
    let mod_ident = ctx.env.module_versioned_names.get(&sym(&resolved_mod))
        .map(|s| s.as_str().to_string())
        .or_else(|| {
            let parts: Vec<&str> = resolved_mod.split('.').collect();
            for i in (1..parts.len()).rev() {
                let prefix = parts[..i].join(".");
                if let Some(versioned) = ctx.env.module_versioned_names.get(&sym(&prefix)) {
                    let suffix = &resolved_mod[prefix.len()..];
                    return Some(format!("{}{}", versioned.as_str(), suffix));
                }
            }
            None
        })
        .unwrap_or_else(|| resolved_mod.clone());
    let clean_name = field.as_str().to_uppercase();
    let origin = mod_ident.replace('.', "_");
    let var_id = ctx.var_table.alloc(sym(&clean_name), ty.clone(), Mutability::Let, None);
    ctx.var_table.entries[var_id.0 as usize].module_origin = Some(origin);
    let def_id = ctx.def_map.get(&sym(&qual_let_key)).copied();
    Some((var_id, def_id))
}

