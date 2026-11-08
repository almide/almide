set +e
cat > /tmp/d.almd <<'X'
fn main() -> Unit = {
  var d = deque.from_list([1, 2, 3])
  let x = deque.pop(d)
  let y = deque.pop_front(d)
  println("${x} ${y}")
}
X
cat > /tmp/m.almd <<'X'
fn main() -> Unit = {
  let m = matrix.from_lists([[1.0, 2.0]])
  println("${matrix.to_lists(matrix.concat_rows([m, m]))}")
  println("${matrix.to_lists(matrix.zip_with(m, m, (a, b) => a + b))}")
}
X
for f in /tmp/d.almd /tmp/m.almd; do echo "=== $f"; almide run $f 2>&1 | head -60; echo "=== wasm"; almide run $f --target wasm 2>&1 | head -10; done
