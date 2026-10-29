// ffi — memory at the foreign-function boundary (R-ffi-01, #3853, ADR-0031).
//
// The native oracle for the three `local` raw operations of stdlib/ffi.almd;
// the wasm bodies are in stdlib/ffi_bridge.almd. They are reached only through
// `ffi.read` / `ffi.write` / `ffi.to_bytes`, which check `0 <= off` and
// `off + n <= len` against the Region's own length — and, for a write, its
// permission — in Almide before calling here. The length came from the mint
// (`ffi.region`: the Bytes it views; `ffi.alloc`: the requested size; an
// `@extern` with `@region(len = …)`: the binding's declared length), never
// from the caller of a read or write.

// An EMPTY buffer has no data region, so its address is NULL on both targets:
// `Vec::as_ptr` of an empty Vec is a dangling sentinel, and a write through it
// was UB (C-062).
pub fn almide_rt_ffi_address(b: &Vec<u8>) -> *mut u8 {
    if b.is_empty() { std::ptr::null_mut() } else { b.as_ptr() as *mut u8 }
}

// The address `off` bytes past `base` (`ffi.slice`): reached only after the
// slice's bounds check against its parent's length, so the result stays inside
// the parent. Null stays null (an empty region has no data). No memory is
// touched, so no `unsafe`.
pub fn almide_rt_ffi_offset(base: *mut u8, off: i64) -> *mut u8 {
    if base.is_null() || off <= 0 { base } else { base.wrapping_add(off as usize) }
}

pub fn almide_rt_ffi_load(base: *mut u8, off: i64, n: i64) -> Vec<u8> {
    if base.is_null() || n <= 0 || off < 0 { return Vec::new(); }
    // SAFETY: `ffi.read` checked `off + n <= len` for the Region `base` came
    // from, and the Region keeps its owner (or its foreign release guard)
    // alive while it is borrowed here (ADR-0031 D2).
    unsafe { std::slice::from_raw_parts(base.add(off as usize), n as usize).to_vec() }
}

pub fn almide_rt_ffi_store(base: *mut u8, off: i64, b: &Vec<u8>) {
    if base.is_null() || b.is_empty() || off < 0 { return; }
    // SAFETY: `ffi.write` checked the Region is writable (it came from
    // `ffi.alloc`, whose buffer no Almide value aliases, or from a foreign
    // `@region` declaration) and that `off + len(b) <= len`.
    unsafe { std::ptr::copy_nonoverlapping(b.as_ptr(), base.add(off as usize), b.len()); }
}

// The release guard of a foreign Region (ADR-0031 D6): the `@extern(c)`
// wrapper of a fn declared `@region(len = …, release = "f")` stores this
// closure in the Region it builds, with `release` the glue's call of `f`.
// Every copy of the Region shares the one `Rc`, so the guard drops — and
// calls `release(base)` once — when the last copy goes away. Almide code never
// frees, so there is no double free and no use after free on the Almide side.
pub struct AlmideFfiReleaseGuard<F: FnMut(*mut u8)> {
    base: *mut u8,
    release: F,
}

impl<F: FnMut(*mut u8)> Drop for AlmideFfiReleaseGuard<F> {
    fn drop(&mut self) {
        if !self.base.is_null() {
            (self.release)(self.base)
        }
    }
}

pub fn almide_rt_ffi_release_guard<F: FnMut(*mut u8) + 'static>(base: *mut u8, release: F) -> std::rc::Rc<dyn Fn() -> ()> {
    let guard = AlmideFfiReleaseGuard { base, release };
    std::rc::Rc::new(move || { let _ = &guard; })
}

// The `keep` of a Region an `@export(c)` fn is lent by its C caller (ADR-0031
// amendment A2): the memory is the caller's, borrowed for the one call. The
// wrapper holds this `Rc` while the Almide fn runs, and every copy of the
// Region shares it, so after the call `almide_rt_ffi_lent_end` can see whether
// a copy outlived the call.
pub fn almide_rt_ffi_lent_keep() -> std::rc::Rc<dyn Fn() -> ()> {
    std::rc::Rc::new(|| ())
}

// A lent Region that is still referenced after its call returned would read
// or write the C caller's memory after the caller may have freed it. Nothing
// can take the lend back from every copy, so the process stops instead, the
// way Go's cgocheck does, before the caller regains control.
pub fn almide_rt_ffi_lent_end(keep: &std::rc::Rc<dyn Fn() -> ()>, symbol: &str, param: &str) {
    if std::rc::Rc::strong_count(keep) > 1 {
        eprintln!("ffi: the Region `{param}` lent to @export(c) fn `{symbol}` is still held after the call returned; a lent Region lives only for its call");
        std::process::abort();
    }
}
