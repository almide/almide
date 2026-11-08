set +e
echo "SHA $(git rev-parse --short HEAD)"
res=()
t() { local name="$1"; shift; timeout 2400 cargo test --release --test "$@" > /tmp/t.log 2>&1; local rc=$?; res+=("$name rc=$rc"); echo "=== $name rc=$rc"; grep -E "panicked|^test .* (ok|FAILED)$|left:|right:|error\[" /tmp/t.log | head -20 | cut -c1-400; }
almide build research/benchmark/perf/listbuild/listbuild_append.almd --release -o /tmp/lb > /tmp/lb.log 2>&1; res+=("listbuild rc=$?"); grep -m3 "error\[" /tmp/lb.log
t ord_rv ord_record_variant_test
t ord_rec ord_recursive_test
t counted counted_index_chain_test a_row_the_v1_render
t meter_splice fan_meter_splice_test
t region_floor fan_region_floor_test closure_hops
t spelling runtime_spelling_hygiene_test a_real_metered
t topnames runtime_top_level_names_test
echo "SUMMARY $(git rev-parse --short HEAD): ${res[*]}"
