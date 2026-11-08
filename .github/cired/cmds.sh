set +e
git diff --stat; git diff > /tmp/cired-out-regen.patch
sudo apt-get install -y -qq valgrind >/dev/null 2>&1
D=research/benchmark/perf/listbuild
for v in append combinator; do
  cargo run -q --release -p almide-mir --example probe_native $D/listbuild_$v.almd > /tmp/cired-out-v1-$v.rs 2>&1
  almide $D/listbuild_$v.almd --target rust > /tmp/cired-out-v0-$v.rs 2>&1
  almide build $D/listbuild_$v.almd -o /tmp/lb_$v >/dev/null 2>&1
  echo "=== $v"; /tmp/lb_$v 12
  valgrind --tool=callgrind --callgrind-out-file=/tmp/cg_$v.out /tmp/lb_$v 20 2>&1 | grep -E "Collected|refs"
  head -c 300 /tmp/cired-out-v1-$v.rs; echo
  callgrind_annotate /tmp/cg_$v.out 2>/dev/null | head -40 > /tmp/cired-out-cg-$v.txt
  head -30 /tmp/cired-out-cg-$v.txt | cut -c1-200
done
echo "=== native floor + perf gate pieces"
curl -sSfL https://github.com/bytecodealliance/wasmtime/releases/download/v50.0.1/wasmtime-v50.0.1-x86_64-linux.tar.xz | tar -xJ -C /tmp; export PATH=/tmp/wasmtime-v50.0.1-x86_64-linux:$PATH
cargo test --release --test native_verified_floor_test --test wasm_runtime_cross_target 2>&1 | grep -E "test result|panicked" -A3 | head -20
cargo test --release -p almide-mir --lib 2>&1 | grep -E "test result|FAILED" | head
grep -n "flat_map_arr\|__arr" /tmp/cired-out-v1-combinator.rs | head -8 | cut -c1-200
for f in spec/wasm_cross/list_comprehensive.almd; do echo "=== $f"; almide run $f > /tmp/n.txt 2>&1; almide run $f --target wasm > /tmp/w.txt 2>&1; cmp /tmp/n.txt /tmp/w.txt && echo SAME; done
