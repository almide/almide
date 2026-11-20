// AlmideList: ADR-0037's shared list block (stage 1: the type, its tests and Miri).
//
// One heap block `[rc, len, cap | elements]` with a 32-byte, 16-aligned header:
// - `Clone` adds one to `rc`; it never copies the elements.
// - `Drop` takes one away; the last owner drops the elements and frees the block.
// - `rc == 0` marks the immortal static empty block. It is shared by every element
//   type aligned to at most 16, is never counted, written or freed, and is never
//   reported as unique, so the first write to an empty list allocates.
// Writes go through `make_mut()`, which copies a shared block first (`Rc::make_mut`
// semantics): the `&mut [T]` it lends borrows the handle, so the block cannot be
// cloned while it is lent (ADR-0037 Q1).
//
// Counts are atomic: a list may cross a `fan` thread, and the owner ruled atomic
// counts the default until a measured non-atomic fast path exists (ADR-0037 Q1).
// Inside a marking window (`almide_str_mark_reachable`) a clone also clones every
// element once, so the strings a list holds are marked as a clone of a `Vec` marked
// them.
//
// Splice discipline (as almide_str.rs, which this file is spliced after): no
// `crate::` paths, and `use` lines at the top level only. `ALMIDE_STR_MARKING` is
// almide_str.rs's marking window.

use std::marker::PhantomData;
use std::sync::atomic::AtomicUsize as AlmideListCount;

/// The block header. The elements follow it at [`almide_list_offset`].
#[repr(C, align(16))]
pub struct AlmideListHeader {
    rc: AlmideListCount,
    len: usize,
    cap: usize,
}

/// The header's size: 32 bytes on a 64-bit target, a multiple of its 16-byte alignment.
const ALMIDE_LIST_HDR: usize = std::mem::size_of::<AlmideListHeader>();

/// The largest element alignment the static empty block serves. A more aligned
/// element type allocates its empty list (ADR-0037 Q2).
const ALMIDE_LIST_STATIC_ALIGN: usize = std::mem::align_of::<AlmideListHeader>();

/// The one static empty block (ADR-0037 Q2): `rc == 0`, no elements, no capacity.
static ALMIDE_LIST_EMPTY: AlmideListHeader = AlmideListHeader { rc: AlmideListCount::new(0), len: 0, cap: 0 };

/// Where the elements of a `T` list start: the header rounded up to `T`'s alignment.
#[inline]
const fn almide_list_offset<T>() -> usize {
    let a = std::mem::align_of::<T>();
    (ALMIDE_LIST_HDR + a - 1) / a * a
}

/// The block's alignment for `T`: the header's, or `T`'s when that is larger.
#[inline]
const fn almide_list_align<T>() -> usize {
    let a = std::mem::align_of::<T>();
    if a > ALMIDE_LIST_STATIC_ALIGN { a } else { ALMIDE_LIST_STATIC_ALIGN }
}

fn almide_list_layout<T>(cap: usize) -> Layout {
    let elems = std::mem::size_of::<T>().checked_mul(cap).expect("AlmideList capacity overflow");
    let size = almide_list_offset::<T>().checked_add(elems).expect("AlmideList capacity overflow");
    Layout::from_size_align(size, almide_list_align::<T>()).expect("AlmideList capacity overflow")
}

/// A fresh heap block with `rc = 1`, `len = 0` and room for `cap` elements.
fn almide_list_alloc<T>(cap: usize) -> NonNull<AlmideListHeader> {
    let layout = almide_list_layout::<T>(cap);
    // SAFETY: the layout's size is at least the header's, so it is non-zero.
    let raw = unsafe { alloc(layout) }.cast::<AlmideListHeader>();
    let block = NonNull::new(raw).unwrap_or_else(|| handle_alloc_error(layout));
    // SAFETY: the block is freshly allocated, sized and aligned for the header.
    unsafe { block.as_ptr().write(AlmideListHeader { rc: AlmideListCount::new(1), len: 0, cap }) };
    block
}

pub struct AlmideList<T> {
    ptr: NonNull<AlmideListHeader>,
    _elems: PhantomData<T>,
}

impl<T> AlmideList<T> {
    /// The empty list: the static block when `T` is aligned to at most 16, else a fresh empty block.
    #[inline]
    pub fn new() -> Self {
        if std::mem::align_of::<T>() <= ALMIDE_LIST_STATIC_ALIGN {
            AlmideList { ptr: NonNull::from(&ALMIDE_LIST_EMPTY), _elems: PhantomData }
        } else {
            AlmideList { ptr: almide_list_alloc::<T>(0), _elems: PhantomData }
        }
    }

    /// An empty list with room for `cap` elements (the static block when `cap` is 0).
    pub fn with_capacity(cap: usize) -> Self {
        if cap == 0 {
            return Self::new();
        }
        AlmideList { ptr: almide_list_alloc::<T>(cap), _elems: PhantomData }
    }

    #[inline]
    fn header(&self) -> &AlmideListHeader {
        // SAFETY: `ptr` always points at a live header, heap or static.
        unsafe { self.ptr.as_ref() }
    }

    #[inline]
    fn is_static(&self) -> bool {
        self.header().rc.load(AtomicOrdering::Relaxed) == 0
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.header().len
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[inline]
    pub fn capacity(&self) -> usize {
        self.header().cap
    }

    /// True when this handle is the block's only owner, so `make_mut()` will not copy.
    /// Never true for the static empty block.
    #[inline]
    pub fn is_unique(&self) -> bool {
        // `Acquire` pairs with the `Release` of another owner's drop, so its reads of the elements come before our writes.
        self.header().rc.load(AtomicOrdering::Acquire) == 1
    }

    /// The number of owners, or 0 for the immortal static block.
    pub fn ref_count(&self) -> usize {
        self.header().rc.load(AtomicOrdering::Relaxed)
    }

    #[inline]
    fn data(&self) -> *mut T {
        // SAFETY: the elements start `almide_list_offset::<T>()` bytes into the block. For the static block (len and cap 0)
        // the pointer is only used for an empty slice, which needs it non-null and aligned: the header is 16-aligned and
        // the offset is a multiple of `T`'s alignment (at most 16 there), so it is both.
        unsafe { self.ptr.as_ptr().cast::<u8>().add(almide_list_offset::<T>()).cast::<T>() }
    }

    #[inline]
    pub fn as_slice(&self) -> &[T] {
        // SAFETY: the first `len` elements are initialised, and only a unique owner writes them, through `&mut self`.
        unsafe { std::slice::from_raw_parts(self.data(), self.len()) }
    }

    /// A unique block with room for `extra` more elements: the block itself when it is unique and has the room, else a copy.
    fn reserve_unique(&mut self, extra: usize) where T: Clone {
        let len = self.len();
        let need = len.checked_add(extra).expect("AlmideList capacity overflow");
        if self.is_unique() {
            if need > self.capacity() {
                self.grow(need);
            }
            return;
        }
        let cap = if extra == 0 { len } else { need.max(len.saturating_mul(2)).max(4) };
        let fresh = AlmideList::<T> { ptr: almide_list_alloc::<T>(cap), _elems: PhantomData };
        for (i, x) in self.as_slice().iter().enumerate() {
            // SAFETY: slot `i < len <= cap` of the fresh block is uninitialised; `len` is bumped after each write, so a
            // panicking `clone` leaves the fresh block holding exactly the elements written so far.
            unsafe {
                fresh.data().add(i).write(x.clone());
                (*fresh.ptr.as_ptr()).len = i + 1;
            }
        }
        *self = fresh;
    }

    /// Grows a unique heap block so `need` elements fit. The capacity at least doubles.
    #[cold]
    fn grow(&mut self, need: usize) {
        debug_assert!(self.is_unique());
        let old_cap = self.capacity();
        let new_cap = need.max(old_cap.saturating_mul(2)).max(4);
        let new_size = almide_list_layout::<T>(new_cap).size();
        // SAFETY: the caller holds the only owner of a heap block (rc == 1, never the static one), allocated with
        // `old_cap`'s layout; realloc keeps the alignment of that layout, which is `almide_list_align::<T>()`.
        let raw = unsafe { realloc(self.ptr.as_ptr().cast(), almide_list_layout::<T>(old_cap), new_size) }.cast::<AlmideListHeader>();
        self.ptr = NonNull::new(raw).unwrap_or_else(|| handle_alloc_error(almide_list_layout::<T>(new_cap)));
        // SAFETY: the block is unique and live.
        unsafe { (*self.ptr.as_ptr()).cap = new_cap };
    }

    /// The elements for writing. A shared or static block is copied first, so the write is never seen by another owner;
    /// the borrow of `self` keeps the list from being cloned while the slice is lent.
    pub fn make_mut(&mut self) -> &mut [T] where T: Clone {
        if self.is_empty() {
            return &mut [];
        }
        self.reserve_unique(0);
        // SAFETY: the block is unique and holds `len` initialised elements; the borrow of `self` is the only access.
        unsafe { std::slice::from_raw_parts_mut(self.data(), self.len()) }
    }

    pub fn push(&mut self, x: T) where T: Clone {
        self.reserve_unique(1);
        let len = self.len();
        // SAFETY: the block is unique with room for `len + 1` elements; slot `len` is uninitialised.
        unsafe {
            self.data().add(len).write(x);
            (*self.ptr.as_ptr()).len = len + 1;
        }
    }

    pub fn pop(&mut self) -> Option<T> where T: Clone {
        if self.is_empty() {
            return None;
        }
        self.reserve_unique(0);
        let len = self.len() - 1;
        // SAFETY: the block is unique; slot `len` is initialised and leaves the list before it is read out.
        unsafe {
            (*self.ptr.as_ptr()).len = len;
            Some(self.data().add(len).read())
        }
    }

    /// Keeps the first `n` elements and drops the rest.
    pub fn truncate(&mut self, n: usize) where T: Clone {
        if n >= self.len() {
            return;
        }
        if !self.is_unique() {
            *self = self.as_slice()[..n].iter().cloned().collect();
            return;
        }
        let len = self.len();
        // SAFETY: the block is unique; the tail `n..len` is initialised. `len` is set first, so a panicking element
        // drop cannot run a second drop of the same slot.
        unsafe {
            (*self.ptr.as_ptr()).len = n;
            std::ptr::drop_in_place(std::ptr::slice_from_raw_parts_mut(self.data().add(n), len - n));
        }
    }

    pub fn clear(&mut self) where T: Clone {
        self.truncate(0);
    }

    pub fn extend_from_slice(&mut self, xs: &[T]) where T: Clone {
        self.reserve_unique(xs.len());
        for x in xs {
            self.push(x.clone());
        }
    }

    /// The elements as a `Vec`: moved out when this handle is the only owner, cloned otherwise.
    pub fn into_vec(self) -> Vec<T> where T: Clone {
        if !self.is_unique() {
            return self.as_slice().to_vec();
        }
        let len = self.len();
        let mut out = Vec::with_capacity(len);
        // SAFETY: the block is unique and holds `len` initialised elements; they are moved into `out`, and the block's
        // `len` is set to 0 before it is freed, so they are not dropped twice.
        unsafe {
            std::ptr::copy_nonoverlapping(self.data(), out.as_mut_ptr(), len);
            out.set_len(len);
            (*self.ptr.as_ptr()).len = 0;
        }
        out
    }

    /// The last owner: drops the elements and frees the block.
    fn free_block(&mut self) {
        debug_assert!(!std::ptr::eq(self.ptr.as_ptr().cast_const(), &ALMIDE_LIST_EMPTY), "the static empty block is never freed");
        let (len, cap) = (self.len(), self.capacity());
        // SAFETY: the caller is the last owner of a heap block allocated with `cap`'s layout; its first `len`
        // elements are initialised and dropped exactly once here.
        unsafe {
            std::ptr::drop_in_place(std::ptr::slice_from_raw_parts_mut(self.data(), len));
            dealloc(self.ptr.as_ptr().cast(), almide_list_layout::<T>(cap));
        }
    }
}

impl<T> Drop for AlmideList<T> {
    fn drop(&mut self) {
        if self.is_static() {
            return;
        }
        if self.header().rc.fetch_sub(1, AtomicOrdering::Release) != 1 {
            return;
        }
        // Every other owner's use of the block happens before the free.
        fence(AtomicOrdering::Acquire);
        self.free_block();
    }
}

impl<T: Clone> Clone for AlmideList<T> {
    fn clone(&self) -> Self {
        // Inside a marking window, every element is cloned once so the blocks it reaches are marked too.
        if ALMIDE_STR_MARKING.load(AtomicOrdering::Relaxed) != 0 {
            for x in self.as_slice() {
                drop(x.clone());
            }
        }
        if !self.is_static() && self.header().rc.fetch_add(1, AtomicOrdering::Relaxed) > isize::MAX as usize {
            // An overflowing count would free a live block; abort as `Arc` does.
            std::process::abort();
        }
        AlmideList { ptr: self.ptr, _elems: PhantomData }
    }
}

impl<T> Default for AlmideList<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Deref for AlmideList<T> {
    type Target = [T];
    #[inline]
    fn deref(&self) -> &[T] {
        self.as_slice()
    }
}

impl<T> AsRef<[T]> for AlmideList<T> {
    fn as_ref(&self) -> &[T] {
        self.as_slice()
    }
}

impl<T> Borrow<[T]> for AlmideList<T> {
    fn borrow(&self) -> &[T] {
        self.as_slice()
    }
}

impl<T: Clone> From<Vec<T>> for AlmideList<T> {
    fn from(v: Vec<T>) -> Self {
        let mut out = AlmideList::with_capacity(v.len());
        for x in v {
            out.push(x);
        }
        out
    }
}

impl<T: Clone> From<&[T]> for AlmideList<T> {
    fn from(xs: &[T]) -> Self {
        let mut out = AlmideList::with_capacity(xs.len());
        out.extend_from_slice(xs);
        out
    }
}

impl<T: Clone> FromIterator<T> for AlmideList<T> {
    fn from_iter<I: IntoIterator<Item = T>>(it: I) -> Self {
        let it = it.into_iter();
        let mut out = AlmideList::with_capacity(it.size_hint().0);
        for x in it {
            out.push(x);
        }
        out
    }
}

impl<'a, T> IntoIterator for &'a AlmideList<T> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.as_slice().iter()
    }
}

impl<T: PartialEq> PartialEq for AlmideList<T> {
    fn eq(&self, o: &Self) -> bool {
        self.as_slice() == o.as_slice()
    }
}

impl<T: Eq> Eq for AlmideList<T> {}

impl<T: PartialOrd> PartialOrd for AlmideList<T> {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        self.as_slice().partial_cmp(o.as_slice())
    }
}

impl<T: Ord> Ord for AlmideList<T> {
    fn cmp(&self, o: &Self) -> Ordering {
        self.as_slice().cmp(o.as_slice())
    }
}

impl<T: Hash> Hash for AlmideList<T> {
    fn hash<H: Hasher>(&self, h: &mut H) {
        self.as_slice().hash(h)
    }
}

impl<T: fmt::Debug> fmt::Debug for AlmideList<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(self.as_slice()).finish()
    }
}

// SAFETY: every count change is atomic, and the elements are written only through `&mut self` after an `Acquire`
// count of one, as for `Arc<[T]>`; the elements themselves must be `Send + Sync` to be reached from two threads.
unsafe impl<T: Send + Sync> Send for AlmideList<T> {}
// SAFETY: as for `Send`: `&AlmideList<T>` only reads the elements or clones (an atomic increment).
unsafe impl<T: Send + Sync> Sync for AlmideList<T> {}
