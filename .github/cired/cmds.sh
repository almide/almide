set +e
gen() { # $1 = almd file
  touch /tmp/marker; sleep 1
  almide build "$1" --release -o /tmp/gen-bin > /tmp/gen.log 2>&1; echo "build rc=$?"; grep -m5 "error\[" -A6 /tmp/gen.log
  f=$(find "${TMPDIR:-/tmp}" /tmp -name '*.rs' -newer /tmp/marker 2>/dev/null | xargs grep -l "fn almd_main\|fn main" 2>/dev/null | head -1)
  echo "--- generated: $f"; [ -n "$f" ] && cp "$f" "/tmp/cired-out-$(basename $1).rs"
}
echo "=== listbuild"; gen research/benchmark/perf/listbuild/listbuild_append.almd
cat > /tmp/ord.almd <<'X'
type Inner: Ord = { n: Int, s: String }
effect fn main() -> Unit = {
  let xs = [Inner { n: 2, s: "x" }, Inner { n: 1, s: "y" }, Inner { n: 1, s: "a" }]
  for i in xs |> list.sort {
    println("${i.n} ${i.s}")
  }
}
X
echo "=== ord"; gen /tmp/ord.almd
cp /tmp/ord.almd /tmp/cired-out-ord.almd
