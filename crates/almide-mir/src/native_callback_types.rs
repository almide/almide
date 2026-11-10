//! The records and variants a linked callback's signature names (#4640).
//!
//! Native code is written against the struct or enum the standard codegen
//! declares for a module type: it builds one, reads its fields, matches its
//! cases. So the DEFINITION is not re-derived here either. Each named type a
//! callback signature reaches is carried from the standard codegen's output
//! for the same program, derives included, and placed at the crate root
//! beside the adapters.
//!
//! This render holds the same value in its own generated type (`AlmdRecN`, or
//! the slot list of an all-scalar record), so the adapter copies it across,
//! field by field and case by case, in a function of its own per type and
//! direction: a recursive variant converts through a call, not an expression
//! that never ends. A carried definition is admitted only where every field
//! agrees with the declared one — same constructor, same label, and the host
//! spelling of the declared type, boxed or not as the definition says. One
//! that disagrees is not carried, and its callbacks are refused by name.
//!
//! A generic type has one definition (`pub struct Pair<A>`), admitted with
//! each parameter spelled as itself, and one conversion per instance the
//! signatures name (`Pair<i64>`), over the fields that instance has.
//!
//! Every conversion reads its source through a shared borrow and builds a
//! fresh value: nothing the certified side owns is aliased, and a value the
//! native code lent is never consumed.
use almide_ir::{IrFieldDecl, IrTypeDecl, IrTypeDeclKind, IrVariantKind};
use almide_lang::types::Ty;
use crate::lower::LowerError;
use crate::native_callback_host::canon;
use crate::native_foreign::{convert_value, host_type};
use crate::render_native::{wall, NTy};
use crate::render_native_rec as rec;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fmt::Write as _;

/// One field, in declared order; a tuple case's fields have no label.
#[derive(Clone)]
struct Slot {
    label: String,
    ty: Ty,
    /// The host definition holds it behind an `AlmideBox`.
    boxed: bool,
}

/// One constructor. A record is the single, unnamed case of its type.
#[derive(Clone)]
struct Case {
    ctor: String,
    fields: Vec<Slot>,
}

#[derive(Clone)]
struct Shape {
    host: String,
    /// The generic parameters, in declared order.
    params: Vec<String>,
    record: bool,
    /// A structural record's definition: its fields are sorted by label.
    structural: bool,
    cases: Vec<Case>,
}

#[derive(Default)]
struct Table {
    shapes: BTreeMap<String, Shape>,
    /// The standard codegen's definitions, as it emitted them.
    defs: String,
    /// The parameters of the generic definition being admitted: each is its
    /// own host spelling there.
    scope: Vec<String>,
    /// The conversions the adapters call: type, this render's kind, direction.
    wanted: Vec<(Ty, NTy, bool)>,
}

thread_local! {
    static TABLE: RefCell<Table> = RefCell::new(Table::default());
}

/// A constructor and its fields (label, type): as declared, and as the host
/// source spells them.
type DeclaredCase = (String, Vec<(String, Ty)>);
type SpelledCase = (String, Vec<(String, String)>);
type Declared = Vec<DeclaredCase>;
type Spelled = Vec<SpelledCase>;

/// The table of the render made while this guard lives.
pub(crate) struct Guard(Table);
impl Drop for Guard {
    fn drop(&mut self) {
        TABLE.with(|t| t.replace(std::mem::take(&mut self.0)));
    }
}

/// Carries the definitions of the named types `signature` reaches from the
/// host source, which is rendered only when there is one to carry.
pub(crate) fn install<'a>(
    decls: &[IrTypeDecl], signature: impl Iterator<Item = &'a Ty>, host: impl FnOnce() -> Option<std::rc::Rc<str>>,
) -> Guard {
    let guard = Guard(TABLE.with(|t| t.take()));
    let mut reached = Vec::new();
    for ty in signature {
        reach(ty, decls, &mut reached);
    }
    if reached.is_empty() { return guard; }
    let carried = host().and_then(|text| read(reached, &text)).and_then(admit);
    if carried.is_none() {
        TABLE.with(|t| t.take());
    }
    guard
}

/// One type a signature reaches: the name the host declares it under, its
/// generic parameters, and its cases (`None` where it has no definition).
struct Reached {
    name: String,
    host: String,
    params: Vec<String>,
    structural: bool,
    shape: Option<(bool, Declared)>,
}

/// The types `ty` names, and the ones their fields name.
fn reach(ty: &Ty, decls: &[IrTypeDecl], out: &mut Vec<Reached>) {
    match ty {
        Ty::Named(name, args) => {
            args.iter().for_each(|a| reach(a, decls, out));
            let Some(decl) = decls.iter().find(|d| d.name == *name) else { return };
            if out.iter().any(|o| o.name == name.as_str()) { return; }
            let (params, shape) = (params(decl), declared(decl));
            let fields: Vec<Ty> = shape.iter().flat_map(|(_, cases)| cases).flat_map(|(_, fields)| fields)
                .map(|(_, t)| bound(t, &params, args)).collect();
            out.push(Reached { name: name.to_string(), host: host_name(name.as_str()), params, structural: false, shape });
            fields.iter().for_each(|t| reach(t, decls, out));
        }
        Ty::Record { fields } => {
            fields.iter().for_each(|(_, t)| reach(t, decls, out));
            let Some(Ty::Named(name, _)) = nominal(ty) else { return };
            if out.iter().all(|o| o.name != name.as_str()) { out.push(structural(name.as_str(), fields)); }
        }
        Ty::Applied(_, args) | Ty::Tuple(args) => args.iter().for_each(|a| reach(a, decls, out)),
        _ => {}
    }
}

/// The labels of a structural record in the order the standard codegen
/// declares them — sorted — each with its type.
fn sorted(fields: &[(almide_lang::intern::Sym, Ty)]) -> Vec<(String, &Ty)> {
    let mut fields: Vec<(String, &Ty)> = fields.iter().map(|(label, ty)| (label.to_string(), ty)).collect();
    fields.sort_by(|a, b| a.0.cmp(&b.0));
    fields
}

fn parameter(name: &str) -> Ty {
    Ty::Named(almide_base::intern::sym(name), Vec::new().into())
}

/// A structural record as the standard codegen types it: an instance of the
/// one definition it declares for those labels (`AlmdRec_x_y<T0, T1>`),
/// generic in every field.
pub(crate) fn nominal(ty: &Ty) -> Option<Ty> {
    let Ty::Record { fields } = ty else { return None };
    let fields = sorted(fields);
    let labels: Vec<&str> = fields.iter().map(|(label, _)| label.as_str()).collect();
    let name = format!("AlmdRec_{}", labels.join("_"));
    (!fields.is_empty()).then(|| Ty::Named(almide_base::intern::sym(&name), fields.iter().map(|(_, t)| (*t).clone()).collect::<Vec<_>>().into()))
}

/// The definition `nominal` names an instance of.
fn structural(name: &str, fields: &[(almide_lang::intern::Sym, Ty)]) -> Reached {
    let params: Vec<String> = (0..fields.len()).map(|j| format!("T{j}")).collect();
    let fields = sorted(fields).into_iter().zip(&params).map(|((label, _), p)| (label, parameter(p))).collect();
    Reached { name: name.to_string(), host: name.to_string(), params, structural: true, shape: Some((true, vec![(String::new(), fields)])) }
}

/// The generic parameters of `decl`, in declared order.
fn params(decl: &IrTypeDecl) -> Vec<String> {
    decl.generics.iter().flatten().map(|g| g.name.to_string()).collect()
}

/// `ty`, a field type of a definition with `params`, at the instance `args`.
/// An instance of another arity binds nothing, and is refused where it is
/// spelled (`host_named`).
fn bound(ty: &Ty, params: &[String], args: &[Ty]) -> Ty {
    if params.is_empty() || params.len() != args.len() { return ty.clone(); }
    let subst = params.iter().map(|p| almide_base::intern::sym(p)).zip(args.iter().cloned()).collect();
    crate::lower::substitute_display_type(ty, &subst)
}

/// Whether `decl` is a record, and its cases, a generic parameter left as the
/// declaration names it. An alias has no definition to carry.
fn declared(decl: &IrTypeDecl) -> Option<(bool, Declared)> {
    let labelled = |fields: &[IrFieldDecl]| fields.iter().map(|f| (f.name.to_string(), f.ty.clone())).collect();
    match &decl.kind {
        IrTypeDeclKind::Record { fields } => Some((true, vec![(String::new(), labelled(fields))])),
        IrTypeDeclKind::Variant { cases, .. } => Some((false, cases.iter().map(|c| (c.name.to_string(), match &c.kind {
            IrVariantKind::Unit => Vec::new(),
            IrVariantKind::Tuple { fields } => fields.iter().map(|t| (String::new(), t.clone())).collect(),
            IrVariantKind::Record { fields } => labelled(fields),
        })).collect())),
        IrTypeDeclKind::Alias { .. } => None,
    }
}

/// The name the standard codegen declares a type under: a module's type is
/// mangled, the entry's own keeps its name.
fn host_name(name: &str) -> String {
    if name.contains('.') { format!("almide_rt_{}", almide_base::names::qualified_ident(name)) } else { name.to_string() }
}

/// `text` at its top-level commas.
fn pieces(text: &str) -> Vec<&str> {
    let (mut depth, mut from, mut prev) = (0usize, 0usize, ' ');
    let mut out = Vec::new();
    for (i, c) in text.char_indices() {
        match c {
            '(' | '[' | '<' | '{' => depth += 1,
            '>' if prev == '-' => {}
            ')' | ']' | '>' | '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                out.push(&text[from..i]);
                from = i + 1;
            }
            _ => {}
        }
        prev = c;
    }
    out.push(&text[from..]);
    out.into_iter().map(str::trim).filter(|p| !p.is_empty()).collect()
}

/// `label: type` fields, a `pub` in front or not.
fn labelled(body: &str) -> Option<Vec<(String, String)>> {
    pieces(body).into_iter().map(|p| {
        let (label, ty) = p.strip_prefix("pub ").unwrap_or(p).split_once(':')?;
        Some((label.trim().to_string(), ty.trim().to_string()))
    }).collect()
}

/// One enum case: `Name`, `Name(T, U)` or `Name { a: T }`.
fn case(piece: &str) -> Option<SpelledCase> {
    let Some(at) = piece.find(['(', '{']) else { return Some((piece.to_string(), Vec::new())) };
    let inner = piece[at + 1..].strip_suffix([')', '}'])?;
    let fields = if piece[at..].starts_with('(') {
        pieces(inner).into_iter().map(|t| (String::new(), t.to_string())).collect()
    } else { labelled(inner)? };
    Some((piece[..at].trim().to_string(), fields))
}

/// Where the brace `text` opens with closes.
fn closing(text: &str) -> Option<usize> {
    let mut depth = 0usize;
    for (i, c) in text.char_indices() {
        match c {
            '{' => depth += 1,
            '}' if depth == 1 => return Some(i),
            '}' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    None
}

/// The item `text` declares `host` with — its derive line included — and
/// the cases it spells.
fn definition<'a>(text: &'a str, host: &str, record: bool) -> Option<(&'a str, Spelled)> {
    let head = format!("\npub {} {host} {{", if record { "struct" } else { "enum" });
    // The body opens at the head's last byte, the brace.
    let at = text.find(&head)?;
    let open = at + head.len() - 1;
    let close = open + closing(&text[open..])?;
    let line = text[..at].rfind('\n').map_or(0, |i| i + 1);
    let start = if text[line..].starts_with("#[derive(") { line } else { at + 1 };
    let body = &text[open + 1..close];
    let cases = if record { vec![(String::new(), labelled(body)?)] }
        else { pieces(body).into_iter().map(case).collect::<Option<_>>()? };
    Some((&text[start..=close], cases))
}

/// Every reached type's host name and definition, with the cases each side
/// spells; `None` when one of them has no definition in `text`.
fn read(reached: Vec<Reached>, text: &str) -> Option<Vec<(String, Declared, Spelled)>> {
    let mut table = Table::default();
    let mut both = Vec::new();
    for Reached { name, host, params, structural, shape } in reached {
        let (record, ours) = shape?;
        let generic = if params.is_empty() { host.clone() } else { format!("{host}<{}>", params.join(", ")) };
        let (item, theirs) = definition(text, &generic, record)?;
        writeln!(table.defs, "#[allow(non_camel_case_types)]\n{item}\n").expect("writing to a String cannot fail");
        table.shapes.insert(name.clone(), Shape { host, params, record, structural, cases: Vec::new() });
        both.push((name, ours, theirs));
    }
    TABLE.with(|t| t.replace(table));
    Some(both)
}

/// Holds each carried definition to the declared type, field by field.
fn admit(both: Vec<(String, Declared, Spelled)>) -> Option<()> {
    for (name, ours, theirs) in both {
        if ours.len() != theirs.len() { return None; }
        // A generic definition spells its parameters as themselves.
        let scope = TABLE.with(|t| t.borrow().shapes.get(&name).map(|s| s.params.clone()))?;
        TABLE.with(|t| t.borrow_mut().scope.clone_from(&scope));
        let cases = ours.iter().zip(&theirs).map(|both| agreed(both, &scope)).collect::<Option<Vec<_>>>();
        TABLE.with(|t| t.borrow_mut().scope.clear());
        let cases = cases?;
        TABLE.with(|t| t.borrow_mut().shapes.get_mut(&name).map(|s| s.cases = cases))?;
    }
    Some(())
}

fn agreed(((ctor, fields), (their_ctor, their_fields)): (&DeclaredCase, &SpelledCase), scope: &[String]) -> Option<Case> {
    if ctor != their_ctor || fields.len() != their_fields.len() { return None; }
    let own: Vec<Ty> = scope.iter().map(|p| parameter(p)).collect();
    let fields = fields.iter().zip(their_fields).map(|((label, ty), (their_label, spelled))| {
        let (owned, spelled) = (canon(&host_type(&bound(ty, scope, &own)).ok()?), canon(spelled));
        let boxed = spelled == format!("AlmideBox<{owned}>");
        (label == their_label && (boxed || spelled == owned)).then(|| Slot { label: label.clone(), ty: ty.clone(), boxed })
    }).collect::<Option<Vec<_>>>()?;
    Some(Case { ctor: ctor.clone(), fields })
}

/// The host spelling of the named type `name` at `args`, where its
/// definition is carried.
pub(crate) fn host_named(name: &str, args: &[Ty]) -> Result<String, LowerError> {
    let found = TABLE.with(|t| {
        let t = t.borrow();
        if args.is_empty() && t.scope.iter().any(|p| p == name) { return Some(name.to_string()); }
        t.shapes.get(name).filter(|s| s.params.len() == args.len()).map(|s| s.host.clone())
    });
    let host = found.ok_or_else(|| wall(format!("native: callback type `{name}` has no host definition to carry")))?;
    if args.is_empty() { return Ok(host); }
    Ok(format!("{host}<{}>", args.iter().map(host_type).collect::<Result<Vec<_>, _>>()?.join(", ")))
}

/// The carried definitions as the runtime index reads them: the runtime types
/// they name travel with them. The box of a recursive field does not — the
/// standard codegen's `AlmideBox` owns its cell through a raw pointer and
/// thread-local work queues, which this crate does not hold. `BOX` stands in
/// for it under the same name.
pub(crate) fn named_runtime() -> String {
    TABLE.with(|t| t.borrow().defs.replace("AlmideBox<", "Box<"))
}

/// The box a carried recursive definition names, for the native code that
/// builds and reads one: the constructor, the move out, the dereferences and
/// the derives of the standard codegen's `AlmideBox`, over a plain `Box` and
/// with no `unsafe`. It drops, clones and compares by recursion.
const BOX: &str = "#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AlmideBox<T>(Box<T>);
impl<T> AlmideBox<T> {
    pub fn new(x: T) -> Self { AlmideBox(Box::new(x)) }
    pub fn into_inner(self) -> T { *self.0 }
}
impl<T> std::ops::Deref for AlmideBox<T> {
    type Target = T;
    fn deref(&self) -> &T { &self.0 }
}
impl<T> std::ops::DerefMut for AlmideBox<T> {
    fn deref_mut(&mut self) -> &mut T { &mut self.0 }
}
impl<T: std::fmt::Debug> std::fmt::Debug for AlmideBox<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { (**self).fmt(f) }
}
";

/// `e`, a borrow of a named value on one side, as an owned value of the other.
pub(crate) fn convert(e: &str, ty: &Ty, kind: NTy, to_host: bool) -> Result<String, LowerError> {
    let ty = nominal(ty).unwrap_or_else(|| ty.clone());
    let Ty::Named(name, args) = &ty else { return Err(wall("native: callback conversion of a type with no name")) };
    host_named(name.as_str(), args)?;
    let key = (ty.clone(), kind, to_host);
    let id = TABLE.with(|t| {
        let wanted = &mut t.borrow_mut().wanted;
        wanted.iter().position(|w| *w == key).unwrap_or_else(|| {
            wanted.push(key);
            wanted.len() - 1
        })
    });
    Ok(format!("{}({e})", function(id)))
}

fn function(id: usize) -> String {
    format!("__almide_callback_value_{id}")
}

/// The carried definitions and every conversion the adapters call; a
/// conversion's own fields may call for more.
pub(crate) fn render() -> Result<String, LowerError> {
    let mut out = TABLE.with(|t| t.borrow().defs.clone());
    if out.contains("AlmideBox<") {
        out.push_str(BOX);
    }
    let mut next = 0;
    while let Some((ty, kind, to_host)) = TABLE.with(|t| t.borrow().wanted.get(next).cloned()) {
        out.push_str(&conversion(next, &ty, kind, to_host)?);
        next += 1;
    }
    Ok(out)
}

fn conversion(id: usize, ty: &Ty, kind: NTy, to_host: bool) -> Result<String, LowerError> {
    let (mut shape, args) = match ty {
        Ty::Named(name, args) => TABLE.with(|t| t.borrow().shapes.get(name.as_str()).cloned()).map(|s| (s, args)),
        _ => None,
    }.ok_or_else(|| wall("native: callback conversion of a type with no carried definition"))?;
    // The fields of this instance: each parameter is the argument it names.
    for slot in shape.cases.iter_mut().flat_map(|c| c.fields.iter_mut()) {
        slot.ty = bound(&slot.ty, &shape.params, args);
    }
    let (own, host) = (rec::owned_ty(kind), host_type(ty)?);
    let body = if shape.record { record(&shape, kind, to_host) } else { variant(&shape, kind, to_host) }
        .map_err(|e| wall(format!("native: callback type `{host}` ({}) has no host adapter: {e:?}", ty.display())))?;
    let (from, to) = if to_host { (own.as_str(), host.as_str()) } else { (host.as_str(), own.as_str()) };
    Ok(format!("fn {}(v: &{from}) -> {to} {{\n    {body}\n}}\n", function(id)))
}

/// The kind this render holds each of a case's `n` fields in, and whether it
/// boxes it. A record is the case with no tag.
fn own_fields(kind: NTy, tag: Option<u32>, n: usize) -> Result<Vec<(NTy, bool)>, LowerError> {
    let kinds = match (kind, tag) {
        (NTy::Vec, None) => Some(vec![NTy::I64; n]),
        (NTy::Rec(i), None) if rec::enum_cases(i).is_none() => (0..rec::field_count(i)).map(|k| rec::rec_field(i, k)).collect(),
        (NTy::Rec(i), Some(tag)) => rec::enum_cases(i).and_then(|cases| cases.into_iter().find(|(t, _)| *t == tag)).map(|(_, kinds)| kinds),
        _ => None,
    }.filter(|kinds| kinds.len() == n).ok_or_else(|| wall(format!("this render holds it as {kind:?}, which has no such fields")))?;
    Ok(kinds.into_iter().map(|k| (k, matches!(kind, NTy::Rec(i) if rec::enum_field_boxed(i, k)))).collect())
}

/// One field copied across. `place` borrows it on the side it comes from.
fn crossed(place: &str, slot: &Slot, (kind, own_boxed): (NTy, bool), to_host: bool) -> Result<String, LowerError> {
    let (from_boxed, wrap) = if to_host { (own_boxed, slot.boxed.then_some("AlmideBox")) } else { (slot.boxed, own_boxed.then_some("Box")) };
    let source = if from_boxed { format!("&**{place}") } else { place.to_string() };
    let value = convert_value(&source, &slot.ty, kind, to_host, 0, true)?;
    Ok(match wrap {
        Some(wrap) => format!("{wrap}::new({value})"),
        None => value,
    })
}

/// Where this render holds each of a record's host fields: under the same
/// label where it keeps the labels, at the same position in a slot block,
/// which is in declared order. A structural record's host fields are sorted,
/// so it crosses by label only.
fn positions(shape: &Shape, kind: NTy) -> Result<Vec<usize>, LowerError> {
    let fields = &shape.cases[0].fields;
    let labels = match kind {
        NTy::Rec(i) => rec::record_fields(i),
        _ => None,
    };
    let Some((_, labels)) = labels else {
        return if shape.structural { Err(wall(format!("this render holds it as {kind:?}, with no field labels"))) }
            else { Ok((0..fields.len()).collect()) };
    };
    fields.iter().map(|slot| labels.iter().position(|(label, _)| *label == slot.label)
        .ok_or_else(|| wall(format!("this render holds no field `{}` for it", slot.label)))).collect()
}

fn record(shape: &Shape, kind: NTy, to_host: bool) -> Result<String, LowerError> {
    let fields = &shape.cases[0].fields;
    let slots = kind == NTy::Vec;
    let own = own_fields(kind, None, fields.len())?;
    let mut parts = Vec::new();
    for (slot, j) in fields.iter().zip(positions(shape, kind)?) {
        let own = own[j];
        let place = if !to_host { format!("&v.{}", slot.label) } else if slots { format!("&v[{j}]") } else { format!("&v.f{j}") };
        let value = crossed(&place, slot, own, to_host)?;
        parts.push(if to_host { format!("{}: {value}", slot.label) } else if slots { value } else { format!("f{j}: {value}") });
    }
    let parts = parts.join(", ");
    Ok(if to_host { format!("{} {{ {parts} }}", shape.host) }
        else if slots { format!("vec![{parts}]") }
        else { format!("{} {{ {parts} }}", rec::owned_ty(kind)) })
}

fn variant(shape: &Shape, kind: NTy, to_host: bool) -> Result<String, LowerError> {
    let mut arms = String::new();
    for (tag, case) in shape.cases.iter().enumerate() {
        let binds: Vec<String> = (0..case.fields.len()).map(|j| format!("__case_f{j}")).collect();
        let mut values = Vec::new();
        for ((slot, own), bind) in case.fields.iter().zip(own_fields(kind, Some(tag as u32), case.fields.len())?).zip(&binds) {
            values.push(crossed(bind, slot, own, to_host)?);
        }
        let (theirs, ours) = if to_host { (&values, &binds) } else { (&binds, &values) };
        let (theirs, ours) = (host_case(&shape.host, case, theirs), own_case(kind, tag, ours));
        let (pattern, built) = if to_host { (ours, theirs) } else { (theirs, ours) };
        writeln!(arms, "        {pattern} => {built},").expect("writing to a String cannot fail");
    }
    Ok(format!("match v {{\n{arms}    }}"))
}

fn host_case(host: &str, case: &Case, items: &[String]) -> String {
    let path = format!("{host}::{}", case.ctor);
    if items.is_empty() { return path; }
    if case.fields[0].label.is_empty() { return format!("{path}({})", items.join(", ")); }
    let named: Vec<String> = case.fields.iter().zip(items).map(|(f, item)| format!("{}: {item}", f.label)).collect();
    format!("{path} {{ {} }}", named.join(", "))
}

fn own_case(kind: NTy, tag: usize, items: &[String]) -> String {
    let path = format!("{}::C{tag}", rec::owned_ty(kind));
    if items.is_empty() { path } else { format!("{path}({})", items.join(", ")) }
}
