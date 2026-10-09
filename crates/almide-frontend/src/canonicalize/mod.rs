//! Canonicalize: name resolution and declaration registration.
//!
//! Extracts import resolution and declaration registration from the type checker
//! into a standalone pre-pass. The pipeline becomes:
//!
//! ```text
//! Parser → AST → Canonicalize (this module) → Checker (inference only) → Lowering → IR
//! ```

pub mod resolve;
pub mod protocols;
pub mod registration;

use almide_lang::ast;
use almide_base::diagnostic::Diagnostic;
use crate::import_table::build_import_table;
use almide_base::intern::sym;
use crate::types::TypeEnv;

/// Result of the canonicalization pass.
pub struct CanonicalizationResult {
    pub env: TypeEnv,
    pub diagnostics: Vec<Diagnostic>,
}

/// Register a user module's declarations into the environment (with prefix).
pub fn register_module(
    env: &mut TypeEnv,
    diagnostics: &mut Vec<Diagnostic>,
    name: &str,
    prog: &ast::Program,
    is_self: bool,
) {
    env.user_modules.insert(name.into());
    if is_self {
        env.self_module_name = Some(sym(name));
    }
    // The module's own import scope for its bare type names (#2715). Its
    // imports registered before it (the resolver loads leaves first).
    let package = name.split('.').next().unwrap_or(name);
    let (table, _) = build_import_table(prog, Some(package), &env.user_modules);
    env.module_import_aliases.insert(sym(name), table.aliases.clone());
    let saved = std::mem::replace(&mut env.import_table, table);
    resolve::register_scoped_bare_type_keys(env, Some(name));
    env.import_table = saved;
    registration::register_decls(env, diagnostics, &prog.decls, Some(name));
}

/// Run the full canonicalization pass: builtin protocols → module registration →
/// import table → main program registration.
///
/// After this, `env` is fully populated and ready for `Checker::from_env`.
///
/// The module-set E008 summaries are skipped when no program of the
/// compilation can have a concurrent site (#3509): the checker reads them only
/// from a site's walk. An interface consumer (`Checker::concurrent_fn_facts`)
/// reads them from every fn, so it goes through [`canonicalize_program_in`],
/// which always computes them.
pub fn canonicalize_program<'a>(
    program: &ast::Program,
    modules: impl Iterator<Item = (&'a str, &'a ast::Program, bool)>,
) -> CanonicalizationResult {
    canonicalize_program_with(program, modules, None, true)
}

/// [`canonicalize_program`] with the entry program's IDENTITY: when the
/// entry is a bundled stdlib module checked on its own (`almide compile
/// bytes --json` stages the bundled source as the entry),
/// `entry_bundled_module` names it, so the module's own `type` declarations
/// of the names it owns keep the stdlib's bare key instead of a user
/// program's shadow scope (#1828, `TypeEnv::entry_bundled_module`). `None`
/// is every other entry program, unchanged.
pub fn canonicalize_program_in<'a>(
    program: &ast::Program,
    modules: impl Iterator<Item = (&'a str, &'a ast::Program, bool)>,
    entry_bundled_module: Option<&str>,
) -> CanonicalizationResult {
    canonicalize_program_with(program, modules, entry_bundled_module, false)
}

fn canonicalize_program_with<'a>(
    program: &ast::Program,
    modules: impl Iterator<Item = (&'a str, &'a ast::Program, bool)>,
    entry_bundled_module: Option<&str>,
    check_only: bool,
) -> CanonicalizationResult {
    let mut env = TypeEnv::new();
    env.entry_bundled_module = entry_bundled_module
        .filter(|m| almide_lang::stdlib_info::is_bundled_module(m))
        .map(sym);
    let mut diagnostics = Vec::new();

    // 1. Built-in protocols
    protocols::register_builtin_protocols(&mut env);

    // 1b. Register type declarations from ALL bundled stdlib modules.
    // Ensures aliases like `type TcpStream = Int` from net.almd are
    // available when user modules reference them in type declarations.
    for module_name in almide_lang::stdlib_info::BUNDLED_MODULES {
        crate::bundled_sigs::register_bundled_types(module_name, &mut env);
    }

    // 2. Register user modules (with prefix)
    let collect_dep_roots = |env: &mut TypeEnv, prog: &ast::Program| {
        for imp in &prog.imports {
            if let ast::Decl::Import { path, .. } = imp {
                if let Some(root) = path.first() {
                    if root.as_str() != "self"
                        && !almide_lang::stdlib_info::is_stdlib_module(root.as_str())
                    {
                        env.dep_root_modules.insert(*root);
                    }
                }
            }
        }
    };
    collect_dep_roots(&mut env, program);
    let modules: Vec<(&'a str, &'a ast::Program, bool)> = modules.collect();
    for &(name, mod_prog, is_self) in &modules {
        collect_dep_roots(&mut env, mod_prog);
        // An imported module carrying a future dialect stamp is the same
        // error as the main file carrying one — it was verified somewhere
        // this compiler cannot reproduce. Attributed to the module by name so
        // the reader is not sent to the wrong file.
        crate::dialect_check::check_dialect_stamp_in(Some(name), mod_prog, &mut diagnostics);
        register_module(&mut env, &mut diagnostics, name, mod_prog, is_self);
    }
    compute_concurrent_summaries(&mut env, &modules, check_only.then_some(program));

    // 2b. The file's dialect stamp, if it carries one. Program-level and
    // resolution-independent, so it runs before any name is resolved: a file
    // stamped for a dialect this compiler does not speak should say so rather
    // than produce a pile of downstream errors about names that moved.
    crate::dialect_check::check_dialect_stamp(program, &mut diagnostics);

    // 3. Build import table for main program
    let self_name = env.self_module_name.map(|s| s.to_string());
    let (table, import_diags) = build_import_table(program, self_name.as_deref(), &env.user_modules);
    env.import_table = table;
    diagnostics.extend(import_diags);
    // Every alias spelling of a dependency type (`sh.Box`, `shape.Box`)
    // resolves to its canonical key from here on (#1955).
    resolve::register_alias_type_keys(&mut env);
    resolve::register_scoped_bare_type_keys(&mut env, None);

    // 4. Register main program declarations
    registration::register_decls(&mut env, &mut diagnostics, &program.decls, None);

    // 5. Carry parse-failure fn names so checker can suppress cascades.
    env.failed_fn_names.extend(program.failed_fn_names.iter().cloned());

    // Attribute diagnostics gathered during registration join the rest.
    diagnostics.extend(std::mem::take(&mut env.attr_diagnostics));

    CanonicalizationResult { env, diagnostics }
}


// ── Stage-2 split (greenfield structure-new code; ARCHITECTURE.md §6.5) ──
//
// `canonicalize_program` above is the incumbent's, verbatim and untouched.
// The two functions below split the same work at the module/entry seam so a
// caller can CACHE the module half per import set (it re-registers every
// bundled stdlib module — 25% of a per-file check, measured by s4_probe) and
// run only the entry half per file. Validity domain: entries whose imports
// are all stdlib modules (an entry with dependency-package imports seeds
// `dep_root_modules` BEFORE module registration in the verbatim path, which
// this split does not reproduce). Byte-equivalence over that domain is
// adjudicated by the 1,062-file oracle parity gate.

/// Module half: builtin protocols + bundled type registration + module
/// registration — everything that depends only on the resolved module set.
pub fn canonicalize_modules_env<'a>(
    modules: impl Iterator<Item = (&'a str, &'a ast::Program, bool)>,
) -> CanonicalizationResult {
    let mut env = TypeEnv::new();
    let mut diagnostics = Vec::new();
    protocols::register_builtin_protocols(&mut env);
    for module_name in almide_lang::stdlib_info::BUNDLED_MODULES {
        crate::bundled_sigs::register_bundled_types(module_name, &mut env);
    }
    let modules: Vec<(&'a str, &'a ast::Program, bool)> = modules.collect();
    for &(name, mod_prog, is_self) in &modules {
        for imp in &mod_prog.imports {
            if let ast::Decl::Import { path, .. } = imp {
                if let Some(root) = path.first() {
                    if root.as_str() != "self"
                        && !almide_lang::stdlib_info::is_stdlib_module(root.as_str())
                    {
                        env.dep_root_modules.insert(*root);
                    }
                }
            }
        }
        crate::dialect_check::check_dialect_stamp_in(Some(name), mod_prog, &mut diagnostics);
        register_module(&mut env, &mut diagnostics, name, mod_prog, is_self);
    }
    compute_concurrent_summaries(&mut env, &modules, None);
    CanonicalizationResult { env, diagnostics }
}

/// E008 (ADR-0020 §3): each user module fn's inferred concurrent slots and
/// whether its body reaches a `var`, so a program that calls it across a
/// module or package boundary is judged by the callee's facts.
///
/// `check_entry` is the entry program when the facts are read only by the
/// reach check: then a compilation where neither it nor any module can put an
/// argument in a concurrent slot skips them (#3509). Without a site every
/// inferred slot is empty and no walk reads a reach, so the empty map reads
/// exactly as the computed one would.
fn compute_concurrent_summaries(
    env: &mut TypeEnv,
    modules: &[(&str, &ast::Program, bool)],
    check_entry: Option<&ast::Program>,
) {
    use crate::concurrent_reach::may_have_sites;
    let tables: Vec<_> = modules
        .iter()
        .filter(|(name, _, _)| !almide_lang::stdlib_info::is_stdlib_module(name))
        .map(|&(name, prog, _)| {
            let (table, _) = build_import_table(prog, Some(name), &env.user_modules);
            (sym(name), prog, table.aliases, table.direct)
        })
        .collect();
    if tables.is_empty() {
        return;
    }
    if let Some(entry) = check_entry {
        // The entry's table is the one step 3 builds: the same inputs.
        let none = crate::concurrent_reach::Summaries::new();
        let self_name = env.self_module_name.map(|s| s.to_string());
        let (t, _) = build_import_table(entry, self_name.as_deref(), &env.user_modules);
        let quiet = !may_have_sites(entry, &t.aliases, &t.direct, &none)
            && tables.iter().all(|(_, p, a, d)| !may_have_sites(p, a, d, &none));
        if quiet {
            return;
        }
    }
    let summaries = {
        let env_ref = &*env;
        let type_is_fn = |te: &ast::TypeExpr| crate::concurrent_reach_types::type_expr_is_fn_valued(env_ref, te);
        crate::concurrent_reach::module_summaries(&tables, &type_is_fn)
    };
    env.concurrent_summaries = summaries;
}

/// Entry half: the per-file steps (2b–5 of the verbatim path) applied onto a
/// modules env. Appends its diagnostics in the verbatim order.
pub fn canonicalize_entry_onto(
    env: &mut TypeEnv,
    diagnostics: &mut Vec<Diagnostic>,
    program: &ast::Program,
) {
    for imp in &program.imports {
        if let ast::Decl::Import { path, .. } = imp {
            if let Some(root) = path.first() {
                if root.as_str() != "self"
                    && !almide_lang::stdlib_info::is_stdlib_module(root.as_str())
                {
                    env.dep_root_modules.insert(*root);
                }
            }
        }
    }
    crate::dialect_check::check_dialect_stamp(program, diagnostics);
    let self_name = env.self_module_name.map(|s| s.to_string());
    let (table, import_diags) = build_import_table(program, self_name.as_deref(), &env.user_modules);
    env.import_table = table;
    diagnostics.extend(import_diags);
    // Every alias spelling of a dependency type (`sh.Box`, `shape.Box`)
    // resolves to its canonical key from here on (#1955).
    resolve::register_alias_type_keys(env);
    resolve::register_scoped_bare_type_keys(env, None);
    registration::register_decls(env, diagnostics, &program.decls, None);
    env.failed_fn_names.extend(program.failed_fn_names.iter().cloned());
    diagnostics.extend(std::mem::take(&mut env.attr_diagnostics));
}
