// map extern — Rust native implementations
// TOML templates use &{m} for read-only, {m} for consuming
//
// `Map[K,V]` is an INSERTION-ORDERED map, mirroring the wasm Map's intended
// insertion order so native == wasm observably (std HashMap iterates in
// hash-bucket order, randomized per process). `AlmideMap` is a Vec<(K,V)>
// keyed by first-seen insertion order: insert updates a key's value in place
// (keeping its position) and appends new keys; remove preserves the order of
// survivors. Key bound is `PartialEq` (not `Eq + Hash`) — same as the wasm
// keyed lookup contract, and lets non-Hash keys work. Backed by linear scan,
// matching the persistent (clone-on-write) op model where each op is already
// O(n) from the clone.

#[derive(Clone, Debug, Default)]
pub struct AlmideMap<K, V> {
    entries: Vec<(K, V)>,
}

impl<K, V> AlmideMap<K, V> {
    pub fn new() -> Self {
        AlmideMap { entries: Vec::new() }
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    pub fn keys(&self) -> impl Iterator<Item = &K> {
        self.entries.iter().map(|(k, _)| k)
    }
    pub fn values(&self) -> impl Iterator<Item = &V> {
        self.entries.iter().map(|(_, v)| v)
    }
    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.entries.iter().map(|(k, v)| (k, v))
    }
    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

impl<K: PartialEq, V> AlmideMap<K, V> {
    pub fn get(&self, k: &K) -> Option<&V> {
        self.entries.iter().find(|(ek, _)| ek == k).map(|(_, v)| v)
    }
    pub fn get_mut(&mut self, k: &K) -> Option<&mut V> {
        self.entries.iter_mut().find(|(ek, _)| ek == k).map(|(_, v)| v)
    }
    pub fn contains_key(&self, k: &K) -> bool {
        self.entries.iter().any(|(ek, _)| ek == k)
    }
    /// Insert: update the value in place if the key exists (preserving its
    /// position), else append the new entry. Matches insertion-order semantics.
    pub fn insert(&mut self, k: K, v: V) {
        if let Some(slot) = self.entries.iter_mut().find(|(ek, _)| ek == &k) {
            slot.1 = v;
        } else {
            self.entries.push((k, v));
        }
    }
    /// Remove, keeping the order of the remaining entries.
    pub fn remove(&mut self, k: &K) {
        if let Some(i) = self.entries.iter().position(|(ek, _)| ek == k) {
            self.entries.remove(i);
        }
    }
}

// Almide-literal repr for compound string interpolation: `["a": 1, "b": 2]`
// (brackets, Swift-style), empty → `[:]`, keys rendered in their own literal
// form (string keys quoted, int keys bare). Pair order = insertion order, so the
// output matches the wasm compact-ordered-dict walk byte-for-byte.
impl<K: AlmideRepr, V: AlmideRepr> AlmideRepr for AlmideMap<K, V> {
    fn almide_repr(&self) -> String {
        if self.entries.is_empty() {
            return "[:]".to_string();
        }
        let mut o = String::from("[");
        for (i, (k, v)) in self.entries.iter().enumerate() {
            if i > 0 { o.push_str(", "); }
            o.push_str(&k.almide_repr());
            o.push_str(": ");
            o.push_str(&v.almide_repr());
        }
        o.push(']');
        o
    }
}

// Map equality is order-INDEPENDENT (same size + same key/value pairs), matching
// std HashMap and the wasm structural Map `==`.
impl<K: PartialEq, V: PartialEq> PartialEq for AlmideMap<K, V> {
    fn eq(&self, other: &Self) -> bool {
        self.entries.len() == other.entries.len()
            && self.entries.iter().all(|(k, v)| other.get(k) == Some(v))
    }
}

impl<K: PartialEq, V> FromIterator<(K, V)> for AlmideMap<K, V> {
    fn from_iter<I: IntoIterator<Item = (K, V)>>(iter: I) -> Self {
        let mut m = AlmideMap::new();
        for (k, v) in iter {
            m.insert(k, v);
        }
        m
    }
}

impl<K: PartialEq, V, const N: usize> From<[(K, V); N]> for AlmideMap<K, V> {
    fn from(arr: [(K, V); N]) -> Self {
        arr.into_iter().collect()
    }
}

impl<K, V> IntoIterator for AlmideMap<K, V> {
    type Item = (K, V);
    type IntoIter = std::vec::IntoIter<(K, V)>;
    fn into_iter(self) -> Self::IntoIter {
        self.entries.into_iter()
    }
}

pub fn almide_rt_map_new<K, V>() -> AlmideMap<K, V> { AlmideMap::new() }
pub fn almide_rt_map_len<K, V>(m: &AlmideMap<K, V>) -> i64 { m.len() as i64 }
pub fn almide_rt_map_is_empty<K, V>(m: &AlmideMap<K, V>) -> bool { m.is_empty() }
pub fn almide_rt_map_get<K: PartialEq, V: Clone>(m: &AlmideMap<K, V>, k: K) -> Option<V> { m.get(&k).cloned() }
pub fn almide_rt_map_get_or<K: PartialEq, V: Clone>(m: &AlmideMap<K, V>, k: K, default: V) -> V { m.get(&k).cloned().unwrap_or(default) }
pub fn almide_rt_map_set<K: PartialEq + Clone, V: Clone>(m: &AlmideMap<K, V>, k: K, v: V) -> AlmideMap<K, V> { let mut r = m.clone(); r.insert(k, v); r }
pub fn almide_rt_map_remove<K: PartialEq + Clone, V: Clone>(m: &AlmideMap<K, V>, k: K) -> AlmideMap<K, V> { let mut r = m.clone(); r.remove(&k); r }
pub fn almide_rt_map_contains<K: PartialEq, V>(m: &AlmideMap<K, V>, k: K) -> bool { m.contains_key(&k) }
pub fn almide_rt_map_keys<K: Clone, V>(m: &AlmideMap<K, V>) -> Vec<K> { m.keys().cloned().collect() }
pub fn almide_rt_map_values<K, V: Clone>(m: &AlmideMap<K, V>) -> Vec<V> { m.values().cloned().collect() }
pub fn almide_rt_map_entries<K: Clone, V: Clone>(m: &AlmideMap<K, V>) -> Vec<(K, V)> { m.iter().map(|(k, v)| (k.clone(), v.clone())).collect() }
pub fn almide_rt_map_merge<K: PartialEq + Clone, V: Clone>(a: &AlmideMap<K, V>, b: &AlmideMap<K, V>) -> AlmideMap<K, V> { let mut r = a.clone(); for (k, v) in b.iter() { r.insert(k.clone(), v.clone()); } r }

pub fn almide_rt_map_filter<K: PartialEq + Clone, V: Clone>(m: &AlmideMap<K, V>, f: std::rc::Rc<dyn Fn(K, V) -> bool>) -> AlmideMap<K, V> {
    let f = move |a, b| f(a, b);
    m.iter().filter(|(k, v)| f((*k).clone(), (*v).clone())).map(|(k, v)| (k.clone(), v.clone())).collect()
}

pub fn almide_rt_map_map_values<K: PartialEq + Clone, V: Clone, W>(m: &AlmideMap<K, V>, f: std::rc::Rc<dyn Fn(V) -> W>) -> AlmideMap<K, W> {
    let f = move |a| f(a);
    m.iter().map(|(k, v)| (k.clone(), f((*v).clone()))).collect()
}

pub fn almide_rt_map_from_entries<K: PartialEq, V>(entries: Vec<(K, V)>) -> AlmideMap<K, V> { entries.into_iter().collect() }
pub fn almide_rt_map_from_list<K: PartialEq + Clone, V: Clone>(keys: &[K], values: &[V]) -> AlmideMap<K, V> { keys.iter().cloned().zip(values.iter().cloned()).collect() }

pub fn almide_rt_map_fold<K: Clone, V: Clone, A>(m: &AlmideMap<K, V>, init: A, f: std::rc::Rc<dyn Fn(A, K, V) -> A>) -> A {
    let f = move |a, k, v| f(a, k, v);
    let mut acc = init;
    for (k, v) in m.iter() { acc = f(acc, k.clone(), v.clone()); }
    acc
}
pub fn almide_rt_map_any<K: Clone, V: Clone>(m: &AlmideMap<K, V>, f: std::rc::Rc<dyn Fn(K, V) -> bool>) -> bool {
    let f = move |a, b| f(a, b);
    m.iter().any(|(k, v)| f(k.clone(), v.clone()))
}
pub fn almide_rt_map_all<K: Clone, V: Clone>(m: &AlmideMap<K, V>, f: std::rc::Rc<dyn Fn(K, V) -> bool>) -> bool {
    let f = move |a, b| f(a, b);
    m.iter().all(|(k, v)| f(k.clone(), v.clone()))
}
pub fn almide_rt_map_count<K: Clone, V: Clone>(m: &AlmideMap<K, V>, f: std::rc::Rc<dyn Fn(K, V) -> bool>) -> i64 {
    let f = move |a, b| f(a, b);
    m.iter().filter(|&(k, v)| f(k.clone(), v.clone())).count() as i64
}
pub fn almide_rt_map_each<K: Clone, V: Clone>(m: &AlmideMap<K, V>, f: std::rc::Rc<dyn Fn(K, V)>) {
    let f = move |a, b| f(a, b);
    for (k, v) in m.iter() { f(k.clone(), v.clone()); }
}
pub fn almide_rt_map_find<K: Clone + PartialEq, V: Clone>(m: &AlmideMap<K, V>, f: std::rc::Rc<dyn Fn(K, V) -> bool>) -> Option<(K, V)> {
    let f = move |a, b| f(a, b);
    m.iter().find(|&(k, v)| f(k.clone(), v.clone())).map(|(k, v)| (k.clone(), v.clone()))
}
pub fn almide_rt_map_update<K: PartialEq + Clone, V: Clone>(m: &AlmideMap<K, V>, key: K, f: std::rc::Rc<dyn Fn(V) -> V>) -> AlmideMap<K, V> {
    let f = move |a| f(a);
    let mut m = m.clone();
    if let Some(v) = m.get(&key).cloned() { m.insert(key, f(v)); }
    m
}

// ── Mutable operations ──

pub fn almide_rt_map_insert<K: PartialEq, V>(m: &mut AlmideMap<K, V>, k: K, v: V) { m.insert(k, v); }
pub fn almide_rt_map_delete<K: PartialEq, V>(m: &mut AlmideMap<K, V>, k: K) { m.remove(&k); }
pub fn almide_rt_map_clear<K, V>(m: &mut AlmideMap<K, V>) { m.clear(); }
