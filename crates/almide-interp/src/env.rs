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
