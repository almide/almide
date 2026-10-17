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

pub fn almide_rt_test_assert_none<T: std::fmt::Debug>(o: &Option<T>) {
    assert!(o.is_none(), "assert_none failed: got {:?}", o);
}

pub fn almide_rt_test_assert_err<T: std::fmt::Debug, E: std::fmt::Debug>(r: &Result<T, E>) {
    assert!(r.is_err(), "assert_err failed: got {:?}", r);
}

// testing.assert_aborts (ADR-0029): the native leg's abort-site pair. The
// test binary is libtest; the codegen emits one hidden, ignored test,
// `tests::__almd_abort_site_entry`, that runs the site named by
// ALMIDE_ABORT_SITE. A call re-executes this same binary selecting only that
// entry (Go's `BE_CRASHER` pattern, done by the runtime), with
// ALMIDE_TEST_ABORT_PROBE set so `panic` takes the C-219 abort path in the
// child, and answers [status, stdout, stderr]. The verdict is
// stdlib/testing.almd's `__assert_aborts_check`.

/// The libtest banner the child prints before the entry runs (`-q`).
const ALMIDE_ABORT_SITE_BANNER: &str = "\nrunning 1 test\n";

pub fn almide_rt_test_abort_site_run(site: i64) -> Vec<AlmideString> {
    almide_stdout_flush();
    // A child never starts a grandchild (E099 refuses the nesting at check
    // time; this bounds a direct call of the private fn).
    if std::env::var_os("ALMIDE_ABORT_SITE").is_some() {
        let why = "the abort-site child did not start: an abort site cannot start another";
        return vec![AlmideString::from("-1".to_string()), AlmideString::from(String::new()), AlmideString::from(why.to_string())];
    }
    let child = std::env::current_exe().and_then(|exe| {
        std::process::Command::new(exe)
            .args(["tests::__almd_abort_site_entry", "--exact", "--ignored", "--nocapture", "--test-threads=1", "-q"])
            .env("ALMIDE_ABORT_SITE", site.to_string())
            .env("ALMIDE_TEST_ABORT_PROBE", "1")
            .stdin(std::process::Stdio::null())
            .output()
    });
    let (status, stdout, stderr) = match child {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let stdout = stdout.strip_prefix(ALMIDE_ABORT_SITE_BANNER).unwrap_or(&stdout).to_string();
            (out.status.code().unwrap_or(-1), stdout, String::from_utf8_lossy(&out.stderr).to_string())
        }
        Err(e) => (-1, String::new(), format!("the abort-site child did not start: {e}")),
    };
    vec![AlmideString::from(status.to_string()), AlmideString::from(stdout), AlmideString::from(stderr)]
}

/// The abort site this process was started to run, or -1.
pub fn almide_rt_test_abort_site_selected() -> i64 {
    std::env::var("ALMIDE_ABORT_SITE").ok().and_then(|v| v.parse().ok()).unwrap_or(-1)
}
