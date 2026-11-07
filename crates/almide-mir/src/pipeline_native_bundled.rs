//! Link reached primary stdlib bodies into the native proof pipeline.
//!
//! These are ordinary checked Almide functions, not additions to the trusted
//! native runtime floor. Keep module calls through monomorphization, then
//! unify VarIds and namespace definitions before the shared MIR preparation.

use super::{LowerError, Checker};
use almide_ir::{CallTarget, IrExpr, IrExprKind, IrProgram, IrVisitor, IrMutVisitor};
use almide_lang::{ast::{Decl, Program}, intern::sym};
use std::collections::{BTreeMap, HashMap, HashSet};

fn body_needed(module: &str, func: &str) -> bool {
    let call = format!("{module}.{}", almide_ir::mono_base_or_self(func));
    !super::served_natively(&call)
        && !super::registry_served_names().contains(call.as_str())
        && almide_ir::mut_args::stdlib_fn_has_almide_body(module, func)
}

#[derive(Default)]
struct Demands(BTreeMap<String, HashSet<String>>, BTreeMap<String, Option<almide_ir::Span>>);

fn defaults(types: &mut [almide_ir::IrTypeDecl], mut visit: impl FnMut(&mut IrExpr)) {
    use almide_ir::{IrTypeDeclKind, IrVariantKind};
    for ty in types {
        let fields: Vec<_> = match &mut ty.kind {
            IrTypeDeclKind::Record { fields } => fields.iter_mut().collect(),
            IrTypeDeclKind::Variant { cases, .. } => cases.iter_mut().flat_map(|c| match &mut c.kind {
                IrVariantKind::Record { fields } => fields.iter_mut().collect::<Vec<_>>(),
                _ => Vec::new(),
            }).collect(),
            _ => Vec::new(),
        };
        for field in fields {
            if let Some(e) = &mut field.default { visit(e); }
        }
    }
}

impl Demands {
    fn add(&mut self, module: &str, func: &str, span: Option<almide_ir::Span>) {
        if body_needed(module, func) {
            self.1.entry(module.into()).or_insert(span);
            self.0.entry(module.into()).or_default()
                .insert(almide_ir::mono_base_or_self(func).into());
        }
    }
}

impl IrVisitor for Demands {
    fn visit_expr(&mut self, e: &IrExpr) {
        match &e.kind {
            IrExprKind::Call { target: CallTarget::Module { module, func, .. }, .. } => {
                self.add(module.as_str(), func.as_str(), e.span);
            }
            IrExprKind::FnRef { name } => {
                if let Some((m, f)) = name.as_str().rsplit_once('.') { self.add(m, f, e.span); }
            }
            _ => {}
        }
        almide_ir::walk_expr(self, e);
    }
}

fn errors_since(checker: &Checker, start: usize, module: &str) -> Result<(), LowerError> {
    let errors: Vec<_> = checker.diagnostics[start..].iter()
        .filter(|d| d.level == almide_frontend::diagnostic::Level::Error)
        .map(|d| d.message.as_str()).collect();
    if errors.is_empty() { return Ok(()); }
    let span = checker.diagnostics[start..].iter()
        .find(|d| d.level == almide_frontend::diagnostic::Level::Error)
        .and_then(|d| Some(almide_ir::Span { line: d.line?, col: d.col?, end_col: d.end_col.unwrap_or(d.col?) }));
    Err(LowerError::at(span, format!("native: checked module {module}: {errors:?}")))
}

fn lower_module(checker: &mut Checker, name: &str, mut program: Program) -> Result<almide_ir::IrModule, LowerError> {
    // A primary stdlib source may qualify calls through its own public
    // module name without importing itself. Give checking and lowering the
    // same explicit scope; no diagnostic is discarded for being bundled.
    if !program.imports.iter().any(|d| matches!(d, Decl::Import { path, alias: None, .. }
        if path.len() == 1 && path[0].as_str() == name)) {
        program.imports.push(Decl::Import { path: vec![sym(name)], names: None, alias: None, span: None });
    }
    let start = checker.diagnostics.len();
    almide_frontend::canonicalize::register_module(
        &mut checker.env, &mut checker.diagnostics, name, &program, false,
    );
    errors_since(checker, start, name)?;
    let saved_bundled = checker.in_bundled_module;
    checker.in_bundled_module = true;
    checker.infer_module(&mut program, name);
    checker.in_bundled_module = saved_bundled;
    errors_since(checker, start, name)?;
    let (table, _) = almide_frontend::import_table::build_import_table(
        &program, Some(name), &checker.env.user_modules,
    );
    let saved = std::mem::replace(&mut checker.env.import_table, table);
    let module = almide_frontend::lower::lower_module(
        name, &program, &checker.env, &checker.type_map, None,
    );
    checker.env.import_table = saved;
    Ok(module)
}

/// Load only bodies demanded by typed calls. A growing demand rechecks that
/// module from its source; dependencies discovered in its IR join the next
/// round. Intrinsics stay declared for checking, but never become proof bodies.
fn load(ir: &mut IrProgram, checker: &mut Checker, extra: &Demands) -> Result<(), LowerError> {
    let mut loaded: BTreeMap<String, HashSet<String>> = ir.modules.iter().map(|m| (
        m.name.to_string(), m.functions.iter().map(|f| f.name.to_string()).collect(),
    )).collect();
    let mut sources: BTreeMap<String, Program> = BTreeMap::new();
    loop {
        let mut demands = Demands::default();
        demands.1 = extra.1.clone();
        for (m, fs) in &extra.0 { demands.0.entry(m.clone()).or_default().extend(fs.iter().cloned()); }
        for f in ir.functions.iter().chain(ir.modules.iter().flat_map(|m| &m.functions)) {
            demands.visit_expr(&f.body);
        }
        for t in ir.top_lets.iter().chain(ir.modules.iter().flat_map(|m| &m.top_lets)) {
            demands.visit_expr(&t.value);
        }
        defaults(&mut ir.type_decls, |e| demands.visit_expr(e));
        for m in &mut ir.modules { defaults(&mut m.type_decls, |e| demands.visit_expr(e)); }
        let mut changed = false;
        for (name, roots) in demands.0 {
            if loaded.get(&name).is_some_and(|have| roots.is_subset(have)) { continue; }
            let roots = {
                let have = loaded.entry(name.clone()).or_default();
                have.extend(roots);
                have.clone()
            };
            if !sources.contains_key(&name) {
                let src = almide_lang::stdlib_info::bundled_source(&name)
                    .ok_or_else(|| LowerError::at(demands.1.get(&name).copied().flatten(), format!("native: missing module {name}")))?;
                // The recovery parser may return a partial AST. Reject every
                // diagnostic just as for user source, before reach pruning.
                sources.insert(name.clone(), super::parse_or_wall(src)?);
            }
            let source = &sources[&name];
            let mut required = roots;
            required.extend(almide_frontend::bundled_reach::compiler_called_fns(&name, source));
            let live = almide_frontend::bundled_reach::reach_within(&name, source, &required);
            let mut program = source.clone();
            program.decls.retain(|d| match d {
                Decl::Fn { name, .. } => live.contains(name.as_str()),
                Decl::Test { .. } | Decl::TestWhereDef { .. } => false,
                _ => true,
            });
            let module = lower_module(checker, &name, program)?;
            if let Some(old) = ir.modules.iter_mut().find(|m| m.name.as_str() == name) {
                *old = module;
            } else {
                ir.modules.push(module);
            }
            changed = true;
        }
        if !changed { return Ok(()); }
    }
}

/// Specialization can reveal a protocol implementation not named before
/// mono (`T.__int_exact` becomes `float.__int_exact`). Discover those calls
/// on the linked copy, load their checked source into the pre-link IR, and
/// replay the canonical driver. Never optimize an already optimized body a
/// second time, and never replace a specialized function with its template.
pub(super) fn link(ir: &mut IrProgram, checker: &mut Checker) -> Result<(), LowerError> {
    let mut extra = Demands::default();
    loop {
        load(ir, checker, &extra)?;
        let mut linked = ir.clone();
        almide_driver::link_ir(&mut linked);
        let mut demands = Demands::default();
        for f in linked.functions.iter().chain(linked.modules.iter().flat_map(|m| &m.functions)) {
            demands.visit_expr(&f.body);
        }
        for t in linked.top_lets.iter().chain(linked.modules.iter().flat_map(|m| &m.top_lets)) {
            demands.visit_expr(&t.value);
        }
        let mut changed = false;
        for (m, fs) in demands.0 {
            for f in fs {
                let present = ir.modules.iter().any(|module| module.name.as_str() == m
                    && module.functions.iter().any(|func| func.name.as_str() == f));
                if !present && extra.0.entry(m.clone()).or_default().insert(f) {
                    extra.1.entry(m.clone()).or_insert(demands.1.get(&m).copied().flatten());
                    changed = true;
                }
            }
        }
        if !changed {
            *ir = linked;
            return Ok(());
        }
    }
}

/// Native runtime mutators need an owned local, never a mutable reference to
/// the caller's borrowed parameter. Bind a real copy before any branch and
/// leave the existing move-mode return/write-back and MIR ownership checks
/// to account for it. The move-parameter pass may transfer ownership instead
/// when every call site can prove that transfer.
pub(super) fn own_written_params(ir: &mut IrProgram) {
    use almide_ir::{IrStmt, IrStmtKind, Mutability, VarId};
    struct Writes(HashSet<VarId>);
    impl IrVisitor for Writes {
        fn visit_expr(&mut self, e: &IrExpr) {
            if let IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. } = &e.kind {
                for k in almide_ir::mut_args::stdlib_mut_positions(module.as_str(), func.as_str()).into_iter().flatten() {
                    if let Some(v) = args.get(k).and_then(almide_ir::mut_args::place_root) { self.0.insert(v); }
                }
            }
            almide_ir::walk_expr(self, e);
        }
    }
    for f in &mut ir.functions {
        let mut writes = Writes(HashSet::new());
        writes.visit_expr(&f.body);
        let mut bindings = Vec::new();
        for p in &mut f.params {
            if !writes.0.contains(&p.var) || !crate::lower::is_heap_ty(&p.ty) { continue; }
            let local = p.var;
            ir.var_table.entries[local.0 as usize].mutability = Mutability::Var;
            p.var = ir.var_table.alloc(p.name, p.ty.clone(), Mutability::Let, None);
            bindings.push(IrStmt {
                kind: IrStmtKind::Bind {
                    var: local,
                    mutability: Mutability::Var,
                    ty: p.ty.clone(),
                    value: IrExpr { kind: IrExprKind::Var { id: p.var }, ty: p.ty.clone(), span: None, def_id: None },
                },
                span: None,
            });
        }
        if !bindings.is_empty() {
            let old = std::mem::take(&mut f.body);
            f.body = IrExpr { ty: old.ty.clone(), span: old.span, def_id: old.def_id,
                kind: IrExprKind::Block { stmts: bindings, expr: Some(Box::new(old)) } };
        }
    }
}

/// Snapshot a read that aliases a runtime mutator's place before taking the
/// mutable borrow. These are IR bindings, so the real MIR Dup and release
/// appear in ownership witnesses. Evaluate all non-place operands in their
/// source order, then resolve the current mut places (C-132 / C-258).
pub(super) fn snapshot_mut_reads(ir: &mut IrProgram) {
    use almide_ir::{IrStmt, IrStmtKind, Mutability, VarTable};
    struct Snapshot<'a>(&'a mut VarTable);
    impl IrMutVisitor for Snapshot<'_> {
        fn visit_expr_mut(&mut self, e: &mut IrExpr) {
            almide_ir::walk_expr_mut(self, e);
            let IrExprKind::Call { target: CallTarget::Module { module, func, .. }, args, .. } = &mut e.kind else { return };
            let Some(positions) = almide_ir::mut_args::stdlib_mut_positions(module.as_str(), func.as_str()) else { return };
            let roots: HashSet<_> = positions.iter().filter_map(|&i| args.get(i))
                .filter_map(almide_ir::call_writes::place_root).collect();
            let conflict = args.iter().enumerate().any(|(i, a)| !positions.contains(&i)
                && almide_ir::call_writes::place_root(a).is_some_and(|v| roots.contains(&v)));
            if !conflict { return; }
            let mut binds = Vec::new();
            for (i, a) in args.iter_mut().enumerate() {
                if positions.contains(&i) { continue; }
                let (ty, span) = (a.ty.clone(), a.span);
                let id = self.0.alloc_fresh("__native_read_before_mut", ty.clone(), Mutability::Let, span);
                let value = std::mem::replace(a, IrExpr { kind: IrExprKind::Var { id }, ty: ty.clone(), span, def_id: None });
                binds.push(IrStmt { kind: IrStmtKind::Bind { var: id, mutability: Mutability::Let, ty, value }, span });
            }
            let old = std::mem::take(e);
            *e = IrExpr { ty: old.ty.clone(), span: old.span, def_id: old.def_id,
                kind: IrExprKind::Block { stmts: binds, expr: Some(Box::new(old)) } };
        }
    }
    let mut visitor = Snapshot(&mut ir.var_table);
    for f in &mut ir.functions { visitor.visit_expr_mut(&mut f.body); }
    for t in &mut ir.top_lets { visitor.visit_expr_mut(&mut t.value); }
    defaults(&mut ir.type_decls, |e| visitor.visit_expr_mut(e));
}

struct Resolve<'a> {
    bodies: &'a HashMap<String, HashSet<String>>,
    owner: Option<&'a str>,
}

impl Resolve<'_> {
    fn qualified(&self, m: &str, f: &str) -> Option<String> {
        self.bodies.get(m)?.contains(f).then(|| super::user_module_fn_name(m, f))
    }
    fn reference(&self, name: &str) -> Option<String> {
        if let Some(m) = self.owner {
            if let Some(n) = self.qualified(m, name) { return Some(n); }
        }
        let (m, f) = name.rsplit_once('.')?;
        self.qualified(m, f)
    }
}

impl IrMutVisitor for Resolve<'_> {
    fn visit_expr_mut(&mut self, e: &mut IrExpr) {
        almide_ir::walk_expr_mut(self, e);
        match &mut e.kind {
            IrExprKind::Call { target, .. } => {
                let resolved = match target {
                    CallTarget::Module { module, func, .. } => self.qualified(module.as_str(), func.as_str()),
                    CallTarget::Named { name } => self.reference(name.as_str()),
                    _ => None,
                };
                if let Some(name) = resolved { *target = CallTarget::Named { name: sym(&name) }; }
            }
            IrExprKind::FnRef { name } | IrExprKind::ClosureCreate { func_name: name, .. } => {
                if let Some(n) = self.reference(name.as_str()) { *name = sym(&n); }
            }
            _ => {}
        }
    }
}

#[derive(Default)]
struct References(HashSet<String>);
impl IrVisitor for References {
    fn visit_expr(&mut self, e: &IrExpr) {
        match &e.kind {
            IrExprKind::Call { target: CallTarget::Named { name }, .. }
            | IrExprKind::FnRef { name }
            | IrExprKind::ClosureCreate { func_name: name, .. } => { self.0.insert(name.to_string()); }
            _ => {}
        }
        almide_ir::walk_expr(self, e);
    }
}

/// All user functions remain roots, even unused ones. Only unreached library
/// definitions/templates are pruned. Every retained body takes the same native
/// signature, MIR ownership and artifact checks as a user function.
pub(super) fn flatten(mut ir: IrProgram) -> Result<IrProgram, LowerError> {
    if ir.modules.is_empty() { return Ok(ir); }
    let bodies: HashMap<String, HashSet<String>> = ir.modules.iter().map(|m| {
        let names = super::linkable_module_fns(m).into_iter()
            .filter(|f| {
                let call = format!("{}.{}", m.name, almide_ir::mono_base_or_self(f));
                !super::served_natively(&call) && !super::registry_served_names().contains(call.as_str())
            })
            .collect();
        (m.name.to_string(), names)
    }).collect();
    let roots: HashSet<String> = ir.functions.iter().map(|f| f.name.to_string()).collect();
    let mut resolve = Resolve { bodies: &bodies, owner: None };
    for f in &mut ir.functions { resolve.visit_expr_mut(&mut f.body); }
    for t in &mut ir.top_lets { resolve.visit_expr_mut(&mut t.value); }
    defaults(&mut ir.type_decls, |e| resolve.visit_expr_mut(e));
    for m in &mut ir.modules {
        resolve.owner = Some(m.name.as_str());
        m.functions.retain(|f| bodies[m.name.as_str()].contains(f.name.as_str()));
        for f in &mut m.functions { resolve.visit_expr_mut(&mut f.body); }
        for t in &mut m.top_lets { resolve.visit_expr_mut(&mut t.value); }
        defaults(&mut m.type_decls, |e| resolve.visit_expr_mut(e));
    }
    use almide_codegen::pass::{NanoPass, Target};
    ir = almide_codegen::pass_unify_var_tables::UnifyVarTablesPass.run(ir, Target::Rust).program;
    almide_frontend::ir_link::ir_link_flatten(&mut ir);
    let mut names = HashSet::new();
    for f in &ir.functions {
        if !names.insert(f.name) {
            return Err(LowerError::at(f.body.span, format!("native: linked function name collision: {}", f.name)));
        }
    }
    let mut refs = References(roots);
    for t in &ir.top_lets { refs.visit_expr(&t.value); }
    defaults(&mut ir.type_decls, |e| refs.visit_expr(e));
    let mut visited = HashSet::new();
    loop {
        let mut changed = false;
        for f in &ir.functions {
            if refs.0.contains(f.name.as_str()) && visited.insert(f.name) {
                refs.visit_expr(&f.body);
                changed = true;
            }
        }
        if !changed { break; }
    }
    ir.functions.retain(|f| refs.0.contains(f.name.as_str()));
    Ok(ir)
}
