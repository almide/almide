// testing extern — Rust native implementations

pub fn almide_rt_test_assert_approx(a: f64, b: f64, epsilon: f64) {
    assert!((a - b).abs() < epsilon, "assert_approx failed: {} != {} (epsilon {})", a, b, epsilon);
}

pub fn almide_rt_test_assert_contains(haystack: &str, needle: &str) {
    assert!(haystack.contains(needle), "assert_contains failed: {:?} does not contain {:?}", haystack, needle);
}

pub fn almide_rt_test_assert_gt(a: i64, b: i64) {
    assert!(a > b, "assert_gt failed: {} is not > {}", a, b);
}

pub fn almide_rt_test_assert_lt(a: i64, b: i64) {
    assert!(a < b, "assert_lt failed: {} is not < {}", a, b);
}

pub fn almide_rt_test_assert_ok<T: std::fmt::Debug, E: std::fmt::Debug>(r: &Result<T, E>) {
    assert!(r.is_ok(), "assert_ok failed: got {:?}", r);
}

pub fn almide_rt_test_assert_some<T: std::fmt::Debug>(o: &Option<T>) {
    assert!(o.is_some(), "assert_some failed: got None");
}

pub fn almide_rt_test_assert_throws(f: std::rc::Rc<dyn Fn()>, expected: &str) {
    // `Rc<dyn Fn>` is neither `FnOnce` nor `UnwindSafe`; wrap it in a fresh
    // closure (FnOnce) and assert unwind-safety — the closure body is expected to
    // panic, and its captured `Rc` is only read, so the catch is sound.
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || f()));
    match result {
        Err(panic) => {
            let msg = panic.downcast_ref::<String>().map(|s| s.as_str())
                .or_else(|| panic.downcast_ref::<&str>().copied())
                .unwrap_or("unknown panic");
            assert!(msg.contains(expected), "assert_throws: expected panic containing {:?}, got {:?}", expected, msg);
        }
        Ok(_) => panic!("assert_throws: expected panic but function returned normally"),
    }
}
