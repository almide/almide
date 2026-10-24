//! EffectInferencePass: auto-infer capability requirements from stdlib usage.
//!
//! Analyzes which stdlib modules each function calls (directly and transitively)
//! and maps them to effect categories (IO, Net, Env, Time, Rand, Fan — the
//! `almide_ir::effect::Effect` enum; there is no `Log` category). The
//! module table is `almide_ir::effect::STDLIB_MODULE_EFFECTS`; an `@extern`
//! fn is every category (⊤).
//!
//! This is the foundation for Security Layer 2-3:
//! - Layer 2: Package declares allowed capabilities in almide.toml
//! - Layer 3: Consumer restricts dependency capabilities
//!
//! Design principle: "User writes `effect fn`. Compiler infers the rest."
//!
//! Results are stored in `IrProgram.effect_map`; `almide check --effects`
//! prints them and `[permissions].allow` in almide.toml is checked on them.
//! The sets are solved by `crate::effect_flow` (ADR-0026 D1): a category set
//! rides the fn value, so a function that calls a closure it is handed is
//! charged with that closure's set at the call site that hands it over.

use std::collections::HashSet;
use almide_ir::*;
use super::pass::{NanoPass, PassResult, Target};

// Re-export from almide-ir
pub use almide_ir::effect::{Effect, FunctionEffects, EffectMap};

/// The category of a source-level stdlib call `module.func` — the one table
/// in `almide_ir::effect::STDLIB_MODULE_EFFECTS` (#3246). With
/// [`runtime_name_to_effect`], the only per-operation input of
/// `crate::effect_flow`.
pub(crate) fn module_to_effect(module: &str, func: &str) -> Option<Effect> {
    almide_ir::effect::stdlib_call_effect(module, func)
}

/// The category of a runtime call (`almide_rt_<module>_<fn>`).
pub(crate) fn runtime_name_to_effect(name: &str) -> Option<Effect> {
    almide_ir::effect::runtime_symbol_effect(name)
}

/// The categories a function declaration carries regardless of its body. An
/// `@extern` body is foreign: inference cannot see into it and no bound can
/// be declared on it yet, so it is ⊤ — every category (#3245, ADR-0027 §5).
/// Before this, its `_` body contributed nothing and a foreign call reached
/// the host uncategorised, past `[permissions]` and `check --effects`.
/// `crate::effect_flow` seeds it as a constant on the fn's own node.
///
/// An `@inline_rust` template outside the stdlib is foreign code the same
/// way (#4100): the native build splices the Rust text in place of the
/// Almide body, which inference reads but the binary never runs. It is ⊤
/// too. The stdlib's own templates (`in_stdlib`) are categorised by the
/// stdlib call table instead, as before.
pub(crate) fn declared_direct_effects(func: &IrFunction, in_stdlib: bool) -> HashSet<Effect> {
    if !func.extern_attrs.is_empty() || (!in_stdlib && has_inline_rust(func)) {
        return Effect::ALL.into_iter().collect();
    }
    HashSet::new()
}

/// Does `func` carry an `@inline_rust` template?
pub(crate) fn has_inline_rust(func: &IrFunction) -> bool {
    func.attrs.iter().any(|a| a.name.as_str() == "inline_rust")
}

/// Is `module` the stdlib's own: a stdlib module name lowered without a
/// package identity? A dependency package always carries its versioned
/// name, so one named like a stdlib module is not mistaken for it.
pub(crate) fn is_stdlib_module(module: &IrModule) -> bool {
    module.versioned_name.is_none() && almide_lang::stdlib_info::is_any_stdlib(module.name.as_str())
}

#[derive(Debug)]
pub struct EffectInferencePass;

impl NanoPass for EffectInferencePass {
    fn name(&self) -> &str { "EffectInference" }
    fn targets(&self) -> Option<Vec<Target>> { None } // All targets

    /// An analysis over Module calls, taken before the lowerings replace them.
    fn run_before(&self) -> Vec<&'static str> { vec!["StdlibLowering", "ResultPropagation"] }

    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        let effect_map = crate::effect_flow::infer(&program);
        if almide_base::env::flag("ALMIDE_DEBUG_EFFECTS") {
            debug_print_effects(&effect_map);
        }
        program.effect_map = effect_map;
        PassResult { program, changed: true }
    }
}

/// `ALMIDE_DEBUG_EFFECTS` debug-output phase of `EffectInferencePass::run`,
/// extracted verbatim (cog>30 decomposition).
fn debug_print_effects(effect_map: &EffectMap) {
    let mut entries: Vec<_> = effect_map.functions.iter().collect();
    entries.sort_by_key(|(name, _)| (*name).clone());
    for (name, fe) in &entries {
        if !fe.transitive.is_empty() || !fe.indirect.is_empty() {
            eprintln!(
                "[EffectInference] {} → {} {}",
                name,
                fe.report(),
                if fe.is_effect { "(effect fn)" } else { "" }
            );
        }
    }
    // Summary
    let (pure, dependent, effects) = EffectMap::summary_counts(entries.iter().map(|(_, fe)| *fe));
    eprintln!(
        "[EffectInference] {} functions analyzed: {} pure, {} callback-dependent, {} with effects",
        entries.len(), pure, dependent, effects
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn module_to_effect_mapping() {
        assert_eq!(module_to_effect("fs", "read_text"), Some(Effect::IO));
        assert_eq!(module_to_effect("io", "read_line"), Some(Effect::IO));
        assert_eq!(module_to_effect("path", "join"), None);
        assert_eq!(module_to_effect("http", "get"), Some(Effect::Net));
        assert_eq!(module_to_effect("url", "parse"), None);
        assert_eq!(module_to_effect("env", "get"), Some(Effect::Env));
        assert_eq!(module_to_effect("process", "exec"), Some(Effect::Env));
        assert_eq!(module_to_effect("datetime", "now"), Some(Effect::Time));
        assert_eq!(module_to_effect("random", "int"), Some(Effect::Rand));
        assert_eq!(module_to_effect("fan", "map"), Some(Effect::Fan));
        assert_eq!(module_to_effect("list", "map"), None);
        assert_eq!(module_to_effect("string", "len"), None);
        assert_eq!(module_to_effect("math", "sqrt"), None);
    }

    #[test]
    fn runtime_name_to_effect_mapping() {
        assert_eq!(runtime_name_to_effect("almide_rt_fs_read_text"), Some(Effect::IO));
        assert_eq!(runtime_name_to_effect("almide_rt_http_get"), Some(Effect::Net));
        assert_eq!(runtime_name_to_effect("almide_rt_env_get"), Some(Effect::Env));
        assert_eq!(runtime_name_to_effect("almide_rt_datetime_now"), Some(Effect::Time));
        assert_eq!(runtime_name_to_effect("almide_rt_random_float"), Some(Effect::Rand));
        assert_eq!(runtime_name_to_effect("almide_rt_list_map"), None);
        assert_eq!(runtime_name_to_effect("println"), None);
    }

    fn extern_fn(name: &str, is_effect: bool) -> IrFunction {
        IrFunction {
            name: almide_base::intern::sym(name),
            params: vec![],
            ret_ty: almide_lang::types::Ty::Unit,
            body: IrExpr { kind: IrExprKind::Hole, ty: almide_lang::types::Ty::Unit, span: None, def_id: None },
            is_effect,
            is_test: false,
            generics: None,
            extern_attrs: vec![almide_lang::ast::ExternAttr {
                target: almide_base::intern::sym("rust"),
                module: almide_base::intern::sym("host"),
                function: almide_base::intern::sym(name),
                returns_promise: false,
            }],
            export_attrs: vec![],
            attrs: vec![],
            visibility: IrVisibility::Public,
            doc: None,
            blank_lines_before: 0,
            def_id: None,
            mutated_params: vec![], // fresh-fn: a test fixture extern with no params
            module_origin: None,
        }
    }

    // #3245: an `@extern` fn is ⊤ whether it is spelled `fn` or `effect fn`;
    // its `_` body no longer reads as "touches nothing".
    #[test]
    fn an_extern_fn_is_every_category() {
        for is_effect in [false, true] {
            let f = extern_fn("push_u32", is_effect);
            assert_eq!(declared_direct_effects(&f, false), Effect::ALL.into_iter().collect(), "is_effect={is_effect}");
        }
    }

    // #4100: an `@inline_rust` template is foreign code outside the stdlib.
    #[test]
    fn an_inline_rust_fn_outside_the_stdlib_is_every_category() {
        let mut f = extern_fn("splice", false);
        f.extern_attrs.clear();
        assert!(declared_direct_effects(&f, false).is_empty(), "a plain fn declares nothing");
        f.attrs.push(almide_lang::ast::Attribute {
            name: almide_base::intern::sym("inline_rust"),
            args: vec![],
            span: None,
        });
        assert_eq!(declared_direct_effects(&f, false), Effect::ALL.into_iter().collect());
        assert!(declared_direct_effects(&f, true).is_empty(), "the stdlib's own templates are categorised by the call table");
    }

    #[test]
    fn effect_display() {
        assert_eq!(format!("{}", Effect::IO), "IO");
        assert_eq!(format!("{}", Effect::Net), "Net");
    }

    #[test]
    fn format_effects_empty() {
        let effects = HashSet::new();
        assert_eq!(EffectMap::format_effects(&effects), "{}");
    }

    #[test]
    fn format_effects_sorted() {
        let mut effects = HashSet::new();
        effects.insert(Effect::Net);
        effects.insert(Effect::IO);
        assert_eq!(EffectMap::format_effects(&effects), "{IO, Net}");
    }
}
