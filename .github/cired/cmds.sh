set +e
bash .github/cired/probe.sh $(command -v almide)
cargo test --release --no-fail-fast --test field_pop_in_loop_test --test for_in_clone_elision_test --test chain_source_borrow_vs_consuming_seed_test --test list_unique_linear_test --test list_unique_wasm_linear_test --test mut_field_arg_sibling_read_test --test string_push_native_append_test --test native_borrow_oracle_test --test native_v1_differential_test 2>&1 | grep -E "^test |test result|panicked|^error|allocations moved" | head -120
cargo test --release -p almide-mir 2>&1 | grep -E "test result: F|panicked|^error" | head -20
