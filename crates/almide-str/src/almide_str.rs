// AlmideStr: ADR-0013's shared immutable string buffer (stage 2, step 2).
//
// One heap block `[rc, len, cap | bytes]`, the wasm leg's string layout:
// - `Clone` adds one to `rc`.
// - `Drop` takes one away and frees the block at zero.
// - `rc == 0` marks an immortal static block (the empty string, literals). It is never written and never freed.
// Mutation goes through `unique()`, which copies a shared or static block first (`Rc::make_mut` semantics), then keeps `len` and `cap` in locals until the view ends. Writes through the byte pointer cannot force those to reload (docs/roadmap/active/adr-0013-string-sharing.md, "the unique view").
//
// Thread-shared blocks (ADR-0013 design point 3, Koka's negative count): a
// block reachable from a value that crosses into a scoped thread carries the
// `ALMIDE_STR_SHARED` bit, and its count then changes only atomically. An
// unmarked block belongs to one thread, and its count is a plain load and
// store (`Relaxed` atomics compile to ordinary moves). The compiler marks at
// every boundary it emits (`almide_str_mark_reachable`); that marking is what
// the `Send` / `Sync` impls below rely on.
//
// Splice discipline (as almide-rt-core): this file must also compile as flat text inside the runtime module, so there are no `crate::` paths and `use` lines appear at the top level only.

use std::alloc::{alloc, dealloc, handle_alloc_error, realloc, Layout};
use std::borrow::Borrow;
use std::cmp::Ordering;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::ops::Deref;
use std::ptr::NonNull;
use std::sync::atomic::{fence, AtomicUsize, Ordering as AtomicOrdering};

/// The block header. The bytes follow it directly, at offset `ALMIDE_STR_HDR`.
#[repr(C)]
pub struct AlmideStrHeader {
    rc: AtomicUsize,
    len: usize,
    cap: usize,
}

const ALMIDE_STR_HDR: usize = std::mem::size_of::<AlmideStrHeader>();

/// The thread-shared bit of `rc`: the block may be owned from several threads.
const ALMIDE_STR_SHARED: usize = 1 << (usize::BITS - 1);

/// Non-zero while a thread marks a value: every block cloned in that window
/// is marked, so a clone of the value marks everything it reaches. A clone on
/// another thread in the same window marks its own block too, which is only
/// slower, never wrong.
static ALMIDE_STR_MARKING: AtomicUsize = AtomicUsize::new(0);

/// Marks every block reachable from `v` as thread-shared (Koka's
/// `kk_block_mark_shared`), by cloning `v` inside a marking window. The
/// compiler calls this on every value it hands to a scoped thread, and on
/// every top-level value, before any other thread can reach it.
pub fn almide_str_mark_reachable<T: Clone>(v: &T) {
    struct Window;
    impl Drop for Window {
        fn drop(&mut self) {
            ALMIDE_STR_MARKING.fetch_sub(1, AtomicOrdering::SeqCst);
        }
    }
    ALMIDE_STR_MARKING.fetch_add(1, AtomicOrdering::SeqCst);
    let _window = Window;
    drop(v.clone());
}

/// An immortal block in static memory: a header with `rc == 0` and `N` bytes.
/// Built at compile time by `AlmideStrStatic::new`, wrapped by `AlmideStr::from_static`.
#[repr(C)]
pub struct AlmideStrStatic<const N: usize> {
    header: AlmideStrHeader,
    bytes: [u8; N],
}

impl<const N: usize> AlmideStrStatic<N> {
    pub const fn new(s: &str) -> Self {
        assert!(s.len() == N, "AlmideStrStatic<N> needs a string of exactly N bytes");
        let src = s.as_bytes();
        let mut bytes = [0u8; N];
        let mut i = 0;
        while i < N {
            bytes[i] = src[i];
            i += 1;
        }
        AlmideStrStatic { header: AlmideStrHeader { rc: AtomicUsize::new(0), len: N, cap: N }, bytes }
    }
}

static ALMIDE_STR_EMPTY: AlmideStrStatic<0> = AlmideStrStatic::new("");

fn almide_str_layout(cap: usize) -> Layout {
    let size = ALMIDE_STR_HDR.checked_add(cap).expect("AlmideStr capacity overflow");
    Layout::from_size_align(size, std::mem::align_of::<AlmideStrHeader>()).expect("AlmideStr capacity overflow")
}

/// A fresh heap block with `rc = 1`, the given capacity and length; the bytes are uninitialised.
#[inline]
fn almide_str_alloc(cap: usize, len: usize) -> NonNull<AlmideStrHeader> {
    let layout = almide_str_layout(cap);
    // SAFETY: the layout's size is at least the header's, so it is non-zero.
    let raw = unsafe { alloc(layout) }.cast::<AlmideStrHeader>();
    let block = NonNull::new(raw).unwrap_or_else(|| handle_alloc_error(layout));
    // SAFETY: the block is freshly allocated, sized and aligned for the header.
    unsafe { block.as_ptr().write(AlmideStrHeader { rc: AtomicUsize::new(1), len, cap }) };
    block
}

/// Resizes a unique heap block so `extra` more bytes fit after `len`. The capacity at least doubles. Returns the block and its new capacity.
#[cold]
#[inline(never)]
fn almide_str_grow(block: NonNull<AlmideStrHeader>, len: usize, cap: usize, extra: usize) -> (NonNull<AlmideStrHeader>, usize) {
    let need = len.checked_add(extra).expect("AlmideStr capacity overflow");
    let new_cap = need.max(cap.saturating_mul(2)).max(8);
    let new_size = almide_str_layout(new_cap).size();
    // SAFETY: the caller holds the only owner of a heap block (static ones were copied by `unique_with`), allocated with `cap`'s layout.
    let raw = unsafe { realloc(block.as_ptr().cast(), almide_str_layout(cap), new_size) }.cast::<AlmideStrHeader>();
    let grown = NonNull::new(raw).unwrap_or_else(|| handle_alloc_error(almide_str_layout(new_cap)));
    // SAFETY: `grown` is the live, resized block; only its capacity changes here.
    unsafe { (*grown.as_ptr()).cap = new_cap };
    (grown, new_cap)
}

pub struct AlmideStr {
    ptr: NonNull<AlmideStrHeader>,
}

impl AlmideStr {
    /// The empty string, which does not allocate.
    pub fn new() -> Self {
        AlmideStr::from_static(&ALMIDE_STR_EMPTY)
    }

    pub fn from_static<const N: usize>(block: &'static AlmideStrStatic<N>) -> Self {
        // The pointer is derived from the whole block, so it covers the bytes. A static block has `rc == 0` and is only ever read.
        AlmideStr { ptr: NonNull::from(block).cast() }
    }

    pub fn with_capacity(cap: usize) -> Self {
        if cap == 0 { AlmideStr::new() } else { AlmideStr { ptr: almide_str_alloc(cap, 0) } }
    }

    #[inline]
    fn rc_cell(&self) -> &AtomicUsize {
        // SAFETY: `ptr` always points at a live header, heap or static; only the count is borrowed.
        unsafe { &(*self.ptr.as_ptr()).rc }
    }

    #[inline]
    fn rc(&self) -> usize {
        self.rc_cell().load(AtomicOrdering::Relaxed)
    }

    /// True when the block may be owned from several threads.
    pub fn is_thread_shared(&self) -> bool {
        self.rc() & ALMIDE_STR_SHARED != 0
    }

    /// Marks this block thread-shared. An immortal static block is never written, so it stays as it is.
    pub fn mark_shared(&self) {
        if self.rc() != 0 {
            self.rc_cell().fetch_or(ALMIDE_STR_SHARED, AtomicOrdering::Relaxed);
        }
    }

    #[inline]
    pub fn len(&self) -> usize {
        // SAFETY: as in `rc`.
        unsafe { (*self.ptr.as_ptr()).len }
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[inline]
    pub fn capacity(&self) -> usize {
        // SAFETY: as in `rc`.
        unsafe { (*self.ptr.as_ptr()).cap }
    }

    /// True when this handle is the block's only owner, so `unique()` will not copy.
    #[inline]
    pub fn is_unique(&self) -> bool {
        // `Acquire` pairs with the `Release` of another thread's last drop, so its reads of the bytes come before our writes.
        self.rc_cell().load(AtomicOrdering::Acquire) & !ALMIDE_STR_SHARED == 1
    }

    /// The number of owners, or 0 for an immortal static block.
    pub fn ref_count(&self) -> usize {
        self.rc() & !ALMIDE_STR_SHARED
    }

    /// True when both handles share one block.
    pub fn ptr_eq(a: &AlmideStr, b: &AlmideStr) -> bool {
        a.ptr == b.ptr
    }

    #[inline]
    fn data(&self) -> *mut u8 {
        // SAFETY: the bytes start right after the header, inside the same block.
        unsafe { self.ptr.as_ptr().cast::<u8>().add(ALMIDE_STR_HDR) }
    }

    #[inline]
    pub fn as_str(&self) -> &str {
        // SAFETY: the first `len` bytes are initialised UTF-8 (every writer copies whole `&str`s). Only a unique view writes them, and it holds `&mut self`.
        unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(self.data(), self.len())) }
    }

    /// A unique view for a run of in-place edits. A shared or static block is copied first; the copy has room for `extra` more bytes.
    pub fn unique_with(&mut self, extra: usize) -> UniqueStr<'_> {
        if !self.is_unique() {
            let len = self.len();
            let fresh = almide_str_alloc(len.checked_add(extra).expect("AlmideStr capacity overflow"), len);
            let copy = AlmideStr { ptr: fresh };
            // SAFETY: `copy` is a new block with room for `len` bytes; the source is a different block.
            unsafe { std::ptr::copy_nonoverlapping(self.data(), copy.data(), len) };
            *self = copy;
        }
        let (len, cap, data) = (self.len(), self.capacity(), self.data());
        UniqueStr { s: self, len, cap, data }
    }

    pub fn unique(&mut self) -> UniqueStr<'_> {
        self.unique_with(0)
    }

    #[inline]
    pub fn push_str(&mut self, t: &str) {
        let (len, n) = (self.len(), t.len());
        if self.is_unique() && self.capacity() - len >= n {
            // SAFETY: a unique heap block with room for `n` more bytes; `t` is not inside it (this handle is its only owner and is held mutably).
            unsafe {
                std::ptr::copy_nonoverlapping(t.as_ptr(), self.data().add(len), n);
                (*self.ptr.as_ptr()).len = len + n;
            }
            return;
        }
        self.unique_with(n).push_str(t);
    }

    #[inline]
    pub fn push(&mut self, c: char) {
        let mut buf = [0u8; 4];
        self.push_str(c.encode_utf8(&mut buf));
    }

    /// Empties the string. A shared block is left to its other owners rather than copied.
    pub fn clear(&mut self) {
        if self.is_unique() { self.unique().clear() } else { *self = AlmideStr::new() }
    }

    /// `self + rhs`. Appends in place when this handle is the only owner; otherwise builds one exact-size block.
    #[inline]
    pub fn concat(mut self, rhs: &str) -> AlmideStr {
        if self.is_unique() {
            self.push_str(rhs);
            return self;
        }
        let mut out = AlmideStr::with_capacity(self.len().checked_add(rhs.len()).expect("AlmideStr capacity overflow"));
        {
            let mut u = out.unique();
            u.push_str(self.as_str());
            u.push_str(rhs);
        }
        out
    }

    pub fn into_string(self) -> String {
        self.as_str().to_owned()
    }
}

impl Drop for AlmideStr {
    #[inline]
    fn drop(&mut self) {
        let rc = self.rc();
        if rc == 0 {
            return;
        }
        if rc & ALMIDE_STR_SHARED != 0 {
            if self.rc_cell().fetch_sub(1, AtomicOrdering::Release) != ALMIDE_STR_SHARED | 1 {
                return;
            }
            // Every other owner's use of the block happens before the free.
            fence(AtomicOrdering::Acquire);
        } else if rc > 1 {
            self.rc_cell().store(rc - 1, AtomicOrdering::Relaxed);
            return;
        }
        let layout = almide_str_layout(self.capacity());
        // SAFETY: the last owner frees the heap block with the layout it was allocated or grown with.
        unsafe { dealloc(self.ptr.as_ptr().cast(), layout) };
    }
}

impl Clone for AlmideStr {
    #[inline]
    fn clone(&self) -> Self {
        let rc = self.rc();
        if rc == 0 {
            return AlmideStr { ptr: self.ptr };
        }
        if rc & ALMIDE_STR_SHARED == 0 && ALMIDE_STR_MARKING.load(AtomicOrdering::Relaxed) == 0 {
            // An overflowing count would reach the shared bit; abort as `Rc` does.
            if rc + 1 >= ALMIDE_STR_SHARED {
                std::process::abort();
            }
            self.rc_cell().store(rc + 1, AtomicOrdering::Relaxed);
        } else {
            self.mark_shared();
            if self.rc_cell().fetch_add(1, AtomicOrdering::Relaxed) & !ALMIDE_STR_SHARED >= ALMIDE_STR_SHARED - 1 {
                std::process::abort();
            }
        }
        AlmideStr { ptr: self.ptr }
    }
}

/// A run of in-place edits on a uniquely owned block. `len`, `cap` and the byte pointer live in locals and are written back when the view drops.
// SAFETY: an `AlmideStr` handed to another thread was marked first
// (`almide_str_mark_reachable`, emitted at every thread boundary), so every
// count change on its block is atomic, and its bytes are only written through
// a unique view after an `Acquire` count of one. An unmarked block is never
// reachable from two threads.
unsafe impl Send for AlmideStr {}
// SAFETY: as for `Send`; `&AlmideStr` only reads the bytes or clones, and a
// clone of a marked block is atomic.
unsafe impl Sync for AlmideStr {}

pub struct UniqueStr<'a> {
    s: &'a mut AlmideStr,
    len: usize,
    cap: usize,
    data: *mut u8,
}

impl UniqueStr<'_> {
    #[inline]
    pub fn push_str(&mut self, t: &str) {
        let n = t.len();
        if self.cap - self.len < n {
            self.grow(n);
        }
        // SAFETY: `len + n <= cap` now, and `t` cannot point into this block, since the view holds the only owner mutably.
        unsafe { std::ptr::copy_nonoverlapping(t.as_ptr(), self.data.add(self.len), n) };
        self.len += n;
    }

    #[inline]
    pub fn push(&mut self, c: char) {
        if c.is_ascii() && self.len < self.cap {
            // SAFETY: `len < cap`, so one more byte fits; an ASCII byte is complete UTF-8.
            unsafe { self.data.add(self.len).write(c as u8) };
            self.len += 1;
            return;
        }
        let mut buf = [0u8; 4];
        self.push_str(c.encode_utf8(&mut buf));
    }

    pub fn clear(&mut self) {
        self.len = 0;
    }

    pub fn reserve(&mut self, extra: usize) {
        if self.cap - self.len < extra {
            self.grow(extra);
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn as_str(&self) -> &str {
        // SAFETY: the first `len` bytes are initialised UTF-8.
        unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(self.data, self.len)) }
    }

    // The cold path takes and returns values, never `&mut self`: if the view's address escaped into a call, the view would live on the stack, and every push would store `len` and reload `cap` and `data`.
    #[inline]
    fn grow(&mut self, extra: usize) {
        let (block, cap) = almide_str_grow(self.s.ptr, self.len, self.cap, extra);
        self.s.ptr = block;
        self.cap = cap;
        self.data = self.s.data();
    }
}

impl Drop for UniqueStr<'_> {
    fn drop(&mut self) {
        // SAFETY: the view's block is a unique heap block; `len <= cap` holds throughout.
        unsafe { (*self.s.ptr.as_ptr()).len = self.len };
    }
}

impl Default for AlmideStr {
    fn default() -> Self {
        AlmideStr::new()
    }
}

impl Deref for AlmideStr {
    type Target = str;
    #[inline]
    fn deref(&self) -> &str {
        self.as_str()
    }
}

impl AsRef<str> for AlmideStr {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl AsRef<[u8]> for AlmideStr {
    fn as_ref(&self) -> &[u8] {
        self.as_str().as_bytes()
    }
}

impl Borrow<str> for AlmideStr {
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

impl From<&str> for AlmideStr {
    #[inline]
    fn from(s: &str) -> Self {
        if s.is_empty() {
            return AlmideStr::new();
        }
        let out = AlmideStr { ptr: almide_str_alloc(s.len(), s.len()) };
        // SAFETY: a fresh block with room for exactly `s.len()` bytes, which `s` fills with UTF-8.
        unsafe { std::ptr::copy_nonoverlapping(s.as_ptr(), out.data(), s.len()) };
        out
    }
}

impl From<&String> for AlmideStr {
    fn from(s: &String) -> Self {
        AlmideStr::from(s.as_str())
    }
}

impl From<String> for AlmideStr {
    fn from(s: String) -> Self {
        AlmideStr::from(s.as_str())
    }
}

impl From<char> for AlmideStr {
    fn from(c: char) -> Self {
        let mut buf = [0u8; 4];
        AlmideStr::from(&*c.encode_utf8(&mut buf))
    }
}

impl From<AlmideStr> for String {
    fn from(s: AlmideStr) -> Self {
        s.into_string()
    }
}

impl FromIterator<char> for AlmideStr {
    fn from_iter<I: IntoIterator<Item = char>>(iter: I) -> Self {
        let mut out = AlmideStr::new();
        {
            let mut u = out.unique();
            for c in iter {
                u.push(c);
            }
        }
        out
    }
}

impl<'a> FromIterator<&'a str> for AlmideStr {
    fn from_iter<I: IntoIterator<Item = &'a str>>(iter: I) -> Self {
        let mut out = AlmideStr::new();
        {
            let mut u = out.unique();
            for s in iter {
                u.push_str(s);
            }
        }
        out
    }
}

impl PartialEq for AlmideStr {
    #[inline]
    fn eq(&self, other: &AlmideStr) -> bool {
        AlmideStr::ptr_eq(self, other) || self.as_str() == other.as_str()
    }
}

impl Eq for AlmideStr {}

impl PartialEq<str> for AlmideStr {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl PartialEq<&str> for AlmideStr {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

impl PartialEq<String> for AlmideStr {
    fn eq(&self, other: &String) -> bool {
        self.as_str() == other.as_str()
    }
}

impl PartialEq<AlmideStr> for str {
    fn eq(&self, other: &AlmideStr) -> bool {
        self == other.as_str()
    }
}

impl PartialEq<AlmideStr> for &str {
    fn eq(&self, other: &AlmideStr) -> bool {
        *self == other.as_str()
    }
}

impl PartialOrd for AlmideStr {
    fn partial_cmp(&self, other: &AlmideStr) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for AlmideStr {
    fn cmp(&self, other: &AlmideStr) -> Ordering {
        self.as_str().cmp(other.as_str())
    }
}

impl Hash for AlmideStr {
    // Hash as `str` so `Borrow<str>` lookups find the same bucket.
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_str().hash(state)
    }
}

impl fmt::Display for AlmideStr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self.as_str(), f)
    }
}

impl fmt::Debug for AlmideStr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.as_str(), f)
    }
}
