//! AlmideList (ADR-0037) against `Vec`: sharing by count, copy-on-write, the
//! static empty block and its immortal count (Q2), element drops, alignment,
//! zero-sized elements and threads. CI runs this file under Miri with the rest
//! of almide-str, which checks every raw access: no use-after-free, double
//! free, leak, out-of-bounds or aliasing violation.
use almide_str::{almide_str_mark_reachable, AlmideList, AlmideStr};
use std::cell::Cell;
use std::rc::Rc;
use std::sync::Mutex;

/// The marking window is a process-wide switch: while one test marks, every
/// clone in the binary clones its elements once more. A test that counts
/// element clones holds this lock, as every test that marks does, so they
/// never overlap.
static MARKING_WINDOW: Mutex<()> = Mutex::new(());

#[test]
fn a_clone_shares_the_block_and_a_write_copies_it() {
    let mut a: AlmideList<i64> = vec![1, 2, 3].into();
    assert!(a.is_unique());
    let b = a.clone();
    assert_eq!(a.ref_count(), 2);
    assert!(!a.is_unique());
    a.make_mut()[0] = 10;
    assert_eq!(&*a, &[10, 2, 3]);
    assert_eq!(&*b, &[1, 2, 3], "the other owner never sees the write");
    assert!(a.is_unique() && b.is_unique());
    a.push(4);
    assert_eq!(a.pop(), Some(4));
    assert_eq!(a.len(), 3);
}

#[test]
fn a_unique_write_happens_in_place() {
    let mut a: AlmideList<i64> = AlmideList::with_capacity(8);
    for i in 0..8 {
        a.push(i);
    }
    let before = a.as_ptr();
    a.make_mut()[3] = 99;
    a.truncate(5);
    assert_eq!(a.as_ptr(), before, "a unique block is edited where it is");
    assert_eq!(&*a, &[0, 1, 2, 99, 4]);
    for i in 0..100 {
        a.push(i);
    }
    assert_eq!(a.len(), 105);
    assert_eq!(a[104], 99);
}

#[test]
fn the_static_empty_block_is_immortal_and_never_unique() {
    let mut e: AlmideList<i64> = AlmideList::new();
    assert_eq!(e.ref_count(), 0, "the static block is not counted");
    assert!(!e.is_unique(), "the static block is never reported unique");
    let copies: Vec<AlmideList<i64>> = (0..4).map(|_| e.clone()).collect();
    assert!(copies.iter().all(|c| c.ref_count() == 0 && c.is_empty()));
    drop(copies);
    assert_eq!(e.ref_count(), 0, "clones and drops never touch its count");
    assert!(e.make_mut().is_empty());
    e.push(7);
    assert_eq!(e.ref_count(), 1, "the first write allocates a block of its own");
    assert_eq!(&*e, &[7]);
    let fresh: AlmideList<i64> = AlmideList::new();
    assert!(fresh.is_empty() && fresh.ref_count() == 0);
    let mut d: AlmideList<i64> = AlmideList::default();
    assert_eq!(d.pop(), None);
    d.clear();
    assert!(d.into_vec().is_empty());
}

#[test]
fn one_empty_block_serves_every_element_type_aligned_to_16() {
    let a: AlmideList<u8> = AlmideList::new();
    let b: AlmideList<(i64, f64)> = AlmideList::new();
    let c: AlmideList<AlmideStr> = AlmideList::new();
    #[repr(align(16))]
    #[derive(Clone)]
    struct A16(#[allow(dead_code)] u8);
    let d: AlmideList<A16> = AlmideList::new();
    for (n, empty) in [(a.ref_count(), a.is_empty()), (b.ref_count(), b.is_empty()), (c.ref_count(), c.is_empty()), (d.ref_count(), d.is_empty())] {
        assert_eq!((n, empty), (0, true));
    }
    assert_eq!(d.as_ptr() as usize % 16, 0, "an empty slice of a 16-aligned type is aligned");
}

#[test]
fn an_element_aligned_above_16_allocates_and_starts_aligned() {
    #[repr(align(64))]
    #[derive(Clone, Debug, PartialEq)]
    struct A64(u64);
    let empty: AlmideList<A64> = AlmideList::new();
    assert_eq!(empty.ref_count(), 1, "an over-aligned type allocates its empty list");
    assert_eq!(empty.as_ptr() as usize % 64, 0);
    let mut xs = empty.clone();
    for i in 0..10 {
        xs.push(A64(i));
        assert_eq!(xs.as_ptr() as usize % 64, 0, "the elements start aligned after every growth");
    }
    assert_eq!(xs[9], A64(9));
    assert!(empty.is_empty());
}

#[test]
fn every_element_is_dropped_exactly_once() {
    // Each live `Counted` adds one to `live`, so a leak leaves it above zero and a double drop below.
    struct Counted(Rc<Cell<i64>>);
    impl Counted {
        fn new(live: &Rc<Cell<i64>>) -> Self {
            live.set(live.get() + 1);
            Counted(live.clone())
        }
    }
    impl Clone for Counted {
        fn clone(&self) -> Self {
            Counted::new(&self.0)
        }
    }
    impl Drop for Counted {
        fn drop(&mut self) {
            self.0.set(self.0.get() - 1);
        }
    }
    let live = Rc::new(Cell::new(0));
    {
        let mut a = AlmideList::new();
        for _ in 0..5 {
            a.push(Counted::new(&live));
        }
        let b = a.clone();
        assert_eq!(live.get(), 5, "a clone shares the block and copies no element");
        let mut c = b.clone();
        assert_eq!(c.make_mut().len(), 5);
        assert_eq!(live.get(), 10, "a write to a shared block copies its elements once");
        c.truncate(2);
        assert_eq!(live.get(), 7);
        drop(c.pop());
        assert_eq!(live.get(), 6);
        let v = b.into_vec();
        assert_eq!(live.get(), 11, "into_vec of a shared block clones; the block itself stays with `a`");
        drop(v);
        assert_eq!(live.get(), 6);
        let w = a.into_vec();
        assert_eq!(live.get(), 6, "into_vec of a unique block moves the elements out");
        drop(w);
        assert_eq!(live.get(), 1, "only `c`'s element is left");
        drop(c);
    }
    assert_eq!(live.get(), 0, "no element leaked or dropped twice");
}

#[test]
fn zero_sized_elements_are_counted_not_stored() {
    let mut xs: AlmideList<()> = AlmideList::new();
    for _ in 0..200 {
        xs.push(());
    }
    let ys = xs.clone();
    assert_eq!(xs.pop(), Some(()));
    assert_eq!((xs.len(), ys.len()), (199, 200));
}

#[test]
fn it_agrees_with_vec_on_a_run_of_edits() {
    let mut model: Vec<i64> = Vec::new();
    let mut list: AlmideList<i64> = AlmideList::new();
    let mut held: Vec<(AlmideList<i64>, Vec<i64>)> = Vec::new();
    let mut seed: u64 = 0x9E37_79B9_7F4A_7C15;
    for _ in 0..400 {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        match seed % 6 {
            0 | 1 => {
                model.push(seed as i64 % 100);
                list.push(seed as i64 % 100);
            }
            2 => assert_eq!(list.pop(), model.pop()),
            3 if !model.is_empty() => {
                let i = (seed as usize / 7) % model.len();
                model[i] = -1;
                list.make_mut()[i] = -1;
            }
            4 => held.push((list.clone(), model.clone())),
            _ => {
                let n = (seed as usize / 11) % (model.len() + 1);
                model.truncate(n);
                list.truncate(n);
            }
        }
        assert_eq!(&*list, model.as_slice());
    }
    for (l, m) in &held {
        assert_eq!(&**l, m.as_slice(), "an earlier owner's view never changes");
    }
}

#[test]
fn marking_a_list_marks_the_strings_it_holds() {
    let _window = MARKING_WINDOW.lock().unwrap_or_else(|e| e.into_inner());
    let s = AlmideStr::from("shared text");
    let xs: AlmideList<AlmideStr> = vec![s.clone()].into();
    assert!(!s.is_thread_shared());
    almide_str_mark_reachable(&xs);
    assert!(s.is_thread_shared(), "the marking clone reaches the element's block");
}

#[test]
fn owners_on_several_threads_drop_the_block_once() {
    let _window = MARKING_WINDOW.lock().unwrap_or_else(|e| e.into_inner());
    let xs: AlmideList<AlmideStr> = (0..64).map(|i| AlmideStr::from(format!("item {i}").as_str())).collect();
    almide_str_mark_reachable(&xs);
    std::thread::scope(|scope| {
        for t in 0..4 {
            let mine = xs.clone();
            scope.spawn(move || {
                let mut mine = mine;
                if t % 2 == 0 {
                    mine.make_mut()[0] = AlmideStr::from("written");
                }
                assert_eq!(mine.len(), 64);
            });
        }
    });
    assert_eq!(xs[0].as_str(), "item 0");
    assert!(xs.is_unique());
}

/// Stage 2a (ADR-0037): the `Vec` surface the runtime and the renderers use —
/// insert, remove, retain, dedup, split_off, append, drain, extend, owned
/// iteration and slice writes — agrees with `Vec` on a run of edits, and every
/// write to a shared block leaves the other owners' view unchanged.
#[test]
fn the_vec_surface_agrees_with_vec_and_copies_on_write() {
    use almide_str::almide_list;
    let mut model: Vec<i64> = Vec::new();
    let mut list: AlmideList<i64> = almide_list![];
    let mut held: Vec<(AlmideList<i64>, Vec<i64>)> = Vec::new();
    let mut seed: u64 = 0x2545_F491_4F6C_DD1D;
    for _ in 0..600 {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        let v = seed as i64 % 50;
        let at = |n: usize| (seed as usize / 13) % (n + 1);
        match seed % 11 {
            0 | 1 => {
                let i = at(model.len());
                model.insert(i, v);
                list.insert(i, v);
            }
            2 if !model.is_empty() => {
                let i = at(model.len() - 1);
                assert_eq!(list.remove(i), model.remove(i));
            }
            3 => {
                model.retain(|x| x % 7 != 0);
                list.retain(|x| x % 7 != 0);
            }
            4 => {
                model.dedup();
                list.dedup();
            }
            5 => {
                let i = at(model.len());
                let (mt, lt) = (model.split_off(i), list.split_off(i));
                assert_eq!(&*lt, mt.as_slice());
                let mut back = lt;
                list.append(&mut back);
                model.extend(mt);
                assert!(back.is_empty());
            }
            6 => {
                let i = at(model.len());
                let d: Vec<i64> = list.drain(i..).collect();
                assert_eq!(d, model.drain(i..).collect::<Vec<_>>());
            }
            7 => {
                list.extend([v, v + 1]);
                model.extend([v, v + 1]);
                list.extend(&[v]);
                model.push(v);
            }
            8 if !model.is_empty() => {
                let i = at(model.len() - 1);
                list[i] = -v;
                model[i] = -v;
                list.sort();
                model.sort();
            }
            9 => held.push((list.clone(), model.clone())),
            _ => {
                let owned: Vec<i64> = list.clone().into_iter().collect();
                assert_eq!(owned, model);
            }
        }
        assert_eq!(&*list, model.as_slice());
    }
    for (l, m) in &held {
        assert_eq!(&**l, m.as_slice(), "an earlier owner's view never changes");
    }
    assert_eq!(&*almide_list![1, 2, 3], &[1, 2, 3]);
    assert_eq!(&*almide_list![7i64; 4], &[7, 7, 7, 7]);
}

/// Owned iteration moves the elements out of a unique block and clones them out
/// of a shared one; slice writes through a shared block copy it once.
#[test]
fn owned_iteration_and_slice_writes_count_their_copies() {
    let _window = MARKING_WINDOW.lock().unwrap_or_else(|e| e.into_inner());
    struct Counted(Rc<Cell<i64>>);
    impl Clone for Counted {
        fn clone(&self) -> Self {
            self.0.set(self.0.get() + 1);
            Counted(self.0.clone())
        }
    }
    let clones = Rc::new(Cell::new(0));
    let mut a: AlmideList<Counted> = AlmideList::new();
    for _ in 0..4 {
        a.push(Counted(clones.clone()));
    }
    let b = a.clone();
    assert_eq!(b.into_iter().count(), 4);
    assert_eq!(clones.get(), 4, "iterating a shared block by value clones each element");
    assert_eq!(a.clone().into_iter().count(), 4);
    assert_eq!(clones.get(), 8);
    let mut c = a.clone();
    c.swap(0, 3);
    assert_eq!(clones.get(), 12, "a slice write through a shared block copies it once");
    c.swap(1, 2);
    assert_eq!(clones.get(), 12, "the copy is unique: the next write is in place");
    assert_eq!(a.into_iter().count(), 4);
    assert_eq!(clones.get(), 12, "iterating the only owner by value moves the elements");
}

/// An FFI-style foreign write (ADR-0037 Q1): the runtime hands a callee a raw
/// pointer and length taken from a writable view. The view makes a shared or
/// static block unique first, so the foreign write lands in a block no other
/// owner reads, and the borrow keeps the list from being cloned while lent.
unsafe extern "C" fn foreign_fill(p: *mut u8, n: usize, byte: u8) {
    for i in 0..n {
        // SAFETY: the caller lends `n` writable bytes at `p`.
        unsafe { p.add(i).write(byte) };
    }
}

#[test]
fn a_foreign_write_through_a_pointer_never_reaches_another_owner() {
    let mut a: AlmideList<u8> = vec![1, 2, 3, 4].into();
    let b = a.clone();
    let shared_at = b.as_ptr();
    let view = a.as_mut_slice();
    let (p, n) = (view.as_mut_ptr(), view.len());
    assert_ne!(p.cast_const(), shared_at, "a shared block is copied before it is lent for writing");
    // SAFETY: `p` and `n` come from the unique writable view above.
    unsafe { foreign_fill(p, n, 9) };
    assert_eq!(&*a, &[9, 9, 9, 9]);
    assert_eq!(&*b, &[1, 2, 3, 4], "the other owner never sees the foreign write");
    assert!(a.is_unique() && b.is_unique());

    // The static empty block is never lent writable: an empty view has no bytes to write.
    let mut e: AlmideList<u8> = AlmideList::new();
    assert!(e.as_mut_slice().is_empty());
    // A unique block is lent in place.
    let before = a.as_ptr();
    let view = a.as_mut_slice();
    // SAFETY: as above.
    unsafe { foreign_fill(view.as_mut_ptr(), view.len(), 7) };
    assert_eq!(a.as_ptr(), before, "a unique block is written in place");
    assert_eq!(&*a, &[7, 7, 7, 7]);
}

#[test]
fn try_reserve_resize_and_swap_remove_agree_with_vec_and_copy_on_write() {
    let mut a: AlmideList<i64> = vec![1, 2, 3].into();
    let b = a.clone();
    a.try_reserve_exact(10).expect("room for ten more");
    assert!(a.capacity() >= 13 && a.is_unique());
    assert_eq!(&*b, &[1, 2, 3]);
    assert!(a.try_reserve_exact(usize::MAX).is_err(), "an impossible reservation is an error, not an abort");
    a.resize(5, 0);
    assert_eq!(&*a, &[1, 2, 3, 0, 0]);
    a.resize(2, 0);
    assert_eq!(&*a, &[1, 2]);
    let c = a.clone();
    assert_eq!(a.swap_remove(0), 1);
    assert_eq!(&*a, &[2]);
    assert_eq!(&*c, &[1, 2]);
    let v: Vec<i64> = vec![4, 5];
    let moved: AlmideList<i64> = v.into();
    assert_eq!(&*moved, &[4, 5]);
    assert_eq!(moved.to_vec().ref_count(), 2, "to_vec shares the block");
}

/// `almide_list![x; n]` in a context that only knows `T: Clone` takes the general fill.
fn fill_generic<T: Clone>(x: T, n: usize) -> AlmideList<T> {
    almide_str::almide_list![x; n]
}

#[test]
fn a_zero_fill_allocates_zeroed_and_the_list_is_an_ordinary_unique_block() {
    // The zeroed path: an all-zero value of each kind of scalar.
    let mut ints: AlmideList<i64> = almide_str::almide_list![0i64; 1000];
    assert_eq!(&ints[..], &vec![0i64; 1000][..]);
    assert!(ints.is_unique());
    assert_eq!(ints.capacity(), 1000);
    let floats: AlmideList<f64> = almide_str::almide_list![0.0f64; 7];
    assert_eq!(&floats[..], &[0.0f64; 7][..]);
    let flags: AlmideList<bool> = almide_str::almide_list![false; 3];
    assert_eq!(&flags[..], &[false; 3][..]);
    let bytes: AlmideList<u8> = almide_str::almide_list![0u8; 33];
    assert_eq!(&bytes[..], &[0u8; 33][..]);
    // Every element is readable and writable in place, and the block grows and drops as any other.
    ints[999] = 5;
    ints.push(6);
    let shared = ints.clone();
    ints[0] = 1;
    assert_eq!((ints[0], shared[0], shared[999], shared[1000]), (1, 0, 5, 6));
    // Zero elements: the static empty block.
    let none: AlmideList<i64> = almide_str::almide_list![0i64; 0];
    assert!(none.is_empty() && !none.is_unique());
}

#[test]
fn a_nonzero_or_general_fill_writes_each_element_into_one_block() {
    // A nonzero scalar, and -0.0 (not all-zero bits), take the clone fill.
    let sevens: AlmideList<i64> = almide_str::almide_list![7i64; 5];
    assert_eq!(&sevens[..], &[7i64; 5][..]);
    assert_eq!(sevens.capacity(), 5);
    let neg: AlmideList<f64> = almide_str::almide_list![-0.0f64; 4];
    assert!(neg.iter().all(|x| x.to_bits() == (-0.0f64).to_bits()));
    // A zero inside a generic context also takes the clone fill.
    let generic = fill_generic(0i64, 6);
    assert_eq!(&generic[..], &[0i64; 6][..]);
    // A heap element: the list holds `x` and n - 1 clones, as `vec!` does.
    let x = Rc::new(9);
    let held: AlmideList<Rc<i32>> = almide_str::almide_list![x.clone(); 4];
    assert_eq!(Rc::strong_count(&x), 5);
    drop(held);
    assert_eq!(Rc::strong_count(&x), 1);
    let strs = fill_generic(AlmideStr::from("ab"), 3);
    assert_eq!(strs.iter().map(|s| &**s).collect::<Vec<&str>>(), vec!["ab"; 3]);
    let empty = fill_generic(Rc::new(1), 0);
    assert!(empty.is_empty());
}

/// A clone that panics partway leaves the fill with exactly the elements it wrote, each dropped once.
#[test]
fn a_clone_that_panics_mid_fill_drops_what_was_written() {
    struct Bomb(Rc<Cell<usize>>, Rc<Cell<usize>>);
    impl Clone for Bomb {
        fn clone(&self) -> Self {
            self.0.set(self.0.get() + 1);
            assert!(self.0.get() < 3, "the third clone panics");
            Bomb(self.0.clone(), self.1.clone())
        }
    }
    impl Drop for Bomb {
        fn drop(&mut self) {
            self.1.set(self.1.get() + 1);
        }
    }
    let (clones, drops) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let bomb = Bomb(clones.clone(), drops.clone());
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| fill_generic(bomb, 10)));
    assert!(r.is_err());
    // two clones written, then the original: three drops
    assert_eq!((clones.get(), drops.get()), (3, 3));
}

#[test]
fn a_literal_moves_its_elements_into_one_exact_block() {
    use almide_str::almide_list;
    let l: AlmideList<String> = almide_list!["a".to_string(), "bc".to_string(), "d".to_string()];
    assert_eq!(l.len(), 3);
    assert_eq!(l.capacity(), 3);
    assert_eq!(l.iter().map(|s| s.as_str()).collect::<Vec<_>>(), ["a", "bc", "d"]);
    let one: AlmideList<i64> = almide_list![7];
    assert_eq!(one.as_slice(), &[7]);
    let empty: AlmideList<String> = AlmideList::from([] as [String; 0]);
    assert!(empty.is_empty());
}

#[test]
fn a_by_value_iteration_allocates_nothing_and_drops_what_it_never_yields() {
    // unique: elements move out; a partial walk drops the rest exactly once
    let l: AlmideList<String> = AlmideList::from(vec!["a".to_string(), "b".to_string(), "c".to_string(), "d".to_string()]);
    let mut it = l.into_iter();
    assert_eq!(it.len(), 4);
    assert_eq!(it.next().as_deref(), Some("a"));
    assert_eq!(it.next_back().as_deref(), Some("d"));
    assert_eq!(it.size_hint(), (2, Some(2)));
    drop(it);
    // shared: elements are cloned and the other owner keeps its block intact
    let l: AlmideList<String> = AlmideList::from(vec!["x".to_string(), "y".to_string()]);
    let keep = l.clone();
    let got: Vec<String> = l.into_iter().rev().collect();
    assert_eq!(got, ["y", "x"]);
    assert_eq!(keep.as_slice(), &["x".to_string(), "y".to_string()]);
    // empty: the static block is never written
    let e: AlmideList<String> = AlmideList::new();
    assert_eq!(e.into_iter().count(), 0);
}

#[test]
fn a_by_value_iterator_clones_only_what_it_has_not_yielded() {
    let l: AlmideList<String> = AlmideList::from(vec!["a".to_string(), "b".to_string(), "c".to_string()]);
    let mut it = l.into_iter();
    assert_eq!(it.next().as_deref(), Some("a"));
    let twin = it.clone();
    assert_eq!(it.collect::<Vec<_>>(), ["b", "c"]);
    assert_eq!(twin.collect::<Vec<_>>(), ["b", "c"]);
}

#[test]
fn a_map_over_a_unique_block_rewrites_it_in_place_and_unwinds_cleanly() {
    let l: AlmideList<i64> = AlmideList::from(vec![1, 2, 3]);
    let at = l.as_slice().as_ptr() as usize;
    let m: AlmideList<u64> = l.map_owned(|x| (x * 10) as u64);
    assert_eq!(m.as_slice(), &[10, 20, 30]);
    assert_eq!(m.as_slice().as_ptr() as usize, at, "same size and alignment: the block is reused");
    // shared: the other owner keeps its elements
    let l: AlmideList<String> = AlmideList::from(vec!["a".to_string(), "b".to_string()]);
    let keep = l.clone();
    let m: AlmideList<String> = l.map_owned(|s| s + "!");
    assert_eq!(m.as_slice(), &["a!".to_string(), "b!".to_string()]);
    assert_eq!(keep.as_slice(), &["a".to_string(), "b".to_string()]);
    // a panic part way: converted and unconverted elements are each dropped once (Miri checks the leak/double free)
    let l: AlmideList<String> = AlmideList::from(vec!["x".to_string(), "boom".to_string(), "z".to_string()]);
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        l.map_owned(|s| if s == "boom" { panic!("stop") } else { s + "?" })
    }));
    assert!(r.is_err());
}

/// `extend` and `extend_from_slice` count the length in a local; a panicking iterator or `clone` must still leave the
/// list holding exactly the elements written before it (each dropped once, nothing leaked).
#[test]
fn a_panic_while_extending_keeps_the_elements_written_so_far() {
    #[derive(Debug)]
    struct Bomb(Rc<Cell<usize>>, bool);
    impl Clone for Bomb {
        fn clone(&self) -> Self {
            if self.1 {
                panic!("clone");
            }
            Bomb(self.0.clone(), false)
        }
    }
    impl Drop for Bomb {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    let drops = Rc::new(Cell::new(0));
    let src = vec![Bomb(drops.clone(), false), Bomb(drops.clone(), false), Bomb(drops.clone(), true)];
    let mut a: AlmideList<Bomb> = AlmideList::new();
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| a.extend_from_slice(&src)));
    assert!(r.is_err());
    assert_eq!(a.len(), 2);
    drop(a);
    assert_eq!(drops.get(), 2);

    let mut b: AlmideList<i64> = AlmideList::with_capacity(8);
    b.push(1);
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        b.extend((2..10).map(|x| if x == 5 { panic!("iter") } else { x }));
    }));
    assert!(r.is_err());
    assert_eq!(&b[..], &[1, 2, 3, 4]);
    b.extend(5..12);
    assert_eq!(&b[..], &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]);
    let c: AlmideList<i64> = (0..3).collect();
    assert_eq!(&c[..], &[0, 1, 2]);
}

/// `extend` fills the reserved room through the iterator's own `fold`; an iterator that yields more than its size hint
/// grows the block out of line, and a panic in the middle still leaves exactly the elements written.
#[test]
fn an_extend_past_its_size_hint_grows_and_a_folded_extend_unwinds_cleanly() {
    struct Liar(std::ops::Range<i64>);
    impl Iterator for Liar {
        type Item = i64;
        fn next(&mut self) -> Option<i64> {
            self.0.next()
        }
        fn size_hint(&self) -> (usize, Option<usize>) {
            (1, Some(1))
        }
    }
    let mut a: AlmideList<i64> = AlmideList::from(vec![-1]);
    a.extend(Liar(0..100));
    assert_eq!(a.len(), 101);
    assert!(a.iter().copied().eq(std::iter::once(-1).chain(0..100)));
    // A shared source folds by clone and keeps its other owner's elements.
    let src: AlmideList<String> = AlmideList::from(vec!["p".to_string(), "q".to_string(), "boom".to_string()]);
    let keep = src.clone();
    let mut b: AlmideList<String> = AlmideList::new();
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        b.extend(src.into_iter().map(|s| if s == "boom" { panic!("map") } else { s + "!" }));
    }));
    assert!(r.is_err());
    assert_eq!(b.as_slice(), &["p!".to_string(), "q!".to_string()]);
    assert_eq!(keep.len(), 3);
}

/// `map_indexed` and `extend_indexed` fill by the source's count: a unique source's elements move (no clone), a shared
/// one's are cloned and its other owner keeps them, the fill appends after what the list holds, and a panic in `f`
/// leaves every element, written or not, dropped exactly once.
#[test]
fn an_indexed_fill_moves_or_clones_once_and_unwinds_cleanly() {
    let _window = MARKING_WINDOW.lock().unwrap_or_else(|e| e.into_inner());
    struct Counted(Rc<Cell<i64>>, Rc<Cell<i64>>, i64);
    impl Clone for Counted {
        fn clone(&self) -> Self {
            self.0.set(self.0.get() + 1);
            Counted(self.0.clone(), self.1.clone(), self.2)
        }
    }
    impl Drop for Counted {
        fn drop(&mut self) {
            self.1.set(self.1.get() + 1);
        }
    }
    let (clones, drops) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let make = |n: i64| -> AlmideList<Counted> { (0..n).map(|k| Counted(clones.clone(), drops.clone(), k)).collect() };

    let pairs: AlmideList<(i64, i64)> = make(4).map_indexed(|i, c| (i as i64, c.2 * 10));
    assert_eq!(pairs.as_slice(), &[(0, 0), (1, 10), (2, 20), (3, 30)]);
    assert_eq!((clones.get(), drops.get()), (0, 4), "a unique source moves each element once");

    let shared = make(3);
    let keep = shared.clone();
    let mut out: AlmideList<i64> = AlmideList::from(vec![7]);
    out.extend_indexed(shared, |i, c| c.2 + 100 * i as i64);
    assert_eq!(out.as_slice(), &[7, 0, 101, 202]);
    assert_eq!(clones.get(), 3, "a shared source is cloned once per element");
    assert_eq!(keep.len(), 3);
    drop(keep);
    assert_eq!(drops.get(), 4 + 3 + 3);

    drops.set(0);
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        make(5).map_indexed(|i, c| if i == 3 { panic!("f") } else { c })
    }));
    assert!(r.is_err());
    assert_eq!(drops.get(), 5, "three written, one handed to f, one never taken: each dropped once");
    assert!(AlmideList::<i64>::new().map_indexed(|_, x| x).is_empty());
}

#[test]
fn a_pusher_copies_a_shared_block_at_its_first_push_and_writes_the_length_back() {
    // A shared block: the first push copies it, the other owner keeps its elements.
    let mut a: AlmideList<AlmideStr> = vec![AlmideStr::from("x")].into();
    let keep = a.clone();
    {
        let mut p = a.pusher();
        for i in 0..20 {
            p.push(AlmideStr::from(format!("s{i}").as_str()));
        }
    }
    assert_eq!(a.len(), 21);
    assert_eq!(keep.len(), 1);
    assert_eq!(a[20].as_str(), "s19");
    // The static empty block, and a pusher that never pushes changes nothing.
    let mut e: AlmideList<u8> = AlmideList::new();
    drop(e.pusher());
    assert_eq!(e.len(), 0);
    {
        let mut p = e.pusher();
        for x in [1i64, 255, 256, -1] {
            p.push_lane(x);
        }
    }
    assert_eq!(&e[..], &[1u8, 255, 0, 255]);
    // A panic between pushes leaves the length covering what was written.
    let mut b: AlmideList<i64> = AlmideList::new();
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut p = b.pusher();
        for i in 0..10 {
            if i == 6 {
                panic!("body");
            }
            p.push(i);
        }
    }));
    assert!(r.is_err());
    assert_eq!(&b[..], &[0, 1, 2, 3, 4, 5]);
}

/// The standard render's appender loops (#4906): the list is pushed inside nested control flow, the loop is left by
/// `break`, by `return` and by `?`, and the list is read and pushed again after each loop; the length written back
/// covers exactly the elements pushed, and a block shared before the loop keeps its other owner's elements.
#[test]
fn a_pusher_writes_the_length_back_on_every_way_out_of_the_loop() {
    fn by_return(xs: &mut AlmideList<AlmideStr>, stop: i64) -> i64 {
        let mut p = xs.pusher();
        let mut i = 0i64;
        loop {
            if i == stop {
                return i;
            }
            if i % 2 == 0 {
                p.push(AlmideStr::from(format!("r{i}").as_str()));
            } else {
                match i % 3 {
                    0 => p.push(AlmideStr::from("t")),
                    _ => {}
                }
            }
            i += 1;
        }
    }
    fn by_question(xs: &mut AlmideList<i64>, items: &[Result<i64, String>]) -> Result<(), String> {
        let mut p = xs.pusher();
        for r in items {
            p.push(r.clone()?);
        }
        Ok(())
    }
    let mut a: AlmideList<AlmideStr> = vec![AlmideStr::from("x")].into();
    let keep = a.clone();
    {
        let mut p = a.pusher();
        let mut j = 0;
        while j < 40 {
            if j == 33 {
                break;
            }
            p.push(AlmideStr::from(format!("b{j}").as_str()));
            j += 1;
        }
    }
    assert_eq!((a.len(), keep.len()), (34, 1));
    assert_eq!((a[0].as_str(), a[33].as_str()), ("x", "b32"));
    assert_eq!(by_return(&mut a, 7), 7);
    assert_eq!(a.len(), 34 + 4 + 1);
    assert_eq!((a[34].as_str(), a[36].as_str(), a[38].as_str()), ("r0", "t", "r6"));
    a.push(AlmideStr::from("after"));
    assert_eq!(a.len(), 40);
    let mut n: AlmideList<i64> = AlmideList::new();
    assert_eq!(by_question(&mut n, &[Ok(1), Ok(2), Err("e".into()), Ok(4)]), Err("e".to_string()));
    assert_eq!(&n[..], &[1, 2]);
    assert_eq!(by_question(&mut n, &[Ok(3)]), Ok(()));
    assert_eq!(&n[..], &[1, 2, 3]);
}

/// `retain`, `dedup`, `drain` and `split_off` edit a unique block where it lies (no allocation, the elements move or
/// drop once each); a shared block keeps its other owner's elements; a panic part way leaves every element owned once.
#[test]
fn in_place_edits_keep_a_unique_block_and_unwind_cleanly() {
    let s = |x: &str| x.to_string();
    let mut a: AlmideList<String> = AlmideList::from(vec![s("a"), s("b"), s("b"), s("c"), s("d"), s("d")]);
    let at = a.as_slice().as_ptr() as usize;
    a.dedup();
    assert_eq!(a.as_slice(), &[s("a"), s("b"), s("c"), s("d")]);
    a.retain(|x| x != "b");
    assert_eq!(a.as_slice(), &[s("a"), s("c"), s("d")]);
    {
        let mut d = a.drain(..1);
        assert_eq!(d.next(), Some(s("a")));
    }
    assert_eq!(a.as_slice(), &[s("c"), s("d")]);
    a.push(s("e"));
    drop(a.drain(1..2)); // dropped without being read: the element drops, the tail moves down
    assert_eq!(a.as_slice(), &[s("c"), s("e")]);
    let tail = a.split_off(1);
    assert_eq!((a.as_slice(), tail.as_slice()), (&[s("c")][..], &[s("e")][..]));
    assert_eq!(a.as_slice().as_ptr() as usize, at, "every edit stayed in the block");

    // shared: the other owner keeps every element
    let mut b: AlmideList<String> = AlmideList::from(vec![s("x"), s("y"), s("y"), s("z")]);
    let keep = b.clone();
    b.retain(|x| x != "x");
    assert_eq!(b.as_slice(), &[s("y"), s("y"), s("z")]);
    let mut c = keep.clone();
    c.dedup();
    let rev: Vec<String> = c.drain(1..).rev().collect();
    assert_eq!(rev, vec![s("z"), s("y")]);
    let mut d = keep.clone();
    let t = d.split_off(2);
    assert_eq!((d.len(), t.as_slice()), (2, &[s("y"), s("z")][..]));
    assert_eq!(keep.as_slice(), &[s("x"), s("y"), s("y"), s("z")]);

    // a panic in the predicate: what was kept and what was not yet looked at both stay, once each
    let mut e: AlmideList<String> = AlmideList::from(vec![s("p"), s("drop"), s("boom"), s("q")]);
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        e.retain(|x| if x == "boom" { panic!("stop") } else { x != "drop" })
    }));
    assert!(r.is_err());
    assert_eq!(e.as_slice(), &[s("p"), s("boom"), s("q")]);
    let empty: &mut AlmideList<String> = &mut AlmideList::new();
    assert_eq!(empty.drain(..).count(), 0);
    empty.retain(|_| false);
    assert!(empty.split_off(0).is_empty());
}
