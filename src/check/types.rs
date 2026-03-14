/// Inference types, type variables, and constraints for the constraint-based checker.

use std::collections::HashMap;
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
}

#[derive(Debug)]
pub struct Constraint {
    pub expected: InferTy,
    pub actual: InferTy,
    pub context: String,
}
