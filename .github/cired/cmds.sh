set +e
D=.github/cired/src
echo "=== render"
CIRED_SRCS=$D/map_update.almd,$D/accum.almd,$D/scoped.almd cargo test --release --test zz_cired_render -- --nocapture > /tmp/cired-out-render.txt 2>&1; tail -3 /tmp/cired-out-render.txt
echo "=== allocs scoped"; ALMIDE_ALLOC_COUNT=1 almide run $D/scoped.almd 2>&1 | tail -2
echo "=== tests"
for t in map_update_reuse_test accumulate_in_place_test scoped_closure_capture_test native_closure_environment_test native_runtime_boundaries_test string_push_native_append_test native_borrow_oracle_test; do cargo test --release --test $t 2>&1 | grep -E "^test |test result|panicked|left|right|took" | head -30; done
echo "=== mir lib"
cargo test --release -p almide-mir --lib 2>&1 | grep -E "test result|FAILED|panicked" | head -20
echo "=== alloc ledger"
sed -i 's/let _ = std::fs::remove_dir_all(&dir);//' tests/native_borrow_oracle_test.rs
ALMIDE_UPDATE_ALLOC=1 cargo test --release --test native_borrow_oracle_test alloc 2>&1 | tail -3; git diff tests/golden/native-borrow-oracle-alloc.txt
S=$(find / -name oracle_strs.almd 2>/dev/null | head -1); echo "strs at $S"; cp "$S" /tmp/cired-out-oracle_strs.almd
CIRED_SRCS=$S cargo test --release --test zz_cired_render -- --nocapture > /tmp/cired-out-render-strs.txt 2>&1
ALMIDE_ALLOC_COUNT=1 almide run $S 2>&1 | tail -3
git checkout tests/native_borrow_oracle_test.rs
