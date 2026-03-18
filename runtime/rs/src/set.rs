// set — Rust native implementations
// HashSet is imported in the preamble
pub fn almide_rt_set_new<T>() -> HashSet<T> { HashSet::new() }
pub fn almide_rt_set_from_list<T: Eq + std::hash::Hash>(xs: Vec<T>) -> HashSet<T> { xs.into_iter().collect() }
pub fn almide_rt_set_insert<T: Eq + std::hash::Hash + Clone>(s: HashSet<T>, value: T) -> HashSet<T> { let mut s = s; s.insert(value); s }
pub fn almide_rt_set_remove<T: Eq + std::hash::Hash + Clone>(s: HashSet<T>, value: &T) -> HashSet<T> { let mut s = s; s.remove(value); s }
pub fn almide_rt_set_contains<T: Eq + std::hash::Hash>(s: &HashSet<T>, value: &T) -> bool { s.contains(value) }
pub fn almide_rt_set_len<T>(s: &HashSet<T>) -> i64 { s.len() as i64 }
pub fn almide_rt_set_is_empty<T>(s: &HashSet<T>) -> bool { s.is_empty() }
pub fn almide_rt_set_to_list<T: Clone>(s: HashSet<T>) -> Vec<T> { s.into_iter().collect() }
pub fn almide_rt_set_union<T: Eq + std::hash::Hash + Clone>(a: &HashSet<T>, b: &HashSet<T>) -> HashSet<T> { a.union(b).cloned().collect() }
pub fn almide_rt_set_intersection<T: Eq + std::hash::Hash + Clone>(a: &HashSet<T>, b: &HashSet<T>) -> HashSet<T> { a.intersection(b).cloned().collect() }
pub fn almide_rt_set_difference<T: Eq + std::hash::Hash + Clone>(a: &HashSet<T>, b: &HashSet<T>) -> HashSet<T> { a.difference(b).cloned().collect() }
