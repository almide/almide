// Continuation of expressions.rs — per-ExprKind lowering helpers dispatched
// from `lower_expr`'s router (InterpolatedString/IndexAccess/Member/Compose/
// ForIn/IfLet/MatchArm/Unary/Binary/Record/TypeName/Ident). Split out to keep
// expressions.rs under the 800-line codopsy max-lines threshold; pure text
// move, same file scope via `include!` (inherits expressions.rs's `use`
// imports — mirrors the mod_p2.rs/mod_p3.rs pattern already used elsewhere
// in this crate).



fn lower_expr_interp_string(ctx: &mut LowerCtx, expr: &ast::Expr, _ty: Ty, span: Option<crate::ast::Span>) -> IrExpr {
    let ast::ExprKind::InterpolatedString { parts, .. } = &expr.kind else { unreachable!("lower_expr_interp_string called on the wrong ExprKind") };
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

fn lower_expr_index_access(ctx: &mut LowerCtx, expr: &ast::Expr, ty: Ty, span: Option<crate::ast::Span>) -> IrExpr {
    let ast::ExprKind::IndexAccess { object, index, .. } = &expr.kind else { unreachable!("lower_expr_index_access called on the wrong ExprKind") };
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

fn lower_expr_member(ctx: &mut LowerCtx, expr: &ast::Expr, ty: Ty, span: Option<crate::ast::Span>) -> IrExpr {
    let ast::ExprKind::Member { object, field, .. } = &expr.kind else { unreachable!("lower_expr_member called on the wrong ExprKind") };
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

fn lower_expr_compose(ctx: &mut LowerCtx, expr: &ast::Expr, _ty: Ty, span: Option<crate::ast::Span>) -> IrExpr {
    let ast::ExprKind::Compose { left, right, .. } = &expr.kind else { unreachable!("lower_expr_compose called on the wrong ExprKind") };
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

fn lower_expr_for_in(ctx: &mut LowerCtx, expr: &ast::Expr, ty: Ty, span: Option<crate::ast::Span>) -> IrExpr {
    let ast::ExprKind::ForIn { var, var_tuple, iterable, body, .. } = &expr.kind else { unreachable!("lower_expr_for_in called on the wrong ExprKind") };
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

fn lower_expr_if_let(ctx: &mut LowerCtx, expr: &ast::Expr, ty: Ty, span: Option<crate::ast::Span>) -> IrExpr {
    let ast::ExprKind::IfLet { name, scrutinee, then, else_ } = &expr.kind else { unreachable!("lower_expr_if_let called on the wrong ExprKind") };
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

fn lower_expr_match_arm(ctx: &mut LowerCtx, expr: &ast::Expr, ty: Ty, span: Option<crate::ast::Span>) -> IrExpr {
    let ast::ExprKind::Match { subject, arms, .. } = &expr.kind else { unreachable!("lower_expr_match_arm called on the wrong ExprKind") };
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

fn lower_expr_unary(ctx: &mut LowerCtx, expr: &ast::Expr, ty: Ty, span: Option<crate::ast::Span>) -> IrExpr {
    let ast::ExprKind::Unary { op, operand, .. } = &expr.kind else { unreachable!("lower_expr_unary called on the wrong ExprKind") };
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

fn lower_expr_binary(ctx: &mut LowerCtx, expr: &ast::Expr, ty: Ty, span: Option<crate::ast::Span>) -> IrExpr {
    let ast::ExprKind::Binary { op, left, right, .. } = &expr.kind else { unreachable!("lower_expr_binary called on the wrong ExprKind") };
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

fn lower_expr_record(ctx: &mut LowerCtx, expr: &ast::Expr, ty: Ty, span: Option<crate::ast::Span>) -> IrExpr {
    let ast::ExprKind::Record { name, fields, .. } = &expr.kind else { unreachable!("lower_expr_record called on the wrong ExprKind") };
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

fn lower_expr_type_name(ctx: &mut LowerCtx, expr: &ast::Expr, ty: Ty, span: Option<crate::ast::Span>) -> IrExpr {
    let ast::ExprKind::TypeName { name, .. } = &expr.kind else { unreachable!("lower_expr_type_name called on the wrong ExprKind") };
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

fn lower_expr_ident(ctx: &mut LowerCtx, expr: &ast::Expr, ty: Ty, span: Option<crate::ast::Span>) -> IrExpr {
    let ast::ExprKind::Ident { name, .. } = &expr.kind else { unreachable!("lower_expr_ident called on the wrong ExprKind") };
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
