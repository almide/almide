set +e
cargo test --release --test zz_cired_render -- --nocapture 2>&1 | grep -v "^\s*Compiling" > /tmp/cired-out-render.txt; tail -3 /tmp/cired-out-render.txt
cargo test --release --test zz_cired_sibling -- --nocapture > /tmp/cired-out-sibling.txt 2>&1; grep -c MISMATCH /tmp/cired-out-sibling.txt
