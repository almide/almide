//! The order a record takes when its type defines it through its protocol's
//! `compare` requirement (`BigInt: Integer`, #4788, C-456).
//!
//! Both backends order such a value by its `compare` in every natural-order
//! operation a generic body reaches (mono rewrites `list.sort` / `min` / `max`
//! / `sort_by` over it to the comparator forms). The interp runs the stdlib
//! bodies of `set.sort`, `list.min_by`, … from its own pool, unspecialized, and
//! answers the natural-order intrinsics with [`TotalOrder`] — so the record
//! case of that order asks the program's operator-requirement table first, and
//! a type that answers `compare` there orders by evaluating it, never by the
//! layout of its fields. Every other record keeps its field order.

use std::cmp::Ordering;

use almide_base::intern::{Sym, sym};
use almide_ir::CallTarget;

use crate::value::{TotalOrder, Value};
use crate::{Flow, Interpreter};

impl<'a> Interpreter<'a> {
    /// The `compare` requirement of the record type `name`, under its own or
    /// its bare name — the two spellings the table is keyed under.
    fn protocol_compare(&self, name: Sym) -> Option<CallTarget> {
        let table = &self.program.operator_requirements;
        let m = sym("compare");
        table.get(&(name, m)).cloned().or_else(|| {
            let bare = name.as_str().rsplit_once('.').map(|(_, b)| b)?;
            table.get(&(sym(bare), m)).cloned()
        })
    }

    /// `target(a, b)` over evaluated operands.
    fn call_compare(&mut self, target: &CallTarget, a: &Value, b: &Value) -> Flow {
        let args = vec![a.clone(), b.clone()];
        match target {
            CallTarget::Module { module, func, .. } => {
                let saved = self.container_arg_tys.take();
                let out = self.dispatch_module_resolved(*module, *func, args);
                self.container_arg_tys = saved;
                out
            }
            CallTarget::Named { name } => {
                let scope = self.root_scope();
                match self.fn_ref_value(*name, &scope) {
                    Flow::Value(Value::Closure(clo)) => self.apply_closure(&clo, args),
                    other => other,
                }
            }
            _ => Flow::Unsupported("protocol compare through a computed target".into()),
        }
    }

    /// Run `f` over the list-ordering total order, a record whose type orders
    /// through its protocol's `compare` ordered by that compare. A compare that
    /// does not complete (abort, fuel, an abstain) is the result.
    pub(crate) fn with_total_order<R>(&mut self, f: impl FnOnce(&TotalOrder<'_>) -> R) -> Result<R, Flow> {
        let tags = std::rc::Rc::clone(&self.variant_tags);
        if !self.program.operator_requirements.keys().any(|(_, m)| m.as_str() == "compare") {
            return Ok(f(&TotalOrder::new(&tags)));
        }
        let mut failed: Option<Flow> = None;
        let out = {
            let mut records = |a: &Value, b: &Value| -> Option<Option<Ordering>> {
                let (Value::Record { name: Some(x), .. }, Value::Record { name: Some(y), .. }) = (a, b) else {
                    return None;
                };
                let bare = |n: &Sym| n.as_str().rsplit_once('.').map_or(n.as_str(), |(_, b)| b).to_owned();
                if bare(x) != bare(y) {
                    return None;
                }
                let target = self.protocol_compare(*x)?;
                if failed.is_some() {
                    return Some(None);
                }
                match self.call_compare(&target, a, b) {
                    Flow::Value(Value::Int(c)) => Some(Some(c.cmp(&0))),
                    Flow::Value(other) => {
                        failed = Some(Flow::Abort(format!("internal: compare returned {}, not an Int", other.type_name())));
                        Some(None)
                    }
                    other => {
                        failed = Some(other);
                        Some(None)
                    }
                }
            };
            f(&TotalOrder::with_records(&tags, &mut records))
        };
        match failed {
            Some(flow) => Err(flow),
            None => Ok(out),
        }
    }
}
