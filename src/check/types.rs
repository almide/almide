/// Inference types, type variables, and constraints for the constraint-based checker.

use std::collections::HashMap;
use std::collections::HashSet;
use crate::types::Ty;

/// A fresh type variable for constraint-based inference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TyVarId(pub u32);

#[derive(Debug)]
pub struct Constraint {
    pub expected: Ty,
    pub actual: Ty,
    pub context: String,
}

/// Check if a Ty is an inference variable (?N).
pub fn is_inference_var(ty: &Ty) -> Option<TyVarId> {
    if let Ty::TypeVar(name) = ty {
        if name.starts_with('?') {
            if let Ok(id) = name[1..].parse::<u32>() {
                return Some(TyVarId(id));
            }
        }
    }
    None
}

/// Resolve inference variables (?N) in a Ty using the solutions map.
/// Uses a `seen` set to break cycles (e.g. ?0 -> ?1 -> TypeVar("?0")).
pub fn resolve_vars(ty: &Ty, solutions: &HashMap<TyVarId, Ty>) -> Ty {
    resolve_inner(ty, solutions, &mut HashSet::new())
}

fn resolve_inner(ty: &Ty, solutions: &HashMap<TyVarId, Ty>, seen: &mut HashSet<u32>) -> Ty {
    match ty {
        Ty::TypeVar(name) if name.starts_with('?') => {
            if let Ok(id) = name[1..].parse::<u32>() {
                if !seen.insert(id) { return ty.clone(); }
                // Follow Var chain
                let mut current = TyVarId(id);
                let mut chain = HashSet::new();
                let terminal = loop {
                    if !chain.insert(current.0) { break None; }
                    match solutions.get(&current) {
                        Some(Ty::TypeVar(n)) if n.starts_with('?') => {
                            if let Ok(next_id) = n[1..].parse::<u32>() {
                                current = TyVarId(next_id);
                            } else {
                                break Some(Ty::TypeVar(n.clone()));
                            }
                        }
                        Some(other) => break Some(other.clone()),
                        None => break None,
                    }
                };
                let result = if let Some(solved) = terminal {
                    resolve_inner(&solved, solutions, seen)
                } else {
                    ty.clone()
                };
                seen.remove(&id);
                result
            } else {
                ty.clone()
            }
        }
        Ty::List(inner) => Ty::List(Box::new(resolve_inner(inner, solutions, seen))),
        Ty::Option(inner) => Ty::Option(Box::new(resolve_inner(inner, solutions, seen))),
        Ty::Result(ok, err) => Ty::Result(
            Box::new(resolve_inner(ok, solutions, seen)),
            Box::new(resolve_inner(err, solutions, seen)),
        ),
        Ty::Map(k, v) => Ty::Map(
            Box::new(resolve_inner(k, solutions, seen)),
            Box::new(resolve_inner(v, solutions, seen)),
        ),
        Ty::Tuple(elems) => Ty::Tuple(elems.iter().map(|e| resolve_inner(e, solutions, seen)).collect()),
        Ty::Fn { params, ret } => Ty::Fn {
            params: params.iter().map(|p| resolve_inner(p, solutions, seen)).collect(),
            ret: Box::new(resolve_inner(ret, solutions, seen)),
        },
        Ty::Named(name, args) if !args.is_empty() => {
            Ty::Named(name.clone(), args.iter().map(|a| resolve_inner(a, solutions, seen)).collect())
        }
        _ => ty.clone(),
    }
}
