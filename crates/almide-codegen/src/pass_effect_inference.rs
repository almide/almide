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

        // Step 2: Build call graph
        let call_graph = build_call_graph(&program);

        // Step 3: Transitive closure (fixpoint iteration)
        let max_iterations = 20;
        for _ in 0..max_iterations {
            let mut changed = false;
            for (caller, callees) in &call_graph {
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

        // Debug output
        if std::env::var("ALMIDE_DEBUG_EFFECTS").is_ok() {
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

        program.effect_map = effect_map;

        PassResult { program, changed: true }
    }
}

/// Collect direct effects from stdlib calls in an expression.
fn collect_direct_effects(expr: &IrExpr) -> HashSet<Effect> {
    let mut effects = HashSet::new();
    collect_effects_inner(expr, &mut effects);
    effects
}

fn collect_effects_inner(expr: &IrExpr, effects: &mut HashSet<Effect>) {
    match &expr.kind {
        // Module call: list.map, fs.read_text, etc.
        IrExprKind::Call { target: CallTarget::Module { module, .. }, args, .. } => {
            if let Some(effect) = module_to_effect(module) {
                effects.insert(effect);
            }
            // Check for math.random specifically
            if module == "math" {
                // math module functions that use randomness
                // (detected by function name in args or specific math functions)
            }
            for arg in args {
                collect_effects_inner(arg, effects);
            }
        }

        // Named call: almide_rt_fs_read_text, etc.
        IrExprKind::Call { target: CallTarget::Named { name }, args, .. } => {
            if let Some(effect) = runtime_name_to_effect(name) {
                effects.insert(effect);
            }
            for arg in args {
                collect_effects_inner(arg, effects);
            }
        }

        // Pre-resolved runtime call (from @intrinsic). Symbol follows
        // the same `almide_rt_<m>_<f>` mangling as Named, so reuse
        // `runtime_name_to_effect`.
        IrExprKind::RuntimeCall { symbol, args } => {
            if let Some(effect) = runtime_name_to_effect(symbol) {
                effects.insert(effect);
            }
            for arg in args {
                collect_effects_inner(arg, effects);
            }
        }

        // Fan expressions
        IrExprKind::Fan { exprs } => {
            effects.insert(Effect::Fan);
            for e in exprs {
                collect_effects_inner(e, effects);
            }
        }

        // Recurse into all other expression kinds
        IrExprKind::Call { args, target, .. } => {
            if let CallTarget::Method { object, .. } = target {
                collect_effects_inner(object, effects);
            }
            if let CallTarget::Computed { callee } = target {
                collect_effects_inner(callee, effects);
            }
            for arg in args {
                collect_effects_inner(arg, effects);
            }
        }

        IrExprKind::Block { stmts, expr } => {
            for stmt in stmts {
                collect_effects_from_stmt(stmt, effects);
            }
            if let Some(e) = expr {
                collect_effects_inner(e, effects);
            }
        }

        IrExprKind::If { cond, then, else_ } => {
            collect_effects_inner(cond, effects);
            collect_effects_inner(then, effects);
            collect_effects_inner(else_, effects);
        }

        IrExprKind::Match { subject, arms } => {
            collect_effects_inner(subject, effects);
            for arm in arms {
                if let Some(g) = &arm.guard {
                    collect_effects_inner(g, effects);
                }
                collect_effects_inner(&arm.body, effects);
            }
        }

        IrExprKind::Lambda { body, .. } => {
            collect_effects_inner(body, effects);
        }

        IrExprKind::ForIn { iterable, body, .. } => {
            collect_effects_inner(iterable, effects);
            for stmt in body {
                collect_effects_from_stmt(stmt, effects);
            }
        }

        IrExprKind::While { cond, body } => {
            collect_effects_inner(cond, effects);
            for stmt in body {
                collect_effects_from_stmt(stmt, effects);
            }
        }

        IrExprKind::BinOp { left, right, .. } => {
            collect_effects_inner(left, effects);
            collect_effects_inner(right, effects);
        }

        IrExprKind::UnOp { operand, .. } => {
            collect_effects_inner(operand, effects);
        }

        IrExprKind::List { elements } | IrExprKind::Tuple { elements } => {
            for e in elements {
                collect_effects_inner(e, effects);
            }
        }

        IrExprKind::Record { fields, .. } => {
            for (_, v) in fields {
                collect_effects_inner(v, effects);
            }
        }

        IrExprKind::SpreadRecord { base, fields } => {
            collect_effects_inner(base, effects);
            for (_, v) in fields {
                collect_effects_inner(v, effects);
            }
        }

        IrExprKind::OptionSome { expr } | IrExprKind::ResultOk { expr }
        | IrExprKind::ResultErr { expr } | IrExprKind::Try { expr }
        | IrExprKind::Unwrap { expr } | IrExprKind::ToOption { expr }
        | IrExprKind::Member { object: expr, .. } | IrExprKind::OptionalChain { expr, .. }
        | IrExprKind::Borrow { expr, .. }
        | IrExprKind::ToVec { expr } | IrExprKind::Clone { expr }
        | IrExprKind::Deref { expr } => {
            collect_effects_inner(expr, effects);
        }
        IrExprKind::UnwrapOr { expr, fallback } => {
            collect_effects_inner(expr, effects);
            collect_effects_inner(fallback, effects);
        }

        IrExprKind::StringInterp { parts } => {
            for part in parts {
                if let IrStringPart::Expr { expr } = part {
                    collect_effects_inner(expr, effects);
                }
            }
        }

        IrExprKind::MapLiteral { entries } => {
            for (k, v) in entries {
                collect_effects_inner(k, effects);
                collect_effects_inner(v, effects);
            }
        }

        IrExprKind::Range { start, end, .. } => {
            collect_effects_inner(start, effects);
            collect_effects_inner(end, effects);
        }

        IrExprKind::IndexAccess { object, index } => {
            collect_effects_inner(object, effects);
            collect_effects_inner(index, effects);
        }

        IrExprKind::MapAccess { object, key } => {
            collect_effects_inner(object, effects);
            collect_effects_inner(key, effects);
        }

        // Leaf nodes — no effects
        _ => {}
    }
}

fn collect_effects_from_stmt(stmt: &IrStmt, effects: &mut HashSet<Effect>) {
    match &stmt.kind {
        IrStmtKind::Bind { value, .. } => collect_effects_inner(value, effects),
        IrStmtKind::Assign { value, .. } => collect_effects_inner(value, effects),
        IrStmtKind::Expr { expr } => collect_effects_inner(expr, effects),
        IrStmtKind::Guard { cond, else_ } => {
            collect_effects_inner(cond, effects);
            collect_effects_inner(else_, effects);
        }
        IrStmtKind::BindDestructure { value, .. } => collect_effects_inner(value, effects),
        IrStmtKind::FieldAssign { value, .. } => collect_effects_inner(value, effects),
        _ => {}
    }
}

/// Build a call graph: caller → set of callee function names.
fn build_call_graph(program: &IrProgram) -> HashMap<String, HashSet<String>> {
    let mut graph: HashMap<String, HashSet<String>> = HashMap::new();

    for func in &program.functions {
        let mut callees = HashSet::new();
        collect_callees(&func.body, &mut callees);
        graph.insert(func.name.to_string(), callees);
    }

    for module in &program.modules {
        for func in &module.functions {
            let mut callees = HashSet::new();
            collect_callees(&func.body, &mut callees);
            let qualified = format!("{}.{}", module.name, func.name);
            graph.insert(qualified, callees);
        }
    }

    graph
}

fn collect_callees(expr: &IrExpr, callees: &mut HashSet<String>) {
    match &expr.kind {
        IrExprKind::Call { target: CallTarget::Named { name }, args, .. } => {
            // Skip runtime functions — they're stdlib, not user functions
            if !name.starts_with("almide_rt_") {
                callees.insert(name.to_string());
            }
            for arg in args {
                collect_callees(arg, callees);
            }
        }
        IrExprKind::Call { target: CallTarget::Module { module, func }, args, .. } => {
            callees.insert(format!("{}.{}", module, func));
            for arg in args {
                collect_callees(arg, callees);
            }
        }
        IrExprKind::Block { stmts, expr } => {
            for stmt in stmts {
                match &stmt.kind {
                    IrStmtKind::Bind { value, .. } | IrStmtKind::Assign { value, .. } => collect_callees(value, callees),
                    IrStmtKind::Expr { expr } => collect_callees(expr, callees),
                    IrStmtKind::Guard { cond, else_ } => {
                        collect_callees(cond, callees);
                        collect_callees(else_, callees);
                    }
                    IrStmtKind::BindDestructure { value, .. } => collect_callees(value, callees),
                    _ => {}
                }
            }
            if let Some(e) = expr { collect_callees(e, callees); }
        }
        IrExprKind::If { cond, then, else_ } => {
            collect_callees(cond, callees);
            collect_callees(then, callees);
            collect_callees(else_, callees);
        }
        IrExprKind::Lambda { body, .. } => collect_callees(body, callees),
        IrExprKind::Match { subject, arms } => {
            collect_callees(subject, callees);
            for arm in arms {
                if let Some(g) = &arm.guard { collect_callees(g, callees); }
                collect_callees(&arm.body, callees);
            }
        }
        IrExprKind::ForIn { iterable, body, .. } => {
            collect_callees(iterable, callees);
            for stmt in body {
                match &stmt.kind {
                    IrStmtKind::Bind { value, .. } | IrStmtKind::Assign { value, .. } => collect_callees(value, callees),
                    IrStmtKind::Expr { expr } => collect_callees(expr, callees),
                    _ => {}
                }
            }
        }
        IrExprKind::Call { target, args, .. } => {
            if let CallTarget::Method { object, .. } = target { collect_callees(object, callees); }
            if let CallTarget::Computed { callee } = target { collect_callees(callee, callees); }
            for arg in args { collect_callees(arg, callees); }
        }
        IrExprKind::BinOp { left, right, .. } => {
            collect_callees(left, callees);
            collect_callees(right, callees);
        }
        _ => {}
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
