//! On-demand derives: a derived fn the generator stamps `@on_demand` ships
//! only when something reaches it.
//!
//! The strict decode walk (C-429 / C-436) is derived for every `Decode`
//! type, so an enclosing type's walk can enter a nested one and a user can
//! call `T.decode_strict` directly. Most programs call neither, and every
//! user fn ships whole on every leg (only fixed-slot helpers are stubbed),
//! so an unreached walker would cost each Codec program its bytes. After
//! mono, when the generic `http.decode_json[T]` names its concrete
//! `T.decode_strict`, a stamped fn survives only if a call, a fn reference
//! or an unresolved method call can reach it from an unstamped fn or a
//! top-level binding. Matching is deliberately loose (a module-qualified
//! spelling either way, a bare method name against the last segment): it
//! can keep a fn that is never called, never drop one that is.
//!
//! The stamp is the declaration: the generator marks what it may leave
//! out, and this pass reads only the mark.

use std::collections::{HashMap, HashSet};

use almide_base::intern::Sym;
use almide_ir::visit::{walk_expr, walk_stmt, IrVisitor};
use almide_ir::*;

pub use almide_ir::ON_DEMAND_ATTR;

pub fn is_on_demand(f: &IrFunction) -> bool {
    f.attrs.iter().any(|a| a.name.as_str() == ON_DEMAND_ATTR)
}

/// Every name an expression tree may call or reference.
struct Refs {
    names: Vec<String>,
}

impl IrVisitor for Refs {
    fn visit_expr(&mut self, expr: &IrExpr) {
        walk_expr(self, expr);
        let target = match &expr.kind {
            IrExprKind::Call { target, .. } | IrExprKind::TailCall { target, .. } => Some(target),
            IrExprKind::FnRef { name } => {
                self.names.push(name.as_str().to_string());
                None
            }
            _ => None,
        };
        match target {
            Some(CallTarget::Named { name }) => self.names.push(name.as_str().to_string()),
            Some(CallTarget::Module { module, func, .. }) => {
                self.names.push(format!("{}.{}", module, func));
                self.names.push(func.as_str().to_string());
            }
            Some(CallTarget::Method { method, .. }) => self.names.push(method.as_str().to_string()),
            _ => {}
        }
    }
    fn visit_stmt(&mut self, stmt: &IrStmt) {
        walk_stmt(self, stmt);
    }
}

fn last_segment(s: &str) -> &str {
    s.rsplit('.').next().unwrap_or(s)
}

/// `r` may name the fn `name`: the same spelling, or one is the other with
/// a qualifying prefix.
fn may_name(name: &str, r: &str) -> bool {
    name == r
        || (name.len() > r.len() && name.ends_with(r) && name.as_bytes()[name.len() - r.len() - 1] == b'.')
        || (r.len() > name.len() && r.ends_with(name) && r.as_bytes()[r.len() - name.len() - 1] == b'.')
}

/// Drop every `@on_demand` fn nothing reaches. Returns how many were dropped.
pub fn prune_unreached(program: &mut IrProgram) -> usize {
    // (Some(module index) | None = the root program, fn index)
    let mut stamped: Vec<(Option<usize>, usize, Sym)> = Vec::new();
    for (i, f) in program.functions.iter().enumerate() {
        if is_on_demand(f) {
            stamped.push((None, i, f.name));
        }
    }
    for (m, module) in program.modules.iter().enumerate() {
        for (i, f) in module.functions.iter().enumerate() {
            if is_on_demand(f) {
                stamped.push((Some(m), i, f.name));
            }
        }
    }
    if stamped.is_empty() {
        return 0;
    }
    let mut by_last: HashMap<&str, Vec<usize>> = HashMap::new();
    for (k, (_, _, name)) in stamped.iter().enumerate() {
        by_last.entry(last_segment(name.as_str())).or_default().push(k);
    }
    let body = |k: usize| -> &IrFunction {
        let (m, i, _) = stamped[k];
        match m {
            None => &program.functions[i],
            Some(m) => &program.modules[m].functions[i],
        }
    };

    // Roots: every unstamped fn and every top-level binding.
    let mut refs = Refs { names: Vec::new() };
    for f in program.functions.iter().filter(|f| !is_on_demand(f)) {
        refs.visit_expr(&f.body);
    }
    for tl in &program.top_lets {
        refs.visit_expr(&tl.value);
    }
    for module in &program.modules {
        for f in module.functions.iter().filter(|f| !is_on_demand(f)) {
            refs.visit_expr(&f.body);
        }
        for tl in &module.top_lets {
            refs.visit_expr(&tl.value);
        }
    }

    let mut reached: HashSet<usize> = HashSet::new();
    let mut pending = refs.names;
    while let Some(r) = pending.pop() {
        let Some(cands) = by_last.get(last_segment(&r)) else { continue };
        for &k in cands {
            if reached.contains(&k) || !may_name(stamped[k].2.as_str(), &r) {
                continue;
            }
            reached.insert(k);
            let mut inner = Refs { names: Vec::new() };
            inner.visit_expr(&body(k).body);
            pending.extend(inner.names);
        }
    }

    let dropped: HashSet<(Option<usize>, usize)> = stamped.iter().enumerate()
        .filter(|(k, _)| !reached.contains(k))
        .map(|(_, (m, i, _))| (*m, *i))
        .collect();
    if dropped.is_empty() {
        return 0;
    }
    let mut i = 0;
    program.functions.retain(|_| {
        let keep = !dropped.contains(&(None, i));
        i += 1;
        keep
    });
    for (m, module) in program.modules.iter_mut().enumerate() {
        let mut i = 0;
        module.functions.retain(|_| {
            let keep = !dropped.contains(&(Some(m), i));
            i += 1;
            keep
        });
    }
    dropped.len()
}

#[cfg(test)]
mod tests {
    use super::may_name;

    #[test]
    fn qualified_spellings_name_each_other() {
        assert!(may_name("User.__strict", "User.__strict"));
        assert!(may_name("app.User.__strict", "User.__strict"));
        assert!(may_name("User.decode_strict", "app.User.decode_strict"));
        assert!(may_name("User.decode_strict", "decode_strict"));
        assert!(!may_name("User.__strict", "AUser.__strict"));
        assert!(!may_name("User.__strict_at", "User.__strict"));
    }
}
