# Replace the counting allocator in a generated main.rs with one that also
# records a backtrace for every allocation still live when the report runs.
import sys, re
p = sys.argv[1]
s = open(p).read()
start = s.index("unsafe impl std::alloc::GlobalAlloc for __AlmideCountAlloc {")
end = s.index("#[global_allocator]", start)
tracked = r'''
thread_local! { static __PB_BYPASS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }
static __PB_ARMED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static __PB_LIVE: std::sync::Mutex<Vec<(usize, usize, String)>> = std::sync::Mutex::new(Vec::new());
fn __pb_bypassed() -> bool { __PB_BYPASS.with(|b| b.get()) }
fn __pb_track(p: *mut u8, size: usize) {
    if !__PB_ARMED.load(std::sync::atomic::Ordering::Relaxed) || __pb_bypassed() { return; }
    __PB_BYPASS.with(|b| b.set(true));
    let bt = format!("{}", std::backtrace::Backtrace::force_capture());
    __PB_LIVE.lock().unwrap().push((p as usize, size, bt));
    __PB_BYPASS.with(|b| b.set(false));
}
fn __pb_untrack(p: *mut u8) {
    if __pb_bypassed() { return; }
    __PB_BYPASS.with(|b| b.set(true));
    let mut v = __PB_LIVE.lock().unwrap();
    if let Some(i) = v.iter().position(|e| e.0 == p as usize) { v.swap_remove(i); }
    drop(v);
    __PB_BYPASS.with(|b| b.set(false));
}
fn __pb_moved(old: *mut u8, new: *mut u8, size: usize) {
    if __pb_bypassed() { return; }
    __PB_BYPASS.with(|b| b.set(true));
    let mut v = __PB_LIVE.lock().unwrap();
    if let Some(e) = v.iter_mut().find(|e| e.0 == old as usize) { e.0 = new as usize; e.1 = size; }
    drop(v);
    __PB_BYPASS.with(|b| b.set(false));
}
pub fn __pb_dump() {
    __PB_ARMED.store(false, std::sync::atomic::Ordering::Relaxed);
    __PB_BYPASS.with(|b| b.set(true));
    let v = __PB_LIVE.lock().unwrap();
    eprintln!("__PB_LIVE count={}", v.len());
    for (p, size, bt) in v.iter() { eprintln!("__PB_LIVE ptr={:#x} size={}\n{}", p, size, bt); }
}
unsafe impl std::alloc::GlobalAlloc for __AlmideCountAlloc {
    unsafe fn alloc(&self, l: std::alloc::Layout) -> *mut u8 {
        if __pb_bypassed() { return std::alloc::System.alloc(l); }
        __ALMIDE_ALLOCS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.grow(l.size());
        let p = std::alloc::System.alloc(l);
        __pb_track(p, l.size());
        p
    }
    unsafe fn alloc_zeroed(&self, l: std::alloc::Layout) -> *mut u8 {
        if __pb_bypassed() { return std::alloc::System.alloc_zeroed(l); }
        __ALMIDE_ALLOCS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.grow(l.size());
        let p = std::alloc::System.alloc_zeroed(l);
        __pb_track(p, l.size());
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, l: std::alloc::Layout) {
        if __pb_bypassed() { return std::alloc::System.dealloc(p, l); }
        __ALMIDE_DEALLOCS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        __ALMIDE_LIVE.fetch_sub(l.size(), std::sync::atomic::Ordering::Relaxed);
        __pb_untrack(p);
        std::alloc::System.dealloc(p, l)
    }
    unsafe fn realloc(&self, p: *mut u8, l: std::alloc::Layout, new: usize) -> *mut u8 {
        if __pb_bypassed() { return std::alloc::System.realloc(p, l, new); }
        __ALMIDE_REALLOCS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let q = std::alloc::System.realloc(p, l, new);
        __pb_moved(p, q, new);
        q
    }
}
'''
s = s[:start] + tracked + s[end:]
# arm on reset, dump on report
s = s.replace("fn arm() -> Self { __AlmideCountAlloc::reset(); __AlmideAllocGuard }",
              "fn arm() -> Self { __AlmideCountAlloc::reset(); __PB_ARMED.store(true, std::sync::atomic::Ordering::Relaxed); __AlmideAllocGuard }")
s = s.replace("fn drop(&mut self) { __AlmideCountAlloc::report(); }",
              "fn drop(&mut self) { __AlmideCountAlloc::report(); __pb_dump(); }")
assert "__pb_dump(); }" in s and "__PB_ARMED.store(true" in s
open(p, "w").write(s)
