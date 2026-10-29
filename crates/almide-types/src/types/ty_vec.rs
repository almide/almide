//! `TyVec`: the shared argument list of a composite type (ADR-0030 D5, #4114).
//!
//! A type is a tree, and the same subtree is often referenced twice:
//! `let t1 = (t0, t0)` names `t0`'s type in both slots. With the arguments
//! held in a plain `Vec<Ty>`, every clone copies the whole subtree, so a chain
//! of n such lines builds 2^n nodes — 22 lines took `almide check` to 3.7 GB.
//! `TyVec` holds the list behind an `Arc`: a clone is a pointer copy, so
//! `(t, t)` costs O(1) and the doubling chain stays linear in memory.
//!
//! Each node also caches facts about its subtree, computed once from its
//! children's caches: the node count (saturating) and whether an inference
//! variable (`?N`) occurs in it. Walks that only need those facts — the
//! type-size budget, "is there anything left to resolve here" — read them in
//! O(1) instead of re-walking a subtree that may be shared many times over.
//!
//! The cache is never stale: the only way to change a list is through
//! `DerefMut`, which clones a shared node first (`Arc::make_mut`, so other
//! holders keep the old list) and clears this node's cache. A change deeper
//! down goes through the `DerefMut` of every list on the path to it, so every
//! ancestor's cache is cleared too.
//!
//! Lists are hash-consed: building a list whose elements equal a live list's
//! returns that list, so two types built separately but spelled the same
//! share their structure too, and equality is a pointer comparison in the
//! common case (a structural compare otherwise, so it is always exact).
//! `Debug` and serde see a plain list, so printed and serialized forms are
//! unchanged.

use std::sync::{Arc, OnceLock};

use super::Ty;

#[derive(Clone, Copy, Debug, Default)]
struct Meta {
    /// Nodes in the elements' subtrees (the list node itself not counted).
    size: u64,
    /// An inference variable (`?N`) occurs in some element.
    infer_var: bool,
    /// Any type variable (a generic `T` or an inference `?N`) occurs.
    typevar: bool,
    /// The structural hash of the elements, the interning key.
    hash: u64,
}

fn compute_meta(items: &[Ty]) -> Meta {
    use std::hash::{Hash, Hasher};
    let mut m = Meta::default();
    let mut h = std::collections::hash_map::DefaultHasher::new();
    items.len().hash(&mut h);
    for t in items {
        m.size = m.size.saturating_add(t.tree_size());
        m.infer_var |= t.has_infer_var();
        m.typevar |= t.has_typevar();
        structural_hash(t).hash(&mut h);
    }
    m.hash = h.finish();
    m
}

/// A hash of `t`'s structure, consistent with `PartialEq`: a shared list
/// contributes its cached hash, so this is O(1) per list.
pub(crate) fn structural_hash(t: &Ty) -> u64 {
    use std::hash::{Hash, Hasher};
    use super::VariantPayload;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    std::mem::discriminant(t).hash(&mut h);
    match t {
        Ty::Applied(id, args) => {
            id.hash(&mut h);
            args.node_hash().hash(&mut h);
        }
        Ty::Tuple(args) | Ty::Union(args) => args.node_hash().hash(&mut h),
        Ty::Named(n, args) => {
            n.hash(&mut h);
            args.node_hash().hash(&mut h);
        }
        Ty::Record { fields } | Ty::OpenRecord { fields } => {
            for (n, f) in fields {
                n.hash(&mut h);
                structural_hash(f).hash(&mut h);
            }
        }
        Ty::Variant { name, cases } => {
            name.hash(&mut h);
            for c in cases {
                c.name.hash(&mut h);
                match &c.payload {
                    VariantPayload::Unit => 0u8.hash(&mut h),
                    VariantPayload::Tuple(ts) => ts.iter().for_each(|x| structural_hash(x).hash(&mut h)),
                    VariantPayload::Record(fs) => fs.iter().for_each(|(n, x)| {
                        n.hash(&mut h);
                        structural_hash(x).hash(&mut h);
                    }),
                }
            }
        }
        Ty::Fn { params, ret, is_effect } => {
            params.iter().for_each(|x| structural_hash(x).hash(&mut h));
            structural_hash(ret).hash(&mut h);
            is_effect.hash(&mut h);
        }
        Ty::TypeVar(n) => n.hash(&mut h),
        Ty::ConstParam { name, ty } => {
            name.hash(&mut h);
            structural_hash(ty).hash(&mut h);
        }
        Ty::ConstValue { ty, value } => {
            structural_hash(ty).hash(&mut h);
            value.hash(&mut h);
        }
        _ => {}
    }
    h.finish()
}

/// The live lists by structural hash (ADR-0030 D5 hash-consing). Weak, so a
/// list no type holds any more is freed; dead entries are swept as a bucket
/// grows.
fn interner() -> &'static std::sync::Mutex<std::collections::HashMap<u64, Vec<std::sync::Weak<Node>>>> {
    static T: OnceLock<std::sync::Mutex<std::collections::HashMap<u64, Vec<std::sync::Weak<Node>>>>> = OnceLock::new();
    T.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

#[derive(Clone, Default)]
struct Node {
    items: Vec<Ty>,
    meta: OnceLock<Meta>,
}

/// A composite type's element list, shared on clone (see the module doc).
#[derive(Default)]
pub struct TyVec(Arc<Node>);

impl Clone for TyVec {
    fn clone(&self) -> Self {
        // The negative control of the sharing gate: copy the whole tree,
        // as a `Vec<Ty>` did.
        if almide_base::budget::type_sharing_disabled() {
            return TyVec::from(self.0.items.clone());
        }
        TyVec(Arc::clone(&self.0))
    }
}

impl TyVec {
    pub fn new() -> Self {
        TyVec::default()
    }

    fn meta(&self) -> Meta {
        *self.0.meta.get_or_init(|| compute_meta(&self.0.items))
    }

    /// The structural hash of the elements. O(1) once computed.
    pub fn node_hash(&self) -> u64 {
        self.meta().hash
    }

    /// Nodes in the elements' subtrees, saturating. O(1) once computed.
    pub fn subtree_size(&self) -> u64 {
        self.meta().size
    }

    /// Does an inference variable (`?N`) occur in some element? O(1) once
    /// computed.
    pub fn has_infer_var(&self) -> bool {
        self.meta().infer_var
    }

    /// Does any type variable occur in some element? O(1) once computed.
    pub fn has_typevar(&self) -> bool {
        self.meta().typevar
    }

    /// The same shared list (a pointer comparison).
    pub fn shares(&self, other: &TyVec) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }

    /// The identity of the shared list, for walks that visit each shared
    /// list once.
    pub fn ptr_id(&self) -> usize {
        Arc::as_ptr(&self.0) as usize
    }

    pub fn into_vec(self) -> Vec<Ty> {
        match Arc::try_unwrap(self.0) {
            Ok(node) => node.items,
            Err(shared) => shared.items.clone(),
        }
    }
}

impl std::ops::Deref for TyVec {
    type Target = Vec<Ty>;
    fn deref(&self) -> &Vec<Ty> {
        &self.0.items
    }
}

impl std::ops::DerefMut for TyVec {
    fn deref_mut(&mut self) -> &mut Vec<Ty> {
        if Arc::get_mut(&mut self.0).is_none() {
            almide_base::budget::note_type_list_built();
        }
        let node = Arc::make_mut(&mut self.0);
        node.meta = OnceLock::new();
        &mut node.items
    }
}

impl PartialEq for TyVec {
    fn eq(&self, other: &Self) -> bool {
        self.shares(other) || self.0.items == other.0.items
    }
}

impl PartialEq<Vec<Ty>> for TyVec {
    fn eq(&self, other: &Vec<Ty>) -> bool {
        &self.0.items == other
    }
}

impl std::fmt::Debug for TyVec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.items.fmt(f)
    }
}

impl From<Vec<Ty>> for TyVec {
    fn from(items: Vec<Ty>) -> Self {
        let meta = compute_meta(&items);
        // The negative control of the sharing gate builds every list anew.
        if almide_base::budget::type_sharing_disabled() {
            almide_base::budget::note_type_list_built();
            let cell = OnceLock::new();
            let _ = cell.set(meta);
            return TyVec(Arc::new(Node { items, meta: cell }));
        }
        let mut table = interner().lock().unwrap_or_else(|e| e.into_inner());
        let bucket = table.entry(meta.hash).or_default();
        for w in bucket.iter() {
            if let Some(live) = w.upgrade() {
                if live.items == items {
                    return TyVec(live);
                }
            }
        }
        if bucket.len() >= 8 {
            bucket.retain(|w| w.strong_count() > 0);
        }
        almide_base::budget::note_type_list_built();
        let cell = OnceLock::new();
        let _ = cell.set(meta);
        let node = Arc::new(Node { items, meta: cell });
        bucket.push(Arc::downgrade(&node));
        TyVec(node)
    }
}

impl From<&[Ty]> for TyVec {
    fn from(items: &[Ty]) -> Self {
        TyVec::from(items.to_vec())
    }
}

impl From<&Vec<Ty>> for TyVec {
    fn from(items: &Vec<Ty>) -> Self {
        TyVec::from(items.clone())
    }
}

impl<const N: usize> From<[Ty; N]> for TyVec {
    fn from(items: [Ty; N]) -> Self {
        TyVec::from(Vec::from(items))
    }
}

impl From<TyVec> for Vec<Ty> {
    fn from(v: TyVec) -> Self {
        v.into_vec()
    }
}

impl FromIterator<Ty> for TyVec {
    fn from_iter<I: IntoIterator<Item = Ty>>(iter: I) -> Self {
        TyVec::from(iter.into_iter().collect::<Vec<Ty>>())
    }
}

impl IntoIterator for TyVec {
    type Item = Ty;
    type IntoIter = std::vec::IntoIter<Ty>;
    fn into_iter(self) -> Self::IntoIter {
        self.into_vec().into_iter()
    }
}

impl<'a> IntoIterator for &'a TyVec {
    type Item = &'a Ty;
    type IntoIter = std::slice::Iter<'a, Ty>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.items.iter()
    }
}

impl<'a> IntoIterator for &'a mut TyVec {
    type Item = &'a mut Ty;
    type IntoIter = std::slice::IterMut<'a, Ty>;
    fn into_iter(self) -> Self::IntoIter {
        std::ops::DerefMut::deref_mut(self).iter_mut()
    }
}

impl serde::Serialize for TyVec {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.0.items.serialize(s)
    }
}

impl<'de> serde::Deserialize<'de> for TyVec {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Vec::<Ty>::deserialize(d).map(TyVec::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doubling(n: usize) -> Ty {
        let mut t = Ty::Int;
        for _ in 0..n {
            t = Ty::Tuple(TyVec::from(vec![t.clone(), t]));
        }
        t
    }

    #[test]
    fn a_doubled_type_shares_its_halves() {
        let t = doubling(60);
        // 2^61 - 1 nodes, counted from the caches, built in 60 steps.
        assert_eq!(t.tree_size(), (1u64 << 61) - 1);
        let Ty::Tuple(halves) = &t else { unreachable!() };
        let (Ty::Tuple(a), Ty::Tuple(b)) = (&halves[0], &halves[1]) else { unreachable!() };
        assert!(a.shares(b), "the two halves are one list");
        assert_eq!(t.clone(), t, "equality of shared lists is a pointer compare");
    }

    #[test]
    fn a_change_clears_the_cache_on_the_path() {
        let mut t = doubling(3);
        assert!(!t.has_infer_var());
        let before = t.tree_size();
        if let Ty::Tuple(xs) = &mut t {
            xs[0] = Ty::TypeVar(almide_base::intern::sym("?7"));
        }
        assert!(t.has_infer_var());
        assert!(t.tree_size() < before);
    }
}
