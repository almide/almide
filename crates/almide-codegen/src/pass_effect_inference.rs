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
//! Phase 1: Analysis only. Results stored in IrProgram.effect_map.
//! Phase 2: `almide check --effects` command (future).
//! Phase 3: almide.toml [permissions] enforcement (future).

use std::collections::{HashMap, HashSet};
use almide_ir::*;
use almide_lang::types::Ty;
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
    let known = known_function_names(program);
    for func in &program.functions {
        effect_map.functions.insert(func.name.to_string(), function_effects(func, &program.var_table, &known));
    }

    // Also scan module functions (each module numbers its own variables).
    for module in &program.modules {
        for func in &module.functions {
            let qualified = format!("{}.{}", module.name, func.name);
            effect_map.functions.insert(qualified, function_effects(func, &module.var_table, &known));
        }
    }
}

/// One function's seed: its direct categories, and the closure values it
/// calls without creating them (#3268).
fn function_effects(func: &IrFunction, vt: &VarTable, known: &HashSet<String>) -> FunctionEffects {
    let direct = collect_direct_effects(&func.body);
    FunctionEffects {
        direct: direct.clone(),
        transitive: direct,
        is_effect: func.is_effect,
        indirect: collect_indirect_calls(func, vt, known),
    }
}

/// The names a `Named` call can resolve to a user function by: top-level
/// fns bare, module fns bare and qualified. A `Named` call outside this set
/// (a variant constructor, a builtin) does not run its arguments.
fn known_function_names(program: &IrProgram) -> HashSet<String> {
    let mut names: HashSet<String> = program.functions.iter().map(|f| f.name.to_string()).collect();
    for module in &program.modules {
        for func in &module.functions {
            names.insert(func.name.to_string());
            names.insert(format!("{}.{}", module.name, func.name));
        }
    }
    names
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

/// The closure values `func` calls without creating them (#3268), sorted.
///
/// A fn-typed parameter is named with its position (`f (arg 1)`) whether it
/// is called here or handed to another call that may run it (`list.map(xs,
/// f)`, `apply(f, x)`). Any other called value is named by its path: a record
/// field `b.run`, a local `g`, `an element of hs`. A local bound to a lambda
/// or a named fn in this body is not listed: its body is this function's own
/// code and is already in its sets. No callee name is invented (ADR-0026 D4).
fn collect_indirect_calls(func: &IrFunction, vt: &VarTable, known: &HashSet<String>) -> Vec<String> {
    let fn_params = func.params.iter().enumerate()
        .filter(|(_, p)| matches!(p.ty, Ty::Fn { .. }))
        .map(|(i, p)| (p.var, format!("{} (arg {})", p.name, i + 1)))
        .collect();
    let mut collector = IndirectCallCollector { vt, known, fn_params, own_closures: HashSet::new(), found: Default::default() };
    collector.visit_expr(&func.body);
    collector.found.into_iter().collect()
}

struct IndirectCallCollector<'a> {
    vt: &'a VarTable,
    known: &'a HashSet<String>,
    /// fn-typed parameter → `name (arg N)`.
    fn_params: HashMap<VarId, String>,
    /// Locals bound to a lambda or a named fn in this body.
    own_closures: HashSet<VarId>,
    found: std::collections::BTreeSet<String>,
}

impl IndirectCallCollector<'_> {
    /// How a called value is named in the report, or `None` when the call
    /// runs code this function already accounts for.
    fn describe_callee(&self, callee: &IrExpr) -> Option<String> {
        match &callee.kind {
            IrExprKind::Lambda { .. } | IrExprKind::FnRef { .. } => None,
            IrExprKind::Var { id } if self.own_closures.contains(id) => None,
            IrExprKind::Var { id } => Some(self.fn_params.get(id).cloned().unwrap_or_else(|| self.var_name(*id))),
            IrExprKind::IndexAccess { object, .. } | IrExprKind::MapAccess { object, .. } => {
                Some(self.path(object).map_or_else(|| "a closure value".to_string(), |p| format!("an element of {p}")))
            }
            _ => Some(self.path(callee).unwrap_or_else(|| "a closure value".to_string())),
        }
    }

    /// `b.run`, `t.0`, `g` — a value spelled by variables and fields only.
    fn path(&self, expr: &IrExpr) -> Option<String> {
        match &expr.kind {
            IrExprKind::Var { id } => Some(self.var_name(*id)),
            IrExprKind::Member { object, field } => Some(format!("{}.{}", self.path(object)?, field)),
            IrExprKind::TupleIndex { object, index } => Some(format!("{}.{}", self.path(object)?, index)),
            _ => None,
        }
    }

    fn var_name(&self, id: VarId) -> String {
        self.vt.entries.get(id.0 as usize).map_or_else(|| "a closure value".to_string(), |v| v.name.to_string())
    }

    /// A call that may run its arguments: a module function or a user fn.
    fn may_run_args(&self, target: &CallTarget) -> bool {
        match target {
            CallTarget::Module { .. } => true,
            CallTarget::Named { name } => self.known.contains(name.as_str()),
            CallTarget::Method { .. } | CallTarget::Computed { .. } => true,
        }
    }
}

impl IrVisitor for IndirectCallCollector<'_> {
    fn visit_stmt(&mut self, stmt: &IrStmt) {
        if let IrStmtKind::Bind { var, value, .. } = &stmt.kind
            && matches!(value.kind, IrExprKind::Lambda { .. } | IrExprKind::FnRef { .. })
        {
            self.own_closures.insert(*var);
        }
        walk_stmt(self, stmt);
    }

    fn visit_expr(&mut self, expr: &IrExpr) {
        if let IrExprKind::Call { target, args, .. } | IrExprKind::TailCall { target, args } = &expr.kind {
            if let CallTarget::Computed { callee } = target
                && let Some(name) = self.describe_callee(callee)
            {
                self.found.insert(name);
            }
            if self.may_run_args(target) {
                for arg in args {
                    if let IrExprKind::Var { id } = &arg.kind
                        && let Some(name) = self.fn_params.get(id)
                    {
                        self.found.insert(name.clone());
                    }
                }
            }
        }
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
            // A named fn taken as a value is a closure created here: whoever
            // takes it is charged with what it does, the same as a lambda
            // written here (#3268).
            IrExprKind::FnRef { name } => {
                self.callees.insert(name.to_string());
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

    /// Seeds `names` with empty sets, except `seeded` which gets `IO`.
    fn seeded_map(names: &[String], seeded: &str) -> EffectMap {
        let mut map = EffectMap::default();
        for name in names {
            let direct: HashSet<Effect> = if name == seeded { [Effect::IO].into() } else { HashSet::new() };
            map.functions.insert(name.clone(), FunctionEffects {
                direct: direct.clone(),
                transitive: direct,
                is_effect: false,
                indirect: Vec::new(),
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
