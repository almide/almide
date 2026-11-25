//! C-461 (#4006, owner ruling 2026-11-24 W1): an `@extern` fn with no binding
//! for the native target and no Almide body has nothing a native binary could
//! run. Declared only for another target (`@extern(wasm, "js", ...)`, the JS
//! host's hook), it lowered to a typed hole the native build compiled into an
//! abort, and that build succeeded through the unverified codegen. The native
//! routes (`build`, `run`, `bench`, `verify --mir`, and `check --target
//! native`, which reports their verdict) refuse such a fn at check time
//! instead, with E081, before any render, when the program's native roots reach it
//! (`reachability::reachable_native_program`: `main`, `@export(c)` fns,
//! top-level `let` initializers). A fn that also declares `@extern(rust, ...)`
//! or `@extern(c, ...)` binds through that; one with an Almide body runs the
//! body natively. Plain `almide check` stays the language check every target
//! shares, so a program written for the JS host checks clean; `--target wasm`
//! / `--target wasi` judge their own routes.
//!
//! The same routes refuse, with their own diagnostics, the two shapes the
//! verified native render has no form for (owner ruling 2026-11-24: before
//! a wall became a build error, each known shape outside the corpus is
//! rendered or refused here): an `@extern(rust)` fn returning a `Region`
//! (E090) and a `Set` whose element holds a nested `Float` (E081).

use almide::ir::{IrExprKind, IrFunction, IrProgram};

/// The fns `ir`'s native program reaches that have no native implementation,
/// as `(name, declared attrs, line in the entry file)`, in declaration order
/// (a module's fn names its module instead of a line of the entry).
fn unbound(ir: &IrProgram) -> Vec<(String, String, Option<usize>)> {
    let reachable = almide::codegen::reachability::reachable_native_program(ir);
    let is_reachable = |module: Option<&str>, name: &str| {
        almide::codegen::reachability::registered_keys(module, name).iter().any(|k| reachable.contains(k))
    };
    let has_no_native_impl = |f: &IrFunction| {
        !f.extern_attrs.is_empty()
            && !f.extern_attrs.iter().any(|a| almide::types::extern_abi::native_extern_kind(a.target.as_str()).is_some())
            && matches!(f.body.kind, IrExprKind::Hole)
    };
    let attrs = |f: &IrFunction| {
        f.extern_attrs
            .iter()
            .map(|a| format!("@extern({}, \"{}\", \"{}\")", a.target.as_str(), a.module.as_str(), a.function.as_str()))
            .collect::<Vec<_>>()
            .join(" ")
    };
    let top = ir.functions.iter().filter(|f| has_no_native_impl(f) && is_reachable(None, f.name.as_str())).map(|f| (f.name.to_string(), attrs(f), f.body.span.map(|s| s.line)));
    let modules = ir.modules.iter().flat_map(|m| {
        let mname = m.name.to_string();
        m.functions
            .iter()
            .filter(move |f| has_no_native_impl(f) && is_reachable(Some(&mname), f.name.as_str()))
            .map(move |f| (format!("{}.{}", m.name.as_str(), f.name.as_str()), attrs(f), None))
    });
    top.chain(modules).collect()
}

/// The fns the native program reaches, as `(display name, fn, line in the
/// entry file)`, in declaration order.
fn reachable_fns(ir: &IrProgram) -> Vec<(String, &IrFunction, Option<usize>)> {
    let reachable = almide::codegen::reachability::reachable_native_program(ir);
    let is_reachable = |module: Option<&str>, name: &str| {
        almide::codegen::reachability::registered_keys(module, name).iter().any(|k| reachable.contains(k))
    };
    let top = ir.functions.iter().filter(|f| is_reachable(None, f.name.as_str())).map(|f| (f.name.to_string(), f, f.body.span.map(|s| s.line)));
    let modules = ir.modules.iter().flat_map(|m| {
        let mname = m.name.to_string();
        m.functions
            .iter()
            .filter(move |f| is_reachable(Some(&mname), f.name.as_str()))
            .map(move |f| (format!("{}.{}", m.name.as_str(), f.name.as_str()), f, None))
    });
    top.chain(modules).collect()
}

/// #4006 (owner ruling 2026-11-24, the flip): a reachable `@extern(rust)` fn
/// whose return carries a `Region`. The Rust host ABI lends a Region IN as its
/// `(ptr, len)` for the call; nothing defines a Region coming OUT of a Rust
/// host (whose allocation, which length, who frees it), so a native binary
/// has no verified form for it. `@extern(c)` states those with
/// `@region(len = …, release = "…")` (ADR-0031 D2).
fn rust_region_returns(ir: &IrProgram) -> Vec<(String, Option<usize>)> {
    use almide::types::extern_abi::{is_region, native_extern_kind, NativeExtern};
    reachable_fns(ir)
        .into_iter()
        .filter(|(_, f, _)| {
            f.extern_attrs.iter().find_map(|a| native_extern_kind(a.target.as_str())) == Some(NativeExtern::Rust)
                && f.ret_ty.any_node(&mut |t| is_region(t))
        })
        .map(|(name, _, line)| (name, line))
        .collect()
}

/// #4006 (owner ruling 2026-11-24, the flip): a `Set` whose element holds a
/// `Float` the native render has no key equality for. Set membership is the
/// collection key equality (ALS-C11, ADR-0039: every NaN one key, -0.0 the
/// key 0.0). A top-level `Float` element and a `List` / `Option` of Floats
/// hold real floats in the set, and a record, tuple or variant whose Float
/// components are its own fields (a Float, a `List[Float]`, or such a type)
/// writes canonical key bytes; all are served. A Float deeper inside a runtime
/// container (`Set[List[List[Float]]]`, a record field `Option[Float]`) rides
/// a lane the render cannot tell from an Int one, so the set is refused.
fn inexact_sets(ir: &IrProgram) -> Vec<(String, String, Option<usize>)> {
    use almide::ir::{IrExpr, IrStmt, IrStmtKind, IrTypeDecl, IrTypeDeclKind, IrVariantKind, IrVisitor};
    use almide::types::{Ty, TypeConstructorId};
    let decls: Vec<&IrTypeDecl> = ir.type_decls.iter().chain(ir.modules.iter().flat_map(|m| &m.type_decls)).collect();
    fn holds_float(t: &Ty, decls: &[&IrTypeDecl], seen: &mut Vec<String>) -> bool {
        t.any_node(&mut |n| match n {
            Ty::Float | Ty::Float32 | Ty::Float64 => true,
            Ty::Named(name, _) if !seen.iter().any(|s| s == name.as_str()) => {
                seen.push(name.to_string());
                decls.iter().filter(|d| d.name == *name).any(|d| {
                    let members: Vec<&Ty> = match &d.kind {
                        IrTypeDeclKind::Record { fields } => fields.iter().map(|f| &f.ty).collect(),
                        IrTypeDeclKind::Alias { target } => vec![target],
                        IrTypeDeclKind::Variant { cases, .. } => cases.iter().flat_map(|c| match &c.kind {
                            IrVariantKind::Unit => vec![],
                            IrVariantKind::Tuple { fields } => fields.iter().collect(),
                            IrVariantKind::Record { fields } => fields.iter().map(|f| &f.ty).collect(),
                        }).collect(),
                    };
                    members.into_iter().any(|m| holds_float(m, decls, seen))
                })
            }
            _ => false,
        })
    }
    // A generated type whose every Float-holding field is a Float, a
    // `List[Float]`, or such a type again (render_native_eq.rs
    // `float_key_type`).
    fn keyed(t: &Ty, decls: &[&IrTypeDecl], seen: &mut Vec<String>) -> bool {
        let field = |f: &Ty, seen: &mut Vec<String>| {
            !holds_float(f, decls, &mut Vec::new())
                || matches!(f, Ty::Float | Ty::Float32 | Ty::Float64)
                || matches!(f, Ty::Applied(TypeConstructorId::List, a) if a.len() == 1 && matches!(a[0], Ty::Float | Ty::Float32 | Ty::Float64))
                || keyed(f, decls, seen)
        };
        match t {
            Ty::Tuple(es) => es.iter().all(|e| field(e, seen)),
            Ty::Record { fields } => fields.iter().all(|(_, f)| field(f, seen)),
            Ty::Named(name, _) => {
                if seen.iter().any(|s| s == name.as_str()) {
                    return true;
                }
                seen.push(name.to_string());
                let mut found = decls.iter().filter(|d| d.name == *name).peekable();
                found.peek().is_some()
                    && found.all(|d| match &d.kind {
                        IrTypeDeclKind::Record { fields } => fields.iter().all(|f| field(&f.ty, seen)),
                        IrTypeDeclKind::Alias { target } => keyed(target, decls, seen),
                        IrTypeDeclKind::Variant { cases, .. } => cases.iter().all(|c| match &c.kind {
                            IrVariantKind::Unit => true,
                            IrVariantKind::Tuple { fields } => fields.iter().all(|f| field(f, seen)),
                            IrVariantKind::Record { fields } => fields.iter().all(|f| field(&f.ty, seen)),
                        }),
                    })
            }
            _ => false,
        }
    }
    // A List or Option of Floats: the native set holds its real floats.
    fn float_carrier(t: &Ty) -> bool {
        matches!(t, Ty::Applied(TypeConstructorId::List | TypeConstructorId::Option, a) if a.len() == 1 && matches!(a[0], Ty::Float | Ty::Float32 | Ty::Float64))
    }
    let inexact = |t: &Ty| -> Option<Ty> {
        let mut found = None;
        t.any_node(&mut |n| {
            if let Ty::Applied(TypeConstructorId::Set, a) = n {
                if a.len() == 1
                    && !matches!(a[0], Ty::Float | Ty::Float32 | Ty::Float64)
                    && holds_float(&a[0], &decls, &mut Vec::new())
                    && !keyed(&a[0], &decls, &mut Vec::new())
                    && !float_carrier(&a[0])
                {
                    found = Some(n.clone());
                    return true;
                }
            }
            false
        });
        found
    };
    struct Types<'a, F: Fn(&Ty) -> Option<Ty>>(&'a F, Option<Ty>);
    impl<F: Fn(&Ty) -> Option<Ty>> IrVisitor for Types<'_, F> {
        fn visit_expr(&mut self, e: &IrExpr) {
            if self.1.is_none() { self.1 = (self.0)(&e.ty); }
            almide::ir::walk_expr(self, e);
        }
        fn visit_stmt(&mut self, s: &IrStmt) {
            if let (None, IrStmtKind::Bind { ty, .. }) = (&self.1, &s.kind) { self.1 = (self.0)(ty); }
            almide::ir::walk_stmt(self, s);
        }
    }
    reachable_fns(ir)
        .into_iter()
        .filter_map(|(name, f, line)| {
            let sig = f.params.iter().map(|p| &p.ty).chain(std::iter::once(&f.ret_ty)).find_map(|t| inexact(t));
            let ty = sig.or_else(|| {
                let mut v = Types(&inexact, None);
                v.visit_expr(&f.body);
                v.1
            })?;
            Some((name, ty.display().to_string(), line))
        })
        .collect()
}

/// The refusals for `ir` on the native target, one error text per fn.
pub(crate) fn refusals(ir: &IrProgram, file: &str) -> Vec<(&'static str, String)> {
    let at = |line: Option<usize>| line.map(|l| format!("\n  --> {file}:{l}")).unwrap_or_default();
    let unbound = unbound(ir).into_iter().map(|(name, attrs, line)| {
        ("E081", format!(
            "error[E081]: `{name}` has no implementation on the native target{}\n  \
             reason: it is declared only {attrs} and has no Almide body, so a native binary has nothing to run when it is called\n  \
             try: build it for its host (`almide build {file} --target wasm --host js`), add an `@extern(rust, ...)` or `@extern(c, ...)` binding, or give the fn an Almide body the native build runs\n  \
             note: C-461; the wasm route (`almide check --target wasm`) binds the import",
            at(line)
        ))
    });
    let regions = rust_region_returns(ir).into_iter().map(|(name, line)| {
        ("E090", format!(
            "error[E090]: `@extern(rust)` fn `{name}` returns a Region, which has no native form{}\n  \
             reason: a Rust host takes a Region as its `(ptr, len)` for the call; no ABI says how one comes back — whose allocation, which length, who frees it\n  \
             try: return `Bytes` (the host's `Vec<u8>`) and take its Region with `ffi.region(b)`, or bind the C function with `@extern(c, ...)` and `@region(len = n, release = \"free_fn\")` (ADR-0031 D2)",
            at(line)
        ))
    });
    let sets = inexact_sets(ir).into_iter().map(|(name, ty, line)| {
        ("E081", format!(
            "error[E081]: fn `{name}` holds a `{ty}`, which has no implementation on the native target{}\n  \
             reason: a Set's membership is the collection key equality (every NaN one key, -0.0 the key 0.0); the verified native render carries a Float inside a List or Option element as its bits and has no key equality for it\n  \
             try: wrap the element in a record (`type Key = {{ xs: List[List[Float]] }}`), whose Float fields the native render keys, or build it for wasm (`--target wasm`)",
            at(line)
        ))
    });
    unbound.chain(regions).chain(sets).collect()
}

/// Print `ir`'s native refusals; `Err` when there was one.
pub(crate) fn check(ir: &IrProgram, file: &str) -> Result<(), ()> {
    let rows = refusals(ir, file);
    for (_, text) in &rows {
        crate::err(text);
    }
    if rows.is_empty() { Ok(()) } else { Err(()) }
}
