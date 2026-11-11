//! A `Value` built and consumed exactly once by a known consumer is not
//! materialized.
//!
//! Two rewrites, both over the ROOT program (one var table, one binding site
//! per id — the `sink_inline` discipline), both read off the shape of the
//! code; no user function is named, and no stdlib function either: what a
//! stdlib function is to a `Value` (a constructor of one kind, a reader of
//! one kind at a key, the JSON parse, the JSON text) is its `@value_op`
//! declaration in the embedded stdlib source ([`declared_value_op`]). The
//! names below (`value.int`, `json.parse`, ..) are the functions that carry
//! the declaration today.
//!
//! **Reader.** `json.parse(t) ?? value.null()` whose only use is
//! `(value.as_array(_) ?? []) |> list.map(f)`, where `f` reads one Int along
//! a static key path of its element (`value.get(_, "k") ?? value.null()`
//! steps ending in `value.get_int(_, "k") ?? d`), becomes a call to a
//! synthesized helper
//!
//! ```text
//! fn __almd_json_ints_N(t) = match json.__ints_at(t, ["k", ..], d) {
//!   some(col) => col,
//!   none => <the original expression>,
//! }
//! ```
//!
//! `json.__ints_at` (stdlib/json.almd) validates the whole text and answers
//! only for documents it proves `json.parse` accepts; on any refusal the
//! original expression runs, so every error, the repeated-key refusal, the
//! depth bound and every number edge case are the parser's own. The parse is
//! pure and `t` is an immutable binding, so running it at the use instead of
//! at its `let` is unobservable; the use must sit in the block that binds the
//! parse result, outside any loop or lambda, so it still runs at most once.
//!
//! **Writer.** `json.stringify(value.array(xs |> list.map((x) => g(x))))`,
//! where the tail of `g` is a constructor tree (`value.object` /
//! `value.array` literals over `value.int` / `value.bool` / `value.string` /
//! `value.null`, and calls to other such functions), writes the text
//! directly: `g` gets a writer twin (`__almd_json_text_<g>`) with the same
//! parameters and statements whose tail is one interpolation, the field
//! expressions its holes IN SOURCE ORDER, and the site becomes
//! `json.__array_text(xs |> list.map((x) => twin(x)))`. Each hole is
//! evaluated exactly where its constructor argument was, so effects and
//! aborts keep their order; the text between them is pure. A sub-value the
//! pass cannot see through is a hole holding `json.stringify(<it>)`, the
//! runtime writer's own answer. A key or a literal that needs an escape, a
//! repeated key and a string hole that is not an Int or a Bool leave that
//! constructor to the runtime writer. A `let`-bound array is rewritten
//! where it is bound (nothing moves), when its only use is the stringify.
//! A lone tree written once is left alone: nothing per element to save.
//!
//! Both kernels are functions of the `json` module, kept live by
//! `bundled_reach::COMPILER_INSERTED`, stamped `@on_demand` here and so
//! dropped after mono when no rewrite named them. The reader's plan is (key path, Int leaf, default);
//! a derived decoder's field list is the same plan with more leaves.
//!
//! Ablation: `ALMIDE_VALUE_FUSE_OFF=1` skips the pass.

use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};

use almide_base::intern::{sym, Sym};
use almide_ir::visit::{walk_expr, IrVisitor};
use almide_ir::visit_mut::{walk_expr_mut, walk_pattern_mut, walk_stmt_mut, IrMutVisitor};
use almide_ir::*;
use almide_lang::ast::{AttrValue, Decl};
use almide_lang::types::Ty;

pub const ABLATION_ENV: &str = "ALMIDE_VALUE_FUSE_OFF";

const READER: &str = "__ints_at";
const ARRAY_WRITER: &str = "__array_text";

/// The kernels and their helpers (`__jr_*`) in the `json` module.
fn is_kernel(name: &str) -> bool {
    name == READER || name == ARRAY_WRITER || name.starts_with("__jr_")
}

/// Stamps the kernels `@on_demand`: `bundled_reach` keeps them live for
/// every program that loads `json`, and `on_demand::prune_unreached` drops
/// them after mono unless a rewrite named them. Runs whether or not the
/// rewrites do.
pub fn stamp_kernels(program: &mut IrProgram) {
    for m in program.modules.iter_mut().filter(|m| m.name.as_str() == "json") {
        for f in m.functions.iter_mut().filter(|f| is_kernel(f.name.as_str())) {
            if !f.attrs.iter().any(|a| a.name.as_str() == ON_DEMAND_ATTR) {
                f.attrs.push(almide_lang::ast::Attribute { name: sym(ON_DEMAND_ATTR), args: vec![], span: None });
            }
        }
    }
}

pub fn fuse_value_round_trips(program: &mut IrProgram) {
    if almide_base::env::flag(ABLATION_ENV) {
        return;
    }
    let Some(json) = program.modules.iter().find(|m| m.name.as_str() == "json") else { return };
    let has = |f: &str| json.functions.iter().any(|g| g.name.as_str() == f);
    let (reader_live, writer_live) = (has(READER), has(ARRAY_WRITER));
    if !reader_live && !writer_live {
        return;
    }

    let mut readers: HashMap<Sym, Plan> = HashMap::new();
    let mut writers: HashSet<Sym> = HashSet::new();
    if reader_live {
        for f in &program.functions {
            if let Some(plan) = reader_fn(f) {
                readers.insert(f.name, plan);
            }
        }
    }
    if writer_live {
        // A producer may call another producer: grow to the fixpoint.
        loop {
            let before = writers.len();
            for f in &program.functions {
                if !writers.contains(&f.name) && writer_fn(f, &writers) {
                    writers.insert(f.name);
                }
            }
            if writers.len() == before {
                break;
            }
        }
    }

    let IrProgram { functions, var_table, .. } = program;
    let mut cx = Fuser {
        vt: var_table,
        readers: &readers,
        writers: &writers,
        reader_live,
        writer_live,
        new_funcs: Vec::new(),
        twins_wanted: Vec::new(),
        text_fn: None,
        counter: 0,
    };
    for f in functions.iter_mut() {
        cx.visit_expr_mut(&mut f.body);
    }
    // Twins, and the twins their bodies call.
    let mut built: HashSet<Sym> = HashSet::new();
    while let Some(name) = cx.twins_wanted.pop() {
        if !built.insert(name) {
            continue;
        }
        let Some(src) = functions.iter().find(|f| f.name == name) else { continue };
        let twin = cx.twin(src);
        cx.new_funcs.push(twin);
    }
    let new_funcs = std::mem::take(&mut cx.new_funcs);
    functions.extend(new_funcs);
}

// ── shapes ──────────────────────────────────────────────────────────────

/// What the reader kernel is asked for: the key path under each element and
/// the Int answered where an element has no Int there.
#[derive(Clone)]
struct Plan {
    keys: Vec<String>,
    dflt: i64,
}

/// A JSON kind of a `Value`, as `@value_op` spells it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum ValueKind {
    Null,
    Bool,
    Int,
    Float,
    String,
    Array,
    Object,
}

/// What a stdlib fn declared `@value_op(..)` is to a `Value`.
///
/// * `@value_op("make", K)`: the `Value` of kind `K` holding its one
///   argument (none for `"null"`).
/// * `@value_op("get")`: `some` of the member at the key (2nd argument) when
///   the 1st is an Object that has it, `none` otherwise.
/// * `@value_op("get", K)`: that member's payload when it is of kind `K`,
///   `none` otherwise.
/// * `@value_op("as", K)`: `ok` of the argument's payload when it is of kind
///   `K`, `err` otherwise.
/// * `@value_op("parse")`: the `Value` a JSON text denotes, or `err`.
/// * `@value_op("text")`: the compact JSON text of a `Value`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum ValueOp {
    Make(ValueKind),
    Get(Option<ValueKind>),
    As(ValueKind),
    Parse,
    Text,
}

const VALUE_OP_ATTR: &str = "value_op";

fn value_op_of(args: &[&str]) -> Option<ValueOp> {
    let kind = |k: &str| {
        Some(match k {
            "null" => ValueKind::Null,
            "bool" => ValueKind::Bool,
            "int" => ValueKind::Int,
            "float" => ValueKind::Float,
            "string" => ValueKind::String,
            "array" => ValueKind::Array,
            "object" => ValueKind::Object,
            _ => return None,
        })
    };
    Some(match args {
        ["make", k] => ValueOp::Make(kind(k)?),
        ["get"] => ValueOp::Get(None),
        ["get", k] => ValueOp::Get(Some(kind(k)?)),
        ["as", k] => ValueOp::As(kind(k)?),
        ["parse"] => ValueOp::Parse,
        ["text"] => ValueOp::Text,
        _ => return None,
    })
}

/// The `@value_op` the embedded stdlib source declares on `module.func`.
/// Read from the stdlib's own text, so a module of another origin that
/// carries the attribute declares nothing here.
pub(crate) fn declared_value_op(module: &str, func: &str) -> Option<ValueOp> {
    type Table = HashMap<(Sym, Sym), Option<ValueOp>>;
    static CACHE: OnceLock<Mutex<Table>> = OnceLock::new();
    let key = (sym(module), sym(func));
    let cache = CACHE.get_or_init(Default::default);
    if let Some(hit) = cache.lock().unwrap_or_else(|e| e.into_inner()).get(&key) {
        return *hit;
    }
    let made = almide_lang::stdlib_info::bundled_source(module).and_then(almide_lang::parse_cached).and_then(|program| {
        program.decls.iter().find_map(|d| match d {
            Decl::Fn { name, attrs, .. } if name.as_str() == func => {
                let attr = attrs.iter().find(|a| a.name.as_str() == VALUE_OP_ATTR)?;
                let args: Option<Vec<&str>> = attr
                    .args
                    .iter()
                    .map(|a| match (&a.name, &a.value) {
                        (None, AttrValue::String { value }) => Some(value.as_str()),
                        _ => None,
                    })
                    .collect();
                value_op_of(&args?)
            }
            _ => None,
        })
    });
    cache.lock().unwrap_or_else(|e| e.into_inner()).insert(key, made);
    made
}

/// The arguments of a call to a stdlib fn declared `@value_op` as `op`.
fn op_call(e: &IrExpr, op: ValueOp) -> Option<&[IrExpr]> {
    match &e.kind {
        IrExprKind::Call { target: CallTarget::Module { module: m, func: f, .. }, args, .. }
            if declared_value_op(m.as_str(), f.as_str()) == Some(op) =>
        {
            Some(args.as_slice())
        }
        _ => None,
    }
}

fn make_call(e: &IrExpr, kind: ValueKind) -> Option<&[IrExpr]> {
    op_call(e, ValueOp::Make(kind))
}

/// `<the declared JSON text fn>(<x>)` → (the fn, `x`).
fn text_of_call(e: &IrExpr) -> Option<(&CallTarget, &IrExpr)> {
    let [x] = op_call(e, ValueOp::Text)? else { return None };
    let IrExprKind::Call { target, .. } = &e.kind else { return None };
    Some((target, x))
}

fn module_call<'e>(e: &'e IrExpr, module: &str, func: &str) -> Option<&'e [IrExpr]> {
    match &e.kind {
        IrExprKind::Call { target: CallTarget::Module { module: m, func: f, .. }, args, .. }
            if m.as_str() == module && f.as_str() == func =>
        {
            Some(args.as_slice())
        }
        _ => None,
    }
}

fn is_var(e: &IrExpr, id: VarId) -> bool {
    matches!(e.kind, IrExprKind::Var { id: v } if v == id)
}

fn is_value_null(e: &IrExpr) -> bool {
    make_call(e, ValueKind::Null).is_some_and(|a| a.is_empty())
}

fn is_value_ty(ty: &Ty) -> bool {
    matches!(ty, Ty::Named(n, args) if n.as_str() == "Value" && args.is_empty())
}

/// A body is the tail of an empty block as much as it is itself.
fn tail(e: &IrExpr) -> &IrExpr {
    match &e.kind {
        IrExprKind::Block { stmts, expr: Some(t) } if stmts.is_empty() => tail(t),
        _ => e,
    }
}

fn plain_fn(f: &IrFunction) -> bool {
    !f.is_effect
        && !f.is_test
        && f.extern_attrs.is_empty()
        && f.export_attrs.is_empty()
        && f.name.as_str() != "main"
        && !f.generics.as_ref().is_some_and(|g| !g.is_empty())
        && f.params.iter().all(|p| !p.is_mut && p.default.is_none() && p.open_record.is_none())
}

/// `value.get(.. value.get(p, k0) ?? value.null() .., kn) ?? value.null()`
/// chains down from `p` (each step a declared key read whose fallback is the
/// declared Null); the keys are pushed outermost first.
fn path_steps(e: &IrExpr, p: VarId, keys: &mut Vec<String>) -> bool {
    if is_var(e, p) {
        return true;
    }
    let IrExprKind::UnwrapOr { expr, fallback } = &e.kind else { return false };
    let (Some([inner, key]), true) = (op_call(expr, ValueOp::Get(None)), is_value_null(fallback)) else { return false };
    let IrExprKind::LitStr { value } = &key.kind else { return false };
    if !path_steps(inner, p, keys) {
        return false;
    }
    keys.push(value.clone());
    true
}

/// `value.get_int(<steps from p>, k) ?? <Int literal>`.
fn int_leaf(e: &IrExpr, p: VarId) -> Option<Plan> {
    let IrExprKind::UnwrapOr { expr, fallback } = &tail(e).kind else { return None };
    let [inner, key] = op_call(expr, ValueOp::Get(Some(ValueKind::Int)))? else { return None };
    let IrExprKind::LitStr { value: key } = &key.kind else { return None };
    // The pass runs before constant folding: `-1` is still a negation.
    let dflt = match &fallback.kind {
        IrExprKind::LitInt { value } => *value,
        IrExprKind::UnOp { op: UnOp::NegInt, operand } => match operand.kind {
            IrExprKind::LitInt { value } => value.checked_neg()?,
            _ => return None,
        },
        _ => return None,
    };
    let mut keys = Vec::new();
    if !path_steps(inner, p, &mut keys) {
        return None;
    }
    keys.push(key.clone());
    // A key the kernel compares raw must be what the parser decodes: any key
    // is, since the kernel refuses a document key holding an escape.
    Some(Plan { keys, dflt })
}

fn reader_fn(f: &IrFunction) -> Option<Plan> {
    let [p] = f.params.as_slice() else { return None };
    if !plain_fn(f) || !is_value_ty(&p.ty) {
        return None;
    }
    int_leaf(&f.body, p.var)
}

/// `json.parse(<Var t>) ?? value.null()` → `t`.
fn parse_or_null(e: &IrExpr) -> Option<VarId> {
    let IrExprKind::UnwrapOr { expr, fallback } = &e.kind else { return None };
    let ([text], true) = (op_call(expr, ValueOp::Parse)?, is_value_null(fallback)) else { return None };
    match text.kind {
        IrExprKind::Var { id } if matches!(text.ty, Ty::String) => Some(id),
        _ => None,
    }
}

fn clean_text(s: &str) -> bool {
    s.bytes().all(|b| (0x20..0x7f).contains(&b) && b != b'"' && b != b'\\')
}

fn is_literal_ctor(e: &IrExpr) -> bool {
    let literal = |a: &[IrExpr]| matches!(a, [l] if matches!(l.kind, IrExprKind::List { .. }));
    make_call(e, ValueKind::Object).is_some_and(literal) || make_call(e, ValueKind::Array).is_some_and(literal)
}

/// A function whose tail is a constructor tree the writer can spell.
fn writer_fn(f: &IrFunction, writers: &HashSet<Sym>) -> bool {
    if !plain_fn(f) || !is_value_ty(&f.ret_ty) {
        return false;
    }
    let t = match &f.body.kind {
        IrExprKind::Block { expr: Some(t), .. } => t.as_ref(),
        IrExprKind::Block { .. } => return false,
        _ => &f.body,
    };
    let producer_call = matches!(&t.kind, IrExprKind::Call { target: CallTarget::Named { name }, .. } if writers.contains(name));
    (is_literal_ctor(t) || producer_call) && renamable(&f.body) && Spell { writers, wanted: None, text: None }.text_of(t).is_some()
}

// ── the writer's spelling of a constructor tree ─────────────────────────

struct Spell<'a> {
    writers: &'a HashSet<Sym>,
    /// `None` while only asking whether a tree can be spelled.
    wanted: Option<&'a mut Vec<Sym>>,
    /// The declared JSON text fn the rewritten site called: what a hole the
    /// writer cannot see through calls. `None` while only asking.
    text: Option<&'a CallTarget>,
}

fn twin_name(g: Sym) -> Sym {
    sym(&format!("__almd_json_text_{}", g.as_str()))
}

fn lit(parts: &mut Vec<IrStringPart>, s: &str) {
    if let Some(IrStringPart::Lit { value }) = parts.last_mut() {
        value.push_str(s);
    } else {
        parts.push(IrStringPart::Lit { value: s.to_string() });
    }
}

fn str_expr(kind: IrExprKind, span: Option<almide_base::Span>) -> IrExpr {
    IrExpr { kind, ty: Ty::String, span, def_id: None }
}

fn text_call(target: CallTarget, arg: IrExpr) -> IrExpr {
    let span = arg.span;
    str_expr(IrExprKind::Call { target, args: vec![arg], type_args: vec![] }, span)
}

/// A call to one of this pass's own kernels in the `json` module.
fn kernel_call(func: &str, arg: IrExpr) -> IrExpr {
    text_call(CallTarget::Module { module: sym("json"), func: sym(func), def_id: None }, arg)
}

impl Spell<'_> {
    /// The text of the Value `e` as interpolation parts, or `None` when `e`
    /// is not a shape the writer spells.
    fn text_of(&mut self, e: &IrExpr) -> Option<Vec<IrStringPart>> {
        let mut parts = Vec::new();
        self.spell(e, &mut parts).then_some(parts)
    }

    /// A sub-value: spelled, or left to the runtime writer as one hole.
    fn value(&mut self, e: &IrExpr, parts: &mut Vec<IrStringPart>) {
        let mut own = Vec::new();
        if self.spell(e, &mut own) {
            for p in own {
                match p {
                    IrStringPart::Lit { value } => lit(parts, &value),
                    p => parts.push(p),
                }
            }
        } else if let Some(text) = self.text {
            parts.push(IrStringPart::Expr { expr: text_call(text.clone(), e.clone()) });
        }
    }

    fn spell(&mut self, e: &IrExpr, parts: &mut Vec<IrStringPart>) -> bool {
        if let Some([list]) = make_call(e, ValueKind::Object) {
            let IrExprKind::List { elements } = &list.kind else { return false };
            let mut seen = HashSet::new();
            let mut fields = Vec::new();
            for el in elements {
                let IrExprKind::Tuple { elements: kv } = &el.kind else { return false };
                let [k, v] = kv.as_slice() else { return false };
                let IrExprKind::LitStr { value: key } = &k.kind else { return false };
                if !clean_text(key) || !seen.insert(key.as_str()) {
                    return false;
                }
                fields.push((key, v));
            }
            lit(parts, "{");
            for (i, (key, v)) in fields.into_iter().enumerate() {
                lit(parts, &format!("{}\"{}\":", if i > 0 { "," } else { "" }, key));
                self.value(v, parts);
            }
            lit(parts, "}");
            return true;
        }
        if let Some([arg]) = make_call(e, ValueKind::Array) {
            if let IrExprKind::List { elements } = &arg.kind {
                lit(parts, "[");
                for (i, v) in elements.iter().enumerate() {
                    if i > 0 {
                        lit(parts, ",");
                    }
                    self.value(v, parts);
                }
                lit(parts, "]");
                return true;
            }
            // value.array(xs |> list.map((x) => g(x))) over a producer g
            let Some([src, f]) = module_call(arg, "list", "map") else { return false };
            let IrExprKind::Lambda { params, body, lambda_id } = &f.kind else { return false };
            let [(p, _)] = params.as_slice() else { return false };
            let IrExprKind::Call { target: CallTarget::Named { name }, args, type_args } = &body.kind else { return false };
            if !self.writers.contains(name) || !matches!(args.as_slice(), [a] if is_var(a, *p)) {
                return false;
            }
            if let Some(w) = self.wanted.as_deref_mut() {
                w.push(*name);
            }
            let twin_call = str_expr(
                IrExprKind::Call {
                    target: CallTarget::Named { name: twin_name(*name) },
                    args: args.clone(),
                    type_args: type_args.clone(),
                },
                body.span,
            );
            let lambda = IrExpr {
                kind: IrExprKind::Lambda { params: params.clone(), body: Box::new(twin_call), lambda_id: *lambda_id },
                ty: match &f.ty {
                    Ty::Fn { params, is_effect, .. } => Ty::Fn { params: params.clone(), ret: Box::new(Ty::String), is_effect: *is_effect },
                    _ => return false,
                },
                span: f.span,
                def_id: None,
            };
            let IrExprKind::Call { target, type_args, .. } = &arg.kind else { return false };
            let type_args = type_args.iter().map(|t| if is_value_ty(t) { Ty::String } else { t.clone() }).collect();
            let mapped = IrExpr {
                kind: IrExprKind::Call { target: target.clone(), args: vec![src.clone(), lambda], type_args },
                ty: Ty::list(Ty::String),
                span: arg.span,
                def_id: None,
            };
            parts.push(IrStringPart::Expr { expr: kernel_call(ARRAY_WRITER, mapped) });
            return true;
        }
        if let Some([x]) = make_call(e, ValueKind::Int) {
            if !matches!(x.ty, Ty::Int) {
                return false;
            }
            parts.push(IrStringPart::Expr { expr: x.clone() });
            return true;
        }
        if let Some([x]) = make_call(e, ValueKind::Bool) {
            if !matches!(x.ty, Ty::Bool) {
                return false;
            }
            parts.push(IrStringPart::Expr { expr: x.clone() });
            return true;
        }
        if is_value_null(e) {
            lit(parts, "null");
            return true;
        }
        if let Some([x]) = make_call(e, ValueKind::String) {
            return match &x.kind {
                IrExprKind::LitStr { value } if clean_text(value) => {
                    lit(parts, &format!("\"{value}\""));
                    true
                }
                IrExprKind::StringInterp { parts: inner }
                    if inner.iter().all(|p| match p {
                        IrStringPart::Lit { value } => clean_text(value),
                        IrStringPart::Expr { expr } => matches!(expr.ty, Ty::Int | Ty::Bool),
                    }) =>
                {
                    lit(parts, "\"");
                    for p in inner {
                        match p {
                            IrStringPart::Lit { value } => lit(parts, value),
                            p => parts.push(p.clone()),
                        }
                    }
                    lit(parts, "\"");
                    true
                }
                _ => false,
            };
        }
        if let IrExprKind::Call { target: CallTarget::Named { name }, args, type_args } = &e.kind {
            if !self.writers.contains(name) || !type_args.is_empty() {
                return false;
            }
            if let Some(w) = self.wanted.as_deref_mut() {
                w.push(*name);
            }
            let call = str_expr(
                IrExprKind::Call { target: CallTarget::Named { name: twin_name(*name) }, args: args.clone(), type_args: vec![] },
                e.span,
            );
            parts.push(IrStringPart::Expr { expr: call });
            return true;
        }
        false
    }
}

// ── the rewrite ─────────────────────────────────────────────────────────

struct Fuser<'a> {
    vt: &'a mut VarTable,
    readers: &'a HashMap<Sym, Plan>,
    writers: &'a HashSet<Sym>,
    reader_live: bool,
    writer_live: bool,
    new_funcs: Vec<IrFunction>,
    twins_wanted: Vec<Sym>,
    /// The declared JSON text fn of the stringify site being rewritten (and
    /// of the twins it wants).
    text_fn: Option<CallTarget>,
    counter: u32,
}

fn occurrences_in(stmts: &[IrStmt], tail: Option<&IrExpr>, var: VarId) -> usize {
    struct Count {
        var: VarId,
        n: usize,
    }
    impl IrVisitor for Count {
        fn visit_expr(&mut self, e: &IrExpr) {
            if is_var(e, self.var) {
                self.n += 1;
            }
            walk_expr(self, e);
        }
    }
    let mut c = Count { var, n: 0 };
    for s in stmts {
        c.visit_stmt(s);
    }
    if let Some(t) = tail {
        c.visit_expr(t);
    }
    c.n
}

/// Replaces every read of `from` with a copy of `to`.
struct Subst<'e> {
    from: VarId,
    to: &'e IrExpr,
}

impl IrMutVisitor for Subst<'_> {
    fn visit_expr_mut(&mut self, e: &mut IrExpr) {
        if is_var(e, self.from) {
            *e = self.to.clone();
            return;
        }
        walk_expr_mut(self, e);
    }
}

/// Gives every variable a copied body binds a fresh id (`sink_inline`'s
/// rule: binding sites are met before their uses in walk order).
struct Rename<'v> {
    vt: &'v mut VarTable,
    map: HashMap<VarId, VarId>,
}

impl Rename<'_> {
    fn bind(&mut self, id: &mut VarId) {
        let info = self.vt.get(*id);
        let (ty, m) = (info.ty.clone(), info.mutability);
        let fresh = self.vt.alloc_like(*id, ty, m);
        self.map.insert(*id, fresh);
        *id = fresh;
    }
}

impl IrMutVisitor for Rename<'_> {
    fn visit_expr_mut(&mut self, e: &mut IrExpr) {
        match &mut e.kind {
            IrExprKind::Var { id } => {
                if let Some(n) = self.map.get(id) {
                    *id = *n;
                }
            }
            IrExprKind::Lambda { params, .. } => {
                for (id, _) in params.iter_mut() {
                    self.bind(id);
                }
            }
            IrExprKind::ForIn { var, var_tuple, .. } => {
                self.bind(var);
                for id in var_tuple.iter_mut().flatten() {
                    self.bind(id);
                }
            }
            _ => {}
        }
        walk_expr_mut(self, e);
    }
    fn visit_stmt_mut(&mut self, s: &mut IrStmt) {
        walk_stmt_mut(self, s);
        if let IrStmtKind::Bind { var, .. } = &mut s.kind {
            self.bind(var);
        }
    }
    fn visit_pattern_mut(&mut self, p: &mut IrPattern) {
        if let IrPattern::Bind { var, .. } | IrPattern::As { var, .. } = p {
            self.bind(var);
        }
        walk_pattern_mut(self, p);
    }
}

/// Only `let`, destructuring, expression, guard and comment statements name
/// their variables where `Rename` rewrites them.
fn renamable(e: &IrExpr) -> bool {
    struct Scan {
        ok: bool,
    }
    impl IrVisitor for Scan {
        fn visit_expr(&mut self, e: &IrExpr) {
            if matches!(e.kind, IrExprKind::ClosureCreate { .. } | IrExprKind::EnvLoad { .. }) {
                self.ok = false;
            }
            if self.ok {
                walk_expr(self, e);
            }
        }
        fn visit_stmt(&mut self, s: &IrStmt) {
            if !matches!(
                s.kind,
                IrStmtKind::Bind { .. }
                    | IrStmtKind::BindDestructure { .. }
                    | IrStmtKind::Expr { .. }
                    | IrStmtKind::Guard { .. }
                    | IrStmtKind::Comment { .. }
            ) {
                self.ok = false;
            }
            if self.ok {
                almide_ir::visit::walk_stmt(self, s);
            }
        }
    }
    let mut s = Scan { ok: true };
    s.visit_expr(e);
    s.ok
}

fn new_fn(name: Sym, params: Vec<IrParam>, ret_ty: Ty, body: IrExpr) -> IrFunction {
    IrFunction {
        name,
        params,
        ret_ty,
        body,
        is_effect: false,
        is_test: false,
        generics: None,
        extern_attrs: vec![],
        export_attrs: vec![],
        attrs: vec![],
        visibility: IrVisibility::Private,
        doc: None,
        blank_lines_before: 0,
        def_id: None,
        mutated_params: vec![],
        module_origin: None,
    }
}

fn own_param(var: VarId, name: Sym, ty: Ty) -> IrParam {
    IrParam { var, ty, name, borrow: ParamBorrow::Own, is_mut: false, open_record: None, default: None, attrs: vec![] }
}

impl Fuser<'_> {
    /// The reader site `list.map(value.as_array(<doc>) ?? [], f)`: the
    /// document operand and the plan `f` reads by.
    fn reader_site<'e>(&self, e: &'e IrExpr) -> Option<(&'e IrExpr, Plan)> {
        let [xs, f] = module_call(e, "list", "map")? else { return None };
        let IrExprKind::UnwrapOr { expr, fallback } = &xs.kind else { return None };
        let [doc] = op_call(expr, ValueOp::As(ValueKind::Array))? else { return None };
        if !matches!(&fallback.kind, IrExprKind::List { elements } if elements.is_empty()) {
            return None;
        }
        let IrExprKind::Lambda { params, body, .. } = &f.kind else { return None };
        let [(p, pty)] = params.as_slice() else { return None };
        if !is_value_ty(pty) || e.ty != Ty::list(Ty::Int) {
            return None;
        }
        let plan = match &tail(body).kind {
            IrExprKind::Call { target: CallTarget::Named { name }, args, .. } if matches!(args.as_slice(), [a] if is_var(a, *p)) => {
                self.readers.get(name)?.clone()
            }
            _ => int_leaf(body, *p)?,
        };
        Some((doc, plan))
    }

    /// `site` (a reader site whose document is the parse of `text`) as a
    /// call to its helper.
    fn read_through(&mut self, site: IrExpr, text: VarId, plan: Plan) -> IrExpr {
        let span = site.span;
        let col_ty = Ty::list(Ty::Int);
        let mk = |kind: IrExprKind, ty: Ty| IrExpr { kind, ty, span, def_id: None };
        let t = self.vt.alloc(sym("text"), Ty::String, Mutability::Let, span);
        let col = self.vt.alloc(sym("col"), col_ty.clone(), Mutability::Let, span);
        let t_read = mk(IrExprKind::Var { id: t }, Ty::String);
        let mut fallback = site;
        Subst { from: text, to: &t_read }.visit_expr_mut(&mut fallback);
        let keys = mk(
            IrExprKind::List {
                elements: plan.keys.iter().map(|k| mk(IrExprKind::LitStr { value: k.clone() }, Ty::String)).collect(),
            },
            Ty::list(Ty::String),
        );
        let kernel = mk(
            IrExprKind::Call {
                target: CallTarget::Module { module: sym("json"), func: sym(READER), def_id: None },
                args: vec![t_read, keys, mk(IrExprKind::LitInt { value: plan.dflt }, Ty::Int)],
                type_args: vec![],
            },
            Ty::option(col_ty.clone()),
        );
        let arms = vec![
            IrMatchArm {
                pattern: IrPattern::Some { inner: Box::new(IrPattern::Bind { var: col, ty: col_ty.clone() }) },
                guard: None,
                body: mk(IrExprKind::Var { id: col }, col_ty.clone()),
            },
            IrMatchArm { pattern: IrPattern::None, guard: None, body: fallback },
        ];
        let body = mk(IrExprKind::Match { subject: Box::new(kernel), arms }, col_ty.clone());
        let name = sym(&format!("__almd_json_ints_{}", self.counter));
        self.counter += 1;
        self.new_funcs.push(new_fn(name, vec![own_param(t, sym("text"), Ty::String)], col_ty.clone(), body));
        mk(
            IrExprKind::Call {
                target: CallTarget::Named { name },
                args: vec![mk(IrExprKind::Var { id: text }, Ty::String)],
                type_args: vec![],
            },
            col_ty,
        )
    }

    fn immutable(&self, v: VarId) -> bool {
        matches!(self.vt.get(v).mutability, Mutability::Let)
    }

    /// `let back = json.parse(t) ?? value.null()` read once, by a reader
    /// site in the statements after it: the site reads through the kernel
    /// and the bind goes.
    fn fuse_bound_parses(&mut self, stmts: &mut Vec<IrStmt>, tail: &mut Option<Box<IrExpr>>) {
        let mut i = 0;
        while i < stmts.len() {
            let found = match &stmts[i].kind {
                IrStmtKind::Bind { var, mutability: Mutability::Let, value, .. } => {
                    parse_or_null(value).filter(|t| self.immutable(*t)).map(|t| (*var, t, value.clone()))
                }
                _ => None,
            };
            let Some((back, text, parse)) = found else {
                i += 1;
                continue;
            };
            if occurrences_in(&stmts[i + 1..], tail.as_deref(), back) != 1 {
                i += 1;
                continue;
            }
            let mut at = AtSite { cx: self, back, text, parse: &parse, done: false };
            for s in stmts[i + 1..].iter_mut() {
                at.visit_stmt_mut(s);
            }
            if let Some(t) = tail.as_deref_mut() {
                at.visit_expr_mut(t);
            }
            if at.done {
                stmts.remove(i);
            } else {
                i += 1;
            }
        }
    }

    /// `let d = <tree>` read once, by `json.stringify(d)`: the bind holds
    /// the text and the stringify reads it.
    fn fuse_bound_trees(&mut self, stmts: &mut [IrStmt], tail: &mut Option<Box<IrExpr>>) {
        for i in 0..stmts.len() {
            let (head, rest) = stmts.split_at_mut(i + 1);
            let IrStmtKind::Bind { var, mutability: Mutability::Let, ty, value } = &mut head[i].kind else { continue };
            if !is_value_ty(ty) || occurrences_in(rest, tail.as_deref(), *var) != 1 {
                continue;
            }
            let mut probe = StringifyOf { var: *var, text: None, hits: 0, target: None };
            for s in rest.iter() {
                probe.visit_stmt(s);
            }
            if let Some(t) = tail.as_deref() {
                probe.visit_expr(t);
            }
            if probe.hits != 1 {
                continue;
            }
            self.text_fn = probe.target.take();
            let Some(text) = self.spelled(value, true) else { continue };
            let tv = self.vt.alloc_like(*var, Ty::String, Mutability::Let);
            *var = tv;
            *ty = Ty::String;
            *value = text;
            let mut swap = StringifyOf { var: probe.var, text: Some(tv), hits: 0, target: None };
            for s in rest.iter_mut() {
                swap.visit_stmt_mut(s);
            }
            if let Some(t) = tail.as_deref_mut() {
                swap.visit_expr_mut(t);
            }
        }
    }

    /// The Value expression `e` as the String expression that is its text.
    /// At a `json.stringify` site (`site`) only an array of produced records
    /// is taken — `value.array(xs |> list.map((x) => g(x)))` — where the
    /// tree not built is one per element; a lone tree written once would
    /// trade a few constructor calls for a longer interpolation and gain
    /// nothing. Inside a twin every spelled shape is taken.
    fn spelled(&mut self, e: &IrExpr, site: bool) -> Option<IrExpr> {
        if site && !make_call(e, ValueKind::Array).is_some_and(|a| matches!(a, [x] if module_call(x, "list", "map").is_some())) {
            return None;
        }
        let text = self.text_fn.as_ref()?;
        let parts = Spell { writers: self.writers, wanted: Some(&mut self.twins_wanted), text: Some(text) }.text_of(e)?;
        Some(match <[IrStringPart; 1]>::try_from(parts) {
            Ok([IrStringPart::Expr { expr }]) if matches!(expr.ty, Ty::String) => expr,
            Ok([IrStringPart::Lit { value }]) => str_expr(IrExprKind::LitStr { value }, e.span),
            Ok(one) => str_expr(IrExprKind::StringInterp { parts: one.into() }, e.span),
            Err(parts) => str_expr(IrExprKind::StringInterp { parts }, e.span),
        })
    }

    /// The writer twin of producer `src`: the same parameters and
    /// statements under fresh ids, the tail tree spelled as text.
    fn twin(&mut self, src: &IrFunction) -> IrFunction {
        let mut body = src.body.clone();
        let mut params = src.params.clone();
        let mut rn = Rename { vt: self.vt, map: HashMap::new() };
        for p in params.iter_mut() {
            rn.bind(&mut p.var);
        }
        rn.visit_expr_mut(&mut body);
        let spelled = match &mut body.kind {
            IrExprKind::Block { expr: Some(t), .. } => {
                let text = self.spelled(&**t, false).expect("a producer's tail is spelled (writer_fn)");
                **t = text;
                None
            }
            _ => Some(self.spelled(&body, false).expect("a producer's tail is spelled (writer_fn)")),
        };
        if let Some(text) = spelled {
            body = text;
        } else {
            body.ty = Ty::String;
        }
        new_fn(twin_name(src.name), params, Ty::String, body)
    }
}

/// Finds the one reader site over `back` in the statements after its bind,
/// without entering a loop or a lambda, and reads it through the kernel.
struct AtSite<'a, 'b, 'e> {
    cx: &'a mut Fuser<'b>,
    back: VarId,
    text: VarId,
    parse: &'e IrExpr,
    done: bool,
}

impl IrMutVisitor for AtSite<'_, '_, '_> {
    fn visit_expr_mut(&mut self, e: &mut IrExpr) {
        if self.done
            || matches!(e.kind, IrExprKind::Lambda { .. } | IrExprKind::ForIn { .. } | IrExprKind::While { .. })
        {
            return;
        }
        let hit = match self.cx.reader_site(e) {
            Some((doc, plan)) if is_var(doc, self.back) => Some(plan),
            _ => None,
        };
        let Some(plan) = hit else {
            walk_expr_mut(self, e);
            return;
        };
        let mut site = std::mem::take(e);
        Subst { from: self.back, to: self.parse }.visit_expr_mut(&mut site);
        *e = self.cx.read_through(site, self.text, plan);
        self.done = true;
    }
}

/// Counts (`text` unset) or rewrites (`text` set) `json.stringify(<var>)`.
struct StringifyOf {
    var: VarId,
    text: Option<VarId>,
    hits: usize,
    /// The text fn the counted site calls.
    target: Option<CallTarget>,
}

impl IrVisitor for StringifyOf {
    fn visit_expr(&mut self, e: &IrExpr) {
        if let Some((target, _)) = text_of_call(e).filter(|(_, x)| is_var(x, self.var)) {
            self.hits += 1;
            self.target = Some(target.clone());
        }
        walk_expr(self, e);
    }
}

impl IrMutVisitor for StringifyOf {
    fn visit_expr_mut(&mut self, e: &mut IrExpr) {
        if text_of_call(e).is_some_and(|(_, x)| is_var(x, self.var)) {
            if let Some(t) = self.text {
                e.kind = IrExprKind::Var { id: t };
                e.ty = Ty::String;
            }
            return;
        }
        walk_expr_mut(self, e);
    }
}

impl IrMutVisitor for Fuser<'_> {
    fn visit_expr_mut(&mut self, e: &mut IrExpr) {
        // Outermost first: a block's bound forms are rewritten where they
        // are bound before the inline forms inside them are looked at.
        if let IrExprKind::Block { stmts, expr } = &mut e.kind {
            if self.reader_live {
                self.fuse_bound_parses(stmts, expr);
            }
            if self.writer_live {
                self.fuse_bound_trees(stmts, expr);
            }
        }
        if self.writer_live {
            if let Some((target, v)) = text_of_call(e) {
                if !matches!(v.kind, IrExprKind::Var { .. }) {
                    self.text_fn = Some(target.clone());
                    if let Some(text) = self.spelled(v, true) {
                        *e = text;
                    }
                }
            }
        }
        if self.reader_live {
            let inline = self.reader_site(e).and_then(|(doc, plan)| {
                parse_or_null(doc).filter(|t| self.immutable(*t)).map(|t| (t, plan))
            });
            if let Some((text, plan)) = inline {
                let site = std::mem::take(e);
                *e = self.read_through(site, text, plan);
                return;
            }
        }
        walk_expr_mut(self, e);
    }
}
