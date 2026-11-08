set +e
cat > /tmp/p0.almd <<'X'
fn parts(s: String) -> Int = list.len(list.chunk([1, 2, 3], 2)) + string.len(s)
effect fn main() -> Unit = println(int.to_string(fan.min(compute.ms(100)) { parts("a,b"), parts("c") } ?? 0))
X
echo "=== 0: list.len(list.chunk([1, 2, 3], 2))"; ALMIDE_VERBOSE=1 almide run /tmp/p0.almd 2>&1 | grep -E "^[0-9]|note|reason|error" | head -4 | cut -c1-300
cat > /tmp/p1.almd <<'X'
fn parts(s: String) -> Int = list.len(list.windows([1, 2, 3], 2)) + string.len(s)
effect fn main() -> Unit = println(int.to_string(fan.min(compute.ms(100)) { parts("a,b"), parts("c") } ?? 0))
X
echo "=== 1: list.len(list.windows([1, 2, 3], 2))"; ALMIDE_VERBOSE=1 almide run /tmp/p1.almd 2>&1 | grep -E "^[0-9]|note|reason|error" | head -4 | cut -c1-300
cat > /tmp/p2.almd <<'X'
fn parts(s: String) -> Int = string.len(string.pad_start(s, 5, " ")) + string.len(s)
effect fn main() -> Unit = println(int.to_string(fan.min(compute.ms(100)) { parts("a,b"), parts("c") } ?? 0))
X
echo "=== 2: string.len(string.pad_start(s, 5, " "))"; ALMIDE_VERBOSE=1 almide run /tmp/p2.almd 2>&1 | grep -E "^[0-9]|note|reason|error" | head -4 | cut -c1-300
cat > /tmp/p3.almd <<'X'
fn parts(s: String) -> Int = list.len(list.zip([1, 2], [3, 4])) + string.len(s)
effect fn main() -> Unit = println(int.to_string(fan.min(compute.ms(100)) { parts("a,b"), parts("c") } ?? 0))
X
echo "=== 3: list.len(list.zip([1, 2], [3, 4]))"; ALMIDE_VERBOSE=1 almide run /tmp/p3.almd 2>&1 | grep -E "^[0-9]|note|reason|error" | head -4 | cut -c1-300
cat > /tmp/p4.almd <<'X'
fn parts(s: String) -> Int = list.len(list.unique([1, 1, 2])) + string.len(s)
effect fn main() -> Unit = println(int.to_string(fan.min(compute.ms(100)) { parts("a,b"), parts("c") } ?? 0))
X
echo "=== 4: list.len(list.unique([1, 1, 2]))"; ALMIDE_VERBOSE=1 almide run /tmp/p4.almd 2>&1 | grep -E "^[0-9]|note|reason|error" | head -4 | cut -c1-300
cat > /tmp/p5.almd <<'X'
fn parts(s: String) -> Int = string.len(string.reverse(s)) + string.len(s)
effect fn main() -> Unit = println(int.to_string(fan.min(compute.ms(100)) { parts("a,b"), parts("c") } ?? 0))
X
echo "=== 5: string.len(string.reverse(s))"; ALMIDE_VERBOSE=1 almide run /tmp/p5.almd 2>&1 | grep -E "^[0-9]|note|reason|error" | head -4 | cut -c1-300
cat > /tmp/p6.almd <<'X'
fn parts(s: String) -> Int = list.product([1, 2]) + string.len(s)
effect fn main() -> Unit = println(int.to_string(fan.min(compute.ms(100)) { parts("a,b"), parts("c") } ?? 0))
X
echo "=== 6: list.product([1, 2])"; ALMIDE_VERBOSE=1 almide run /tmp/p6.almd 2>&1 | grep -E "^[0-9]|note|reason|error" | head -4 | cut -c1-300
cat > /tmp/p7.almd <<'X'
fn parts(s: String) -> Int = list.len(list.sort_by(["a", "bb", "c"], (x) => string.len(x))) + string.len(s)
effect fn main() -> Unit = println(int.to_string(fan.min(compute.ms(100)) { parts("a,b"), parts("c") } ?? 0))
X
echo "=== 7: list.len(list.sort_by(["a", "bb", "c"], (x) => string.len(x)))"; ALMIDE_VERBOSE=1 almide run /tmp/p7.almd 2>&1 | grep -E "^[0-9]|note|reason|error" | head -4 | cut -c1-300
cat > /tmp/p8.almd <<'X'
fn parts(s: String) -> Int = list.len(list.flatten([[1], [2]])) + string.len(s)
effect fn main() -> Unit = println(int.to_string(fan.min(compute.ms(100)) { parts("a,b"), parts("c") } ?? 0))
X
echo "=== 8: list.len(list.flatten([[1], [2]]))"; ALMIDE_VERBOSE=1 almide run /tmp/p8.almd 2>&1 | grep -E "^[0-9]|note|reason|error" | head -4 | cut -c1-300
cat > /tmp/p9.almd <<'X'
fn parts(s: String) -> Int = list.len(list.scan([1, 2], 0, (a, x) => a + x)) + string.len(s)
effect fn main() -> Unit = println(int.to_string(fan.min(compute.ms(100)) { parts("a,b"), parts("c") } ?? 0))
X
echo "=== 9: list.len(list.scan([1, 2], 0, (a, x) => a + x))"; ALMIDE_VERBOSE=1 almide run /tmp/p9.almd 2>&1 | grep -E "^[0-9]|note|reason|error" | head -4 | cut -c1-300
cat > /tmp/p10.almd <<'X'
fn parts(s: String) -> Int = list.len(list.dedup([1, 1, 2])) + string.len(s)
effect fn main() -> Unit = println(int.to_string(fan.min(compute.ms(100)) { parts("a,b"), parts("c") } ?? 0))
X
echo "=== 10: list.len(list.dedup([1, 1, 2]))"; ALMIDE_VERBOSE=1 almide run /tmp/p10.almd 2>&1 | grep -E "^[0-9]|note|reason|error" | head -4 | cut -c1-300
cat > /tmp/p11.almd <<'X'
fn parts(s: String) -> Int = list.len(string.chars(s)) + string.len(s)
effect fn main() -> Unit = println(int.to_string(fan.min(compute.ms(100)) { parts("a,b"), parts("c") } ?? 0))
X
echo "=== 11: list.len(string.chars(s))"; ALMIDE_VERBOSE=1 almide run /tmp/p11.almd 2>&1 | grep -E "^[0-9]|note|reason|error" | head -4 | cut -c1-300
cat > /tmp/p12.almd <<'X'
fn parts(s: String) -> Int = string.len(float.to_fixed(1.5, 2)) + string.len(s)
effect fn main() -> Unit = println(int.to_string(fan.min(compute.ms(100)) { parts("a,b"), parts("c") } ?? 0))
X
echo "=== 12: string.len(float.to_fixed(1.5, 2))"; ALMIDE_VERBOSE=1 almide run /tmp/p12.almd 2>&1 | grep -E "^[0-9]|note|reason|error" | head -4 | cut -c1-300
cat > /tmp/p13.almd <<'X'
fn parts(s: String) -> Int = list.len(list.take_while([1, 2, 3], (x) => x < 2)) + string.len(s)
effect fn main() -> Unit = println(int.to_string(fan.min(compute.ms(100)) { parts("a,b"), parts("c") } ?? 0))
X
echo "=== 13: list.len(list.take_while([1, 2, 3], (x) => x < 2))"; ALMIDE_VERBOSE=1 almide run /tmp/p13.almd 2>&1 | grep -E "^[0-9]|note|reason|error" | head -4 | cut -c1-300
cat > /tmp/p14.almd <<'X'
fn parts(s: String) -> Int = list.len(list.sort([3, 1, 2])) + string.len(s)
effect fn main() -> Unit = println(int.to_string(fan.min(compute.ms(100)) { parts("a,b"), parts("c") } ?? 0))
X
echo "=== 14: list.len(list.sort([3, 1, 2]))"; ALMIDE_VERBOSE=1 almide run /tmp/p14.almd 2>&1 | grep -E "^[0-9]|note|reason|error" | head -4 | cut -c1-300
cat > /tmp/p15.almd <<'X'
fn parts(s: String) -> Int = list.max([3, 1, 2]) ?? 0 + string.len(s)
effect fn main() -> Unit = println(int.to_string(fan.min(compute.ms(100)) { parts("a,b"), parts("c") } ?? 0))
X
echo "=== 15: list.max([3, 1, 2]) ?? 0"; ALMIDE_VERBOSE=1 almide run /tmp/p15.almd 2>&1 | grep -E "^[0-9]|note|reason|error" | head -4 | cut -c1-300
cat > /tmp/p16.almd <<'X'
fn parts(s: String) -> Int = list.len(list.reverse([1, 2])) + string.len(s)
effect fn main() -> Unit = println(int.to_string(fan.min(compute.ms(100)) { parts("a,b"), parts("c") } ?? 0))
X
echo "=== 16: list.len(list.reverse([1, 2]))"; ALMIDE_VERBOSE=1 almide run /tmp/p16.almd 2>&1 | grep -E "^[0-9]|note|reason|error" | head -4 | cut -c1-300
cat > /tmp/p17.almd <<'X'
fn parts(s: String) -> Int = string.len(string.repeat(s, 2)) + string.len(s)
effect fn main() -> Unit = println(int.to_string(fan.min(compute.ms(100)) { parts("a,b"), parts("c") } ?? 0))
X
echo "=== 17: string.len(string.repeat(s, 2))"; ALMIDE_VERBOSE=1 almide run /tmp/p17.almd 2>&1 | grep -E "^[0-9]|note|reason|error" | head -4 | cut -c1-300
cat > /tmp/p18.almd <<'X'
fn parts(s: String) -> Int = list.len(list.filter_map([1, 2], (x) => some(x))) + string.len(s)
effect fn main() -> Unit = println(int.to_string(fan.min(compute.ms(100)) { parts("a,b"), parts("c") } ?? 0))
X
echo "=== 18: list.len(list.filter_map([1, 2], (x) => some(x)))"; ALMIDE_VERBOSE=1 almide run /tmp/p18.almd 2>&1 | grep -E "^[0-9]|note|reason|error" | head -4 | cut -c1-300
cat > /tmp/p19.almd <<'X'
fn parts(s: String) -> Int = list.len(list.intersperse([1, 2], 0)) + string.len(s)
effect fn main() -> Unit = println(int.to_string(fan.min(compute.ms(100)) { parts("a,b"), parts("c") } ?? 0))
X
echo "=== 19: list.len(list.intersperse([1, 2], 0))"; ALMIDE_VERBOSE=1 almide run /tmp/p19.almd 2>&1 | grep -E "^[0-9]|note|reason|error" | head -4 | cut -c1-300
cat > /tmp/p20.almd <<'X'
fn parts(s: String) -> Int = string.len(string.replace(s, "a", "b")) + string.len(s)
effect fn main() -> Unit = println(int.to_string(fan.min(compute.ms(100)) { parts("a,b"), parts("c") } ?? 0))
X
echo "=== 20: string.len(string.replace(s, "a", "b"))"; ALMIDE_VERBOSE=1 almide run /tmp/p20.almd 2>&1 | grep -E "^[0-9]|note|reason|error" | head -4 | cut -c1-300
cat > /tmp/p21.almd <<'X'
fn parts(s: String) -> Int = list.len(string.lines(s)) + string.len(s)
effect fn main() -> Unit = println(int.to_string(fan.min(compute.ms(100)) { parts("a,b"), parts("c") } ?? 0))
X
echo "=== 21: list.len(string.lines(s))"; ALMIDE_VERBOSE=1 almide run /tmp/p21.almd 2>&1 | grep -E "^[0-9]|note|reason|error" | head -4 | cut -c1-300
cat > /tmp/p22.almd <<'X'
fn parts(s: String) -> Int = list.sum([0]) + string.len(s)
effect fn main() -> Unit = println(int.to_string(fan.min(compute.ms(100)) { parts("a,b"), parts("c") } ?? 0))
X
echo "=== 22: list.sum([0])"; ALMIDE_VERBOSE=1 almide run /tmp/p22.almd 2>&1 | grep -E "^[0-9]|note|reason|error" | head -4 | cut -c1-300
