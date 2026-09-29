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

/// The `mut` positions a declaration gives its params: every `mut p`, plus
/// every param a `@mutating(p)` attribute names (the older spelling). The IR
/// lowering fills `IrFunction::mutated_params` from this, so a lowered fn and
/// an unlowered declaration answer the same.
pub fn declared_mut_positions(params: &[Param], attrs: &[Attribute]) -> Vec<usize> {
    let mut out: Vec<usize> = params.iter().enumerate().filter(|(_, p)| p.is_mut).map(|(i, _)| i).collect();
    for attr in attrs.iter().filter(|a| a.name.as_str() == "mutating") {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bundled_mutator_answers_from_its_declaration() {
        assert_eq!(stdlib_mut_positions("list", "push"), Some(vec![0]));
        assert_eq!(stdlib_mut_positions("list", "len"), None);
        assert_eq!(stdlib_mut_positions("bytes", "set_uint16"), Some(vec![0]));
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
