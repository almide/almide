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
//! Phase 1: Analysis only. Results stored in IrProgram.effect_map.
//! Phase 2: `almide check --effects` command (future).
//! Phase 3: almide.toml [permissions] enforcement (future).

use std::collections::{HashMap, HashSet};
use almide_ir::*;
use super::pass::{NanoPass, PassResult, Target};

// Re-export from almide-ir
pub use almide_ir::effect::{Effect, FunctionEffects, EffectMap};

/// The category of a source-level stdlib call `module.func` — the one table
/// in `almide_ir::effect::STDLIB_MODULE_EFFECTS` (#3246).
fn module_call_effect(module: &str, func: &str) -> Option<Effect> {
    almide_ir::effect::stdlib_call_effect(module, func)
}

/// The category of a runtime call (`almide_rt_<module>_<fn>`).
fn runtime_name_to_effect(name: &str) -> Option<Effect> {
    almide_ir::effect::runtime_symbol_effect(name)
}

/// The direct effects of a function declaration. An `@extern` body is
/// foreign: inference cannot see into it and no bound can be declared on it
/// yet, so it is ⊤ — every category (#3245, ADR-0027 §5). Before this, its
/// `_` body contributed nothing and a foreign call reached the host
/// uncategorised, past `[permissions]` and `check --effects`.
fn declared_direct_effects(func: &IrFunction) -> HashSet<Effect> {
    if !func.extern_attrs.is_empty() {
        return Effect::ALL.into_iter().collect();
    }
    collect_direct_effects(&func.body)
}

#[derive(Debug)]
pub struct EffectInferencePass;

impl NanoPass for EffectInferencePass {
    fn name(&self) -> &str { "EffectInference" }
    fn targets(&self) -> Option<Vec<Target>> { None } // All targets

    /// An analysis over Module calls, taken before the lowerings replace them.
    fn run_before(&self) -> Vec<&'static str> { vec!["StdlibLowering", "ResultPropagation"] }

    fn run(&self, mut program: IrProgram, _target: Target) -> PassResult {
        let mut effect_map = EffectMap::default();

        // Step 1: Collect direct effects for each function
        seed_function_effects(&program, &mut effect_map);

        // Step 2: Build call graph
        let call_graph = build_call_graph(&program);

        // Step 3: Transitive closure (fixpoint iteration)
        close_effects_transitively(&call_graph, &mut effect_map);

        // Debug output
        if almide_base::env::flag("ALMIDE_DEBUG_EFFECTS") {
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
        let direct = declared_direct_effects(func);
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
            let direct = declared_direct_effects(func);
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

/// Step 3 of `EffectInferencePass::run`: solve the call graph to its least
/// fixpoint, so every caller's `transitive` set has absorbed every callee's.
///
/// Worklist over reverse edges: when a function's set grows, only its callers
/// are revisited. Sets only grow and are bounded by the finite `Effect` enum,
/// so this terminates with no iteration cap — a fixed round budget (formerly
/// 20) would hand a partially-propagated set to `check_permissions` on a call
/// chain deeper than the budget, reporting "no effect" for an effectful caller.
fn close_effects_transitively(call_graph: &HashMap<String, HashSet<String>>, effect_map: &mut EffectMap) {
    let mut callers_of: HashMap<&str, Vec<&str>> = HashMap::new();
    for (caller, callees) in call_graph {
        for callee in callees {
            callers_of.entry(callee.as_str()).or_default().push(caller.as_str());
        }
    }

    let mut worklist: Vec<String> = effect_map.functions.iter()
        .filter(|(_, fe)| !fe.transitive.is_empty())
        .map(|(name, _)| name.clone())
        .collect();
    while let Some(callee) = worklist.pop() {
        let Some(callers) = callers_of.get(callee.as_str()) else { continue };
        let callee_effects = match effect_map.functions.get(&callee) {
            Some(fe) => fe.transitive.clone(),
            None => continue,
        };
        for &caller in callers {
            if let Some(fe) = effect_map.functions.get_mut(caller) {
                let before = fe.transitive.len();
                fe.transitive.extend(callee_effects.iter().copied());
                if fe.transitive.len() > before {
                    worklist.push(caller.to_string());
                }
            }
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
            // Module call: list.map, fs.read_text, etc.
            IrExprKind::Call { target: CallTarget::Module { module, func, .. }, .. } => {
                if let Some(effect) = module_call_effect(module, func) {
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
    fn module_call_effect_mapping() {
        assert_eq!(module_call_effect("fs", "read_text"), Some(Effect::IO));
        assert_eq!(module_call_effect("io", "read_line"), Some(Effect::IO));
        assert_eq!(module_call_effect("path", "join"), None);
        assert_eq!(module_call_effect("http", "get"), Some(Effect::Net));
        assert_eq!(module_call_effect("url", "parse"), None);
        assert_eq!(module_call_effect("env", "get"), Some(Effect::Env));
        assert_eq!(module_call_effect("process", "exec"), Some(Effect::Env));
        assert_eq!(module_call_effect("datetime", "now"), Some(Effect::Time));
        assert_eq!(module_call_effect("random", "int"), Some(Effect::Rand));
        assert_eq!(module_call_effect("fan", "map"), Some(Effect::Fan));
        assert_eq!(module_call_effect("list", "map"), None);
        assert_eq!(module_call_effect("string", "len"), None);
        assert_eq!(module_call_effect("math", "sqrt"), None);
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
            assert_eq!(declared_direct_effects(&f), Effect::ALL.into_iter().collect(), "is_effect={is_effect}");
        }
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

    /// Seeds `names` with empty sets, except `seeded` which gets `IO`.
    fn seeded_map(names: &[String], seeded: &str) -> EffectMap {
        let mut map = EffectMap::default();
        for name in names {
            let direct: HashSet<Effect> = if name == seeded { [Effect::IO].into() } else { HashSet::new() };
            map.functions.insert(name.clone(), FunctionEffects {
                direct: direct.clone(),
                transitive: direct,
                is_effect: false,
            });
        }
        map
    }

    // T03: a call chain far deeper than the former 20-round budget still
    // carries the leaf's effect to the root.
    #[test]
    fn long_call_chain_reaches_the_root() {
        let n = 200;
        let names: Vec<String> = (0..n).map(|i| format!("f{i}")).collect();
        let graph: HashMap<String, HashSet<String>> = (0..n - 1)
            .map(|i| (names[i].clone(), [names[i + 1].clone()].into()))
            .collect();
        let mut map = seeded_map(&names, &names[n - 1]);
        close_effects_transitively(&graph, &mut map);
        for name in &names {
            assert_eq!(map.functions[name].transitive, [Effect::IO].into(), "{name} lost IO");
        }
    }

    // T04: an effect entering anywhere in a cycle reaches the whole cycle and
    // every caller of it; the direct set is untouched.
    #[test]
    fn cycle_propagates_to_every_member_and_caller() {
        let names: Vec<String> = ["main", "a", "b", "c", "pure"].iter().map(|s| s.to_string()).collect();
        let graph: HashMap<String, HashSet<String>> = [
            ("main", vec!["a", "pure"]),
            ("a", vec!["b"]),
            ("b", vec!["c"]),
            ("c", vec!["a"]),
        ].into_iter()
            .map(|(k, v)| (k.to_string(), v.into_iter().map(String::from).collect()))
            .collect();
        let mut map = seeded_map(&names, "b");
        close_effects_transitively(&graph, &mut map);
        for name in ["main", "a", "b", "c"] {
            assert_eq!(map.functions[name].transitive, [Effect::IO].into(), "{name} lost IO");
        }
        assert!(map.functions["pure"].transitive.is_empty());
        assert!(map.functions["a"].direct.is_empty());
    }
}
