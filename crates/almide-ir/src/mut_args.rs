//! Which arguments a call WRITES: the `mut` parameter positions of its callee,
//! read from the callee's DECLARATION.
//!
//! The question "does `f(x)` write `x`?" is asked by the shared optimizer
//! (branch_lift), by the shared lowering (`var` demotion), and by the wasm
//! emitter (copy-on-write at a call site, closure cells). For a stdlib callee
//! the legs used to answer it from what each happened to have in hand: the
//! native leg lowers the bundled modules into the IR and read their
//! `IrFunction::mutated_params`; the wasm leg does not lower a bridge-surface
//! module, links a registry implementation whose params carry no `mut`, and
//! kept a hand list of mutators. Three answers to one question disagreed
//! (#2931, #2948, #2949, #2951). This module is the single answer: the positions
//! a declaration marks `mut` (or names in `@mutating(p)`), whichever leg asks
//! and whatever is lowered.

use almide_lang::ast::{AttrValue, Attribute, Decl, Param};
use almide_base::intern::Sym;
use std::collections::HashMap;
use crate::{CallTarget, IrExpr, IrExprKind, VarId};

/// The `mut` positions a declaration gives its params: every `mut p`, plus
/// every param a `@mutating(p)` attribute names (the older spelling), and the
/// FIRST param for a bare `@mutating` (the receiver — the reading native borrow
/// inference has always given it, and the one the runtime's `&mut` agrees
/// with: `@mutating fn as_mut_ptr(b: Bytes)` is `almide_rt_bytes_as_mut_ptr(b:
/// &mut ..)`). The IR lowering fills `IrFunction::mutated_params` from this,
/// so a lowered fn and an unlowered declaration answer the same.
pub fn declared_mut_positions(params: &[Param], attrs: &[Attribute]) -> Vec<usize> {
    let mut out: Vec<usize> = params.iter().enumerate().filter(|(_, p)| p.is_mut).map(|(i, _)| i).collect();
    for attr in attrs.iter().filter(|a| a.name.as_str() == "mutating") {
        if attr.args.is_empty() && !params.is_empty() && !out.contains(&0) {
            out.push(0);
        }
        for arg in &attr.args {
            if let AttrValue::Ident { name } = &arg.value {
                if let Some(idx) = params.iter().position(|p| p.name == *name) {
                    if !out.contains(&idx) {
                        out.push(idx);
                    }
                }
            }
        }
    }
    out
}

/// The `mut` positions of the bundled stdlib fn `module.func`, from its own
/// declaration in `stdlib/<module>.almd`, or `None` when `module` is not a
/// bundled module, has no such fn, or the fn writes none of its params.
///
/// Read from the declaration on EVERY leg: the answer must not depend on
/// whether this leg lowered the module into the IR or which implementation the
/// self-host registry links for it.
pub fn stdlib_mut_positions(module: &str, func: &str) -> Option<Vec<usize>> {
    if !almide_lang::stdlib_info::is_bundled_module(module) {
        return None;
    }
    let program = almide_lang::parse_cached(almide_lang::stdlib_info::bundled_source(module)?)?;
    program.decls.iter().find_map(|d| match d {
        Decl::Fn { name, params, attrs, .. } if name.as_str() == func => {
            let idxs = declared_mut_positions(params, attrs);
            (!idxs.is_empty()).then_some(idxs)
        }
        _ => None,
    })
}

/// Every bundled stdlib fn that writes a param, as `(module, func, positions)`,
/// enumerated from the declarations. The gates iterate this rather than a
/// hand list, so a new `mut` fn is covered the day it is declared.
pub fn stdlib_mut_fns() -> Vec<(&'static str, String, Vec<usize>)> {
    let mut out = Vec::new();
    for &module in almide_lang::stdlib_info::BUNDLED_MODULES {
        let Some(src) = almide_lang::stdlib_info::bundled_source(module) else { continue };
        let Some(program) = almide_lang::parse_cached(src) else { continue };
        for d in &program.decls {
            if let Decl::Fn { name, params, attrs, .. } = d {
                let idxs = declared_mut_positions(params, attrs);
                if !idxs.is_empty() {
                    out.push((module, name.as_str().to_string(), idxs));
                }
            }
        }
    }
    out
}

/// The positions a call may write through `mut` parameters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallWrites {
    /// The callee is identified; these are its `mut` positions (maybe none).
    Positions(Vec<usize>),
    /// The callee is not identified (an unresolved method, a call through a
    /// fn value, a bare name no table holds or two modules share): any
    /// argument that names a place may be written.
    Unknown,
}

/// The `mut` positions of every fn the program defines, so a call of any
/// shape can be asked what it writes. A user fn answers from its lowered
/// `mutated_params`; a bundled stdlib fn from its declaration
/// ([`stdlib_mut_positions`]), on every leg. A bare call inside a module
/// (`bump(s)` next to `fn bump(mut s)`) resolves in that module first: the
/// table that keyed only module-qualified calls let LICM hoist a read past a
/// same-module write (#3452).
#[derive(Debug, Default)]
pub struct MutParamTable {
    /// Root-program fns by bare name (every fn, mut positions or none).
    root: HashMap<Sym, Vec<usize>>,
    /// Module fns by `(module, fn)`.
    modules: HashMap<(Sym, Sym), Vec<usize>>,
    /// User-module fns by bare name, for a root call that names an imported
    /// fn bare; `None` when two modules disagree.
    bare: HashMap<Sym, Option<Vec<usize>>>,
}

impl MutParamTable {
    pub fn of(program: &crate::IrProgram) -> Self {
        let mut table = MutParamTable::default();
        for f in &program.functions {
            table.root.insert(f.name, f.mutated_params.clone());
        }
        for m in &program.modules {
            let bundled = almide_lang::stdlib_info::is_bundled_module(m.name.as_str());
            for f in &m.functions {
                let idxs = if bundled {
                    stdlib_mut_positions(m.name.as_str(), f.name.as_str()).unwrap_or_default()
                } else {
                    f.mutated_params.clone()
                };
                if !bundled {
                    table.bare.entry(f.name)
                        .and_modify(|e| if e.as_ref() != Some(&idxs) { *e = None })
                        .or_insert_with(|| Some(idxs.clone()));
                }
                table.modules.insert((m.name, f.name), idxs);
            }
        }
        table
    }

    /// What a call to `target` writes, made from inside module `scope`
    /// (`None` for the root program).
    pub fn writes(&self, target: &CallTarget, scope: Option<Sym>) -> CallWrites {
        match target {
            CallTarget::Module { module, func, .. } => {
                if almide_lang::stdlib_info::is_bundled_module(module.as_str()) {
                    return CallWrites::Positions(
                        stdlib_mut_positions(module.as_str(), func.as_str()).unwrap_or_default(),
                    );
                }
                self.modules.get(&(*module, *func)).cloned().map_or(CallWrites::Unknown, CallWrites::Positions)
            }
            CallTarget::Named { name } => {
                let own = match scope {
                    Some(m) => self.modules.get(&(m, *name)),
                    None => self.root.get(name),
                };
                match (own, self.bare.get(name)) {
                    (Some(idxs), _) | (None, Some(Some(idxs))) => CallWrites::Positions(idxs.clone()),
                    _ => CallWrites::Unknown,
                }
            }
            CallTarget::Method { .. } | CallTarget::Computed { .. } => CallWrites::Unknown,
        }
    }
}

/// The variable a place expression is rooted at: `s`, `w.inner`, `w.a.b`,
/// `t.0`, `xs[i]` all name their root var. `None` for a temporary.
pub fn place_root(expr: &IrExpr) -> Option<VarId> {
    match &expr.kind {
        IrExprKind::Var { id } => Some(*id),
        IrExprKind::Member { object, .. }
        | IrExprKind::TupleIndex { object, .. }
        | IrExprKind::IndexAccess { object, .. } => place_root(object),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bundled_mutator_answers_from_its_declaration() {
        assert_eq!(stdlib_mut_positions("list", "push"), Some(vec![0]));
        assert_eq!(stdlib_mut_positions("list", "len"), None);
        assert_eq!(stdlib_mut_positions("bytes", "set_uint16"), Some(vec![0]));
        // A bare `@mutating` writes its receiver.
        assert_eq!(stdlib_mut_positions("bytes", "as_mut_ptr"), Some(vec![0]));
        assert_eq!(stdlib_mut_positions("no_such_module", "push"), None);
    }

    #[test]
    fn the_enumeration_covers_every_module_the_lookup_answers_for() {
        let all = stdlib_mut_fns();
        assert!(all.len() > 20, "the stdlib declares many mut fns, found {}", all.len());
        for (m, f, idxs) in &all {
            assert_eq!(stdlib_mut_positions(m, f).as_ref(), Some(idxs), "{m}.{f}");
        }
    }
}
