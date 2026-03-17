const __almd_testing = {
  assert_throws(f, expected) {
    try { f(); throw new Error("__no_throw__"); }
    catch (e) {
      if (e.message === "__no_throw__") throw new Error("assert_throws: expected error '" + expected + "' but function succeeded");
      if (!e.message.includes(expected)) throw new Error("assert_throws: expected error containing '" + expected + "' but got '" + e.message + "'");
    }
  },
  assert_contains(haystack, needle) {
    if (!haystack.includes(needle)) throw new Error("assert_contains failed\n  expected to contain: \"" + needle + "\"\n  in: \"" + haystack + "\"");
  },
  assert_approx(a, b, tolerance) {
    if (Math.abs(a - b) > tolerance) throw new Error("assert_approx failed\n  left:  " + a + "\n  right: " + b + "\n  diff:  " + Math.abs(a - b) + " > tolerance " + tolerance);
  },
  assert_gt(a, b) {
    if (a <= b) throw new Error("assert_gt failed: " + a + " is not greater than " + b);
  },
  assert_lt(a, b) {
    if (a >= b) throw new Error("assert_lt failed: " + a + " is not less than " + b);
  },
  assert_some(opt) {
    if (opt === null || opt === undefined) throw new Error("assert_some failed: got none");
  },
  assert_ok(result) {
    if (result instanceof __Err) throw new Error("assert_ok failed: got err(" + result.message + ")");
  },
};
