// set — Rust native implementations
// HashSet is imported in the preamble
pub fn almide_rt_set_new<T>() -> HashSet<T> { HashSet::new() }
pub fn almide_rt_set_from_list<T: Eq + std::hash::Hash + Clone>(xs: &[T]) -> HashSet<T> { xs.iter().cloned().collect() }
pub fn almide_rt_set_insert<T: Eq + std::hash::Hash + Clone>(s: &HashSet<T>, value: T) -> HashSet<T> { let mut s = s.clone(); s.insert(value); s }
pub fn almide_rt_set_remove<T: Eq + std::hash::Hash + Clone>(s: &HashSet<T>, value: T) -> HashSet<T> { let mut s = s.clone(); s.remove(&value); s }
pub fn almide_rt_set_contains<T: Eq + std::hash::Hash>(s: &HashSet<T>, value: T) -> bool { s.contains(&value) }
pub fn almide_rt_set_len<T>(s: &HashSet<T>) -> i64 { s.len() as i64 }
pub fn almide_rt_set_is_empty<T>(s: &HashSet<T>) -> bool { s.is_empty() }
pub fn almide_rt_set_to_list<T: Clone>(s: &HashSet<T>) -> Vec<T> { s.iter().cloned().collect() }
pub fn almide_rt_set_union<T: Eq + std::hash::Hash + Clone>(a: &HashSet<T>, b: &HashSet<T>) -> HashSet<T> { a.union(b).cloned().collect() }
pub fn almide_rt_set_intersection<T: Eq + std::hash::Hash + Clone>(a: &HashSet<T>, b: &HashSet<T>) -> HashSet<T> { a.intersection(b).cloned().collect() }
pub fn almide_rt_set_difference<T: Eq + std::hash::Hash + Clone>(a: &HashSet<T>, b: &HashSet<T>) -> HashSet<T> { a.difference(b).cloned().collect() }
pub fn almide_rt_set_symmetric_difference<T: Eq + std::hash::Hash + Clone>(a: &HashSet<T>, b: &HashSet<T>) -> HashSet<T> { a.symmetric_difference(b).cloned().collect() }
pub fn almide_rt_set_is_subset<T: Eq + std::hash::Hash>(a: &HashSet<T>, b: &HashSet<T>) -> bool { a.is_subset(b) }
pub fn almide_rt_set_is_disjoint<T: Eq + std::hash::Hash>(a: &HashSet<T>, b: &HashSet<T>) -> bool { a.is_disjoint(b) }
pub fn almide_rt_set_filter<T: Eq + std::hash::Hash + Clone>(s: &HashSet<T>, mut f: impl FnMut(T) -> bool) -> HashSet<T> { s.iter().cloned().filter(|x| f(x.clone())).collect() }
pub fn almide_rt_set_map<T: Eq + std::hash::Hash + Clone, U: Eq + std::hash::Hash>(s: &HashSet<T>, f: impl Fn(T) -> U) -> HashSet<U> { s.iter().cloned().map(f).collect() }
pub fn almide_rt_set_fold<T: Eq + std::hash::Hash + Clone, B>(s: &HashSet<T>, init: B, f: impl Fn(B, T) -> B) -> B { s.iter().cloned().fold(init, f) }
pub fn almide_rt_set_each<T: Eq + std::hash::Hash + Clone>(s: &HashSet<T>, mut f: impl FnMut(T)) { for x in s { f(x.clone()); } }
pub fn almide_rt_set_any<T: Eq + std::hash::Hash + Clone>(s: &HashSet<T>, mut f: impl FnMut(T) -> bool) -> bool { s.iter().any(|x| f(x.clone())) }
pub fn almide_rt_set_all<T: Eq + std::hash::Hash + Clone>(s: &HashSet<T>, mut f: impl FnMut(T) -> bool) -> bool { s.iter().all(|x| f(x.clone())) }
