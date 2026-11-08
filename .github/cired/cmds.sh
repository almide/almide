set +e
echo "=== NC revert 500452060"
cargo test --release --test map_update_reuse_test 2>&1 | grep -E "^test |test result|panicked|took|allocations"
