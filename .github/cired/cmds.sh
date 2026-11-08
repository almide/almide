set +e
D=.github/cired/src
echo "=== render"
CIRED_SRCS=$D/map_update.almd,$D/accum.almd,$D/scoped.almd cargo test --release --test zz_cired_render -- --nocapture > /tmp/cired-out-render.txt 2>&1; tail -3 /tmp/cired-out-render.txt
echo "=== allocs scoped"; ALMIDE_ALLOC_COUNT=1 almide run $D/scoped.almd 2>&1 | tail -2
echo "=== tests"
for t in map_update_reuse_test accumulate_in_place_test scoped_closure_capture_test; do cargo test --release --test $t -- --nocapture 2>&1 | grep -v "^\s*Compiling" | tail -25; done
echo "=== alloc ledger"
ALMIDE_UPDATE_ALLOC=1 cargo test --release --test native_borrow_oracle_test alloc 2>&1 | tail -5; git diff tests/golden/native-borrow-oracle-alloc.txt
cargo test --release --test native_borrow_oracle_test 2>&1 | grep -E "^test |test result" | head -20
echo "=== insta"
INSTA_UPDATE=always cargo test --release --test ide_outline_snapshot_test 2>&1 | grep -E "^test |test result"; git status --short tests/snapshots; git diff tests/snapshots > /tmp/cired-out-insta.patch; command ls tests/snapshots | grep "\.snap\.new" 
echo "=== inspect"
sudo sysctl -w kernel.perf_event_paranoid=-1 >/dev/null 2>&1; which perf || (sudo apt-get install -y -qq linux-tools-common linux-tools-$(uname -r) >/dev/null 2>&1; which perf)
perf stat -e instructions:u true 2>&1 | tail -6
