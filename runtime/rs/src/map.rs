// map extern — Rust native implementations
// TOML templates use &{m} for read-only, {m} for consuming
//
// `Map[K,V]` is an INSERTION-ORDERED map, mirroring the wasm Map's intended
// insertion order so native == wasm observably (std HashMap iterates in
// hash-bucket order, randomized per process). `AlmideMap` is a Vec<(K,V)>
// keyed by first-seen insertion order: insert updates a key's value in place
// (keeping its position) and appends new keys; remove preserves the order of
// survivors. Key bound is `PartialEq` (not `Eq + Hash`) — same as the wasm
// keyed lookup contract, and lets non-Hash keys work.
//
// Lookup: `entries` stays the single source of truth for order, equality and
// repr, but once a map grows past `ALMIDE_MAP_INDEX_THRESHOLD` a sidecar hash index
// (key fingerprint → entry positions) makes `get` / `contains` / mutable
// `insert` O(1) instead of O(n) — without it, the everyday
// `for … { map.insert(m, k, v) }` build loop and lookup loops go quadratic.
// Only key types with a fingerprint (see `key_fingerprint`) are indexed;
// everything else (notably Float keys, where `NaN != NaN` must keep behaving
// exactly like the linear scan) stays on the linear path. The persistent ops
// (`map.set` et al) still clone O(n) per op — the index does not change that,
// it changes the read side and the mutable-insert side.

/// Entry count at which an indexable map builds its sidecar hash index.
/// Below this a linear scan over `Vec<(K, V)>` is faster than hashing.
/// Shared with set.rs via flat inlining (set's source references the
/// `almide_rt_map_`-prefixed fn below, which is what RUNTIME_DEPS keys on).
pub const ALMIDE_MAP_INDEX_THRESHOLD: usize = 16;

/// Fingerprint for the key types the sidecar index supports. `None` = this
/// key type stays on the linear path. Equality is always re-confirmed with
/// `PartialEq` after a fingerprint hit, so a collision can never produce a
/// wrong answer — only an extra comparison. The `almide_rt_map_` name is
/// deliberate: it is how build.rs's RUNTIME_DEPS extraction learns that a
/// module referencing this helper (set.rs) needs map's source spliced in.
pub fn almide_rt_map_key_fingerprint<K: 'static>(k: &K) -> Option<u64> {
    use std::hash::{Hash, Hasher};
    let a = k as &dyn std::any::Any;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    if let Some(x) = a.downcast_ref::<i64>() {
        x.hash(&mut h);
    } else if let Some(s) = a.downcast_ref::<String>() {
        s.hash(&mut h);
    } else if let Some(b) = a.downcast_ref::<bool>() {
        b.hash(&mut h);
    } else if let Some(t) = a.downcast_ref::<(i64, i64)>() {
        t.hash(&mut h);
    } else if let Some(t) = a.downcast_ref::<(String, String)>() {
        t.hash(&mut h);
    } else {
        return None;
    }
    Some(h.finish())
}

#[derive(Clone, Debug, Default)]
pub struct AlmideMap<K, V> {
    entries: Vec<(K, V)>,
    /// fingerprint → position in `entries`. `Some` only after the map has
    /// crossed `ALMIDE_MAP_INDEX_THRESHOLD` with a fingerprintable key type.
    /// Flat (one position per fingerprint, last write wins) so cloning it is
    /// a single memcpy-class copy — the persistent ops clone per call. A
    /// fingerprint collision merely evicts one key from the index; the probe
    /// falls back to the linear scan in that case, so answers never change.
    index: Option<std::collections::HashMap<u64, u32>>,
}

impl<K, V> AlmideMap<K, V> {
    pub fn new() -> Self {
        AlmideMap { entries: Vec::new(), index: None }
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
        self.index = None;
    }
}

impl<K: PartialEq + 'static, V> AlmideMap<K, V> {
    /// Position of `k` in `entries`, via the index when present.
    fn position(&self, k: &K) -> Option<usize> {
        if let Some(idx) = &self.index {
            if let Some(fp) = almide_rt_map_key_fingerprint(k) {
                return match idx.get(&fp) {
                    // Absent fingerprint = absent key (the index covers every
                    // entry; a same-key probe always recomputes the same fp).
                    None => None,
                    Some(&p) if self.entries[p as usize].0 == *k => Some(p as usize),
                    // fp present but holding a different key: a collision
                    // evicted `k` (or `k` is absent) — only the linear scan
                    // can tell, and only in this vanishingly rare branch.
                    Some(_) => self.entries.iter().position(|(ek, _)| ek == k),
                };
            }
        }
        self.entries.iter().position(|(ek, _)| ek == k)
    }

    /// Build the index over the current entries if the map has grown past
    /// the threshold and the key type is fingerprintable.
    fn maybe_build_index(&mut self) {
        if self.index.is_some() || self.entries.len() < ALMIDE_MAP_INDEX_THRESHOLD {
            return;
        }
        let mut idx: std::collections::HashMap<u64, u32> = std::collections::HashMap::with_capacity(self.entries.len());
        for (i, (k, _)) in self.entries.iter().enumerate() {
            // One un-fingerprintable key means the whole key type is —
            // stay on the linear path for good.
            let Some(fp) = almide_rt_map_key_fingerprint(k) else { return };
            idx.insert(fp, i as u32);
        }
        self.index = Some(idx);
    }

    /// Recompute the index from `entries` (positions shifted after a remove).
    fn rebuild_index(&mut self) {
        if self.index.is_none() {
            return;
        }
        self.index = None;
        self.maybe_build_index();
    }

    pub fn get(&self, k: &K) -> Option<&V> {
        self.position(k).map(|i| &self.entries[i].1)
    }
    pub fn get_mut(&mut self, k: &K) -> Option<&mut V> {
        self.position(k).map(|i| &mut self.entries[i].1)
    }
    pub fn contains_key(&self, k: &K) -> bool {
        self.position(k).is_some()
    }
    /// Insert: update the value in place if the key exists (preserving its
    /// position), else append the new entry. Matches insertion-order semantics.
    pub fn insert(&mut self, k: K, v: V) {
        if let Some(i) = self.position(&k) {
            self.entries[i].1 = v;
            return;
        }
        if let Some(idx) = self.index.as_mut() {
            if let Some(fp) = almide_rt_map_key_fingerprint(&k) {
                idx.insert(fp, self.entries.len() as u32);
            }
        }
        self.entries.push((k, v));
        self.maybe_build_index();
    }
    /// Remove, keeping the order of the remaining entries.
    pub fn remove(&mut self, k: &K) {
        if let Some(i) = self.position(k) {
            self.entries.remove(i);
            self.rebuild_index();
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
impl<K: PartialEq + 'static, V: PartialEq> PartialEq for AlmideMap<K, V> {
    fn eq(&self, other: &Self) -> bool {
        self.entries.len() == other.entries.len()
            && self.entries.iter().all(|(k, v)| other.get(k) == Some(v))
    }
}

impl<K: PartialEq + 'static, V> FromIterator<(K, V)> for AlmideMap<K, V> {
    fn from_iter<I: IntoIterator<Item = (K, V)>>(iter: I) -> Self {
        let mut m = AlmideMap::new();
        for (k, v) in iter {
            m.insert(k, v);
        }
        m
    }
}

impl<K: PartialEq + 'static, V, const N: usize> From<[(K, V); N]> for AlmideMap<K, V> {
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
pub fn almide_rt_map_get<K: PartialEq + 'static, V: Clone>(m: &AlmideMap<K, V>, k: K) -> Option<V> { m.get(&k).cloned() }
pub fn almide_rt_map_get_or<K: PartialEq + 'static, V: Clone>(m: &AlmideMap<K, V>, k: K, default: V) -> V { m.get(&k).cloned().unwrap_or(default) }
// Consuming (@consume(m) in stdlib/map.almd): a caller whose map is dead at
// the call moves it in and this is one hash insert; a caller that still uses
// the source gets its clone inserted by pass_clone at the call site. The
// borrowing `let mut r = m.clone()` form cloned the WHOLE map on every call —
// the fold-accumulator hot loop (#1143) paid it per line. Composes with the
// sidecar index: the moved-in map keeps its index, so the insert is O(1) for
// fingerprintable keys.
pub fn almide_rt_map_set<K: PartialEq + Clone + 'static, V: Clone>(mut m: AlmideMap<K, V>, k: K, v: V) -> AlmideMap<K, V> { m.insert(k, v); m }
// Single-scan insert-or-update, consuming like map_set: present → f(old)
// in place (position preserved), absent → append init. Lookup and append go
// through the index-aware `position`/`insert` — a raw `entries.push` here
// would leave a present key out of the index, and a later probe would
// wrongly report it absent.
pub fn almide_rt_map_upsert<K: PartialEq + 'static, V: Clone>(mut m: AlmideMap<K, V>, k: K, init: V, f: std::rc::Rc<dyn Fn(V) -> V>) -> AlmideMap<K, V> {
    if let Some(v) = m.get_mut(&k) {
        let old = v.clone();
        *v = f(old);
    } else {
        m.insert(k, init);
    }
    m
}
pub fn almide_rt_map_remove<K: PartialEq + Clone + 'static, V: Clone>(m: &AlmideMap<K, V>, k: K) -> AlmideMap<K, V> { let mut r = m.clone(); r.remove(&k); r }
pub fn almide_rt_map_contains<K: PartialEq + 'static, V>(m: &AlmideMap<K, V>, k: K) -> bool { m.contains_key(&k) }
pub fn almide_rt_map_keys<K: Clone, V>(m: &AlmideMap<K, V>) -> Vec<K> { m.keys().cloned().collect() }
pub fn almide_rt_map_values<K, V: Clone>(m: &AlmideMap<K, V>) -> Vec<V> { m.values().cloned().collect() }
pub fn almide_rt_map_entries<K: Clone, V: Clone>(m: &AlmideMap<K, V>) -> Vec<(K, V)> { m.iter().map(|(k, v)| (k.clone(), v.clone())).collect() }
pub fn almide_rt_map_merge<K: PartialEq + Clone + 'static, V: Clone>(a: &AlmideMap<K, V>, b: &AlmideMap<K, V>) -> AlmideMap<K, V> { let mut r = a.clone(); for (k, v) in b.iter() { r.insert(k.clone(), v.clone()); } r }

pub fn almide_rt_map_filter<K: PartialEq + Clone + 'static, V: Clone>(m: &AlmideMap<K, V>, f: std::rc::Rc<dyn Fn(K, V) -> bool>) -> AlmideMap<K, V> {
    let f = move |a, b| f(a, b);
    m.iter().filter(|(k, v)| f((*k).clone(), (*v).clone())).map(|(k, v)| (k.clone(), v.clone())).collect()
}

pub fn almide_rt_map_map_values<K: PartialEq + Clone + 'static, V: Clone, W>(m: &AlmideMap<K, V>, f: std::rc::Rc<dyn Fn(V) -> W>) -> AlmideMap<K, W> {
    let f = move |a| f(a);
    m.iter().map(|(k, v)| (k.clone(), f((*v).clone()))).collect()
}

pub fn almide_rt_map_from_entries<K: PartialEq + 'static, V>(entries: Vec<(K, V)>) -> AlmideMap<K, V> { entries.into_iter().collect() }
pub fn almide_rt_map_from_list<K: PartialEq + Clone + 'static, V: Clone>(keys: &[K], values: &[V]) -> AlmideMap<K, V> { keys.iter().cloned().zip(values.iter().cloned()).collect() }

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
pub fn almide_rt_map_find<K: Clone + PartialEq + 'static, V: Clone>(m: &AlmideMap<K, V>, f: std::rc::Rc<dyn Fn(K, V) -> bool>) -> Option<(K, V)> {
    let f = move |a, b| f(a, b);
    m.iter().find(|&(k, v)| f(k.clone(), v.clone())).map(|(k, v)| (k.clone(), v.clone()))
}
pub fn almide_rt_map_update<K: PartialEq + Clone + 'static, V: Clone>(m: &AlmideMap<K, V>, key: K, f: std::rc::Rc<dyn Fn(V) -> V>) -> AlmideMap<K, V> {
    let f = move |a| f(a);
    let mut m = m.clone();
    if let Some(v) = m.get(&key).cloned() { m.insert(key, f(v)); }
    m
}

// ── Mutable operations ──

pub fn almide_rt_map_insert<K: PartialEq + 'static, V>(m: &mut AlmideMap<K, V>, k: K, v: V) { m.insert(k, v); }
pub fn almide_rt_map_delete<K: PartialEq + 'static, V>(m: &mut AlmideMap<K, V>, k: K) { m.remove(&k); }
pub fn almide_rt_map_clear<K, V>(m: &mut AlmideMap<K, V>) { m.clear(); }
