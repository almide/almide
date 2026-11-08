set +e
echo "SHA $(git rev-parse --short HEAD)"
cat > /tmp/dq.almd <<'X'
import deque
fn main() -> Unit = {
  var d = deque.from_list([1, 2, 3])
  let a = deque.pop(d)
  let b = deque.pop_front(d)
  println("${a} ${b} ${deque.to_list(d)}")
}
X
almide run /tmp/dq.almd 2>&1 | grep -v "^\s*$" | head -40 | cut -c1-300
ALMIDE_BIN=$(command -v almide) bash proofs/check-wasm-reachability.sh 2>&1 | tail -8
