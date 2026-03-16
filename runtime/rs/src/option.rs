// option extern — Rust native implementations

pub fn almide_rt_option_map<T: Clone, U>(o: Option<T>, f: impl Fn(T) -> U) -> Option<U> { o.map(f) }
pub fn almide_rt_option_and_then<T: Clone, U>(o: Option<T>, f: impl Fn(T) -> Option<U>) -> Option<U> { o.and_then(f) }
pub fn almide_rt_option_unwrap_or<T>(o: Option<T>, default: T) -> T { o.unwrap_or(default) }
pub fn almide_rt_option_is_some<T>(o: &Option<T>) -> bool { o.is_some() }
pub fn almide_rt_option_is_none<T>(o: &Option<T>) -> bool { o.is_none() }
pub fn almide_rt_option_flatten<T>(o: Option<Option<T>>) -> Option<T> { o.flatten() }
pub fn almide_rt_option_to_result<T, E>(o: Option<T>, err: E) -> Result<T, E> { o.ok_or(err) }
