// set — Rust native implementations
//
// `Set[T]` is an INSERTION-ORDERED unique collection, mirroring the wasm Set
// (a dense de-duplicated list). std's `HashSet` iterates in hash-bucket order
// (randomized per process), which diverges from wasm and is nondeterministic;
// `AlmideSet` keeps first-seen insertion order so native == wasm observably.
// Backed by a `Vec` with linear membership — same representation & complexity
// as the wasm Set. Element bound is `PartialEq` (not `Eq + Hash`), so `Set[Float]`
// and sets of records-without-Hash work too.

#[derive(Clone, Debug, Default)]
pub struct AlmideSet<T> {
    items: Vec<T>,
}

impl<T> AlmideSet<T> {
    pub fn new() -> Self {
        AlmideSet { items: Vec::new() }
    }
    pub fn len(&self) -> usize {
        self.items.len()
    }
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
    pub fn iter(&self) -> std::slice::Iter<'_, T> {
        self.items.iter()
    }
}

impl<T: PartialEq> AlmideSet<T> {
    /// Append if absent (preserves first-seen order). Returns true if inserted.
    pub fn insert(&mut self, value: T) -> bool {
        if self.items.iter().any(|x| x == &value) {
            false
        } else {
            self.items.push(value);
            true
        }
    }
    pub fn contains(&self, value: &T) -> bool {
        self.items.iter().any(|x| x == value)
    }
    /// Remove, keeping the order of the survivors. Returns true if present.
    pub fn remove(&mut self, value: &T) -> bool {
        if let Some(i) = self.items.iter().position(|x| x == value) {
            self.items.remove(i);
            true
        } else {
            false
        }
    }
}

// Set equality is order-INDEPENDENT (same size + same members), matching both
// std HashSet semantics and the wasm structural Set `==`.
impl<T: PartialEq> PartialEq for AlmideSet<T> {
    fn eq(&self, other: &Self) -> bool {
        self.items.len() == other.items.len() && self.items.iter().all(|x| other.contains(x))
    }
}

impl<T: PartialEq> FromIterator<T> for AlmideSet<T> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        let mut s = AlmideSet::new();
        for x in iter {
            s.insert(x);
        }
        s
    }
}

impl<T: PartialEq, const N: usize> From<[T; N]> for AlmideSet<T> {
    fn from(arr: [T; N]) -> Self {
        arr.into_iter().collect()
    }
}

impl<T> IntoIterator for AlmideSet<T> {
    type Item = T;
    type IntoIter = std::vec::IntoIter<T>;
    fn into_iter(self) -> Self::IntoIter {
        self.items.into_iter()
    }
}

impl<'a, T> IntoIterator for &'a AlmideSet<T> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.items.iter()
    }
}

pub fn almide_rt_set_new<T>() -> AlmideSet<T> { AlmideSet::new() }
pub fn almide_rt_set_from_list<T: PartialEq + Clone>(xs: &[T]) -> AlmideSet<T> { xs.iter().cloned().collect() }
pub fn almide_rt_set_insert<T: PartialEq + Clone>(s: &AlmideSet<T>, value: T) -> AlmideSet<T> { let mut s = s.clone(); s.insert(value); s }
pub fn almide_rt_set_remove<T: PartialEq + Clone>(s: &AlmideSet<T>, value: T) -> AlmideSet<T> { let mut s = s.clone(); s.remove(&value); s }
pub fn almide_rt_set_contains<T: PartialEq>(s: &AlmideSet<T>, value: T) -> bool { s.contains(&value) }
pub fn almide_rt_set_len<T>(s: &AlmideSet<T>) -> i64 { s.len() as i64 }
pub fn almide_rt_set_is_empty<T>(s: &AlmideSet<T>) -> bool { s.is_empty() }
pub fn almide_rt_set_to_list<T: Clone>(s: &AlmideSet<T>) -> Vec<T> { s.iter().cloned().collect() }
// union = a's order, then members of b not already in a (in b's order).
pub fn almide_rt_set_union<T: PartialEq + Clone>(a: &AlmideSet<T>, b: &AlmideSet<T>) -> AlmideSet<T> {
    let mut r = a.clone();
    for x in b.iter() { r.insert(x.clone()); }
    r
}
// intersection = a's order, members also in b.
pub fn almide_rt_set_intersection<T: PartialEq + Clone>(a: &AlmideSet<T>, b: &AlmideSet<T>) -> AlmideSet<T> {
    a.iter().filter(|x| b.contains(x)).cloned().collect()
}
// difference = a's order, members not in b.
pub fn almide_rt_set_difference<T: PartialEq + Clone>(a: &AlmideSet<T>, b: &AlmideSet<T>) -> AlmideSet<T> {
    a.iter().filter(|x| !b.contains(x)).cloned().collect()
}
// symmetric_difference = (a not in b, a's order) then (b not in a, b's order).
pub fn almide_rt_set_symmetric_difference<T: PartialEq + Clone>(a: &AlmideSet<T>, b: &AlmideSet<T>) -> AlmideSet<T> {
    let mut r: AlmideSet<T> = a.iter().filter(|x| !b.contains(x)).cloned().collect();
    for x in b.iter() { if !a.contains(x) { r.insert(x.clone()); } }
    r
}
pub fn almide_rt_set_is_subset<T: PartialEq>(a: &AlmideSet<T>, b: &AlmideSet<T>) -> bool { a.iter().all(|x| b.contains(x)) }
pub fn almide_rt_set_is_disjoint<T: PartialEq>(a: &AlmideSet<T>, b: &AlmideSet<T>) -> bool { a.iter().all(|x| !b.contains(x)) }
pub fn almide_rt_set_filter<T: PartialEq + Clone>(s: &AlmideSet<T>, f: std::rc::Rc<dyn Fn(T) -> bool>) -> AlmideSet<T> { let f = move |a| f(a); s.iter().filter(|x| f((*x).clone())).cloned().collect() }
pub fn almide_rt_set_map<T: PartialEq + Clone, U: PartialEq>(s: &AlmideSet<T>, f: std::rc::Rc<dyn Fn(T) -> U>) -> AlmideSet<U> { let f = move |a| f(a); s.iter().cloned().map(f).collect() }
pub fn almide_rt_set_fold<T: Clone, B>(s: &AlmideSet<T>, init: B, f: std::rc::Rc<dyn Fn(B, T) -> B>) -> B { let f = move |a, b| f(a, b); s.iter().cloned().fold(init, f) }
pub fn almide_rt_set_each<T: Clone>(s: &AlmideSet<T>, f: std::rc::Rc<dyn Fn(T)>) { let f = move |a| f(a); for x in s { f(x.clone()); } }
pub fn almide_rt_set_any<T: Clone>(s: &AlmideSet<T>, f: std::rc::Rc<dyn Fn(T) -> bool>) -> bool { let f = move |a| f(a); s.iter().any(|x| f(x.clone())) }
pub fn almide_rt_set_all<T: Clone>(s: &AlmideSet<T>, f: std::rc::Rc<dyn Fn(T) -> bool>) -> bool { let f = move |a| f(a); s.iter().all(|x| f(x.clone())) }
