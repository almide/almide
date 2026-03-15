// list extern — Rust native implementations

pub fn almide_rt_list_len<T>(xs: &[T]) -> i64 {
    xs.len() as i64
}

pub fn almide_rt_list_map<A, B>(xs: Vec<A>, f: impl Fn(A) -> B) -> Vec<B> {
    xs.into_iter().map(f).collect()
}

pub fn almide_rt_list_filter<A: Clone>(xs: Vec<A>, f: impl Fn(&A) -> bool) -> Vec<A> {
    xs.into_iter().filter(|x| f(x)).collect()
}

pub fn almide_rt_list_fold<A, B>(xs: Vec<A>, init: B, f: impl Fn(B, A) -> B) -> B {
    xs.into_iter().fold(init, f)
}

pub fn almide_rt_list_find<A: Clone>(xs: Vec<A>, f: impl Fn(&A) -> bool) -> Option<A> {
    xs.into_iter().find(|x| f(x))
}

pub fn almide_rt_list_any<A>(xs: Vec<A>, f: impl Fn(&A) -> bool) -> bool {
    xs.iter().any(f)
}

pub fn almide_rt_list_all<A>(xs: Vec<A>, f: impl Fn(&A) -> bool) -> bool {
    xs.iter().all(f)
}

pub fn almide_rt_list_reverse<A>(xs: Vec<A>) -> Vec<A> {
    xs.into_iter().rev().collect()
}

pub fn almide_rt_list_sort<A: Ord + Clone>(xs: Vec<A>) -> Vec<A> {
    let mut v = xs;
    v.sort();
    v
}

pub fn almide_rt_list_first<A: Clone>(xs: &[A]) -> Option<A> {
    xs.first().cloned()
}

pub fn almide_rt_list_last<A: Clone>(xs: &[A]) -> Option<A> {
    xs.last().cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_len() {
        assert_eq!(almide_rt_list_len(&[1, 2, 3]), 3);
        assert_eq!(almide_rt_list_len::<i64>(&[]), 0);
    }

    #[test]
    fn test_map() {
        assert_eq!(almide_rt_list_map(vec![1, 2, 3], |x| x * 2), vec![2, 4, 6]);
    }

    #[test]
    fn test_filter() {
        assert_eq!(almide_rt_list_filter(vec![1, 2, 3, 4], |x| x % 2 == 0), vec![2, 4]);
    }

    #[test]
    fn test_fold() {
        assert_eq!(almide_rt_list_fold(vec![1, 2, 3], 0, |acc, x| acc + x), 6);
    }
}
