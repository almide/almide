set +e
f=spec/wasm_cross/captured_var_value_stored.almd
almide run $f > /tmp/n.txt 2>&1; echo "native exit $?"; almide run $f --target wasm > /tmp/w.txt 2>&1; echo "wasm exit $?"; cat /tmp/n.txt; diff /tmp/n.txt /tmp/w.txt && echo SAME
ALMIDE_UPDATE_NATIVE_VERIFIED_FLOOR=1 cargo test --release --test native_verified_floor_test 2>&1 | grep -E "test result|floor" | head -5
git diff proofs/native-verified-floor.txt | tee /tmp/cired-out-floor.patch
cargo test --release --test native_verified_floor_test 2>&1 | grep -E "test result"
