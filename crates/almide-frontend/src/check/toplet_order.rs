//! #3164: the order in which the top-level `let`s are type-checked.
//!
//! A top-level `let` may name a LATER one — `let PAIRS = [("a", TEXT)]` above
//! `let TEXT = "hello"`. Registration seeds each entry in source order, so
//! `PAIRS`'s seed is `List[(String, Unknown)]`, and checking the initializers
//! in source order re-read that `Unknown` for `TEXT` before `TEXT` had been
//! checked. The result was never upgraded (an `Unknown` read is not an
//! inference variable the solver could bind later), a function that iterated
//! `PAIRS` typed its loop variable `(String, Unknown)`, and the build panicked
//! in `ConcretizeTypes` behind a green `almide check`.
//!
//! Top-level declarations are order-independent (a function may call one
//! declared below it), so the initializers are checked dependencies first: a
//! post-order walk over the `let`-to-`let` references. Members of a reference
//! cycle keep their source order — the cycle itself is the error the real
//! pass reports.

use crate::types::{Ty, TypeEnv};
use almide_base::intern::{Sym, sym};
use almide_lang::ast;
use std::collections::HashMap;

/// Indices into `decls` of every `TopLet`, each one after the top-level
/// `let`s its initializer names (source order otherwise).
pub(super) fn toplet_check_order(decls: &[ast::Decl]) -> Vec<usize> {
    let lets: Vec<(usize, Sym)> = decls
        .iter()
        .enumerate()
        .filter_map(|(i, d)| match d {
            ast::Decl::TopLet { name, .. } => Some((i, *name)),
            _ => None,
        })
        .collect();
    let slot: HashMap<Sym, usize> = lets.iter().enumerate().map(|(k, (_, n))| (*n, k)).collect();
    let deps: Vec<Vec<usize>> = lets
        .iter()
        .map(|(i, _)| match &decls[*i] {
            ast::Decl::TopLet { value, .. } => referenced_lets(value, &slot),
            _ => Vec::new(),
        })
        .collect();
    // 0 = unvisited, 1 = on the current path, 2 = emitted.
    let mut state = vec![0u8; lets.len()];
    let mut order = Vec::with_capacity(lets.len());
    for root in 0..lets.len() {
        visit(root, &deps, &mut state, &mut order);
    }
    order.into_iter().map(|k| lets[k].0).collect()
}

/// `(bare name, type)` for every top-level `let` of `decls` whose entry —
/// under `prefix.name` for a module, the bare name otherwise — is fully
/// concrete.
pub(super) fn concrete_top_lets(env: &TypeEnv, decls: &[ast::Decl], prefix: Option<&str>) -> Vec<(Sym, Ty)> {
    decls
        .iter()
        .filter_map(|d| match d {
            ast::Decl::TopLet { name, .. } => Some(*name),
            _ => None,
        })
        .filter_map(|name| {
            let key = match prefix {
                Some(p) => sym(&format!("{}.{}", p, name)),
                None => name,
            };
            let t = env.top_lets.get(&key)?;
            (!t.contains_unknown() && !t.contains_typevar()).then(|| (name, t.clone()))
        })
        .collect()
}

/// The entry program's pre-pass runs under the pseudo-module `__entry`, so the
/// user types in what it inferred are spelled `__entry.Op`; the entry's real
/// pass names them `Op`. Strip the pseudo-qualifier before the types are read
/// as the entry's own (a `List[__entry.Op]` is a type nothing else spells —
/// it routed a closure-holding `let` to the wrong storage, #3164).
pub(super) fn unqualify_entry(refreshed: Vec<(Sym, Ty)>) -> Vec<(Sym, Ty)> {
    refreshed.into_iter().map(|(n, t)| (n, strip_qualifier(&t, "__entry."))).collect()
}

fn strip_qualifier(t: &Ty, pfx: &str) -> Ty {
    use crate::types::TypeConstructorId as C;
    let bare = |s: &str| s.strip_prefix(pfx).map(str::to_string);
    let own = match t {
        Ty::Named(n, args) => match bare(n.as_str()) {
            Some(b) => Ty::Named(sym(&b), args.clone()),
            None => t.clone(),
        },
        Ty::Applied(C::UserDefined(s), args) => match bare(s) {
            Some(b) => Ty::Applied(C::UserDefined(b), args.clone()),
            None => t.clone(),
        },
        Ty::Variant { name, cases } => match bare(name.as_str()) {
            Some(b) => Ty::Variant { name: sym(&b), cases: cases.clone() },
            None => t.clone(),
        },
        _ => t.clone(),
    };
    own.map_children(&|c| strip_qualifier(c, pfx))
}

/// Upgrade each bare `top_lets` entry that is still a partial seed to the
/// concrete type `concrete_top_lets` found. A concrete entry (an annotation)
/// is never replaced.
pub(super) fn adopt_top_lets(env: &mut TypeEnv, refreshed: Vec<(Sym, Ty)>) {
    for (name, t) in refreshed {
        let partial = env
            .top_lets
            .get(&name)
            .is_none_or(|e| e.contains_unknown() || e.contains_typevar());
        if partial {
            env.top_lets.insert(name, t);
        }
    }
}

fn referenced_lets(value: &ast::Expr, slot: &HashMap<Sym, usize>) -> Vec<usize> {
    let mut out = Vec::new();
    ast::visit_expr(value, &mut |e| {
        // A capitalised name (`TEXT`) parses as a TypeName, not an Ident.
        let name = match &e.kind {
            ast::ExprKind::Ident { name, .. } | ast::ExprKind::TypeName { name } => name,
            _ => return,
        };
        if let Some(&k) = slot.get(name)
            && !out.contains(&k)
        {
            out.push(k);
        }
    });
    out
}

fn visit(k: usize, deps: &[Vec<usize>], state: &mut [u8], order: &mut Vec<usize>) {
    if state[k] != 0 {
        return;
    }
    state[k] = 1;
    for &d in &deps[k] {
        visit(d, deps, state, order);
    }
    state[k] = 2;
    order.push(k);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn order_of(src: &str) -> Vec<String> {
        let tokens = almide_lang::lexer::Lexer::tokenize(src);
        let prog = almide_lang::parser::Parser::new(tokens).parse().expect("parse");
        toplet_check_order(&prog.decls)
            .into_iter()
            .map(|i| match &prog.decls[i] {
                ast::Decl::TopLet { name, .. } => name.to_string(),
                _ => unreachable!(),
            })
            .collect()
    }

    #[test]
    fn a_let_is_checked_after_the_lets_it_names() {
        let o = order_of("let A = [B, C]\nlet B = C + 1\nlet C = 1\n");
        assert_eq!(o, vec!["C", "B", "A"]);
    }

    #[test]
    fn independent_lets_keep_source_order() {
        let o = order_of("let X = 1\nlet Y = 2\nlet Z = 3\n");
        assert_eq!(o, vec!["X", "Y", "Z"]);
    }

    #[test]
    fn a_cycle_keeps_every_member_once() {
        let o = order_of("let P = Q\nlet Q = P\nlet R = 1\n");
        assert_eq!(o.len(), 3);
        assert_eq!(o[2], "R");
    }
}
