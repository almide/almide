/// The indirection of every field that closes a cycle of a recursive type
/// (#4251, #4252, #4250). It owns its value like `Box`, and its drop, clone
/// and `==` never take one native stack frame per nesting level: while an
/// outer one runs, an inner one queues its work on a per-thread list, and the
/// outer one drains the list in a loop. So a 200 000-long chain drops,
/// clones and compares in a constant number of frames, whichever container
/// (an `Option`, a `List`, a record, a variant payload) closes its cycle.
pub struct AlmideBox<T> {
    almide_p: std::ptr::NonNull<T>,
    almide_m: std::marker::PhantomData<T>,
}

// SAFETY: an AlmideBox uniquely owns its heap cell, as Box<T> does (new leaks a
// fresh Box, into_inner and Drop free it once, Clone makes a new cell), so it
// may cross or be shared between threads exactly when T may.
unsafe impl<T: Send> Send for AlmideBox<T> {}
unsafe impl<T: Sync> Sync for AlmideBox<T> {}

impl<T> AlmideBox<T> {
    #[inline]
    pub fn new(x: T) -> Self {
        AlmideBox { almide_p: std::ptr::NonNull::from(std::boxed::Box::leak(std::boxed::Box::new(x))), almide_m: std::marker::PhantomData }
    }
    /// The value, moved out — `*b` on a `Box`.
    #[inline]
    pub fn into_inner(self) -> T {
        let me = std::mem::ManuallyDrop::new(self);
        // SAFETY: almide_p came from Box::leak in new / clone; ManuallyDrop keeps
        // Drop from freeing it a second time.
        unsafe { *std::boxed::Box::from_raw(me.almide_p.as_ptr()) }
    }
}

/// `*b` on an owned recursive field, spelled as a call (`AlmideBox` cannot
/// move out through `Deref`). A region twin's `AlmideRgn` handle (#1991)
/// copies its value out the same way.
pub trait AlmideBoxTake {
    type Out;
    fn almide_box_take_out(self) -> Self::Out;
}

impl<T> AlmideBoxTake for AlmideBox<T> {
    type Out = T;
    #[inline]
    fn almide_box_take_out(self) -> T {
        self.into_inner()
    }
}

impl<T: Copy> AlmideBoxTake for AlmideRgn<T> {
    type Out = T;
    #[inline]
    fn almide_box_take_out(self) -> T {
        *self
    }
}

#[inline]
pub fn almide_box_take<B: AlmideBoxTake>(b: B) -> B::Out {
    b.almide_box_take_out()
}

impl<T> std::ops::Deref for AlmideBox<T> {
    type Target = T;
    #[inline]
    fn deref(&self) -> &T {
        // SAFETY: almide_p is a live, initialised cell owned by self (new writes it,
        // clone's step writes it before clone returns); the borrow is tied to &self.
        unsafe { self.almide_p.as_ref() }
    }
}

impl<T> std::ops::DerefMut for AlmideBox<T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut T {
        // SAFETY: as deref, and &mut self is the only path to the cell.
        unsafe { self.almide_p.as_mut() }
    }
}

/// One queued step of a deep walk: the operand addresses and the
/// monomorphic function that finishes it.
// SAFETY: (the three step types): each pairs an address with the one
// monomorphic fn that reads it as its own T; the steps are built only below,
// from live cells, and each is run exactly once by the outermost walk.
type AlmideBoxDropStep = (*mut u8, unsafe fn(*mut u8));
type AlmideBoxCloneStep = (*const u8, *mut u8, unsafe fn(*const u8, *mut u8));
type AlmideBoxEqStep = (*const u8, *const u8, unsafe fn(*const u8, *const u8) -> bool);

std::thread_local! {
    static ALMIDE_BOX_DROP_Q: std::cell::Cell<*mut Vec<AlmideBoxDropStep>> = const { std::cell::Cell::new(std::ptr::null_mut()) };
    static ALMIDE_BOX_CLONE_Q: std::cell::Cell<*mut Vec<AlmideBoxCloneStep>> = const { std::cell::Cell::new(std::ptr::null_mut()) };
    static ALMIDE_BOX_EQ_Q: std::cell::Cell<*mut Vec<AlmideBoxEqStep>> = const { std::cell::Cell::new(std::ptr::null_mut()) };
}

/// Restores a queue slot to "no walk running" however the walk ends.
struct AlmideBoxQueueReset<Q: 'static>(&'static std::thread::LocalKey<std::cell::Cell<*mut Vec<Q>>>);

impl<Q: 'static> Drop for AlmideBoxQueueReset<Q> {
    fn drop(&mut self) {
        self.0.with(|q| q.set(std::ptr::null_mut()));
    }
}

/// While a walk runs, its queue is reached ONLY through the slot's pointer —
/// a nested call's push and the outer walk's pop alike (#4303). The outer walk
/// using its `work` by name between steps retags it, which ends the pointer the
/// slot holds under Stacked Borrows, so the next nested push was undefined
/// behaviour Miri reported on every deep recursive-type program.
/// `Some(step)` back means no walk is running: the caller starts one.
#[inline(always)]
fn almide_box_q_push<Q: 'static>(slot: &'static std::thread::LocalKey<std::cell::Cell<*mut Vec<Q>>>, step: Q) -> Option<Q> {
    let w = slot.with(|q| q.get());
    // SAFETY: a non-null slot is the &mut work of the outer walk on this thread, live until its reset nulls the slot; that walk is suspended in the call that led here and reaches work only through this same pointer.
    match unsafe { w.as_mut() } {
        Some(work) => { work.push(step); None }
        None => Some(step),
    }
}

/// The outer walk's next step, popped through the slot's pointer.
#[inline(always)]
fn almide_box_q_pop<Q: 'static>(slot: &'static std::thread::LocalKey<std::cell::Cell<*mut Vec<Q>>>) -> Option<Q> {
    let w = slot.with(|q| q.get());
    // SAFETY: as in almide_box_q_push; called by the outer walk between steps, when no nested call holds the queue.
    unsafe { w.as_mut() }.and_then(|work| work.pop())
}

// SAFETY: callers pass an AlmideBox<T>'s cell (a leaked Box<T>), once.
unsafe fn almide_box_free<T>(p: *mut u8) {
    drop(std::boxed::Box::from_raw(p as *mut T));
}

impl<T> Drop for AlmideBox<T> {
    fn drop(&mut self) {
        let step: AlmideBoxDropStep = (self.almide_p.as_ptr() as *mut u8, almide_box_free::<T>);
        let Some(step) = almide_box_q_push(&ALMIDE_BOX_DROP_Q, step) else { return };
        let mut work: Vec<AlmideBoxDropStep> = Vec::new();
        ALMIDE_BOX_DROP_Q.with(|q| q.set(&mut work));
        let _reset = AlmideBoxQueueReset(&ALMIDE_BOX_DROP_Q);
        // SAFETY: step is this box's own cell with almide_box_free::<T>; Drop runs
        // once per box, and the queued steps are popped (taken) once each.
        unsafe { (step.1)(step.0) };
        while let Some((p, f)) = almide_box_q_pop(&ALMIDE_BOX_DROP_Q) {
            // SAFETY: a step a nested drop queued: its own cell and free fn, popped once.
            unsafe { f(p) };
        }
    }
}

// SAFETY: callers pass a live T as src and a fresh, uninitialised cell
// sized and aligned for T as dst, which this initialises once.
unsafe fn almide_box_clone_into<T: Clone>(src: *const u8, dst: *mut u8) {
    std::ptr::write(dst as *mut T, (*(src as *const T)).clone());
}

impl<T: Clone> Clone for AlmideBox<T> {
    fn clone(&self) -> Self {
        // The copy's cell is written by the step, before anything reads it:
        // the outermost clone drains every queued step before it returns.
        let cell = std::ptr::NonNull::from(std::boxed::Box::leak(std::boxed::Box::new(std::mem::MaybeUninit::<T>::uninit()))).cast::<T>();
        let step: AlmideBoxCloneStep = (self.almide_p.as_ptr() as *const u8, cell.as_ptr() as *mut u8, almide_box_clone_into::<T>);
        if let Some(step) = almide_box_q_push(&ALMIDE_BOX_CLONE_Q, step) {
            let mut work: Vec<AlmideBoxCloneStep> = Vec::new();
            ALMIDE_BOX_CLONE_Q.with(|q| q.set(&mut work));
            let _reset = AlmideBoxQueueReset(&ALMIDE_BOX_CLONE_Q);
            // SAFETY: step.0 is self's live cell, step.1 the fresh cell above, written
            // once here; each queued step is popped and run once before return.
            unsafe { (step.2)(step.0, step.1) };
            while let Some((s, d, f)) = almide_box_q_pop(&ALMIDE_BOX_CLONE_Q) {
                // SAFETY: a step a nested clone queued: its live source and fresh cell, popped once.
                unsafe { f(s, d) };
            }
        }
        AlmideBox { almide_p: cell, almide_m: std::marker::PhantomData }
    }
}

// SAFETY: callers pass two live T cells, borrowed for the call.
unsafe fn almide_box_eq_at<T: PartialEq>(a: *const u8, b: *const u8) -> bool {
    *(a as *const T) == *(b as *const T)
}

impl<T: PartialEq> PartialEq for AlmideBox<T> {
    /// Structural `==` is the conjunction of its leaves, so a nested
    /// comparison may answer `true` for now and run later: the outermost
    /// one folds every queued step into its verdict.
    fn eq(&self, other: &Self) -> bool {
        let step: AlmideBoxEqStep = (self.almide_p.as_ptr() as *const u8, other.almide_p.as_ptr() as *const u8, almide_box_eq_at::<T>);
        let Some(step) = almide_box_q_push(&ALMIDE_BOX_EQ_Q, step) else { return true };
        let mut work: Vec<AlmideBoxEqStep> = Vec::new();
        ALMIDE_BOX_EQ_Q.with(|q| q.set(&mut work));
        let _reset = AlmideBoxQueueReset(&ALMIDE_BOX_EQ_Q);
        // SAFETY: both cells belong to self / other, borrowed for this call, which
        // outlives every queued step (drained below before return).
        if !unsafe { (step.2)(step.0, step.1) } {
            return false;
        }
        while let Some((a, b, f)) = almide_box_q_pop(&ALMIDE_BOX_EQ_Q) {
            // SAFETY: a step a nested == queued: two cells inside self / other, still borrowed.
            if !unsafe { f(a, b) } {
                return false;
            }
        }
        true
    }
}

impl<T: Eq> Eq for AlmideBox<T> {}

impl<T: PartialOrd> PartialOrd for AlmideBox<T> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        (**self).partial_cmp(&**other)
    }
}

impl<T: Ord> Ord for AlmideBox<T> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (**self).cmp(&**other)
    }
}

impl<T: std::hash::Hash> std::hash::Hash for AlmideBox<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        (**self).hash(state)
    }
}

impl<T: std::fmt::Debug> std::fmt::Debug for AlmideBox<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        (**self).fmt(f)
    }
}

impl<T: AlmideRepr> AlmideRepr for AlmideBox<T> {
    fn almide_repr(&self) -> String {
        (**self).almide_repr()
    }
}

/// One piece of a recursive type's literal form (#4250): a fixed or built
/// text, or a value of a recursive type whose own pieces are produced when
/// the walk reaches it. `almide_repr_walk` writes them in order with an
/// explicit stack, so the form of a 200 000-deep value takes no native stack
/// per level.
pub enum AlmideReprPart {
    Lit(&'static str),
    Text(String),
    // SAFETY: built only by almide_repr_node from a &T alive for the walk.
    Node(*const u8, unsafe fn(*const u8, &mut Vec<AlmideReprPart>)),
}

/// The pieces of one level of a recursive type's literal form.
pub trait AlmideReprNode {
    fn almide_repr_parts(&self, out: &mut Vec<AlmideReprPart>);
}

// SAFETY: callers pass the address almide_repr_node took from a live &T.
unsafe fn almide_repr_node_at<T: AlmideReprNode>(p: *const u8, out: &mut Vec<AlmideReprPart>) {
    (*(p as *const T)).almide_repr_parts(out)
}

/// A nested value of a recursive type, produced when the walk reaches it.
#[inline]
pub fn almide_repr_node<T: AlmideReprNode>(x: &T) -> AlmideReprPart {
    AlmideReprPart::Node(x as *const T as *const u8, almide_repr_node_at::<T>)
}

/// The literal form of a recursive value, written piece by piece in order.
pub fn almide_repr_walk<T: AlmideReprNode>(x: &T) -> String {
    let mut out = String::new();
    let mut stack: Vec<AlmideReprPart> = Vec::new();
    let mut level: Vec<AlmideReprPart> = Vec::new();
    x.almide_repr_parts(&mut level);
    stack.extend(level.drain(..).rev());
    while let Some(part) = stack.pop() {
        match part {
            AlmideReprPart::Lit(s) => out.push_str(s),
            AlmideReprPart::Text(s) => out.push_str(&s),
            AlmideReprPart::Node(p, f) => {
                // SAFETY: p, f came from almide_repr_node on a part of x, which is borrowed
                // for the whole walk, so p is still a live T.
                unsafe { f(p, &mut level) };
                stack.extend(level.drain(..).rev());
            }
        }
    }
    out
}
