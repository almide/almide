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

/// The block's layout for `cap` elements, or `None` when it would overflow.
fn almide_list_try_layout<T>(cap: usize) -> Option<Layout> {
    let elems = std::mem::size_of::<T>().checked_mul(cap)?;
    let size = almide_list_offset::<T>().checked_add(elems)?;
    Layout::from_size_align(size, almide_list_align::<T>()).ok()
}

/// A `TryReserveError` to return: std builds one only inside its own reserve, so ask it for a capacity that cannot fit.
#[cold]
fn almide_list_reserve_error() -> std::collections::TryReserveError {
    Vec::<u8>::new().try_reserve_exact(usize::MAX).expect_err("usize::MAX bytes never fit")
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

    /// Another owner of the same block (ADR-0037 D4): the `Vec::to_vec` spelling of an owned copy, O(1) here.
    /// The copy is made by the first write while the block is shared.
    #[inline]
    pub fn to_vec(&self) -> AlmideList<T> where T: Clone {
        self.clone()
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

    /// Room for `extra` more elements in a unique block (a shared block is copied first).
    pub fn reserve(&mut self, extra: usize) where T: Clone {
        self.reserve_unique(extra);
    }

    /// The elements for writing, as `Vec::as_mut_slice` (ADR-0037 D2: a shared block is copied first).
    pub fn as_mut_slice(&mut self) -> &mut [T] where T: Clone {
        self.make_mut()
    }

    /// Inserts `x` at `i`, shifting the rest right. Panics when `i > len`, as `Vec::insert`.
    pub fn insert(&mut self, i: usize, x: T) where T: Clone {
        let len = self.len();
        assert!(i <= len, "insertion index (is {i}) should be <= len (is {len})");
        self.reserve_unique(1);
        // SAFETY: the block is unique with room for `len + 1`; `i..len` moves one slot right (overlapping, hence
        // `copy`), then slot `i` is written. `len` is bumped after the write.
        unsafe {
            let p = self.data().add(i);
            std::ptr::copy(p, p.add(1), len - i);
            p.write(x);
            (*self.ptr.as_ptr()).len = len + 1;
        }
    }

    /// Removes and returns the element at `i`, shifting the rest left. Panics when `i >= len`, as `Vec::remove`.
    pub fn remove(&mut self, i: usize) -> T where T: Clone {
        let len = self.len();
        assert!(i < len, "removal index (is {i}) should be < len (is {len})");
        self.reserve_unique(0);
        // SAFETY: the block is unique; slot `i` is read out, `i + 1..len` moves one slot left, and `len` drops by one,
        // so every remaining element is initialised exactly once.
        unsafe {
            let p = self.data().add(i);
            let x = p.read();
            std::ptr::copy(p.add(1), p, len - i - 1);
            (*self.ptr.as_ptr()).len = len - 1;
            x
        }
    }

    /// The list's elements as a `Vec` for an edit `Vec` already has, then back: moved through when unique.
    fn edit_as_vec<R>(&mut self, f: impl FnOnce(&mut Vec<T>) -> R) -> R where T: Clone {
        let mut v = std::mem::take(self).into_vec();
        let r = f(&mut v);
        *self = AlmideList::from(v);
        r
    }

    /// Keeps the elements `f` accepts, in order, as `Vec::retain`.
    pub fn retain(&mut self, f: impl FnMut(&T) -> bool) where T: Clone {
        self.edit_as_vec(|v| v.retain(f));
    }

    /// Removes consecutive repeated elements, as `Vec::dedup`.
    pub fn dedup(&mut self) where T: Clone + PartialEq {
        self.edit_as_vec(|v| v.dedup());
    }

    /// Splits the list at `at`, keeping `..at` and returning `at..`, as `Vec::split_off`.
    pub fn split_off(&mut self, at: usize) -> AlmideList<T> where T: Clone {
        self.edit_as_vec(|v| AlmideList::from(v.split_off(at)))
    }

    /// Moves every element of `other` onto the end of this list, as `Vec::append`.
    pub fn append(&mut self, other: &mut AlmideList<T>) where T: Clone {
        let moved = std::mem::take(other);
        self.extend(moved);
    }

    /// Removes the elements in `range` and returns them, as `Vec::drain` (collected: the list is edited at once).
    pub fn drain(&mut self, range: impl std::ops::RangeBounds<usize>) -> std::vec::IntoIter<T> where T: Clone {
        self.edit_as_vec(|v| v.drain(range).collect::<Vec<T>>().into_iter())
    }

    /// Room for exactly `extra` more elements in a unique block, as `Vec::try_reserve_exact`: an allocation the system
    /// refuses is an `Err`, never an abort, so a caller can report it in its own words.
    pub fn try_reserve_exact(&mut self, extra: usize) -> Result<(), std::collections::TryReserveError> where T: Clone {
        let len = self.len();
        let Some(need) = len.checked_add(extra) else { return Err(almide_list_reserve_error()) };
        if self.is_unique() && need <= self.capacity() {
            return Ok(());
        }
        let Some(layout) = almide_list_try_layout::<T>(need) else { return Err(almide_list_reserve_error()) };
        if self.is_unique() {
            // SAFETY: the only owner of a heap block allocated with its capacity's layout; realloc keeps the alignment.
            let raw = unsafe { realloc(self.ptr.as_ptr().cast(), almide_list_layout::<T>(self.capacity()), layout.size()) };
            let Some(block) = NonNull::new(raw.cast::<AlmideListHeader>()) else { return Err(almide_list_reserve_error()) };
            self.ptr = block;
            // SAFETY: the block is unique and live.
            unsafe { (*self.ptr.as_ptr()).cap = need };
            return Ok(());
        }
        // SAFETY: the layout's size is at least the header's, so it is non-zero.
        let raw = unsafe { alloc(layout) };
        let Some(block) = NonNull::new(raw.cast::<AlmideListHeader>()) else { return Err(almide_list_reserve_error()) };
        // SAFETY: the block is freshly allocated, sized and aligned for the header.
        unsafe { block.as_ptr().write(AlmideListHeader { rc: AlmideListCount::new(1), len: 0, cap: need }) };
        let fresh = AlmideList::<T> { ptr: block, _elems: PhantomData };
        for (i, x) in self.as_slice().iter().enumerate() {
            // SAFETY: slot `i < len <= need` of the fresh block is uninitialised; `len` is bumped after each write.
            unsafe {
                fresh.data().add(i).write(x.clone());
                (*fresh.ptr.as_ptr()).len = i + 1;
            }
        }
        *self = fresh;
        Ok(())
    }

    /// As [`Self::try_reserve_exact`] (`Vec::try_reserve` may round up; this block does not need to).
    pub fn try_reserve(&mut self, extra: usize) -> Result<(), std::collections::TryReserveError> where T: Clone {
        self.try_reserve_exact(extra)
    }

    /// Nothing to give back: a block keeps its capacity (`Vec::shrink_to_fit` may too).
    pub fn shrink_to_fit(&mut self) {}

    /// Grows to `n` elements with clones of `x`, or truncates to `n`, as `Vec::resize`.
    pub fn resize(&mut self, n: usize, x: T) where T: Clone {
        self.resize_with(n, || x.clone());
    }

    /// Grows to `n` elements made by `f`, or truncates to `n`, as `Vec::resize_with`.
    pub fn resize_with(&mut self, n: usize, mut f: impl FnMut() -> T) where T: Clone {
        let len = self.len();
        if n <= len {
            self.truncate(n);
            return;
        }
        self.reserve_unique(n - len);
        for _ in len..n {
            self.push(f());
        }
    }

    /// Removes the element at `i`, moving the last one into its place, as `Vec::swap_remove`.
    pub fn swap_remove(&mut self, i: usize) -> T where T: Clone {
        let len = self.len();
        assert!(i < len, "swap_remove index (is {i}) should be < len (is {len})");
        self.make_mut().swap(i, len - 1);
        self.pop().expect("a non-empty list")
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

/// Writing through the slice copies a shared block first (ADR-0037 D2), so `xs[i] = v`, `xs.sort()` and every
/// other `&mut [T]` edit is copy-on-write.
impl<T: Clone> std::ops::DerefMut for AlmideList<T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut [T] {
        self.make_mut()
    }
}

/// The elements by value: moved out when this handle is the only owner, cloned otherwise.
impl<T: Clone> IntoIterator for AlmideList<T> {
    type Item = T;
    type IntoIter = AlmideListIntoIter<T>;
    /// No allocation: a unique block hands its elements out by move, a shared one by clone.
    fn into_iter(self) -> Self::IntoIter {
        let end = self.len();
        let unique = end > 0 && self.is_unique();
        if unique {
            // SAFETY: the block is unique; with its `len` at 0 it no longer drops the elements, which the
            // iterator now owns: it moves each one out once and drops those it never yields.
            unsafe { (*self.ptr.as_ptr()).len = 0 };
        }
        AlmideListIntoIter { list: self, next: 0, end, unique }
    }
}

impl<T: Clone> AlmideList<T> {
    /// `self.into_iter().map(f).collect()`. A unique block whose element layout `U` shares is rewritten in place,
    /// as std's `Vec` collects `into_iter().map(f)` into its source buffer: no second block.
    pub fn map_owned<U: Clone, F: FnMut(T) -> U>(self, mut f: F) -> AlmideList<U> {
        let len = self.len();
        let fits = std::mem::size_of::<T>() == std::mem::size_of::<U>()
            && std::mem::align_of::<T>() == std::mem::align_of::<U>()
            && std::mem::size_of::<T>() > 0;
        if len == 0 || !fits || !self.is_unique() {
            return self.into_iter().map(f).collect();
        }
        /// Unwinding out of `f` at element `done`: the `done` elements already written are `U`s, the one `f` was
        /// handed is gone, the rest are `T`s. Drop each as what it is, then free the block.
        struct Unwind<T, U> {
            ptr: NonNull<AlmideListHeader>,
            done: usize,
            len: usize,
            _elems: PhantomData<(T, U)>,
        }
        impl<T, U> Drop for Unwind<T, U> {
            fn drop(&mut self) {
                // SAFETY: the block is unique with its `len` at 0, so freeing it drops no element; the ranges below are
                // exactly the initialised `U`s and `T`s described above, each dropped once.
                unsafe {
                    let data = self.ptr.as_ptr().cast::<u8>().add(almide_list_offset::<U>());
                    std::ptr::drop_in_place(std::ptr::slice_from_raw_parts_mut(data.cast::<U>(), self.done));
                    let rest = self.len - self.done - 1;
                    std::ptr::drop_in_place(std::ptr::slice_from_raw_parts_mut(data.cast::<T>().add(self.done + 1), rest));
                    drop(AlmideList::<U> { ptr: self.ptr, _elems: PhantomData });
                }
            }
        }
        let me = std::mem::ManuallyDrop::new(self);
        let data = me.data();
        // SAFETY: the block is unique; with `len` at 0 nothing else reads or drops its elements while they change type.
        unsafe { (*me.ptr.as_ptr()).len = 0 };
        let mut guard = Unwind::<T, U> { ptr: me.ptr, done: 0, len, _elems: PhantomData };
        while guard.done < len {
            // SAFETY: slot `done` holds a `T` not yet read; `U` has `T`'s size and alignment, so the `U` written
            // back fits the slot exactly, and the offset of the elements is the same for both.
            unsafe {
                let x = std::ptr::read(data.add(guard.done));
                let y = f(x);
                std::ptr::write(data.cast::<U>().add(guard.done), y);
            }
            guard.done += 1;
        }
        let ptr = guard.ptr;
        std::mem::forget(guard);
        // SAFETY: every slot now holds a `U`; the block's capacity in elements is unchanged because the sizes agree.
        unsafe { (*ptr.as_ptr()).len = len };
        AlmideList { ptr, _elems: PhantomData }
    }
}

/// The by-value iterator of an `AlmideList`: owns the block for its lifetime.
pub struct AlmideListIntoIter<T: Clone> {
    list: AlmideList<T>,
    next: usize,
    end: usize,
    unique: bool,
}

impl<T: Clone> AlmideListIntoIter<T> {
    fn element(&self, i: usize) -> T {
        if self.unique {
            // SAFETY: `i` is in `next..end`, each index is taken once, and the block no longer drops them.
            unsafe { std::ptr::read(self.list.data().add(i)) }
        } else {
            self.list.as_slice()[i].clone()
        }
    }
}

impl<T: Clone> Iterator for AlmideListIntoIter<T> {
    type Item = T;
    fn next(&mut self) -> Option<T> {
        if self.next == self.end {
            return None;
        }
        let i = self.next;
        self.next += 1;
        Some(self.element(i))
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        let n = self.end - self.next;
        (n, Some(n))
    }
}

impl<T: Clone> DoubleEndedIterator for AlmideListIntoIter<T> {
    fn next_back(&mut self) -> Option<T> {
        if self.next == self.end {
            return None;
        }
        self.end -= 1;
        Some(self.element(self.end))
    }
}

impl<T: Clone> Clone for AlmideListIntoIter<T> {
    /// The elements not yet yielded, cloned into a block of their own (as `vec::IntoIter` clones its rest).
    fn clone(&self) -> Self {
        // SAFETY: `next..end` holds initialised elements in either mode: none of them has been moved out.
        let rest: &[T] = unsafe { std::slice::from_raw_parts(self.list.data().add(self.next), self.end - self.next) };
        AlmideList::from(rest).into_iter()
    }
}

impl<T: Clone> ExactSizeIterator for AlmideListIntoIter<T> {}
impl<T: Clone> std::iter::FusedIterator for AlmideListIntoIter<T> {}

impl<T: Clone> Drop for AlmideListIntoIter<T> {
    fn drop(&mut self) {
        if self.unique && self.next < self.end {
            // SAFETY: the elements in `next..end` were never moved out; the block does not drop them.
            unsafe {
                std::ptr::drop_in_place(std::ptr::slice_from_raw_parts_mut(self.list.data().add(self.next), self.end - self.next));
            }
        }
    }
}

impl<T: Clone> Extend<T> for AlmideList<T> {
    fn extend<I: IntoIterator<Item = T>>(&mut self, it: I) {
        let it = it.into_iter();
        self.reserve_unique(it.size_hint().0);
        for x in it {
            self.push(x);
        }
    }
}

impl<'a, T: Clone + 'a> Extend<&'a T> for AlmideList<T> {
    fn extend<I: IntoIterator<Item = &'a T>>(&mut self, it: I) {
        for x in it {
            self.push(x.clone());
        }
    }
}

impl<T: Clone> From<AlmideList<T>> for Vec<T> {
    fn from(l: AlmideList<T>) -> Vec<T> {
        l.into_vec()
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

impl<T> From<Vec<T>> for AlmideList<T> {
    fn from(mut v: Vec<T>) -> Self {
        let len = v.len();
        let out = AlmideList::<T>::with_capacity(len);
        if len > 0 {
            // SAFETY: the fresh block is unique with room for `len` elements; they are moved out of `v` bit for bit,
            // and `v`'s length is set to 0 first so it frees only its buffer.
            unsafe {
                v.set_len(0);
                std::ptr::copy_nonoverlapping(v.as_ptr(), out.data(), len);
                (*out.ptr.as_ptr()).len = len;
            }
        }
        out
    }
}

impl<T, const N: usize> From<[T; N]> for AlmideList<T> {
    /// A literal's elements moved into one block of exactly `N` (`almide_list![a, b, ..]`): one allocation, where
    /// going through `vec!` allocated the `Vec` buffer and then the block.
    fn from(a: [T; N]) -> Self {
        let out = AlmideList::<T>::with_capacity(N);
        if N > 0 {
            let a = std::mem::ManuallyDrop::new(a);
            // SAFETY: the fresh block is unique with room for `N` elements; they are moved out of `a` bit for bit,
            // and `a` is never dropped, so each element has exactly one owner.
            unsafe {
                std::ptr::copy_nonoverlapping(a.as_ptr(), out.data(), N);
                (*out.ptr.as_ptr()).len = N;
            }
        }
        out
    }
}

impl<T: Clone> From<&AlmideList<T>> for AlmideList<T> {
    /// Another owner of the block, O(1) (ADR-0037 D4): `Vec::from(&v)` is a copy in std's spelling.
    fn from(xs: &AlmideList<T>) -> Self {
        xs.clone()
    }
}

impl<T: Clone> From<&[T]> for AlmideList<T> {
    fn from(xs: &[T]) -> Self {
        let mut out = AlmideList::with_capacity(xs.len());
        out.extend_from_slice(xs);
        out
    }
}

// The full path: `FromIterator` is only in the 2021 prelude (see almide_str.rs).
impl<T: Clone> std::iter::FromIterator<T> for AlmideList<T> {
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

// A borrowed list (`*xs` of a `&[T]` param) compared with a list value, the
// way `[T] == Vec<T>` holds for std vectors. Only this direction: a second
// `PartialEq` on `AlmideList` would make `list == iter.collect()` ambiguous.
impl<T: PartialEq> PartialEq<AlmideList<T>> for [T] {
    fn eq(&self, o: &AlmideList<T>) -> bool {
        self == o.as_slice()
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

/// A runtime call's string list as the list block of `AlmideStr` (ADR-0037): the list itself when it already is one
/// (every runtime list of strings), element by element otherwise. A rendered call converted every result element by
/// element, a full pass and a fresh list for a list it already had.
pub trait AlmideIntoStrList {
    fn into_str_list(self) -> AlmideList<AlmideStr>;
}

impl AlmideIntoStrList for AlmideList<AlmideStr> {
    #[inline(always)]
    fn into_str_list(self) -> AlmideList<AlmideStr> {
        self
    }
}

impl AlmideIntoStrList for AlmideList<String> {
    fn into_str_list(self) -> AlmideList<AlmideStr> {
        self.as_slice().iter().map(|s| AlmideStr::from(s.as_str())).collect()
    }
}

impl AlmideIntoStrList for AlmideList<&str> {
    fn into_str_list(self) -> AlmideList<AlmideStr> {
        self.as_slice().iter().map(|s| AlmideStr::from(*s)).collect()
    }
}

impl AlmideIntoStrList for Vec<AlmideStr> {
    #[inline(always)]
    fn into_str_list(self) -> AlmideList<AlmideStr> {
        AlmideList::from(self)
    }
}

impl AlmideIntoStrList for Vec<String> {
    fn into_str_list(self) -> AlmideList<AlmideStr> {
        self.into_iter().map(AlmideStr::from).collect()
    }
}

impl AlmideIntoStrList for Vec<&str> {
    fn into_str_list(self) -> AlmideList<AlmideStr> {
        self.into_iter().map(AlmideStr::from).collect()
    }
}

/// [`AlmideIntoStrList::into_str_list`] as a function, so a call site needs no trait in scope.
#[inline(always)]
pub fn almide_str_list<L: AlmideIntoStrList>(l: L) -> AlmideList<AlmideStr> {
    l.into_str_list()
}

/// The owned list of a borrowed slice, `to_vec`'s job in std's spelling (ADR-0037): a copy of the elements.
/// An `AlmideList` receiver answers with its own inherent `to_list`, another owner of the same block, O(1).
pub trait AlmideToList<T> {
    fn to_list(&self) -> AlmideList<T>;
}

impl<T: Clone> AlmideToList<T> for [T] {
    fn to_list(&self) -> AlmideList<T> {
        AlmideList::from(self)
    }
}

impl<T: Clone> AlmideList<T> {
    /// Another owner of the same block, O(1): what a slice's `to_list` copies, a list shares.
    #[inline]
    pub fn to_list(&self) -> AlmideList<T> {
        self.clone()
    }
}

/// A fresh heap block of `n > 0` elements whose bytes are all zero (`alloc_zeroed`, so the
/// allocator may hand back untouched pages): only the header is written.
fn almide_list_alloc_zeroed<T>(n: usize) -> NonNull<AlmideListHeader> {
    let layout = almide_list_layout::<T>(n);
    // SAFETY: the layout's size is at least the header's, so it is non-zero.
    let raw = unsafe { std::alloc::alloc_zeroed(layout) }.cast::<AlmideListHeader>();
    let block = NonNull::new(raw).unwrap_or_else(|| handle_alloc_error(layout));
    // SAFETY: the block is freshly allocated, sized and aligned for the header; the
    // caller guarantees all-zero bytes are `n` valid elements.
    unsafe { block.as_ptr().write(AlmideListHeader { rc: AlmideListCount::new(1), len: n, cap: n }) };
    block
}

impl<T> AlmideList<T> {
    /// `n` clones of `x` (`vec![x; n]`): one block at its final capacity, filled in place;
    /// the last element is `x` itself.
    pub fn from_elem(x: T, n: usize) -> Self where T: Clone {
        if n == 0 {
            return Self::new();
        }
        let out = Self::with_capacity(n);
        let data = out.data();
        for i in 0..n - 1 {
            // SAFETY: the block is unique with room for `n` elements; `len` follows each
            // write, so a panicking clone drops exactly the elements written.
            unsafe {
                data.add(i).write(x.clone());
                (*out.ptr.as_ptr()).len = i + 1;
            }
        }
        // SAFETY: as above, the last slot.
        unsafe {
            data.add(n - 1).write(x);
            (*out.ptr.as_ptr()).len = n;
        }
        out
    }
}

/// An element type whose value is its bytes and whose all-zero bytes are a value: a
/// `Copy` type with no padding and no invalid bit pattern at zero. `almide_list![x; n]`
/// with an all-zero `x` of such a type allocates its block zeroed.
///
/// # Safety
/// Every byte of a value must be initialized (no padding), and the all-zero bit
/// pattern must be a valid value. A clone of a value is then its bytes.
pub unsafe trait AlmideZeroValid: Copy {}
// SAFETY: the primitive scalars have no padding, and zero is a valid value of each
// (`0`, `0.0`, `false`, `'\0'`).
unsafe impl AlmideZeroValid for i8 {}
unsafe impl AlmideZeroValid for i16 {}
unsafe impl AlmideZeroValid for i32 {}
unsafe impl AlmideZeroValid for i64 {}
unsafe impl AlmideZeroValid for i128 {}
unsafe impl AlmideZeroValid for isize {}
unsafe impl AlmideZeroValid for u8 {}
unsafe impl AlmideZeroValid for u16 {}
unsafe impl AlmideZeroValid for u32 {}
unsafe impl AlmideZeroValid for u64 {}
unsafe impl AlmideZeroValid for u128 {}
unsafe impl AlmideZeroValid for usize {}
unsafe impl AlmideZeroValid for f32 {}
unsafe impl AlmideZeroValid for f64 {}
unsafe impl AlmideZeroValid for bool {}
unsafe impl AlmideZeroValid for char {}

/// `almide_list![x; n]`'s element. The macro calls `(&&AlmideListElem::new(x)).almide_list_fill(n)`:
/// method lookup tries the `&&` receiver first, which only an [`AlmideZeroValid`] element
/// has, and otherwise derefs to the general fill. The cell lets either fill move `x` out
/// through the shared receiver, so the list holds `x` and `n - 1` clones, as `vec!` does.
pub struct AlmideListElem<T>(std::cell::Cell<Option<T>>);

impl<T> AlmideListElem<T> {
    #[inline]
    pub fn new(x: T) -> Self {
        AlmideListElem(std::cell::Cell::new(Some(x)))
    }

    #[inline]
    fn take(&self) -> T {
        self.0.take().expect("an almide_list! element is filled once")
    }
}

/// The fill an element type that is valid when zeroed takes: a zero value allocates the block zeroed.
pub trait AlmideListFillZeroed<T> {
    fn almide_list_fill(self, n: usize) -> AlmideList<T>;
}

impl<T: AlmideZeroValid> AlmideListFillZeroed<T> for &&AlmideListElem<T> {
    #[inline]
    fn almide_list_fill(self, n: usize) -> AlmideList<T> {
        let x = self.take();
        // SAFETY: `AlmideZeroValid` promises every byte of `x` is initialized.
        let bytes = unsafe { std::slice::from_raw_parts((&x as *const T).cast::<u8>(), std::mem::size_of::<T>()) };
        if n == 0 || bytes.iter().any(|&b| b != 0) {
            return AlmideList::from_elem(x, n);
        }
        // The zeroed block holds `n` copies of `x`: its bytes are `x`'s, and a copy of `x` is its bytes.
        AlmideList { ptr: almide_list_alloc_zeroed::<T>(n), _elems: PhantomData }
    }
}

/// The general fill: `n` clones written into one block.
pub trait AlmideListFill<T> {
    fn almide_list_fill(self, n: usize) -> AlmideList<T>;
}

impl<T: Clone> AlmideListFill<T> for &AlmideListElem<T> {
    #[inline]
    fn almide_list_fill(self, n: usize) -> AlmideList<T> {
        AlmideList::from_elem(self.take(), n)
    }
}
