//! AlmideStr against `String`: sharing, copy-on-write, the unique view, and
//! a differential run of random edits over handles that share blocks. CI also
//! runs this file under Miri, which checks the raw block handling: no
//! use-after-free, leak, out-of-bounds or aliasing violation.
use almide_str::{AlmideStr, AlmideStrStatic};
use std::collections::{BTreeSet, HashMap};

#[test]
fn ascii_case_construction_preserves_unicode_and_shared_inputs() {
    for text in ["", "aBc\0z!", "ßıİKΣςéAbZ", "東京🦀aZ"] {
        let original = AlmideStr::from(text);
        original.mark_shared();
        let alias = original.clone();
        let upper = AlmideStr::from_ascii_uppercase(original.as_str());
        let lower = AlmideStr::from_ascii_lowercase(alias.as_str());
        assert_eq!(upper.as_str(), text.to_ascii_uppercase());
        assert_eq!(lower.as_str(), text.to_ascii_lowercase());
        assert_eq!(original.as_str(), text);
        assert_eq!(alias.as_str(), text);
        if !text.is_empty() {
            assert!(!AlmideStr::ptr_eq(&upper, &original));
            assert!(!AlmideStr::ptr_eq(&lower, &original));
            assert_eq!(upper.capacity(), text.len());
            assert_eq!(lower.capacity(), text.len());
        }
    }
    static TEXT: AlmideStrStatic<3> = AlmideStrStatic::new("aBz");
    let original = AlmideStr::from_static(&TEXT);
    assert_eq!(AlmideStr::from_ascii_uppercase(&original).as_str(), "ABZ");
    assert_eq!(AlmideStr::from_ascii_lowercase(&original).as_str(), "abz");
    assert_eq!(original.as_str(), "aBz");
}

#[test]
fn ascii_case_construction_covers_every_ascii_byte_and_preserves_static_empty() {
    let ascii: String = (0u8..=127).map(char::from).collect();
    let text = format!("{ascii}é🦀Az");
    assert_eq!(AlmideStr::from_ascii_uppercase(&text).as_str(), text.to_ascii_uppercase());
    assert_eq!(AlmideStr::from_ascii_lowercase(&text).as_str(), text.to_ascii_lowercase());
    let empty = AlmideStr::new();
    for converted in [AlmideStr::from_ascii_uppercase(""), AlmideStr::from_ascii_lowercase("")] {
        assert!(AlmideStr::ptr_eq(&converted, &empty));
        assert_eq!(converted.capacity(), 0);
    }
}

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
fn prepend_writes_in_place_when_unique_and_copies_when_shared() {
    let mut s = AlmideStr::with_capacity(16);
    s.push_str("tail");
    let block = s.clone();
    drop(block);
    let p = s.prepend("head-");
    assert_eq!(p, "head-tail");
    assert!(p.is_unique());
    let shared = AlmideStr::from("b");
    let keep = shared.clone();
    let q = shared.prepend("a");
    assert_eq!((q.as_str(), keep.as_str()), ("ab", "b"), "a shared right operand is left as it was");
    assert_eq!(q.capacity(), 2);
    assert_eq!(AlmideStr::from("x").prepend(""), "x");
    assert_eq!(AlmideStr::new().prepend("lit"), "lit");
    let long = "é".repeat(100);
    assert_eq!(AlmideStr::from("z").prepend(&long).as_str(), format!("{long}z"));
}

#[test]
fn from_fmt_renders_like_format_in_one_block() {
    let long = "x".repeat(1000);
    for (got, want) in [
        (AlmideStr::from_fmt(format_args!("plain")), "plain".to_string()),
        (AlmideStr::from_fmt(format_args!("{}-{}", 1, "a")), format!("{}-{}", 1, "a")),
        (AlmideStr::from_fmt(format_args!("{:>300}", "r")), format!("{:>300}", "r")),
        (AlmideStr::from_fmt(format_args!("{long}{long}é{}", 7)), format!("{long}{long}é{}", 7)),
    ] {
        assert_eq!(got.as_str(), want);
        assert!(got.is_unique());
    }
    let short = AlmideStr::from_fmt(format_args!("n={}", 42));
    assert_eq!(short.capacity(), short.len(), "a short rendering is one exact-size block");
    assert_eq!(AlmideStr::from_fmt(format_args!("{}", "")).ref_count(), 0, "empty is the static block");
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
fn int_text_formats_like_display() {
    for n in [0i64, 1, -1, 9, 10, -10, 99, 100, 12345, -98765, i64::MAX, i64::MIN, 1 << 40, -(1 << 40)] {
        assert_eq!(format!("<{}>", almide_str::IntText(n)), format!("<{n}>"));
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

#[test]
fn retaining_utf8_ranges_reuses_unique_capacity_and_preserves_aliases() {
    let original = " \u{2003}é🦀z\u{2009}\t";
    let start = original.len() - original.trim_start().len();
    let end = start + original.trim().len();
    for shared in [false, true] {
        let mut text = AlmideStr::with_capacity(128);
        text.push_str(original);
        text.mark_shared();
        let alias = shared.then(|| text.clone());
        let before = text.as_str().as_ptr();
        text.retain_range(start..end);
        assert_eq!(text.as_str(), "é🦀z");
        if let Some(alias) = alias {
            assert_eq!(alias.as_str(), original);
            assert_ne!(text.as_str().as_ptr(), before);
        } else {
            assert_eq!(text.as_str().as_ptr(), before);
            assert_eq!(text.capacity(), 128);
        }
        text.push_str("after");
        assert_eq!(text.as_str(), "é🦀zafter");
        let end = text.len();
        text.retain_range(end..end);
        assert!(text.is_empty());
        text.push('é');
        assert_eq!(text.as_str(), "é");
    }
    static TEXT: AlmideStrStatic<4> = AlmideStrStatic::new(" aZ ");
    let mut text = AlmideStr::from_static(&TEXT);
    let alias = text.clone();
    text.retain_range(1..3);
    text.unique().make_ascii_uppercase();
    assert_eq!(text.as_str(), "AZ");
    assert_eq!(alias.as_str(), " aZ ");
    let mut empty = AlmideStr::new();
    empty.retain_range(0..0);
    assert_eq!(empty.ref_count(), 0);
}

#[test]
fn invalid_retained_ranges_panic_before_mutation() {
    for range in [1..2, 0..1, 0..10, 3..2] {
        let mut text = AlmideStr::from("éAz");
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| text.retain_range(range)));
        assert!(result.is_err());
        assert_eq!(text.as_str(), "éAz");
    }
}

#[test]
fn unique_ascii_case_edits_preserve_unicode_capacity_and_shared_handles() {
    let ascii: String = (0u8..=127).map(char::from).collect();
    let original = format!("{ascii}ßıİKΣςé東京🦀Az");
    for shared in [false, true] {
        let mut text = AlmideStr::from(original.as_str());
        text.mark_shared();
        let alias = shared.then(|| text.clone());
        let before = text.as_str().as_ptr();
        text.unique().make_ascii_uppercase();
        assert_eq!(text.as_str(), original.to_ascii_uppercase());
        if let Some(alias) = alias {
            assert_eq!(alias.as_str(), original);
            assert_ne!(text.as_str().as_ptr(), before);
        } else { assert_eq!(text.as_str().as_ptr(), before); }
        text.unique().make_ascii_lowercase();
        assert_eq!(text.as_str(), original.to_ascii_lowercase());
    }
    let mut empty = AlmideStr::new();
    empty.unique().make_ascii_uppercase();
    empty.unique().make_ascii_lowercase();
    assert!(empty.is_empty());
}
