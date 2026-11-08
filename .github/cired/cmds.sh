set +e
curl -sSfL https://github.com/bytecodealliance/wasmtime/releases/download/v50.0.1/wasmtime-v50.0.1-x86_64-linux.tar.xz | tar -xJ -C /tmp; export PATH=/tmp/wasmtime-v50.0.1-x86_64-linux:$PATH
cat > /tmp/mx.almd <<'X'
fn main() -> Unit = {
  let m = matrix.from_lists([[1.0, 2.0]])
  println("${matrix.to_lists(matrix.concat_rows([m, m]))}")
  println("${matrix.to_lists(matrix.zip_with(m, m, (a, b) => a + b))}")
}
X
echo "=== matrix native"; almide run /tmp/mx.almd 2>&1 | grep -v "^\s*$" | head -20 | cut -c1-300
echo "=== matrix wasm"; almide run /tmp/mx.almd --target wasm 2>&1 | head -5
echo "=== reachability"; ALMIDE_BIN=$(command -v almide) bash proofs/check-wasm-reachability.sh 2>&1 | tail -15
echo "=== listbuild"; almide run research/benchmark/perf/listbuild/listbuild_append.almd -- 12; almide run research/benchmark/perf/listbuild/listbuild_append.almd --target wasm -- 12
echo "=== suite"
cargo test --release --no-fail-fast --test fan_region_floor_test --test fan_meter_splice_test --test runtime_spelling_hygiene_test --test counted_index_chain_test --test ord_record_variant_test --test ord_recursive_test --test runtime_top_level_names_test --test native_verified_floor_test 2>&1 | grep -E "^test .*(ok|FAILED)$|panicked|left:|right:|test result" -A2 | cut -c1-400 | head -150
cargo test --release -p almide-ir --test fn_clone_discipline 2>&1 | grep "test result"
cargo test --release -p almide-mir --lib 2>&1 | grep -E "FAILED|test result|panicked" | head
