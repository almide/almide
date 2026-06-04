// Fan concurrency runtime functions

// `fan.map` runs SEQUENTIALLY over an `Rc<dyn Fn>` thunk (the uniform closure
// repr). This is what lets a closure VALUE bound to a var reach `fan.map`: that
// value is an `Rc<dyn Fn>`, which is neither `Send` nor `Sync`, so it could not
// be moved across the `thread::scope` boundary the parallel version required.
// Results are identical to a parallel map (input order preserved); only the
// (unobservable) parallelism is dropped. race/settle/timeout keep their
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

pub fn almide_rt_fan_race<T: Send + 'static>(
    thunks: Vec<impl Fn() -> Result<T, String> + Send + Sync>,
) -> T {
    use std::sync::mpsc;
    let (tx, rx) = mpsc::channel();
    std::thread::scope(|s| {
        for thunk in &thunks {
            let tx = tx.clone();
            s.spawn(move || {
                if let Ok(val) = thunk() {
                    let _ = tx.send(val);
                }
            });
        }
        drop(tx);
        rx.recv().unwrap()
    })
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

pub fn almide_rt_fan_timeout<T: Send + 'static>(
    ms: i64,
    thunk: impl Fn() -> Result<T, String> + Send,
) -> Result<T, String> {
    use std::sync::mpsc;
    use std::time::Duration;
    let (tx, rx) = mpsc::channel();
    std::thread::scope(|s| {
        s.spawn(move || {
            let result = thunk();
            let _ = tx.send(result);
        });
        rx.recv_timeout(Duration::from_millis(ms as u64))
            .unwrap_or_else(|_| Err("timeout".to_string()))
    })
}
