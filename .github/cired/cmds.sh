set +e
for t in native_verified_floor_test string_accumulator_growth_test; do cargo test --release --test $t 2>&1 | grep -v "^\s*Compiling" | tail -80; done
