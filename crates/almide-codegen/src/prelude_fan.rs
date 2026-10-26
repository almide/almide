//! Per-element output timelines for `fan` (ADR-0024 D5/D6, ADR-0011 D1).
//!
//! A `fan` whose elements run on threads must still be OBSERVED as the
//! sequential evaluation: element 0, then element 1, and so on. Each element
//! therefore writes into its own timeline — stdout and stderr interleaved in
//! one sequence of segments, split into the two fds only when flushed — and
//! the timelines reach the enclosing output in list order:
//!
//! - The HEAD (the lowest element not yet finished) streams straight through,
//!   since nothing precedes it. Every other element buffers.
//! - When an element finishes, every finished element from the head up is
//!   flushed in order, and the next unfinished element's buffer so far is
//!   flushed as it becomes the head.
//! - A nested fan's "enclosing output" is the element that started it, so
//!   its timelines flush into that element's timeline, not into the fds.
//!
//! A trap in element k (`almide_abort`, `almide_panic_abort`, `process.exit`,
//! and through them a failed assert) calls `almide_fan_trap_wait` first: it
//! waits until every element below k has finished and been flushed, so the
//! output up to and including k's partial timeline is exactly what the
//! sequential evaluation would have printed, and nothing above k appears. A
//! lower element that traps meanwhile wins (the higher one keeps waiting while
//! the winner exits the process). Then the same wait runs one level up, for
//! the element that started this fan, and the abort proceeds.
//!
//! A helper thread that works FOR an element (the `list.par_*` chunk workers
//! under a `fan { }` arm) adopts that element's sink, so a trap on it waits
//! like a trap on the element's own thread.
//!
//! Lock order is always child group → parent group, so routing a write
//! upward while holding the child's lock cannot deadlock.
//!
//! An arm starts through `almide_fan_spawn` (#4141): on a thread of its own
//! while fewer than `ALMIDE_FAN_ARM_THREADS_PER_CPU` × CPUs (one per CPU, at
//! most `ALMIDE_FAN_THREADS`) arm and line-fold threads are live, else — or when the OS refuses the thread — INLINE on the thread that
//! reached it, before the next arm starts. That is the sequential evaluation
//! the threaded form reproduces, so the observation does not change; only the
//! overlap is given up. `Scope::spawn` panicked instead (exit 101 with a std
//! source path): a recursive two-arm fan of depth 11 under `ulimit -u 2048`.

const FAN_PRELUDE: &str = r#"
#[derive(Clone)] VIS struct AlmideFanSink { group: std::sync::Arc<AlmideFanGroup>, index: usize }
VIS struct AlmideFanGroup { state: std::sync::Mutex<AlmideFanState>, cv: std::sync::Condvar, parent: Option<AlmideFanSink> }
VIS struct AlmideFanState { head: usize, done: Vec<bool>, trapped: Vec<bool>, bufs: Vec<Vec<(bool, Vec<u8>)>> }
VIS struct AlmideFanElem(AlmideFanSink, Option<AlmideFanSink>);
thread_local! { VIS static ALMIDE_FAN_SINK: std::cell::RefCell<Option<AlmideFanSink>> = const { std::cell::RefCell::new(None) }; }
fn almide_fan_lock(g: &AlmideFanGroup) -> std::sync::MutexGuard<'_, AlmideFanState> { g.state.lock().unwrap_or_else(|e| e.into_inner()) }
VIS fn almide_fan_current() -> Option<AlmideFanSink> { ALMIDE_FAN_SINK.try_with(|c| c.borrow().clone()).ok().flatten() }
VIS fn almide_fan_adopt(sink: Option<AlmideFanSink>) { almide_stack_guard_register(); let _ = ALMIDE_FAN_SINK.try_with(|c| *c.borrow_mut() = sink); }
VIS fn almide_fan_active() -> bool { ALMIDE_FAN_SINK.try_with(|c| c.borrow().is_some()).unwrap_or(false) }
fn almide_out_real(err: bool, bytes: &[u8]) { if err { let _ = std::io::Write::write_all(&mut std::io::stderr().lock(), bytes); } else { ALMIDE_STDOUT_BUF.with(|buf| { let mut w = buf.borrow_mut(); let _ = std::io::Write::write_all(&mut *w, bytes); if bytes.contains(&b'\n') || almide_stdout_is_terminal() { let _ = std::io::Write::flush(&mut *w); } }); } }
fn almide_out_route(sink: Option<&AlmideFanSink>, err: bool, bytes: &[u8]) {
    let mut cur = sink;
    while let Some(s) = cur {
        let mut st = almide_fan_lock(&s.group);
        if st.head != s.index {
            let segs = &mut st.bufs[s.index];
            match segs.last_mut() { Some((e, b)) if *e == err => b.extend_from_slice(bytes), _ => segs.push((err, bytes.to_vec())) }
            return;
        }
        drop(st);
        cur = s.group.parent.as_ref();
    }
    almide_out_real(err, bytes)
}
VIS fn almide_out_write(err: bool, bytes: &[u8]) { let sink = almide_fan_current(); almide_out_route(sink.as_ref(), err, bytes) }
VIS fn almide_fan_group(n: usize) -> std::sync::Arc<AlmideFanGroup> {
    almide_stdout_flush();
    let state = AlmideFanState { head: 0, done: vec![false; n], trapped: vec![false; n], bufs: (0..n).map(|_| Vec::new()).collect() };
    std::sync::Arc::new(AlmideFanGroup { state: std::sync::Mutex::new(state), cv: std::sync::Condvar::new(), parent: almide_fan_current() })
}
VIS fn almide_fan_enter(g: &std::sync::Arc<AlmideFanGroup>, index: usize) -> AlmideFanElem {
    almide_stack_guard_register();
    let sink = AlmideFanSink { group: g.clone(), index };
    let prev = ALMIDE_FAN_SINK.with(|c| c.replace(Some(sink.clone())));
    AlmideFanElem(sink, prev)
}
fn almide_fan_drain(g: &AlmideFanGroup, st: &mut AlmideFanState) {
    while st.head < st.done.len() {
        let h = st.head;
        for (e, b) in std::mem::take(&mut st.bufs[h]) { almide_out_route(g.parent.as_ref(), e, &b); }
        if !st.done[h] { break; }
        st.head += 1;
    }
    almide_stdout_flush();
}
impl Drop for AlmideFanElem {
    fn drop(&mut self) {
        almide_stdout_flush();
        let g = &self.0.group;
        let mut st = almide_fan_lock(g);
        st.done[self.0.index] = true;
        almide_fan_drain(g, &mut st);
        drop(st);
        g.cv.notify_all();
        let prev = self.1.take();
        let _ = ALMIDE_FAN_SINK.try_with(|c| *c.borrow_mut() = prev);
    }
}
VIS fn almide_fan_trap_wait() {
    almide_stdout_flush();
    let mut cur = almide_fan_current();
    while let Some(s) = cur {
        let g = &s.group;
        let mut st = almide_fan_lock(g);
        st.trapped[s.index] = true;
        g.cv.notify_all();
        loop {
            if st.head >= s.index && !st.trapped[..s.index].contains(&true) { break; }
            st = g.cv.wait(st).unwrap_or_else(|e| e.into_inner());
        }
        drop(st);
        cur = g.parent.clone();
    }
}
VIS fn almide_fan_join<T>(h: std::thread::ScopedJoinHandle<'_, T>) -> T { match h.join() { Ok(v) => v, Err(p) => std::panic::resume_unwind(p) } }
VIS enum AlmideFanArm<'s, T> { Spawned(std::thread::ScopedJoinHandle<'s, T>), Ran(T) }
static ALMIDE_FAN_ARM_THREADS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
fn almide_fan_arm_threads_max() -> usize {
    static MAX: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    *MAX.get_or_init(|| {
        // The generated program's own read of its run-time switch (not a
        // compiler read, so not through almide_base::env).
        const ALMIDE_FAN_THREADS_ENV: &str = "ALMIDE_FAN_THREADS";
        let cap = std::env::var(ALMIDE_FAN_THREADS_ENV).ok().and_then(|v| v.parse::<usize>().ok()).filter(|&n| n > 0).unwrap_or(usize::MAX);
        std::thread::available_parallelism().map_or(1, |n| n.get()).saturating_mul(ALMIDE_FAN_ARM_THREADS_PER_CPU).min(cap).max(1)
    })
}
struct AlmideFanArmSlot;
impl Drop for AlmideFanArmSlot { fn drop(&mut self) { ALMIDE_FAN_ARM_THREADS.fetch_sub(1, std::sync::atomic::Ordering::SeqCst); } }
VIS fn almide_fan_spawn<'s, 'e, T: Send + 's, F: FnOnce() -> T + Send + 's>(s: &'s std::thread::Scope<'s, 'e>, f: F) -> AlmideFanArm<'s, T> {
    if ALMIDE_FAN_ARM_THREADS.fetch_add(1, std::sync::atomic::Ordering::SeqCst) >= almide_fan_arm_threads_max() {
        drop(AlmideFanArmSlot);
        return AlmideFanArm::Ran(f());
    }
    let slot = AlmideFanArmSlot;
    let cell = std::sync::Arc::new(std::sync::Mutex::new(Some(f)));
    let theirs = cell.clone();
    let spawned = std::thread::Builder::new().spawn_scoped(s, move || {
        let _slot = slot;
        let f = theirs.lock().unwrap_or_else(|e| e.into_inner()).take();
        match f { Some(f) => f(), None => unreachable!("a fan arm runs once") }
    });
    match spawned {
        Ok(h) => AlmideFanArm::Spawned(h),
        Err(_) => {
            let f = cell.lock().unwrap_or_else(|e| e.into_inner()).take();
            match f { Some(f) => AlmideFanArm::Ran(f()), None => unreachable!("a fan arm runs once") }
        }
    }
}
VIS fn almide_fan_take<T>(arm: AlmideFanArm<'_, T>) -> T { match arm { AlmideFanArm::Spawned(h) => almide_fan_join(h), AlmideFanArm::Ran(v) => v } }
"#;

/// Live worker threads per available CPU that `fan { }` arms and the
/// line-fold workers (`fs.fold_lines_chunked`) share before a further one runs
/// inline on the thread that reached it (#4141): one, so the threads they start
/// are a pool bounded by `available_parallelism` (and by `ALMIDE_FAN_THREADS`,
/// the lever the `list.par_*` pool reads too). A recursive fan of depth d asked
/// for 2^d threads; an arm that gets no thread is the sequential evaluation
/// (ADR-0024 D1), so only the overlap is given up — including for arms that
/// wait on I/O, which overlap at most one per CPU.
/// tests/fan_spawn_failure_test.rs counts the live threads of a deep fan
/// against this bound.
const ALMIDE_FAN_ARM_THREADS_PER_CPU: usize = 1;

/// The fan timeline prelude as emitted into a program (`vis` is `pub ` for
/// the `almide_rt` rlib, empty for an inline main).
pub(crate) fn fan_timeline_prelude(vis: &str) -> String {
    let per_cpu = format!("const ALMIDE_FAN_ARM_THREADS_PER_CPU: usize = {ALMIDE_FAN_ARM_THREADS_PER_CPU};\n");
    per_cpu + &FAN_PRELUDE.replace("VIS ", vis)
}
