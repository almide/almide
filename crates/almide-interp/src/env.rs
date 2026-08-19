//! Variable environment.
//!
//! Variables are keyed by `VarId` (the IR's shadow-free identifier), so the
//! environment is a flat `VarId -> Value` map per scope. Scopes are reference
//! counted and shared so a closure can cheaply snapshot the environment it
//! captures: cloning a `Scope` clones an `Rc`, and writes go through a
//! `RefCell`. This reproduces native `RcCow` capture semantics — a captured
//! variable observed *after* the closure was created reflects the value at
//! capture time (we snapshot the frame chain by Rc).

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use almide_ir::VarId;

use crate::value::Value;

/// A single lexical frame: `VarId -> Value`, chained to its parent.
#[derive(Clone)]
pub struct Scope {
    inner: Rc<ScopeInner>,
}

struct ScopeInner {
    vars: RefCell<HashMap<VarId, Value>>,
    parent: Option<Scope>,
}

impl Scope {
    /// A fresh root scope with no parent.
    pub fn root() -> Scope {
        Scope {
            inner: Rc::new(ScopeInner {
                vars: RefCell::new(HashMap::new()),
                parent: None,
            }),
        }
    }

    /// Push a new child frame on top of `self`. Lookups fall through to the
    /// parent chain; binds land in the new frame.
    pub fn child(&self) -> Scope {
        Scope {
            inner: Rc::new(ScopeInner {
                vars: RefCell::new(HashMap::new()),
                parent: Some(self.clone()),
            }),
        }
    }

    /// Bind (or rebind) a variable in *this* frame.
    pub fn bind(&self, id: VarId, value: Value) {
        self.inner.vars.borrow_mut().insert(id, value);
    }

    /// Look up a variable, walking the parent chain.
    pub fn get(&self, id: VarId) -> Option<Value> {
        if let Some(v) = self.inner.vars.borrow().get(&id) {
            return Some(v.clone());
        }
        match &self.inner.parent {
            Some(p) => p.get(id),
            None => None,
        }
    }

    /// Hand the owning frame's storage for `id` to `f` as a mutable slot.
    /// `None` = the variable was never bound.
    ///
    /// `get` + `assign` would do the same job, but `get` hands back a CLONE, so
    /// an in-place container mutator holding it can never be the sole owner of
    /// the inner `Rc` and has to copy the whole container on every call —
    /// turning a push loop quadratic. With the slot itself, `Rc::make_mut`
    /// copies only when an alias actually exists, which is both O(1) amortized
    /// and exactly the COW rule the backends implement (C-033).
    ///
    /// `f` must not touch this scope: the owning frame's `RefCell` is borrowed
    /// for the duration of the call.
    pub fn with_slot<R>(&self, id: VarId, f: impl FnOnce(&mut Value) -> R) -> Option<R> {
        {
            let mut vars = self.inner.vars.borrow_mut();
            if let Some(slot) = vars.get_mut(&id) {
                return Some(f(slot));
            }
        }
        self.inner.parent.as_ref()?.with_slot(id, f)
    }

    /// Assign to an existing variable, walking the parent chain to find the
    /// frame that owns it. Returns `true` if the variable was found and
    /// updated, `false` if it was never bound (a should-not-happen on
    /// well-formed IR, which the evaluator turns into an ICE-style abort).
    pub fn assign(&self, id: VarId, value: Value) -> bool {
        if self.inner.vars.borrow().contains_key(&id) {
            self.inner.vars.borrow_mut().insert(id, value);
            return true;
        }
        match &self.inner.parent {
            Some(p) => p.assign(id, value),
            None => false,
        }
    }
}
