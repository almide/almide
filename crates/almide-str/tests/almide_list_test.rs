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
/// element clones holds this lock, as the marking test does, so the two
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
