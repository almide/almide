// set — Rust native implementations
//
// `Set[T]` is an INSERTION-ORDERED unique collection, mirroring the wasm Set
// (a dense de-duplicated list). std's `HashSet` iterates in hash-bucket order
// (randomized per process), which diverges from wasm and is nondeterministic;
// `AlmideSet` keeps first-seen insertion order so native == wasm observably.
// Element bound is `PartialEq` (not `Eq + Hash`), so `Set[Float]` and sets of
// records-without-Hash work too.
//
// Membership: `items` stays the source of truth for order, equality and repr;
// past `ALMIDE_MAP_INDEX_THRESHOLD` the same `AlmideKeyIndex` a map carries
// (compact-ordered-dict: slot table of positions + cached hashes — the type,
// the hasher and the threshold live in map.rs and resolve via flat inlining;
// the `almide_rt_map_` reference also declares the map dependency to
// RUNTIME_DEPS) makes `insert` / `contains` O(1). Element types with no hash
// (Float et al) stay on the linear path. `remove` on an indexed set leaves a
// `None` hole and compacts once the holes outnumber the members — the map's
// tombstones (#3691), amortized O(1) instead of a shift plus a full rebuild.


#[derive(Clone, Debug, Default)]
pub struct AlmideSet<T> {
    items: Vec<Option<T>>,
    dead: usize,
    lookup: AlmideKeyLookup,
}

impl<T> AlmideSet<T> {
    pub fn new() -> Self {
        AlmideSet { items: Vec::new(), dead: 0, lookup: AlmideKeyLookup::Linear }
    }
    pub fn len(&self) -> usize {
        self.items.len() - self.dead
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn iter(&self) -> std::iter::Flatten<std::slice::Iter<'_, Option<T>>> {
        self.items.iter().flatten()
    }
}

impl<T: PartialEq + 'static> AlmideSet<T> {
    /// `Q` is the borrowed member form (`str` for a `String` set), as in map.rs.
    #[inline]
    fn position_with<Q: ?Sized + PartialEq>(&self, value: &Q, h: Option<u64>) -> Option<usize>
    where T: std::borrow::Borrow<Q> {
        let is_v = |e: &Option<T>| matches!(e, Some(x) if x.borrow() == value);
        match (&self.lookup, h) {
            (AlmideKeyLookup::Built(ix), Some(h)) => ix.find(h, |p| is_v(&self.items[p])),
            _ => self.items.iter().position(is_v),
        }
    }

    #[inline]
    fn position<Q: ?Sized + PartialEq + AlmideMapKey>(&self, value: &Q) -> Option<usize>
    where T: std::borrow::Borrow<Q> {
        self.position_with(value, self.lookup.hash_for(value))
    }

    /// A new item was appended: extend the index, or build it at the threshold.
    #[inline]
    fn note_push(&mut self, h: Option<u64>) {
        if let AlmideKeyLookup::Built(ix) = &mut self.lookup {
            if let Some(h) = h {
                ix.push(h);
            }
            return;
        }
        // A linear set has no holes, so member order is position order.
        if matches!(self.lookup, AlmideKeyLookup::Linear) && self.items.len() >= ALMIDE_MAP_INDEX_THRESHOLD {
            self.lookup.build(self.items.iter().flatten(), self.items.len());
        }
    }

    /// Append if absent (preserves first-seen order). Returns true if inserted.
    pub fn insert(&mut self, value: T) -> bool {
        let h = self.lookup.hash_for(&value);
        if self.position_with(&value, h).is_some() {
            return false;
        }
        self.items.push(Some(value));
        self.note_push(h);
        true
    }
    pub fn contains<Q: ?Sized + PartialEq + AlmideMapKey>(&self, value: &Q) -> bool
    where T: std::borrow::Borrow<Q> {
        self.position(value).is_some()
    }
    /// Remove, keeping the order of the survivors: an indexed set leaves a
    /// hole (amortized O(1), #3691), a linear one shifts. Returns true if
    /// present.
    pub fn remove(&mut self, value: &T) -> bool {
        let Some(i) = self.position(value) else { return false };
        let AlmideKeyLookup::Built(ix) = &mut self.lookup else {
            self.items.remove(i);
            return true;
        };
        ix.kill(i);
        self.items[i] = None;
        self.dead += 1;
        if self.dead * 2 > self.items.len() {
            let items = &self.items;
            ix.retain(|p| items[p].is_some());
            self.items.retain(Option::is_some);
            self.dead = 0;
        }
        true
    }
}

// Almide-literal repr for compound string interpolation. Almide has no set
// literal, so a set renders in CONSTRUCTOR form `set.from_list([a, b, c])`
// (empty → `set.from_list([])`). Element order = insertion order, matching the
// wasm dense-list Set walk byte-for-byte.
impl<T: AlmideRepr> AlmideRepr for AlmideSet<T> {
    fn almide_repr(&self) -> String {
        let mut o = String::from("set.from_list([");
        for (i, e) in self.iter().enumerate() {
            if i > 0 { o.push_str(", "); }
            o.push_str(&e.almide_repr());
        }
        o.push_str("])");
        o
    }
}

// Set equality is order-INDEPENDENT (same size + same members), matching both
// std HashSet semantics and the wasm structural Set `==`.
impl<T: PartialEq + 'static> PartialEq for AlmideSet<T> {
    fn eq(&self, other: &Self) -> bool {
        self.len() == other.len() && self.iter().all(|x| other.contains(x))
    }
}

impl<T: PartialEq + 'static> FromIterator<T> for AlmideSet<T> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        let mut s = AlmideSet::new();
        for x in iter {
            s.insert(x);
        }
        s
    }
}

impl<T: PartialEq + 'static, const N: usize> From<[T; N]> for AlmideSet<T> {
    fn from(arr: [T; N]) -> Self {
        arr.into_iter().collect()
    }
}

impl<T> IntoIterator for AlmideSet<T> {
    type Item = T;
    type IntoIter = std::iter::Flatten<std::vec::IntoIter<Option<T>>>;
    fn into_iter(self) -> Self::IntoIter {
        self.items.into_iter().flatten()
    }
}

impl<'a, T> IntoIterator for &'a AlmideSet<T> {
    type Item = &'a T;
    type IntoIter = std::iter::Flatten<std::slice::Iter<'a, Option<T>>>;
    fn into_iter(self) -> Self::IntoIter {
        self.items.iter().flatten()
    }
}

pub fn almide_rt_set_new<T>() -> AlmideSet<T> { AlmideSet::new() }
pub fn almide_rt_set_from_list<T: PartialEq + Clone + 'static>(xs: &[T]) -> AlmideSet<T> { xs.iter().cloned().collect() }
pub fn almide_rt_set_insert<T: PartialEq + Clone + 'static>(s: &AlmideSet<T>, value: T) -> AlmideSet<T> { let mut s = s.clone(); s.insert(value); s }
pub fn almide_rt_set_remove<T: PartialEq + Clone + 'static>(s: &AlmideSet<T>, value: T) -> AlmideSet<T> { let mut s = s.clone(); s.remove(&value); s }
// Owned twins (`@reuse(s)` in stdlib/set.almd): a source at its last use is
// edited in place instead of cloned whole.
pub fn almide_rt_set_insert_owned<T: PartialEq + Clone + 'static>(mut s: AlmideSet<T>, value: T) -> AlmideSet<T> { s.insert(value); s }
pub fn almide_rt_set_remove_owned<T: PartialEq + Clone + 'static>(mut s: AlmideSet<T>, value: T) -> AlmideSet<T> { s.remove(&value); s }
// Borrowed member (`@borrow_ref(value)` in stdlib/set.almd), as map.get is.
pub fn almide_rt_set_contains<T: PartialEq + 'static, Q: ?Sized + PartialEq + AlmideMapKey>(s: &AlmideSet<T>, value: &Q) -> bool
where T: std::borrow::Borrow<Q> { s.contains(value) }
pub fn almide_rt_set_len<T>(s: &AlmideSet<T>) -> i64 { s.len() as i64 }
pub fn almide_rt_set_is_empty<T>(s: &AlmideSet<T>) -> bool { s.is_empty() }
pub fn almide_rt_set_to_list<T: Clone>(s: &AlmideSet<T>) -> Vec<T> { s.iter().cloned().collect() }
// union = a's order, then members of b not already in a (in b's order).
pub fn almide_rt_set_union<T: PartialEq + Clone + 'static>(a: &AlmideSet<T>, b: &AlmideSet<T>) -> AlmideSet<T> {
    let mut r = a.clone();
    for x in b.iter() { r.insert(x.clone()); }
    r
}
// intersection = a's order, members also in b.
pub fn almide_rt_set_intersection<T: PartialEq + Clone + 'static>(a: &AlmideSet<T>, b: &AlmideSet<T>) -> AlmideSet<T> {
    a.iter().filter(|x| b.contains(x)).cloned().collect()
}
// difference = a's order, members not in b.
pub fn almide_rt_set_difference<T: PartialEq + Clone + 'static>(a: &AlmideSet<T>, b: &AlmideSet<T>) -> AlmideSet<T> {
    a.iter().filter(|x| !b.contains(x)).cloned().collect()
}
// symmetric_difference = (a not in b, a's order) then (b not in a, b's order).
pub fn almide_rt_set_symmetric_difference<T: PartialEq + Clone + 'static>(a: &AlmideSet<T>, b: &AlmideSet<T>) -> AlmideSet<T> {
    let mut r: AlmideSet<T> = a.iter().filter(|x| !b.contains(x)).cloned().collect();
    for x in b.iter() { if !a.contains(x) { r.insert(x.clone()); } }
    r
}
pub fn almide_rt_set_is_subset<T: PartialEq + 'static>(a: &AlmideSet<T>, b: &AlmideSet<T>) -> bool { a.iter().all(|x| b.contains(x)) }
pub fn almide_rt_set_is_disjoint<T: PartialEq + 'static>(a: &AlmideSet<T>, b: &AlmideSet<T>) -> bool { a.iter().all(|x| !b.contains(x)) }
pub fn almide_rt_set_filter<T: PartialEq + Clone + 'static>(s: &AlmideSet<T>, f: std::rc::Rc<dyn Fn(T) -> bool>) -> AlmideSet<T> { let f = move |a| f(a); s.iter().filter(|x| f((*x).clone())).cloned().collect() }
pub fn almide_rt_set_map<T: PartialEq + Clone, U: PartialEq + 'static>(s: &AlmideSet<T>, f: std::rc::Rc<dyn Fn(T) -> U>) -> AlmideSet<U> { let f = move |a| f(a); s.iter().cloned().map(f).collect() }
pub fn almide_rt_set_fold<T: Clone, B>(s: &AlmideSet<T>, init: B, f: std::rc::Rc<dyn Fn(B, T) -> B>) -> B { let f = move |a, b| f(a, b); s.iter().cloned().fold(init, f) }
pub fn almide_rt_set_each<T: Clone>(s: &AlmideSet<T>, f: std::rc::Rc<dyn Fn(T)>) { let f = move |a| f(a); for x in s { f(x.clone()); } }
pub fn almide_rt_set_any<T: Clone>(s: &AlmideSet<T>, f: std::rc::Rc<dyn Fn(T) -> bool>) -> bool { let f = move |a| f(a); s.iter().any(|x| f(x.clone())) }
pub fn almide_rt_set_all<T: Clone>(s: &AlmideSet<T>, f: std::rc::Rc<dyn Fn(T) -> bool>) -> bool { let f = move |a| f(a); s.iter().all(|x| f(x.clone())) }
