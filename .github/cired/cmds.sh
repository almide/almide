set +e
echo "=== NC revert e9e371e20"
cargo test --release --test scoped_closure_capture_test 2>&1 | grep -E "^test |test result|panicked|took|allocations"
