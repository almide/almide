//! Typed display helpers for the native MIR path. Helpers are ordinary IR:
//! their calls, borrows and releases pass through the same MIR checks as user
//! functions. No formatter name grants a proof exemption or a v0 route.
use almide_ir::{visit_mut::{walk_expr_mut, IrMutVisitor}, *};
use almide_lang::{intern::{sym, Sym}, types::{constructor::TypeConstructorId as TC, Ty}};
use crate::lower::{RecordLayouts, VariantLayouts};
use std::collections::{HashMap, HashSet};

pub(crate) fn expand(ir: &mut IrProgram, records: &RecordLayouts, variants: &VariantLayouts) {
    let shapes = almide_ir::record_shape::RecordShapeIndex::build(ir, |d| d.declared_name().to_owned());
    let mut record_cases = HashSet::new();
    for d in &ir.type_decls {
        if let IrTypeDeclKind::Variant { cases, .. } = &d.kind {
            for c in cases {
                if matches!(c.kind, IrVariantKind::Record { .. }) { record_cases.insert((d.name, c.name)); }
            }
        }
    }
    let mut pass = Display {
        vars: &mut ir.var_table, records, variants,
        shapes, record_cases,
        used: ir.functions.iter().map(|f| f.name).collect(),
        helpers: Vec::new(), keys: Vec::new(),
    };
    for f in &mut ir.functions { pass.visit_expr_mut(&mut f.body); }
    for t in &mut ir.top_lets { pass.visit_expr_mut(&mut t.value); }
    ir.functions.extend(pass.helpers);
}

struct Display<'a> {
    vars: &'a mut VarTable,
    records: &'a RecordLayouts,
    variants: &'a VariantLayouts,
    shapes: almide_ir::record_shape::RecordShapeIndex,
    record_cases: HashSet<(Sym, Sym)>,
    used: HashSet<Sym>,
    helpers: Vec<IrFunction>,
    keys: Vec<(Ty, Sym)>,
}

fn structural(t: &Ty) -> bool {
    match t {
        Ty::Named(..) | Ty::Record { .. } | Ty::Tuple(_) => true,
        Ty::Applied(TC::Option | TC::Result | TC::Map | TC::Set, _) => true,
        Ty::Applied(TC::List, args) => args.iter().any(|t| structural(t) || matches!(t, Ty::Applied(..))),
        _ => false,
    }
}

impl IrMutVisitor for Display<'_> {
    fn visit_expr_mut(&mut self, e: &mut IrExpr) {
        walk_expr_mut(self, e);
        let IrExprKind::StringInterp { parts } = &mut e.kind else { return };
        for part in parts {
            let IrStringPart::Expr { expr } = part else { continue };
            if !structural(&expr.ty) { continue; }
            // Unsupported fields must leave the original expression intact.
            // Generated helpers and variables are committed together.
            let vars = self.vars.entries.len();
            let keys = self.keys.len();
            let helpers = self.helpers.len();
            if let Some(value) = self.format(expr.clone()) {
                *expr = value;
            } else {
                self.vars.entries.truncate(vars);
                self.keys.truncate(keys);
                self.helpers.truncate(helpers);
            }
        }
    }
}

impl Display<'_> {
    fn format(&mut self, value: IrExpr) -> Option<IrExpr> {
        if let Some((module, func)) = scalar(&value.ty) {
            return Some(call(module, func, value, Ty::String));
        }
        if value.ty == Ty::Unit { return Some(text("()")); }
        if let Some((_, name)) = self.keys.iter().find(|(t, _)| *t == value.ty) {
            return Some(named(*name, value));
        }
        // Stops expanding polymorphic recursion; ordinary recursive types
        // reach their existing key above and produce a real recursive call.
        if self.keys.len() >= 256 { return None; }
        let name = self.fresh_name();
        let ty = value.ty.clone();
        self.keys.push((ty.clone(), name));
        let var = self.vars.alloc_fresh("display", ty.clone(), Mutability::Let, None);
        let body = self.body(var_expr(var, &ty))?;
        self.push_helper(name, var, ty, body);
        Some(named(name, value))
    }

    fn fresh_name(&mut self) -> Sym {
        let mut next = self.used.len();
        loop {
            let name = sym(&format!("__native_display_{next}"));
            if self.used.insert(name) { return name; }
            next += 1;
        }
    }

    fn push_helper(&mut self, name: Sym, var: VarId, ty: Ty, body: IrExpr) {
        self.helpers.push(IrFunction {
            name, params: vec![IrParam {
                var, ty, name: self.vars.get(var).name, borrow: ParamBorrow::Own,
                is_mut: false, open_record: None, default: None, attrs: Vec::new(),
            }],
            ret_ty: Ty::String, body, is_effect: false, is_test: false,
            generics: None, extern_attrs: Vec::new(), export_attrs: Vec::new(),
            attrs: Vec::new(), visibility: IrVisibility::Private, doc: None,
            blank_lines_before: 0, def_id: None, mutated_params: Vec::new(), module_origin: None,
        });
    }

    fn body(&mut self, obj: IrExpr) -> Option<IrExpr> {
        if crate::lower::is_value_ty(&obj.ty) {
            return Some(call("json", "stringify", obj, Ty::String));
        }
        match obj.ty.clone() {
            Ty::Bytes => self.format(call("bytes", "to_list", obj, Ty::list(Ty::Int))),
            Ty::Tuple(ts) => {
                let mut parts = vec![text("(")];
                for (i, t) in ts.iter().enumerate() {
                    if i > 0 { parts.push(text(", ")); }
                    let field = node(IrExprKind::TupleIndex { object: Box::new(obj.clone()), index: i }, t.clone());
                    parts.push(self.format(field)?);
                }
                parts.push(text(")"));
                Some(concat(parts))
            }
            Ty::Record { mut fields } => {
                if let Some((shape, _)) = self.shapes.lookup(&fields) {
                    let name = shape.label.clone();
                    let ordered = shape.fields.iter().map(|(n, _)|
                        fields.iter().find(|(f, _)| f == n).cloned()).collect::<Option<Vec<_>>>()?;
                    return self.record(obj, &name, ordered);
                }
                fields.sort_by(|a, b| a.0.as_str().cmp(b.0.as_str()));
                self.record(obj, "", fields)
            }
            Ty::Named(n, args) => {
                if let Some((gs, fs)) = self.records.get(n.as_str()).cloned() {
                    let fields = instantiate(&gs, &args, &fs)?;
                    self.record(obj, almide_ir::declared_type_name(n.as_str()), fields)
                } else if let Some(layout) = self.variants.by_type.get(n.as_str()).cloned() {
                    let mut arms = Vec::new();
                    for case in layout.cases {
                        let fields = instantiate(&layout.generics, &args, &case.fields)?;
                        let record = self.record_cases.contains(&(n, case.ctor));
                        let mut patterns = Vec::new();
                        let mut parts = vec![text(case.ctor.as_str())];
                        if !fields.is_empty() { parts.push(text(if record { " { " } else { "(" })); }
                        for (i, (field, ty)) in fields.iter().enumerate() {
                            if i > 0 { parts.push(text(", ")); }
                            if record { parts.push(text(&format!("{field}: "))); }
                            let v = self.vars.alloc_fresh("display_field", ty.clone(), Mutability::Let, None);
                            let p = IrPattern::Bind { var: v, ty: ty.clone() };
                            patterns.push((*field, p));
                            parts.push(self.format(var_expr(v, ty))?);
                        }
                        if !fields.is_empty() { parts.push(text(if record { " }" } else { ")" })); }
                        let pattern = if record {
                            IrPattern::RecordPattern { name: case.ctor.to_string(), fields: patterns.into_iter()
                                .map(|(n, p)| IrFieldPattern { name: n.to_string(), pattern: Some(p) }).collect(), rest: false }
                        } else {
                            IrPattern::Constructor { name: case.ctor.to_string(), args: patterns.into_iter().map(|(_, p)| p).collect() }
                        };
                        arms.push(IrMatchArm { pattern, guard: None, body: concat(parts) });
                    }
                    Some(node(IrExprKind::Match { subject: Box::new(obj), arms }, Ty::String))
                } else {
                    let surface = almide_lang::stdlib_info::runtime_backed_repr_surface(n.as_str())?;
                    let (module, func) = surface.rsplit_once('.')?;
                    Some(call(module, func, obj, Ty::String))
                }
            }
            Ty::Applied(TC::List, args) if args.len() == 1 => self.sequence(obj, &args[0], None),
            Ty::Applied(TC::Option, args) if args.len() == 1 => self.carrier(obj, &args, true),
            Ty::Applied(TC::Result, args) if args.len() == 2 => self.carrier(obj, &args, false),
            Ty::Applied(TC::Set, args) if args.len() == 1 => {
                let xs = call("set", "to_list", obj, Ty::Applied(TC::List, args.clone()));
                Some(concat(vec![text("set.from_list("), self.format(xs)?, text(")")]))
            }
            Ty::Applied(TC::Map, args) if args.len() == 2 => self.map(obj, &args),
            _ => None,
        }
    }

    fn map(&mut self, obj: IrExpr, args: &[Ty]) -> Option<IrExpr> {
        let pair = Ty::Tuple(args.to_vec().into());
        let var = self.vars.alloc_fresh("display_entry", pair.clone(), Mutability::Let, None);
        let mut parts = Vec::new();
        for (i, ty) in args.iter().enumerate() {
            if i > 0 { parts.push(text(": ")); }
            parts.push(self.format(node(IrExprKind::TupleIndex {
                object: Box::new(var_expr(var, &pair)), index: i,
            }, ty.clone()))?);
        }
        let name = self.fresh_name();
        self.push_helper(name, var, pair.clone(), concat(parts));
        let entries = call("map", "to_list", obj, Ty::list(pair.clone()));
        self.sequence(entries, &pair, Some(name))
    }

    fn record(&mut self, obj: IrExpr, name: &str, fields: Vec<(Sym, Ty)>) -> Option<IrExpr> {
        let mut parts = vec![text(&if name.is_empty() { "{ ".into() } else { format!("{name} {{ ") })];
        for (i, (field, ty)) in fields.into_iter().enumerate() {
            if i > 0 { parts.push(text(", ")); }
            parts.push(text(&format!("{field}: ")));
            let access = node(IrExprKind::Member { object: Box::new(obj.clone()), field }, ty);
            parts.push(self.format(access)?);
        }
        parts.push(text(" }"));
        Some(concat(parts))
    }

    fn carrier(&mut self, obj: IrExpr, args: &[Ty], option: bool) -> Option<IrExpr> {
        let mut arms = Vec::new();
        for (i, ty) in args.iter().enumerate() {
            let v = self.vars.alloc_fresh("display_payload", ty.clone(), Mutability::Let, None);
            let inner = Box::new(IrPattern::Bind { var: v, ty: ty.clone() });
            let (label, pattern) = if option { ("some(", IrPattern::Some { inner }) }
                else if i == 0 { ("ok(", IrPattern::Ok { inner }) }
                else { ("err(", IrPattern::Err { inner }) };
            let body = concat(vec![text(label), self.format(var_expr(v, ty))?, text(")")]);
            arms.push(IrMatchArm { pattern, guard: None, body });
        }
        if option { arms.push(IrMatchArm { pattern: IrPattern::None, guard: None, body: text("none") }); }
        Some(node(IrExprKind::Match { subject: Box::new(obj), arms }, Ty::String))
    }

    fn sequence(&mut self, obj: IrExpr, elem: &Ty, entry: Option<Sym>) -> Option<IrExpr> {
        let out = self.vars.alloc_fresh("display_out", Ty::String, Mutability::Var, None);
        let first = self.vars.alloc_fresh("display_first", Ty::Bool, Mutability::Var, None);
        let v = self.vars.alloc_fresh("display_item", elem.clone(), Mutability::Let, None);
        let sep = node(IrExprKind::If {
            cond: Box::new(var_expr(first, &Ty::Bool)), then: Box::new(text("")), else_: Box::new(text(", ")),
        }, Ty::String);
        let item = if let Some(name) = entry { named(name, var_expr(v, elem)) }
            else { self.format(var_expr(v, elem))? };
        let next = concat(vec![var_expr(out, &Ty::String), sep, item]);
        let body = vec![stmt(IrStmtKind::Assign { var: out, value: next }),
            stmt(IrStmtKind::Assign { var: first, value: node(IrExprKind::LitBool { value: false }, Ty::Bool) })];
        let stmts = vec![
            stmt(IrStmtKind::Bind { var: out, mutability: Mutability::Var, ty: Ty::String, value: text("[") }),
            stmt(IrStmtKind::Bind { var: first, mutability: Mutability::Var, ty: Ty::Bool,
                value: node(IrExprKind::LitBool { value: true }, Ty::Bool) }),
            stmt(IrStmtKind::Expr { expr: node(IrExprKind::ForIn { var: v, var_tuple: None, iterable: Box::new(obj), body }, Ty::Unit) }),
        ];
        let close = if entry.is_some() { node(IrExprKind::If {
            cond: Box::new(var_expr(first, &Ty::Bool)), then: Box::new(text(":]")), else_: Box::new(text("]")),
        }, Ty::String) } else { text("]") };
        Some(node(IrExprKind::Block { stmts,
            expr: Some(Box::new(concat(vec![var_expr(out, &Ty::String), close]))),
        }, Ty::String))
    }
}

fn instantiate(gs: &[Sym], args: &[Ty], fs: &[(Sym, Ty)]) -> Option<Vec<(Sym, Ty)>> {
    if gs.len() != args.len() { return None; }
    let subst: HashMap<_, _> = gs.iter().copied().zip(args.iter().cloned()).collect();
    Some(fs.iter().map(|(n, t)| (*n, crate::lower::substitute_display_type(t, &subst))).collect())
}

fn scalar(t: &Ty) -> Option<(&'static str, &'static str)> {
    Some(match t {
        Ty::Int | Ty::Int8 | Ty::Int16 | Ty::Int32 | Ty::Int64 | Ty::UInt8 | Ty::UInt16 | Ty::UInt32 => ("int", "to_string"),
        Ty::UInt64 => ("uint64", "to_string"),
        Ty::Bool => ("bool", "to_string"),
        Ty::Float | Ty::Float64 => ("float", "to_string_compound"),
        Ty::Float32 => ("float32", "to_string_compound"),
        Ty::String => ("string", "quote"),
        _ => return None,
    })
}
fn node(kind: IrExprKind, ty: Ty) -> IrExpr { IrExpr { kind, ty, span: None, def_id: None } }
fn text(s: &str) -> IrExpr { node(IrExprKind::LitStr { value: s.to_owned() }, Ty::String) }
fn var_expr(id: VarId, ty: &Ty) -> IrExpr { node(IrExprKind::Var { id }, ty.clone()) }
fn stmt(kind: IrStmtKind) -> IrStmt { IrStmt { kind, span: None } }
fn call(module: &str, func: &str, value: IrExpr, ty: Ty) -> IrExpr {
    node(IrExprKind::Call { target: CallTarget::Module { module: sym(module), func: sym(func), def_id: None }, args: vec![value], type_args: Vec::new() }, ty)
}
fn named(name: Sym, value: IrExpr) -> IrExpr {
    node(IrExprKind::Call { target: CallTarget::Named { name }, args: vec![value], type_args: Vec::new() }, Ty::String)
}
fn concat(parts: Vec<IrExpr>) -> IrExpr {
    parts.into_iter().fold(text(""), |a, b| node(IrExprKind::BinOp { op: BinOp::ConcatStr, left: Box::new(a), right: Box::new(b) }, Ty::String))
}
