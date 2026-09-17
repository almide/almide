// Host-side checks for marshal_types.almd: every marshalled type crosses in
// both directions, a String larger than a wasm page forces memory growth
// (the views are rebuilt after `memory.grow`), and an Int outside ±2^53 is
// refused by the wrapper rather than truncated.
import assert from "node:assert/strict";

export async function after(m) {
  assert.equal(m.add(2, 3), 5);
  assert.equal(m.add(-7, 2), -5);
  assert.equal(m.half(5), 2.5);
  assert.equal(m.both(true, false), false);
  assert.equal(m.both(true, true), true);
  assert.equal(m.shout("hey"), "HEY!");
  assert.equal(m.shout("héllo wörld"), "HÉLLO WÖRLD!");
  assert.equal(m.width("héllo"), 5);
  assert.equal(m.repeat_dash(3), "---");
  // 200 KiB in, 200 KiB out: three pages of growth on the way.
  const big = "abc".repeat(70000);
  assert.equal(m.shout(big), big.toUpperCase() + "!");
  assert.equal(m.repeat_dash(200000).length, 200000);
  // A thousand round trips through the module's own allocator and release.
  for (let i = 0; i < 1000; i++) assert.equal(m.shout("x" + i), "X" + i + "!");
  assert.throws(() => m.add(2 ** 53, 1), RangeError);
  assert.throws(() => m.add(1.5, 1), RangeError);
  m.ping();
}
