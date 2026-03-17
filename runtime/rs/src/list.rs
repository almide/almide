// list extern — Rust native implementations
// Signatures match TOML templates: &Vec for read-only, Vec for consuming

pub fn almide_rt_list_len<T>(xs: &Vec<T>) -> i64 { xs.len() as i64 }
pub fn almide_rt_list_is_empty<T>(xs: &Vec<T>) -> bool { xs.is_empty() }
pub fn almide_rt_list_first<A: Clone>(xs: &Vec<A>) -> Option<A> { xs.first().cloned() }
pub fn almide_rt_list_last<A: Clone>(xs: &Vec<A>) -> Option<A> { xs.last().cloned() }
pub fn almide_rt_list_get<T: Clone>(xs: &Vec<T>, i: i64) -> Option<T> { xs.get(i as usize).cloned() }
pub fn almide_rt_list_get_or<T: Clone>(xs: &Vec<T>, i: i64, default: T) -> T { xs.get(i as usize).cloned().unwrap_or(default) }
pub fn almide_rt_list_contains<T: PartialEq>(xs: &Vec<T>, x: &T) -> bool { xs.contains(x) }
pub fn almide_rt_list_index_of<T: PartialEq>(xs: &Vec<T>, x: &T) -> Option<i64> { xs.iter().position(|v| v == x).map(|i| i as i64) }
pub fn almide_rt_list_join(xs: &Vec<String>, sep: &str) -> String { xs.join(sep) }
pub fn almide_rt_list_reverse<A: Clone>(xs: &Vec<A>) -> Vec<A> { xs.iter().rev().cloned().collect() }
pub fn almide_rt_list_sort<A: Ord + Clone>(xs: &Vec<A>) -> Vec<A> { let mut v = xs.clone(); v.sort(); v }
pub fn almide_rt_list_sum(xs: &Vec<i64>) -> i64 { xs.iter().sum() }
pub fn almide_rt_list_sum_float(xs: &Vec<f64>) -> f64 { xs.iter().sum() }
pub fn almide_rt_list_product(xs: &Vec<i64>) -> i64 { xs.iter().product() }
pub fn almide_rt_list_product_float(xs: &Vec<f64>) -> f64 { xs.iter().product() }
pub fn almide_rt_list_min<T: Ord + Clone>(xs: &Vec<T>) -> Option<T> { xs.iter().min().cloned() }
pub fn almide_rt_list_max<T: Ord + Clone>(xs: &Vec<T>) -> Option<T> { xs.iter().max().cloned() }
pub fn almide_rt_list_chunk<T: Clone>(xs: &Vec<T>, n: i64) -> Vec<Vec<T>> { xs.chunks(n as usize).map(|c| c.to_vec()).collect() }
pub fn almide_rt_list_windows<T: Clone>(xs: &Vec<T>, n: i64) -> Vec<Vec<T>> { if (n as usize) > xs.len() { return vec![]; } xs.windows(n as usize).map(|w| w.to_vec()).collect() }
pub fn almide_rt_list_dedup<T: Clone + PartialEq>(xs: &Vec<T>) -> Vec<T> { let mut r = Vec::new(); for x in xs { if r.last() != Some(x) { r.push(x.clone()); } } r }
pub fn almide_rt_list_unique<T: Clone + PartialEq>(xs: &Vec<T>) -> Vec<T> { let mut r = Vec::new(); for x in xs { if !r.contains(x) { r.push(x.clone()); } } r }
pub fn almide_rt_list_set<T: Clone>(xs: &Vec<T>, i: i64, x: T) -> Vec<T> { let mut r = xs.clone(); if let Some(s) = r.get_mut(i as usize) { *s = x; } r }
pub fn almide_rt_list_swap<T: Clone>(xs: &Vec<T>, i: i64, j: i64) -> Vec<T> { let mut r = xs.clone(); let (a, b) = (i as usize, j as usize); if a < r.len() && b < r.len() { r.swap(a, b); } r }

// Consuming functions (templates use .to_vec())
pub fn almide_rt_list_map<A, B>(xs: Vec<A>, mut f: impl FnMut(A) -> B) -> Vec<B> { xs.into_iter().map(f).collect() }
pub fn almide_rt_list_filter<A: Clone>(xs: Vec<A>, mut f: impl FnMut(A) -> bool) -> Vec<A> { xs.into_iter().filter(|x| f(x.clone())).collect() }
pub fn almide_rt_list_fold<A, B>(xs: Vec<A>, init: B, mut f: impl FnMut(B, A) -> B) -> B { xs.into_iter().fold(init, f) }
pub fn almide_rt_list_find<A: Clone>(xs: Vec<A>, mut f: impl FnMut(A) -> bool) -> Option<A> { xs.into_iter().find(|x| f(x.clone())) }
pub fn almide_rt_list_any<A: Clone>(xs: &Vec<A>, mut f: impl FnMut(A) -> bool) -> bool { xs.iter().any(|x| f(x.clone())) }
pub fn almide_rt_list_all<A: Clone>(xs: &Vec<A>, mut f: impl FnMut(A) -> bool) -> bool { xs.iter().all(|x| f(x.clone())) }
pub fn almide_rt_list_each<A: Clone>(xs: &Vec<A>, mut f: impl FnMut(A)) { for x in xs { f(x.clone()); } }
pub fn almide_rt_list_count<A: Clone>(xs: &Vec<A>, mut f: impl FnMut(A) -> bool) -> i64 { xs.iter().filter(|x| f((*x).clone())).count() as i64 }
pub fn almide_rt_list_enumerate<T: Clone>(xs: Vec<T>) -> Vec<(i64, T)> { xs.into_iter().enumerate().map(|(i, x)| (i as i64, x)).collect() }
pub fn almide_rt_list_zip<T: Clone, U: Clone>(a: Vec<T>, b: Vec<U>) -> Vec<(T, U)> { a.into_iter().zip(b.into_iter()).collect() }
pub fn almide_rt_list_zip_with<A: Clone, B: Clone, C>(a: Vec<A>, b: Vec<B>, mut f: impl FnMut(A, B) -> C) -> Vec<C> { a.into_iter().zip(b.into_iter()).map(|(x, y)| f(x, y)).collect() }
pub fn almide_rt_list_flatten<T: Clone>(xs: Vec<Vec<T>>) -> Vec<T> { xs.into_iter().flatten().collect() }
pub fn almide_rt_list_flat_map<A, B>(xs: Vec<A>, mut f: impl FnMut(A) -> Vec<B>) -> Vec<B> { xs.into_iter().flat_map(f).collect() }
pub fn almide_rt_list_flat_map_effect<A, B>(xs: Vec<A>, mut f: impl FnMut(A) -> Result<Vec<B>, String>) -> Result<Vec<B>, String> { let mut r = Vec::new(); for x in xs { r.extend(f(x)?); } Ok(r) }
pub fn almide_rt_list_filter_map<A, B>(xs: Vec<A>, mut f: impl FnMut(A) -> Option<B>) -> Vec<B> { xs.into_iter().filter_map(f).collect() }
pub fn almide_rt_list_find_index<A: Clone>(xs: Vec<A>, mut f: impl FnMut(A) -> bool) -> Option<i64> { xs.into_iter().position(|x| f(x)).map(|i| i as i64) }
pub fn almide_rt_list_take<T>(xs: Vec<T>, n: i64) -> Vec<T> { xs.into_iter().take(n as usize).collect() }
pub fn almide_rt_list_drop<T>(xs: Vec<T>, n: i64) -> Vec<T> { xs.into_iter().skip(n as usize).collect() }
pub fn almide_rt_list_take_while<A: Clone>(xs: Vec<A>, mut f: impl FnMut(A) -> bool) -> Vec<A> { xs.into_iter().take_while(|x| f(x.clone())).collect() }
pub fn almide_rt_list_drop_while<A: Clone>(xs: Vec<A>, mut f: impl FnMut(A) -> bool) -> Vec<A> { xs.into_iter().skip_while(|x| f(x.clone())).collect() }
pub fn almide_rt_list_partition<A: Clone>(xs: Vec<A>, mut f: impl FnMut(A) -> bool) -> (Vec<A>, Vec<A>) { xs.into_iter().partition(|x| f(x.clone())) }
pub fn almide_rt_list_group_by<A: Clone, K: Clone + PartialEq>(xs: Vec<A>, mut f: impl FnMut(A) -> K) -> Vec<(K, Vec<A>)> { let mut g: Vec<(K, Vec<A>)> = Vec::new(); for x in xs { let k = f(x.clone()); if let Some(e) = g.iter_mut().find(|(ek, _)| ek == &k) { e.1.push(x); } else { g.push((k, vec![x])); } } g }
pub fn almide_rt_list_slice<T: Clone>(xs: Vec<T>, start: i64, end: i64) -> Vec<T> { let s = start as usize; let e = (end as usize).min(xs.len()); if s >= e { vec![] } else { xs[s..e].to_vec() } }
pub fn almide_rt_list_insert<T>(mut xs: Vec<T>, i: i64, x: T) -> Vec<T> { xs.insert(i as usize, x); xs }
pub fn almide_rt_list_remove_at<T>(mut xs: Vec<T>, i: i64) -> Vec<T> { if (i as usize) < xs.len() { xs.remove(i as usize); } xs }
pub fn almide_rt_list_update<A: Clone>(mut xs: Vec<A>, i: i64, mut f: impl FnMut(A) -> A) -> Vec<A> { if let Some(s) = xs.get_mut(i as usize) { *s = f(s.clone()); } xs }
pub fn almide_rt_list_intersperse<T: Clone>(xs: Vec<T>, sep: T) -> Vec<T> { let mut r = Vec::new(); for (i, x) in xs.into_iter().enumerate() { if i > 0 { r.push(sep.clone()); } r.push(x); } r }
pub fn almide_rt_list_repeat<T: Clone>(x: T, n: i64) -> Vec<T> { vec![x; n as usize] }
pub fn almide_rt_list_range(start: i64, end: i64) -> Vec<i64> { (start..end).collect() }
pub fn almide_rt_list_reduce<A: Clone>(xs: Vec<A>, mut f: impl FnMut(A, A) -> A) -> Option<A> { xs.into_iter().reduce(f) }
pub fn almide_rt_list_scan<A: Clone, B: Clone>(xs: Vec<A>, init: B, mut f: impl FnMut(B, A) -> B) -> Vec<B> { let mut r = Vec::new(); let mut a = init; for x in xs { a = f(a, x); r.push(a.clone()); } r }
pub fn almide_rt_list_sort_by<A: Clone>(mut xs: Vec<A>, mut f: impl FnMut(A) -> i64) -> Vec<A> { xs.sort_by_key(|x| f(x.clone())); xs }
pub fn almide_rt_list_fold_effect<A, B>(xs: Vec<A>, init: B, mut f: impl FnMut(B, A) -> Result<B, String>) -> Result<B, String> { let mut a = init; for x in xs { a = f(a, x)?; } Ok(a) }
pub fn almide_rt_list_map_effect<A, B>(xs: Vec<A>, mut f: impl FnMut(A) -> Result<B, String>) -> Result<Vec<B>, String> { xs.into_iter().map(f).collect() }

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn test_len() { assert_eq!(almide_rt_list_len(&vec![1, 2, 3]), 3); }
    #[test] fn test_map() { assert_eq!(almide_rt_list_map(vec![1, 2, 3], |x| x * 2), vec![2, 4, 6]); }
    #[test] fn test_filter() { assert_eq!(almide_rt_list_filter(vec![1, 2, 3, 4], |x| x % 2 == 0), vec![2, 4]); }
}
