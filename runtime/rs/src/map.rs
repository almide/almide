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
// Lookup (#2150): `entries` stays the single source of truth for order,
// equality and repr; once a map grows past `ALMIDE_MAP_INDEX_THRESHOLD` with
// a hashable key type it carries an `AlmideKeyIndex` — the compact-ordered-
// dict shape (CPython 3.6+, Roc's Dict): an open-addressing slot table of
// entry POSITIONS beside a per-entry cache of the full 64-bit hash. A probe
// compares the cached hash before touching the key, so a String miss costs
// one hash and no string compares; a hit costs one hash and one `==`. One
// hash per operation (the earlier sidecar SipHashed the key AND the
// fingerprint again inside a `HashMap<u64, u32>`, ~4 SipHash passes per
// `get_or` + `insert` pair). Key types with no hash (see `key_hash` —
// notably Float, where `NaN != NaN` must keep behaving exactly like the
// linear scan) stay on the linear path for good. The persistent ops
// (`map.set` et al) still clone O(n) per op — the index changes the read
// side and the mutable-insert side, not the copy.

/// Entry count at which an indexable map builds its index. Below this a
/// linear scan over `Vec<(K, V)>` is faster than hashing. Shared with set.rs
/// via flat inlining (set's source references the `almide_rt_map_`-prefixed
/// fns below, which is what RUNTIME_DEPS keys on).
pub const ALMIDE_MAP_INDEX_THRESHOLD: usize = 16;

/// Empty slot marker in `AlmideKeyIndex::slots`.
pub const ALMIDE_MAP_SLOT_EMPTY: u32 = u32::MAX;

/// Odd 64-bit multiplier (the golden-ratio constant) shared by the hashers.
pub const ALMIDE_MAP_HASH_MUL: u64 = 0x9E37_79B9_7F4A_7C15;

/// splitmix64 finalizer: every input bit reaches every output bit, so the
/// low bits the slot mask keeps are as good as the high ones.
#[inline]
pub fn almide_rt_map_mix64(x: u64) -> u64 {
    let mut z = x.wrapping_add(ALMIDE_MAP_HASH_MUL);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Multiply-fold over 8-byte words (the FxHash step) with the length mixed
/// in and a splitmix finalizer — one multiply per word, no per-byte loop.
#[inline]
pub fn almide_rt_map_hash_bytes(b: &[u8]) -> u64 {
    let mut h: u64 = (b.len() as u64).wrapping_mul(ALMIDE_MAP_HASH_MUL);
    let mut words = b.chunks_exact(8);
    for w in &mut words {
        let mut buf = [0u8; 8];
        buf.copy_from_slice(w);
        h = (h ^ u64::from_le_bytes(buf)).wrapping_mul(ALMIDE_MAP_HASH_MUL).rotate_left(29);
    }
    let rest = words.remainder();
    if !rest.is_empty() {
        let mut buf = [0u8; 8];
        buf[..rest.len()].copy_from_slice(rest);
        h = (h ^ u64::from_le_bytes(buf)).wrapping_mul(ALMIDE_MAP_HASH_MUL).rotate_left(29);
    }
    almide_rt_map_mix64(h)
}

/// Order-sensitive pair combine: `(a, b)` and `(b, a)` hash differently.
#[inline]
pub fn almide_rt_map_hash_pair(a: u64, b: u64) -> u64 {
    almide_rt_map_mix64(a.wrapping_mul(ALMIDE_MAP_HASH_MUL) ^ b)
}

#[inline]
fn almide_map_hash_seq(hs: impl Iterator<Item = u64>) -> u64 {
    almide_rt_map_mix64(hs.fold(0x51_7CC1_B727_220A_95u64, almide_rt_map_hash_pair))
}

/// Hash for the key types the index supports. `None` = this key type stays
/// on the linear path (the answer is per TYPE, never per value, so one probe
/// decides a map's fate). Equality is always re-confirmed with `PartialEq`
/// after a hash hit, so a collision can never produce a wrong answer — only
/// an extra comparison. The `&dyn Any` downcasts fold to one branch per
/// monomorphization. The `almide_rt_map_` name is deliberate: it is how
/// build.rs's RUNTIME_DEPS extraction learns that a module referencing this
/// helper (set.rs) needs map's source spliced in.
pub fn almide_rt_map_key_hash<K: 'static>(k: &K) -> Option<u64> {
    let a = k as &dyn std::any::Any;
    if let Some(x) = a.downcast_ref::<i64>() {
        return Some(almide_rt_map_mix64(*x as u64));
    }
    if let Some(s) = a.downcast_ref::<String>() {
        return Some(almide_rt_map_hash_bytes(s.as_bytes()));
    }
    if let Some(b) = a.downcast_ref::<bool>() {
        return Some(almide_rt_map_mix64(*b as u64));
    }
    almide_map_key_hash_compound(a)
}

fn almide_map_key_hash_compound(a: &dyn std::any::Any) -> Option<u64> {
    if let Some((x, y)) = a.downcast_ref::<(i64, i64)>() {
        return Some(almide_rt_map_hash_pair(*x as u64, *y as u64));
    }
    if let Some((x, y)) = a.downcast_ref::<(String, String)>() {
        return Some(almide_rt_map_hash_pair(almide_rt_map_hash_bytes(x.as_bytes()), almide_rt_map_hash_bytes(y.as_bytes())));
    }
    if let Some((x, y)) = a.downcast_ref::<(String, i64)>() {
        return Some(almide_rt_map_hash_pair(almide_rt_map_hash_bytes(x.as_bytes()), *y as u64));
    }
    if let Some((x, y)) = a.downcast_ref::<(i64, String)>() {
        return Some(almide_rt_map_hash_pair(*x as u64, almide_rt_map_hash_bytes(y.as_bytes())));
    }
    if let Some(xs) = a.downcast_ref::<Vec<i64>>() {
        return Some(almide_map_hash_seq(xs.iter().map(|x| *x as u64)));
    }
    if let Some(xs) = a.downcast_ref::<Vec<String>>() {
        return Some(almide_map_hash_seq(xs.iter().map(|s| almide_rt_map_hash_bytes(s.as_bytes()))));
    }
    almide_map_key_hash_sized_int(a)
}

fn almide_map_key_hash_sized_int(a: &dyn std::any::Any) -> Option<u64> {
    if let Some(x) = a.downcast_ref::<i32>() { return Some(almide_rt_map_mix64(*x as u64)); }
    if let Some(x) = a.downcast_ref::<i16>() { return Some(almide_rt_map_mix64(*x as u64)); }
    if let Some(x) = a.downcast_ref::<i8>() { return Some(almide_rt_map_mix64(*x as u64)); }
    if let Some(x) = a.downcast_ref::<u64>() { return Some(almide_rt_map_mix64(*x)); }
    if let Some(x) = a.downcast_ref::<u32>() { return Some(almide_rt_map_mix64(*x as u64)); }
    if let Some(x) = a.downcast_ref::<u16>() { return Some(almide_rt_map_mix64(*x as u64)); }
    if let Some(x) = a.downcast_ref::<u8>() { return Some(almide_rt_map_mix64(*x as u64)); }
    None
}

/// What a lookup key must provide: its hash, agreeing with the stored key
/// type's hash for every `K: Borrow<Q>` pair the read side admits. The
/// blanket impl covers every sized key; `str` is the one unsized borrow —
/// a `&str` parameter (the borrow-inferred rendering of a String param)
/// probes a `Map[String, _]` without materializing a `String` first, and
/// hashes byte-identically to the stored `String`.
pub trait AlmideMapKey {
    fn almide_key_hash(&self) -> Option<u64>;
}
impl<T: 'static> AlmideMapKey for T {
    #[inline]
    fn almide_key_hash(&self) -> Option<u64> {
        almide_rt_map_key_hash(self)
    }
}
impl AlmideMapKey for str {
    #[inline]
    fn almide_key_hash(&self) -> Option<u64> {
        Some(almide_rt_map_hash_bytes(self.as_bytes()))
    }
}

/// The hash index beside an insertion-ordered entry vector: `hashes[p]` is
/// the full hash of entry `p`, `slots` is a power-of-two linear-probe table
/// of entry positions (load ≤ 3/4). Shared by `AlmideMap` and `AlmideSet`.
/// No tombstones: a removal shifts the entry vector, so the table is rebuilt
/// from the cached hashes (O(n), no key access) — the same order the old
/// sidecar paid, and removal is not the hot path the index exists for.
#[derive(Clone, Debug, Default)]
pub struct AlmideKeyIndex {
    hashes: Vec<u64>,
    slots: Vec<u32>,
}

impl AlmideKeyIndex {
    pub fn with_capacity(n: usize) -> Self {
        let slot_len = (n * 4 / 3 + 1).next_power_of_two().max(32);
        AlmideKeyIndex { hashes: Vec::with_capacity(n), slots: vec![ALMIDE_MAP_SLOT_EMPTY; slot_len] }
    }

    /// Position of the entry whose hash is `h` and whose key `eq` confirms.
    #[inline]
    pub fn find(&self, h: u64, mut eq: impl FnMut(usize) -> bool) -> Option<usize> {
        let mask = self.slots.len() - 1;
        let mut i = (h as usize) & mask;
        loop {
            let p = self.slots[i];
            if p == ALMIDE_MAP_SLOT_EMPTY {
                return None;
            }
            let p = p as usize;
            if self.hashes[p] == h && eq(p) {
                return Some(p);
            }
            i = (i + 1) & mask;
        }
    }

    /// Record the entry just appended at position `hashes.len()`.
    #[inline]
    pub fn push(&mut self, h: u64) {
        let p = self.hashes.len() as u32;
        self.hashes.push(h);
        if self.hashes.len() * 4 > self.slots.len() * 3 {
            self.rebuild(self.slots.len() * 2);
        } else {
            self.place(h, p);
        }
    }

    /// The entry at `p` was removed and every later entry shifted down one.
    pub fn remove_at(&mut self, p: usize) {
        self.hashes.remove(p);
        self.rebuild(self.slots.len());
    }

    fn rebuild(&mut self, slot_len: usize) {
        self.slots.clear();
        self.slots.resize(slot_len, ALMIDE_MAP_SLOT_EMPTY);
        for p in 0..self.hashes.len() {
            self.place(self.hashes[p], p as u32);
        }
    }

    #[inline]
    fn place(&mut self, h: u64, p: u32) {
        let mask = self.slots.len() - 1;
        let mut i = (h as usize) & mask;
        while self.slots[i] != ALMIDE_MAP_SLOT_EMPTY {
            i = (i + 1) & mask;
        }
        self.slots[i] = p;
    }
}

/// Where a keyed collection's lookups go. `Linear` until the threshold;
/// then `Built` for a hashable key type or `Unhashable` for good.
#[derive(Clone, Debug, Default)]
pub enum AlmideKeyLookup {
    #[default]
    Linear,
    Unhashable,
    Built(AlmideKeyIndex),
}

impl AlmideKeyLookup {
    /// Hash `k` only when a built index will consume it.
    #[inline]
    pub fn hash_for<Q: ?Sized + AlmideMapKey>(&self, k: &Q) -> Option<u64> {
        match self {
            AlmideKeyLookup::Built(_) => k.almide_key_hash(),
            _ => None,
        }
    }

    /// Index every key of a collection that just crossed the threshold.
    pub fn build<'a, K: AlmideMapKey + 'a>(&mut self, keys: impl Iterator<Item = &'a K>, n: usize) {
        let mut ix = AlmideKeyIndex::with_capacity(n);
        for k in keys {
            match k.almide_key_hash() {
                Some(h) => ix.push(h),
                None => {
                    *self = AlmideKeyLookup::Unhashable;
                    return;
                }
            }
        }
        *self = AlmideKeyLookup::Built(ix);
    }

    pub fn removed_at(&mut self, p: usize) {
        if let AlmideKeyLookup::Built(ix) = self {
            ix.remove_at(p);
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct AlmideMap<K, V> {
    entries: Vec<(K, V)>,
    lookup: AlmideKeyLookup,
}

impl<K, V> AlmideMap<K, V> {
    pub fn new() -> Self {
        AlmideMap { entries: Vec::new(), lookup: AlmideKeyLookup::Linear }
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
        self.lookup = AlmideKeyLookup::Linear;
    }
}

impl<K: PartialEq + 'static, V> AlmideMap<K, V> {
    /// Position of `k` in `entries`, given its hash when the index is built.
    /// `Q` is the borrowed form of the key (`str` for a `String` key), so a
    /// read never has to own a key it only compares.
    #[inline]
    fn position_with<Q: ?Sized + PartialEq>(&self, k: &Q, h: Option<u64>) -> Option<usize>
    where K: std::borrow::Borrow<Q> {
        match (&self.lookup, h) {
            (AlmideKeyLookup::Built(ix), Some(h)) => ix.find(h, |p| self.entries[p].0.borrow() == k),
            _ => self.entries.iter().position(|(ek, _)| ek.borrow() == k),
        }
    }

    #[inline]
    fn position<Q: ?Sized + PartialEq + AlmideMapKey>(&self, k: &Q) -> Option<usize>
    where K: std::borrow::Borrow<Q> {
        self.position_with(k, self.lookup.hash_for(k))
    }

    /// A new entry was appended: extend the index, or build it at the threshold.
    #[inline]
    fn note_push(&mut self, h: Option<u64>) {
        if let AlmideKeyLookup::Built(ix) = &mut self.lookup {
            if let Some(h) = h {
                ix.push(h);
            }
            return;
        }
        if matches!(self.lookup, AlmideKeyLookup::Linear) && self.entries.len() >= ALMIDE_MAP_INDEX_THRESHOLD {
            self.lookup.build(self.entries.iter().map(|(k, _)| k), self.entries.len());
        }
    }

    pub fn get<Q: ?Sized + PartialEq + AlmideMapKey>(&self, k: &Q) -> Option<&V>
    where K: std::borrow::Borrow<Q> {
        self.position(k).map(|i| &self.entries[i].1)
    }
    pub fn get_mut<Q: ?Sized + PartialEq + AlmideMapKey>(&mut self, k: &Q) -> Option<&mut V>
    where K: std::borrow::Borrow<Q> {
        self.position(k).map(|i| &mut self.entries[i].1)
    }
    pub fn contains_key<Q: ?Sized + PartialEq + AlmideMapKey>(&self, k: &Q) -> bool
    where K: std::borrow::Borrow<Q> {
        self.position(k).is_some()
    }
    /// Insert: update the value in place if the key exists (preserving its
    /// position), else append the new entry. Matches insertion-order semantics.
    pub fn insert(&mut self, k: K, v: V) {
        let h = self.lookup.hash_for(&k);
        if let Some(i) = self.position_with(&k, h) {
            self.entries[i].1 = v;
            return;
        }
        self.entries.push((k, v));
        self.note_push(h);
    }
    /// Remove, keeping the order of the remaining entries.
    pub fn remove(&mut self, k: &K) {
        if let Some(i) = self.position(k) {
            self.entries.remove(i);
            self.lookup.removed_at(i);
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
// The read side borrows its key (`@borrow_ref(key)` in stdlib/map.almd):
// `map.get_or(counts, w, 0)` no longer clones `w` at the call site — the
// clone was one third of the word-count loop's map cost (#2157). `Q` is the
// borrowed key form: `&String`/`&str` for a String key, `&i64` for Int.
pub fn almide_rt_map_get<K: PartialEq + 'static, V: Clone, Q: ?Sized + PartialEq + AlmideMapKey>(m: &AlmideMap<K, V>, k: &Q) -> Option<V>
where K: std::borrow::Borrow<Q> { m.get(k).cloned() }
pub fn almide_rt_map_get_or<K: PartialEq + 'static, V: Clone, Q: ?Sized + PartialEq + AlmideMapKey>(m: &AlmideMap<K, V>, k: &Q, default: V) -> V
where K: std::borrow::Borrow<Q> { m.get(k).cloned().unwrap_or(default) }
// Consuming (@consume(m) in stdlib/map.almd): a caller whose map is dead at
// the call moves it in and this is one hash insert; a caller that still uses
// the source gets its clone inserted by pass_clone at the call site. The
// borrowing `let mut r = m.clone()` form cloned the WHOLE map on every call —
// the fold-accumulator hot loop (#1143) paid it per line. Composes with the
// index: the moved-in map keeps its index, so the insert is O(1) for
// hashable keys.
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
pub fn almide_rt_map_contains<K: PartialEq + 'static, V, Q: ?Sized + PartialEq + AlmideMapKey>(m: &AlmideMap<K, V>, k: &Q) -> bool
where K: std::borrow::Borrow<Q> { m.contains_key(k) }
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
