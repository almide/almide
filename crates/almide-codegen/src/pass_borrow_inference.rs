//! BorrowInferencePass: Roc-style "borrowed by default, own when needed" analysis.
//!
//! For each user function parameter of heap type (String, Vec, Record, etc.):
//! 1. Start as Borrowed
//! 2. Walk the function body to find ownership-requiring uses
//! 3. If none found → mark param as Ref/RefStr/RefSlice
//! 4. Insert Borrow nodes at call sites for borrowed params
//!
//! This eliminates unnecessary .clone() at call sites when the callee only reads the value.

use std::collections::HashMap;
use std::cell::RefCell;
use almide_ir::*;
use almide_lang::types::{Ty, TypeConstructorId};
use almide_base::intern::sym;

/// `true` if the bundled `module.func`'s `@inline_rust` template
/// borrows the param at position `pos` (`&{name}`, `&*{name}`,
/// `&mut {name}`, or `&mut *{name}`). Consumed ("owned") params have
/// no sigil and render via `{name}` alone.
fn bundled_borrow_at(module: &str, func: &str, pos: usize) -> bool {
    use almide_lang::ast::{AttrValue, Decl};
    let Some(source) = almide_lang::stdlib_info::bundled_source(module) else {
        return false;
    };
    let Some(program) = almide_lang::parse_cached(source) else { return false; };
    for decl in &program.decls {
        let Decl::Fn { name, attrs, params, .. } = decl else { continue };
        if name.as_str() != func { continue; }
        let Some(pname) = params.get(pos).map(|p| p.name) else { return false; };
        let Some(attr) = attrs.iter().find(|a| a.name.as_str() == "inline_rust") else {
            return false;
        };
        let Some(first) = attr.args.first() else { return false; };
        let AttrValue::String { value } = &first.value else { return false; };
        let p = pname.as_str();
        return value.contains(&format!("&{{{}}}", p))
            || value.contains(&format!("&*{{{}}}", p))
            || value.contains(&format!("&mut {{{}}}", p))
            || value.contains(&format!("&mut *{{{}}}", p));
    }
    false
}

// Thread-local snapshot of currently-known borrow signatures, used during
// inference so that when we check `fn caller(data: Bytes) { other(data) }`
// we can consult `other`'s borrows and avoid pessimistically marking `data`
// as owned. Populated before each fixed-point iteration in
// `infer_borrow_signatures`.
thread_local! {
    static SIGS_SNAPSHOT: RefCell<HashMap<String, Vec<ParamBorrow>>> = RefCell::new(HashMap::new());
    static MOD_SCOPE: RefCell<Option<String>> = RefCell::new(None);
    // Name of the function currently being analysed. Self-recursive calls to
    // this function are treated optimistically (we don't scan their args for
    // ownership needs), which lets a TCO-loop body like `foo(data, next, ...)`
    // keep `data: &Vec<u8>` instead of collapsing to `Vec<u8>` on the first
    // pass and never recovering.
    static CURRENT_FN: RefCell<Option<String>> = RefCell::new(None);
}

fn lookup_user_borrows(callee: &str) -> Option<Vec<ParamBorrow>> {
    SIGS_SNAPSHOT.with(|s| {
        let s = s.borrow();
        MOD_SCOPE.with(|m| {
            let m = m.borrow();
            if let Some(mod_name) = m.as_deref() {
                if let Some(v) = s.get(&format!("{}::{}", mod_name, callee)) {
                    return Some(v.clone());
                }
            }
            s.get(callee).cloned()
        })
    })
}

/// Phase 1: Infer borrow signatures for all functions via fixed-point iteration.
///
/// One pass is not enough because a caller's ownership needs depend on the
/// borrow signatures of its callees. Round 1 handles leaf functions; later
/// rounds propagate those borrows up through their callers. Converges quickly
/// in practice — typical fix-points reach in 2-3 rounds; we cap at 6 for
/// safety.
pub fn infer_borrow_signatures(program: &mut IrProgram) -> HashMap<String, Vec<ParamBorrow>> {
    let mut sigs: HashMap<String, Vec<ParamBorrow>> = HashMap::new();

    // Seed `sigs` with `@intrinsic` fns from every bundled stdlib
    // module — including ones that weren't lowered into
    // `program.modules` (non auto-imported modules like `bytes`,
    // `regex`, `fs`). The key is the mangled runtime symbol so
    // `rewrite_calls` can look it up from `RuntimeCall.symbol` verbatim.
    {
        use almide_lang::ast::{AttrValue, Decl};
        for &mod_name in almide_lang::stdlib_info::BUNDLED_MODULES {
            let Some(source) = almide_lang::stdlib_info::bundled_source(mod_name) else { continue };
            let Some(parsed) = almide_lang::parse_cached(source) else { continue };
            for decl in &parsed.decls {
                let Decl::Fn { params, attrs, return_type, .. } = decl else { continue };
                let Some(attr) = attrs.iter().find(|a| a.name.as_str() == "intrinsic") else { continue };
                let Some(first) = attr.args.first() else { continue };
                let AttrValue::String { value: symbol } = &first.value else { continue };
                // Params in AST are `TypeExpr`, not resolved `Ty`. Convert
                // the simple cases into a `Ty` so `intrinsic_borrow_mode`
                // can reuse the same logic as the IR-side path.
                let mut borrows: Vec<ParamBorrow> = params.iter()
                    .map(|p| intrinsic_borrow_mode_from_type_expr(&p.ty))
                    .collect();
                // `@consume(p1, p2, ...)` overrides the inferred borrow for
                // the named params to `Own`. Required when the runtime fn
                // consumes a container (e.g. `xs: Vec<T>` on
                // `almide_rt_list_map`) rather than borrowing it.
                let consume_names: Vec<&str> = attrs.iter()
                    .filter(|a| a.name.as_str() == "consume")
                    .flat_map(|a| a.args.iter().filter_map(|arg| match &arg.value {
                        AttrValue::Ident { name } => Some(name.as_str()),
                        AttrValue::String { value } => Some(value.as_str()),
                        _ => None,
                    }))
                    .collect();
                for (idx, p) in params.iter().enumerate() {
                    if consume_names.iter().any(|n| n == &p.name.as_str()) {
                        borrows[idx] = ParamBorrow::Own;
                    }
                }
                // `@borrow_ref(p1, p2, ...)` — opposite override: force
                // `Ref` on params the default heuristic would pass by
                // value (e.g. user-defined named types whose runtime fn
                // takes `&T`, like `JsonPath`).
                let borrow_ref_names: Vec<&str> = attrs.iter()
                    .filter(|a| a.name.as_str() == "borrow_ref")
                    .flat_map(|a| a.args.iter().filter_map(|arg| match &arg.value {
                        AttrValue::Ident { name } => Some(name.as_str()),
                        AttrValue::String { value } => Some(value.as_str()),
                        _ => None,
                    }))
                    .collect();
                for (idx, p) in params.iter().enumerate() {
                    if borrow_ref_names.iter().any(|n| n == &p.name.as_str()) {
                        borrows[idx] = ParamBorrow::Ref;
                    }
                }
                // First param is mutated in place when: Almide fn is marked
                // `@mutating`, or returns Unit with a Ref-mode container
                // first arg (common case for `.clear`, `.push`, etc.).
                // Explicit `@borrow_ref(first_param)` always wins — the
                // caller has opted into "read-only reference even if
                // return is Unit" (e.g. `almide_rt_test_assert_some`).
                let has_mutating = attrs.iter().any(|a| a.name.as_str() == "mutating");
                let implicit_mut = is_unit_type_expr(return_type);
                if has_mutating || implicit_mut {
                    let first_name = params.first().map(|p| p.name.as_str());
                    let first_is_borrow_ref = first_name
                        .map(|n| borrow_ref_names.iter().any(|r| r == &n))
                        .unwrap_or(false);
                    if !first_is_borrow_ref {
                        if let Some(first_borrow) = borrows.first_mut() {
                            if matches!(first_borrow, ParamBorrow::Ref | ParamBorrow::RefSlice) {
                                *first_borrow = ParamBorrow::RefMut;
                            }
                        }
                    }
                }
                sigs.insert(symbol.clone(), borrows);
            }
        }
    }

    for _iter in 0..6 {
        // Snapshot current sigs into thread-local so check_needs_ownership can see them.
        SIGS_SNAPSHOT.with(|s| *s.borrow_mut() = sigs.clone());
        let prev_sigs = sigs.clone();

        MOD_SCOPE.with(|m| *m.borrow_mut() = None);
        for func in &mut program.functions {
            if func.is_test || is_derive_fn(&func.name) || is_monomorphized(&func.name) || func.generics.as_ref().map_or(false, |g| !g.is_empty()) { continue; }
            let borrows = infer_function_borrows(func);
            // Always record the signature (including all-Own) so that the
            // fixed-point iteration can distinguish "known to be Own" from
            // "not yet analysed". Without this, self-recursive functions
            // whose first-pass inference produced all-Own would be looked
            // up as None forever → conservative fallback → Own sticks.
            sigs.insert(func.name.to_string(), borrows.clone());
            for (param, borrow) in func.params.iter_mut().zip(borrows) {
                param.borrow = borrow;
            }
        }

        for module in &mut program.modules {
            let mod_name = module.name.to_string();
            MOD_SCOPE.with(|m| *m.borrow_mut() = Some(mod_name.clone()));
            for func in &mut module.functions {
                if func.is_test || is_derive_fn(&func.name) || is_monomorphized(&func.name) || func.generics.as_ref().map_or(false, |g| !g.is_empty()) { continue; }
                let borrows = infer_function_borrows(func);
                sigs.insert(format!("{}::{}", mod_name, func.name), borrows.clone());
                for (param, borrow) in func.params.iter_mut().zip(borrows) {
                    param.borrow = borrow;
                }
            }
        }

        if sigs == prev_sigs {
            break;
        }
    }

    // Clean up thread-locals so they don't leak across separate compilations.
    SIGS_SNAPSHOT.with(|s| s.borrow_mut().clear());
    MOD_SCOPE.with(|m| *m.borrow_mut() = None);

    sigs
}

fn infer_function_borrows(func: &IrFunction) -> Vec<ParamBorrow> {
    CURRENT_FN.with(|c| *c.borrow_mut() = Some(func.name.to_string()));

    // `@inline_rust` / `@wasm_intrinsic` fns (Stdlib Declarative
    // Unification Stage 2+) are dispatch-only declarations with a
    // Hole body. Their call sites route through a literal template
    // that is authoritative for borrow semantics — if the template
    // writes `&*{s}`, the underlying runtime takes `&str`; if it
    // writes `{s}`, the runtime consumes ownership. Running the
    // inference on a Hole body would spuriously mark every heap
    // param as `RefStr` / `RefSlice`, causing BorrowInsertionPass
    // to wrap the arg again and produce `&*&*` in the emitted Rust.
    // Default every param to Own here so the template is the sole
    // authority.
    // `@inline_rust` / `@wasm_intrinsic`: the template is authoritative
    // for borrow semantics (it spells out `&*{s}` / `&{m}` / `{n}`
    // explicitly), so every param is `Own` and the template controls
    // the arg decoration verbatim.
    let has_inline_template = func.attrs.iter().any(|a|
        matches!(a.name.as_str(), "inline_rust" | "wasm_intrinsic"));
    if has_inline_template {
        return func.params.iter().map(|_| ParamBorrow::Own).collect();
    }

    // `@intrinsic`: no template. Derive the borrow mode mechanically
    // from each param's Almide type so BorrowInsertion (not the walker)
    // decorates args at the call site:
    //   String                        → RefStr   (`&*{s}`)
    //   List / Bytes / Record / Option / Result / Map / Set
    //                                 → Ref      (`&{m}`)
    //   Int / Float / Bool / sized numerics
    //                                 → Own      (by value)
    //   Generic (TypeVar)             → Own      (caller decides)
    let has_intrinsic = func.attrs.iter().any(|a| a.name.as_str() == "intrinsic");
    if has_intrinsic {
        return func.params.iter().map(|param| {
            intrinsic_borrow_mode(&param.ty)
        }).collect();
    }

    func.params.iter().map(|param| {
        if !is_heap_type(&param.ty) {
            return ParamBorrow::Own;
        }

        // If the function body directly returns this param, it needs ownership
        if is_var(&func.body, param.var) {
            return ParamBorrow::Own;
        }

        let mut needs_own = false;
        check_needs_ownership(&func.body, param.var, &mut needs_own);


        if needs_own {
            ParamBorrow::Own
        } else if matches!(&param.ty, Ty::String) {
            ParamBorrow::RefStr
        } else if matches!(&param.ty, Ty::Applied(TypeConstructorId::List, _)) {
            ParamBorrow::RefSlice
        } else {
            ParamBorrow::Ref
        }
    }).collect()
}

fn is_derive_fn(name: &str) -> bool {
    name.contains("_encode") || name.contains("_decode") || name.contains("_eq")
        || name.contains("_display") || name.contains("_to_string") || name.contains("_from_")
}

fn is_monomorphized(name: &str) -> bool {
    name.contains("__")
}

/// AST-side variant of `intrinsic_borrow_mode` — derives the borrow
/// mode directly from an `ast::TypeExpr` (no resolve pass needed).
/// Used to seed the signature table from bundled stdlib source before
/// the IR-level fns are visited.
fn intrinsic_borrow_mode_from_type_expr(ty: &almide_lang::ast::TypeExpr) -> ParamBorrow {
    use almide_lang::ast::TypeExpr;
    match ty {
        TypeExpr::Simple { name } => {
            let n = name.as_str();
            match n {
                "Int" | "Int8" | "Int16" | "Int32" | "Int64"
                | "UInt8" | "UInt16" | "UInt32" | "UInt64"
                | "Float" | "Float32" | "Float64"
                | "Bool" | "Unit"
                    => ParamBorrow::Own,
                "String" => ParamBorrow::RefStr,
                "Bytes" => ParamBorrow::Ref,
                // Known stdlib struct types whose runtime fns uniformly
                // take `&T`. `Value` is the codec universal model,
                // `Matrix` / `AlmideMatrix` is the numeric tensor, both
                // heavy enough that pass-by-ref is the default.
                "Value" | "Matrix" | "AlmideMatrix" => ParamBorrow::Ref,
                // Named types, possibly type parameters (`A`, `B`) —
                // treat as Own so the caller keeps ownership. When the
                // concrete type is a heap value, the Borrow/Clone IR
                // nodes travel through unchanged.
                _ => ParamBorrow::Own,
            }
        }
        TypeExpr::Generic { name, .. } => {
            let n = name.as_str();
            match n {
                "List" => ParamBorrow::RefSlice,
                "Map" | "Set" => ParamBorrow::Ref,
                // Option / Result: consume by value (see doc on the
                // IR-side `intrinsic_borrow_mode`).
                "Option" | "Result" => ParamBorrow::Own,
                // `Matrix[T]` parametric form — same borrow surface as
                // bare `Matrix` (both map to `&AlmideMatrix` at the
                // runtime boundary).
                "Matrix" => ParamBorrow::Ref,
                _ => ParamBorrow::Own,
            }
        }
        TypeExpr::Record { .. } | TypeExpr::Variant { .. } => ParamBorrow::Ref,
        TypeExpr::Tuple { .. } => ParamBorrow::Ref,
        // Fn / OpenRecord / Union — pass owned.
        _ => ParamBorrow::Own,
    }
}

fn is_unit_type_expr(ty: &almide_lang::ast::TypeExpr) -> bool {
    use almide_lang::ast::TypeExpr;
    matches!(ty, TypeExpr::Simple { name } if name.as_str() == "Unit")
}

/// Borrow mode derived from an `@intrinsic` fn's Almide param type.
/// Used to populate the signature table so `BorrowInsertion` can
/// decorate call-site args uniformly without walker-side heuristics.
fn intrinsic_borrow_mode(ty: &Ty) -> ParamBorrow {
    match ty {
        // Owned scalars — pass by value.
        Ty::Int | Ty::Int8 | Ty::Int16 | Ty::Int32
        | Ty::UInt8 | Ty::UInt16 | Ty::UInt32 | Ty::UInt64
        | Ty::Float | Ty::Float32 | Ty::Bool | Ty::Unit
            => ParamBorrow::Own,

        // String → &str.
        Ty::String => ParamBorrow::RefStr,

        // List → &Vec / &[T].
        Ty::Applied(TypeConstructorId::List, _) => ParamBorrow::RefSlice,

        // Bytes / Record / Variant / Map / Set → & reference.
        Ty::Bytes
        | Ty::Record { .. } | Ty::Variant { .. }
        | Ty::Applied(TypeConstructorId::Map, _)
        | Ty::Applied(TypeConstructorId::Set, _)
            => ParamBorrow::Ref,

        // Option / Result → Own. `.unwrap_or` / `.map` consume the
        // container, and the walker renders `.is_some()` /
        // `.is_none()` via `Fn(Option<T>) -> bool` signatures that
        // accept the value by move and borrow internally. Passing a
        // `&Option<T>` would break the runtime-fn ergonomics for no
        // Almide-level gain.
        Ty::Applied(TypeConstructorId::Option, _)
        | Ty::Applied(TypeConstructorId::Result, _)
            => ParamBorrow::Own,

        // Generic TypeVar / user types / Fn / Tuple / etc. — pass owned.
        // The caller knows the concrete type; if it resolves to a borrow
        // type downstream, Clone/Borrow annotations travel through the
        // call unchanged.
        _ => ParamBorrow::Own,
    }
}

/// Eligible types for borrow inference. Bytes is the key addition here —
/// binary parsers clone the entire buffer on every read without it.
fn is_heap_type(ty: &Ty) -> bool {
    matches!(ty, Ty::String | Ty::Bytes | Ty::Applied(TypeConstructorId::List, _))
}

/// Check if a parameter variable needs ownership.
/// Conservative: marks as Owned if used in ANY ownership-requiring position.
fn check_needs_ownership(expr: &IrExpr, var: VarId, needs: &mut bool) {
    if *needs { return; }
    match &expr.kind {
        // ── Tail position: returned value needs ownership ──
        IrExprKind::Var { id } if *id == var => {
            // Bare var reference — context determines if ownership needed.
            // When used as a standalone expression (tail), it's returned → own.
            // But we handle tail detection at the Block level below.
        }

        IrExprKind::Block { stmts, expr: Some(tail) } => {
            for s in stmts { check_needs_ownership_stmt(s, var, needs); }
            if is_var(tail, var) { *needs = true; return; }
            check_needs_ownership(tail, var, needs);
        }
        IrExprKind::Block { stmts, expr: None } => {
            for s in stmts { check_needs_ownership_stmt(s, var, needs); }
        }

        // ── Concatenation consumes operands ──
        IrExprKind::BinOp { op: BinOp::ConcatStr | BinOp::ConcatList, left, right } => {
            if is_var(left, var) || is_var(right, var) { *needs = true; return; }
            check_needs_ownership(left, var, needs);
            check_needs_ownership(right, var, needs);
        }

        // ── Function call ──
        // For stdlib Module calls, consult arg_transforms to learn which args
        // are borrowed (BorrowRef / BorrowStr / BorrowMut) vs. consumed. Only
        // consumed args require ownership. This is what lets a hot loop like
        // `bytes.read_u32_le(data, pos)` pass `data` 50 000× without cloning.
        // For user-defined Named calls, consult the fixed-point SIGS snapshot
        // so a caller can transitively keep `data` borrowed when the callee
        // also borrows it.
        IrExprKind::Call { target, args, .. } => {
            // Bytes-only stdlib-aware: only skip ownership for Bytes args in
            // stdlib Module calls. Lists/Strings keep the old conservative
            // behaviour to avoid lambda-typing regressions in filter/map.
            if let CallTarget::Module { module, func } = target {
                if almide_lang::stdlib_info::is_bundled_module(module.as_str()) {
                    for (i, arg) in args.iter().enumerate() {
                        let borrowed = bundled_borrow_at(module.as_str(), func.as_str(), i)
                            && matches!(arg.ty, Ty::Bytes);
                        if !borrowed && is_var(arg, var) {
                            *needs = true;
                            return;
                        }
                    }
                    for arg in args { check_needs_ownership(arg, var, needs); }
                    return;
                }
            }
            // Self-recursive Named call: treat optimistically. For tail-recursive
            // parsers passing the same `data` through, we don't want the first-pass
            // pessimism to lock the param to Own and prevent the fixed point from
            // promoting it to Ref.
            if let CallTarget::Named { name } = target {
                let is_self = CURRENT_FN.with(|c| c.borrow().as_deref() == Some(name.as_str()));
                if is_self {
                    for arg in args { check_needs_ownership(arg, var, needs); }
                    return;
                }
            }
            // User-defined Named call: only skip ownership when the arg is Bytes
            // AND the callee borrows that slot.
            if let CallTarget::Named { name } = target {
                if let Some(borrows) = lookup_user_borrows(name.as_str()) {
                    for (i, arg) in args.iter().enumerate() {
                        let borrowed = borrows.get(i).map_or(false, |b| !matches!(b, ParamBorrow::Own))
                            && matches!(arg.ty, Ty::Bytes);
                        if !borrowed && is_var(arg, var) { *needs = true; return; }
                    }
                    for arg in args { check_needs_ownership(arg, var, needs); }
                    return;
                }
            }
            // Non-stdlib fallback: any arg use needs ownership.
            for arg in args {
                if is_var(arg, var) { *needs = true; return; }
            }
            if let CallTarget::Method { object, .. } = target {
                if is_var(object, var) { *needs = true; return; }
            }
            match target {
                CallTarget::Method { object, .. } => check_needs_ownership(object, var, needs),
                CallTarget::Computed { callee } => check_needs_ownership(callee, var, needs),
                _ => {}
            }
            for arg in args { check_needs_ownership(arg, var, needs); }
        }

        // ── Collection construction consumes ──
        IrExprKind::Record { fields, .. } => {
            for (_, v) in fields { if is_var(v, var) { *needs = true; return; } }
            for (_, v) in fields { check_needs_ownership(v, var, needs); }
        }
        IrExprKind::List { elements } | IrExprKind::Tuple { elements } => {
            for e in elements { if is_var(e, var) { *needs = true; return; } }
            for e in elements { check_needs_ownership(e, var, needs); }
        }
        IrExprKind::SpreadRecord { base, fields } => {
            if is_var(base, var) { *needs = true; return; }
            for (_, v) in fields { if is_var(v, var) { *needs = true; return; } }
            check_needs_ownership(base, var, needs);
            for (_, v) in fields { check_needs_ownership(v, var, needs); }
        }
        IrExprKind::MapLiteral { entries } => {
            for (k, v) in entries { if is_var(k, var) || is_var(v, var) { *needs = true; return; } }
        }

        // ── Wrapping in Result/Option/Some ──
        IrExprKind::ResultOk { expr } | IrExprKind::ResultErr { expr }
        | IrExprKind::OptionSome { expr } => {
            if is_var(expr, var) { *needs = true; return; }
            check_needs_ownership(expr, var, needs);
        }

        // ── Lambda capture: captured vars need ownership ──
        IrExprKind::Lambda { body, .. } => {
            if uses_var(body, var) { *needs = true; }
        }

        // ── String interpolation consumes ──
        IrExprKind::StringInterp { parts } => {
            for p in parts {
                if let IrStringPart::Expr { expr } = p {
                    if is_var(expr, var) { *needs = true; return; }
                    check_needs_ownership(expr, var, needs);
                }
            }
        }

        // ── ForIn: iterable is consumed ──
        IrExprKind::ForIn { iterable, body, .. } => {
            if is_var(iterable, var) { *needs = true; return; }
            check_needs_ownership(iterable, var, needs);
            for s in body { check_needs_ownership_stmt(s, var, needs); }
        }

        // ── IterChain: source consumed if consume=true ──
        IrExprKind::IterChain { source, consume, steps, collector } => {
            if *consume && is_var(source, var) { *needs = true; return; }
            check_needs_ownership(source, var, needs);
            for step in steps {
                match step {
                    IterStep::Map { lambda } | IterStep::Filter { lambda }
                    | IterStep::FlatMap { lambda } | IterStep::FilterMap { lambda } => {
                        if uses_var(lambda, var) { *needs = true; return; }
                    }
                }
            }
            match collector {
                IterCollector::Collect => {}
                IterCollector::Fold { init, lambda } => {
                    if is_var(init, var) { *needs = true; return; }
                    if uses_var(lambda, var) { *needs = true; return; }
                }
                IterCollector::Any { lambda } | IterCollector::All { lambda }
                | IterCollector::Find { lambda } | IterCollector::Count { lambda } => {
                    if uses_var(lambda, var) { *needs = true; return; }
                }
            }
        }

        // ── Safe reads (no ownership needed) ──
        IrExprKind::IndexAccess { object, index } | IrExprKind::MapAccess { object, key: index } => {
            // Indexing borrows — safe
            check_needs_ownership(object, var, needs);
            check_needs_ownership(index, var, needs);
        }
        IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. } => {
            check_needs_ownership(object, var, needs);
        }
        IrExprKind::BinOp { left, right, .. } => {
            // Non-concat binop: comparison, arithmetic — safe reads
            check_needs_ownership(left, var, needs);
            check_needs_ownership(right, var, needs);
        }

        // ── Control flow: recurse ──
        IrExprKind::If { cond, then, else_ } => {
            check_needs_ownership(cond, var, needs);
            check_needs_ownership(then, var, needs);
            check_needs_ownership(else_, var, needs);
        }
        IrExprKind::Match { subject, arms } => {
            // Match subject: destructuring a borrowed value changes bind types
            // → needs ownership to avoid &-pattern complications
            if is_var(subject, var) { *needs = true; return; }
            check_needs_ownership(subject, var, needs);
            for arm in arms {
                if let Some(g) = &arm.guard { check_needs_ownership(g, var, needs); }
                check_needs_ownership(&arm.body, var, needs);
            }
        }
        IrExprKind::While { cond, body } => {
            check_needs_ownership(cond, var, needs);
            for s in body { check_needs_ownership_stmt(s, var, needs); }
        }

        // ── Wrappers: recurse ──
        IrExprKind::UnOp { operand, .. } => check_needs_ownership(operand, var, needs),
        IrExprKind::Try { expr } | IrExprKind::Unwrap { expr } | IrExprKind::ToOption { expr }
        | IrExprKind::Clone { expr } | IrExprKind::Deref { expr }
        | IrExprKind::Borrow { expr, .. } | IrExprKind::BoxNew { expr }
        | IrExprKind::ToVec { expr } | IrExprKind::Await { expr } => {
            check_needs_ownership(expr, var, needs);
        }
        IrExprKind::UnwrapOr { expr, fallback } => {
            check_needs_ownership(expr, var, needs);
            check_needs_ownership(fallback, var, needs);
        }
        IrExprKind::OptionalChain { expr, .. } => check_needs_ownership(expr, var, needs),
        IrExprKind::Range { start, end, .. } => {
            check_needs_ownership(start, var, needs);
            check_needs_ownership(end, var, needs);
        }
        IrExprKind::Fan { exprs } => {
            for e in exprs { if is_var(e, var) { *needs = true; return; } }
            for e in exprs { check_needs_ownership(e, var, needs); }
        }
        IrExprKind::RustMacro { args, .. } => {
            for a in args { check_needs_ownership(a, var, needs); }
        }
        // RuntimeCall: lowered form of `@intrinsic` / bundled Module call.
        // Its borrow signature lives in SIGS_SNAPSHOT keyed by the mangled
        // symbol. If the arg slot is Own, the call consumes that arg and
        // the enclosing var must also be owned.
        IrExprKind::RuntimeCall { symbol, args } => {
            let borrows = SIGS_SNAPSHOT.with(|s| s.borrow().get(symbol.as_str()).cloned());
            if let Some(borrows) = borrows {
                for (i, arg) in args.iter().enumerate() {
                    let is_own = matches!(borrows.get(i), Some(ParamBorrow::Own));
                    if is_own && is_var(arg, var) { *needs = true; return; }
                }
            } else {
                // No sig: be conservative — any var arg needs ownership.
                for arg in args { if is_var(arg, var) { *needs = true; return; } }
            }
            for arg in args { check_needs_ownership(arg, var, needs); }
        }
        _ => {}
    }
}

fn check_needs_ownership_stmt(stmt: &IrStmt, var: VarId, needs: &mut bool) {
    if *needs { return; }
    match &stmt.kind {
        IrStmtKind::Bind { value, .. } | IrStmtKind::BindDestructure { value, .. }
        | IrStmtKind::Assign { value, .. } | IrStmtKind::FieldAssign { value, .. } => {
            check_needs_ownership(value, var, needs);
        }
        IrStmtKind::IndexAssign { index, value, .. } | IrStmtKind::MapInsert { key: index, value, .. } => {
            check_needs_ownership(index, var, needs);
            check_needs_ownership(value, var, needs);
        }
        IrStmtKind::Expr { expr } => check_needs_ownership(expr, var, needs),
        IrStmtKind::Guard { cond, else_ } => {
            check_needs_ownership(cond, var, needs);
            check_needs_ownership(else_, var, needs);
        }
        _ => {}
    }
}

fn is_var(expr: &IrExpr, var: VarId) -> bool {
    matches!(&expr.kind, IrExprKind::Var { id } if *id == var)
}

fn uses_var(expr: &IrExpr, var: VarId) -> bool {
    match &expr.kind {
        IrExprKind::Var { id } => *id == var,
        IrExprKind::Block { stmts, expr } => {
            stmts.iter().any(|s| stmt_uses_var(s, var))
            || expr.as_ref().map_or(false, |e| uses_var(e, var))
        }
        IrExprKind::If { cond, then, else_ } => uses_var(cond, var) || uses_var(then, var) || uses_var(else_, var),
        IrExprKind::Call { args, target, .. } => {
            match target {
                CallTarget::Method { object, .. } => { if uses_var(object, var) { return true; } }
                CallTarget::Computed { callee } => { if uses_var(callee, var) { return true; } }
                _ => {}
            }
            args.iter().any(|a| uses_var(a, var))
        }
        IrExprKind::BinOp { left, right, .. } => uses_var(left, var) || uses_var(right, var),
        IrExprKind::UnOp { operand, .. } => uses_var(operand, var),
        IrExprKind::Lambda { body, .. } => uses_var(body, var),
        IrExprKind::Match { subject, arms } => {
            uses_var(subject, var) || arms.iter().any(|a| {
                a.guard.as_ref().map_or(false, |g| uses_var(g, var)) || uses_var(&a.body, var)
            })
        }
        IrExprKind::ForIn { iterable, body, .. } => {
            uses_var(iterable, var) || body.iter().any(|s| stmt_uses_var(s, var))
        }
        IrExprKind::While { cond, body } => {
            uses_var(cond, var) || body.iter().any(|s| stmt_uses_var(s, var))
        }
        IrExprKind::Member { object, .. } | IrExprKind::TupleIndex { object, .. }
        | IrExprKind::OptionalChain { expr: object, .. } => uses_var(object, var),
        IrExprKind::IndexAccess { object, index } | IrExprKind::MapAccess { object, key: index } => {
            uses_var(object, var) || uses_var(index, var)
        }
        IrExprKind::StringInterp { parts } => parts.iter().any(|p| {
            matches!(p, IrStringPart::Expr { expr } if uses_var(expr, var))
        }),
        IrExprKind::ResultOk { expr } | IrExprKind::ResultErr { expr }
        | IrExprKind::OptionSome { expr } | IrExprKind::Try { expr }
        | IrExprKind::Unwrap { expr } | IrExprKind::ToOption { expr }
        | IrExprKind::Clone { expr } | IrExprKind::Deref { expr }
        | IrExprKind::Borrow { expr, .. } | IrExprKind::BoxNew { expr }
        | IrExprKind::ToVec { expr } | IrExprKind::Await { expr } => uses_var(expr, var),
        IrExprKind::UnwrapOr { expr, fallback } => uses_var(expr, var) || uses_var(fallback, var),
        IrExprKind::List { elements } | IrExprKind::Tuple { elements }
        | IrExprKind::Fan { exprs: elements } => elements.iter().any(|e| uses_var(e, var)),
        IrExprKind::Record { fields, .. } => fields.iter().any(|(_, v)| uses_var(v, var)),
        IrExprKind::SpreadRecord { base, fields } => {
            uses_var(base, var) || fields.iter().any(|(_, v)| uses_var(v, var))
        }
        IrExprKind::IterChain { source, steps, collector, .. } => {
            uses_var(source, var)
            || steps.iter().any(|s| match s {
                IterStep::Map { lambda } | IterStep::Filter { lambda }
                | IterStep::FlatMap { lambda } | IterStep::FilterMap { lambda } => uses_var(lambda, var),
            })
            || match collector {
                IterCollector::Collect => false,
                IterCollector::Fold { init, lambda } => uses_var(init, var) || uses_var(lambda, var),
                IterCollector::Any { lambda } | IterCollector::All { lambda }
                | IterCollector::Find { lambda } | IterCollector::Count { lambda } => uses_var(lambda, var),
            }
        }
        IrExprKind::RustMacro { args, .. } => args.iter().any(|a| uses_var(a, var)),
        IrExprKind::Range { start, end, .. } => uses_var(start, var) || uses_var(end, var),
        IrExprKind::MapLiteral { entries } => entries.iter().any(|(k, v)| uses_var(k, var) || uses_var(v, var)),
        _ => false,
    }
}

fn stmt_uses_var(stmt: &IrStmt, var: VarId) -> bool {
    match &stmt.kind {
        IrStmtKind::Bind { value, .. } | IrStmtKind::BindDestructure { value, .. }
        | IrStmtKind::Assign { value, .. } | IrStmtKind::FieldAssign { value, .. } => uses_var(value, var),
        IrStmtKind::IndexAssign { index, value, .. } | IrStmtKind::MapInsert { key: index, value, .. } => {
            uses_var(index, var) || uses_var(value, var)
        }
        IrStmtKind::Expr { expr } => uses_var(expr, var),
        IrStmtKind::Guard { cond, else_ } => uses_var(cond, var) || uses_var(else_, var),
        _ => false,
    }
}

// ── Phase 2: Insert Borrow nodes at call sites ─────────────────────

pub fn insert_borrows_at_call_sites(program: &mut IrProgram, sigs: &HashMap<String, Vec<ParamBorrow>>) {
    for func in &mut program.functions {
        func.body = rewrite_calls(std::mem::take(&mut func.body), sigs, None);
    }
    for tl in &mut program.top_lets {
        tl.value = rewrite_calls(std::mem::take(&mut tl.value), sigs, None);
    }
    for module in &mut program.modules {
        let mod_name = module.name.to_string();
        for func in &mut module.functions {
            func.body = rewrite_calls(std::mem::take(&mut func.body), sigs, Some(&mod_name));
        }
        for tl in &mut module.top_lets {
            tl.value = rewrite_calls(std::mem::take(&mut tl.value), sigs, Some(&mod_name));
        }
    }
}

fn rewrite_calls(expr: IrExpr, sigs: &HashMap<String, Vec<ParamBorrow>>, mod_scope: Option<&str>) -> IrExpr {
    let ty = expr.ty.clone();
    let span = expr.span;

    let kind = match expr.kind {
        IrExprKind::Call { target, args, type_args } => {
            let args: Vec<IrExpr> = args.into_iter().map(|a| rewrite_calls(a, sigs, mod_scope)).collect();

            // `is_method_with_self` marks that the call target carries a
            // receiver object that walker will splice in ahead of `args`.
            // In that case the sig's param list starts at the receiver,
            // and the IR `args` align to params 1..N (not 0..N).
            let (callee_name, is_method_with_self) = match &target {
                CallTarget::Named { name } => (Some(name.to_string()), false),
                CallTarget::Module { module, func } => (Some(format!("{}::{}", module, func)), false),
                CallTarget::Method { method, .. } if method.contains('.') => (Some(method.to_string()), true),
                _ => (None, false),
            };

            let args = if let Some(ref name) = callee_name {
                // For module-scoped calls, look up with "module::func" key first
                let borrows = mod_scope
                    .and_then(|m| sigs.get(&format!("{}::{}", m, name)))
                    .or_else(|| sigs.get(name));
                if let Some(borrows) = borrows {
                    let arg_offset = if is_method_with_self { 1 } else { 0 };
                    args.into_iter().enumerate().map(|(i, arg)| {
                        match borrows.get(i + arg_offset) {
                            Some(ParamBorrow::Ref | ParamBorrow::RefSlice) => {
                                let t = arg.ty.clone(); let s = arg.span;
                                IrExpr { kind: IrExprKind::Borrow { expr: Box::new(arg), as_str: false, mutable: false }, ty: t, span: s }
                            }
                            Some(ParamBorrow::RefMut) => {
                                let t = arg.ty.clone(); let s = arg.span;
                                IrExpr { kind: IrExprKind::Borrow { expr: Box::new(arg), as_str: false, mutable: true }, ty: t, span: s }
                            }
                            Some(ParamBorrow::RefStr) => {
                                let t = arg.ty.clone(); let s = arg.span;
                                IrExpr { kind: IrExprKind::Borrow { expr: Box::new(arg), as_str: true, mutable: false }, ty: t, span: s }
                            }
                            _ => arg,
                        }
                    }).collect()
                } else { args }
            } else { args };

            let target = match target {
                CallTarget::Method { object, method } => {
                    let mut obj = rewrite_calls(*object, sigs, mod_scope);
                    if method.contains('.') {
                        if let Some(borrows) = sigs.get(method.as_str()) {
                            if let Some(b) = borrows.first() {
                                match b {
                                    ParamBorrow::Ref | ParamBorrow::RefSlice => {
                                        let t = obj.ty.clone(); let s = obj.span;
                                        obj = IrExpr { kind: IrExprKind::Borrow { expr: Box::new(obj), as_str: false, mutable: false }, ty: t, span: s };
                                    }
                                    ParamBorrow::RefStr => {
                                        let t = obj.ty.clone(); let s = obj.span;
                                        obj = IrExpr { kind: IrExprKind::Borrow { expr: Box::new(obj), as_str: true, mutable: false }, ty: t, span: s };
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                    CallTarget::Method { object: Box::new(obj), method }
                },
                CallTarget::Computed { callee } => CallTarget::Computed {
                    callee: Box::new(rewrite_calls(*callee, sigs, mod_scope)),
                },
                other => other,
            };
            IrExprKind::Call { target, args, type_args }
        }

        IrExprKind::Block { stmts, expr } => IrExprKind::Block {
            stmts: stmts.into_iter().map(|s| rewrite_calls_stmt(s, sigs, mod_scope)).collect(),
            expr: expr.map(|e| Box::new(rewrite_calls(*e, sigs, mod_scope))),
        },
        IrExprKind::If { cond, then, else_ } => IrExprKind::If {
            cond: Box::new(rewrite_calls(*cond, sigs, mod_scope)),
            then: Box::new(rewrite_calls(*then, sigs, mod_scope)),
            else_: Box::new(rewrite_calls(*else_, sigs, mod_scope)),
        },
        IrExprKind::Match { subject, arms } => IrExprKind::Match {
            subject: Box::new(rewrite_calls(*subject, sigs, mod_scope)),
            arms: arms.into_iter().map(|a| IrMatchArm {
                pattern: a.pattern,
                guard: a.guard.map(|g| rewrite_calls(g, sigs, mod_scope)),
                body: rewrite_calls(a.body, sigs, mod_scope),
            }).collect(),
        },
        IrExprKind::ForIn { var, var_tuple, iterable, body } => IrExprKind::ForIn {
            var, var_tuple,
            iterable: Box::new(rewrite_calls(*iterable, sigs, mod_scope)),
            body: body.into_iter().map(|s| rewrite_calls_stmt(s, sigs, mod_scope)).collect(),
        },
        IrExprKind::While { cond, body } => IrExprKind::While {
            cond: Box::new(rewrite_calls(*cond, sigs, mod_scope)),
            body: body.into_iter().map(|s| rewrite_calls_stmt(s, sigs, mod_scope)).collect(),
        },
        IrExprKind::Lambda { params, body, lambda_id } => IrExprKind::Lambda {
            params, body: Box::new(rewrite_calls(*body, sigs, mod_scope)), lambda_id,
        },
        IrExprKind::BinOp { op, left, right } => IrExprKind::BinOp {
            op, left: Box::new(rewrite_calls(*left, sigs, mod_scope)), right: Box::new(rewrite_calls(*right, sigs, mod_scope)),
        },
        IrExprKind::UnOp { op, operand } => IrExprKind::UnOp {
            op, operand: Box::new(rewrite_calls(*operand, sigs, mod_scope)),
        },
        IrExprKind::ResultOk { expr } => IrExprKind::ResultOk { expr: Box::new(rewrite_calls(*expr, sigs, mod_scope)) },
        IrExprKind::ResultErr { expr } => IrExprKind::ResultErr { expr: Box::new(rewrite_calls(*expr, sigs, mod_scope)) },
        IrExprKind::OptionSome { expr } => IrExprKind::OptionSome { expr: Box::new(rewrite_calls(*expr, sigs, mod_scope)) },
        IrExprKind::Try { expr } => IrExprKind::Try { expr: Box::new(rewrite_calls(*expr, sigs, mod_scope)) },
        IrExprKind::Unwrap { expr } => IrExprKind::Unwrap { expr: Box::new(rewrite_calls(*expr, sigs, mod_scope)) },
        IrExprKind::UnwrapOr { expr, fallback } => IrExprKind::UnwrapOr {
            expr: Box::new(rewrite_calls(*expr, sigs, mod_scope)),
            fallback: Box::new(rewrite_calls(*fallback, sigs, mod_scope)),
        },
        IrExprKind::StringInterp { parts } => IrExprKind::StringInterp {
            parts: parts.into_iter().map(|p| match p {
                IrStringPart::Expr { expr } => IrStringPart::Expr { expr: rewrite_calls(expr, sigs, mod_scope) },
                other => other,
            }).collect(),
        },
        IrExprKind::Fan { exprs } => IrExprKind::Fan {
            exprs: exprs.into_iter().map(|e| rewrite_calls(e, sigs, mod_scope)).collect(),
        },
        IrExprKind::IterChain { source, consume, steps, collector } => IrExprKind::IterChain {
            source: Box::new(rewrite_calls(*source, sigs, mod_scope)),
            consume, steps, collector,
        },
        IrExprKind::Record { name, fields } => IrExprKind::Record {
            name,
            fields: fields.into_iter()
                .map(|(k, v)| (k, rewrite_calls(v, sigs, mod_scope))).collect(),
        },
        IrExprKind::SpreadRecord { base, fields } => IrExprKind::SpreadRecord {
            base: Box::new(rewrite_calls(*base, sigs, mod_scope)),
            fields: fields.into_iter()
                .map(|(k, v)| (k, rewrite_calls(v, sigs, mod_scope))).collect(),
        },
        IrExprKind::List { elements } => IrExprKind::List {
            elements: elements.into_iter()
                .map(|e| rewrite_calls(e, sigs, mod_scope)).collect(),
        },
        IrExprKind::Tuple { elements } => IrExprKind::Tuple {
            elements: elements.into_iter()
                .map(|e| rewrite_calls(e, sigs, mod_scope)).collect(),
        },
        IrExprKind::MapLiteral { entries } => IrExprKind::MapLiteral {
            entries: entries.into_iter()
                .map(|(k, v)| (rewrite_calls(k, sigs, mod_scope), rewrite_calls(v, sigs, mod_scope))).collect(),
        },
        IrExprKind::Member { object, field } => IrExprKind::Member {
            object: Box::new(rewrite_calls(*object, sigs, mod_scope)), field,
        },
        IrExprKind::TupleIndex { object, index } => IrExprKind::TupleIndex {
            object: Box::new(rewrite_calls(*object, sigs, mod_scope)), index,
        },
        IrExprKind::OptionalChain { expr, field } => IrExprKind::OptionalChain {
            expr: Box::new(rewrite_calls(*expr, sigs, mod_scope)), field,
        },
        IrExprKind::IndexAccess { object, index } => IrExprKind::IndexAccess {
            object: Box::new(rewrite_calls(*object, sigs, mod_scope)),
            index: Box::new(rewrite_calls(*index, sigs, mod_scope)),
        },
        IrExprKind::MapAccess { object, key } => IrExprKind::MapAccess {
            object: Box::new(rewrite_calls(*object, sigs, mod_scope)),
            key: Box::new(rewrite_calls(*key, sigs, mod_scope)),
        },
        IrExprKind::Range { start, end, inclusive } => IrExprKind::Range {
            start: Box::new(rewrite_calls(*start, sigs, mod_scope)),
            end: Box::new(rewrite_calls(*end, sigs, mod_scope)),
            inclusive,
        },
        IrExprKind::Clone { expr } => IrExprKind::Clone { expr: Box::new(rewrite_calls(*expr, sigs, mod_scope)) },
        IrExprKind::Deref { expr } => IrExprKind::Deref { expr: Box::new(rewrite_calls(*expr, sigs, mod_scope)) },
        IrExprKind::Borrow { expr, as_str, mutable } => IrExprKind::Borrow {
            expr: Box::new(rewrite_calls(*expr, sigs, mod_scope)), as_str, mutable,
        },
        IrExprKind::BoxNew { expr } => IrExprKind::BoxNew { expr: Box::new(rewrite_calls(*expr, sigs, mod_scope)) },
        IrExprKind::ToVec { expr } => IrExprKind::ToVec { expr: Box::new(rewrite_calls(*expr, sigs, mod_scope)) },
        IrExprKind::ToOption { expr } => IrExprKind::ToOption { expr: Box::new(rewrite_calls(*expr, sigs, mod_scope)) },
        IrExprKind::Await { expr } => IrExprKind::Await { expr: Box::new(rewrite_calls(*expr, sigs, mod_scope)) },
        IrExprKind::RustMacro { name, args } => IrExprKind::RustMacro {
            name, args: args.into_iter().map(|a| rewrite_calls(a, sigs, mod_scope)).collect(),
        },
        IrExprKind::RuntimeCall { symbol, args } => {
            let args: Vec<IrExpr> = args.into_iter()
                .map(|a| rewrite_calls(a, sigs, mod_scope))
                .collect();
            // Look up the borrow signature by the mangled runtime symbol
            // (populated from bundled `@intrinsic` attrs at the top of
            // `infer_borrow_signatures`). On hit, wrap each arg with the
            // corresponding Borrow IR node; on miss, leave args untouched
            // (walker still has its ty-based fallback).
            let args = if let Some(borrows) = sigs.get(symbol.as_str()) {
                args.into_iter().enumerate().map(|(i, arg)| {
                    match borrows.get(i) {
                        Some(ParamBorrow::Ref | ParamBorrow::RefSlice) => {
                            let t = arg.ty.clone(); let s = arg.span;
                            IrExpr { kind: IrExprKind::Borrow { expr: Box::new(arg), as_str: false, mutable: false }, ty: t, span: s }
                        }
                        Some(ParamBorrow::RefMut) => {
                            let t = arg.ty.clone(); let s = arg.span;
                            IrExpr { kind: IrExprKind::Borrow { expr: Box::new(arg), as_str: false, mutable: true }, ty: t, span: s }
                        }
                        Some(ParamBorrow::RefStr) => {
                            let t = arg.ty.clone(); let s = arg.span;
                            IrExpr { kind: IrExprKind::Borrow { expr: Box::new(arg), as_str: true, mutable: false }, ty: t, span: s }
                        }
                        _ => arg,
                    }
                }).collect()
            } else { args };
            IrExprKind::RuntimeCall { symbol, args }
        }
        other => other,
    };

    IrExpr { kind, ty, span }
}

fn rewrite_calls_stmt(stmt: IrStmt, sigs: &HashMap<String, Vec<ParamBorrow>>, mod_scope: Option<&str>) -> IrStmt {
    let kind = match stmt.kind {
        IrStmtKind::Bind { var, mutability, ty, value } => IrStmtKind::Bind {
            var, mutability, ty, value: rewrite_calls(value, sigs, mod_scope),
        },
        IrStmtKind::Assign { var, value } => IrStmtKind::Assign { var, value: rewrite_calls(value, sigs, mod_scope) },
        IrStmtKind::Expr { expr } => IrStmtKind::Expr { expr: rewrite_calls(expr, sigs, mod_scope) },
        IrStmtKind::Guard { cond, else_ } => IrStmtKind::Guard {
            cond: rewrite_calls(cond, sigs, mod_scope), else_: rewrite_calls(else_, sigs, mod_scope),
        },
        IrStmtKind::BindDestructure { pattern, value } => IrStmtKind::BindDestructure {
            pattern, value: rewrite_calls(value, sigs, mod_scope),
        },
        other => other,
    };
    IrStmt { kind, span: stmt.span }
}
