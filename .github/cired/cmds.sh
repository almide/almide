set +e
bash .github/cired/probe.sh $(command -v almide)
T=$(mktemp -d); cp .github/cired/progs/p5_unique.almd $T/; (cd $T && ALMIDE_DUMP_MIR=1 almide build p5_unique.almd -o p5.bin 2>&1 | grep -n "CallFn" | head -20)
cargo test --release --no-fail-fast --test list_unique_linear_test --test list_unique_wasm_linear_test --test mut_field_arg_sibling_read_test --test string_push_native_append_test 2>&1 | grep -E "^test |test result|panicked|^error" | head -40
