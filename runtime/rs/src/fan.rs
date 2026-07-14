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

