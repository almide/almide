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

    // 1b. Register stdlib named types (from [[types]] blocks in
    //     stdlib/defs/*.toml). Each is stored under "module.Name" so
    //     user code can reference `process.ProcessStatus` etc.
    register_stdlib_named_types(&mut env);

    // 2. Register user modules (with prefix)
    for (name, mod_prog, is_self) in modules {
        register_module(&mut env, &mut diagnostics, name, mod_prog, is_self);
    }

    // 3. Build import table for main program
    let self_name = env.self_module_name.map(|s| s.to_string());
    let (table, import_diags) = build_import_table(program, self_name.as_deref(), &env.user_modules);
    env.import_table = table;
    diagnostics.extend(import_diags);

    // 4. Register main program declarations
    registration::register_decls(&mut env, &mut diagnostics, &program.decls, None);

    CanonicalizationResult { env, diagnostics }
}

/// Parse a TOML type string (as written in `stdlib/defs/*.toml [[types]]`
/// field declarations) into a `Ty`. Primitives only for this PR —
/// List[T] / Option[T] / user-defined named types land with the
/// downstream codegen PRs that actually need to resolve them.
fn parse_stdlib_field_ty(ty_str: &str) -> almide_lang::types::Ty {
    use almide_lang::types::Ty;
    match ty_str {
        "Int" => Ty::Int,
        "Float" => Ty::Float,
        "String" => Ty::String,
        "Bool" => Ty::Bool,
        "Unit" => Ty::Unit,
        "Bytes" => Ty::Bytes,
        _ => Ty::Unknown,
    }
}

fn register_stdlib_named_types(env: &mut TypeEnv) {
    use almide_base::intern::sym;
    use almide_lang::types::Ty;
    for t in crate::generated::stdlib_types::STDLIB_TYPES {
        let fields: Vec<(almide_base::intern::Sym, Ty)> = t
            .fields
            .iter()
            .map(|f| (sym(f.name), parse_stdlib_field_ty(f.ty)))
            .collect();
        let record = Ty::Record { fields };
        // Mirror register_type_decl: "module.Name" (what the resolver matches
        // against module-qualified type expressions) AND bare "Name" (what
        // resolve_named falls back to via Ty::Named).
        let key = format!("{}.{}", t.module, t.name);
        env.types.insert(sym(&key), record.clone());
        env.types.insert(sym(t.name), record);
    }
}

#[cfg(test)]
mod stdlib_types_registration_tests {
    use super::*;
    use almide_base::intern::sym;
    use almide_lang::types::Ty;

    #[test]
    fn process_status_registered_with_correct_fields() {
        let mut env = TypeEnv::new();
        register_stdlib_named_types(&mut env);

        // Both the module-qualified key and the bare key are populated.
        let qualified = env.types.get(&sym("process.ProcessStatus"));
        let bare = env.types.get(&sym("ProcessStatus"));
        assert!(qualified.is_some(), "process.ProcessStatus should be registered");
        assert!(bare.is_some(), "bare ProcessStatus should be registered as fallback");

        // Field shape matches the [[types]] declaration.
        match qualified.unwrap() {
            Ty::Record { fields } => {
                let got: Vec<(&str, &Ty)> =
                    fields.iter().map(|(n, t)| (n.as_str(), t)).collect();
                assert_eq!(got.len(), 3);
                assert_eq!(got[0].0, "code");
                assert!(matches!(got[0].1, Ty::Int));
                assert_eq!(got[1].0, "stdout");
                assert!(matches!(got[1].1, Ty::String));
                assert_eq!(got[2].0, "stderr");
                assert!(matches!(got[2].1, Ty::String));
            }
            other => panic!("expected Ty::Record, got {:?}", other),
        }
    }
}
