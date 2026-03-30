// map extern — Rust native implementations
// TOML templates use &{m} for read-only, {m} for consuming

pub fn almide_rt_map_new<K, V>() -> HashMap<K, V> { HashMap::new() }
pub fn almide_rt_map_len<K, V>(m: &HashMap<K, V>) -> i64 { m.len() as i64 }
pub fn almide_rt_map_is_empty<K, V>(m: &HashMap<K, V>) -> bool { m.is_empty() }
pub fn almide_rt_map_get<K: Eq + std::hash::Hash, V: Clone>(m: &HashMap<K, V>, k: &K) -> Option<V> { m.get(k).cloned() }
pub fn almide_rt_map_get_or<K: Eq + std::hash::Hash, V: Clone>(m: &HashMap<K, V>, k: &K, default: V) -> V { m.get(k).cloned().unwrap_or(default) }
pub fn almide_rt_map_set<K: Eq + std::hash::Hash + Clone, V: Clone>(m: &HashMap<K, V>, k: K, v: V) -> HashMap<K, V> { let mut r = m.clone(); r.insert(k, v); r }
pub fn almide_rt_map_remove<K: Eq + std::hash::Hash + Clone, V: Clone>(m: &HashMap<K, V>, k: &K) -> HashMap<K, V> { let mut r = m.clone(); r.remove(k); r }
pub fn almide_rt_map_contains<K: Eq + std::hash::Hash, V>(m: &HashMap<K, V>, k: &K) -> bool { m.contains_key(k) }
pub fn almide_rt_map_keys<K: Clone, V>(m: &HashMap<K, V>) -> Vec<K> { m.keys().cloned().collect() }
pub fn almide_rt_map_values<K, V: Clone>(m: &HashMap<K, V>) -> Vec<V> { m.values().cloned().collect() }
pub fn almide_rt_map_entries<K: Clone, V: Clone>(m: &HashMap<K, V>) -> Vec<(K, V)> { m.iter().map(|(k, v)| (k.clone(), v.clone())).collect() }
pub fn almide_rt_map_merge<K: Eq + std::hash::Hash + Clone, V: Clone>(a: &HashMap<K, V>, b: &HashMap<K, V>) -> HashMap<K, V> { let mut r = a.clone(); for (k, v) in b { r.insert(k.clone(), v.clone()); } r }

pub fn almide_rt_map_filter<K: Eq + std::hash::Hash + Clone, V: Clone>(m: &HashMap<K, V>, f: impl Fn(K, V) -> bool) -> HashMap<K, V> {
    m.iter().filter(|(k, v)| f((*k).clone(), (*v).clone())).map(|(k, v)| (k.clone(), v.clone())).collect()
}

pub fn almide_rt_map_map_values<K: Eq + std::hash::Hash + Clone, V: Clone, W>(m: &HashMap<K, V>, f: impl Fn(V) -> W) -> HashMap<K, W> {
    m.iter().map(|(k, v)| (k.clone(), f((*v).clone()))).collect()
}

pub fn almide_rt_map_from_entries<K: Eq + std::hash::Hash, V>(entries: Vec<(K, V)>) -> HashMap<K, V> { entries.into_iter().collect() }
pub fn almide_rt_map_from_list<K: Eq + std::hash::Hash + Clone, V: Clone>(keys: Vec<K>, values: Vec<V>) -> HashMap<K, V> { keys.into_iter().zip(values.into_iter()).collect() }

pub fn almide_rt_map_fold<K: Clone, V: Clone, A>(m: &HashMap<K, V>, init: A, mut f: impl FnMut(A, K, V) -> A) -> A {
    let mut acc = init;
    for (k, v) in m { acc = f(acc, k.clone(), v.clone()); }
    acc
}
pub fn almide_rt_map_any<K: Clone, V: Clone>(m: &HashMap<K, V>, f: impl Fn(K, V) -> bool) -> bool {
    m.iter().any(|(k, v)| f(k.clone(), v.clone()))
}
pub fn almide_rt_map_all<K: Clone, V: Clone>(m: &HashMap<K, V>, f: impl Fn(K, V) -> bool) -> bool {
    m.iter().all(|(k, v)| f(k.clone(), v.clone()))
}
pub fn almide_rt_map_count<K: Clone, V: Clone>(m: &HashMap<K, V>, f: impl Fn(K, V) -> bool) -> i64 {
    m.iter().filter(|&(k, v)| f(k.clone(), v.clone())).count() as i64
}
pub fn almide_rt_map_each<K: Clone, V: Clone>(m: &HashMap<K, V>, f: impl Fn(K, V)) {
    for (k, v) in m.iter() { f(k.clone(), v.clone()); }
}
pub fn almide_rt_map_find<K: Clone + Eq + std::hash::Hash, V: Clone>(m: &HashMap<K, V>, f: impl Fn(K, V) -> bool) -> Option<(K, V)> {
    m.iter().find(|&(k, v)| f(k.clone(), v.clone())).map(|(k, v)| (k.clone(), v.clone()))
}
pub fn almide_rt_map_update<K: Eq + std::hash::Hash + Clone, V: Clone>(m: HashMap<K, V>, key: K, f: impl Fn(V) -> V) -> HashMap<K, V> {
    let mut m = m;
    if let Some(v) = m.get(&key).cloned() { m.insert(key, f(v)); }
    m
}

// ── Mutable operations ──

pub fn almide_rt_map_insert<K: Eq + std::hash::Hash, V>(m: &mut HashMap<K, V>, k: K, v: V) { m.insert(k, v); }
pub fn almide_rt_map_delete<K: Eq + std::hash::Hash, V>(m: &mut HashMap<K, V>, k: &K) { m.remove(k); }
pub fn almide_rt_map_clear<K, V>(m: &mut HashMap<K, V>) { m.clear(); }
