set +e
bash .github/cired/probe.sh $(command -v almide)
T=$(mktemp -d)
for n in p1_pop_arm p6_sibling; do cp .github/cired/progs/$n.almd $T/; (cd $T && ALMIDE_DUMP_MIR=1 almide build $n.almd -o $n.bin > /tmp/cired-out-mir-$n.txt 2>&1); done
cargo test --release --test zz_cired_render -- --nocapture 2>&1 | grep -v "^\s*Compiling" > /tmp/cired-out-render.txt; tail -3 /tmp/cired-out-render.txt
