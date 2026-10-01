//! Structural identity of a record shape against the DECLARED record types
//! (#3189).
//!
//! A record whose type is structural (`Ty::Record` — an un-annotated literal,
//! or one annotated with an anonymous record type) IS a declared record type
//! when it has that type's field names AND field types, in any order: the
//! checker unifies the two, and the wasm leg's type table has always matched
//! on names and types together. Native and the interp keyed the match on the
//! sorted field NAMES alone, so `{ v: Int, n: Int }` beside a declared
//! `type R = { v: String, n: Int }` was built as `R`'s Rust struct (rustc
//! E0308) and repr'd as `R { .. }` — a match on spelling where the full type
//! decides, the #3176 family.
//!
//! [`RecordShapeIndex`] is the one answer both legs read. Field types compare
//! structurally; a declared generic parameter matches any type (and its first
//! binding is reported, so a caller can spell `Box<i64>`); an inference hole
//! (`Unknown`, an un-substituted `TypeVar`) matches anything; a declared alias
//! is seen through; a nominal type matches a structural record (the nested
//! literal inside an outer one is inferred structurally too). When several
//! declared types fit, a concrete one beats a generic one and the FIRST in
//! declaration order wins — the wasm table's rule (its concrete decls take
//! the first indices), now every leg's.

use std::collections::HashMap;
use almide_base::intern::Sym;
use almide_lang::types::Ty;
use crate::{declared_type_name, IrProgram, IrTypeDecl, IrTypeDeclKind};

/// One declared record type, as the index matches it.
#[derive(Debug, Clone)]
pub struct RecordShape {
    /// The IR decl name (`R`, or `m.R` for a module type).
    pub name: Sym,
    /// The caller's spelling of the type (native: the Rust struct name).
    pub label: String,
    /// Field names and declared types, in declaration order.
    pub fields: Vec<(Sym, Ty)>,
    /// The decl's generic parameter names, in declaration order.
    pub generics: Vec<Sym>,
}

/// A match: the declared shape and, per generic parameter, the type the
/// structural record binds it to (`None` when no field pins it).
pub type ShapeMatch<'a> = (&'a RecordShape, Vec<Option<Ty>>);

/// Every declared record type, keyed by its sorted field names.
#[derive(Debug, Clone, Default)]
pub struct RecordShapeIndex {
    by_names: HashMap<Vec<String>, Vec<usize>>,
    shapes: Vec<RecordShape>,
    /// Non-generic alias decl name → its target, seen through when comparing.
    aliases: HashMap<String, Ty>,
}

fn sorted_names<'a>(names: impl Iterator<Item = &'a Sym>) -> Vec<String> {
    let mut v: Vec<String> = names.map(|n| n.to_string()).collect();
    v.sort();
    v
}

impl RecordShapeIndex {
    /// Index the program's record decls (entry first, then each module, in
    /// declaration order), spelling each with `label`.
    pub fn build(program: &IrProgram, label: impl Fn(&IrTypeDecl) -> String) -> Self {
        let mut index = Self::default();
        let decls = program.type_decls.iter()
            .chain(program.modules.iter().flat_map(|m| m.type_decls.iter()));
        for td in decls {
            index.add(td, &label);
        }
        index
    }

    fn add(&mut self, td: &IrTypeDecl, label: &impl Fn(&IrTypeDecl) -> String) {
        let generics: Vec<Sym> = td.generics.iter().flatten().map(|g| g.name).collect();
        match &td.kind {
            IrTypeDeclKind::Record { fields } => {
                let fields: Vec<(Sym, Ty)> = fields.iter().map(|f| (f.name, f.ty.clone())).collect();
                let key = sorted_names(fields.iter().map(|(n, _)| n));
                self.by_names.entry(key).or_default().push(self.shapes.len());
                self.shapes.push(RecordShape { name: td.name, label: label(td), fields, generics });
            }
            IrTypeDeclKind::Alias { target } if generics.is_empty() => {
                self.aliases.insert(td.name.to_string(), target.clone());
            }
            _ => {}
        }
    }

    /// The declared record type a structural record with `fields` IS: same
    /// field names, compatible field types. A concrete decl is preferred to a
    /// generic one; within each, the first in declaration order wins.
    pub fn lookup(&self, fields: &[(Sym, Ty)]) -> Option<ShapeMatch<'_>> {
        self.lookup_kind(fields, false).or_else(|| self.lookup_generic(fields))
    }

    /// [`Self::lookup`] restricted to the GENERIC decls — for a leg that
    /// matches concrete decls by its own representation and instantiates a
    /// generic one on demand (the wasm type table).
    pub fn lookup_generic(&self, fields: &[(Sym, Ty)]) -> Option<ShapeMatch<'_>> {
        self.lookup_kind(fields, true)
    }

    fn lookup_kind(&self, fields: &[(Sym, Ty)], generic: bool) -> Option<ShapeMatch<'_>> {
        let key = sorted_names(fields.iter().map(|(n, _)| n));
        self.by_names.get(&key)?.iter()
            .map(|&i| &self.shapes[i])
            .filter(|shape| shape.generics.is_empty() != generic)
            .find_map(|shape| self.bind(shape, fields).map(|b| (shape, b)))
    }

    /// The declared shape's label for `fields`, if it is one.
    pub fn label_for(&self, fields: &[(Sym, Ty)]) -> Option<String> {
        self.lookup(fields).map(|(s, _)| s.label.clone())
    }

    fn bind(&self, shape: &RecordShape, fields: &[(Sym, Ty)]) -> Option<Vec<Option<Ty>>> {
        let mut m = Matcher { index: self, generics: &shape.generics, bound: vec![None; shape.generics.len()] };
        for (name, decl_ty) in &shape.fields {
            let (_, actual) = fields.iter().find(|(n, _)| n == name)?;
            if !m.fits(decl_ty, actual, 0) {
                return None;
            }
        }
        Some(m.bound)
    }

    fn alias_target(&self, name: &str) -> Option<&Ty> {
        self.aliases.get(name)
            .or_else(|| self.aliases.iter()
                .find(|(k, _)| declared_type_name(k) == declared_type_name(name))
                .map(|(_, t)| t))
    }
}

/// Alias chains deeper than this are treated as a fit (a cycle is a checker
/// error long before codegen; this only keeps the walk finite).
const MAX_DEPTH: usize = 32;

struct Matcher<'a> {
    index: &'a RecordShapeIndex,
    generics: &'a [Sym],
    bound: Vec<Option<Ty>>,
}

impl Matcher<'_> {
    /// Does an `actual` field type fit the `decl` field type?
    fn fits(&mut self, decl: &Ty, actual: &Ty, depth: usize) -> bool {
        if depth > MAX_DEPTH || is_hole(actual) {
            return true;
        }
        if let Some(i) = self.generic_slot(decl) {
            // A parameter binds once; every later field it types must agree
            // (`{ x: T, y: T }` is not `{ x: 1, y: "p" }`).
            return match self.bound[i].clone() {
                Some(bound) => self.fits(&bound, actual, depth + 1),
                None => {
                    self.bound[i] = Some(actual.clone());
                    true
                }
            };
        }
        if is_hole(decl) {
            return true;
        }
        if let Some(d) = self.see_through(decl) {
            return self.fits(&d, actual, depth + 1);
        }
        if let Some(a) = self.see_through(actual) {
            return self.fits(decl, &a, depth + 1);
        }
        self.fits_resolved(decl, actual, depth)
    }

    fn generic_slot(&self, decl: &Ty) -> Option<usize> {
        let name = match decl {
            Ty::TypeVar(n) => *n,
            Ty::Named(n, args) if args.is_empty() => *n,
            _ => return None,
        };
        self.generics.iter().position(|g| *g == name)
    }

    fn see_through(&self, ty: &Ty) -> Option<Ty> {
        match ty {
            Ty::Named(n, args) if args.is_empty() => self.index.alias_target(n.as_str()).cloned(),
            _ => None,
        }
    }

    /// Both sides are concrete and neither is an alias.
    fn fits_resolved(&mut self, decl: &Ty, actual: &Ty, depth: usize) -> bool {
        match (decl, actual) {
            (Ty::Named(a, xs), Ty::Named(b, ys)) => {
                declared_type_name(a.as_str()) == declared_type_name(b.as_str())
                    && self.all_fit(xs, ys, depth)
            }
            (Ty::Named(n, _), Ty::Variant { name, .. }) | (Ty::Variant { name, .. }, Ty::Named(n, _)) => {
                declared_type_name(n.as_str()) == declared_type_name(name.as_str())
            }
            // A nominal record beside a structural one: the nested literal of
            // an outer record is inferred structurally; the checker unified it.
            (Ty::Named(..), Ty::Record { .. } | Ty::OpenRecord { .. })
            | (Ty::Record { .. } | Ty::OpenRecord { .. }, Ty::Named(..)) => true,
            (Ty::Named(..), _) | (_, Ty::Named(..)) => false,
            _ => self.fits_structural(decl, actual, depth),
        }
    }

    fn fits_structural(&mut self, decl: &Ty, actual: &Ty, depth: usize) -> bool {
        match (decl, actual) {
            (Ty::Applied(c, xs), Ty::Applied(d, ys)) => c == d && self.all_fit(xs, ys, depth),
            (Ty::Tuple(xs), Ty::Tuple(ys)) => self.all_fit(xs, ys, depth),
            (Ty::Fn { params: xs, ret: r, .. }, Ty::Fn { params: ys, ret: s, .. }) => {
                self.all_fit(xs, ys, depth) && self.fits(r, s, depth + 1)
            }
            (Ty::Record { fields: xs }, Ty::Record { fields: ys }) => self.records_fit(xs, ys, depth),
            (Ty::OpenRecord { .. }, _) | (_, Ty::OpenRecord { .. }) | (Ty::Union(_), _) | (_, Ty::Union(_)) => true,
            (Ty::Variant { name: a, .. }, Ty::Variant { name: b, .. }) => {
                declared_type_name(a.as_str()) == declared_type_name(b.as_str())
            }
            _ => same_scalar(decl, actual),
        }
    }

    fn all_fit(&mut self, xs: &[Ty], ys: &[Ty], depth: usize) -> bool {
        xs.len() == ys.len() && xs.iter().zip(ys).all(|(x, y)| self.fits(x, y, depth + 1))
    }

    fn records_fit(&mut self, xs: &[(Sym, Ty)], ys: &[(Sym, Ty)], depth: usize) -> bool {
        xs.len() == ys.len()
            && xs.iter().all(|(n, x)| match ys.iter().find(|(m, _)| m == n) {
                Some((_, y)) => self.fits(x, y, depth + 1),
                None => false,
            })
    }
}

/// A type that inference has not pinned: it fits whatever the decl says.
fn is_hole(ty: &Ty) -> bool {
    matches!(ty, Ty::Unknown | Ty::TypeVar(_) | Ty::Never | Ty::ConstParam { .. })
}

/// Leaf equality, with the same-width spellings (`Int64` is `Int`, `Float64`
/// is `Float`) identified.
fn same_scalar(a: &Ty, b: &Ty) -> bool {
    fn canon(t: &Ty) -> &Ty {
        match t {
            Ty::Int64 => &Ty::Int,
            Ty::Float64 => &Ty::Float,
            other => other,
        }
    }
    canon(a) == canon(b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use almide_base::intern::sym;

    fn index(shapes: &[(&str, &[(&str, Ty)], &[&str])]) -> RecordShapeIndex {
        let mut ix = RecordShapeIndex::default();
        for (name, fields, generics) in shapes {
            let fields: Vec<(Sym, Ty)> = fields.iter().map(|(n, t)| (sym(n), t.clone())).collect();
            let key = sorted_names(fields.iter().map(|(n, _)| n));
            ix.by_names.entry(key).or_default().push(ix.shapes.len());
            ix.shapes.push(RecordShape {
                name: sym(name),
                label: name.to_string(),
                fields,
                generics: generics.iter().map(|g| sym(g)).collect(),
            });
        }
        ix
    }

    fn rec(fields: &[(&str, Ty)]) -> Vec<(Sym, Ty)> {
        fields.iter().map(|(n, t)| (sym(n), t.clone())).collect()
    }

    #[test]
    fn same_names_different_types_is_not_the_declared_record() {
        let ix = index(&[("R", &[("v", Ty::String), ("n", Ty::Int)], &[])]);
        assert_eq!(ix.label_for(&rec(&[("v", Ty::Int), ("n", Ty::Int)])), None);
        assert_eq!(ix.label_for(&rec(&[("n", Ty::Int), ("v", Ty::String)])).as_deref(), Some("R"));
    }

    #[test]
    fn the_types_pick_between_two_decls_and_the_first_wins_a_tie() {
        let ix = index(&[
            ("A", &[("x", Ty::Int), ("y", Ty::String)], &[]),
            ("B", &[("x", Ty::String), ("y", Ty::Int)], &[]),
            ("C", &[("x", Ty::String), ("y", Ty::Int)], &[]),
        ]);
        assert_eq!(ix.label_for(&rec(&[("x", Ty::Int), ("y", Ty::String)])).as_deref(), Some("A"));
        assert_eq!(ix.label_for(&rec(&[("x", Ty::String), ("y", Ty::Int)])).as_deref(), Some("B"));
        assert_eq!(ix.label_for(&rec(&[("x", Ty::Int), ("y", Ty::Int)])), None);
    }

    #[test]
    fn a_generic_parameter_binds_and_holes_fit() {
        let ix = index(&[("Box", &[("v", Ty::TypeVar(sym("T"))), ("n", Ty::Int)], &["T"])]);
        let (shape, bound) = ix.lookup(&rec(&[("v", Ty::Bool), ("n", Ty::Int)])).unwrap();
        assert_eq!(shape.label, "Box");
        assert_eq!(bound, vec![Some(Ty::Bool)]);
        assert!(ix.lookup(&rec(&[("v", Ty::Unknown), ("n", Ty::Unknown)])).is_some());
        assert_eq!(ix.label_for(&rec(&[("v", Ty::Int), ("n", Ty::String)])), None);
    }

    #[test]
    fn a_generic_parameter_binds_once() {
        let t = || Ty::TypeVar(sym("T"));
        let ix = index(&[("Pair", &[("x", t()), ("y", t())], &["T"])]);
        assert_eq!(ix.label_for(&rec(&[("x", Ty::Int), ("y", Ty::Int)])).as_deref(), Some("Pair"));
        assert_eq!(ix.label_for(&rec(&[("x", Ty::Int), ("y", Ty::String)])), None);
    }

    #[test]
    fn a_concrete_decl_beats_an_earlier_generic_one() {
        let ix = index(&[
            ("Box", &[("v", Ty::TypeVar(sym("T"))), ("n", Ty::Int)], &["T"]),
            ("R", &[("v", Ty::String), ("n", Ty::Int)], &[]),
        ]);
        assert_eq!(ix.label_for(&rec(&[("v", Ty::String), ("n", Ty::Int)])).as_deref(), Some("R"));
        assert_eq!(ix.label_for(&rec(&[("v", Ty::Int), ("n", Ty::Int)])).as_deref(), Some("Box"));
        assert_eq!(ix.lookup_generic(&rec(&[("v", Ty::String), ("n", Ty::Int)])).map(|(s, _)| s.label.as_str()), Some("Box"));
    }

    #[test]
    fn subset_and_superset_never_match() {
        let ix = index(&[("R", &[("v", Ty::String), ("n", Ty::Int)], &[])]);
        assert_eq!(ix.label_for(&rec(&[("v", Ty::String)])), None);
        assert_eq!(ix.label_for(&rec(&[("v", Ty::String), ("n", Ty::Int), ("k", Ty::Int)])), None);
    }

    #[test]
    fn nested_types_compare_structurally() {
        let list_of = |t: Ty| Ty::list(t);
        let ix = index(&[("R", &[("xs", list_of(Ty::String))], &[])]);
        assert_eq!(ix.label_for(&rec(&[("xs", list_of(Ty::Int))])), None);
        assert_eq!(ix.label_for(&rec(&[("xs", list_of(Ty::String))])).as_deref(), Some("R"));
    }
}
