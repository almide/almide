set +e
curl -sSfL https://github.com/bytecodealliance/wasmtime/releases/download/v50.0.1/wasmtime-v50.0.1-x86_64-linux.tar.xz | tar -xJ -C /tmp; export PATH=/tmp/wasmtime-v50.0.1-x86_64-linux:$PATH
for v in var lit; do
if [ $v = var ]; then body='let f = (x) => spin(x)
  list.sum(list.map(xs, f))'; else body='list.sum(list.map(xs, (x) => spin(x)))'; fi
cat > /tmp/m_$v.almd <<X
fn spin(n: Int) -> Int = {
  var i = 0
  while i < n {
    i = i + 1
  }
  i
}

fn mapped(xs: List[Int]) -> Int = {
  $body
}

effect fn units(ns: Int) -> Result[Int, String] =
  if ns > 6000 then ok(-1)
  else {
    let r = fan.bounded(compute.ns(ns)) { mapped([1, 2, 3]) } ?? -1
    if r >= 0 then ok(ns / 3) else units(ns + 3)!
  }

effect fn main() -> Unit = println("units: \${units(0)!}")
X
echo "=== $v"; almide run /tmp/m_$v.almd; almide run /tmp/m_$v.almd --target wasm
touch /tmp/marker; sleep 1
almide build /tmp/m_$v.almd -o /tmp/mb > /dev/null 2>&1
find "${TMPDIR:-/tmp}" /tmp -name 'main.rs' -newer /tmp/marker 2>/dev/null | head -3
f=$(find "${TMPDIR:-/tmp}" /tmp -name 'main.rs' -newer /tmp/marker 2>/dev/null | head -1)
echo "--- $f"; [ -n "$f" ] && cp "$f" /tmp/cired-out-m_$v.rs
done
for v in var lit; do
cargo run -q --release -p almide-mir --example probe_native /tmp/m_$v.almd > /tmp/cired-out-render-$v.rs 2>&1
cargo run -q --release -p almide-mir --example probe_native /tmp/m_$v.almd --mir > /tmp/cired-out-mir-$v.txt 2>&1
echo "=== render $v"; grep -n "^fn \|charge\|budget\|CallIndirect\|__lambda\|list_map\|list_sum" /tmp/cired-out-render-$v.rs | grep -v "^[0-9]*:\s*//" | head -80
done
