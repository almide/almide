//! What KIND of callee a call the program does not define is (decision row
//! effect.flow-categories). A call the scan cannot resolve to a scanned
//! function used to charge nothing; each kind now has a rule that derives its
//! categories from what it really runs, and anything no rule answers for is
//! UNKNOWN and charged every category (⊤), as an `@extern` is:
//!
//! - an output or abort builtin (`println`, `panic`): its D2 leaves;
//! - a runtime symbol whose head names a classified module: its declaring
//!   function's categories (`almide_ir::effect::runtime_symbol_effect`);
//! - a FUNCTION OF THE PROGRAM spelled another way than the scan's resolver
//!   keys it (a convention method of another module's type, `Order.total`;
//!   a derived helper named for a module's type) — matched the way the later
//!   passes link it ([`Callees::program_fns`]): a call of each;
//! - a CONSTRUCTOR — a variant case of the type the call builds, or the
//!   type's own name (an opaque or newtype wrapper): it runs nothing, it
//!   builds a value from arguments the scan already walked;
//! - a PROTOCOL DISPATCH — a requirement called through a type variable of
//!   the enclosing generic (`T.__join(xs, sep)`, `h.__self_read(n)` on
//!   `h: H`): it runs the conformer the instantiation picks, so it is charged
//!   with the conformer of EVERY concrete type the program holds a value of
//!   ([`Callees::conformers`]), resolved the way monomorphisation resolves it
//!   (a built-in or stdlib-owned type: its module's requirement function; a
//!   user type: its convention method);
//! - a DERIVED HELPER — a compiler-generated call the self-host registry
//!   serves under a bare name (the derived Codec's `__err_at`,
//!   `__decode_option_string`): its bodies' categories
//!   (`almide_ir::effect::derived_helper_effect`);
//! - a stdlib function the scan does not hold: its declaration
//!   (`almide_ir::effect::stdlib_call_effect`, itself ⊤ for a fn no stdlib
//!   source declares).
//!
//! Every rule reads declarations, types or bodies — never a callee's name.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use almide_ir::visit::{walk_expr, walk_stmt, IrVisitor};
use almide_ir::*;
use almide_lang::types::Ty;
use super::graph::FnIx;
use super::index::Index;

/// One conformer a dispatch may run.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Conformer {
    /// A function the scan holds (a user convention method, or a bundled
    /// stdlib function); `stdlib` when it is a stdlib module's, whose
    /// declaration's categories are charged too.
    Scanned { f: FnIx, stdlib: Option<(String, String)> },
    /// A stdlib function the scan does not hold, judged by its declaration.
    Declared { module: String, func: String },
}

/// The program-wide facts the callee rules read, built once per solve.
pub(super) struct Callees {
    /// Every concrete type the program holds a value of: each subterm of
    /// every expression, parameter, result and `let` type, aliases expanded.
    types: Vec<Ty>,
    /// Convention methods by their method name: `(type, fn)` for every
    /// scanned fn declared `Type.method`.
    methods: HashMap<String, Vec<(String, FnIx)>>,
    /// Variant cases by name: the declared types (bare names) that have it.
    cases: HashMap<String, BTreeSet<String>>,
    /// The protocols each declared type says it conforms to (`type T: P`),
    /// by bare names.
    declared: HashMap<String, BTreeSet<String>>,
    /// Conformer sets already computed, by requirement name.
    memo: RefCell<HashMap<String, Vec<Conformer>>>,
    /// Every scanned fn by its qualified key (`m.f`, or `f` at the root),
    /// with its module's name and its own declared name.
    keys: Vec<(String, Option<String>, String, FnIx)>,
}

fn last(s: &str) -> &str {
    s.rsplit_once('.').map_or(s, |(_, b)| b)
}

/// Every type subterm an IR walk sees.
struct Types<'t>(&'t mut BTreeMap<String, Ty>);

impl Types<'_> {
    fn add(&mut self, ty: &Ty) {
        let key = format!("{ty:?}");
        if self.0.contains_key(&key) {
            return;
        }
        self.0.insert(key, ty.clone());
        if let Some(t) = super::scan::aliases::target(ty) {
            self.add(&t);
        }
        for c in ty.children() {
            self.add(c);
        }
    }
}

impl IrVisitor for Types<'_> {
    fn visit_expr(&mut self, e: &IrExpr) {
        self.add(&e.ty);
        if let IrExprKind::Call { type_args, .. } = &e.kind {
            for t in type_args {
                self.add(t);
            }
        }
        walk_expr(self, e);
    }

    fn visit_stmt(&mut self, s: &IrStmt) {
        if let IrStmtKind::Bind { ty, .. } = &s.kind {
            self.add(ty);
        }
        walk_stmt(self, s);
    }
}

impl Callees {
    pub fn build(program: &IrProgram, ix: &Index<'_>) -> Callees {
        let mut seen: BTreeMap<String, Ty> = BTreeMap::new();
        {
            let mut w = Types(&mut seen);
            let funcs = program.functions.iter().chain(program.modules.iter().flat_map(|m| m.functions.iter()));
            for f in funcs {
                for p in &f.params {
                    w.add(&p.ty);
                }
                w.add(&f.ret_ty);
                w.visit_expr(&f.body);
            }
            let lets = program.top_lets.iter().chain(program.modules.iter().flat_map(|m| m.top_lets.iter()));
            for l in lets {
                w.add(&l.ty);
                w.visit_expr(&l.value);
            }
        }
        let types = seen.into_values().filter(|t| !matches!(t, Ty::TypeVar(_) | Ty::Unknown)).collect();
        let mut methods: HashMap<String, Vec<(String, FnIx)>> = HashMap::new();
        for (f, info) in ix.fns.iter().enumerate() {
            let Some(func) = info.func else { continue };
            if let Some((ty, m)) = func.name.as_str().rsplit_once('.') {
                methods.entry(m.to_string()).or_default().push((last(ty).to_string(), f));
            }
        }
        let mut cases: HashMap<String, BTreeSet<String>> = HashMap::new();
        let mut declared: HashMap<String, BTreeSet<String>> = HashMap::new();
        let decls = program.type_decls.iter().chain(program.modules.iter().flat_map(|m| m.type_decls.iter()));
        for d in decls {
            let ps = declared.entry(last(d.name.as_str()).to_string()).or_default();
            ps.extend(d.deriving.iter().flatten().map(|p| last(p.as_str()).to_string()));
            if let IrTypeDeclKind::Variant { cases: cs, .. } = &d.kind {
                for c in cs {
                    cases.entry(c.name.to_string()).or_default().insert(last(d.name.as_str()).to_string());
                }
            }
        }
        let keys = ix
            .fns
            .iter()
            .enumerate()
            .filter_map(|(f, info)| {
                let func = info.func?;
                let module = info.table.checked_sub(1).map(|m| program.modules[m].name.to_string());
                Some((info.name.clone(), module, func.name.to_string(), f))
            })
            .collect();
        Callees { types, methods, cases, declared, memo: RefCell::new(HashMap::new()), keys }
    }

    /// The program's own functions a name the scan's resolver did not find
    /// may call, matched the way the later passes link it: a module-qualified
    /// spelling either way (`Order.total` is `domain.Order.total`;
    /// `varlib.Pigment.__strict` is varlib's `Pigment.__strict`), or a fn of
    /// module `m` whose name is the call's with the qualifier `m.` taken out
    /// (a derived helper named for a module's type: `__encode_list_varlib.Pigment`
    /// is varlib's `__encode_list_Pigment`). Loose on purpose: a fn it keeps
    /// that is never called only widens the charge.
    pub fn program_fns(&self, name: &str) -> Vec<FnIx> {
        let dotted = |long: &str, short: &str| {
            long.len() > short.len() && long.ends_with(short) && long.as_bytes()[long.len() - short.len() - 1] == b'.'
        };
        self.keys
            .iter()
            .filter(|(key, module, own, _)| {
                key == name
                    || dotted(key, name)
                    || dotted(name, key)
                    || module.as_deref().is_some_and(|m| name.contains(&format!("{m}.")) && name.replace(&format!("{m}."), "") == *own)
            })
            .map(|(_, _, _, f)| *f)
            .collect()
    }

    /// Is a call of `name` whose result has type `ret` a constructor — a
    /// declared variant case of the type it builds, or the type's own name
    /// (an opaque or newtype wrapper)?
    pub fn is_constructor(&self, name: &str, ret: &Ty) -> bool {
        let built = match ret {
            Ty::Named(n, _) => last(n.as_str()).to_string(),
            Ty::Variant { name: n, .. } => last(n.as_str()).to_string(),
            _ => return false,
        };
        let case = last(name);
        if case == built {
            return true;
        }
        self.cases.get(case).is_some_and(|tys| tys.contains(&built))
    }

    /// The conformers a requirement `method` called through a type variable
    /// bounded by `bounds` may run: for every concrete type the program holds
    /// a value of that may conform — a declared type that says it conforms
    /// to a bound, or that declares no protocol at all, and every built-in
    /// type — the function monomorphisation would rewrite the call to, when
    /// one exists.
    pub fn conformers(&self, ix: &Index<'_>, table: usize, method: &str, bounds: &[String]) -> Vec<Conformer> {
        let memo_key = format!("{method}:{}", bounds.join(","));
        if let Some(c) = self.memo.borrow().get(&memo_key) {
            return c.clone();
        }
        let may_conform = |ty: &Ty| {
            let Some(n) = ty_name(ty) else { return true };
            match self.declared.get(last(&n)) {
                Some(ps) if !ps.is_empty() && !bounds.is_empty() => bounds.iter().any(|b| ps.contains(b)),
                _ => true,
            }
        };
        let mut out: BTreeSet<Conformer> = BTreeSet::new();
        let public = almide_lang::stdlib_info::requirement_public_name(method);
        for ty in self.types.iter().filter(|t| may_conform(t)) {
            if let Some(module) = ty.owning_module() {
                let func = match ty_name(ty) {
                    Some(n) => almide_lang::stdlib_info::requirement_fn_name(&n, method),
                    None => method.to_string(),
                };
                let stdlib = Some((module.to_string(), func.clone()));
                match ix.resolve(table, &format!("{module}.{func}")) {
                    Some(f) => {
                        out.insert(Conformer::Scanned { f, stdlib });
                    }
                    None if almide_ir::effect::stdlib_declares(module, &func) => {
                        out.insert(Conformer::Declared { module: module.to_string(), func });
                    }
                    None => {}
                }
                continue;
            }
            let Some(n) = ty_name(ty) else { continue };
            let bare = last(&n).to_string();
            for (t, f) in self.methods.get(public).into_iter().flatten() {
                if *t == bare {
                    out.insert(Conformer::Scanned { f: *f, stdlib: None });
                }
            }
        }
        let out: Vec<Conformer> = out.into_iter().collect();
        self.memo.borrow_mut().insert(memo_key, out.clone());
        out
    }
}

/// The name a concrete type is dispatched by (monomorphisation's
/// `ty_to_name`).
fn ty_name(ty: &Ty) -> Option<String> {
    Some(match ty {
        Ty::Named(name, _) => name.to_string(),
        Ty::Int => "Int".into(),
        Ty::Float => "Float".into(),
        Ty::Int8 => "Int8".into(),
        Ty::Int16 => "Int16".into(),
        Ty::Int32 => "Int32".into(),
        Ty::Int64 => "Int64".into(),
        Ty::UInt8 => "UInt8".into(),
        Ty::UInt16 => "UInt16".into(),
        Ty::UInt32 => "UInt32".into(),
        Ty::UInt64 => "UInt64".into(),
        Ty::Float32 => "Float32".into(),
        Ty::Float64 => "Float64".into(),
        Ty::String => "String".into(),
        Ty::Bool => "Bool".into(),
        Ty::Bytes => "Bytes".into(),
        Ty::Matrix => "Matrix".into(),
        Ty::Unit => "Unit".into(),
        _ => return None,
    })
}

/// The type variable a dispatch is called through: `T` of `T.__join` when
/// `T` is a generic of the enclosing function.
/// The protocol bounds (bare names) of the type variable `tv` of `func`.
pub(super) fn bounds_of(tv: &str, func: Option<&IrFunction>) -> Vec<String> {
    func.and_then(|f| f.generics.as_ref())
        .into_iter()
        .flatten()
        .filter(|g| g.name.as_str() == tv)
        .flat_map(|g| g.bounds.iter().flatten().map(|b| last(b.as_str()).to_string()))
        .collect()
}

pub(super) fn dispatch_var<'n>(name: &'n str, func: Option<&IrFunction>) -> Option<(&'n str, &'n str)> {
    let (tv, method) = name.split_once('.')?;
    let generic = func?.generics.iter().flatten().any(|g| g.name.as_str() == tv);
    generic.then_some((tv, method))
}
