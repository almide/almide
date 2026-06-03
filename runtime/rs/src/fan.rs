// Fan concurrency runtime functions

// `fan.map` runs SEQUENTIALLY over an `Rc<dyn Fn>` thunk (the uniform closure
// repr). This is what lets a closure VALUE bound to a var reach `fan.map`: that
// value is an `Rc<dyn Fn>`, which is neither `Send` nor `Sync`, so it could not
// be moved across the `thread::scope` boundary the parallel version required.
// Results are identical to a parallel map (input order preserved); only the
// (unobservable) parallelism is dropped. race/any/settle/timeout keep their
// thread::scope and receive `Box<dyn Fn + Send[+ Sync]>` thunks (the box pass
// boxes them), which still satisfy the existing `impl Fn + Send + Sync` bounds.
pub fn almide_rt_fan_map<A, B>(
    items: Vec<A>,
    f: std::rc::Rc<dyn Fn(A) -> Result<B, String>>,
) -> Vec<B> {
    items.into_iter().map(|item| f(item).unwrap()).collect()
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

pub fn almide_rt_fan_any<T: Send + 'static>(
    thunks: Vec<impl Fn() -> Result<T, String> + Send + Sync>,
) -> T {
    almide_rt_fan_race(thunks)
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
