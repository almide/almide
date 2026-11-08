set +e
curl -sSfL https://github.com/bytecodealliance/wasmtime/releases/download/v50.0.1/wasmtime-v50.0.1-x86_64-linux.tar.xz | tar -xJ -C /tmp; export PATH=/tmp/wasmtime-v50.0.1-x86_64-linux:$PATH
res=()
t() { local name="$1"; shift; timeout 2400 cargo test --release "$@" > /tmp/t.log 2>&1; local rc=$?; res+=("$name rc=$rc"); echo "=== $name rc=$rc"; grep -E "panicked|^test .* (ok|FAILED)$|left:|right:|error\[|error:" -A3 /tmp/t.log | head -60 | cut -c1-600; }
almide build research/benchmark/perf/listbuild/listbuild_append.almd --release -o /tmp/lb > /tmp/lb.log 2>&1; rc=$?; res+=("listbuild rc=$rc"); grep -m3 "error\[" -A6 /tmp/lb.log; /tmp/lb 12; /tmp/lb 12 | md5sum
almide run research/benchmark/perf/listbuild/listbuild_append.almd --target wasm -- 12
t ord_rv --test ord_record_variant_test
t ord_rec --test ord_recursive_test
t counted --test counted_index_chain_test
t meter_splice --test fan_meter_splice_test
t region_floor --test fan_region_floor_test
t spelling --test runtime_spelling_hygiene_test
t topnames --test runtime_top_level_names_test
t clone_disc -p almide-ir --test fn_clone_discipline
t str -p almide-str
t mir_lib -p almide-mir --lib
echo "SUMMARY ${res[*]}"
