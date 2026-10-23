// ffi — raw addresses at the foreign-function boundary (R-ffi-01, #3853).
//
// The native oracle for `ffi.{address,to_int,read,write}`; the wasm bodies are
// in stdlib/bytes_rawptr.almd. `read` and `write` are the only stdlib functions that
// reach memory outside an Almide value (R-L0-45): nothing here can check that
// an address and a length describe memory the program owns, so the caller of
// `ffi.read` / `ffi.write` promises it, and `--profile critical` refuses both
// (#4008).

// An EMPTY buffer has no data region, so its address is NULL on both targets:
// `Vec::as_ptr` of an empty Vec is a dangling sentinel, and a write through it
// was UB (C-062).
pub fn almide_rt_ffi_address(b: &Vec<u8>) -> *mut u8 {
    if b.is_empty() { std::ptr::null_mut() } else { b.as_ptr() as *mut u8 }
}

pub fn almide_rt_ffi_to_int(p: *mut u8) -> i64 {
    p as usize as i64
}

pub fn almide_rt_ffi_read(ptr: *mut u8, len: i64) -> Vec<u8> {
    if ptr.is_null() || len <= 0 { return Vec::new(); }
    // SAFETY: none provided here — `ffi.read` is the declared boundary where the
    // caller promises `len` readable bytes at `ptr` (R-L0-45).
    unsafe { std::slice::from_raw_parts(ptr, len as usize).to_vec() }
}

pub fn almide_rt_ffi_write(ptr: *mut u8, b: &Vec<u8>, cap: i64) -> i64 {
    if ptr.is_null() { return 0; }
    let n = b.len().min(cap as usize);
    // SAFETY: none provided here — `ffi.write` is the declared boundary where the
    // caller promises `cap` writable bytes at `ptr` (R-L0-45).
    unsafe { std::ptr::copy_nonoverlapping(b.as_ptr(), ptr, n); }
    n as i64
}
