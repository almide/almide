// result extern — Rust native implementations

pub fn almide_rt_result_map<T: Clone, U, E: Clone>(r: Result<T, E>, f: impl Fn(T) -> U) -> Result<U, E> {
    r.map(f)
}

pub fn almide_rt_result_map_err<T: Clone, E: Clone, F>(r: Result<T, E>, f: impl Fn(E) -> F) -> Result<T, F> {
    r.map_err(f)
}

pub fn almide_rt_result_and_then<T: Clone, U, E: Clone>(r: Result<T, E>, f: impl Fn(T) -> Result<U, E>) -> Result<U, E> {
    r.and_then(f)
}

pub fn almide_rt_result_unwrap_or<T: Clone, E>(r: Result<T, E>, default: T) -> T {
    r.unwrap_or(default)
}

pub fn almide_rt_result_is_ok<T, E>(r: &Result<T, E>) -> bool { r.is_ok() }
pub fn almide_rt_result_is_err<T, E>(r: &Result<T, E>) -> bool { r.is_err() }

pub fn almide_rt_result_ok<T: Clone, E>(r: &Result<T, E>) -> Option<T> {
    r.as_ref().ok().cloned()
}

pub fn almide_rt_result_err<T, E: Clone>(r: &Result<T, E>) -> Option<E> {
    r.as_ref().err().cloned()
}

pub fn almide_rt_result_flat_map<T: Clone, U, E: Clone>(r: Result<T, E>, f: impl Fn(T) -> Result<U, E>) -> Result<U, E> {
    r.and_then(f)
}

pub fn almide_rt_result_unwrap_or_else<T, E>(r: Result<T, E>, f: impl Fn(E) -> T) -> T {
    r.unwrap_or_else(f)
}

pub fn almide_rt_result_to_option<T: Clone, E>(r: Result<T, E>) -> Option<T> {
    r.ok()
}

pub fn almide_rt_result_to_err_option<T, E: Clone>(r: Result<T, E>) -> Option<E> {
    r.err()
}

pub fn almide_rt_result_flatten<T, E>(r: Result<Result<T, E>, E>) -> Result<T, E> {
    match r { Ok(inner) => inner, Err(e) => Err(e) }
}

pub fn almide_rt_result_collect<T, E>(rs: Vec<Result<T, E>>) -> Result<Vec<T>, Vec<E>> {
    let mut oks = Vec::new();
    let mut errs = Vec::new();
    for r in rs {
        match r {
            Ok(v) => oks.push(v),
            Err(e) => errs.push(e),
        }
    }
    if errs.is_empty() { Ok(oks) } else { Err(errs) }
}

pub fn almide_rt_result_partition<T, E>(rs: Vec<Result<T, E>>) -> (Vec<T>, Vec<E>) {
    let mut oks = Vec::new();
    let mut errs = Vec::new();
    for r in rs {
        match r {
            Ok(v) => oks.push(v),
            Err(e) => errs.push(e),
        }
    }
    (oks, errs)
}

pub fn almide_rt_result_collect_map<T, U, E>(xs: Vec<T>, mut f: impl FnMut(T) -> Result<U, E>) -> Result<Vec<U>, Vec<E>> {
    let mut oks = Vec::new();
    let mut errs = Vec::new();
    for x in xs {
        match f(x) {
            Ok(v) => oks.push(v),
            Err(e) => errs.push(e),
        }
    }
    if errs.is_empty() { Ok(oks) } else { Err(errs) }
}

// Note: the symmetry fills (`flatten` / `to_list` / `zip` / `or_else`
// / `filter`) are bundled Almide bodies in `stdlib/result.almd`.
// Keeping a Rust duplicate here caused signature drift between the
// `.almd` and runtime sides — the bundled body +
// monomorphization now owns the implementation.
