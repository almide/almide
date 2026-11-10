//! Preserve the product selector's parallel map sites through MIR lowering.
//! A fresh wrapper keeps the literal callback and its closure origin. Its
//! unique name, not a source span or user-spelled prefix, identifies the site.
use almide_base::intern::{sym, Sym};
use almide_ir::visit_mut::{walk_expr_mut, IrMutVisitor};
use almide_ir::*;
use almide_lang::types::Ty;
use std::cell::RefCell;
use std::collections::{BTreeMap, HashSet};
use crate::lower::LowerError;

#[derive(Clone)]
struct MapSite { source: String, base: String, floats: usize, declared: Option<(String, String, String)> }
thread_local! {
    static MAPS: RefCell<BTreeMap<String, MapSite>> = const { RefCell::new(BTreeMap::new()) };
}

pub(crate) fn is_wrapper(name: &str) -> bool {
    MAPS.with(|m| m.borrow().contains_key(name))
}

/// Match the registered source call or the implementation the shared registry
/// links it to. Other calls in this function (including callback bodies) are
/// not parallel map sites.
pub(crate) fn selected(function: &str, callee: &str) -> bool {
    MAPS.with(|m| m.borrow().get(function).is_some_and(|site| {
        site.source == callee || site.base == callee || crate::mir_ops::self_host_runtime().iter().any(|(_, pairs)| {
            pairs.iter().any(|(implementation, call)| *implementation == callee && (*call == site.source || *call == site.base))
        })
    }))
}

/// Only a checked call actually registered by this render proposes a native
/// boundary for a pairing beyond the scalar self-host registry matrix.
pub(crate) fn declared_call(call: &str) -> bool {
    MAPS.with(|m| m.borrow().values().any(|site| site.source == call || site.base == call))
}

pub(crate) fn declaration(function: &str) -> Option<(String, String, String)> {
    MAPS.with(|m| m.borrow().get(function).and_then(|site| site.declared.clone()))
}

pub(crate) fn float_lanes(function: &str) -> Option<usize> {
    MAPS.with(|m| m.borrow().get(function).map(|site| site.floats))
}

pub(crate) fn prepare(ir: &mut IrProgram) -> Result<(), LowerError> {
    MAPS.with(|m| m.borrow_mut().clear());
    if !has_fan(ir) { return Ok(()); }
    if !almide_codegen::pass_rust_lowering_fan::route_fan_parallel(ir) { return Ok(()); }
    if almide_ir::meter_plan::program_has_regions(ir) {
        return Err(LowerError::at(None, "native: parallel map in a metered region requires per-thread budgets"));
    }
    let globals = ir.top_lets.iter().map(|t| t.var).collect();
    let mut lift = Lift {
        vars: &mut ir.var_table, used: ir.functions.iter().map(|f| f.name).collect(),
        globals, functions: Vec::new(), sites: BTreeMap::new(), error: None,
    };
    for f in ir.functions.iter_mut().filter(|f| !f.is_test) { lift.visit_expr_mut(&mut f.body); }
    let Lift { functions, sites, error, .. } = lift;
    if let Some(error) = error { return Err(error); }
    MAPS.with(|m| *m.borrow_mut() = sites);
    ir.functions.extend(functions);
    crate::lower::install_effect_abi_facts(ir);
    Ok(())
}

fn has_fan(ir: &IrProgram) -> bool {
    use almide_ir::visit::{walk_expr, IrVisitor};
    struct Find(bool);
    impl IrVisitor for Find {
        fn visit_expr(&mut self, e: &IrExpr) {
            if self.0 { return; }
            self.0 = matches!(&e.kind, IrExprKind::Fan { .. }) || matches!(&e.kind,
                IrExprKind::Call { target: CallTarget::Module { module, func, .. }, .. }
                if module.as_str() == "fan" && func.as_str() == "map");
            if !self.0 { walk_expr(self, e); }
        }
    }
    let mut find = Find(false);
    for f in ir.functions.iter().filter(|f| !f.is_test) { find.visit_expr(&f.body); }
    find.0
}

struct Lift<'a> {
    vars: &'a mut VarTable,
    used: HashSet<Sym>,
    globals: HashSet<VarId>,
    functions: Vec<IrFunction>,
    sites: BTreeMap<String, MapSite>,
    error: Option<LowerError>,
}

impl IrMutVisitor for Lift<'_> {
    fn visit_expr_mut(&mut self, e: &mut IrExpr) {
        walk_expr_mut(self, e);
        if self.error.is_some() { return; }
        let declared = if let IrExprKind::RuntimeCall { symbol, args } = &e.kind {
            let Some((module, func, _, parallel)) = almide_codegen::pass_rust_lowering_fan::parallel_declarations()
                .iter().find(|(_, _, _, p)| *p == symbol.as_str()) else { return };
            let declared = Some((module.to_string(), func.clone(), parallel.to_string()));
            e.kind = IrExprKind::Call { target: CallTarget::Module { module: sym(module), func: sym(func), def_id: None },
                args: args.clone(), type_args: Vec::new() };
            declared
        } else {
            let IrExprKind::Call { target: CallTarget::Module { module, func, .. }, .. } = &e.kind else { return };
            if module.as_str() != "fan" || func.as_str() != "map_par" { return; }
            None
        };
        if let Err(why) = self.map(e, declared) { self.error = Some(LowerError::at(e.span, why)); }
    }
}

impl Lift<'_> {
    fn map(&mut self, e: &mut IrExpr, declared: Option<(String, String, String)>) -> Result<(), String> {
        let IrExprKind::Call { target: CallTarget::Module { func, .. }, args, .. } = &mut e.kind else { unreachable!() };
        if args.len() != 2 || !matches!(args[1].kind, IrExprKind::Lambda { .. }) {
            return Err("native: selected parallel map lost its literal callback".into());
        }
        let arg_tys = args.iter().map(|a| a.ty.clone()).collect::<Vec<_>>();
        let call = if let Some((module, function, _)) = &declared {
            crate::lower::list_heap_call_name(module, function, &arg_tys, &e.ty, Default::default())
        } else {
            *func = sym("map");
            crate::lower::fan_map_call_name(&arg_tys, &e.ty)
        };
        let floats = float_pair(&args[0].ty, &e.ty);
        let mut next = self.used.len();
        let name = loop {
            let name = sym(&format!("__native_parallel_map_{next}"));
            if self.used.insert(name) { break name; }
            next += 1;
        };
        // Evaluate the input at the original call site, before reading captures.
        let input = args[0].clone();
        let id = self.vars.alloc_fresh("fan_input", input.ty.clone(), Mutability::Let, input.span);
        args[0] = variable(id, input.ty.clone(), input.span);
        let mut params = vec![param(id, &input.ty, self.vars)];
        let mut outer = vec![input];
        let free = almide_ir::free_vars::free_vars(&args[1], &HashSet::new());
        let mut subst = BTreeMap::new();
        for var in free {
            if subst.contains_key(&var) { continue; }
            let info = self.vars.get(var).clone();
            if info.mutability == Mutability::Var || self.globals.contains(&var) {
                return Err(format!("native: selected parallel map captures mutable/global `{}`", info.name));
            }
            let p = self.vars.alloc_like(var, info.ty.clone(), Mutability::Let);
            subst.insert(var, p);
            params.push(param(p, &info.ty, self.vars));
            outer.push(variable(var, info.ty, e.span));
        }
        crate::fan_native::rename_vars(&mut args[1], &subst);
        let body = e.clone();
        self.functions.push(IrFunction {
            name, params, ret_ty: e.ty.clone(), body, is_effect: false, is_test: false,
            generics: None, extern_attrs: Vec::new(), export_attrs: Vec::new(), attrs: Vec::new(),
            visibility: IrVisibility::Private, doc: None, blank_lines_before: 0, def_id: None,
            mutated_params: Vec::new(), module_origin: None, // fresh-fn: a lifted map site whose params are all fresh `let` binds (a `var` capture is refused above)
        });
        let base = declared.as_ref().map(|(module, func, _)| format!("{module}.{func}")).unwrap_or_else(|| "fan.map".into());
        self.sites.insert(name.to_string(), MapSite { source: call, base, floats, declared });
        e.kind = IrExprKind::Call { target: CallTarget::Named { name }, args: outer, type_args: Vec::new() };
        Ok(())
    }
}

fn float_pair(input: &Ty, result: &Ty) -> usize {
    use almide_lang::types::constructor::TypeConstructorId as TC;
    let list_float = |t: &Ty| matches!(t, Ty::Applied(TC::List, ts) if ts.len() == 1 && ts[0] == Ty::Float);
    usize::from(list_float(input)) | (usize::from(list_float(result) || matches!(result, Ty::Applied(TC::Result, ts) if ts.len() == 2 && list_float(&ts[0]))) << 1)
}

fn variable(id: VarId, ty: Ty, span: Option<Span>) -> IrExpr {
    IrExpr { kind: IrExprKind::Var { id }, ty, span, def_id: None }
}

fn param(var: VarId, ty: &Ty, vars: &VarTable) -> IrParam {
    IrParam { var, ty: ty.clone(), name: vars.get(var).name, borrow: ParamBorrow::Own,
        is_mut: false, open_record: None, default: None, attrs: Vec::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrapper_names_avoid_existing_functions() {
        let mut ir = crate::pipeline::source_to_ir_for_tests(
            r#"effect fn main() -> Unit = { let xs = fan.map([1, 2], (x) => ok(x))!; println("${xs}") }"#
        ).unwrap();
        let mut reserved = ir.functions[0].clone();
        reserved.name = sym(&format!("__native_parallel_map_{}", ir.functions.len() + 1));
        reserved.is_test = true;
        let name = reserved.name;
        ir.functions.push(reserved);
        prepare(&mut ir).unwrap();
        assert!(!is_wrapper(name.as_str()), "a pre-existing name never becomes a registered site");
        let names: HashSet<_> = ir.functions.iter().map(|f| f.name).collect();
        assert_eq!(names.len(), ir.functions.len());
    }

    #[test]
    fn parallel_list_family_cannot_disappear_into_sequential_loops() {
        let source = r#"effect fn main() -> Unit = {
 let xs = [1, 2, 3]
 let (a, b, c, d) = fan {
  list.map(xs, (x) => x + 1)
  list.filter(xs, (x) => x > 1)
  list.any(xs, (x) => x == 2)
  list.all(xs, (x) => x > 0)
 }
 println("${a} ${b} ${c} ${d}")
}"#;
        let (_, certs) = crate::pipeline::try_render_rust_source_certified(source).expect("parallel list family renders");
        let sites = crate::native_hof::sites();
        for (_, _, _, symbol) in almide_codegen::pass_rust_lowering_fan::parallel_declarations() {
            assert_eq!(sites.values().filter(|s| s.as_str() == *symbol).count(), 1, "selected {symbol} survives defunctionalization");
        }
        assert!(certs.iter().any(|(property, _, _)| property == "native-calls"));
    }

    #[test]
    fn selected_sites_survive_lowering_and_enter_runtime_witnesses() {
        let source = r#"
fn noisy(x: Int) -> Result[Int, String] = { println(int.to_string(x)); ok(x) }
effect fn main() -> Unit = {
  let a = fan.map([1, 2], (x) => ok(x + 1))!
  let b = fan.map([3, 4], (x) => noisy(x))!
  let c = fan.map([5, 6], (x) => ok(x + 7))!
  println("${a} ${b} ${c}")
}
"#;
        let (rust, certs) = crate::pipeline::try_render_rust_source_certified(source).expect("registered parallel calls render");
        let sites = crate::native_hof::sites();
        let parallel: Vec<_> = sites.iter().filter(|(_, symbol)| *symbol == "almide_rt_fan_map_par").collect();
        assert_eq!(parallel.len(), 2, "exactly the two selected call sites reach the parallel runtime");
        assert!(parallel.iter().all(|((function, _), _)| is_wrapper(function)));
        assert!(rust.contains("almide_rt_fan_map_par("));
        assert!(certs.iter().any(|(property, _, _)| property == "native-calls"));
        let wrappers: Vec<_> = MAPS.with(|m| m.borrow().keys().cloned().collect());
        crate::pipeline::try_render_rust_source_certified("effect fn main() -> Unit = println(\"plain\")").unwrap();
        assert!(wrappers.iter().all(|name| !is_wrapper(name)), "registry resets between renders");
    }
    /// A `fan.map` pairing outside the scalar self-host matrix that the
    /// product selector keeps sequential (an effect callback): the builtin's
    /// sequential runtime body serves it, with the closure taken statically.
    #[test]
    fn unselected_heap_pairings_ride_the_sequential_runtime_body() {
        let render = |callback: &str, shown: &str| crate::pipeline::try_render_rust_source_certified(&format!(r#"
fn noisy(x: Int) -> Result[List[Int], String] = {{ println(int.to_string(x)); ok([x, x]) }}
fn loud(x: Int) -> Result[Int, String] = {{ println(int.to_string(x)); ok(x) }}
effect fn main() -> Unit = {{
  let rows = fan.map([1, 2, 3], (x) => {callback}(x))!
  println("${{{shown}}}")
}}
"#));
        let (rust, certs) = render("noisy", "list.len(rows)").expect("the heap pairing renders");
        let sites = crate::native_hof::sites();
        assert_eq!(sites.values().filter(|s| s.as_str() == "almide_rt_fan_map").count(), 1);
        assert!(sites.values().all(|s| s != "almide_rt_fan_map_par"), "an effect callback is never threaded");
        assert!(rust.contains("almide_rt_fan_map_static("), "the closure is monomorphized, not boxed");
        assert!(certs.iter().any(|(property, _, _)| property == "native-calls"));
        // Negative control: a pairing inside the matrix keeps its linked body.
        render("loud", "rows").expect("the scalar pairing renders");
        assert!(crate::native_hof::sites().values().all(|s| !s.starts_with("almide_rt_fan_map")));
        assert!(crate::native_hof::sequential_map_call("fan.map_x"));
        for other in ["fan.map", "fan.map_is", "fan.any_map_x", "list.map_x"] {
            assert!(!crate::native_hof::sequential_map_call(other), "{other}");
        }
    }
    /// A threaded site offers the builtin's linked scalar body to every
    /// pairing of the render: the sites it refuses ride the sequential
    /// runtime body, each at its own element kinds.
    #[test]
    fn pairings_a_threaded_render_hands_to_the_scalar_link_stay_served() {
        let source = r#"
fn noisy(x: Float) -> Result[List[Float], String] = { println(float.to_string(x)); ok([x, x * 2.0]) }
fn pairs(x: String) -> Result[(Int, String), String] = { println(x); ok((string.len(x), x + "!")) }
effect fn main() -> Unit = {
  let rows = fan.map([1.5, 2.5], (x) => noisy(x))!
  let pure = fan.map([1, 2, 3], (x) => ok([x, x + 1]))!
  let ps = fan.map(["a", "bb"], (x) => pairs(x))!
  println("${rows} ${pure} ${ps}")
}
"#;
        crate::pipeline::try_render_rust_source_certified(source).expect("every pairing renders");
        let sites = crate::native_hof::sites();
        let served = |symbol: &str| sites.values().filter(|s| s.as_str() == symbol).count();
        assert_eq!((served("almide_rt_fan_map_par"), served("almide_rt_fan_map")), (1, 2));
    }
}
