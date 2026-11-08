set +e
ALMIDE_BIN=$(command -v almide) python3 tools/wasm_reachability_sweep.py --modules deque matrix --json /tmp/sw.json > /tmp/sw.txt 2>&1
python3 -c "
import json
for r in json.load(open('/tmp/sw.json')):
    if r['verdict'] in ('GAP','UNPROBEABLE'): print(json.dumps(r)[:1500])
"
cat > /tmp/d.almd <<'X'
fn main() -> Unit = {
  var d = deque.from_list([1, 2, 3])
  let x = deque.pop(d)
  let y = deque.pop_front(d)
  println("${x} ${y}")
  let m = matrix.from_lists([[1.0, 2.0]])
  println("${matrix.concat_rows(m, m)}")
  println("${matrix.zip_with(m, m, (a, b) => a + b)}")
}
X
almide check /tmp/d.almd; almide run /tmp/d.almd 2>&1 | head -60
grep -n "fn pop\b\|fn pop_front\|fn pop(" stdlib/deque*.almd | head
grep -n "fn concat_rows\|fn zip_with" stdlib/matrix*.almd | head
