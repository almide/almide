//! Equality over a type that holds a Map, expressed as ordinary typed IR.
//!
//! The slot equality engine compares strings, lists, options, results,
//! records and variants, and a bare Map has its own routes; a Map inside one
//! of the others, or a Map whose values are heap values, had no comparison and
//! the function was refused. Such an `==` becomes a call to a helper written
//! here per compared type: a record or tuple compares field by field, an
//! Option or Result by case, a variant by constructor, a list by position, a
//! map by size and by looking each entry of one side up in the other (keys
//! compare independently of insertion order). A part that holds no Map stays a
//! plain `==`, so the engine decides it exactly as it does anywhere else.
//! Helpers are ordinary functions: their reads, calls and releases pass the
//! same MIR checks as user code.
use almide_ir::{visit_mut::{walk_expr_mut, IrMutVisitor}, *};
use almide_lang::{intern::{sym, Sym}, types::{constructor::TypeConstructorId as TC, Ty}};
use crate::lower::{RecordLayouts, VariantLayouts};
use std::collections::{HashMap, HashSet};

pub(crate) fn expand(ir: &mut IrProgram, records: &RecordLayouts, variants: &VariantLayouts) {
    let mut record_cases = HashSet::new();
    for d in &ir.type_decls {
        let IrTypeDeclKind::Variant { cases, .. } = &d.kind else { continue };
        for c in cases {
            if matches!(c.kind, IrVariantKind::Record { .. }) { record_cases.insert((d.name, c.name)); }
        }
    }
    let mut pass = Helpers {
        vars: &mut ir.var_table, records, variants, record_cases,
        used: ir.functions.iter().map(|f| f.name).collect(),
        helpers: Vec::new(), keys: Vec::new(),
    };
    for f in &mut ir.functions { pass.visit_expr_mut(&mut f.body); }
    for t in &mut ir.top_lets { pass.visit_expr_mut(&mut t.value); }
    ir.functions.extend(pass.helpers);
}

struct Helpers<'a> {
    vars: &'a mut VarTable,
    records: &'a RecordLayouts,
    variants: &'a VariantLayouts,
    record_cases: HashSet<(Sym, Sym)>,
    used: HashSet<Sym>,
    helpers: Vec<IrFunction>,
    keys: Vec<(Ty, Sym)>,
}

impl IrMutVisitor for Helpers<'_> {
    fn visit_expr_mut(&mut self, e: &mut IrExpr) {
        walk_expr_mut(self, e);
        let IrExprKind::BinOp { op, left, right } = &e.kind else { return };
        if !matches!(op, BinOp::Eq | BinOp::Neq) || left.ty != right.ty || !self.wants(&left.ty) { return; }
        // A part with no comparison leaves the original expression intact:
        // generated helpers and variables are committed together.
        let (vars, keys, helpers, used) = (self.vars.entries.len(), self.keys.len(), self.helpers.len(), self.used.clone());
        let Some(mut result) = self.helper_call((**left).clone(), (**right).clone()) else {
            self.vars.entries.truncate(vars);
            self.keys.truncate(keys);
            self.helpers.truncate(helpers);
            self.used = used;
            return;
        };
        if *op == BinOp::Neq { result = eq(result, boolean(false)); }
        result.span = e.span;
        *e = result;
    }
}

impl Helpers<'_> {
    /// A bare Map with a comparison of its own keeps it; anything else that
    /// reaches a Map is compared by a helper.
    fn wants(&self, t: &Ty) -> bool {
        match t {
            Ty::Applied(TC::Map, _) => !routed_map(t),
            _ => self.holds_map(t, &mut Vec::new()),
        }
    }

    fn holds_map(&self, t: &Ty, visiting: &mut Vec<Ty>) -> bool {
        match t {
            Ty::Applied(TC::Map, _) => true,
            Ty::Applied(TC::List | TC::Option | TC::Result, args) => args.iter().any(|t| self.holds_map(t, visiting)),
            Ty::Tuple(ts) => ts.iter().any(|t| self.holds_map(t, visiting)),
            Ty::Record { fields } => fields.iter().any(|(_, t)| self.holds_map(t, visiting)),
            Ty::Named(n, args) => {
                if visiting.contains(t) || visiting.len() >= 64 { return false; }
                visiting.push(t.clone());
                let fields: Vec<Ty> = if let Some((gs, fs)) = self.records.get(n.as_str()) {
                    instantiate(gs, args, fs).unwrap_or_default().into_iter().map(|(_, t)| t).collect()
                } else if let Some(layout) = self.variants.by_type.get(n.as_str()) {
                    layout.cases.iter().flat_map(|c| instantiate(&layout.generics, args, &c.fields).unwrap_or_default())
                        .map(|(_, t)| t).collect()
                } else { Vec::new() };
                let found = fields.iter().any(|t| self.holds_map(t, visiting));
                visiting.pop();
                found
            }
            _ => false,
        }
    }

    /// `left == right`: a plain comparison where no Map is reached.
    fn compare(&mut self, left: IrExpr, right: IrExpr) -> Option<IrExpr> {
        if self.wants(&left.ty) { self.helper_call(left, right) } else { Some(eq(left, right)) }
    }

    fn helper_call(&mut self, left: IrExpr, right: IrExpr) -> Option<IrExpr> {
        let ty = left.ty.clone();
        if let Some((_, name)) = self.keys.iter().find(|(t, _)| *t == ty) {
            return Some(named(*name, left, right));
        }
        // Stops expanding polymorphic recursion; an ordinary recursive type
        // reaches its key above and produces a real recursive call.
        if self.keys.len() >= 256 { return None; }
        let name = self.fresh_name();
        self.keys.push((ty.clone(), name));
        let a = self.vars.alloc_fresh("eq_left", ty.clone(), Mutability::Let, None);
        let b = self.vars.alloc_fresh("eq_right", ty.clone(), Mutability::Let, None);
        let body = self.body(var(a, &ty), var(b, &ty))?;
        let param = |vars: &VarTable, v: VarId| IrParam {
            var: v, ty: ty.clone(), name: vars.get(v).name, borrow: ParamBorrow::Own,
            is_mut: false, open_record: None, default: None, attrs: Vec::new(),
        };
        self.helpers.push(IrFunction {
            name, params: vec![param(self.vars, a), param(self.vars, b)],
            ret_ty: Ty::Bool, body, is_effect: false, is_test: false,
            generics: None, extern_attrs: Vec::new(), export_attrs: Vec::new(),
            attrs: Vec::new(), visibility: IrVisibility::Private, doc: None,
            blank_lines_before: 0, def_id: None, module_origin: None,
            mutated_params: Vec::new(), // fresh-fn: a synthesized comparison that only reads its two operands
        });
        Some(named(name, left, right))
    }

    fn fresh_name(&mut self) -> Sym {
        let mut next = self.used.len();
        loop {
            let name = sym(&format!("__native_eq_{next}"));
            if self.used.insert(name) { return name; }
            next += 1;
        }
    }

    fn body(&mut self, a: IrExpr, b: IrExpr) -> Option<IrExpr> {
        match a.ty.clone() {
            Ty::Applied(TC::Map, args) if args.len() == 2 => self.map(a, b, &args[0], &args[1]),
            Ty::Applied(TC::List, args) if args.len() == 1 => self.list(a, b, &args[0]),
            Ty::Applied(TC::Option, args) if args.len() == 1 => self.carrier(a, b, &args, true),
            Ty::Applied(TC::Result, args) if args.len() == 2 => self.carrier(a, b, &args, false),
            Ty::Tuple(ts) => {
                let mut parts = Vec::new();
                for (i, t) in ts.iter().enumerate() {
                    let at = |o: &IrExpr| node(IrExprKind::TupleIndex { object: Box::new(o.clone()), index: i }, t.clone());
                    parts.push(self.compare(at(&a), at(&b))?);
                }
                Some(all(parts))
            }
            Ty::Record { fields } => self.fields(a, b, fields),
            Ty::Named(n, args) => {
                if let Some((gs, fs)) = self.records.get(n.as_str()).cloned() {
                    return self.fields(a, b, instantiate(&gs, &args, &fs)?);
                }
                let layout = self.variants.by_type.get(n.as_str()).cloned()?;
                let single = layout.cases.len() == 1;
                let mut arms = Vec::new();
                for case in layout.cases {
                    let fields = instantiate(&layout.generics, &args, &case.fields)?;
                    let (left, xs) = self.case_pattern(n, case.ctor, &fields);
                    let (right, ys) = self.case_pattern(n, case.ctor, &fields);
                    let mut parts = Vec::new();
                    for ((x, y), (_, t)) in xs.into_iter().zip(ys).zip(&fields) {
                        parts.push(self.compare(var(x, t), var(y, t))?);
                    }
                    let mut inner = vec![IrMatchArm { pattern: right, guard: None, body: all(parts) }];
                    if !single { inner.push(IrMatchArm { pattern: IrPattern::Wildcard, guard: None, body: boolean(false) }); }
                    let body = node(IrExprKind::Match { subject: Box::new(b.clone()), arms: inner }, Ty::Bool);
                    arms.push(IrMatchArm { pattern: left, guard: None, body });
                }
                Some(node(IrExprKind::Match { subject: Box::new(a), arms }, Ty::Bool))
            }
            _ => None,
        }
    }

    fn fields(&mut self, a: IrExpr, b: IrExpr, fields: Vec<(Sym, Ty)>) -> Option<IrExpr> {
        let mut parts = Vec::new();
        for (field, t) in fields {
            let at = |o: &IrExpr| node(IrExprKind::Member { object: Box::new(o.clone()), field }, t.clone());
            parts.push(self.compare(at(&a), at(&b))?);
        }
        Some(all(parts))
    }

    /// The pattern of constructor `ctor` binding every field, and the binds.
    fn case_pattern(&mut self, n: Sym, ctor: Sym, fields: &[(Sym, Ty)]) -> (IrPattern, Vec<VarId>) {
        let binds: Vec<VarId> = fields.iter()
            .map(|(_, t)| self.vars.alloc_fresh("eq_field", t.clone(), Mutability::Let, None)).collect();
        let bind = |i: usize| IrPattern::Bind { var: binds[i], ty: fields[i].1.clone() };
        let pattern = if self.record_cases.contains(&(n, ctor)) {
            IrPattern::RecordPattern { name: ctor.to_string(), rest: false, fields: fields.iter().enumerate()
                .map(|(i, (f, _))| IrFieldPattern { name: f.to_string(), pattern: Some(bind(i)) }).collect() }
        } else {
            IrPattern::Constructor { name: ctor.to_string(), args: (0..fields.len()).map(bind).collect() }
        };
        (pattern, binds)
    }

    /// Both sides in the same case with equal payloads; `none` equals `none`.
    fn carrier(&mut self, a: IrExpr, b: IrExpr, args: &[Ty], option: bool) -> Option<IrExpr> {
        let wrap = |i: usize, inner: IrPattern| {
            let inner = Box::new(inner);
            if option { IrPattern::Some { inner } } else if i == 0 { IrPattern::Ok { inner } } else { IrPattern::Err { inner } }
        };
        let mut arms = Vec::new();
        for (i, t) in args.iter().enumerate() {
            let x = self.vars.alloc_fresh("eq_payload", t.clone(), Mutability::Let, None);
            let y = self.vars.alloc_fresh("eq_payload", t.clone(), Mutability::Let, None);
            let same = self.compare(var(x, t), var(y, t))?;
            let other = if option { IrPattern::None } else { wrap(1 - i, IrPattern::Wildcard) };
            let inner = vec![
                IrMatchArm { pattern: wrap(i, IrPattern::Bind { var: y, ty: t.clone() }), guard: None, body: same },
                IrMatchArm { pattern: other, guard: None, body: boolean(false) },
            ];
            let body = node(IrExprKind::Match { subject: Box::new(b.clone()), arms: inner }, Ty::Bool);
            arms.push(IrMatchArm { pattern: wrap(i, IrPattern::Bind { var: x, ty: t.clone() }), guard: None, body });
        }
        if option {
            let inner = vec![
                IrMatchArm { pattern: IrPattern::None, guard: None, body: boolean(true) },
                IrMatchArm { pattern: wrap(0, IrPattern::Wildcard), guard: None, body: boolean(false) },
            ];
            let body = node(IrExprKind::Match { subject: Box::new(b), arms: inner }, Ty::Bool);
            arms.push(IrMatchArm { pattern: IrPattern::None, guard: None, body });
        }
        Some(node(IrExprKind::Match { subject: Box::new(a), arms }, Ty::Bool))
    }

    /// Equal lengths, then every position; a mismatch stops comparing.
    fn list(&mut self, a: IrExpr, b: IrExpr, elem: &Ty) -> Option<IrExpr> {
        let equal = self.vars.alloc_fresh("eq_equal", Ty::Bool, Mutability::Var, None);
        let index = self.vars.alloc_fresh("eq_index", Ty::Int, Mutability::Var, None);
        let x = self.vars.alloc_fresh("eq_item", elem.clone(), Mutability::Let, None);
        let y = self.vars.alloc_fresh("eq_item", elem.clone(), Mutability::Let, None);
        let same = self.compare(var(x, elem), var(y, elem))?;
        let other = call("list", "get", vec![b.clone(), var(index, &Ty::Int)], Ty::option(elem.clone()));
        let step = node(IrExprKind::BinOp { op: BinOp::AddInt, left: Box::new(var(index, &Ty::Int)),
            right: Box::new(node(IrExprKind::LitInt { value: 1 }, Ty::Int)) }, Ty::Int);
        let body = vec![
            stmt(IrStmtKind::Assign { var: equal, value: self.keep(equal, other, y, elem, same) }),
            stmt(IrStmtKind::Assign { var: index, value: step }),
        ];
        let lengths = eq(call("list", "len", vec![a.clone()], Ty::Int), call("list", "len", vec![b], Ty::Int));
        Some(node(IrExprKind::Block { stmts: vec![
            bind(equal, lengths, Mutability::Var),
            bind(index, node(IrExprKind::LitInt { value: 0 }, Ty::Int), Mutability::Var),
            stmt(IrStmtKind::Expr { expr: node(IrExprKind::ForIn { var: x, var_tuple: None, iterable: Box::new(a), body }, Ty::Unit) }),
        ], expr: Some(Box::new(var(equal, &Ty::Bool))) }, Ty::Bool))
    }

    /// Equal sizes, then every entry of `a` found in `b` with an equal value.
    fn map(&mut self, a: IrExpr, b: IrExpr, key: &Ty, value: &Ty) -> Option<IrExpr> {
        let pair = Ty::Tuple(vec![key.clone(), value.clone()].into());
        let equal = self.vars.alloc_fresh("eq_equal", Ty::Bool, Mutability::Var, None);
        let entry = self.vars.alloc_fresh("eq_entry", pair.clone(), Mutability::Let, None);
        let found = self.vars.alloc_fresh("eq_found", value.clone(), Mutability::Let, None);
        let field = |index, t: &Ty| node(IrExprKind::TupleIndex { object: Box::new(var(entry, &pair)), index }, t.clone());
        let same = self.compare(field(1, value), var(found, value))?;
        let other = call("map", "get", vec![b.clone(), field(0, key)], Ty::option(value.clone()));
        let body = vec![stmt(IrStmtKind::Assign { var: equal, value: self.keep(equal, other, found, value, same) })];
        let entries = call("map", "to_list", vec![a.clone()], Ty::list(pair));
        let sizes = eq(call("map", "len", vec![a], Ty::Int), call("map", "len", vec![b], Ty::Int));
        Some(node(IrExprKind::Block { stmts: vec![
            bind(equal, sizes, Mutability::Var),
            stmt(IrStmtKind::Expr { expr: node(IrExprKind::ForIn { var: entry, var_tuple: None, iterable: Box::new(entries), body }, Ty::Unit) }),
        ], expr: Some(Box::new(var(equal, &Ty::Bool))) }, Ty::Bool))
    }

    /// `if equal then match lookup { some(found) => same, none => false } else false`
    fn keep(&mut self, equal: VarId, lookup: IrExpr, found: VarId, ty: &Ty, same: IrExpr) -> IrExpr {
        let hit = node(IrExprKind::Match { subject: Box::new(lookup), arms: vec![
            IrMatchArm { pattern: IrPattern::Some { inner: Box::new(IrPattern::Bind { var: found, ty: ty.clone() }) }, guard: None, body: same },
            IrMatchArm { pattern: IrPattern::None, guard: None, body: boolean(false) },
        ] }, Ty::Bool);
        node(IrExprKind::If { cond: Box::new(var(equal, &Ty::Bool)), then: Box::new(hit), else_: Box::new(boolean(false)) }, Ty::Bool)
    }
}

/// A bare Map that already has a comparison: the dynamic-value expansion
/// that runs after this pass, or a runtime call the lowering emits. The
/// runtime call compares a Float value by its bits (`-0.0` differs from `0.0`,
/// a NaN equals itself), so a Map with a Float in its values is compared here,
/// entry by entry, by the declared type.
fn routed_map(t: &Ty) -> bool {
    let Ty::Applied(TC::Map, args) = t else { return false };
    if args.len() != 2 { return false; }
    if crate::lower::is_value_ty(&args[1]) || args.contains(&Ty::Float32) { return true; }
    !float_lane(&args[1]) && crate::lower::LowerCtx::map_set_eq_module_name(t).is_some()
}

fn float_lane(t: &Ty) -> bool {
    match t {
        Ty::Float | Ty::Float64 => true,
        Ty::Applied(_, args) => args.iter().any(float_lane),
        Ty::Tuple(ts) => ts.iter().any(float_lane),
        Ty::Record { fields } => fields.iter().any(|(_, t)| float_lane(t)),
        _ => false,
    }
}

fn instantiate(gs: &[Sym], args: &[Ty], fs: &[(Sym, Ty)]) -> Option<Vec<(Sym, Ty)>> {
    if gs.len() != args.len() { return None; }
    let subst: HashMap<_, _> = gs.iter().copied().zip(args.iter().cloned()).collect();
    Some(fs.iter().map(|(n, t)| (*n, crate::lower::substitute_display_type(t, &subst))).collect())
}

fn node(kind: IrExprKind, ty: Ty) -> IrExpr { IrExpr { kind, ty, span: None, def_id: None } }
fn var(id: VarId, ty: &Ty) -> IrExpr { node(IrExprKind::Var { id }, ty.clone()) }
fn boolean(value: bool) -> IrExpr { node(IrExprKind::LitBool { value }, Ty::Bool) }
fn stmt(kind: IrStmtKind) -> IrStmt { IrStmt { kind, span: None } }
fn bind(var: VarId, value: IrExpr, mutability: Mutability) -> IrStmt {
    stmt(IrStmtKind::Bind { var, mutability, ty: value.ty.clone(), value })
}
fn eq(left: IrExpr, right: IrExpr) -> IrExpr {
    node(IrExprKind::BinOp { op: BinOp::Eq, left: Box::new(left), right: Box::new(right) }, Ty::Bool)
}
/// Every part holds; no parts is `true`.
fn all(parts: Vec<IrExpr>) -> IrExpr {
    let mut parts = parts.into_iter();
    let Some(first) = parts.next() else { return boolean(true) };
    parts.fold(first, |a, b| node(IrExprKind::BinOp { op: BinOp::And, left: Box::new(a), right: Box::new(b) }, Ty::Bool))
}
fn call(module: &str, func: &str, args: Vec<IrExpr>, ty: Ty) -> IrExpr {
    node(IrExprKind::Call { target: CallTarget::Module { module: sym(module), func: sym(func), def_id: None }, args, type_args: Vec::new() }, ty)
}
fn named(name: Sym, left: IrExpr, right: IrExpr) -> IrExpr {
    node(IrExprKind::Call { target: CallTarget::Named { name }, args: vec![left, right], type_args: Vec::new() }, Ty::Bool)
}
