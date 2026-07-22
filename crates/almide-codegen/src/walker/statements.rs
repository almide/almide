//! Statement and pattern rendering: converts IrStmt and IrPattern nodes
//! to target-specific code strings.

use almide_ir::*;
use almide_ir::annotations::VarStorage;
use almide_lang::types::{Ty, TypeConstructorId};
use super::RenderContext;
use super::types::render_type;
use super::expressions::render_expr;
use super::helpers::{template_or, terminate_stmt, ty_has_named_typevar, erase_named_typevars, erase_fn_types};

/// When a binding's initializer reads a fn param that is emitted as a Rust
/// reference (`&str`, `&[T]`, `&AlmideMap<..>`, `&T`), the binding's declared
/// type is the OWNED form (`String`, `Vec<T>`, `AlmideMap<..>`, `T`), so the
/// borrow must be converted to an owned value. The right method depends on the
/// value type: a `&str` owns via `.to_string()`, a `&[T]` slice via `.to_vec()`
/// (`.clone()` on a slice yields another `&[T]`, not `Vec<T>`), and every other
/// borrowed type (`&AlmideMap`, `&T`) owns via `.clone()`. Returns the bare
/// owning expression (e.g. `l.to_vec()`) when the value reads a borrowed param,
/// or `None` when no conversion is needed. A `Clone{Var}` initializer is
/// rendered from the bare var so we never stack `l.clone().to_vec()`. #624
fn borrowed_param_owning_value(ctx: &RenderContext, value: &IrExpr) -> Option<String> {
    let (var_id, val_ty) = match &value.kind {
        IrExprKind::Var { id } => (*id, &value.ty),
        IrExprKind::Clone { expr: inner } => match &inner.kind {
            IrExprKind::Var { id } => (*id, &inner.ty),
            _ => return None,
        },
        _ => return None,
    };
    if !ctx.ref_params.contains(&var_id) {
        return None;
    }
    let conv = match val_ty {
        Ty::String => "to_string",
        Ty::Applied(TypeConstructorId::List, _) => "to_vec",
        _ => "clone",
    };
    Some(format!("{}.{}()", ctx.var_name(var_id), conv))
}

/// Check if an expression references a specific variable (any depth).
pub fn render_stmt(ctx: &RenderContext, stmt: &IrStmt) -> String {
    match &stmt.kind {
        IrStmtKind::Bind { .. } => render_stmt_bind(ctx, stmt),
        IrStmtKind::Assign { .. } => render_stmt_assign(ctx, stmt),
        IrStmtKind::Expr { expr } => {
            let rendered = render_expr(ctx, expr);
            terminate_stmt(ctx, rendered)
        }
        IrStmtKind::Guard { .. } => render_stmt_guard(ctx, stmt),
        IrStmtKind::IndexAssign { .. } => render_stmt_index_assign(ctx, stmt),
        IrStmtKind::MapInsert { .. } => render_stmt_map_insert(ctx, stmt),
        IrStmtKind::FieldAssign { .. } => render_stmt_field_assign(ctx, stmt),
        IrStmtKind::BindDestructure { .. } => render_stmt_bind_destructure(ctx, stmt),
        IrStmtKind::ListSwap { target, a, b } => {
            let t = ctx.var_name(*target).to_string();
            let upper = ctx.global_static_name(*target);
            let a_s = render_expr(ctx, a);
            let b_s = render_expr(ctx, b);
            if let Some(info) = ctx.ann.global(*target) {
                use almide_ir::top_let_storage::TopLetStorage as Tls;
                return match info.storage {
                    Tls::RcRefCell => format!("{}.with(|c| std::rc::Rc::make_mut(&mut *c.borrow_mut()).swap({} as usize, {} as usize));", info.static_name, a_s, b_s),
                    other => unreachable!("[COMPILER BUG] list-swap on {:?} global `{}`", other, info.static_name),
                };
            }
            match ctx.ann.get_var_storage(target) {
                VarStorage::RcCow => format!("{}.make_mut().swap({} as usize, {} as usize);", t, a_s, b_s),
                _ => ctx.templates.render_with("peep_swap", None, &[], &[("target", &t), ("a", &a_s), ("b", &b_s)])
                    .unwrap_or_else(|| format!("{}.swap({}, {});", t, a_s, b_s)),
            }
        }
        IrStmtKind::ListReverse { target, end } => {
            let t = ctx.var_name(*target).to_string();
            let upper = ctx.global_static_name(*target);
            let e = render_expr(ctx, end);
            if let Some(info) = ctx.ann.global(*target) {
                use almide_ir::top_let_storage::TopLetStorage as Tls;
                return match info.storage {
                    Tls::RcRefCell => format!("{}.with(|c| std::rc::Rc::make_mut(&mut *c.borrow_mut())[..={} as usize].reverse());", info.static_name, e),
                    other => unreachable!("[COMPILER BUG] list-reverse on {:?} global `{}`", other, info.static_name),
                };
            }
            match ctx.ann.get_var_storage(target) {
                VarStorage::RcCow => format!("{}.make_mut()[..={} as usize].reverse();", t, e),
                _ => ctx.templates.render_with("peep_reverse", None, &[], &[("target", &t), ("end", &e)])
                    .unwrap_or_else(|| format!("{}[..={} as usize].reverse();", t, e)),
            }
        }
        IrStmtKind::ListRotateLeft { target, end } => {
            let t = ctx.var_name(*target).to_string();
            let upper = ctx.global_static_name(*target);
            let e = render_expr(ctx, end);
            if let Some(info) = ctx.ann.global(*target) {
                use almide_ir::top_let_storage::TopLetStorage as Tls;
                return match info.storage {
                    Tls::RcRefCell => format!("{}.with(|c| std::rc::Rc::make_mut(&mut *c.borrow_mut())[..={} as usize].rotate_left(1));", info.static_name, e),
                    other => unreachable!("[COMPILER BUG] list-rotate on {:?} global `{}`", other, info.static_name),
                };
            }
            match ctx.ann.get_var_storage(target) {
                VarStorage::RcCow => format!("{}.make_mut()[..={} as usize].rotate_left(1);", t, e),
                _ => ctx.templates.render_with("peep_rotate_left", None, &[], &[("target", &t), ("end", &e)])
                    .unwrap_or_else(|| format!("{}[..={} as usize].rotate_left(1);", t, e)),
            }
        }
        IrStmtKind::ListCopySlice { dst, src, len } => {
            let d = ctx.var_name(*dst).to_string();
            let s = ctx.var_name(*src).to_string();
            let upper_d = ctx.global_static_name(*dst);
            let n = render_expr(ctx, len);
            // §4 Stage 2: both the dst write AND the src re-read dispatch on
            // the attribute (the src probe was the 9th hand-rolled copy of
            // the ModuleRc protocol).
            let src_read = match ctx.ann.global(*src) {
                Some(si) if matches!(si.storage, almide_ir::top_let_storage::TopLetStorage::RcRefCell) =>
                    format!("{}.with(|c| c.borrow().clone())", si.static_name),
                _ => s.clone(),
            };
            if let Some(info) = ctx.ann.global(*dst) {
                use almide_ir::top_let_storage::TopLetStorage as Tls;
                return match info.storage {
                    Tls::RcRefCell => format!("{}.with(|c| std::rc::Rc::make_mut(&mut *c.borrow_mut())[..{n} as usize].copy_from_slice(&{src_read}[..{n} as usize]));", info.static_name, n=n, src_read=src_read),
                    other => unreachable!("[COMPILER BUG] copy-slice into {:?} global `{}`", other, info.static_name),
                };
            }
            match ctx.ann.get_var_storage(dst) {
                VarStorage::RcCow => format!("{}.make_mut()[..{} as usize].copy_from_slice(&{}[..{} as usize]);", d, n, src_read, n),
                _ => ctx.templates.render_with("peep_copy_slice", None, &[], &[("dst", &d), ("src", &src_read), ("n", &n)])
                    .unwrap_or_else(|| format!("{}[..{} as usize].copy_from_slice(&{}[..{} as usize]);", d, n, src_read, n)),
            }
        }
        IrStmtKind::Comment { text } => format!("// {}", text),
        // Perceus RC ops are WASM-only; Rust handles ownership natively.
        IrStmtKind::RcInc { .. } | IrStmtKind::RcDec { .. } => String::new(),
    }
}

// ── render_stmt arm extraction (cog>100 decomposition, pattern 2) ──
//
// The following helpers are 1:1 text-moves of the largest `render_stmt`
// match arms. Each re-narrows `stmt.kind` via `let-else` and returns the
// exact String the inline arm used to produce — no behavior change.

/// Shared-mut local (`Rc<Cell<T>>`, Closure v2 P3): a fresh cell at the
/// declaration, or an `Rc::clone` of the original for the `__cap_*`
/// capture rename (so the closure shares the cell). Extracted from
/// `render_stmt_bind` (cog>30 decomposition): `Some` mirrors the
/// original's early `return`, `None` means "not shared-mut, fall through".
fn try_render_bind_shared_mut(ctx: &RenderContext, var: &VarId, ty: &Ty, value: &IrExpr) -> Option<String> {
    if !ctx.ann.is_shared_mut(var) { return None; }
    let name_s = ctx.var_name(*var).to_string();
    // Copy scalars use `Rc<Cell<T>>` (P3); non-Copy values use `SharedMut`
    // (`Rc<RefCell<T>>`, P6). A `__cap_*` capture rename is an `Rc::clone`
    // of the original for either kind, so the closure shares the SAME cell.
    let is_copy = almide_ir::top_let_storage::capture_copy_cell(ty);
    let fresh_cell = |ctx: &RenderContext| if is_copy {
        format!("std::rc::Rc::new(std::cell::Cell::new({}))", render_expr(ctx, value))
    } else {
        format!("SharedMut::new({})", render_expr(ctx, value))
    };
    // A `__cap_N` capture rename is an `Rc::clone` of the original shared
    // cell. Its value is a bare `Var` or a `Clone{Var}` (CloneInsertionPass
    // wraps non-Copy values) — either way emit a single `.clone()` of the
    // cell so the closure shares it rather than allocating a fresh one.
    let cap_orig = if name_s.starts_with("__cap_") {
        match &value.kind {
            IrExprKind::Var { id } => Some(*id),
            IrExprKind::Clone { expr: inner } => match &inner.kind {
                IrExprKind::Var { id } => Some(*id),
                _ => None,
            },
            _ => None,
        }
    } else { None };
    let val_s = match cap_orig {
        Some(id) => format!("{}.clone()", ctx.var_name(id)),
        None => fresh_cell(ctx),
    };
    Some(format!("let {} = {};", name_s, val_s))
}

/// Resolve the `Ty` to render for a Bind statement: erase Fn types (Rust
/// can't write `impl Fn` in let position), aliases that resolve to Fn,
/// named typevars not in scope, and Fn types nested in containers.
/// Extracted from `render_stmt_bind` (cog>30 decomposition). The original
/// threaded a `&Ty` through several owned-buffer + reference-rebind steps
/// (relying on rvalue static promotion for `&Ty::Unknown`); this returns a
/// plain owned `Ty` at each step instead — same final value, no promotion
/// trick needed.
fn erase_bind_ty(ctx: &RenderContext, ty: &Ty) -> Ty {
    // List[Fn] Rc wrapping is now handled by RustLoweringPass
    // which inserts RcWrap nodes into the IR.
    // Erase Fn types in bindings (Rust can't write `impl Fn` in let position; TS gets `any`)
    // Also resolve aliases first — `type Handler = Fn(String) -> String` should erase too
    let ty: Ty = if matches!(ty, Ty::Fn { .. }) {
        Ty::Unknown
    } else if let Ty::Named(name, args) = ty {
        if args.is_empty() {
            if let Some(target) = ctx.type_aliases.get(name) {
                if matches!(target, Ty::Fn { .. }) {
                    Ty::Unknown
                } else {
                    target.clone()
                }
            } else {
                ty.clone()
            }
        } else {
            ty.clone()
        }
    } else {
        ty.clone()
    };
    // Erase named TypeVars (K, V, B) — not in scope for bindings
    let ty = if ty_has_named_typevar(&ty) {
        erase_named_typevars(ty)
    } else {
        ty
    };
    // Erase Fn types nested inside containers (tuple element, map value,
    // record field, ...). Rust forbids `impl Trait` in a binding type
    // (E0562), so `(impl Fn() -> () + Clone, i64)` is illegal; rewrite the
    // Fn subtree to `_` (-> `(_, i64)`) and let Rust infer the concrete
    // closure type from the RHS. A top-level Fn was already turned into
    // Ty::Unknown above, so this only touches the nested-container case.
    if matches!(&ty, Ty::Tuple(_) | Ty::Applied(..) | Ty::Named(_, _) | Ty::Record { .. } | Ty::OpenRecord { .. })
        && ty.any_child_recursive(&|t| matches!(t, Ty::Fn { .. }))
    {
        erase_fn_types(ty)
    } else {
        ty
    }
}

/// When binding a lambda to a Fn-typed variable (e.g. type alias Handler = (String) -> String),
/// the let type is erased to `_` but the lambda params have no type annotations either,
/// causing Rust type inference failure. Render lambda params with explicit types in this case.
/// Extracted from `render_stmt_bind`'s local closure of the same name — a
/// closure that already took `ctx` as an explicit param is just a named
/// function that hasn't been given a name yet.
fn annotate_bind_lambda(ctx: &RenderContext, params: &[(VarId, Ty)], body: &IrExpr) -> String {
    let params_str = params.iter()
        .map(|(id, pty)| {
            let name = ctx.var_name(*id).to_string();
            if matches!(pty, Ty::Unknown) {
                name
            } else if matches!(pty, Ty::Fn { .. }) {
                // A closure can't take an `impl Fn` parameter (E0562);
                // a function-typed param is `Rc<dyn Fn>` (callers box
                // the closure they pass — see render_generic_call).
                format!("{}: {}", name, super::helpers::render_type_field_fn(ctx, pty))
            } else {
                format!("{}: {}", name, super::types::render_type(ctx, pty))
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    let body_str = render_expr(ctx, body);
    ctx.templates.render_with("lambda_single", None, &[], &[("params", params_str.as_str()), ("body", body_str.as_str())])
        .unwrap_or_else(|| format!("move |{}| {}", params_str, body_str))
}

/// Render a Bind's RHS value expression, annotating an erased-to-`Unknown`
/// binding's lambda tail with explicit param types where needed (see
/// `annotate_bind_lambda`). Extracted from `render_stmt_bind`.
fn render_bind_value_str(ctx: &RenderContext, ty: &Ty, value: &IrExpr) -> String {
    let has_typed = |params: &[(VarId, Ty)]| params.iter().any(|(_, t)| !matches!(t, Ty::Unknown));
    if !matches!(ty, Ty::Unknown) {
        return render_expr(ctx, value);
    }
    match &value.kind {
        IrExprKind::Lambda { params, body, .. } if has_typed(params) => annotate_bind_lambda(ctx, params, body),
        // Capture-clone-wrapped closure: a shared-mut-capturing raw closure
        // lowers to `{ let __cap = x.clone(); move |k| … }`. The wrapping
        // block hides the lambda from the bare-Lambda case above, so a typed
        // param rendered `move |k|` with no type → E0282. Annotate the tail
        // lambda's params, keeping the capture prologue. HOF-arg lambdas
        // never reach here (render_iter_chain splices them inline), so this
        // is safe. (Closure v2 P6.)
        IrExprKind::Block { stmts, expr: Some(tail) }
            if matches!(&tail.kind, IrExprKind::Lambda { params, .. } if has_typed(params)) =>
        {
            if let IrExprKind::Lambda { params, body, .. } = &tail.kind {
                let stmts_s = stmts.iter().map(|s| render_stmt(ctx, s)).collect::<Vec<_>>().join("\n");
                format!("{{\n{}\n{}\n}}", stmts_s, annotate_bind_lambda(ctx, params, body))
            } else {
                render_expr(ctx, value)
            }
        }
        _ => render_expr(ctx, value),
    }
}

/// Val-wrap: var of non-Copy type → RcCow<T> with RcCow::new(value) for
/// COW. Extracted from `render_stmt_bind`: `Some` mirrors the original's
/// early `return`, `None` means "not RcCow, fall through".
fn try_render_bind_rc_cow(ctx: &RenderContext, var: &VarId, name_s: &str, type_s: &str, value: &IrExpr, value_s: &str) -> Option<String> {
    if !ctx.ann.is_rc_cow(var) { return None; }
    let val_type = format!("RcCow<{}>", type_s);
    // If the value is a fn param passed by reference (&Vec<u8>, &[T]),
    // clone it to get an owned value for RcCow::new().
    let needs_clone = match &value.kind {
        IrExprKind::Var { id } => ctx.ref_params.contains(id),
        IrExprKind::Clone { expr: inner } => match &inner.kind {
            IrExprKind::Var { id } => ctx.ref_params.contains(id),
            _ => false,
        },
        _ => false,
    };
    let val_value = if needs_clone {
        format!("RcCow::new({}.clone())", value_s)
    } else {
        format!("RcCow::new({})", value_s)
    };
    Some(ctx.templates.render_with("var_binding", None, &[], &[("name", name_s), ("type", val_type.as_str()), ("value", val_value.as_str())])
        .unwrap_or_else(|| if name_s == "_" { format!("let {}: {} = {};", name_s, val_type, val_value) } else { format!("let mut {}: {} = {};", name_s, val_type, val_value) }))
}

/// `render_stmt_bind`'s RcCow-clone type/value adjustment, extracted
/// verbatim (cog>30 decomposition) — if `value` comes from an RcCow-wrapped
/// var (`Clone` or direct `Var`), re-derive the rendered `(type, value)`
/// pair to wrap in `RcCow<..>`; otherwise pass `type_s`/`value_s` through
/// unchanged.
fn rc_cow_clone_bind_type_value(ctx: &RenderContext, value: &IrExpr, type_s: String, value_s: String) -> (String, String) {
    // Check if value comes from a RcCow-wrapped var (Clone or direct)
    let is_val_clone = match &value.kind {
        IrExprKind::Clone { expr: inner } => {
            if let IrExprKind::Var { id } = &inner.kind {
                ctx.ann.is_rc_cow(id)
            } else { false }
        }
        IrExprKind::Var { id } => ctx.ann.is_rc_cow(id),
        _ => false,
    };
    if !is_val_clone {
        return (type_s, value_s);
    }
    match &value.kind {
        // Direct Var from RcCow: use .clone() (Rc::clone O(1))
        IrExprKind::Var { .. } => {
            let val_type = format!("RcCow<{}>", type_s);
            let val_value = format!("{}.clone()", value_s);
            (val_type, val_value)
        }
        // Clone of RcCow var: deref+clone returned T, re-wrap
        _ => {
            let val_type = format!("RcCow<{}>", type_s);
            let val_value = format!("RcCow::new({})", value_s);
            (val_type, val_value)
        }
    }
}

fn render_stmt_bind(ctx: &RenderContext, stmt: &IrStmt) -> String {
    let IrStmtKind::Bind { var, ty, value, mutability } = &stmt.kind else { unreachable!() };
    if let Some(rendered) = try_render_bind_shared_mut(ctx, var, ty, value) {
        return rendered;
    }
    let name_s = ctx.var_name(*var).to_string();
    // Bindings whose runtime representation is a borrow the `Ty` system
    // cannot spell (TCO borrow-preserved `Bytes` temps): render the
    // annotation as `_` and let Rust infer — the IR type stays real for
    // the ConcretizeTypes postcondition.
    let ty_owned = if ctx.ann.is_infer_binding(var) { Ty::Unknown } else { erase_bind_ty(ctx, ty) };
    let ty = &ty_owned;
    let type_s = render_type(ctx, ty);
    let value_s = render_bind_value_str(ctx, ty, value);
    let (type_s, value_s) = rc_cow_clone_bind_type_value(ctx, value, type_s, value_s);
    let needs_mut = matches!(mutability, Mutability::Let) && {
        let ty_str = type_s.as_str();
        ty_str == "Vec<u8>"
            || ty_str.starts_with("Vec<")
            || ty_str.starts_with("HashMap<")
            // #617: Bytes/Matrix render as RcCow — their in-place mutators
            // (&mut deref-coerce = make_mut COW) still need a `mut` binding,
            // exactly like the raw spellings above.
            || ty_str.starts_with("RcCow<")
    };
    if let Some(rendered) = try_render_bind_rc_cow(ctx, var, &name_s, &type_s, value, &value_s) {
        return rendered;
    }
    // Non-RcCow binding whose initializer reads a borrowed param: the
    // binding's type is the OWNED form, so convert the borrow to an
    // owned value (slice→`.to_vec()`, `&str`→`.to_string()`, else
    // `.clone()`). Applies to both `let` and `var` — a slice cloned as
    // `.clone()` would stay a `&[T]` and mismatch `Vec<T>` (#624).
    let value_s = if !ctx.ann.is_rc_cow(var) {
        borrowed_param_owning_value(ctx, value).unwrap_or(value_s)
    } else { value_s };
    let is_wildcard = name_s == "_";
    let construct = match mutability {
        _ if is_wildcard => "let_binding",
        Mutability::Let if needs_mut => "var_binding",
        Mutability::Let => "let_binding",
        Mutability::Var => "var_binding",
    };
    ctx.templates.render_with(construct, None, &[], &[("name", name_s.as_str()), ("type", type_s.as_str()), ("value", value_s.as_str())])
        .unwrap_or_else(|| format!("let _ = _;"))
}

fn render_stmt_assign(ctx: &RenderContext, stmt: &IrStmt) -> String {
    let IrStmtKind::Assign { var, value } = &stmt.kind else { unreachable!() };
    let target_s = ctx.var_name(*var).to_string();
    // Shared-mut local (`Rc<Cell<T>>`): write through the cell. Cell's
    // interior mutability means the binding need not be `mut`. (Closure v2, P3.)
    if ctx.ann.is_shared_mut(var) {
        return format!("{}.set({});", target_s, render_expr(ctx, value));
    }
    let value_s = render_expr(ctx, value);
    // §4 Stage 2: module globals dispatch on the alias-resolved
    // attribute (one lookup owns storage AND the emitted name) —
    // replaces the name-keyed get_var_storage probe whose prefixing
    // subtleties produced #505. A Lazy/Const global in assign
    // position is impossible (the checker rejects assignment to an
    // immutable binding) — encoded as an ICE, not a silent arm.
    if let Some(info) = ctx.ann.global(*var) {
        use almide_ir::top_let_storage::TopLetStorage as Tls;
        // #617: the static stores the RAW Bytes/Matrix shape — un-wrap an
        // RcCow-shaped value at the assign boundary (identity otherwise).
        let value_s = super::expressions::rc_cow_unglue(value_s.clone(), &value.ty);
        return match info.storage {
            Tls::Cell => format!("{}.with(|c| c.set({}));", info.static_name, value_s),
            Tls::RcRefCell => format!("{}.with(|c| *c.borrow_mut() = std::rc::Rc::new(({}).into()));", info.static_name, value_s),
            Tls::Const | Tls::Lazy { .. } => unreachable!(
                "[COMPILER BUG] assignment to immutable global `{}` reached codegen",
                info.static_name
            ),
        };
    }
    match ctx.ann.get_var_storage(var) {
        VarStorage::RcCow => format!("{} = RcCow::new({});", target_s, value_s),
        _ => ctx.templates.render_with("assignment", None, &[], &[("target", target_s.as_str()), ("value", value_s.as_str())])
            .unwrap_or_else(|| format!("_ = _;")),
    }
}

fn render_stmt_guard(ctx: &RenderContext, stmt: &IrStmt) -> String {
    let IrStmtKind::Guard { cond, else_ } = &stmt.kind else { unreachable!() };
    let cond_str = render_expr(ctx, cond);
    let else_str = render_expr(ctx, else_);
    // Determine action: break for loop guards, return for function guards.
    // Check both the expression kind (for direct Unit/Break/Continue/ResultOk(Unit))
    // and the expression type (for LICM-hoisted vars whose kind is Var but type is Result[Unit,_]).
    let is_loop_control = matches!(&else_.kind, IrExprKind::Unit | IrExprKind::Break | IrExprKind::Continue)
        || (matches!(&else_.kind, IrExprKind::ResultOk { .. }) && {
            if let IrExprKind::ResultOk { expr: inner } = &else_.kind {
                matches!(&inner.kind, IrExprKind::Unit)
            } else { false }
        })
        // Block wrapping Continue/Break: { continue } has ty=Unit but action=continue
        || (matches!(&else_.kind, IrExprKind::Block { .. }) && {
            if let IrExprKind::Block { stmts, expr: None } = &else_.kind {
                stmts.len() == 1 && matches!(&stmts[0].kind, IrStmtKind::Expr { expr } if matches!(&expr.kind, IrExprKind::Continue | IrExprKind::Break))
            } else { false }
        })
        // LICM-hoisted ok(()) → Var with Result[Unit,_] type
        || (matches!(&else_.kind, IrExprKind::Var { .. }) &&
            matches!(&else_.ty, Ty::Applied(TypeConstructorId::Result, args) if args.first().is_some_and(|t| matches!(t, Ty::Unit))));
    let has_continue = matches!(&else_.kind, IrExprKind::Continue)
        || matches!(&else_.kind, IrExprKind::Block { stmts, expr: None }
            if stmts.len() == 1 && matches!(&stmts[0].kind, IrStmtKind::Expr { expr } if matches!(&expr.kind, IrExprKind::Continue)));
    let action = if is_loop_control {
        if has_continue { "continue" } else { "break" }
    } else { "return" };
    let neg = ctx.templates.render_with("guard_negate", None, &[], &[("cond", cond_str.as_str())])
        .unwrap_or_else(|| format!("!cond"));
    if action == "break" || action == "continue" {
        format!("if {} {{ {} }}", neg, action)
    } else {
        format!("if {} {{ return {} }}", neg, else_str)
    }
}

fn render_stmt_index_assign(ctx: &RenderContext, stmt: &IrStmt) -> String {
    let IrStmtKind::IndexAssign { target, index, value } = &stmt.kind else { unreachable!() };
    let target_str = ctx.var_name(*target).to_string();
    let upper = ctx.global_static_name(*target);
    let idx_str = render_expr(ctx, index);
    let val_str = render_expr(ctx, value);
    let var_ty = &ctx.var_table.get(*target).ty;
    let is_bytes = matches!(var_ty, Ty::Bytes);
    let cast_val = if is_bytes { format!("{} as u8", val_str) } else { val_str };
    // Shared-mut non-Copy var (`SharedMut`, P6): index through the cell.
    if ctx.ann.is_shared_mut(target) {
        return format!("almide_index_set!({}.borrow_mut(), {}, {});", target_str, idx_str, cast_val);
    }
    // §4 Stage 2: globals dispatch on the attribute. A scalar Cell
    // global cannot be index-assigned — the legacy arm emitted a
    // SILENT NO-OP `c.get();` for that impossible cell; it is now an
    // ICE so any future routing hole fails the build instead.
    if let Some(info) = ctx.ann.global(*target) {
        use almide_ir::top_let_storage::TopLetStorage as Tls;
        return match info.storage {
            Tls::RcRefCell => format!("{}.with(|c| almide_index_set!(std::rc::Rc::make_mut(&mut *c.borrow_mut()), {}, {}));", info.static_name, idx_str, cast_val),
            other => unreachable!(
                "[COMPILER BUG] index-assign to {:?} global `{}`",
                other, info.static_name
            ),
        };
    }
    // #554: bounds-checked store — a native OOB `xs[i] = v` aborts with
    // the unified `Error: index out of bounds` + exit 1 (matching wasm
    // and the div/mod contract) instead of a raw panic (exit 101).
    match ctx.ann.get_var_storage(target) {
        VarStorage::RcCow => format!("almide_index_set!({}.make_mut(), {}, {});", target_str, idx_str, cast_val),
        _ => format!("almide_index_set!({}, {}, {});", target_str, idx_str, cast_val),
    }
}

fn render_stmt_map_insert(ctx: &RenderContext, stmt: &IrStmt) -> String {
    let IrStmtKind::MapInsert { target, key, value } = &stmt.kind else { unreachable!() };
    let target_str = ctx.var_name(*target).to_string();
    let upper = ctx.global_static_name(*target);
    let key_str = render_expr(ctx, key);
    let val_str = render_expr(ctx, value);
    // Shared-mut non-Copy var (`SharedMut`, P6): insert through the cell.
    if ctx.ann.is_shared_mut(target) {
        return format!("{}.borrow_mut().insert({}, {});", target_str, key_str, val_str);
    }
    if let Some(info) = ctx.ann.global(*target) {
        use almide_ir::top_let_storage::TopLetStorage as Tls;
        return match info.storage {
            Tls::RcRefCell => format!("{}.with(|c| std::rc::Rc::make_mut(&mut *c.borrow_mut()).insert({}, {}));", info.static_name, key_str, val_str),
            other => unreachable!(
                "[COMPILER BUG] map-insert into {:?} global `{}`",
                other, info.static_name
            ),
        };
    }
    match ctx.ann.get_var_storage(target) {
        VarStorage::RcCow => format!("{}.make_mut().insert({}, {});", target_str, key_str, val_str),
        _ => ctx.templates.render_with("map_insert", None, &[], &[("target", target_str.as_str()), ("key", key_str.as_str()), ("value", val_str.as_str())])
            .unwrap_or_else(|| "map_set(...)".into()),
    }
}

fn render_stmt_field_assign(ctx: &RenderContext, stmt: &IrStmt) -> String {
    let IrStmtKind::FieldAssign { target, field, value } = &stmt.kind else { unreachable!() };
    let target_str = ctx.var_name(*target).to_string();
    let upper = ctx.global_static_name(*target);
    let val_str = render_expr(ctx, value);
    // Shared-mut non-Copy var (`SharedMut`, P6): assign the field through the cell.
    if ctx.ann.is_shared_mut(target) {
        return format!("{}.borrow_mut().{} = {};", target_str, field, val_str);
    }
    if let Some(info) = ctx.ann.global(*target) {
        use almide_ir::top_let_storage::TopLetStorage as Tls;
        return match info.storage {
            Tls::RcRefCell => format!("{}.with(|c| std::rc::Rc::make_mut(&mut *c.borrow_mut()).{} = {});", info.static_name, field, val_str),
            other => unreachable!(
                "[COMPILER BUG] field-assign to {:?} global `{}`",
                other, info.static_name
            ),
        };
    }
    match ctx.ann.get_var_storage(target) {
        VarStorage::RcCow => format!("{}.make_mut().{} = {};", target_str, field, val_str),
        _ => format!("{}.{} = {};", target_str, field, val_str),
    }
}

fn render_stmt_bind_destructure(ctx: &RenderContext, stmt: &IrStmt) -> String {
    let IrStmtKind::BindDestructure { pattern, value } = &stmt.kind else { unreachable!() };
    // For record patterns with empty name, resolve from value type
    let pat_str = match pattern {
        IrPattern::RecordPattern { name, fields, rest } if name.is_empty() => {
            // Determine the total field count of the value type so we
            // can automatically insert `..` when the pattern only
            // destructures a subset (otherwise Rust complains with
            // E0027 "pattern does not mention field X").
            let total_fields: Option<usize> = match &value.ty {
                Ty::Named(n, _) => ctx.ann.record_field_counts.get(n.as_str()).copied(),
                Ty::Record { fields: ty_fields } | Ty::OpenRecord { fields: ty_fields } =>
                    Some(ty_fields.len()),
                _ => None,
            };
            let type_name = match &value.ty {
                Ty::Named(n, _) => n.to_string(),
                Ty::Record { fields: ty_fields } | Ty::OpenRecord { fields: ty_fields } => {
                    let mut names: Vec<String> = ty_fields.iter().map(|(n, _)| n.to_string()).collect();
                    names.sort();
                    ctx.ann.named_records.get(&names).cloned()
                        .or_else(|| ctx.ann.anon_records.get(&names).cloned())
                        .unwrap_or_else(|| names.join("_"))
                }
                _ => "_".into(),
            };
            let qualified = if let Some(enum_name) = ctx.ann.ctor_to_enum.get(&type_name) {
                ctx.templates.render_with("ctor_qualify", None, &[], &[("enum_name", enum_name.as_str()), ("ctor_name", type_name.as_str())])
                    .unwrap_or_else(|| format!("{}::{}", enum_name, type_name))
            } else {
                type_name
            };
            let fields_str = fields.iter()
                .map(|f| match &f.pattern {
                    Some(p) => format!("{}: {}", f.name, render_pattern(ctx, p)),
                    None => f.name.clone(),
                })
                .collect::<Vec<_>>().join(", ");
            let needs_rest = *rest
                || total_fields.map_or(false, |n| fields.len() < n);
            if needs_rest {
                let construct = if fields_str.is_empty() { "record_pattern_rest_empty" } else { "record_pattern_rest" };
                ctx.templates.render_with(construct, None, &[], &[("name", qualified.as_str()), ("fields", fields_str.as_str())])
                    .unwrap_or_else(|| format!("{} {{ {} }}", qualified, fields_str))
            } else {
                ctx.templates.render_with("destructure_pattern", None, &[], &[("name", qualified.as_str()), ("fields", fields_str.as_str())])
                    .unwrap_or_else(|| format!("{} {{ {} }}", qualified, fields_str))
            }
        }
        _ => render_pattern(ctx, pattern),
    };
    let val_str = render_expr(ctx, value);
    ctx.templates.render_with("bind_destructure", None, &[], &[("pattern", pat_str.as_str()), ("value", val_str.as_str())])
        .unwrap_or_else(|| format!("let _ = _;"))
}

// ── #610: nested constructor patterns through a `Box` ──
//
// Rust cannot pattern-match through a `Box` on stable (box-patterns are
// unstable). A nested constructor on a BOXED recursive field —
// `Node(Leaf(a), Leaf(b))` where `Node`'s fields are `Box<Tree>` — used to render
// `Tree::Node(Tree::Leaf(a), ..)`, which rustc rejects: the field is `Box<Tree>`,
// not `Tree` (E0308). We rewrite the arm instead:
//   * each boxed-nested position becomes a fresh `Box` binding in a FLAT pattern,
//   * a `matches!` shape-guard verifies the nested structure (a non-match falls
//     through to a later arm — refinement, exactly like the wasm emitter),
//   * a `let-else` moves the value out of the box in the body and binds the inner
//     names by value (matching the by-value `*box` convention of simple arms).
// All stable since 1.65, edition-agnostic, any nesting depth. With no boxed-nested
// position `unbox_arm_pattern` returns None and the arm renders unchanged.

fn pattern_is_complex(p: &IrPattern) -> bool {
    matches!(p, IrPattern::Constructor { .. } | IrPattern::RecordPattern { .. })
}

fn fresh_box_var(counter: &mut usize) -> String {
    let v = format!("__bx{}", *counter);
    *counter += 1;
    v
}

fn qualify_ctor(ctx: &RenderContext, name: &str, enum_hint: Option<&str>) -> String {
    let enum_name = enum_hint.map(|s| s.to_string())
        .or_else(|| ctx.ann.ctor_to_enum.get(name).map(|s| s.to_string()));
    match enum_name {
        Some(en) => ctx.templates.render_with("ctor_qualify", None, &[],
            &[("enum_name", en.as_str()), ("ctor_name", name)])
            .unwrap_or_else(|| format!("{}::{}", en, name)),
        None => name.to_string(),
    }
}

fn is_boxed_tuple_field(ctx: &RenderContext, ctor: &str, idx: usize) -> bool {
    ctx.ann.boxed_fields.contains(&(ctor.to_string(), idx.to_string()))
}

fn is_boxed_record_field(ctx: &RenderContext, ctor: &str, field: &str) -> bool {
    ctx.ann.boxed_fields.contains(&(ctor.to_string(), field.to_string()))
}

/// `matches!`-shaped boolean that `access` (a `&Enum`-typed expr) structurally
/// matches `pat`, deref-ing one box per level. The shape must carry EVERY
/// refutable constraint the body's `let-else` will re-assert — the guard is the
/// only thing standing between a non-matching value and the let-else's
/// `unreachable!()` (#757: erasing a non-boxed inner tag like `Color::Red` to
/// `_` let a `Black` node through the guard and panicked instead of falling
/// through to the next arm).
fn box_shape_guard(ctx: &RenderContext, pat: &IrPattern, access: &str, counter: &mut usize) -> String {
    match pat {
        IrPattern::Constructor { .. } | IrPattern::RecordPattern { .. } => {
            let mut subs = Vec::new();
            let shape = guard_shape(ctx, pat, counter, &mut subs);
            if subs.is_empty() {
                format!("matches!({}, {})", access, shape)
            } else {
                format!("matches!({}, {} if {})", access, shape, subs.join(" && "))
            }
        }
        // Non-constructor pattern in a boxed position cannot occur (a recursive
        // field is always a variant); be conservative and impose no constraint.
        _ => "true".to_string(),
    }
}

/// Guard shape of a sub-pattern inside a `matches!` clause: bindings and
/// wildcards impose no constraint (`_` — a real binding here would be dead and
/// warn), literals and non-boxed constructors keep their refutable structure
/// inline, and a boxed-nested sub-pattern binds a fresh var whose deref guard
/// joins `subs` (Rust can't pattern-match through a `Box` on stable).
fn guard_shape(ctx: &RenderContext, pat: &IrPattern, counter: &mut usize, subs: &mut Vec<String>) -> String {
    match pat {
        IrPattern::Wildcard | IrPattern::Bind { .. } => "_".to_string(),
        IrPattern::Literal { .. } => render_pattern_hinted(ctx, pat, None),
        IrPattern::Some { inner } => format!("Some({})", guard_shape(ctx, inner, counter, subs)),
        IrPattern::None => "None".to_string(),
        IrPattern::Ok { inner } => format!("Ok({})", guard_shape(ctx, inner, counter, subs)),
        IrPattern::Err { inner } => format!("Err({})", guard_shape(ctx, inner, counter, subs)),
        IrPattern::Tuple { elements } => {
            let shapes: Vec<String> =
                elements.iter().map(|p| guard_shape(ctx, p, counter, subs)).collect();
            format!("({})", shapes.join(", "))
        }
        IrPattern::Constructor { name, args } => {
            let qualified = qualify_ctor(ctx, name.as_str(), None);
            if args.is_empty() {
                return qualified;
            }
            let shapes: Vec<String> = args
                .iter()
                .enumerate()
                .map(|(i, arg)| {
                    if is_boxed_tuple_field(ctx, name.as_str(), i) && pattern_is_complex(arg) {
                        let g = fresh_box_var(counter);
                        subs.push(box_shape_guard(ctx, arg, &format!("&**{}", g), counter));
                        g
                    } else {
                        guard_shape(ctx, arg, counter, subs)
                    }
                })
                .collect();
            format!("{}({})", qualified, shapes.join(", "))
        }
        IrPattern::RecordPattern { name, fields, .. } => {
            let qualified = qualify_ctor(ctx, name.as_str(), None);
            let shapes: Vec<String> = fields
                .iter()
                .map(|fp| match &fp.pattern {
                    Some(p) if is_boxed_record_field(ctx, name.as_str(), fp.name.as_str())
                        && pattern_is_complex(p) =>
                    {
                        let g = fresh_box_var(counter);
                        subs.push(box_shape_guard(ctx, p, &format!("&**{}", g), counter));
                        format!("{}: {}", fp.name, g)
                    }
                    Some(p) => format!("{}: {}", fp.name, guard_shape(ctx, p, counter, subs)),
                    None => format!("{}: _", fp.name),
                })
                .collect();
            format!("{} {{ {}, .. }}", qualified, shapes.join(", "))
        }
        // ListPatternLowering rewrites list patterns before rendering reaches
        // this point; nothing refutable can arrive here.
        IrPattern::List { .. } => "_".to_string(),
    }
}

/// Emit `let <flat pat> = <move_expr> else { unreachable!() };` to move the value
/// out of its box and bind the inner names by value, recursing for deeper boxes.
/// The guard has already verified the structure, so `else` is dead.
fn box_extract(ctx: &RenderContext, pat: &IrPattern, move_expr: &str, binds: &mut Vec<String>, counter: &mut usize) {
    match pat {
        IrPattern::Constructor { name, args } => box_extract_constructor(ctx, name, args, move_expr, binds, counter),
        IrPattern::RecordPattern { name, fields, .. } => box_extract_record(ctx, name, fields, move_expr, binds, counter),
        _ => {}
    }
}

/// `IrPattern::Constructor` case of `box_extract`, extracted verbatim
/// (cog>30 decomposition, pattern 1 — `binds`/`counter` are write-only
/// accumulators, same safety class as `check_needs_ownership`'s `needs`).
fn box_extract_constructor(ctx: &RenderContext, name: &str, args: &[IrPattern], move_expr: &str, binds: &mut Vec<String>, counter: &mut usize) {
    let qualified = qualify_ctor(ctx, name, None);
    let mut flat = Vec::with_capacity(args.len());
    let mut deeper: Vec<(String, &IrPattern)> = Vec::new();
    for (i, arg) in args.iter().enumerate() {
        if is_boxed_tuple_field(ctx, name, i) && pattern_is_complex(arg) {
            let e = fresh_box_var(counter);
            flat.push(e.clone());
            deeper.push((e, arg));
        } else {
            flat.push(render_pattern_hinted(ctx, arg, None));
        }
    }
    let flat_pat = if args.is_empty() { qualified } else { format!("{}({})", qualified, flat.join(", ")) };
    binds.push(format!("let {} = {} else {{ unreachable!() }};", flat_pat, move_expr));
    for (e, sub) in deeper {
        box_extract(ctx, sub, &format!("*{}", e), binds, counter);
    }
}

/// `IrPattern::RecordPattern` case of `box_extract`, extracted verbatim
/// (cog>30 decomposition).
fn box_extract_record(ctx: &RenderContext, name: &str, fields: &[IrFieldPattern], move_expr: &str, binds: &mut Vec<String>, counter: &mut usize) {
    let qualified = qualify_ctor(ctx, name, None);
    let mut flat = Vec::with_capacity(fields.len());
    let mut deeper: Vec<(String, &IrPattern)> = Vec::new();
    for fp in fields {
        match &fp.pattern {
            Some(p) if is_boxed_record_field(ctx, name, fp.name.as_str())
                && pattern_is_complex(p) =>
            {
                let e = fresh_box_var(counter);
                flat.push(format!("{}: {}", fp.name, e));
                deeper.push((e, p));
            }
            Some(p) => flat.push(format!("{}: {}", fp.name, render_pattern_hinted(ctx, p, None))),
            None => flat.push(fp.name.to_string()),
        }
    }
    binds.push(format!("let {} {{ {}, .. }} = {} else {{ unreachable!() }};", qualified, flat.join(", "), move_expr));
    for (e, sub) in deeper {
        box_extract(ctx, sub, &format!("*{}", e), binds, counter);
    }
}

/// Rewrite an arm whose top-level variant pattern has a boxed-nested constructor.
/// Returns `(flat_pattern, shape_guards, body_let_else_binds)` or None if the arm
/// has no boxed-nested position (the common case → no rewrite).
fn unbox_arm_pattern(ctx: &RenderContext, pat: &IrPattern, enum_hint: Option<&str>)
    -> Option<(String, Vec<String>, Vec<String>)>
{
    let mut counter = 0usize;
    let mut guards = Vec::new();
    let mut binds = Vec::new();
    let flat = match pat {
        IrPattern::Constructor { name, args } => {
            let qualified = qualify_ctor(ctx, name.as_str(), enum_hint);
            let mut flat = Vec::with_capacity(args.len());
            for (i, arg) in args.iter().enumerate() {
                if is_boxed_tuple_field(ctx, name.as_str(), i) && pattern_is_complex(arg) {
                    let v = fresh_box_var(&mut counter);
                    guards.push(box_shape_guard(ctx, arg, &format!("&*{}", v), &mut counter));
                    box_extract(ctx, arg, &format!("*{}", v), &mut binds, &mut counter);
                    flat.push(v);
                } else {
                    flat.push(render_pattern_hinted(ctx, arg, None));
                }
            }
            if args.is_empty() { qualified } else { format!("{}({})", qualified, flat.join(", ")) }
        }
        IrPattern::RecordPattern { name, fields, .. } => {
            let qualified = qualify_ctor(ctx, name.as_str(), enum_hint);
            let mut flat = Vec::with_capacity(fields.len());
            for fp in fields {
                match &fp.pattern {
                    Some(p) if is_boxed_record_field(ctx, name.as_str(), fp.name.as_str())
                        && pattern_is_complex(p) =>
                    {
                        let v = fresh_box_var(&mut counter);
                        guards.push(box_shape_guard(ctx, p, &format!("&*{}", v), &mut counter));
                        box_extract(ctx, p, &format!("*{}", v), &mut binds, &mut counter);
                        flat.push(format!("{}: {}", fp.name, v));
                    }
                    Some(p) => flat.push(format!("{}: {}", fp.name, render_pattern_hinted(ctx, p, None))),
                    None => flat.push(fp.name.to_string()),
                }
            }
            format!("{} {{ {} }}", qualified, flat.join(", "))
        }
        _ => return None,
    };
    if guards.is_empty() { None } else { Some((flat, guards, binds)) }
}

// ── Match arm rendering ──

pub fn render_match_arm(ctx: &RenderContext, arm: &IrMatchArm, match_ty: &almide_lang::types::Ty, subject_ty: &almide_lang::types::Ty) -> String {
    // #413: a top-level variant pattern belongs to the match SUBJECT's enum, so
    // pass that enum's (mangled) name as a hint — it disambiguates a constructor
    // name shared across packages, which the global ctor→enum map collapses.
    let enum_hint = match subject_ty {
        // Only when the subject is a known variant enum — never a struct/opaque
        // type (whose patterns must not be qualified `Type::field`).
        almide_lang::types::Ty::Named(n, _) if ctx.ann.ctor_to_enum.values().any(|e| e.as_str() == n.as_str())
            => Some(n.as_str()),
        _ => None,
    };
    // #610: a boxed-nested constructor pattern is rewritten to a flat pattern + a
    // `matches!` shape-guard + `let-else` box move-outs in the body. None when the
    // arm has no boxed-nested position (the common case → identical to before).
    let (pattern, shape_guards, box_binds) = match unbox_arm_pattern(ctx, &arm.pattern, enum_hint) {
        Some((flat, guards, binds)) => (flat, guards, binds),
        None => (render_pattern_hinted(ctx, &arm.pattern, enum_hint), Vec::new(), Vec::new()),
    };
    // err() in a match arm where the match type is NOT Result: early return.
    // This handles `let x: T = match ... { none => err("msg") }` in
    // functions returning Result — the err() doesn't contribute a T value,
    // it exits the function with an error.
    let raw_body = if matches!(&arm.body.kind, IrExprKind::ResultErr { .. }) && !match_ty.is_result() {
        format!("return {}", render_expr(ctx, &arm.body))
    } else {
        super::expressions::render_expr_owned(ctx, &arm.body)
    };
    // The box move-outs (`let Tree::Leaf(a) = *__bx0 else …`) run FIRST in the
    // arm body, then the original body sees the inner bindings.
    let body = if box_binds.is_empty() {
        raw_body
    } else {
        format!("{{ {} {} }}", box_binds.join(" "), raw_body)
    };
    // Append guards: the structural shape-guards (which must hold for the rewritten
    // arm to apply, so a non-match falls through) then any user guard.
    let mut guard_conds = shape_guards;
    if let Some(ref guard) = arm.guard {
        guard_conds.push(render_expr(ctx, guard));
    }
    let full_pattern = if guard_conds.is_empty() {
        pattern
    } else {
        format!("{} if {}", pattern, guard_conds.join(" && "))
    };
    ctx.templates.render_with("match_arm_inline", None, &[], &[("pattern", full_pattern.as_str()), ("body", body.as_str())])
        .unwrap_or_else(|| format!("_ => _,"))
}

/// Check if any match arm uses a list pattern.
pub fn arms_have_list_pattern(arms: &[IrMatchArm]) -> bool {
    arms.iter().any(|arm| matches!(&arm.pattern, IrPattern::List { .. }))
}

pub fn render_pattern(ctx: &RenderContext, pat: &IrPattern) -> String {
    render_pattern_hinted(ctx, pat, None)
}

/// Like `render_pattern`, but `enum_hint` is the match subject's enum type name
/// (mangled), used to qualify a TOP-LEVEL variant pattern. This disambiguates a
/// constructor name shared across packages (#413): the pattern belongs to the
/// subject's enum, not the (collapsed) global `ctor_to_enum` entry. Nested patterns
/// recurse without a hint (fall back to `ctor_to_enum`).
pub fn render_pattern_hinted(ctx: &RenderContext, pat: &IrPattern, enum_hint: Option<&str>) -> String {
    match pat {
        IrPattern::Wildcard => template_or(ctx, "pattern_wildcard", &[], "_"),
        IrPattern::Bind { var, .. } => ctx.var_name(*var).to_string(),
        IrPattern::Literal { expr } => {
            // In patterns, literals must be bare (no .to_string(), no i64 suffix for match)
            match &expr.kind {
                IrExprKind::LitStr { value } => {
                    let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
                    format!("\"{}\"", escaped)
                }
                IrExprKind::LitInt { value } => format!("{}", value),
                IrExprKind::LitFloat { value } => format!("{}", value),
                IrExprKind::LitBool { value } => format!("{}", value),
                _ => render_expr(ctx, expr),
            }
        }
        IrPattern::Some { inner } => {
            let binding_s = render_pattern(ctx, inner);
            ctx.templates.render_with("pattern_some", None, &[], &[("binding", binding_s.as_str())])
                .unwrap_or_else(|| format!("Some(_)"))
        }
        IrPattern::None => template_or(ctx, "pattern_none", &[], "None"),
        IrPattern::Ok { inner } => {
            let binding_s = render_pattern(ctx, inner);
            ctx.templates.render_with("pattern_ok", None, &[], &[("binding", binding_s.as_str())])
                .unwrap_or_else(|| format!("Ok(_)"))
        }
        IrPattern::Err { inner } => {
            let binding_s = render_pattern(ctx, inner);
            ctx.templates.render_with("pattern_err", None, &[], &[("binding", binding_s.as_str())])
                .unwrap_or_else(|| format!("Err(_)"))
        }
        IrPattern::Constructor { name, args } => {
            // #413: prefer the subject's enum (hint) over the collapsing global map.
            let enum_name = enum_hint.map(|s| s.to_string())
                .or_else(|| ctx.ann.ctor_to_enum.get(name).map(|s| s.to_string()));
            let qualified = if let Some(enum_name) = enum_name {
                ctx.templates.render_with("ctor_qualify", None, &[], &[("enum_name", enum_name.as_str()), ("ctor_name", name.as_str())])
                    .unwrap_or_else(|| format!("{}::{}", enum_name, name))
            } else {
                name.clone()
            };
            if args.is_empty() {
                qualified
            } else {
                let args_str = args.iter().map(|a| render_pattern(ctx, a)).collect::<Vec<_>>().join(", ");
                format!("{}({})", qualified, args_str)
            }
        }
        IrPattern::Tuple { elements } => {
            let elems = elements.iter().map(|e| render_pattern(ctx, e)).collect::<Vec<_>>().join(", ");
            ctx.templates.render_with("tuple_literal", None, &[], &[("elements", elems.as_str())])
                .unwrap_or_else(|| "tuple(...)".into())
        }
        IrPattern::List { elements } => {
            if elements.is_empty() {
                "[]".to_string()
            } else {
                let elems = elements.iter().map(|e| render_pattern(ctx, e)).collect::<Vec<_>>().join(", ");
                format!("[{}]", elems)
            }
        }
        IrPattern::RecordPattern { name, fields, rest } => {
            // Qualify enum variant record patterns: Circle → Shape::Circle.
            // #413: prefer the subject's enum (hint) over the collapsing global map.
            let qualified_name = if let Some(enum_name) = enum_hint.map(|s| s.to_string())
                .or_else(|| ctx.ann.ctor_to_enum.get(name).map(|s| s.to_string())) {
                format!("{}::{}", enum_name, name)
            } else {
                name.clone()
            };
            let fields_str = fields.iter()
                .map(|f| match &f.pattern {
                    Some(p) => format!("{}: {}", f.name, render_pattern(ctx, p)),
                    None => f.name.clone(),
                })
                .collect::<Vec<_>>()
                .join(", ");
            if *rest {
                let construct = if fields_str.is_empty() { "record_pattern_rest_empty" } else { "record_pattern_rest" };
                ctx.templates.render_with(construct, None, &[], &[("name", qualified_name.as_str()), ("fields", fields_str.as_str())])
                    .unwrap_or_else(|| format!("{} {{ {} }}", qualified_name, fields_str))
            } else {
                format!("{} {{ {} }}", qualified_name, fields_str)
            }
        }
    }
}
