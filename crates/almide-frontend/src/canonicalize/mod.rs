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
    registration::register_decls(env, diagnostics, &prog.decls, Some(name));
}

/// Run the full canonicalization pass: builtin protocols → module registration →
/// import table → main program registration.
///
/// After this, `env` is fully populated and ready for `Checker::from_env`.
pub fn canonicalize_program<'a>(
    program: &ast::Program,
    modules: impl Iterator<Item = (&'a str, &'a ast::Program, bool)>,
) -> CanonicalizationResult {
    let mut env = TypeEnv::new();
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
    for (name, mod_prog, is_self) in modules {
        collect_dep_roots(&mut env, mod_prog);
        // An imported module carrying a future dialect stamp is the same
        // error as the main file carrying one — it was verified somewhere
        // this compiler cannot reproduce. Attributed to the module by name so
        // the reader is not sent to the wrong file.
        crate::dialect_check::check_dialect_stamp_in(Some(name), mod_prog, &mut diagnostics);
        register_module(&mut env, &mut diagnostics, name, mod_prog, is_self);
    }

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

    // 4. Register main program declarations
    registration::register_decls(&mut env, &mut diagnostics, &program.decls, None);

    // 5. Carry parse-failure fn names so checker can suppress cascades.
    env.failed_fn_names.extend(program.failed_fn_names.iter().cloned());

    CanonicalizationResult { env, diagnostics }
}

