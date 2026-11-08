set +e
echo "=== cluster tests"
for t in map_update_reuse_test accumulate_in_place_test scoped_closure_capture_test ide_outline_snapshot_test stdlib_inspection_test native_closure_environment_test native_runtime_boundaries_test; do cargo test --release --test $t 2>&1 | grep -E "^test |test result|panicked|took|re-inspect|over the ceiling" | head -20; done
echo "=== alloc ledger regen"
ALMIDE_UPDATE_ALLOC=1 cargo test --release --test native_borrow_oracle_test alloc 2>&1 | tail -2; git diff tests/golden/native-borrow-oracle-alloc.txt > /tmp/cired-out-alloc.patch; git diff tests/golden/native-borrow-oracle-alloc.txt
cargo test --release --test native_borrow_oracle_test 2>&1 | grep -E "^test |test result"
echo "=== native tests"
for t in $(ls tests | grep -E '^(native_|closure|hof|map_|fold|scoped|interp|string_)' | sed 's/\.rs$//'); do cargo test --release --test $t 2>&1 | grep -E "test result|FAILED|panicked" | sed "s/^/$t: /" | head -4; done
echo "=== mir lib"
cargo test --release -p almide-mir 2>&1 | grep -E "test result|FAILED|panicked" | head -20
