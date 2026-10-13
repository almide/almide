//! The deterministic meter's plan (ALS-DT2), read from the IR by every leg
//! that meters a region: the wasm emitter and the standard native codegen
//! (#3673). One definition, so the two legs charge the same fns.
//!
//! The ENTRY-EXEMPT analysis mirrors the interp's `det_entry_exempt` cell for
//! cell: a USER fn whose body holds no While/ForIn anywhere and that cannot
//! reach itself through user-fn call edges is the class the shared-MIR
//! inliner folds away, entry charge included. Names are SIMPLE names on both
//! sides — the mirror is deliberately bug-compatible with the oracle's keying.

use std::collections::{HashMap, HashSet};

use crate::visit::{walk_expr, IrVisitor};
use crate::{CallTarget, IrExpr, IrExprKind, IrProgram};

/// The stdlib modules whose higher-order fns take a callback the metered
/// legs run INLINE: a lambda literal written as such a call's argument is
/// the loop body of the call, not a closure, so calling it is no hop and
/// charges nothing. A fn VALUE handed to the same call is a closure, and
/// each call through it is a hop. The wasm leg's eta expansion
/// (`almide-wasm/src/eta.rs`) and the native codegen's meter
/// (`pass_meter_charge.rs`) both read this one list.
pub const INLINED_CALLBACK_MODULES: &[&str] = &["list", "map", "set", "option", "result", "fan", "fs", "matrix"];

pub struct MeterPlan {
    /// SIMPLE names of user fns (entry program + user modules).
    pub user: HashSet<String>,
    /// SIMPLE names whose ENTRY charge is exempt.
    pub exempt: HashSet<String>,
}

struct Scan {
    has_loop: bool,
    calls: HashSet<String>,
}

impl IrVisitor for Scan {
    fn visit_expr(&mut self, e: &IrExpr) {
        match &e.kind {
            IrExprKind::While { .. } | IrExprKind::ForIn { .. } => self.has_loop = true,
            IrExprKind::Call { target, .. } => match target {
                CallTarget::Named { name } => {
                    self.calls.insert(name.as_str().to_string());
                }
                CallTarget::Module { func, .. } => {
                    self.calls.insert(func.as_str().to_string());
                }
                _ => {}
            },
            _ => {}
        }
        walk_expr(self, e);
    }
}

/// True iff the program brackets ANY region (budget/timeout prims in
/// the IR). Without one, charges are UNOBSERVABLE — verdict/spend are
/// only read through region desugars — so the whole meter elides,
/// reclaiming the per-iteration loop-head cost for ordinary programs.
pub fn program_has_regions(ir: &IrProgram) -> bool {
    struct Scan {
        found: bool,
    }
    impl IrVisitor for Scan {
        fn visit_expr(&mut self, e: &IrExpr) {
            if let IrExprKind::RuntimeCall { symbol, .. } = &e.kind
                && crate::runtime_use::METERED_PRIM_PREFIXES.iter().any(|p| symbol.as_str().starts_with(p))
            {
                self.found = true;
            }
            if !self.found {
                walk_expr(self, e);
            }
        }
    }
    let mut s = Scan { found: false };
    for f in &ir.functions {
        s.visit_expr(&f.body);
    }
    for m in &ir.modules {
        for f in &m.functions {
            s.visit_expr(&f.body);
        }
    }
    s.found
}

/// Who charges and whose entry is exempt. `registry_names` are the
/// self-host registry's implementation symbols: a pool body never charges.
pub fn meter_plan(ir: &IrProgram, registry_names: &HashSet<&str>) -> MeterPlan {
    if !program_has_regions(ir) {
        return MeterPlan { user: HashSet::new(), exempt: HashSet::new() };
    }
    let mut user: HashSet<String> = ir
        .functions
        .iter()
        .filter(|f| !f.is_test)
        .map(|f| f.name.as_str().to_string())
        .collect();
    let mut bodies: Vec<(&str, &IrExpr)> = ir
        .functions
        .iter()
        .filter(|f| !f.is_test && f.name.as_str() != "main")
        .map(|f| (f.name.as_str(), &f.body))
        .collect();
    for m in &ir.modules {
        for f in &m.functions {
            if registry_names.contains(f.name.as_str()) {
                continue; // pool bodies never charge
            }
            user.insert(f.name.as_str().to_string());
            bodies.push((f.name.as_str(), &f.body));
        }
    }
    let mut loopy: HashSet<String> = HashSet::new();
    let mut edges: HashMap<String, HashSet<String>> = HashMap::new();
    for (name, body) in &bodies {
        let mut s = Scan { has_loop: false, calls: HashSet::new() };
        s.visit_expr(body);
        if s.has_loop {
            loopy.insert((*name).to_string());
        }
        s.calls.retain(|c| user.contains(c));
        edges.entry((*name).to_string()).or_default().extend(s.calls);
    }
    let exempt = bodies
        .iter()
        .filter(|(name, _)| !loopy.contains(*name) && !reaches_self(&edges, name))
        .map(|(name, _)| (*name).to_string())
        .collect();
    MeterPlan { user, exempt }
}

/// Can `start` reach itself through the user call edges?
fn reaches_self(edges: &HashMap<String, HashSet<String>>, start: &str) -> bool {
    let mut seen: HashSet<&str> = HashSet::new();
    let mut stack: Vec<&str> = edges.get(start).into_iter().flatten().map(String::as_str).collect();
    while let Some(n) = stack.pop() {
        if n == start {
            return true;
        }
        if seen.insert(n) {
            stack.extend(edges.get(n).into_iter().flatten().map(String::as_str));
        }
    }
    false
}
