// option extern — Rust native implementations

pub fn almide_rt_option_map<T: Clone, U>(o: Option<T>, f: impl Fn(T) -> U) -> Option<U> { o.map(f) }
pub fn almide_rt_option_and_then<T: Clone, U>(o: Option<T>, f: impl Fn(T) -> Option<U>) -> Option<U> { o.and_then(f) }
pub fn almide_rt_option_unwrap_or<T>(o: Option<T>, default: T) -> T { o.unwrap_or(default) }
pub fn almide_rt_option_is_some<T>(o: &Option<T>) -> bool { o.is_some() }
pub fn almide_rt_option_is_none<T>(o: &Option<T>) -> bool { o.is_none() }
pub fn almide_rt_option_flatten<T>(o: Option<Option<T>>) -> Option<T> { o.flatten() }
pub fn almide_rt_option_to_result<T>(o: Option<T>, err: String) -> Result<T, String> { o.ok_or(err) }
pub fn almide_rt_option_unwrap_or_else<T>(o: Option<T>, f: impl Fn() -> T) -> T { o.unwrap_or_else(f) }
pub fn almide_rt_option_filter<T: Clone>(o: Option<T>, f: impl Fn(T) -> bool) -> Option<T> { o.filter(|x| f(x.clone())) }
pub fn almide_rt_option_zip<T, U>(a: Option<T>, b: Option<U>) -> Option<(T, U)> { a.zip(b) }
pub fn almide_rt_option_or_else<T>(o: Option<T>, f: impl Fn() -> Option<T>) -> Option<T> { o.or_else(f) }
pub fn almide_rt_option_to_list<T>(o: Option<T>) -> Vec<T> {
    match o { Some(v) => vec![v], None => vec![] }
}
