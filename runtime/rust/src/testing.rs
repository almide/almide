// testing extern — Rust native implementations

pub fn almide_rt_testing_assert_approx(a: f64, b: f64, epsilon: f64) {
    assert!((a - b).abs() < epsilon, "assert_approx failed: {} != {} (epsilon {})", a, b, epsilon);
}

pub fn almide_rt_testing_assert_contains(haystack: &str, needle: &str) {
    assert!(haystack.contains(needle), "assert_contains failed: {:?} does not contain {:?}", haystack, needle);
}

pub fn almide_rt_testing_assert_gt(a: i64, b: i64) {
    assert!(a > b, "assert_gt failed: {} is not > {}", a, b);
}

pub fn almide_rt_testing_assert_lt(a: i64, b: i64) {
    assert!(a < b, "assert_lt failed: {} is not < {}", a, b);
}

pub fn almide_rt_testing_assert_ok<T: std::fmt::Debug, E: std::fmt::Debug>(r: &Result<T, E>) {
    assert!(r.is_ok(), "assert_ok failed: got {:?}", r);
}

pub fn almide_rt_testing_assert_some<T: std::fmt::Debug>(o: &Option<T>) {
    assert!(o.is_some(), "assert_some failed: got None");
}
