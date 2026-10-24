//! Which auto-imported bundled stdlib modules a program REACHES (#4071).
//!
//! Auto-import is a naming convenience: `list.map(xs, f)` and `xs.map(f)`
//! work without `import list`. It used to be a loading rule as well — every
//! module in `AUTO_IMPORT_BUNDLED` was parsed, checked and lowered into every
//! program, hello-world included, so the stdlib's size was a fixed cost of
//! every compile and every native emit carried every Almide-bodied stdlib
//! function. Now a module is loaded exactly when something already loaded
//! names it, and this file is the one place that says what "names it" means.
//! The resolver (`src/resolve.rs`), the single-file harness resolver
//! (`almide-mir::pipeline::bundled_self_modules`) and, through the resolver,
//! the spine and the structural wasm leg all ask [`reached_auto_bundled`], so
//! the native and wasm legs link the same module set.
//!
//! The rule is an over-approximation read off the SPELLED program, never a
//! list of module names: module `M` is reached when a loaded source spells
//!
//! - the identifier `M` (`M.f(..)`, `M.f` passed as a value, `|> M.f`, a
//!   test `where M.f = ..` override, a `M.T` type) — a qualified reference
//!   reaches `M` and only `M`, or
//! - a member name `f` that `M` declares as a function, on a receiver that
//!   is not a module name (UFCS: `xs.f(..)` resolves by receiver type, which
//!   is not known before checking, so every module declaring `f` is
//!   reached). A module name that the sources also BIND (`let list = "s"`)
//!   is read as a receiver, so `list.trim()` there reaches `string`, or
//! - a type or variant constructor that `M` declares.
//!
//! What `M` declares is read from `M`'s own embedded source. The closure is
//! transitive: a reached module's body is a loaded source too (`int8` calls
//! `int.*`). An over-approximation only costs time; a miss is a missing body,
//! which is why every position the parser keeps a name in is walked.

use std::collections::HashSet;
use std::sync::OnceLock;

use almide_lang::ast::{
    Decl, Expr, ExprKind, GenericParam, Param, Pattern, Program, ProtocolRef, Stmt,
    TestWhere, TypeExpr, VariantCase,
};
use almide_lang::stdlib_info::{bundled_source, AUTO_IMPORT_BUNDLED};

/// The names one source spells, split by the position they can reach a
/// module from.
#[derive(Default)]
struct Spelled {
    /// Bare identifiers, type names, constructor names — a declared
    /// type/ctor, or (in type position) a module qualifier.
    idents: HashSet<String>,
    /// Member names on a receiver that is not a bare name (`f(x).g()`,
    /// `p.q.r`) — a UFCS method candidate.
    members: HashSet<String>,
    /// `a.f` on a bare name `a`: a qualified call when `a` is a module, a
    /// UFCS call when `a` is a binding (`let list = "s"; list.len()` is
    /// `string.len`).
    qualified: HashSet<(String, String)>,
    /// Every name the sources bind (params, lets, patterns, loop vars). A
    /// module name bound anywhere is read as a binding wherever it is spelled.
    binders: HashSet<String>,
}

impl Spelled {
    /// Is `a` in `a.f` (certainly) a module rather than a binding?
    fn is_module_qualifier(&self, a: &str) -> bool {
        !self.binders.contains(a)
            && (almide_lang::stdlib_info::is_any_stdlib(a) || almide_lang::stdlib_info::is_bundled_module(a))
    }
}

/// What an auto-imported module declares: its function names (the UFCS
/// surface) and its type and constructor names.
struct Surface {
    fns: HashSet<String>,
    types: HashSet<String>,
}

/// Every auto-imported bundled module's parsed program and surface, built
/// once per process from the embedded sources.
fn auto_modules() -> &'static [(&'static str, &'static Program, Surface)] {
    static CELL: OnceLock<Vec<(&'static str, &'static Program, Surface)>> = OnceLock::new();
    CELL.get_or_init(|| {
        AUTO_IMPORT_BUNDLED
            .iter()
            .filter_map(|name| {
                let prog = almide_lang::parse_cached(bundled_source(name)?)?;
                let mut fns = HashSet::new();
                let mut types = HashSet::new();
                for d in &prog.decls {
                    match d {
                        Decl::Fn { name, .. } => {
                            fns.insert(name.to_string());
                        }
                        Decl::Type { name, ty, .. } => {
                            types.insert(name.to_string());
                            if let TypeExpr::Variant { cases, .. } = ty {
                                for c in cases {
                                    types.insert(variant_case_name(c).to_string());
                                }
                            }
                        }
                        _ => {}
                    }
                }
                Some((*name, prog, Surface { fns, types }))
            })
            .collect()
    })
}

/// The auto-imported bundled modules (in `AUTO_IMPORT_BUNDLED` order) that
/// `roots` reach, transitively through the reached modules' own bodies.
/// `roots` is every source already loaded: the entry program, its project
/// and package modules, and explicitly imported bundled modules.
pub fn reached_auto_bundled<'a>(roots: impl IntoIterator<Item = &'a Program>) -> Vec<&'static str> {
    let mut spelled = Spelled::default();
    for p in roots {
        spell_program(p, &mut spelled);
    }
    let modules = auto_modules();
    let mut reached: HashSet<&'static str> = HashSet::new();
    loop {
        let mut grew = false;
        for (name, prog, surface) in modules {
            if reached.contains(name) || !reaches(&spelled, name, surface) {
                continue;
            }
            reached.insert(name);
            spell_program(prog, &mut spelled);
            grew = true;
        }
        if !grew {
            break;
        }
    }
    modules.iter().map(|(n, ..)| *n).filter(|n| reached.contains(n)).collect()
}

fn reaches(spelled: &Spelled, name: &str, surface: &Surface) -> bool {
    let qualified_here = spelled
        .qualified
        .iter()
        .any(|(a, _)| a == name && spelled.is_module_qualifier(a));
    // A member on a binding (or on anything that is not a module name) may
    // be a UFCS call into any module declaring it.
    let ufcs = |f: &String| {
        spelled.members.contains(f)
            || spelled.qualified.iter().any(|(a, m)| m == f && !spelled.is_module_qualifier(a))
    };
    qualified_here
        || spelled.idents.contains(name)
        || surface.fns.iter().any(ufcs)
        || surface.types.iter().any(|t| spelled.idents.contains(t))
}

fn variant_case_name(c: &VariantCase) -> &str {
    match c {
        VariantCase::Unit { name } | VariantCase::Tuple { name, .. } | VariantCase::Record { name, .. } => {
            name.as_str()
        }
    }
}

fn spell_program(p: &Program, out: &mut Spelled) {
    for d in p.decls.iter().chain(p.imports.iter()) {
        spell_decl(d, out);
    }
}

/// Exhaustive over `Decl` with no wildcard: a new declaration form fails to
/// compile here until the names it can spell are walked.
fn spell_decl(d: &Decl, out: &mut Spelled) {
    match d {
        Decl::Fn { generics, params, return_type, body, .. } => {
            spell_generics(generics, out);
            spell_params(params, out);
            spell_type(return_type, out);
            if let Some(b) = body {
                spell_expr(b, out);
            }
        }
        Decl::Test { body, where_clauses, .. } => {
            spell_test_wheres(where_clauses, out);
            spell_expr(body, out);
        }
        Decl::TestWhereDef { clauses, .. } => spell_test_wheres(clauses, out),
        Decl::TopLet { name, ty, value, .. } => {
            out.binders.insert(name.to_string());
            if let Some(t) = ty {
                spell_type(t, out);
            }
            spell_expr(value, out);
        }
        Decl::Type { ty, deriving_refs, generics, .. } => {
            spell_type(ty, out);
            spell_protocol_refs(deriving_refs.as_deref(), out);
            spell_generics(generics, out);
        }
        Decl::Protocol { generics, methods, .. } => {
            spell_generics(generics, out);
            for m in methods {
                spell_params(&m.params, out);
                spell_type(&m.return_type, out);
            }
        }
        Decl::Module { .. } | Decl::Import { .. } => {}
    }
}

fn spell_params(params: &[Param], out: &mut Spelled) {
    for p in params {
        out.binders.insert(p.name.to_string());
        spell_type(&p.ty, out);
        if let Some(d) = &p.default {
            spell_expr(d, out);
        }
    }
}

fn spell_generics(generics: &Option<Vec<GenericParam>>, out: &mut Spelled) {
    for g in generics.iter().flatten() {
        spell_protocol_refs(g.bound_refs.as_deref(), out);
        if let Some(sb) = &g.structural_bound {
            spell_type(sb, out);
        }
    }
}

fn spell_protocol_refs(refs: Option<&[ProtocolRef]>, out: &mut Spelled) {
    for r in refs.into_iter().flatten() {
        if let Some(m) = &r.module {
            out.idents.insert(m.to_string());
        }
        for a in &r.args {
            spell_type(a, out);
        }
    }
}

fn spell_test_wheres(wheres: &[TestWhere], out: &mut Spelled) {
    for w in wheres {
        match w {
            TestWhere::Bind { name, value } => {
                out.binders.insert(name.to_string());
                spell_expr(value, out)
            }
            TestWhere::Override { path, value } => {
                for s in path {
                    out.idents.insert(s.to_string());
                }
                spell_expr(value, out);
            }
            TestWhere::CallResponse { target, params, response } => {
                for s in target {
                    out.idents.insert(s.to_string());
                }
                for p in params {
                    spell_pattern(p, out);
                }
                spell_expr(response, out);
            }
            TestWhere::Case { bindings, .. } => spell_test_wheres(bindings, out),
        }
    }
}

/// A type name, split at its qualifier: `M.T` spells `M` and `T`.
fn spell_type_name(name: &str, out: &mut Spelled) {
    for seg in name.split('.') {
        out.idents.insert(seg.to_string());
    }
}

fn spell_type(t: &TypeExpr, out: &mut Spelled) {
    match t {
        TypeExpr::Simple { name } => spell_type_name(name.as_str(), out),
        TypeExpr::Generic { name, args } => {
            spell_type_name(name.as_str(), out);
            for a in args {
                spell_type(a, out);
            }
        }
        TypeExpr::Record { fields } | TypeExpr::OpenRecord { fields } => {
            for f in fields {
                spell_type(&f.ty, out);
                if let Some(d) = &f.default {
                    spell_expr(d, out);
                }
            }
        }
        TypeExpr::Fn { params, ret, .. } => {
            for p in params {
                spell_type(p, out);
            }
            spell_type(ret, out);
        }
        TypeExpr::Tuple { elements } | TypeExpr::Union { members: elements } => {
            for e in elements {
                spell_type(e, out);
            }
        }
        TypeExpr::Variant { cases, .. } => {
            for c in cases {
                match c {
                    VariantCase::Unit { .. } => {}
                    VariantCase::Tuple { fields, .. } => {
                        for f in fields {
                            spell_type(f, out);
                        }
                    }
                    VariantCase::Record { fields, .. } => {
                        for f in fields {
                            spell_type(&f.ty, out);
                            if let Some(d) = &f.default {
                                spell_expr(d, out);
                            }
                        }
                    }
                }
            }
        }
        TypeExpr::ConstLit { .. } => {}
    }
}

fn spell_pattern(p: &Pattern, out: &mut Spelled) {
    match p {
        Pattern::Constructor { name, args } => {
            spell_type_name(name.as_str(), out);
            for a in args {
                spell_pattern(a, out);
            }
        }
        Pattern::RecordPattern { name, fields, .. } => {
            spell_type_name(name.as_str(), out);
            for f in fields {
                if let Some(p) = &f.pattern {
                    spell_pattern(p, out);
                }
            }
        }
        Pattern::Literal { value } => spell_expr(value, out),
        Pattern::Tuple { elements } => {
            for e in elements {
                spell_pattern(e, out);
            }
        }
        Pattern::List { elements, rest } => {
            if let Some(Some(r)) = rest {
                out.binders.insert(r.to_string());
            }
            for e in elements {
                spell_pattern(e, out);
            }
        }
        Pattern::As { name, inner } => {
            out.binders.insert(name.to_string());
            spell_pattern(inner, out)
        }
        Pattern::Some { inner } | Pattern::Ok { inner } | Pattern::Err { inner } => spell_pattern(inner, out),
        Pattern::Or { alts } => {
            for a in alts {
                spell_pattern(a, out);
            }
        }
        Pattern::Ident { name } => {
            out.binders.insert(name.to_string());
        }
        Pattern::Wildcard | Pattern::None => {}
    }
}

/// Every expression through the shared read-only visitor; the per-node
/// callback adds the names the visitor does not descend into (types,
/// patterns, statement annotations).
fn spell_expr(e: &Expr, out: &mut Spelled) {
    almide_lang::ast::visit_expr(e, &mut |x: &Expr| spell_node(x, out));
}

fn spell_node(x: &Expr, out: &mut Spelled) {
    match &x.kind {
        ExprKind::Ident { name } | ExprKind::TypeName { name } => spell_type_name(name.as_str(), out),
        ExprKind::Member { object, field } => match &object.kind {
            ExprKind::Ident { name } => {
                out.qualified.insert((name.to_string(), field.to_string()));
            }
            _ => {
                out.members.insert(field.to_string());
            }
        },
        ExprKind::OptionalChain { field, .. } => {
            out.members.insert(field.to_string());
        }
        ExprKind::Record { name: Some(n), .. } => spell_type_name(n.as_str(), out),
        ExprKind::Lambda { params, .. } => {
            for p in params {
                out.binders.insert(p.name.to_string());
                for n in p.tuple_names.iter().flatten() {
                    out.binders.insert(n.to_string());
                }
                if let Some(t) = &p.ty {
                    spell_type(t, out);
                }
            }
        }
        ExprKind::ForIn { var, var_tuple, body, .. } => {
            out.binders.insert(var.to_string());
            for n in var_tuple.iter().flatten() {
                out.binders.insert(n.to_string());
            }
            spell_stmt_types(body, out);
        }
        ExprKind::IfLet { name, .. } => {
            out.binders.insert(name.to_string());
        }
        ExprKind::TypeAscription { ty, .. } => spell_type(ty, out),
        ExprKind::Call { type_args: Some(ts), .. } => {
            for t in ts {
                spell_type(t, out);
            }
        }
        ExprKind::Match { arms, .. } => {
            for a in arms {
                spell_pattern(&a.pattern, out);
            }
        }
        ExprKind::Block { stmts, .. } => spell_stmt_types(stmts, out),
        ExprKind::While { body, .. } => spell_stmt_types(body, out),
        _ => {}
    }
}

/// The names a statement keeps outside its expressions: `let x: T` (its
/// type and its binder), a destructuring pattern, a `guard let` binder.
fn spell_stmt_types(stmts: &[Stmt], out: &mut Spelled) {
    for s in stmts {
        match s {
            Stmt::Let { name, ty, .. } | Stmt::Var { name, ty, .. } => {
                out.binders.insert(name.to_string());
                if let Some(t) = ty {
                    spell_type(t, out);
                }
            }
            Stmt::LetDestructure { pattern, .. } => spell_pattern(pattern, out),
            Stmt::GuardLet { name, .. } => {
                out.binders.insert(name.to_string());
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::reached_auto_bundled;

    fn reached(src: &str) -> Vec<&'static str> {
        let tokens = almide_lang::lexer::Lexer::tokenize(src);
        let prog = almide_lang::parser::Parser::new(tokens).parse().expect("parse");
        reached_auto_bundled(std::iter::once(&prog))
    }

    #[test]
    fn a_qualified_call_reaches_only_its_module() {
        // `int.to_string` is a call into `int`, not a UFCS `to_string` on
        // every module that declares one.
        let r = reached("fn main() -> Unit = println(int.to_string(1))\n");
        assert!(!r.contains(&"int8") && !r.contains(&"float64"), "{r:?}");
    }

    #[test]
    fn a_binding_named_like_a_module_is_a_ufcs_receiver() {
        // `list` is a String here: `list.trim()` is `string.trim`.
        let r = reached("fn f(list: String) -> String = list.trim()\n");
        assert!(r.contains(&"string"), "{r:?}");
    }

    #[test]
    fn hello_world_reaches_no_bundled_module() {
        assert_eq!(reached("fn main() -> Unit = println(\"x\")\n"), Vec::<&str>::new());
    }

    #[test]
    fn a_qualified_call_reaches_its_module() {
        let r = reached("fn main() -> Unit = println(int.to_string(1))\n");
        assert!(r.contains(&"int"), "{r:?}");
    }

    #[test]
    fn a_ufcs_method_reaches_every_module_declaring_it() {
        // `tail` has an Almide body in list.almd; the receiver type is not
        // known before checking, so the member name alone must reach list.
        let r = reached("fn f(xs: List[Int]) -> List[Int] = xs.tail()\n");
        assert!(r.contains(&"list"), "{r:?}");
    }

    #[test]
    fn a_reached_module_reaches_what_its_body_names() {
        // int8's bodies call `int.*`: reaching int8 reaches int.
        let r = reached("fn f(x: Int8) -> Int16 = int8.to_int16(x)\n");
        assert!(r.contains(&"int8") && r.contains(&"int"), "{r:?}");
    }
}
