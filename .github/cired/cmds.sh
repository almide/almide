set +e
echo "=== NC revert 61b947af4"
cargo test --release --test accumulate_in_place_test 2>&1 | grep -E "^test |test result|panicked|took|allocations"
