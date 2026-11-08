set +e
sudo apt-get install -y -qq valgrind >/dev/null 2>&1
D=research/benchmark/perf/listbuild
for v in append combinator; do
  cargo run -q --release -p almide-mir --example probe_native $D/listbuild_$v.almd > /tmp/cired-out-v1-$v.rs 2>&1
  almide $D/listbuild_$v.almd --target rust > /tmp/cired-out-v0-$v.rs 2>&1
  almide build $D/listbuild_$v.almd -o /tmp/lb_$v >/dev/null 2>&1
  echo "=== $v"; /tmp/lb_$v 12
  valgrind --tool=callgrind --callgrind-out-file=/tmp/cg_$v.out /tmp/lb_$v 20 2>&1 | grep -E "Collected|refs"
  callgrind_annotate /tmp/cg_$v.out 2>/dev/null | head -40 > /tmp/cired-out-cg-$v.txt
  head -30 /tmp/cired-out-cg-$v.txt | cut -c1-200
done
