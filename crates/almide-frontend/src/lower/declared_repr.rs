//! The single renderer rule (#4107) at lowering: a value prints through its
//! type's DECLARED repr wherever it sits. The synthesizer is
//! `almide_ir::declared_repr` — one implementation, run here for every
//! segment whose type the lowering knows, and again after monomorphization
//! for a segment inside a generic body (#4239). This file supplies the facts
//! the lowering has (the TypeEnv's declared reprs and type shapes) and
//! records them on the program for that second run.
//!
//! What it reads is the declaration, never a name: a type is routed iff
//! `find_explicit_convention_fn(ty, "repr")` answers for it.

use super::LowerCtx;
use crate::types::{Ty, TypeEnv};
use almide_base::intern::{sym, Sym};
use almide_ir::declared_repr::{DeclaredReprTable, ReprFacts, ReprSynth};
use almide_ir::*;
use std::collections::{HashMap, HashSet};

/// The lowering's view of the type facts the synthesizer reads.
struct LowerFacts<'a> {
    env: &'a TypeEnv,
    explicit: &'a HashSet<Sym>,
    def_map: &'a HashMap<Sym, DefId>,
}

impl ReprFacts for LowerFacts<'_> {
    fn repr_target(&self, ty: &Ty) -> Option<CallTarget> {
        let key = explicit_convention_fn(self.env, self.explicit, ty, "repr")?;
        Some(convention_call_target(key))
    }

    fn unfold_named(&self, ty: &Ty) -> Option<Ty> {
        crate::secret_type::unfold_named(self.env, ty)
    }

    fn def_id(&self, key: &str) -> Option<DefId> {
        self.def_map.get(&sym(key)).copied()
    }
}

/// [`LowerCtx::find_explicit_convention_fn`] over its two inputs.
pub(super) fn explicit_convention_fn(env: &TypeEnv, explicit: &HashSet<Sym>, ty: &Ty, convention: &str) -> Option<Sym> {
    if let Ty::Named(type_name, _) = ty {
        let fn_name = sym(&format!("{}.{}", type_name, convention));
        if explicit.contains(&fn_name) {
            return Some(fn_name);
        }
        // The set above only holds the program being lowered, so a custom
        // `repr` defined in ANOTHER module was silently skipped and the
        // value fell through to the derived `AlmideRepr` impl — printing
        // the variant name with no error anywhere (#1087).
        if crate::canonicalize::registration::declared_convention_key(env, type_name.as_str(), convention).is_some() {
            return crate::canonicalize::registration::convention_emit_key(env, type_name.as_str(), convention);
        }
    }
    None
}

/// The call target of a convention method found by
/// [`explicit_convention_fn`]: a bundled module's own method
/// (`secret.Secret.repr`) is a call into that module; every other key is
/// a named fn, as before.
fn convention_call_target(key: Sym) -> CallTarget {
    let k = key.as_str();
    let mut segs = k.splitn(2, '.');
    if let (Some(m), Some(rest)) = (segs.next(), segs.next())
        && almide_lang::stdlib_info::is_bundled_module(m)
        && rest.contains('.')
    {
        return CallTarget::Module { module: sym(m), func: sym(rest), def_id: None };
    }
    CallTarget::Named { name: key }
}

impl LowerCtx<'_> {
    /// `expr` rendered as every renderer must render it (#4107): through its
    /// type's declared repr, or a synthesized walker when one is declared
    /// somewhere inside the type. A type with no declared repr anywhere
    /// comes back unchanged, and the leg's own structural renderer prints it.
    pub(super) fn render_declared(&mut self, expr: IrExpr) -> IrExpr {
        let facts = LowerFacts { env: self.env, explicit: &self.explicit_convention_fns, def_map: &self.def_map };
        let scope = self.current_module.map(|m| format!("{}_", m.as_str().replace('.', "_"))).unwrap_or_default();
        let mut synth = ReprSynth {
            facts: &facts,
            var_table: &mut self.var_table,
            out: &mut self.synthesized_fns,
            pending: &mut self.declared_repr_pending,
            scope,
        };
        synth.render_declared(expr)
    }

    /// The declared reprs and nominal shapes of every type the program can
    /// name, for the renderer rule's post-monomorphization run (#4239):
    /// after mono a generic body's `${x}` carries the instance's type, and
    /// the IR alone must then know which types declare their repr.
    pub(super) fn declared_repr_table(&self) -> DeclaredReprTable {
        let facts = LowerFacts { env: self.env, explicit: &self.explicit_convention_fns, def_map: &self.def_map };
        let mut table = DeclaredReprTable::default();
        for (name, shape) in &self.env.types {
            if name.as_str().starts_with("<type-params:") {
                continue;
            }
            let named = Ty::Named(*name, vec![]);
            if let Some(t) = facts.repr_target(&named) {
                table.targets.insert(*name, t);
            }
            let params = match self.env.types.get(&crate::canonicalize::resolve::type_params_key(name.as_str())) {
                Some(Ty::Tuple(ps)) => ps.iter().filter_map(|p| match p { Ty::TypeVar(g) => Some(*g), _ => None }).collect(),
                _ => Vec::new(),
            };
            table.shapes.insert(*name, (params, shape.clone()));
        }
        table
    }
}
