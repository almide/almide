//! EffectInferencePass: auto-infer capability requirements from stdlib usage.
//!
//! Analyzes which stdlib modules each function calls (directly and transitively)
//! and maps them to effect categories (IO, Net, Env, Time, Rand, Fan, Log).
//!
//! This is the foundation for Security Layer 2-3:
//! - Layer 2: Package declares allowed capabilities in almide.toml
//! - Layer 3: Consumer restricts dependency capabilities
//!
//! Design principle: "User writes `effect fn`. Compiler infers the rest."
//!
//! Phase 1: Analysis only. Results stored in IrProgram.effect_map.
//! Phase 2: `almide check --effects` command (future).
//! Phase 3: almide.toml [permissions] enforcement (future).

use std::collections::{HashMap, HashSet};
use almide_ir::*;
use super::pass::{NanoPass, PassResult, Target};

// Re-export from almide-ir
pub use almide_ir::effect::{Effect, FunctionEffects, EffectMap};

fn module_to_effect(module: &str) -> Option<Effect> {
    match module {
        "fs" | "path" => Some(Effect::IO),
        "http" | "url" => Some(Effect::Net),
        "env" | "process" => Some(Effect::Env),
        "time" | "datetime" => Some(Effect::Time),
        "fan" => Some(Effect::Fan),
        _ => None,
    }
}

fn runtime_name_to_effect(name: &str) -> Option<Effect> {
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

    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        let mut effect_map = EffectMap::default();

        // Step 1: Collect direct effects for each function
        seed_function_effects(&program, &mut effect_map);

        // Step 2: Build call graph
        let call_graph = build_call_graph(&program);

        // Step 3: Transitive closure (fixpoint iteration)
        close_effects_transitively(&call_graph, &mut effect_map);

        // Debug output
        if std::env::var("ALMIDE_DEBUG_EFFECTS").is_ok() {
            debug_print_effects(&effect_map);
        }

        program.effect_map = effect_map;

        PassResult { program, changed: true }
    }
}

/// Step 1 of `EffectInferencePass::run`, extracted verbatim (cog>30
/// decomposition, sequential-phase pattern — `effect_map` is a write-only
/// accumulator w.r.t. this phase). Collects each function's direct effects
/// (top-level, then module-scoped).
fn seed_function_effects(program: &IrProgram, effect_map: &mut EffectMap) {
    for func in &program.functions {
        let direct = collect_direct_effects(&func.body);
        let is_effect = func.is_effect;
        effect_map.functions.insert(func.name.to_string(), FunctionEffects {
            direct: direct.clone(),
            transitive: direct,
            is_effect,
        });
    }

    // Also scan module functions
    for module in &program.modules {
        for func in &module.functions {
            let direct = collect_direct_effects(&func.body);
            let qualified = format!("{}.{}", module.name, func.name);
            let is_effect = func.is_effect;
            effect_map.functions.insert(qualified, FunctionEffects {
                direct: direct.clone(),
                transitive: direct,
                is_effect,
            });
        }
    }
}

/// Step 3 of `EffectInferencePass::run`, extracted verbatim (cog>30
/// decomposition): fixpoint-iterate the call graph until every caller's
/// `transitive` effect set has absorbed every (already-seeded) callee's.
fn close_effects_transitively(call_graph: &HashMap<String, HashSet<String>>, effect_map: &mut EffectMap) {
    let max_iterations = 20;
    for _ in 0..max_iterations {
        let mut changed = false;
        for (caller, callees) in call_graph {
            let callee_effects: HashSet<Effect> = callees.iter()
                .filter_map(|callee| effect_map.functions.get(callee))
                .flat_map(|fe| fe.transitive.iter().copied())
                .collect();

            if let Some(fe) = effect_map.functions.get_mut(caller) {
                let before = fe.transitive.len();
                fe.transitive.extend(callee_effects);
                if fe.transitive.len() > before {
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
}

/// `ALMIDE_DEBUG_EFFECTS` debug-output phase of `EffectInferencePass::run`,
/// extracted verbatim (cog>30 decomposition).
fn debug_print_effects(effect_map: &EffectMap) {
    let mut entries: Vec<_> = effect_map.functions.iter().collect();
    entries.sort_by_key(|(name, _)| (*name).clone());
    for (name, fe) in &entries {
        if !fe.transitive.is_empty() {
            eprintln!(
                "[EffectInference] {} → {} {}",
                name,
                EffectMap::format_effects(&fe.transitive),
                if fe.is_effect { "(effect fn)" } else { "" }
            );
        }
    }
    // Summary
    let pure_count = entries.iter().filter(|(_, fe)| fe.transitive.is_empty()).count();
    let effect_count = entries.len() - pure_count;
    eprintln!(
        "[EffectInference] {} functions analyzed: {} pure, {} with effects",
        entries.len(), pure_count, effect_count
    );
}

/// Collect direct effects from stdlib calls in an expression.
fn collect_direct_effects(expr: &IrExpr) -> HashSet<Effect> {
    let mut collector = EffectCollector { effects: HashSet::new() };
    collector.visit_expr(expr);
    collector.effects
}

/// Traversal-total effect collector. Classifies the effect-bearing nodes
/// (module/named/runtime calls, fan) and delegates *all* descent — into those
/// nodes' children and into every other node — to `walk_expr`/`walk_stmt`.
/// A new `IrExprKind`/`IrStmtKind` variant is automatically traversed, and a
/// forgotten one is a compile error in the almide-ir walk primitive, not a
/// silently-dropped subtree here.
struct EffectCollector {
    effects: HashSet<Effect>,
}

impl IrVisitor for EffectCollector {
    fn visit_expr(&mut self, expr: &IrExpr) {
        match &expr.kind {
            // Module call: list.map, fs.read_text, etc.
            IrExprKind::Call { target: CallTarget::Module { module, .. }, .. } => {
                if let Some(effect) = module_to_effect(module) {
                    self.effects.insert(effect);
                }
            }

            // Named call: almide_rt_fs_read_text, etc.
            IrExprKind::Call { target: CallTarget::Named { name }, .. } => {
                if let Some(effect) = runtime_name_to_effect(name) {
                    self.effects.insert(effect);
                }
            }

            // Pre-resolved runtime call (from @intrinsic). Symbol follows
            // the same `almide_rt_<m>_<f>` mangling as Named, so reuse
            // `runtime_name_to_effect`.
            IrExprKind::RuntimeCall { symbol, .. } => {
                if let Some(effect) = runtime_name_to_effect(symbol) {
                    self.effects.insert(effect);
                }
            }

            // Fan expressions.
            IrExprKind::Fan { .. } => {
                self.effects.insert(Effect::Fan);
            }

            _ => {}
        }
        // Exhaustive descent into all children (covers the non-classified
        // nodes, and the children of the classified ones above).
        walk_expr(self, expr);
    }
}

/// Build a call graph: caller → set of callee function names.
fn build_call_graph(program: &IrProgram) -> HashMap<String, HashSet<String>> {
    let mut graph: HashMap<String, HashSet<String>> = HashMap::new();

    for func in &program.functions {
        graph.insert(func.name.to_string(), collect_callees(&func.body));
    }

    for module in &program.modules {
        for func in &module.functions {
            let qualified = format!("{}.{}", module.name, func.name);
            graph.insert(qualified, collect_callees(&func.body));
        }
    }

    graph
}

fn collect_callees(expr: &IrExpr) -> HashSet<String> {
    let mut collector = CalleeCollector { callees: HashSet::new() };
    collector.visit_expr(expr);
    collector.callees
}

/// Traversal-total callee collector. Records the user-function call targets
/// (named non-runtime calls + module calls) and delegates *all* descent to
/// `walk_expr`/`walk_stmt`. A new node kind is automatically traversed, and a
/// forgotten one is a compile error in the almide-ir walk primitive, not a
/// silently-dropped subtree here.
struct CalleeCollector {
    callees: HashSet<String>,
}

impl IrVisitor for CalleeCollector {
    fn visit_expr(&mut self, expr: &IrExpr) {
        match &expr.kind {
            IrExprKind::Call { target: CallTarget::Named { name }, .. }
            | IrExprKind::TailCall { target: CallTarget::Named { name }, .. } => {
                // Skip runtime functions — they're stdlib, not user functions.
                if !name.starts_with("almide_rt_") {
                    self.callees.insert(name.to_string());
                }
            }
            IrExprKind::Call { target: CallTarget::Module { module, func, .. }, .. }
            | IrExprKind::TailCall { target: CallTarget::Module { module, func, .. }, .. } => {
                self.callees.insert(format!("{}.{}", module, func));
            }
            _ => {}
        }
        // Exhaustive descent into all children.
        walk_expr(self, expr);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
