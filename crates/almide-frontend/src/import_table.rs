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

    /// Selective imports: bare name → canonical module name.
    /// `import json.{from_string, stringify}` produces:
    ///   "from_string" → "json", "stringify" → "json"
    /// Consumers (checker, lowering) rewrite a bare call `from_string(x)` to
    /// the qualified form `json.from_string(x)`.
    pub direct: HashMap<Sym, Sym>,
}

impl ImportTable {
    /// Create with Tier 1 auto-imported stdlib modules.
    pub fn new() -> Self {
        let mut stdlib = HashSet::new();
        for m in &["string", "int", "float", "list", "bytes", "matrix", "map", "set", "option", "result", "value", "prim"] {
            stdlib.insert(sym(m));
        }
        ImportTable {
            aliases: HashMap::new(),
            accessible: HashSet::new(),
            stdlib,
            used: HashSet::new(),
            direct: HashMap::new(),
        }
    }

    /// Resolve a bare name (e.g. `from_string`) to its qualified form (`json.from_string`)
    /// if it was selectively imported. Returns `None` otherwise.
    pub fn resolve_direct(&self, name: &str) -> Option<String> {
        self.direct.get(&sym(name)).map(|m| format!("{}.{}", m.as_str(), name))
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
    user_modules: &HashSet<Sym>,
) -> (ImportTable, Vec<Diagnostic>) {
    let mut table = ImportTable::new();
    let mut diagnostics = Vec::new();

    // Track for collision/duplicate detection
    let mut alias_to_canonical: HashMap<String, String> = HashMap::new();
    let mut canonical_to_alias: HashMap<String, String> = HashMap::new();

    for imp in &prog.imports {
        build_import_table_process_import(imp, module_name, user_modules, &mut table, &mut diagnostics, &mut alias_to_canonical, &mut canonical_to_alias);
    }

    // Also register Tier 1 auto-imports from bundled modules
    for m in crate::stdlib::AUTO_IMPORT_BUNDLED {
        table.accessible.insert(sym(m));
    }

    (table, diagnostics)
}

/// Process one `import` declaration into `table`/`diagnostics`: canonical-name
/// resolution (with `import self` special-casing), used-name derivation,
/// ambiguous/duplicate-import diagnostics, and stdlib/selective-import
/// registration. Verbatim text move of the loop body out of
/// [`build_import_table`]; a `continue` in the original loop becomes an
/// early `return` here (both simply skip the rest of this import and move
/// on to the next one).
fn build_import_table_process_import(
    imp: &ast::Decl,
    module_name: Option<&str>,
    user_modules: &HashSet<Sym>,
    table: &mut ImportTable,
    diagnostics: &mut Vec<Diagnostic>,
    alias_to_canonical: &mut HashMap<String, String>,
    canonical_to_alias: &mut HashMap<String, String>,
) {
    let (path, alias, names, span) = match imp {
        ast::Decl::Import { path, alias, names, span } => (path, alias, names, span),
        _ => return,
    };

    let (canonical, is_self) = resolve_import_canonical(path, module_name, user_modules);
    let used_name = resolve_import_used_name(path, alias, is_self, &canonical);

    if !check_import_collision(&used_name, &canonical, alias_to_canonical, diagnostics, span) {
        return;
    }
    check_import_duplicate(&used_name, &canonical, canonical_to_alias, diagnostics, span);
    register_import(&used_name, &canonical, alias_to_canonical, canonical_to_alias, table);
    register_import_stdlib(&used_name, path, is_self, table);
    register_import_selective(names, &canonical, table);
}

/// Steps 1-2 of [`build_import_table_process_import`]: build the canonical
/// module name from the import path, resolving `import self` (and
/// `import self.a.b`) — the resolver's registration is asymmetric (the
/// project's own self-loaded modules use leaf names, a dependency
/// package's submodules use the FQN); prefer FQN when registered, fall
/// back to leaf. Verbatim text move.
fn resolve_import_canonical(path: &[Sym], module_name: Option<&str>, user_modules: &HashSet<Sym>) -> (String, bool) {
    let mut canonical = path.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(".");
    let is_self = path.first().map(|s| s.as_str()) == Some("self");
    if is_self {
        if path.len() == 1 {
            // import self → package root name
            if let Some(mod_name) = module_name {
                canonical = mod_name.to_string();
            }
        } else {
            // import self.a.b → prefer "<module>.a.b" if registered, else "b"
            let leaf = path.last().unwrap().to_string();
            let fqn = if let Some(mod_name) = module_name {
                let suffix = path[1..].iter().map(|s| s.as_str()).collect::<Vec<_>>().join(".");
                format!("{}.{}", mod_name, suffix)
            } else {
                leaf.clone()
            };
            canonical = if user_modules.contains(&sym(&fqn)) {
                fqn
            } else {
                leaf
            };
        }
    }
    (canonical, is_self)
}

/// Step 3 of [`build_import_table_process_import`]: determine the used
/// name (what the programmer writes to call functions). Verbatim text move.
fn resolve_import_used_name(path: &[Sym], alias: &Option<Sym>, is_self: bool, canonical: &str) -> String {
    if let Some(a) = alias {
        a.to_string()
    } else if path.len() > 1 {
        // Go-style: last segment is the namespace
        path.last().unwrap().to_string()
    } else if is_self {
        // import self → use the resolved package name
        canonical.split('.').last().unwrap_or(canonical).to_string()
    } else {
        path.last().unwrap().to_string()
    }
}

/// Step 4 of [`build_import_table_process_import`]: collision detection —
/// same `used_name` for different canonicals is an error. Returns `false`
/// when the caller should stop processing this import (mirrors the
/// original loop's `continue`). Verbatim text move.
fn check_import_collision(
    used_name: &str, canonical: &str, alias_to_canonical: &HashMap<String, String>,
    diagnostics: &mut Vec<Diagnostic>, span: &Option<ast::Span>,
) -> bool {
    if let Some(existing) = alias_to_canonical.get(used_name) {
        if existing != canonical {
            diagnostics.push(Diagnostic::error(
                format!("ambiguous import: '{}' could refer to '{}' or '{}'", used_name, existing, canonical),
                format!("Use `import {} as <alias>` to disambiguate", canonical),
                format!("import at line {}", span.as_ref().map(|s| s.line).unwrap_or(0)),
            ));
            return false;
        }
    }
    true
}

/// Step 5 of [`build_import_table_process_import`]: duplicate detection —
/// same canonical imported twice is a warning. Verbatim text move.
fn check_import_duplicate(
    used_name: &str, canonical: &str, canonical_to_alias: &HashMap<String, String>,
    diagnostics: &mut Vec<Diagnostic>, span: &Option<ast::Span>,
) {
    if let Some(existing_alias) = canonical_to_alias.get(canonical) {
        if existing_alias != used_name {
            diagnostics.push(Diagnostic::warning(
                format!("module '{}' is already imported as '{}'", canonical, existing_alias),
                "Remove the duplicate import".to_string(),
                format!("import at line {}", span.as_ref().map(|s| s.line).unwrap_or(0)),
            ));
        }
    }
}

/// Step 6 of [`build_import_table_process_import`]: register the resolved
/// alias/canonical pair. Verbatim text move.
fn register_import(
    used_name: &str, canonical: &str,
    alias_to_canonical: &mut HashMap<String, String>, canonical_to_alias: &mut HashMap<String, String>,
    table: &mut ImportTable,
) {
    alias_to_canonical.insert(used_name.to_string(), canonical.to_string());
    canonical_to_alias.entry(canonical.to_string()).or_insert_with(|| used_name.to_string());
    table.aliases.insert(sym(used_name), sym(canonical));
    table.accessible.insert(sym(canonical));
}

/// Step 7 of [`build_import_table_process_import`]: stdlib detection.
/// Verbatim text move.
fn register_import_stdlib(used_name: &str, path: &[Sym], is_self: bool, table: &mut ImportTable) {
    if crate::stdlib::is_any_stdlib(used_name) {
        table.stdlib.insert(sym(used_name));
    }
    // Also check canonical for multi-segment stdlib (unlikely but safe)
    let first_segment = path.first().map(|s| s.as_str()).unwrap_or("");
    if crate::stdlib::is_any_stdlib(first_segment) && !is_self {
        table.stdlib.insert(sym(first_segment));
    }
}

/// Step 8 of [`build_import_table_process_import`]: selective import —
/// register each name → canonical module mapping so a bare call
/// `from_string(x)` resolves to `json.from_string(x)`. Verbatim text move.
fn register_import_selective(names: &Option<Vec<Sym>>, canonical: &str, table: &mut ImportTable) {
    if let Some(name_list) = names {
        for n in name_list {
            table.direct.insert(*n, sym(canonical));
        }
    }
}
