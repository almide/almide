set +e
B=$(command -v almide)
bash .github/cired/probe.sh $B
T=$(mktemp -d)
for f in .github/cired/progs/*.almd; do
  n=$(basename $f .almd); mkdir -p $T/$n; cp $f $T/$n/m.almd
  touch /tmp/marker; sleep 1
  (cd $T/$n && almide build m.almd -o m_bin >/dev/null 2>&1)
  src=$(find $HOME $T /tmp -newer /tmp/marker \( -name main.rs -o -name almide_gen_main.rs \) 2>/dev/null | head -1)
  echo "=== GEN $n ($src)"
  [ -n "$src" ] && awk '/^\/\/ __ALMIDE_V1_RT_END__/{p=1} p' "$src" | head -400
done
cargo test --release --test mut_field_arg_sibling_read_test 2>&1 | tail -60
cargo test --release --test string_push_native_append_test 2>&1 | grep -v "^    " | tail -30
