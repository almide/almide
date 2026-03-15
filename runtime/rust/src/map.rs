// map extern — Rust native implementations

pub fn almide_rt_map_new<K, V>() -> HashMap<K, V> { HashMap::new() }
pub fn almide_rt_map_len<K, V>(m: HashMap<K, V>) -> i64 { m.len() as i64 }
pub fn almide_rt_map_is_empty<K, V>(m: HashMap<K, V>) -> bool { m.is_empty() }

pub fn almide_rt_map_get<K: Eq + std::hash::Hash, V: Clone>(m: HashMap<K, V>, k: K) -> Option<V> {
    m.get(&k).cloned()
}

pub fn almide_rt_map_get_or<K: Eq + std::hash::Hash, V: Clone>(m: HashMap<K, V>, k: K, default: V) -> V {
    m.get(&k).cloned().unwrap_or(default)
}

pub fn almide_rt_map_set<K: Eq + std::hash::Hash + Clone, V: Clone>(mut m: HashMap<K, V>, k: K, v: V) -> HashMap<K, V> {
    m.insert(k, v);
    m
}

pub fn almide_rt_map_remove<K: Eq + std::hash::Hash + Clone, V: Clone>(mut m: HashMap<K, V>, k: K) -> HashMap<K, V> {
    m.remove(&k);
    m
}

pub fn almide_rt_map_contains<K: Eq + std::hash::Hash, V>(m: HashMap<K, V>, k: K) -> bool {
    m.contains_key(&k)
}

pub fn almide_rt_map_keys<K: Clone, V>(m: HashMap<K, V>) -> Vec<K> { m.keys().cloned().collect() }
pub fn almide_rt_map_values<K, V: Clone>(m: HashMap<K, V>) -> Vec<V> { m.values().cloned().collect() }
pub fn almide_rt_map_entries<K: Clone, V: Clone>(m: HashMap<K, V>) -> Vec<(K, V)> { m.into_iter().collect() }

pub fn almide_rt_map_merge<K: Eq + std::hash::Hash + Clone, V: Clone>(mut a: HashMap<K, V>, b: HashMap<K, V>) -> HashMap<K, V> {
    for (k, v) in b { a.insert(k, v); }
    a
}

pub fn almide_rt_map_filter<K: Eq + std::hash::Hash + Clone, V: Clone>(m: HashMap<K, V>, f: impl Fn(K, V) -> bool) -> HashMap<K, V> {
    m.into_iter().filter(|(k, v)| f(k.clone(), v.clone())).collect()
}

pub fn almide_rt_map_map_values<K: Eq + std::hash::Hash + Clone, V: Clone, W>(m: HashMap<K, V>, f: impl Fn(V) -> W) -> HashMap<K, W> {
    m.into_iter().map(|(k, v)| (k, f(v))).collect()
}

pub fn almide_rt_map_from_entries<K: Eq + std::hash::Hash, V>(entries: Vec<(K, V)>) -> HashMap<K, V> {
    entries.into_iter().collect()
}

pub fn almide_rt_map_from_list<K: Eq + std::hash::Hash + Clone, V: Clone>(keys: Vec<K>, values: Vec<V>) -> HashMap<K, V> {
    keys.into_iter().zip(values.into_iter()).collect()
}
