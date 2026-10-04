//! EffectInferencePass: auto-infer capability requirements from stdlib usage.
//!
//! Analyzes which stdlib modules each function calls (directly and transitively)
//! and maps them to effect categories (IO, Net, Env, Time, Rand, Fan — the
//! `almide_ir::effect::Effect` enum; there is no `Log` category).
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

use almide_ir::*;
use super::pass::{NanoPass, PassResult, Target};

// Re-export from almide-ir
pub use almide_ir::effect::{Effect, FunctionEffects, EffectMap};

pub(crate) fn module_to_effect(module: &str) -> Option<Effect> {
    match module {
        "fs" | "path" => Some(Effect::IO),
        "http" | "url" => Some(Effect::Net),
        "env" | "process" => Some(Effect::Env),
        "time" | "datetime" => Some(Effect::Time),
        "fan" => Some(Effect::Fan),
        _ => None,
    }
}

pub(crate) fn runtime_name_to_effect(name: &str) -> Option<Effect> {
    if !name.starts_with("almide_rt_") {
        return None;
    }
    let rest = &name["almide_rt_".len()..];
    let module = rest.split('_').next()?;
    module_to_effect(module)
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
        assert_eq!(module_to_effect("fs"), Some(Effect::IO));
        assert_eq!(module_to_effect("path"), Some(Effect::IO));
        assert_eq!(module_to_effect("http"), Some(Effect::Net));
        assert_eq!(module_to_effect("url"), Some(Effect::Net));
        assert_eq!(module_to_effect("env"), Some(Effect::Env));
        assert_eq!(module_to_effect("process"), Some(Effect::Env));
        assert_eq!(module_to_effect("time"), Some(Effect::Time));
        assert_eq!(module_to_effect("datetime"), Some(Effect::Time));
        assert_eq!(module_to_effect("fan"), Some(Effect::Fan));
        assert_eq!(module_to_effect("list"), None);
        assert_eq!(module_to_effect("string"), None);
        assert_eq!(module_to_effect("math"), None);
    }

    #[test]
    fn runtime_name_to_effect_mapping() {
        assert_eq!(runtime_name_to_effect("almide_rt_fs_read_text"), Some(Effect::IO));
        assert_eq!(runtime_name_to_effect("almide_rt_http_get"), Some(Effect::Net));
        assert_eq!(runtime_name_to_effect("almide_rt_env_get"), Some(Effect::Env));
        assert_eq!(runtime_name_to_effect("almide_rt_time_now"), Some(Effect::Time));
        assert_eq!(runtime_name_to_effect("almide_rt_list_map"), None);
        assert_eq!(runtime_name_to_effect("println"), None);
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
