//! A package module's key is its own, never a compiler namespace (#2865).
//!
//! The emitter recognizes its intrinsic surfaces by the module KEY a
//! `CallTarget::Module` carries — `prim.*` is the raw-memory floor,
//! `list.*` / `map.*` / `bytes.*` … have hand-lowered arms, `int.*` reads
//! as scalar-pure inside a region. An in-package module keyed like one of
//! those namespaces (`src/prim.almd`, imported as `self.prim`) reached the
//! same arms: `prim.build` walled as an unknown prim op, and a package
//! `list.map` would have been lowered as the stdlib's.
//!
//! The leg that read the module off disk knows it is the package's own; it
//! hands that set here, and every such module whose key is also a namespace
//! the compiler resolves on its own is re-keyed to a spelling no source can
//! produce (`#` is not an identifier character), together with every call
//! and function reference into it from package code. Every spelling check
//! downstream then misses it by construction, instead of each one having to
//! remember to ask. Calls from compiler-bundled code (stdlib bodies, the
//! self-host implementations) keep their key: they can only mean the
//! compiler's namespace.

use std::collections::{HashMap, HashSet};

use almide_base::intern::{sym, Sym};
use almide_ir::visit_mut::{walk_expr_mut, IrMutVisitor};
use almide_ir::{CallTarget, IrExpr, IrExprKind, IrProgram};

/// The separator of a re-keyed package module: `prim` → `prim#pkg`.
const PACKAGE_KEY_MARK: &str = "#pkg";

/// Namespaces the compiler resolves by name without a package module: the
/// stdlib and bundled registries, the ADR-0001 clock constructors, the
/// `fan` scheduling primitive, and `bool` (a scalar namespace this
/// emitter's region purity reads).
pub fn is_compiler_namespace(key: &str) -> bool {
    use almide_types::stdlib_info::{BUNDLED_MODULES, STDLIB_MODULES};
    STDLIB_MODULES.contains(&key)
        || BUNDLED_MODULES.contains(&key)
        || almide_types::time_units::TIME_MODULES.iter().any(|(t, _)| *t == key)
        || matches!(key, "fan" | "bool")
}

/// The display spelling of a module key this pass may have re-keyed.
pub fn display_key(key: &str) -> &str {
    key.strip_suffix(PACKAGE_KEY_MARK).unwrap_or(key)
}

/// Re-key every module named in `package` (the modules the program's own
/// source declares, in-package or as a dependency) whose key is a compiler
/// namespace, and redirect the package code's calls into it. The entry
/// program's functions and top-lets are package code; so is every module in
/// `package`.
pub fn rekey_package_modules(ir: &mut IrProgram, package: &HashSet<String>) {
    // key → the function names the package module defines under it.
    let mut rekeyed: HashMap<Sym, (Sym, HashSet<Sym>)> = HashMap::new();
    for m in ir.modules.iter_mut() {
        let key = m.name.as_str().to_string();
        if !package.contains(&key) || !is_compiler_namespace(&key) {
            continue;
        }
        let fresh = sym(&format!("{key}{PACKAGE_KEY_MARK}"));
        let fns: HashSet<Sym> = m.functions.iter().map(|f| f.name).collect();
        rekeyed.insert(m.name, (fresh, fns));
        m.name = fresh;
    }
    if rekeyed.is_empty() {
        return;
    }
    let mut v = Rekey { map: &rekeyed };
    for f in ir.functions.iter_mut() {
        v.visit_expr_mut(&mut f.body);
    }
    for tl in ir.top_lets.iter_mut() {
        v.visit_expr_mut(&mut tl.value);
    }
    let fresh_keys: HashSet<Sym> = rekeyed.values().map(|(k, _)| *k).collect();
    for m in ir.modules.iter_mut() {
        let own = fresh_keys.contains(&m.name) || package.contains(m.name.as_str());
        if !own {
            continue;
        }
        for f in m.functions.iter_mut() {
            v.visit_expr_mut(&mut f.body);
        }
        for tl in m.top_lets.iter_mut() {
            v.visit_expr_mut(&mut tl.value);
        }
    }
}

struct Rekey<'a> {
    map: &'a HashMap<Sym, (Sym, HashSet<Sym>)>,
}

impl Rekey<'_> {
    /// The re-keyed module for `module.func`, when the package module under
    /// that key defines `func`.
    fn target(&self, module: Sym, func: Sym) -> Option<Sym> {
        self.map.get(&module).filter(|(_, fns)| fns.contains(&func)).map(|(k, _)| *k)
    }
}

impl IrMutVisitor for Rekey<'_> {
    fn visit_expr_mut(&mut self, expr: &mut IrExpr) {
        match &mut expr.kind {
            IrExprKind::Call { target: CallTarget::Module { module, func, .. }, .. }
            | IrExprKind::TailCall { target: CallTarget::Module { module, func, .. }, .. } => {
                if let Some(k) = self.target(*module, *func) {
                    *module = k;
                }
            }
            IrExprKind::FnRef { name } => {
                if let Some((m, f)) = name.as_str().rsplit_once('.')
                    && let Some(k) = self.target(sym(m), sym(f))
                {
                    *name = sym(&format!("{k}.{f}"));
                }
            }
            _ => {}
        }
        walk_expr_mut(self, expr);
    }
}
