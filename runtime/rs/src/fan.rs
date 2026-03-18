// Fan concurrency runtime functions

pub fn almide_rt_fan_map<A: Send + 'static, B: Send + 'static>(
    items: Vec<A>,
    f: impl Fn(A) -> Result<B, String> + Send + Sync + Clone,
) -> Vec<B> {
    if items.is_empty() {
        return Vec::new();
    }
    std::thread::scope(|s| {
        let handles: Vec<_> = items
            .into_iter()
            .map(|item| {
                let f = f.clone();
                s.spawn(move || f(item))
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().unwrap().unwrap())
            .collect()
    })
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
