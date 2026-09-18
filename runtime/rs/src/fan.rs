// Fan concurrency runtime functions

// `fan.map` runs SEQUENTIALLY over an `Rc<dyn Fn>` thunk (the uniform closure
// repr). This is what lets a closure VALUE bound to a var reach `fan.map`: that
// value is an `Rc<dyn Fn>`, which is neither `Send` nor `Sync`, so it could not
// be moved across the `thread::scope` boundary the parallel version required.
// Results are identical to a parallel map (input order preserved); only the
// (unobservable) parallelism is dropped. race/settle keep their
// thread::scope and receive `Box<dyn Fn + Send[+ Sync]>` thunks (the box pass
// boxes them), which still satisfy the existing `impl Fn + Send + Sync` bounds.
//
// `fan.map` is EFFECTFUL: if any element fn returns `Err`, the whole map
// propagates the FIRST `Err` (in list order) as a defined Result error. The
// caller's auto-`?` then routes it to the effect-main termination path
// (`Error: <msg>` + exit 1), byte-identical to the wasm `__main_runner`.
pub fn almide_rt_fan_map<A, B>(
    items: Vec<A>,
    f: std::rc::Rc<dyn Fn(A) -> Result<B, String>>,
) -> Result<Vec<B>, String> {
    items.into_iter().map(|item| f(item)).collect()
}

// `fan.map`'s PARALLEL twin (#2044). RustLowering's fan routing (native only) routes a
// `fan.map` here when its callback is a PURE lambda LITERAL whose element,
// result and captured types are all Send-safe scalars — Int/Float/Bool/Unit,
// the sized numerics, and tuples/records of those. Anything carrying an `Rc`
// (List/Map/Set/Bytes/String, closure values) and every EFFECT callback (its
// side effects must stay in list order) keeps the sequential `almide_rt_fan_map`.
// The callback is a raw `impl Fn` (the box pass leaves fan args unboxed), so
// a non-Send capture the pass missed is a loud rustc error, never a race.
//
// Observably identical to the sequential twin: results land in pre-sized
// slots by index, and the map's Err is the FIRST Err in LIST ORDER. A worker
// stops at any index above an Err already recorded, so elements past the
// first Err are evaluated only while that Err is still in flight — pure, hence
// unobservable, except through a TRAP in such an element (the sequential twin,
// which the wasm leg is, would never reach it). Threshold 2, not a "big list"
// cut-off: `fan.map` IS the user's fan-out signal, each element a task (a
// 64-chunk fan over 14 cores would never clear a size threshold).
// `ALMIDE_FAN_SEQUENTIAL=1` forces the sequential path — the ablation knob.
pub fn almide_rt_fan_map_par<A: Send + Sync + Clone, B: Send, F: Fn(A) -> Result<B, String> + Send + Sync>(
    items: Vec<A>,
    f: F,
) -> Result<Vec<B>, String> {
    if items.len() < 2 || almide_rt_list_par_sequential() {
        return items.into_iter().map(&f).collect();
    }
    let workers = almide_rt_list_par_workers(items.len());
    let chunk_size = items.len().div_ceil(workers);
    let first_err = std::sync::atomic::AtomicUsize::new(usize::MAX);
    let mut slots: Vec<Option<Result<B, String>>> = (0..items.len()).map(|_| None).collect();
    std::thread::scope(|s| {
        for (chunk_idx, (chunk, out)) in items.chunks(chunk_size).zip(slots.chunks_mut(chunk_size)).enumerate() {
            let f = &f;
            let first_err = &first_err;
            let base = chunk_idx * chunk_size;
            s.spawn(move || {
                for (i, (slot, item)) in out.iter_mut().zip(chunk).enumerate() {
                    let idx = base + i;
                    if idx > first_err.load(std::sync::atomic::Ordering::Acquire) {
                        break;
                    }
                    let r = f(item.clone());
                    if r.is_err() {
                        first_err.fetch_min(idx, std::sync::atomic::Ordering::AcqRel);
                    }
                    *slot = Some(r);
                }
            });
        }
    });
    let mut out = Vec::with_capacity(items.len());
    for slot in slots {
        match slot {
            Some(Ok(v)) => out.push(v),
            Some(Err(e)) => return Err(e),
            // A slot is left empty only at an index ABOVE the first Err, and
            // that Err returns from this loop before the scan reaches it.
            None => unreachable!("fan.map_par: empty slot below the first Err"),
        }
    }
    Ok(out)
}

// `fan.race` returns the FIRST thunk in LIST ORDER to SETTLE — i.e. thunk[0]'s
// `Result` (Ok or Err), DETERMINISTIC (not wall-clock fastest, which is neither
// reproducible nor expressible on the single-threaded WASM target). It differs
// from `fan.any`, which SKIPS failures to find the first Ok: race surfaces
// thunk[0]'s Err. Since fan thunks are pure (capturing a `var` is a compile
// error), the non-head thunks are observably irrelevant, so evaluating only the
// head is equivalent to "start all, take the first to settle". An empty list is
// a defined Err, never a panic. EFFECTFUL: the caller's auto-`?` routes a head
// Err to the unified main-error exit, byte-identical to the wasm path.
pub fn almide_rt_fan_race<T>(
    thunks: Vec<impl Fn() -> Result<T, String>>,
) -> Result<T, String> {
    match thunks.into_iter().next() {
        Some(thunk) => thunk(),
        None => Err("fan.race: no candidates".to_string()),
    }
}

// `fan.any` tries the thunks in LIST ORDER and returns the FIRST `Ok`
// (deterministic — NOT wall-clock fastest). If every candidate fails it returns
// a defined `Err`, never panicking or trapping. This is intentionally NOT
// `fan.race` (which is parallel + wall-clock nondeterministic and stays as-is).
pub fn almide_rt_fan_any<T>(
    thunks: Vec<impl Fn() -> Result<T, String>>,
) -> Result<T, String> {
    for thunk in &thunks {
        if let Ok(val) = thunk() {
            return Ok(val);
        }
    }
    Err("fan.any: all candidates failed".to_string())
}

// `fan.any_map` -- the T2-3 MAPPER form: apply `f` to the items IN LIST ORDER
// and return the FIRST `Ok`; an element's Err disqualifies that element only.
// All-fail (and the empty list) is the ledger-constant Err. Deterministic --
// the early cut is structural (later elements are never evaluated).
pub fn almide_rt_fan_any_map<A, B>(
    items: Vec<A>,
    f: std::rc::Rc<dyn Fn(A) -> Result<B, String>>,
) -> Result<B, String> {
    for item in items {
        if let Ok(v) = f(item) {
            return Ok(v);
        }
    }
    Err("fan.any: all candidates failed".to_string())
}

pub fn almide_rt_fan_settle<T: Send + 'static>(
    thunks: Vec<impl Fn() -> Result<T, String> + Send + Sync>,
) -> Vec<Result<T, String>> {
    std::thread::scope(|s| {
        let handles: Vec<_> = thunks
            .iter()
            .map(|thunk| s.spawn(move || thunk()))
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().unwrap())
            .collect()
    })
}

