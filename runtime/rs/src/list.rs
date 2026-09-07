// list extern — Rust native implementations
// Signatures match TOML templates: &Vec for read-only, Vec for consuming

pub fn almide_rt_list_len<T>(xs: &[T]) -> i64 { xs.len() as i64 }
pub fn almide_rt_list_is_empty<T>(xs: &[T]) -> bool { xs.is_empty() }
pub fn almide_rt_list_first<A: Clone>(xs: &[A]) -> Option<A> { xs.first().cloned() }
pub fn almide_rt_list_last<A: Clone>(xs: &[A]) -> Option<A> { xs.last().cloned() }
pub fn almide_rt_list_get<T: Clone>(xs: &[T], i: i64) -> Option<T> { xs.get(i as usize).cloned() }
pub fn almide_rt_list_get_or<T: Clone>(xs: &[T], i: i64, default: T) -> T { xs.get(i as usize).cloned().unwrap_or(default) }
pub fn almide_rt_list_contains<T: PartialEq>(xs: &[T], x: T) -> bool { xs.contains(&x) }
pub fn almide_rt_list_index_of<T: PartialEq>(xs: &[T], x: T) -> Option<i64> { xs.iter().position(|v| *v == x).map(|i| i as i64) }
pub fn almide_rt_list_join(xs: &[String], sep: &str) -> String { xs.join(sep) }
pub fn almide_rt_list_reverse<A: Clone>(xs: &[A]) -> Vec<A> { xs.iter().rev().cloned().collect() }
pub fn almide_rt_list_sort<A: Ord + Clone>(xs: &[A]) -> Vec<A> { let mut v = xs.to_vec(); v.sort(); v }
// Float ordering uses IEEE-754 totalOrder (`f64::total_cmp`): NaN takes its
// totalOrder position (greatest, after +inf; with a -NaN before -inf) and
// `-0.0 < +0.0`. `f64` is not `Ord`, so `list.sort`/`min`/`max` on `List[Float]`
// (and `sort_by` with a Float key) route to these float-specific variants
// instead of the `Ord`-bounded generics (IntrinsicLoweringPass swaps the
// symbol). This is the ORDERING twin of the wasm sign-magnitude bit trick and
// matches the interp's `total_cmp`. NOTE: this list-min/max totalOrder is a
// DIFFERENT contract from the SCALAR `float.min`/`max`/`math.fmin`/`fmax`,
// which keep their C-049 NaN-IGNORING semantics. See C-055.
pub fn almide_rt_list_sort_float(xs: &[f64]) -> Vec<f64> { let mut v = xs.to_vec(); v.sort_by(|a, b| a.total_cmp(b)); v }
pub fn almide_rt_list_min_float(xs: &[f64]) -> Option<f64> { xs.iter().copied().min_by(|a, b| a.total_cmp(b)) }
pub fn almide_rt_list_max_float(xs: &[f64]) -> Option<f64> { xs.iter().copied().max_by(|a, b| a.total_cmp(b)) }
pub fn almide_rt_list_sort_by_float<A: Clone>(xs: Vec<A>, f: std::rc::Rc<dyn Fn(A) -> f64>) -> Vec<A> {
    let f = move |a| f(a);
    let mut v = xs;
    v.sort_by(|a, b| f(a.clone()).total_cmp(&f(b.clone())));
    v
}
// `list.sum`/`list.product` follow the language's integer-overflow law:
// TWO'S-COMPLEMENT WRAPPING at runtime, identical to plain `a + b` / `a * b`
// (contract C-001/C-047 family) and byte-identical to wasm's `i64.add`/`i64.mul`.
// Std `Iterator::sum`/`product` would PANIC under `-C overflow-checks` (debug /
// `cargo test`) yet wrap in release — a profile-dependent split that diverges
// from wasm. Folding with the explicit `wrapping_*` ops removes that split, so
// the result is the same on native (any profile) and wasm. See C-056.
pub fn almide_rt_list_sum(xs: &[i64]) -> i64 { xs.iter().fold(0i64, |a, &b| a.wrapping_add(b)) }
pub fn almide_rt_list_sum_float(xs: &[f64]) -> f64 { xs.iter().sum() }
pub fn almide_rt_list_product(xs: &[i64]) -> i64 { xs.iter().fold(1i64, |a, &b| a.wrapping_mul(b)) }
pub fn almide_rt_list_product_float(xs: &[f64]) -> f64 { xs.iter().product() }
pub fn almide_rt_list_min<T: Ord + Clone>(xs: &[T]) -> Option<T> { xs.iter().min().cloned() }
pub fn almide_rt_list_max<T: Ord + Clone>(xs: &[T]) -> Option<T> { xs.iter().max().cloned() }
// chunk/windows are TOTAL (ALS-T4): a NEGATIVE n keeps the historical `as usize`
// wrap (huge → chunk: whole-as-one-chunk / windows: empty), now normative; n == 0
// aborts with the ALS-T6 form (`Error: …` + exit 1) instead of leaking Rust's raw
// `chunks(0)`/`windows(0)` panic (exit 101) — wasm previously even returned len+1
// EMPTY windows silently for `windows(xs, 0)`.
pub fn almide_rt_list_chunk<T: Clone>(xs: &[T], n: i64) -> Vec<Vec<T>> { if n == 0 { eprintln!("Error: chunk size must be positive"); std::process::exit(1); } xs.chunks(n as usize).map(|c| c.to_vec()).collect() }
pub fn almide_rt_list_windows<T: Clone>(xs: &[T], n: i64) -> Vec<Vec<T>> { if n == 0 { eprintln!("Error: window size must be positive"); std::process::exit(1); } if (n as usize) > xs.len() { return vec![]; } xs.windows(n as usize).map(|w| w.to_vec()).collect() }
pub fn almide_rt_list_dedup<T: Clone + PartialEq>(xs: &[T]) -> Vec<T> { let mut r = Vec::new(); for x in xs { if r.last() != Some(x) { r.push(x.clone()); } } r }
pub fn almide_rt_list_unique<T: Clone + PartialEq>(xs: &[T]) -> Vec<T> { let mut r = Vec::new(); for x in xs { if !r.contains(x) { r.push(x.clone()); } } r }
pub fn almide_rt_list_set<T: Clone>(xs: &[T], i: i64, x: T) -> Vec<T> { let mut r = xs.to_vec(); if let Some(s) = r.get_mut(i as usize) { *s = x; } r }
pub fn almide_rt_list_swap<T: Clone>(xs: &[T], i: i64, j: i64) -> Vec<T> { let mut r = xs.to_vec(); let (a, b) = (i as usize, j as usize); if a < r.len() && b < r.len() { r.swap(a, b); } r }

// Consuming functions (templates use .to_vec())
pub fn almide_rt_list_map<A, B>(xs: Vec<A>, f: std::rc::Rc<dyn Fn(A) -> B>) -> Vec<B> { let f = move |a| f(a); xs.into_iter().map(f).collect() }
pub fn almide_rt_list_filter<A: Clone>(xs: Vec<A>, f: std::rc::Rc<dyn Fn(A) -> bool>) -> Vec<A> { let f = move |a| f(a); xs.into_iter().filter(|x| f(x.clone())).collect() }
pub fn almide_rt_list_fold<A, B>(xs: Vec<A>, init: B, f: std::rc::Rc<dyn Fn(B, A) -> B>) -> B { let f = move |a, b| f(a, b); xs.into_iter().fold(init, f) }
pub fn almide_rt_list_find<A: Clone>(xs: Vec<A>, f: std::rc::Rc<dyn Fn(A) -> bool>) -> Option<A> { let f = move |a| f(a); xs.into_iter().find(|x| f(x.clone())) }
pub fn almide_rt_list_any<A: Clone>(xs: &[A], f: std::rc::Rc<dyn Fn(A) -> bool>) -> bool { let f = move |a| f(a); xs.iter().any(|x| f(x.clone())) }
pub fn almide_rt_list_all<A: Clone>(xs: &[A], f: std::rc::Rc<dyn Fn(A) -> bool>) -> bool { let f = move |a| f(a); xs.iter().all(|x| f(x.clone())) }
pub fn almide_rt_list_each<A: Clone>(xs: &[A], f: std::rc::Rc<dyn Fn(A)>) { let f = move |a| f(a); for x in xs { f(x.clone()); } }
pub fn almide_rt_list_count<A: Clone>(xs: &[A], f: std::rc::Rc<dyn Fn(A) -> bool>) -> i64 { let f = move |a| f(a); xs.iter().filter(|x| f((*x).clone())).count() as i64 }
pub fn almide_rt_list_enumerate<T: Clone>(xs: Vec<T>) -> Vec<(i64, T)> { xs.into_iter().enumerate().map(|(i, x)| (i as i64, x)).collect() }
pub fn almide_rt_list_zip<T: Clone, U: Clone>(a: Vec<T>, b: Vec<U>) -> Vec<(T, U)> { a.into_iter().zip(b.into_iter()).collect() }
pub fn almide_rt_list_zip_with<A: Clone, B: Clone, C>(a: Vec<A>, b: Vec<B>, f: std::rc::Rc<dyn Fn(A, B) -> C>) -> Vec<C> { let f = move |a, b| f(a, b); a.into_iter().zip(b.into_iter()).map(|(x, y)| f(x, y)).collect() }
// Takes a SLICE, not an owned `Vec`. The element type is already `Clone`, so
// consuming the outer list bought nothing — and it made `flatten` unusable
// alongside a borrow of the same binding in one expression:
// `list.get_or(xs, 0, list.flatten(xs))` moved `xs` into `flatten` while
// `get_or` borrowed it, so `check` accepted and rustc rejected with E0505
// (differential fuzz). A slice parameter takes a borrow like every other
// read-only list fn, and an owned `Vec` still deref-coerces into it.
pub fn almide_rt_list_flatten<T: Clone>(xs: &[Vec<T>]) -> Vec<T> { xs.iter().flatten().cloned().collect() }
pub fn almide_rt_list_flat_map<A, B>(xs: Vec<A>, f: std::rc::Rc<dyn Fn(A) -> Vec<B>>) -> Vec<B> { let f = move |a| f(a); xs.into_iter().flat_map(f).collect() }
// FIXED-ARITY twin of `almide_rt_list_flat_map`, for the `|x| … [a, b]` shape
// the CHEATSHEET's recommended build idiom produces
// (`list.range |> list.flat_map`). `RustLoweringPass::lower_flat_map_arrays`
// retargets those call sites here and rewrites the tail list literal to a Rust
// ARRAY, which is what makes the difference: the `Rc<dyn Fn(A) -> Vec<B>>`
// signature above forces ONE HEAP ALLOCATION PER ELEMENT for the intermediate
// list, and at 2^22 elements that single `vec![a, b]` was 44 ms of a 79 ms
// build — the whole of the 3x gap between the recommended idiom and a
// hand-written append loop (#1337).
//
// `F: Fn` rather than `Rc<dyn Fn>`: it keeps the per-element call STATIC so
// rustc inlines the body and the arity stays a constant it can unroll (the
// generated `build.rs` registry derives the un-boxing from this `F: Fn` bound
// — see `takes_raw_fn_last_arg`). `with_capacity` removes the growth reallocs
// `flat_map().collect()` cannot avoid, since `FlatMap`'s `size_hint` lower
// bound is 0.
//
// Semantics are identical to `almide_rt_list_flat_map`: same order, same
// elements, same length. Only the intermediate container is gone.
pub fn almide_rt_list_flat_map_arr<A, B, const N: usize, F: Fn(A) -> [B; N]>(xs: Vec<A>, f: F) -> Vec<B> {
    let mut out = Vec::with_capacity(xs.len().saturating_mul(N));
    for x in xs { out.extend(f(x)); }
    out
}
pub fn almide_rt_list_flat_map_effect<A, B>(xs: Vec<A>, f: std::rc::Rc<dyn Fn(A) -> Result<Vec<B>, String>>) -> Result<Vec<B>, String> { let f = move |a| f(a); let mut r = Vec::new(); for x in xs { r.extend(f(x)?); } Ok(r) }
pub fn almide_rt_list_filter_map<A, B>(xs: Vec<A>, f: std::rc::Rc<dyn Fn(A) -> Option<B>>) -> Vec<B> { let f = move |a| f(a); xs.into_iter().filter_map(f).collect() }
pub fn almide_rt_list_find_index<A: Clone>(xs: Vec<A>, f: std::rc::Rc<dyn Fn(A) -> bool>) -> Option<i64> { let f = move |a| f(a); xs.into_iter().position(|x| f(x)).map(|i| i as i64) }
pub fn almide_rt_list_take<T>(xs: Vec<T>, n: i64) -> Vec<T> { xs.into_iter().take(n as usize).collect() }
pub fn almide_rt_list_drop<T>(xs: Vec<T>, n: i64) -> Vec<T> { xs.into_iter().skip(n as usize).collect() }
pub fn almide_rt_list_take_while<A: Clone>(xs: Vec<A>, f: std::rc::Rc<dyn Fn(A) -> bool>) -> Vec<A> { let f = move |a| f(a); xs.into_iter().take_while(|x| f(x.clone())).collect() }
pub fn almide_rt_list_drop_while<A: Clone>(xs: Vec<A>, f: std::rc::Rc<dyn Fn(A) -> bool>) -> Vec<A> { let f = move |a| f(a); xs.into_iter().skip_while(|x| f(x.clone())).collect() }
pub fn almide_rt_list_partition<A: Clone>(xs: Vec<A>, f: std::rc::Rc<dyn Fn(A) -> bool>) -> (Vec<A>, Vec<A>) { let f = move |a| f(a); xs.into_iter().partition(|x| f(x.clone())) }
pub fn almide_rt_list_group_by<A: Clone, B: PartialEq + Clone + 'static>(xs: Vec<A>, f: std::rc::Rc<dyn Fn(A) -> B>) -> AlmideMap<B, Vec<A>> {
    let f = move |a| f(a);
    let mut m: AlmideMap<B, Vec<A>> = AlmideMap::new();
    for x in xs {
        let k = f(x.clone());
        if let Some(g) = m.get_mut(&k) { g.push(x); } else { m.insert(k, vec![x]); }
    }
    m
}
pub fn almide_rt_list_slice<T: Clone>(xs: Vec<T>, start: i64, end: i64) -> Vec<T> { let s = start as usize; let e = (end as usize).min(xs.len()); if s >= e { vec![] } else { xs[s..e].to_vec() } }
pub fn almide_rt_list_insert<T>(mut xs: Vec<T>, i: i64, x: T) -> Vec<T> { let idx = (i as usize).min(xs.len()); xs.insert(idx, x); xs }
pub fn almide_rt_list_remove_at<T>(mut xs: Vec<T>, i: i64) -> Vec<T> { if (i as usize) < xs.len() { xs.remove(i as usize); } xs }
pub fn almide_rt_list_update<A: Clone>(mut xs: Vec<A>, i: i64, f: std::rc::Rc<dyn Fn(A) -> A>) -> Vec<A> { let f = move |a| f(a); if let Some(s) = xs.get_mut(i as usize) { *s = f(s.clone()); } xs }
pub fn almide_rt_list_intersperse<T: Clone>(xs: Vec<T>, sep: T) -> Vec<T> { let mut r = Vec::new(); for (i, x) in xs.into_iter().enumerate() { if i > 0 { r.push(sep.clone()); } r.push(x); } r }
// Negative counts clamp to 0 (C-054 discipline — the wasm self-host
// `list_repeat` already clamps; `n as usize` on a negative i64 panicked).
//
// A result over the shared 2^31-BYTE ceiling aborts in the T6 form on BOTH
// targets, the same rule `string.repeat` follows (C-161). Without it the two
// legs failed in different ways for a count native can satisfy but wasm cannot:
// `list.repeat(0.0, i32::MAX)` allocated 16 GiB natively and printed a length,
// while the wasm leg — capped at a 4 GiB address space — trapped out-of-bounds
// (differential fuzz). A machine-dependent success on one leg is not an
// observable the equivalence claim can carry.
//
// The ceiling counts SLOTS at the wasm element width (8 bytes), not
// `size_of::<T>()`, so the limit is the same number on both legs whatever the
// native element happens to be.
pub const ALMIDE_LIST_REPEAT_MAX_ELEMS: i64 = (1 << 31) / 8;
pub fn almide_rt_list_repeat<T: Clone>(x: T, n: i64) -> Vec<T> {
    if n > ALMIDE_LIST_REPEAT_MAX_ELEMS {
        eprintln!("Error: repeat result too large");
        std::process::exit(1);
    }
    vec![x; n.max(0) as usize]
}
// `list.range` has NO chosen ceiling (ratified A, 2026-08-17): this leg fills to
// its own structural bound, and a span the machine cannot satisfy is the C-197
// abort via try_reserve — never a raw `capacity overflow` panic, which is what
// `(start..end).collect()`'s infallible reserve produced for a span like
// (i64::MIN, 3). `saturating_sub` keeps the count honest where `end - start`
// would wrap. The wasm leg fails with the SAME message at its own i32 floor
// bound; success between the two bounds is the contracted divergence.
pub fn almide_rt_list_range(start: i64, end: i64) -> Vec<i64> {
    let count = end.saturating_sub(start).max(0) as usize;
    let mut v: Vec<i64> = Vec::new();
    if v.try_reserve_exact(count).is_err() {
        eprintln!("Error: out of memory");
        std::process::exit(1);
    }
    v.extend(start..end);
    v
}
pub fn almide_rt_list_reduce<A: Clone>(xs: Vec<A>, f: std::rc::Rc<dyn Fn(A, A) -> A>) -> Option<A> { let f = move |a, b| f(a, b); xs.into_iter().reduce(f) }
pub fn almide_rt_list_scan<A: Clone, B: Clone>(xs: Vec<A>, init: B, f: std::rc::Rc<dyn Fn(B, A) -> B>) -> Vec<B> { let f = move |a, b| f(a, b); let mut r = Vec::new(); let mut a = init; for x in xs { a = f(a, x); r.push(a.clone()); } r }
pub fn almide_rt_list_sort_by<A: Clone, B: Ord>(mut xs: Vec<A>, f: std::rc::Rc<dyn Fn(A) -> B>) -> Vec<A> { let f = move |a| f(a); xs.sort_by_cached_key(|x| f(x.clone())); xs }  // #560: cached_key calls the key fn ONCE PER ELEMENT (n), matching the wasm precomputed-key array + the key-extraction intent; sort_by_key called it per COMPARISON (n log n), an observable native!=wasm divergence for side-effectful keys.
pub fn almide_rt_list_fold_effect<A, B>(xs: Vec<A>, init: B, f: std::rc::Rc<dyn Fn(B, A) -> Result<B, String>>) -> Result<B, String> { let f = move |a, b| f(a, b); let mut a = init; for x in xs { a = f(a, x)?; } Ok(a) }
pub fn almide_rt_list_map_effect<A, B>(xs: Vec<A>, f: std::rc::Rc<dyn Fn(A) -> Result<B, String>>) -> Result<Vec<B>, String> { let f = move |a| f(a); xs.into_iter().map(f).collect() }

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn test_len() { assert_eq!(almide_rt_list_len(&vec![1, 2, 3]), 3); }
    #[test] fn test_map() { assert_eq!(almide_rt_list_map(vec![1, 2, 3], std::rc::Rc::new(|x| x * 2)), vec![2, 4, 6]); }
    #[test] fn test_filter() { assert_eq!(almide_rt_list_filter(vec![1, 2, 3, 4], std::rc::Rc::new(|x| x % 2 == 0)), vec![2, 4]); }
}

pub fn almide_rt_list_take_end<T: Clone>(xs: Vec<T>, n: i64) -> Vec<T> {
    let start = if n as usize >= xs.len() { 0 } else { xs.len() - n as usize };
    xs[start..].to_vec()
}
pub fn almide_rt_list_drop_end<T: Clone>(xs: Vec<T>, n: i64) -> Vec<T> {
    let end = if n as usize >= xs.len() { 0 } else { xs.len() - n as usize };
    xs[..end].to_vec()
}
// Keep the FIRST element of each distinct key, in first-occurrence order.
// The key bound is `PartialEq` (not `Eq + Hash`) so a record, variant, tuple,
// Option or Float key dedups exactly as the wasm leg's generic-equality scan
// does (#1812): user types derive `Clone, Debug, PartialEq` only, so the old
// `HashSet` seen set was a check-passes/build-fails gap (rustc E0277), and a
// Float key follows `PartialEq` — `-0.0 == 0.0` collapses, NaN never matches.
pub fn almide_rt_list_unique_by<T: Clone, K: PartialEq>(xs: Vec<T>, f: std::rc::Rc<dyn Fn(T) -> K>) -> Vec<T> {
    let f = move |a| f(a);
    let mut seen: Vec<K> = Vec::new();
    let mut result = Vec::new();
    for x in xs {
        let k = f(x.clone());
        if !seen.iter().any(|s| *s == k) { seen.push(k); result.push(x); }
    }
    result
}
pub fn almide_rt_list_shuffle<T>(mut xs: Vec<T>) -> Vec<T> {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos() as u64;
    for i in (1..xs.len()).rev() {
        let mut h = DefaultHasher::new();
        seed.hash(&mut h);
        i.hash(&mut h);
        let j = (h.finish() as usize) % (i + 1);
        xs.swap(i, j);
        seed = seed.wrapping_add(1);
    }
    xs
}
// Same ALS-T6 guard as `almide_rt_list_windows`: n == 0 aborts with the
// unified `Error: …` + exit 1 form (std's `windows(0)` panics — a raw Rust
// panic exit 101 the wasm leg never shows). n > len returns empty like the
// plural twin instead of leaking std's behavior.
pub fn almide_rt_list_window<T: Clone>(xs: Vec<T>, n: i64) -> Vec<Vec<T>> {
    if n == 0 { eprintln!("Error: window size must be positive"); std::process::exit(1); }
    if (n as usize) > xs.len() { return vec![]; }
    xs.windows(n as usize).map(|w| w.to_vec()).collect()
}

// ── Parallel variants (auto-parallelization for pure lambdas) ──
// Uses std::thread::scope for work-stealing parallelism.
// Falls back to sequential below ALMIDE_PARALLEL_THRESHOLD elements.

// Parallel when there are at least 2 elements.
// Each element may represent arbitrarily heavy work (e.g., fan { list.map(chunks, heavy_fn) }).
// Using a high threshold would skip parallelism for small lists with expensive per-element work.
const ALMIDE_PARALLEL_THRESHOLD: usize = 2;

pub fn almide_rt_list_par_map<A: Send + Sync + Clone, B: Send + Sync>(xs: Vec<A>, f: impl Fn(A) -> B + Send + Sync) -> Vec<B> {
    if xs.len() < ALMIDE_PARALLEL_THRESHOLD {
        return xs.into_iter().map(&f).collect();
    }
    let cpus = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1);
    let chunk_size = (xs.len() + cpus - 1) / cpus;
    let chunks: Vec<Vec<A>> = xs.chunks(chunk_size).map(|c| c.to_vec()).collect();
    let mut results: Vec<Option<Vec<B>>> = (0..chunks.len()).map(|_| None).collect();
    std::thread::scope(|s| {
        let mut handles = Vec::new();
        for chunk in &chunks {
            let f = &f;
            handles.push(s.spawn(move || {
                chunk.iter().map(|x| f(x.clone())).collect::<Vec<B>>()
            }));
        }
        for (i, handle) in handles.into_iter().enumerate() {
            results[i] = Some(handle.join().unwrap());
        }
    });
    results.into_iter().flatten().flatten().collect()
}

pub fn almide_rt_list_par_filter<A: Send + Sync + Clone>(xs: Vec<A>, f: impl Fn(A) -> bool + Send + Sync) -> Vec<A> {
    if xs.len() < ALMIDE_PARALLEL_THRESHOLD {
        return xs.into_iter().filter(|x| f(x.clone())).collect();
    }
    let cpus = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1);
    let chunk_size = (xs.len() + cpus - 1) / cpus;
    let chunks: Vec<Vec<A>> = xs.chunks(chunk_size).map(|c| c.to_vec()).collect();
    let mut results: Vec<Option<Vec<A>>> = (0..chunks.len()).map(|_| None).collect();
    std::thread::scope(|s| {
        let mut handles = Vec::new();
        for chunk in &chunks {
            let f = &f;
            handles.push(s.spawn(move || {
                chunk.iter().filter(|x| f((*x).clone())).cloned().collect::<Vec<A>>()
            }));
        }
        for (i, handle) in handles.into_iter().enumerate() {
            results[i] = Some(handle.join().unwrap());
        }
    });
    results.into_iter().flatten().flatten().collect()
}

pub fn almide_rt_list_par_any<A: Send + Sync + Clone>(xs: &[A], f: impl Fn(A) -> bool + Send + Sync) -> bool {
    if xs.len() < ALMIDE_PARALLEL_THRESHOLD {
        return xs.iter().any(|x| f(x.clone()));
    }
    let cpus = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1);
    let chunk_size = (xs.len() + cpus - 1) / cpus;
    let chunks: Vec<&[A]> = xs.chunks(chunk_size).collect();
    let found = std::sync::atomic::AtomicBool::new(false);
    std::thread::scope(|s| {
        for chunk in &chunks {
            let f = &f;
            let found = &found;
            s.spawn(move || {
                for x in *chunk {
                    if found.load(std::sync::atomic::Ordering::Relaxed) { return; }
                    if f(x.clone()) {
                        found.store(true, std::sync::atomic::Ordering::Relaxed);
                        return;
                    }
                }
            });
        }
    });
    found.load(std::sync::atomic::Ordering::Relaxed)
}

pub fn almide_rt_list_par_all<A: Send + Sync + Clone>(xs: &[A], f: impl Fn(A) -> bool + Send + Sync) -> bool {
    if xs.len() < ALMIDE_PARALLEL_THRESHOLD {
        return xs.iter().all(|x| f(x.clone()));
    }
    let cpus = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1);
    let chunk_size = (xs.len() + cpus - 1) / cpus;
    let chunks: Vec<&[A]> = xs.chunks(chunk_size).collect();
    let failed = std::sync::atomic::AtomicBool::new(false);
    std::thread::scope(|s| {
        for chunk in &chunks {
            let f = &f;
            let failed = &failed;
            s.spawn(move || {
                for x in *chunk {
                    if failed.load(std::sync::atomic::Ordering::Relaxed) { return; }
                    if !f(x.clone()) {
                        failed.store(true, std::sync::atomic::Ordering::Relaxed);
                        return;
                    }
                }
            });
        }
    });
    !failed.load(std::sync::atomic::Ordering::Relaxed)
}

// ── Mutable operations ──

#[inline(always)] pub fn almide_rt_list_push<A>(xs: &mut Vec<A>, x: A) { xs.push(x); }

// Pre-allocate a List with the given capacity. Start-empty (len=0) but
// skips all reallocations up to `cap` pushes. Useful when the caller
// knows the final size up front (Q1_0 tensor decode, fixed-size bulk
// transforms).
//
// The EAGER reservation is clamped to this many bytes — capacity is an
// unobservable hint (`push` grows past it normally), but an unclamped eager
// reservation aborts on huge requests (`with_capacity(i32::MAX)` over a
// 24-byte element = ~51.5 GB, machine-dependent). The ceiling below keeps
// `with_capacity` total on native; the v1 wasm self-host
// (stdlib/list_make.almd) reserves NOTHING — `list_with_capacity` ignores
// `cap` and returns a fresh empty list — so there is no wasm-side ceiling to
// mirror, and the two legs still agree because the reservation is
// unobservable (C-034). v0's wasm leg clamped at the same 64 MiB in
// emit_wasm/calls_list.rs, retired in c71eff7b.
pub const ALMIDE_MAX_WITH_CAPACITY_PREALLOC_BYTES: usize = 64 * 1024 * 1024; // 64 MiB
#[inline(always)] pub fn almide_rt_list_with_capacity<A>(cap: i64) -> Vec<A> {
    let elem_size = std::mem::size_of::<A>().max(1);
    let max_cap = ALMIDE_MAX_WITH_CAPACITY_PREALLOC_BYTES / elem_size;
    Vec::with_capacity((cap.max(0) as usize).min(max_cap))
}
pub fn almide_rt_list_pop<A>(xs: &mut Vec<A>) -> Option<A> { xs.pop() }
pub fn almide_rt_list_clear<A>(xs: &mut Vec<A>) { xs.clear(); }

// ── Algorithmic primitives (Phase 3 stdlib expansion) ──

pub fn almide_rt_list_binary_search(xs: &[i64], target: i64) -> Option<i64> {
    xs.binary_search(&target).ok().map(|i| i as i64)
}
