//! ImportTable: Single source of truth for import resolution.
//!
//! Built once per file (main or module) after parsing and module discovery.
//! Consumed by checker, lowering, and codegen — none of which contain resolution logic.

use std::collections::{HashMap, HashSet};
use almide_lang::ast;
use almide_base::intern::{Sym, sym};
use almide_base::diagnostic::Diagnostic;

/// Resolved import table for one compilation unit (main file or dependency module).
#[derive(Debug, Clone)]
pub struct ImportTable {
    /// Short name → canonical module name (fully resolved).
    /// Examples:
    ///   "python" → "bindgen.bindings.python"  (implicit alias from last segment)
    ///   "lander" → "lander"                    (self → package name, resolved)
    ///   "json"   → "json"                      (stdlib, identity)
    ///   "py"     → "bindgen.bindings.python"   (explicit `as py`)
    pub aliases: HashMap<Sym, Sym>,

    /// Modules directly accessible from this file (canonical names).
    /// Only these can be used in qualified calls. Enforces Go-style: no transitive access.
    pub accessible: HashSet<Sym>,

    /// Stdlib modules in scope (Tier 1 auto-import + explicitly imported).
    pub stdlib: HashSet<Sym>,

    /// Modules actually referenced in code. Written by checker for unused-import detection.
    pub used: HashSet<Sym>,
}

impl ImportTable {
    /// Create with Tier 1 auto-imported stdlib modules.
    pub fn new() -> Self {
        let mut stdlib = HashSet::new();
        for m in &["string", "int", "float", "list", "bytes", "matrix", "map", "set", "option", "result", "value"] {
            stdlib.insert(sym(m));
        }
        ImportTable {
            aliases: HashMap::new(),
            accessible: HashSet::new(),
            stdlib,
            used: HashSet::new(),
        }
    }

    /// Check if a name resolves to a known module (stdlib, accessible, or aliased).
    pub fn is_module(&self, name: &str) -> bool {
        self.stdlib.contains(&sym(name))
            || self.accessible.contains(&sym(name))
            || self.aliases.contains_key(&sym(name))
    }

    /// Resolve a short name to its canonical module name.
    /// Returns the canonical name, or the input name if it's a direct stdlib/accessible module.
    pub fn resolve(&self, name: &str) -> Option<Sym> {
        let s = sym(name);
        if let Some(&canonical) = self.aliases.get(&s) {
            Some(canonical)
        } else if self.stdlib.contains(&s) || self.accessible.contains(&s) {
            Some(s)
        } else {
            None
        }
    }

    /// Mark a module as used (by its short/display name as written in code).
    pub fn mark_used(&mut self, name: &str) {
        self.used.insert(sym(name));
    }

    /// Resolve a dotted module path like `a.b.c` from a Member chain expression.
    /// Works for both `accessible` modules (user packages) and `aliases` (imports).
    /// Used by both checker (infer_pipe) and lowering (lower_call_target) to
    /// eliminate duplicated module resolution logic.
    pub fn resolve_dotted_path(&self, kind: &ast::ExprKind) -> Option<String> {
        match kind {
            ast::ExprKind::Member { object, field, .. } => {
                // Direct root: `root.field`
                if let ast::ExprKind::Ident { name: root, .. } = &object.kind {
                    let resolved_root = self.resolve(root)
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| root.to_string());
                    let candidate = format!("{}.{}", resolved_root, field);
                    if self.accessible.contains(&sym(&candidate)) {
                        return Some(candidate);
                    }
                    let prefix = format!("{}.", candidate);
                    if self.accessible.iter().any(|m| m.as_str().starts_with(&prefix)) {
                        return Some(candidate);
                    }
                }
                // Recursive: `parent.field`
                if let Some(parent) = self.resolve_dotted_path(&object.kind) {
                    let candidate = format!("{}.{}", parent, field);
                    if self.accessible.contains(&sym(&candidate)) {
                        return Some(candidate);
                    }
                    let prefix = format!("{}.", candidate);
                    if self.accessible.iter().any(|m| m.as_str().starts_with(&prefix)) {
                        return Some(candidate);
                    }
                }
                None
            }
            _ => None,
        }
    }
}

/// Build an ImportTable from a program's imports.
///
/// `module_name`: The name of the current module (None for main file).
///                Used to resolve `import self` → actual package name.
/// `user_modules`: Set of all known user module canonical names (from resolver).
///
/// Returns (ImportTable, diagnostics).
pub fn build_import_table(
    prog: &ast::Program,
    module_name: Option<&str>,
    _user_modules: &HashSet<Sym>,
) -> (ImportTable, Vec<Diagnostic>) {
    let mut table = ImportTable::new();
    let mut diagnostics = Vec::new();

    // Track for collision/duplicate detection
    let mut alias_to_canonical: HashMap<String, String> = HashMap::new();
    let mut canonical_to_alias: HashMap<String, String> = HashMap::new();

    for imp in &prog.imports {
        let (path, alias, span) = match imp {
            ast::Decl::Import { path, alias, span, .. } => (path, alias, span),
            _ => continue,
        };

        // 1. Build canonical name
        let mut canonical = path.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(".");
        let is_self = path.first().map(|s| s.as_str()) == Some("self");

        // 2. Resolve self → module name as registered by resolve
        //
        // The resolver registers modules under these names:
        //   import self           → package name (from alias or almide.toml)
        //   import self.sub       → last segment ("sub")
        //   import self.a.b       → last segment ("b")
        // Functions are registered as "module_name.fn_name" via register_module.
        // The canonical name here MUST match what the resolver used.
        if is_self {
            if path.len() == 1 {
                // import self → package root name
                if let Some(mod_name) = module_name {
                    canonical = mod_name.to_string();
                }
            } else {
                // import self.xxx → last segment (matches resolver's registration)
                canonical = path.last().unwrap().to_string();
            }
        }

        // 3. Determine used name (what the programmer writes to call functions)
        let used_name = if let Some(a) = alias {
            a.to_string()
        } else if path.len() > 1 {
            // Go-style: last segment is the namespace
            path.last().unwrap().to_string()
        } else if is_self {
            // import self → use the resolved package name
            canonical.split('.').last().unwrap_or(&canonical).to_string()
        } else {
            path.last().unwrap().to_string()
        };

        // 4. Collision detection: same used_name for different canonicals → error
        if let Some(existing) = alias_to_canonical.get(&used_name) {
            if existing != &canonical {
                diagnostics.push(Diagnostic::error(
                    format!("ambiguous import: '{}' could refer to '{}' or '{}'", used_name, existing, canonical),
                    format!("Use `import {} as <alias>` to disambiguate", canonical),
                    format!("import at line {}", span.as_ref().map(|s| s.line).unwrap_or(0)),
                ));
                continue;
            }
        }

        // 5. Duplicate detection: same canonical imported twice → warning
        if let Some(existing_alias) = canonical_to_alias.get(&canonical) {
            if existing_alias != &used_name {
                diagnostics.push(Diagnostic::warning(
                    format!("module '{}' is already imported as '{}'", canonical, existing_alias),
                    "Remove the duplicate import".to_string(),
                    format!("import at line {}", span.as_ref().map(|s| s.line).unwrap_or(0)),
                ));
            }
        }

        // 6. Register
        alias_to_canonical.insert(used_name.clone(), canonical.clone());
        canonical_to_alias.entry(canonical.clone()).or_insert_with(|| used_name.clone());
        table.aliases.insert(sym(&used_name), sym(&canonical));
        table.accessible.insert(sym(&canonical));

        // 7. Stdlib detection
        if crate::stdlib::is_any_stdlib(&used_name) {
            table.stdlib.insert(sym(&used_name));
        }
        // Also check canonical for multi-segment stdlib (unlikely but safe)
        let first_segment = path.first().map(|s| s.as_str()).unwrap_or("");
        if crate::stdlib::is_any_stdlib(first_segment) && !is_self {
            table.stdlib.insert(sym(first_segment));
        }
    }

    // Also register Tier 1 auto-imports from bundled modules
    for m in crate::stdlib::AUTO_IMPORT_BUNDLED {
        table.accessible.insert(sym(m));
    }

    (table, diagnostics)
}
