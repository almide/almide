/// Inference types, type variables, and constraints for the constraint-based checker.

use std::collections::HashMap;
use std::collections::HashSet;
use crate::types::Ty;

/// A fresh type variable for constraint-based inference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TyVarId(pub u32);

/// A type that can contain inference variables.
#[derive(Debug, Clone)]
pub enum InferTy {
    Concrete(Ty),
    Var(TyVarId),
    List(Box<InferTy>),
    Option(Box<InferTy>),
    Result(Box<InferTy>, Box<InferTy>),
    Map(Box<InferTy>, Box<InferTy>),
    Tuple(Vec<InferTy>),
    Fn { params: Vec<InferTy>, ret: Box<InferTy> },
}

impl InferTy {
    pub fn from_ty(ty: &Ty) -> Self {
        match ty {
            Ty::List(inner) => InferTy::List(Box::new(InferTy::from_ty(inner))),
            Ty::Option(inner) => InferTy::Option(Box::new(InferTy::from_ty(inner))),
            Ty::Result(ok, err) => InferTy::Result(Box::new(InferTy::from_ty(ok)), Box::new(InferTy::from_ty(err))),
            Ty::Map(k, v) => InferTy::Map(Box::new(InferTy::from_ty(k)), Box::new(InferTy::from_ty(v))),
            Ty::Tuple(elems) => InferTy::Tuple(elems.iter().map(InferTy::from_ty).collect()),
            Ty::Fn { params, ret } => InferTy::Fn {
                params: params.iter().map(InferTy::from_ty).collect(),
                ret: Box::new(InferTy::from_ty(ret)),
            },
            Ty::TypeVar(name) if name.starts_with('?') => {
                InferTy::Var(TyVarId(name[1..].parse::<u32>().unwrap_or(0)))
            }
            other => InferTy::Concrete(other.clone()),
        }
    }

    pub fn to_ty(&self, solutions: &HashMap<TyVarId, InferTy>) -> Ty {
        match self {
            InferTy::Concrete(ty) => ty.clone(),
            InferTy::Var(id) => {
                if let Some(solved) = solutions.get(id) { solved.to_ty(solutions) }
                else { Ty::TypeVar(format!("?{}", id.0)) }
            }
            InferTy::List(inner) => Ty::List(Box::new(inner.to_ty(solutions))),
            InferTy::Option(inner) => Ty::Option(Box::new(inner.to_ty(solutions))),
            InferTy::Result(ok, err) => Ty::Result(Box::new(ok.to_ty(solutions)), Box::new(err.to_ty(solutions))),
            InferTy::Map(k, v) => Ty::Map(Box::new(k.to_ty(solutions)), Box::new(v.to_ty(solutions))),
            InferTy::Tuple(elems) => Ty::Tuple(elems.iter().map(|e| e.to_ty(solutions)).collect()),
            InferTy::Fn { params, ret } => Ty::Fn {
                params: params.iter().map(|p| p.to_ty(solutions)).collect(),
                ret: Box::new(ret.to_ty(solutions)),
            },
        }
    }

    /// Post-solve pass: resolve any remaining Ty::TypeVar("?N") in a fully-resolved Ty.
    /// Called AFTER to_ty() and constraint solving, so solutions are final.
    /// Uses a `seen` set to break cycles (e.g. ?0 → ?1 → Concrete(TypeVar("?0"))).
    pub fn resolve_inference_vars(ty: &Ty, solutions: &HashMap<TyVarId, InferTy>) -> Ty {
        Self::resolve_inner(ty, solutions, &mut HashSet::new())
    }

    fn resolve_inner(ty: &Ty, solutions: &HashMap<TyVarId, InferTy>, seen: &mut HashSet<u32>) -> Ty {
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
                            Some(InferTy::Var(next)) => current = *next,
                            Some(other) => break Some(other.clone()),
                            None => break None,
                        }
                    };
                    let result = if let Some(solved) = terminal {
                        let concrete = solved.to_ty(solutions);
                        Self::resolve_inner(&concrete, solutions, seen)
                    } else {
                        ty.clone()
                    };
                    seen.remove(&id);
                    return result;
                }
                ty.clone()
            }
            Ty::List(inner) => Ty::List(Box::new(Self::resolve_inner(inner, solutions, seen))),
            Ty::Option(inner) => Ty::Option(Box::new(Self::resolve_inner(inner, solutions, seen))),
            Ty::Result(ok, err) => Ty::Result(
                Box::new(Self::resolve_inner(ok, solutions, seen)),
                Box::new(Self::resolve_inner(err, solutions, seen)),
            ),
            Ty::Map(k, v) => Ty::Map(
                Box::new(Self::resolve_inner(k, solutions, seen)),
                Box::new(Self::resolve_inner(v, solutions, seen)),
            ),
            Ty::Tuple(elems) => Ty::Tuple(elems.iter().map(|e| Self::resolve_inner(e, solutions, seen)).collect()),
            Ty::Fn { params, ret } => Ty::Fn {
                params: params.iter().map(|p| Self::resolve_inner(p, solutions, seen)).collect(),
                ret: Box::new(Self::resolve_inner(ret, solutions, seen)),
            },
            Ty::Named(name, args) if !args.is_empty() => {
                Ty::Named(name.clone(), args.iter().map(|a| Self::resolve_inner(a, solutions, seen)).collect())
            }
            _ => ty.clone(),
        }
    }
}

#[derive(Debug)]
pub struct Constraint {
    pub expected: InferTy,
    pub actual: InferTy,
    pub context: String,
}
