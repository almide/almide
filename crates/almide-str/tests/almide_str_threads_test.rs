//! The thread-shared mark (ADR-0013 design point 3): a marked block is cloned
//! and dropped from several scoped threads at once, its count stays exact, and
//! it frees exactly once. CI runs this under Miri with the rest of the crate,
//! which checks the atomic path for data races as well as the block handling.
use almide_str::{almide_str_mark_reachable, AlmideStr};

const THREADS: usize = 4;
const CLONES: usize = if cfg!(miri) { 8 } else { 2_000 };

#[test]
fn a_marked_block_is_cloned_and_dropped_from_several_threads() {
    let s = AlmideStr::from("shared across threads");
    almide_str_mark_reachable(&s);
    assert!(s.is_thread_shared());
    std::thread::scope(|scope| {
        for _ in 0..THREADS {
            scope.spawn(|| {
                let held: Vec<AlmideStr> = (0..CLONES).map(|_| s.clone()).collect();
                assert!(held.iter().all(|c| c.as_str() == "shared across threads"));
            });
        }
    });
    assert_eq!(s.ref_count(), 1);
    assert!(s.is_unique());
}

#[test]
fn marking_a_list_marks_every_element_and_leaves_static_blocks_alone() {
    let xs = vec![AlmideStr::from("a"), AlmideStr::from("b"), AlmideStr::new()];
    almide_str_mark_reachable(&xs);
    assert!(xs[0].is_thread_shared() && xs[1].is_thread_shared());
    assert!(!xs[2].is_thread_shared(), "the immortal empty block is never written");
    assert_eq!(xs[0].ref_count(), 1, "the marking clone is dropped again");
}

#[test]
fn an_unmarked_clone_outside_the_window_stays_thread_local() {
    let s = AlmideStr::from("local");
    let t = s.clone();
    assert!(!s.is_thread_shared());
    assert_eq!(t.ref_count(), 2);
}

#[test]
fn a_shared_block_edited_after_the_threads_end_copies_only_while_aliased() {
    let mut s = AlmideStr::from("ab");
    almide_str_mark_reachable(&s);
    let t = s.clone();
    std::thread::scope(|scope| {
        scope.spawn(|| drop(t.clone()));
    });
    s.push_str("x");
    assert_eq!((s.as_str(), t.as_str()), ("abx", "ab"));
    drop(t);
    let mut u = AlmideStr::from("cd");
    almide_str_mark_reachable(&u);
    u.push_str("e");
    assert_eq!(u.as_str(), "cde");
}
