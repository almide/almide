//! AlmideStr against `String`: sharing, copy-on-write, the unique view, and
//! a differential run of random edits over handles that share blocks. CI also
//! runs this file under Miri, which checks the raw block handling: no
//! use-after-free, leak, out-of-bounds or aliasing violation.
use almide_str::{AlmideStr, AlmideStrStatic};
use std::collections::{BTreeSet, HashMap};

#[test]
fn the_empty_string_is_static_and_never_allocates() {
    let e = AlmideStr::new();
    assert_eq!(e.ref_count(), 0);
    assert_eq!(e.as_str(), "");
    assert!(AlmideStr::ptr_eq(&e, &AlmideStr::from("")));
    let c = e.clone();
    assert_eq!(c.ref_count(), 0, "cloning an immortal block does not count");
}

#[test]
fn a_clone_shares_the_block_and_drop_releases_it() {
    let a = AlmideStr::from("shared text");
    assert!(a.is_unique());
    let b = a.clone();
    assert!(AlmideStr::ptr_eq(&a, &b));
    assert_eq!(a.ref_count(), 2);
    drop(b);
    assert!(a.is_unique());
}

#[test]
fn an_edit_through_a_shared_handle_copies_and_leaves_the_other_unchanged() {
    let a = AlmideStr::from("abc");
    let mut b = a.clone();
    b.push_str("def");
    assert_eq!(a, "abc");
    assert_eq!(b, "abcdef");
    assert!(!AlmideStr::ptr_eq(&a, &b));
    assert!(a.is_unique() && b.is_unique());
}

#[test]
fn a_unique_edit_stays_in_place() {
    let mut a = AlmideStr::with_capacity(64);
    a.push_str("abc");
    let before = a.as_str().as_ptr();
    a.push('é');
    a.push_str("xyz");
    assert_eq!(a.as_str().as_ptr(), before, "within capacity the block does not move");
    assert_eq!(a, "abcéxyz");
}

#[test]
fn a_static_literal_is_copied_on_its_first_edit() {
    static HELLO: AlmideStrStatic<5> = AlmideStrStatic::new("hello");
    let lit = AlmideStr::from_static(&HELLO);
    assert_eq!(lit.ref_count(), 0);
    let mut s = lit.clone();
    s.push_str(", world");
    assert_eq!(lit, "hello");
    assert_eq!(s, "hello, world");
    assert!(s.is_unique());
}

#[test]
fn the_unique_view_writes_its_length_back_when_it_ends() {
    let mut s = AlmideStr::new();
    {
        let mut u = s.unique();
        for i in 0..1000 {
            u.push_str(if i % 2 == 0 { "ab" } else { "ç" });
        }
        assert_eq!(u.len(), 2000, "\"ç\" is two bytes");
    }
    assert_eq!(s.len(), 2000);
    assert!(s.capacity() >= 2000);
    assert!(s.starts_with("abçab"));
}

#[test]
fn concat_appends_in_place_when_unique_and_copies_when_shared() {
    let a = AlmideStr::from("left");
    let keep = a.clone();
    let b = a.concat("+right");
    assert_eq!(keep, "left");
    assert_eq!(b, "left+right");
    let c = b.concat("!");
    assert_eq!(c, "left+right!");
}

#[test]
fn clear_on_a_shared_handle_leaves_the_other_owner_its_text() {
    let a = AlmideStr::from("kept");
    let mut b = a.clone();
    b.clear();
    assert_eq!(a, "kept");
    assert!(b.is_empty());
}

#[test]
fn ordering_hashing_and_formatting_follow_str() {
    let words = ["pear", "apple", "fig", "apple", ""];
    let set: BTreeSet<AlmideStr> = words.iter().map(|w| AlmideStr::from(*w)).collect();
    let order: Vec<&str> = set.iter().map(|s| s.as_str()).collect();
    assert_eq!(order, vec!["", "apple", "fig", "pear"]);
    let mut by_name: HashMap<AlmideStr, usize> = HashMap::new();
    for (i, w) in words.iter().enumerate() {
        by_name.insert(AlmideStr::from(*w), i);
    }
    assert_eq!(by_name.get("apple"), Some(&3), "a &str lookup finds the AlmideStr key");
    let s = AlmideStr::from("tab\there \"q\"");
    assert_eq!(format!("{s}"), "tab\there \"q\"");
    assert_eq!(format!("{s:?}"), format!("{:?}", "tab\there \"q\""));
    assert_eq!(format!("[{s:>14}]"), format!("[{:>14}]", "tab\there \"q\""));
}

#[test]
fn conversions_round_trip() {
    let s: AlmideStr = String::from("owned").into();
    let back: String = s.clone().into();
    assert_eq!(back, "owned");
    let chars: AlmideStr = "héllo".chars().rev().collect();
    assert_eq!(chars, "olléh");
    let joined: AlmideStr = ["a", "bc", "", "d"].into_iter().collect();
    assert_eq!(joined, "abcd");
    assert_eq!(AlmideStr::from('ß'), "ß");
}

/// A tiny deterministic generator, so the differential run needs no dependency.
struct Lcg(u64);
impl Lcg {
    fn next(&mut self, n: usize) -> usize {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((self.0 >> 33) as usize) % n
    }
}

#[test]
fn random_edits_over_shared_handles_match_a_string_model() {
    const PIECES: [&str; 6] = ["", "a", "xyz", "ünï", "0123456789abcdef", "🙂"];
    let steps = if cfg!(miri) { 400 } else { 20_000 };
    let mut rng = Lcg(0x5eed);
    let mut real: Vec<AlmideStr> = vec![AlmideStr::new()];
    let mut model: Vec<String> = vec![String::new()];
    for _ in 0..steps {
        let i = rng.next(real.len());
        match rng.next(7) {
            0 if real.len() < 16 => {
                real.push(real[i].clone());
                model.push(model[i].clone());
            }
            1 if real.len() > 1 => {
                real.swap_remove(i);
                model.swap_remove(i);
            }
            2 => {
                let p = PIECES[rng.next(PIECES.len())];
                let taken = std::mem::take(&mut real[i]);
                real[i] = taken.concat(p);
                model[i].push_str(p);
            }
            3 => {
                real[i].clear();
                model[i].clear();
            }
            4 => {
                let n = rng.next(4);
                let mut u = real[i].unique();
                for _ in 0..n {
                    u.push('é');
                }
                drop(u);
                model[i].extend(std::iter::repeat_n('é', n));
            }
            _ => {
                let p = PIECES[rng.next(PIECES.len())];
                real[i].push_str(p);
                model[i].push_str(p);
            }
        }
        for (r, m) in real.iter().zip(&model) {
            assert_eq!(r.as_str(), m.as_str());
        }
    }
}

#[test]
fn the_header_is_sixteen_bytes_and_capacity_survives_every_edit_path() {
    assert_eq!(std::mem::size_of::<almide_str::AlmideStrHeader>(), 16);
    let mut s = AlmideStr::with_capacity(10);
    assert_eq!((s.len(), s.capacity()), (0, 10));
    s.push_str("abc"); // the handle's fast path
    assert_eq!((s.len(), s.capacity()), (3, 10));
    {
        let mut u = s.unique();
        u.push_str("defg");
        u.push('h');
    }
    assert_eq!((s.as_str(), s.capacity()), ("abcdefgh", 10));
    s.push_str("ijklmnop"); // grows
    assert_eq!(s.as_str(), "abcdefghijklmnop");
    assert!(s.capacity() >= 16);
    let cap = s.capacity();
    s.clear();
    assert_eq!((s.len(), s.capacity()), (0, cap), "clearing a unique block keeps its room");
    s.push_str("z");
    assert_eq!((s.as_str(), s.capacity()), ("z", cap));
}

#[test]
fn from_i64_renders_like_to_string_in_one_exact_block() {
    for n in [0i64, 1, -1, 9, 10, -10, 12345, -98765, i64::MAX, i64::MIN, 1 << 40, -(1 << 40)] {
        let s = AlmideStr::from_i64(n);
        assert_eq!(s.as_str(), n.to_string());
        assert_eq!(s.capacity(), s.len(), "one exact-size block, not a copy of a String");
        assert!(s.is_unique());
    }
}

#[test]
fn join_matches_slice_join_and_leaves_the_parts_shared() {
    let cases: Vec<Vec<&str>> = vec![vec![], vec![""], vec!["a"], vec!["", ""], vec!["ab", "", "cde"], vec!["x"; 50]];
    for parts in &cases {
        let owned: Vec<AlmideStr> = parts.iter().map(|p| AlmideStr::from(*p)).collect();
        for sep in ["", ",", "::", "é"] {
            let j = AlmideStr::join(&owned, sep);
            assert_eq!(j.as_str(), parts.join(sep));
            assert_eq!(j.capacity(), j.len());
        }
    }
    let p = AlmideStr::from("shared");
    let q = p.clone();
    let j = AlmideStr::join(&[p, AlmideStr::from("x")], "-");
    assert_eq!(j, "shared-x");
    assert!(q.is_unique(), "join only reads its parts");
}
