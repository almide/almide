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
pub fn almide_rt_list_chunk<T: Clone>(xs: &[T], n: i64) -> Vec<Vec<T>> { if n == 0 { almide_abort("chunk size must be positive"); } xs.chunks(n as usize).map(|c| c.to_vec()).collect() }
pub fn almide_rt_list_windows<T: Clone>(xs: &[T], n: i64) -> Vec<Vec<T>> { if n == 0 { almide_abort("window size must be positive"); } if (n as usize) > xs.len() { return vec![]; } xs.windows(n as usize).map(|w| w.to_vec()).collect() }
pub fn almide_rt_list_dedup<T: Clone + PartialEq>(xs: &[T]) -> Vec<T> { let mut r = Vec::new(); for x in xs { if r.last() != Some(x) { r.push(x.clone()); } } r }
pub fn almide_rt_list_unique<T: Clone + PartialEq>(xs: &[T]) -> Vec<T> { let mut r = Vec::new(); for x in xs { if !r.contains(x) { r.push(x.clone()); } } r }
pub fn almide_rt_list_set<T: Clone>(xs: &[T], i: i64, x: T) -> Vec<T> { let mut r = xs.to_vec(); if let Some(s) = r.get_mut(i as usize) { *s = x; } r }
pub fn almide_rt_list_swap<T: Clone>(xs: &[T], i: i64, j: i64) -> Vec<T> { let mut r = xs.to_vec(); let (a, b) = (i as usize, j as usize); if a < r.len() && b < r.len() { r.swap(a, b); } r }

// Consuming functions (templates use .to_vec())
pub fn almide_rt_list_map<A, B>(xs: Vec<A>, f: std::rc::Rc<dyn Fn(A) -> B>) -> Vec<B> { let f = move |a| f(a); xs.into_iter().map(f).collect() }
pub fn almide_rt_list_filter<A: Clone>(xs: Vec<A>, f: std::rc::Rc<dyn Fn(A) -> bool>) -> Vec<A> { let f = move |a| f(a); xs.into_iter().filter(|x| f(x.clone())).collect() }
pub fn almide_rt_list_fold<A, B>(xs: Vec<A>, init: B, f: std::rc::Rc<dyn Fn(B, A) -> B>) -> B { let f = move |a, b| f(a, b); xs.into_iter().fold(init, f) }
pub fn almide_rt_list_find<A: Clone>(xs: &[A], f: std::rc::Rc<dyn Fn(A) -> bool>) -> Option<A> { let f = move |a| f(a); xs.iter().find(|x| f((*x).clone())).cloned() }
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
pub fn almide_rt_list_find_index<A: Clone>(xs: &[A], f: std::rc::Rc<dyn Fn(A) -> bool>) -> Option<i64> { let f = move |a| f(a); xs.iter().position(|x| f(x.clone())).map(|i| i as i64) }
pub fn almide_rt_list_take<T: Clone>(xs: &[T], n: i64) -> Vec<T> { xs.iter().take(n as usize).cloned().collect() }
pub fn almide_rt_list_drop<T: Clone>(xs: &[T], n: i64) -> Vec<T> { xs.iter().skip(n as usize).cloned().collect() }
pub fn almide_rt_list_take_while<A: Clone>(xs: &[A], f: std::rc::Rc<dyn Fn(A) -> bool>) -> Vec<A> { let f = move |a| f(a); xs.iter().take_while(|x| f((*x).clone())).cloned().collect() }
pub fn almide_rt_list_drop_while<A: Clone>(xs: &[A], f: std::rc::Rc<dyn Fn(A) -> bool>) -> Vec<A> { let f = move |a| f(a); xs.iter().skip_while(|x| f((*x).clone())).cloned().collect() }
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
pub fn almide_rt_list_slice<T: Clone>(xs: &[T], start: i64, end: i64) -> Vec<T> { let s = start as usize; let e = (end as usize).min(xs.len()); if s >= e { vec![] } else { xs[s..e].to_vec() } }
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
        almide_abort("repeat result too large");
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
        almide_abort("out of memory");
    }
    v.extend(start..end);
    v
}
pub fn almide_rt_list_reduce<A: Clone>(xs: Vec<A>, f: std::rc::Rc<dyn Fn(A, A) -> A>) -> Option<A> { let f = move |a, b| f(a, b); xs.into_iter().reduce(f) }
pub fn almide_rt_list_scan<A: Clone, B: Clone>(xs: Vec<A>, init: B, f: std::rc::Rc<dyn Fn(B, A) -> B>) -> Vec<B> { let f = move |a, b| f(a, b); let mut r = Vec::new(); let mut a = init; for x in xs { a = f(a, x); r.push(a.clone()); } r }
pub fn almide_rt_list_sort_by<A: Clone, B: Ord>(mut xs: Vec<A>, f: std::rc::Rc<dyn Fn(A) -> B>) -> Vec<A> { let f = move |a| f(a); xs.sort_by_cached_key(|x| f(x.clone())); xs }  // #560: cached_key calls the key fn ONCE PER ELEMENT (n), matching the wasm precomputed-key array + the key-extraction intent; sort_by_key called it per COMPARISON (n log n), an observable native!=wasm divergence for side-effectful keys.
pub fn almide_rt_list_fold_effect<A, B>(xs: Vec<A>, init: B, f: std::rc::Rc<dyn Fn(B, A) -> Result<B, String>>) -> Result<B, String> { let f = move |a, b| f(a, b); let mut a = init; for x in xs { a = f(a, x)?; } Ok(a) }
pub fn almide_rt_list_map_effect<A, B>(xs: Vec<A>, f: std::rc::Rc<dyn Fn(A) -> Result<B, String>>) -> Result<Vec<B>, String> { let f = move |a| f(a); xs.into_iter().map(f).collect() }

pub fn almide_rt_list_take_end<T: Clone>(xs: &[T], n: i64) -> Vec<T> {
    let start = if n as usize >= xs.len() { 0 } else { xs.len() - n as usize };
    xs[start..].to_vec()
}
pub fn almide_rt_list_drop_end<T: Clone>(xs: &[T], n: i64) -> Vec<T> {
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
pub fn almide_rt_list_window<T: Clone>(xs: &[T], n: i64) -> Vec<Vec<T>> {
    if n == 0 { almide_abort("window size must be positive"); }
    if (n as usize) > xs.len() { return vec![]; }
    xs.windows(n as usize).map(|w| w.to_vec()).collect()
}

// ── Parallel variants (pure list ops under a `fan` block, #2044) ──
// Reached ONLY through `fan { … }`: RustLowering's fan routing rewrites a pure
// `list.map/filter/any/all` whose element, result and captured types are all
// Send-safe scalars to these twins when the call sits under a fan block — the
// explicit fan-out is the user's signal that each element is a task worth a
// thread, so the threshold is 2 elements, not a "big list" cut-off (a 64-chunk
// fan over 14 cores would never clear one). The callback is a NAMED `F: Fn`
// generic in LAST position on purpose: build.rs derives `takes_raw_fn_last_arg`
// from exactly that shape, which is what keeps the closure a raw `impl Fn`
// instead of the uniform `Rc<dyn Fn>` (neither Send nor Sync). `par_map` joins
// into pre-sized slots by index, so its output is byte-identical to `list_map`.
// `ALMIDE_FAN_SEQUENTIAL=1` (read once per process) forces the sequential path
// — the ablation knob check-perf-ratio.sh's fan VICTORY rows use.
const ALMIDE_PARALLEL_THRESHOLD: usize = 2;

/// `ALMIDE_FAN_SEQUENTIAL` — set (non-empty, not "0") means every parallel
/// twin takes its sequential path. Read once; the value is process-wide.
pub fn almide_rt_list_par_sequential() -> bool {
    static SEQ: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *SEQ.get_or_init(|| {
        std::env::var("ALMIDE_FAN_SEQUENTIAL").map(|v| !v.is_empty() && v != "0").unwrap_or(false)
    })
}

/// Worker count for `len` items: one per available core, never more than `len`.
/// `ALMIDE_FAN_THREADS=N` (read once) caps it further — the thread-scaling
/// lever #3003 measures both legs with; the result is the same at any N.
pub fn almide_rt_list_par_workers(len: usize) -> usize {
    static CAP: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    let cap = *CAP.get_or_init(|| {
        std::env::var("ALMIDE_FAN_THREADS").ok().and_then(|v| v.parse::<usize>().ok()).filter(|&n| n > 0).unwrap_or(usize::MAX)
    });
    let cpus = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1);
    cpus.max(1).min(cap).min(len.max(1))
}

// #3341 — THE FAN COST MODEL, one formula on both legs (the embedded wasm
// host's copy is crates/almide-wasm-run/src/host_fan.rs `plan`; the two
// constants are pinned to docs/benchmarks/fan-cost-model.txt by
// tests/fan_cost_model_test.rs). Every parallel twin is a SITE (one per
// callback type — the monomorphised twin's own address), and every offer at
// a site records the compute time its elements took (`AlmideFanClock`: the
// threads' CPU time inside the elements, so spawn latency and the time a
// thread sat descheduled are not counted, and each worker's first element —
// its warm-up — is kept out of the mean). A site's FIRST offer goes parallel, as
// before #3341; later offers estimate each element at the site's last
// measured time `t`. A parallel offer on W workers COSTS `OFFER + W·WORKER`
// (thread spawn, join, the group's bookkeeping — measured) and SAVES
// `t·(n − ⌈n/W⌉)`; the W with the largest positive net saving wins, and none
// = the offer runs here, sequentially. Output is the same either way (C-321).
// Timing element 0 first and deciding after it was measured and rejected: it
// serialises one whole chunk, ~2x wall time when n ≈ workers (fannkuchredux).
pub const ALMIDE_FAN_OFFER_NS: u128 = 30_000;
pub const ALMIDE_FAN_WORKER_NS: u128 = 9_000;

/// The worker count for an offer of `n` elements whose site last measured
/// `per_elem` ns per element (`None` = its first offer): >= 2 = go parallel
/// on that many workers, 1 = stay sequential.
pub fn almide_rt_fan_plan(n: usize, per_elem: Option<u128>) -> usize {
    // `ALMIDE_FAN_COST_OFF` (read once): every offer goes parallel — the
    // pre-#3341 behaviour, the ablation the crossover is measured against.
    static OFF: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    let off = *OFF.get_or_init(|| std::env::var("ALMIDE_FAN_COST_OFF").map(|v| !v.is_empty() && v != "0").unwrap_or(false));
    let Some(t) = per_elem.filter(|_| !off) else { return almide_rt_list_par_workers(n) };
    let mut best = (1usize, 0u128);
    for w in 2..=almide_rt_list_par_workers(n) {
        let saved = t.saturating_mul((n - n.div_ceil(w)) as u128);
        let cost = ALMIDE_FAN_OFFER_NS + ALMIDE_FAN_WORKER_NS * w as u128;
        if saved > cost && saved - cost > best.1 {
            best = (w, saved - cost);
        }
    }
    best.0
}

fn almide_fan_history() -> &'static std::sync::Mutex<std::collections::HashMap<usize, u128>> {
    static H: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<usize, u128>>> = std::sync::OnceLock::new();
    H.get_or_init(Default::default)
}

/// The site's last measured time per element, if it has run before.
fn almide_fan_estimate(site: usize) -> Option<u128> {
    almide_fan_history().lock().ok()?.get(&site).copied()
}

/// The calling thread's CPU time in ns — the clock an offer's elements are
/// measured on (the wasm host's copy is host_fan.rs `cpu_ns`). Wall time
/// would also count the time the thread sat descheduled: on a loaded
/// machine ONE preempted element of a trivial body read 4.6 ms and kept its
/// site parallel. Elsewhere: a monotonic wall clock.
#[cfg(all(any(target_os = "linux", target_os = "macos"), target_pointer_width = "64"))]
fn almide_fan_cpu_ns() -> u64 {
    #[repr(C)]
    struct Timespec { sec: i64, nsec: i64 }
    unsafe extern "C" {
        fn clock_gettime(clock: i32, ts: *mut Timespec) -> i32;
    }
    // CLOCK_THREAD_CPUTIME_ID
    const CLOCK: i32 = if cfg!(target_os = "linux") { 3 } else { 16 };
    let mut ts = Timespec { sec: 0, nsec: 0 };
    if unsafe { clock_gettime(CLOCK, &mut ts) } != 0 {
        return 0;
    }
    (ts.sec as u64).wrapping_mul(1_000_000_000).wrapping_add(ts.nsec as u64)
}
#[cfg(not(all(any(target_os = "linux", target_os = "macos"), target_pointer_width = "64")))]
fn almide_fan_cpu_ns() -> u64 {
    static T0: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    T0.get_or_init(std::time::Instant::now).elapsed().as_nanos() as u64
}

/// What one offer's elements took. A worker thread's FIRST element is its
/// warm-up (a fresh thread's first touch of the body) and is kept apart —
/// that cost belongs to the offer's `WORKER` term, not to `t`; the estimate
/// is the warm elements' mean, or the warm-ups' when no worker ran a second
/// element. The calling thread's sequential run is all warm.
struct AlmideFanClock {
    warm: [std::sync::atomic::AtomicU64; 2],
    cold: [std::sync::atomic::AtomicU64; 2],
}

impl AlmideFanClock {
    fn new() -> Self {
        let z = || std::sync::atomic::AtomicU64::new(0);
        AlmideFanClock { warm: [z(), z()], cold: [z(), z()] }
    }

    fn add(cell: &[std::sync::atomic::AtomicU64; 2], ns: u64, n: u64) {
        cell[0].fetch_add(ns, std::sync::atomic::Ordering::Relaxed);
        cell[1].fetch_add(n, std::sync::atomic::Ordering::Relaxed);
    }

    /// The calling thread runs the offer: `g` returns its result and how
    /// many elements it evaluated.
    fn calling<R>(&self, g: impl FnOnce() -> (R, usize)) -> R {
        let t0 = almide_fan_cpu_ns();
        let (r, n) = g();
        Self::add(&self.warm, almide_fan_cpu_ns().wrapping_sub(t0), n as u64);
        r
    }

    /// A worker thread runs `items` through `each`: `Some(go_on)` = it
    /// evaluated the element, `None` = it stopped without evaluating it.
    fn worker<T>(&self, items: impl IntoIterator<Item = T>, mut each: impl FnMut(T) -> Option<bool>) {
        let mut it = items.into_iter();
        let Some(first) = it.next() else { return };
        let t0 = almide_fan_cpu_ns();
        let go = each(first);
        let t1 = almide_fan_cpu_ns();
        if go.is_some() {
            Self::add(&self.cold, t1.wrapping_sub(t0), 1);
        }
        if go != Some(true) {
            return;
        }
        let mut n = 0u64;
        for x in it {
            match each(x) {
                Some(go_on) => {
                    n += 1;
                    if !go_on { break; }
                }
                None => break,
            }
        }
        Self::add(&self.warm, almide_fan_cpu_ns().wrapping_sub(t1), n);
    }

    /// Record the offer as the site's estimate: ns per element.
    fn record(self, site: usize) {
        let [w, wn] = self.warm.map(|a| a.into_inner());
        let [c, cn] = self.cold.map(|a| a.into_inner());
        let (ns, n) = if wn > 0 { (w, wn) } else { (c, cn) };
        if n > 0 {
            if let Ok(mut h) = almide_fan_history().lock() {
                h.insert(site, u128::from(ns) / u128::from(n));
            }
        }
    }
}

pub fn almide_rt_list_par_map<A: Send + Sync + Clone, B: Send, F: Fn(A) -> B + Send + Sync>(xs: Vec<A>, f: F) -> Vec<B> {
    if xs.len() < ALMIDE_PARALLEL_THRESHOLD || almide_rt_list_par_sequential() {
        return xs.into_iter().map(&f).collect();
    }
    let site = almide_rt_list_par_map::<A, B, F> as fn(Vec<A>, F) -> Vec<B> as usize;
    let n = xs.len();
    let workers = almide_rt_fan_plan(n, almide_fan_estimate(site));
    let clock = AlmideFanClock::new();
    if workers < 2 {
        let out = clock.calling(|| (xs.into_iter().map(&f).collect(), n));
        clock.record(site);
        return out;
    }
    let chunk_size = n.div_ceil(workers);
    let mut slots: Vec<Option<B>> = (0..n).map(|_| None).collect();
    // Flush first: a worker's abort cannot reach this thread's buffer (C-197).
    almide_stdout_flush();
    // The workers work for the enclosing fan element: a trap on one waits for
    // the elements below it (ADR-0024 D6).
    let sink = almide_fan_current();
    std::thread::scope(|s| {
        for (chunk, out) in xs.chunks(chunk_size).zip(slots.chunks_mut(chunk_size)) {
            let (f, clock) = (&f, &clock);
            let sink = sink.clone();
            s.spawn(move || {
                almide_fan_adopt(sink);
                clock.worker(out.iter_mut().zip(chunk), |(slot, x)| {
                    *slot = Some(f(x.clone()));
                    Some(true)
                });
            });
        }
    });
    clock.record(site);
    slots.into_iter().map(|b| b.expect("list.par_map: every slot is written by its worker")).collect()
}

pub fn almide_rt_list_par_filter<A: Send + Sync + Clone, F: Fn(A) -> bool + Send + Sync>(xs: Vec<A>, f: F) -> Vec<A> {
    if xs.len() < ALMIDE_PARALLEL_THRESHOLD || almide_rt_list_par_sequential() {
        return xs.into_iter().filter(|x| f(x.clone())).collect();
    }
    let site = almide_rt_list_par_filter::<A, F> as fn(Vec<A>, F) -> Vec<A> as usize;
    let n = xs.len();
    let workers = almide_rt_fan_plan(n, almide_fan_estimate(site));
    let clock = AlmideFanClock::new();
    if workers < 2 {
        let out = clock.calling(|| (xs.into_iter().filter(|x| f(x.clone())).collect(), n));
        clock.record(site);
        return out;
    }
    let chunk_size = n.div_ceil(workers);
    let chunks: Vec<&[A]> = xs.chunks(chunk_size).collect();
    let mut results: Vec<Option<Vec<A>>> = (0..chunks.len()).map(|_| None).collect();
    // Flush first: a worker's abort cannot reach this thread's buffer (C-197).
    almide_stdout_flush();
    // The workers work for the enclosing fan element: a trap on one waits for
    // the elements below it (ADR-0024 D6).
    let sink = almide_fan_current();
    std::thread::scope(|s| {
        let mut handles = Vec::new();
        for chunk in &chunks {
            let (f, clock) = (&f, &clock);
            let sink = sink.clone();
            handles.push(s.spawn(move || {
                almide_fan_adopt(sink);
                let mut kept = Vec::new();
                clock.worker(chunk.iter(), |x| {
                    if f(x.clone()) {
                        kept.push(x.clone());
                    }
                    Some(true)
                });
                kept
            }));
        }
        for (i, handle) in handles.into_iter().enumerate() {
            results[i] = Some(handle.join().unwrap());
        }
    });
    clock.record(site);
    results.into_iter().flatten().flatten().collect()
}

/// `any` (`stop_on` = true) / `all` (`stop_on` = false): does some element's
/// verdict equal `stop_on`. Records the elements it actually evaluated.
fn almide_fan_search<A: Send + Sync + Clone, F: Fn(A) -> bool + Send + Sync>(site: usize, xs: &[A], f: &F, stop_on: bool) -> bool {
    let clock = AlmideFanClock::new();
    let look = |x: &A| f(x.clone()) == stop_on;
    let workers = almide_rt_fan_plan(xs.len(), almide_fan_estimate(site));
    let hit = if workers < 2 {
        clock.calling(|| {
            let mut seen = 0usize;
            let hit = xs.iter().any(|x| {
                seen += 1;
                look(x)
            });
            (hit, seen)
        })
    } else {
        let chunk_size = xs.len().div_ceil(workers);
        let hit = std::sync::atomic::AtomicBool::new(false);
        // Flush first: a worker's abort cannot reach this thread's buffer (C-197).
        almide_stdout_flush();
        // The workers work for the enclosing fan element: a trap on one waits for
        // the elements below it (ADR-0024 D6).
        let sink = almide_fan_current();
        std::thread::scope(|s| {
            for chunk in xs.chunks(chunk_size) {
                let (hit, clock, look) = (&hit, &clock, &look);
                let sink = sink.clone();
                s.spawn(move || {
                    almide_fan_adopt(sink);
                    clock.worker(chunk, |x| {
                        if hit.load(std::sync::atomic::Ordering::Relaxed) { return None; }
                        if look(x) {
                            hit.store(true, std::sync::atomic::Ordering::Relaxed);
                            return Some(false);
                        }
                        Some(true)
                    });
                });
            }
        });
        hit.load(std::sync::atomic::Ordering::Relaxed)
    };
    clock.record(site);
    hit
}

pub fn almide_rt_list_par_any<A: Send + Sync + Clone, F: Fn(A) -> bool + Send + Sync>(xs: &[A], f: F) -> bool {
    if xs.len() < ALMIDE_PARALLEL_THRESHOLD || almide_rt_list_par_sequential() {
        return xs.iter().any(|x| f(x.clone()));
    }
    almide_fan_search(almide_rt_list_par_any::<A, F> as fn(&[A], F) -> bool as usize, xs, &f, true)
}

pub fn almide_rt_list_par_all<A: Send + Sync + Clone, F: Fn(A) -> bool + Send + Sync>(xs: &[A], f: F) -> bool {
    if xs.len() < ALMIDE_PARALLEL_THRESHOLD || almide_rt_list_par_sequential() {
        return xs.iter().all(|x| f(x.clone()));
    }
    !almide_fan_search(almide_rt_list_par_all::<A, F> as fn(&[A], F) -> bool as usize, xs, &f, false)
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
